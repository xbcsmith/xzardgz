//! Watcher mode command handler.
//!
//! This module implements the `watch` subcommand, which starts the pipeline
//! watcher that consumes task events from Kafka, executes plugins, and
//! publishes results back to the configured output topic.

use crate::cli::WatchArgs;
use crate::config::{Config, ConfigOverrides};
use crate::error::Result;

/// Executes the watcher mode command.
///
/// Loads global configuration, applies CLI overrides for provider, model,
/// workspace, and prints startup information. In dry-run or once-mode the
/// appropriate notice is printed before the stub message.
///
/// Full watcher execution (Kafka consumer loop, task dispatch, result
/// publishing) is implemented in a later phase.
///
/// # Arguments
///
/// * `args` - Parsed CLI arguments from the `watch` subcommand.
///
/// # Errors
///
/// Returns [`crate::error::PipelineError::Config`] if configuration loading
/// fails.
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

    if args.dry_run {
        println!("Dry run mode: watcher will not process messages.");
    }

    if args.once {
        println!("Once mode: watcher will process one task then exit.");
    }

    if let Some(ref provider) = args.provider {
        println!("Provider: {}", provider);
    }

    if let Some(ref workspace) = args.workspace {
        println!("Workspace: {}", workspace);
    }

    if let Some(ref brokers) = args.brokers {
        println!("Kafka brokers: {}", brokers);
    }

    if let Some(ref input_topic) = args.input_topic {
        println!("Input topic: {}", input_topic);
    }

    if let Some(ref output_topic) = args.output_topic {
        println!("Output topic: {}", output_topic);
    }

    if let Some(max_concurrent) = args.max_concurrent {
        println!("Max concurrent tasks: {}", max_concurrent);
    }

    println!("Watcher execution is implemented in a later phase.");
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

    #[tokio::test]
    async fn test_execute_default_returns_ok() {
        let args = make_default_watch_args();
        let result = execute(args).await;
        assert!(
            result.is_ok(),
            "watch default should succeed, got: {:?}",
            result.err()
        );
    }
}
