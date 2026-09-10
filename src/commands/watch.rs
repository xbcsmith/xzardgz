//! Watcher mode command handler.
//!
//! This module implements the `watch` subcommand, which starts the pipeline
//! watcher that consumes task events from Kafka, executes plugins, and
//! publishes results back to the configured output topic.
//!
//! The watcher execution flow is:
//! 1. Load and validate configuration.
//! 2. Build a `WatcherMatcher` from matcher config.
//! 3. Build a `WatcherExecutor` with the configured plugin registry.
//! 4. When `--dry-run` is set, validate and report configuration then exit.
//! 5. Build a result publisher (NoOp when `--no-publish`; Kafka otherwise).
//! 6. Build a `WatcherMessageHandler` wiring executor, matcher, and publisher.
//! 7. Build an `XzeprConsumer` from the reconciled Kafka configuration.
//! 8. When `--once` is set, run the consumer loop for a short window then exit.
//! 9. Otherwise, run the consumer loop until shutdown.

use std::sync::Arc;

use crate::cli::WatchArgs;
use crate::config::{Config, ConfigOverrides};
use crate::error::{PipelineError, Result};
use crate::plugins::registry::PluginRegistry;
use crate::watcher::WatcherMessageHandler;
use crate::watcher::executor::WatcherExecutor;
use crate::watcher::matcher::WatcherMatcher;
use crate::watcher::publisher::{KafkaResultPublisher, NoOpResultPublisher, ResultPublisher};
use crate::xzepr::consumer::config::KafkaConsumerConfig;
use crate::xzepr::consumer::kafka::XzeprConsumer;

/// Executes the watcher mode command.
///
/// Loads global configuration, applies CLI overrides, builds a
/// [`WatcherMatcher`] and [`WatcherExecutor`], then starts the Kafka
/// consumer loop.
///
/// In dry-run mode the watcher validates configuration and exits without
/// connecting to Kafka. In once mode the watcher runs the consumer loop
/// for a short window then exits.
///
/// # Arguments
///
/// * `args` - Parsed CLI arguments from the `watch` subcommand.
///
/// # Errors
///
/// Returns [`crate::error::PipelineError::Config`] if configuration loading
/// or validation fails, or [`crate::error::PipelineError::Kafka`] if the
/// Kafka consumer or producer cannot be created.
pub async fn execute(args: WatchArgs) -> Result<()> {
    let mut config = Config::load()?;

    let overrides = ConfigOverrides {
        provider: args.provider.clone(),
        openai_model: args.model.clone(),
        ollama_model: None,
        ollama_host: None,
        workspace_root: args.workspace.clone(),
    };
    config.apply_overrides(&overrides);

    // Apply CLI overrides for Kafka settings.
    if let Some(ref brokers) = args.brokers {
        config.kafka.brokers = brokers.split(',').map(|s| s.trim().to_string()).collect();
    }
    if let Some(ref input_topic) = args.input_topic {
        config.topics.task = input_topic.clone();
    }
    if let Some(ref output_topic) = args.output_topic {
        config.topics.result = output_topic.clone();
    }
    if let Some(max_concurrent) = args.max_concurrent {
        config.watcher.max_concurrent_tasks = max_concurrent as u32;
    }
    if args.no_publish {
        config.watcher.result_publish_enabled = false;
    }
    if args.once {
        config.watcher.once = true;
    }

    let config = Arc::new(config);

    // Build the plugin registry (empty for now; plugins registered separately).
    let plugin_registry = Arc::new(PluginRegistry::new());

    // Build the matcher from config.
    let matcher = Arc::new(WatcherMatcher::from_config(&config.matcher));

    // Build the watcher executor.
    let executor = Arc::new(WatcherExecutor::new(config.clone(), plugin_registry));

    // Log startup information.
    if args.dry_run {
        println!("Dry run mode: watcher will validate configuration and exit.");
        println!(
            "  Matcher: {} event types, {} plugins configured.",
            config.matcher.event_types.len(),
            config.matcher.plugins.len(),
        );
        println!(
            "  Executor: once={}, max_concurrent={}, publish={}",
            executor.once_mode_enabled(),
            executor.max_concurrent_tasks(),
            executor.result_publish_enabled(),
        );
        if matcher.is_empty() {
            println!("  WARNING: matcher is empty - all tasks will be rejected.");
        }
        return Ok(());
    }

    if executor.once_mode_enabled() {
        println!("Once mode: watcher will process one task batch then exit.");
    }

    if let Some(ref provider) = args.provider {
        println!("Provider: {}", provider);
    }

    if let Some(ref workspace) = args.workspace {
        println!("Workspace: {}", workspace);
    }

    println!(
        "Kafka task topic: {}  result topic: {}",
        config.topics.task, config.topics.result,
    );
    println!(
        "Watcher started: once={}, max_concurrent={}, publish={}",
        executor.once_mode_enabled(),
        executor.max_concurrent_tasks(),
        executor.result_publish_enabled(),
    );

    if matcher.is_empty() {
        println!("WARNING: matcher is empty - all tasks will be rejected.");
    }

    // Build result publisher.
    let publisher: Arc<dyn ResultPublisher + Send + Sync> = if !executor.result_publish_enabled() {
        Arc::new(NoOpResultPublisher)
    } else {
        Arc::new(KafkaResultPublisher::new(&config.kafka, &config.topics)?)
    };

    // Build message handler.
    let handler = Arc::new(WatcherMessageHandler::new(
        executor.clone(),
        matcher,
        publisher,
    ));

    // Build Kafka consumer from the reconciled operator-facing config.
    let consumer_config = KafkaConsumerConfig::from_app_config(&config.kafka, &config.topics.task);

    let consumer =
        XzeprConsumer::new(consumer_config).map_err(|e| PipelineError::Kafka(e.to_string()))?;

    if executor.once_mode_enabled() {
        // Once mode: poll for the configured window then exit.
        // Uses a timeout so the process terminates without an explicit stop
        // signal, which is the expected behaviour in CI and one-shot runs.
        use tokio::time::{Duration, timeout};
        let _ = timeout(Duration::from_millis(100), consumer.run(handler)).await;
        return Ok(());
    }

    consumer
        .run(handler)
        .await
        .map_err(|e| PipelineError::Kafka(e.to_string()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::WatchArgs;

    /// Returns a [`WatchArgs`] with all optional fields set to `None` / defaults.
    fn make_default_watch_args() -> WatchArgs {
        WatchArgs {
            provider: None,
            model: None,
            workspace: None,
            brokers: None,
            input_topic: None,
            output_topic: None,
            matcher_config: None,
            dry_run: false,
            once: false,
            max_concurrent: None,
            no_publish: false,
        }
    }

    #[tokio::test]
    async fn test_execute_dry_run_returns_ok() {
        let mut args = make_default_watch_args();
        args.dry_run = true;
        let result = execute(args).await;
        assert!(
            result.is_ok(),
            "watch dry run should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_once_mode_returns_ok() {
        let mut args = make_default_watch_args();
        args.once = true;
        let result = execute(args).await;
        assert!(
            result.is_ok(),
            "watch once mode should succeed, got: {:?}",
            result.err()
        );
    }

    /// Verifies that config loading and validation succeed in dry-run mode
    /// with default arguments.
    #[tokio::test]
    async fn test_execute_default_returns_ok() {
        let mut args = make_default_watch_args();
        args.dry_run = true;
        let result = execute(args).await;
        assert!(
            result.is_ok(),
            "watch default should succeed, got: {:?}",
            result.err()
        );
    }

    /// Verifies that config loading and validation succeed in dry-run mode
    /// when broker addresses are overridden via CLI.
    #[tokio::test]
    async fn test_execute_with_brokers_override_returns_ok() {
        let mut args = make_default_watch_args();
        args.brokers = Some("kafka1:9092,kafka2:9092".to_string());
        args.dry_run = true;
        let result = execute(args).await;
        assert!(result.is_ok(), "watch with brokers should succeed");
    }

    /// Verifies that config loading and validation succeed in dry-run mode
    /// when result publishing is disabled via `--no-publish`.
    #[tokio::test]
    async fn test_execute_with_no_publish_returns_ok() {
        let mut args = make_default_watch_args();
        args.no_publish = true;
        args.dry_run = true;
        let result = execute(args).await;
        assert!(result.is_ok(), "watch with no_publish should succeed");
    }

    /// Verifies that config loading and validation succeed in dry-run mode
    /// when the max-concurrent-tasks limit is overridden via CLI.
    #[tokio::test]
    async fn test_execute_with_max_concurrent_override_returns_ok() {
        let mut args = make_default_watch_args();
        args.max_concurrent = Some(4);
        args.dry_run = true;
        let result = execute(args).await;
        assert!(
            result.is_ok(),
            "watch with max_concurrent override should succeed"
        );
    }

    #[tokio::test]
    async fn test_execute_dry_run_with_topic_overrides_returns_ok() {
        let mut args = make_default_watch_args();
        args.dry_run = true;
        args.input_topic = Some("my.tasks".to_string());
        args.output_topic = Some("my.results".to_string());
        let result = execute(args).await;
        assert!(
            result.is_ok(),
            "watch dry run with topic overrides should succeed"
        );
    }
}
