//! CloudEventMessage-to-WatcherTaskMessage adapter and WatcherMessageHandler.
//!
//! Implements the Phase 2 integration boundary defined in
//! `docs/explanation/watcher_xzepr_phase1_integration_boundary.md`.
//!
//! The [`cloud_event_to_task`] function translates an inbound XZepr
//! [`CloudEventMessage`] into the internal [`WatcherTaskMessage`] DTO,
//! enforcing all Phase 1 rejection rules.  [`WatcherMessageHandler`] is a
//! concrete [`MessageHandler`] implementation that wires the adapter into
//! [`WatcherExecutor::process_task`].
//!
//! # Rejection rules
//!
//! A message is silently dropped (offset still committed) when:
//!
//! 1. `CloudEventMessage.event_type` is not in the allow-list or is a result
//!    variant (`is_task()` returns `false`).
//! 2. `data.events` is empty.
//! 3. `payload["correlation_id"]` is absent, `null`, or an empty string.
//! 4. `payload["repository"]` is absent, `null`, or an empty string.
//! 5. [`WatcherMatcher`] does not accept the translated task.

use std::sync::Arc;

use crate::watcher::event_type::WatcherEventType;
use crate::watcher::executor::WatcherExecutor;
use crate::watcher::matcher::WatcherMatcher;
use crate::watcher::publisher::ResultPublisher;
use crate::watcher::task::{WATCHER_TASK_VERSION, WatcherTaskMessage};
use crate::xzepr::consumer::kafka::MessageHandler;
use crate::xzepr::consumer::message::CloudEventMessage;

// ---------------------------------------------------------------------------
// AdapterError
// ---------------------------------------------------------------------------

/// Error returned when a [`CloudEventMessage`] cannot be mapped to a
/// [`WatcherTaskMessage`].
///
/// Every variant maps to one of the Phase 1 rejection rules.
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    /// `event_type` is not in the accept-list or is a result variant.
    #[error("event type '{0}' is not accepted for task dispatch")]
    UnacceptedEventType(String),

    /// `data.events` is empty; no
    /// [`EventEntity`][crate::xzepr::consumer::message::EventEntity] to read
    /// payload fields from.
    #[error("data.events is empty; no event entity to process")]
    EmptyEvents,

    /// `payload["correlation_id"]` is absent, null, or an empty string.
    #[error("required payload key 'correlation_id' is missing or empty")]
    MissingCorrelationId,

    /// `payload["repository"]` is absent, null, or an empty string.
    #[error("required payload key 'repository' is missing or empty")]
    MissingRepository,
}

// ---------------------------------------------------------------------------
// cloud_event_to_task
// ---------------------------------------------------------------------------

/// Translates a [`CloudEventMessage`] to a [`WatcherTaskMessage`] per the
/// Phase 1 field mapping.
///
/// All payload extraction is performed against `data.events[0]`.
///
/// # Arguments
///
/// * `msg` - Inbound XZepr CloudEvent to translate.
///
/// # Returns
///
/// A [`WatcherTaskMessage`] ready for dispatch via
/// [`WatcherExecutor::process_task`].
///
/// # Errors
///
/// - [`AdapterError::UnacceptedEventType`] — event type not in allow-list or
///   is a result variant.
/// - [`AdapterError::EmptyEvents`] — `data.events` is empty.
/// - [`AdapterError::MissingCorrelationId`] — `payload["correlation_id"]`
///   absent or empty.
/// - [`AdapterError::MissingRepository`] — `payload["repository"]` absent or
///   empty.
///
/// # Examples
///
/// ```
/// use xzardgz::watcher::adapter::cloud_event_to_task;
/// use xzardgz::xzepr::consumer::message::CloudEventMessage;
///
/// let json = serde_json::json!({
///     "success": true,
///     "id": "01TEST000000000000000000000",
///     "specversion": "1.0.1",
///     "type": "xzardgz.technical_review.task",
///     "source": "xzepr://ci",
///     "api_version": "v1",
///     "name": "test",
///     "version": "1.0.0",
///     "release": "1.0.0",
///     "platform_id": "github",
///     "package": "myapp",
///     "data": {
///         "events": [{
///             "id": "evt1",
///             "name": "name",
///             "version": "1.0.0",
///             "release": "1.0.0",
///             "platform_id": "github",
///             "package": "pkg",
///             "description": "desc",
///             "payload": {
///                 "correlation_id": "corr-001",
///                 "repository": "https://github.com/example/repo"
///             },
///             "success": true,
///             "event_receiver_id": "rcv1",
///             "created_at": "2024-01-01T00:00:00Z"
///         }],
///         "event_receivers": [],
///         "event_receiver_groups": []
///     }
/// });
/// let msg: CloudEventMessage = serde_json::from_value(json).unwrap();
/// let task = cloud_event_to_task(&msg).unwrap();
/// assert_eq!(task.correlation_id, "corr-001");
/// assert_eq!(task.repository, "https://github.com/example/repo");
/// assert_eq!(task.plugin, "technical-review");
/// ```
pub fn cloud_event_to_task(
    msg: &CloudEventMessage,
) -> std::result::Result<WatcherTaskMessage, AdapterError> {
    // Validate and map event type. from_event_str returns None for unknown strings.
    let event_type = WatcherEventType::from_event_str(&msg.event_type)
        .ok_or_else(|| AdapterError::UnacceptedEventType(msg.event_type.clone()))?;

    // Result variants are valid but not dispatchable as tasks.
    if !event_type.is_task() {
        return Err(AdapterError::UnacceptedEventType(msg.event_type.clone()));
    }

    // Plugin name is derived deterministically from the event type.
    let plugin = match &event_type {
        WatcherEventType::TechnicalReviewTask => "technical-review",
        WatcherEventType::SecurityReviewTask => "security-review",
        _ => {
            // Unreachable: is_task() only returns true for the two task variants.
            return Err(AdapterError::UnacceptedEventType(msg.event_type.clone()));
        }
    };

    // Extract data.events[0].
    let entity = msg.data.events.first().ok_or(AdapterError::EmptyEvents)?;

    // Required payload fields.
    let correlation_id = extract_required_str(&entity.payload, "correlation_id")
        .ok_or(AdapterError::MissingCorrelationId)?;
    let repository = extract_required_str(&entity.payload, "repository")
        .ok_or(AdapterError::MissingRepository)?;

    // Optional payload fields with Phase 1 documented defaults.
    let target_branch = extract_optional_str(&entity.payload, "target_branch");
    let provider = extract_optional_str(&entity.payload, "provider");
    let model = extract_optional_str(&entity.payload, "model");
    let dry_run = entity
        .payload
        .get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let workspace_directory = extract_optional_str(&entity.payload, "workspace_directory");
    let plugin_config = entity
        .payload
        .get("plugin_config")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let requested_report_formats = entity
        .payload
        .get("report_formats")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let reply_topic_override = extract_optional_str(&entity.payload, "reply_topic");
    let metadata = entity
        .payload
        .get("metadata")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    Ok(WatcherTaskMessage {
        version: WATCHER_TASK_VERSION.to_string(),
        id: ulid::Ulid::new().to_string(),
        spec_version: "1.0".to_string(),
        event_type,
        source: msg.source.clone(),
        repository,
        target_branch,
        provider,
        model,
        plugin: plugin.to_string(),
        plugin_config,
        dry_run,
        workspace_directory,
        metadata,
        requested_report_formats,
        correlation_id,
        reply_topic_override,
    })
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Extracts a non-empty string from a JSON payload by key.
///
/// Returns `None` when the key is absent, not a string, or an empty string.
fn extract_required_str(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Extracts an optional non-empty string from a JSON payload by key.
///
/// Returns `None` when the key is absent, not a string, or an empty string.
fn extract_optional_str(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// WatcherMessageHandler
// ---------------------------------------------------------------------------

/// Concrete [`MessageHandler`] that wires the Phase 2 adapter into the watcher
/// execution path.
///
/// For each inbound [`CloudEventMessage`], `WatcherMessageHandler`:
///
/// 1. Calls [`cloud_event_to_task`] to translate the envelope.
/// 2. Silently drops result-type events (not errors).
/// 3. Logs a warning and drops other malformed messages.
/// 4. Passes the task through the [`WatcherMatcher`] allow-list.
/// 5. Calls [`WatcherExecutor::process_task`] and the configured
///    [`ResultPublisher`].
///
/// The handler always returns `Ok(())` so the consumer loop continues even
/// when individual messages are rejected or tasks fail.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use xzardgz::config::Config;
/// use xzardgz::plugins::registry::PluginRegistry;
/// use xzardgz::watcher::adapter::WatcherMessageHandler;
/// use xzardgz::watcher::executor::WatcherExecutor;
/// use xzardgz::watcher::matcher::WatcherMatcher;
/// use xzardgz::watcher::publisher::NoOpResultPublisher;
///
/// let config = Arc::new(Config::default());
/// let registry = Arc::new(PluginRegistry::new());
/// let executor = Arc::new(WatcherExecutor::new(config.clone(), registry));
/// let matcher = Arc::new(WatcherMatcher::from_config(&config.matcher));
/// let publisher = Arc::new(NoOpResultPublisher);
/// let _handler = WatcherMessageHandler::new(executor, matcher, publisher);
/// ```
pub struct WatcherMessageHandler {
    executor: Arc<WatcherExecutor>,
    matcher: Arc<WatcherMatcher>,
    publisher: Arc<dyn ResultPublisher + Send + Sync>,
}

impl WatcherMessageHandler {
    /// Creates a new [`WatcherMessageHandler`].
    ///
    /// # Arguments
    ///
    /// * `executor` - Shared executor for task dispatch.
    /// * `matcher` - Shared allow-list filter.
    /// * `publisher` - Shared result publisher.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use xzardgz::config::Config;
    /// use xzardgz::plugins::registry::PluginRegistry;
    /// use xzardgz::watcher::adapter::WatcherMessageHandler;
    /// use xzardgz::watcher::executor::WatcherExecutor;
    /// use xzardgz::watcher::matcher::WatcherMatcher;
    /// use xzardgz::watcher::publisher::NoOpResultPublisher;
    ///
    /// let config = Arc::new(Config::default());
    /// let registry = Arc::new(PluginRegistry::new());
    /// let executor = Arc::new(WatcherExecutor::new(config.clone(), registry));
    /// let matcher = Arc::new(WatcherMatcher::from_config(&config.matcher));
    /// let publisher = Arc::new(NoOpResultPublisher);
    /// let handler = WatcherMessageHandler::new(executor, matcher, publisher);
    /// drop(handler);
    /// ```
    pub fn new(
        executor: Arc<WatcherExecutor>,
        matcher: Arc<WatcherMatcher>,
        publisher: Arc<dyn ResultPublisher + Send + Sync>,
    ) -> Self {
        Self {
            executor,
            matcher,
            publisher,
        }
    }
}

#[async_trait::async_trait]
impl MessageHandler for WatcherMessageHandler {
    /// Processes one inbound [`CloudEventMessage`].
    ///
    /// Always returns `Ok(())`. Rejection reasons and task errors are emitted
    /// as structured log events at `warn` or `debug` level.
    async fn handle(
        &self,
        message: CloudEventMessage,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let event_id = message.id.clone();
        let event_type_str = message.event_type.clone();

        // Translate CloudEventMessage -> WatcherTaskMessage.
        let task = match cloud_event_to_task(&message) {
            Ok(t) => t,
            Err(AdapterError::UnacceptedEventType(_)) => {
                // Result events are normal traffic; skip silently without warning.
                tracing::debug!(
                    event_id = %event_id,
                    event_type = %event_type_str,
                    "skipping non-task or unrecognised event type"
                );
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(
                    event_id = %event_id,
                    event_type = %event_type_str,
                    error = %e,
                    "rejecting CloudEventMessage"
                );
                return Ok(());
            }
        };

        // Apply WatcherMatcher allow-list.
        if !self.matcher.matches(&task) {
            tracing::debug!(
                event_id = %event_id,
                plugin = %task.plugin,
                repository = %task.repository,
                "message filtered by WatcherMatcher"
            );
            return Ok(());
        }

        // Delegate to WatcherExecutor.
        if let Err(e) = self
            .executor
            .process_task(task, self.publisher.as_ref())
            .await
        {
            tracing::warn!(
                event_id = %event_id,
                error = %e,
                "task execution failed"
            );
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use async_trait::async_trait;
    use tokio::sync::Mutex;

    use crate::config::{Config, MatcherConfig};
    use crate::error::Result;
    use crate::plugins::context::{PluginContext, ToolAccessLevel};
    use crate::plugins::output::PluginOutput;
    use crate::plugins::registry::PluginRegistry;
    use crate::plugins::trait_def::{PluginMetadata, WorkflowPlugin};
    use crate::watcher::executor::WatcherExecutor;
    use crate::watcher::matcher::WatcherMatcher;
    use crate::watcher::publisher::NoOpResultPublisher;
    use crate::watcher::result::WatcherResultMessage;

    // -----------------------------------------------------------------------
    // MockResultPublisher
    // -----------------------------------------------------------------------

    struct MockResultPublisher {
        published: Arc<Mutex<Vec<WatcherResultMessage>>>,
    }

    #[async_trait]
    impl ResultPublisher for MockResultPublisher {
        async fn publish(&self, result: &WatcherResultMessage) -> crate::error::Result<()> {
            self.published.lock().await.push(result.clone());
            Ok(())
        }
    }

    // -----------------------------------------------------------------------
    // TestPlugin
    // -----------------------------------------------------------------------

    /// Minimal [`WorkflowPlugin`] registered as `"technical-review"` for
    /// integration tests.
    struct TestPlugin;

    #[async_trait]
    impl WorkflowPlugin for TestPlugin {
        fn name(&self) -> &str {
            "technical-review"
        }

        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new("technical-review", "0.1.0", "Test plugin for adapter tests")
        }

        fn supported_formats(&self) -> Vec<String> {
            vec!["json".to_string()]
        }

        fn required_tool_access(&self) -> ToolAccessLevel {
            ToolAccessLevel::None
        }

        async fn run(&self, _ctx: PluginContext) -> Result<PluginOutput> {
            Ok(PluginOutput::success("ok"))
        }
    }

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_cloud_event(event_type: &str, payload: serde_json::Value) -> CloudEventMessage {
        serde_json::from_value(serde_json::json!({
            "success": true,
            "id": "01TEST000000000000000000001",
            "specversion": "1.0.1",
            "type": event_type,
            "source": "xzepr://ci",
            "api_version": "v1",
            "name": "test",
            "version": "1.0.0",
            "release": "1.0.0",
            "platform_id": "github",
            "package": "myapp",
            "data": {
                "events": [{
                    "id": "evt1",
                    "name": "name",
                    "version": "1.0.0",
                    "release": "1.0.0",
                    "platform_id": "github",
                    "package": "pkg",
                    "description": "desc",
                    "payload": payload,
                    "success": true,
                    "event_receiver_id": "rcv1",
                    "created_at": "2024-01-01T00:00:00Z"
                }],
                "event_receivers": [],
                "event_receiver_groups": []
            }
        }))
        // SAFETY: test fixture is well-formed JSON matching the CloudEventMessage schema.
        .unwrap()
    }

    fn make_empty_events_cloud_event() -> CloudEventMessage {
        serde_json::from_value(serde_json::json!({
            "success": true,
            "id": "01TEST000000000000000000002",
            "specversion": "1.0.1",
            "type": "xzardgz.technical_review.task",
            "source": "xzepr://ci",
            "api_version": "v1",
            "name": "test",
            "version": "1.0.0",
            "release": "1.0.0",
            "platform_id": "github",
            "package": "myapp",
            "data": {
                "events": [],
                "event_receivers": [],
                "event_receiver_groups": []
            }
        }))
        // SAFETY: test fixture is well-formed JSON.
        .unwrap()
    }

    /// Builds a minimal handler with an empty matcher and a no-op publisher.
    ///
    /// Suitable for tests that exercise message-rejection logic without
    /// reaching the executor.
    fn make_simple_handler() -> WatcherMessageHandler {
        let config = Arc::new(Config::default());
        let registry = Arc::new(PluginRegistry::new());
        let executor = Arc::new(WatcherExecutor::new(config, Arc::clone(&registry)));
        let mut matcher_cfg = MatcherConfig::default();
        matcher_cfg.event_types.clear();
        matcher_cfg.plugins.clear();
        let matcher = Arc::new(WatcherMatcher::from_config(&matcher_cfg));
        let publisher = Arc::new(NoOpResultPublisher);
        WatcherMessageHandler::new(executor, matcher, publisher)
    }

    // -----------------------------------------------------------------------
    // cloud_event_to_task tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_cloud_event_to_task_with_valid_technical_review_returns_task() {
        let msg = make_cloud_event(
            "xzardgz.technical_review.task",
            serde_json::json!({
                "correlation_id": "corr-001",
                "repository": "https://github.com/example/repo"
            }),
        );
        let task = cloud_event_to_task(&msg).unwrap();
        assert_eq!(task.plugin, "technical-review");
        assert_eq!(task.correlation_id, "corr-001");
        assert_eq!(task.repository, "https://github.com/example/repo");
    }

    #[test]
    fn test_cloud_event_to_task_with_valid_security_review_returns_task() {
        let msg = make_cloud_event(
            "xzardgz.security_review.task",
            serde_json::json!({
                "correlation_id": "corr-002",
                "repository": "https://github.com/example/repo"
            }),
        );
        let task = cloud_event_to_task(&msg).unwrap();
        assert_eq!(task.plugin, "security-review");
        assert_eq!(task.correlation_id, "corr-002");
        assert_eq!(task.repository, "https://github.com/example/repo");
    }

    #[test]
    fn test_cloud_event_to_task_with_result_event_returns_unaccepted_error() {
        let msg = make_cloud_event(
            "xzardgz.technical_review.result",
            serde_json::json!({
                "correlation_id": "corr-001",
                "repository": "https://github.com/example/repo"
            }),
        );
        let err = cloud_event_to_task(&msg).unwrap_err();
        assert!(
            matches!(err, AdapterError::UnacceptedEventType(_)),
            "expected UnacceptedEventType, got: {err}",
        );
    }

    #[test]
    fn test_cloud_event_to_task_with_unknown_event_type_returns_unaccepted_error() {
        let msg = make_cloud_event(
            "unknown.event",
            serde_json::json!({
                "correlation_id": "corr-001",
                "repository": "https://github.com/example/repo"
            }),
        );
        let err = cloud_event_to_task(&msg).unwrap_err();
        assert!(
            matches!(err, AdapterError::UnacceptedEventType(_)),
            "expected UnacceptedEventType, got: {err}",
        );
    }

    #[test]
    fn test_cloud_event_to_task_with_empty_events_returns_empty_events_error() {
        let msg = make_empty_events_cloud_event();
        let err = cloud_event_to_task(&msg).unwrap_err();
        assert!(
            matches!(err, AdapterError::EmptyEvents),
            "expected EmptyEvents, got: {err}",
        );
    }

    #[test]
    fn test_cloud_event_to_task_missing_correlation_id_returns_error() {
        let msg = make_cloud_event(
            "xzardgz.technical_review.task",
            serde_json::json!({
                "repository": "https://github.com/example/repo"
            }),
        );
        let err = cloud_event_to_task(&msg).unwrap_err();
        assert!(
            matches!(err, AdapterError::MissingCorrelationId),
            "expected MissingCorrelationId, got: {err}",
        );
    }

    #[test]
    fn test_cloud_event_to_task_empty_correlation_id_returns_error() {
        let msg = make_cloud_event(
            "xzardgz.technical_review.task",
            serde_json::json!({
                "correlation_id": "",
                "repository": "https://github.com/example/repo"
            }),
        );
        let err = cloud_event_to_task(&msg).unwrap_err();
        assert!(
            matches!(err, AdapterError::MissingCorrelationId),
            "expected MissingCorrelationId, got: {err}",
        );
    }

    #[test]
    fn test_cloud_event_to_task_missing_repository_returns_error() {
        let msg = make_cloud_event(
            "xzardgz.technical_review.task",
            serde_json::json!({
                "correlation_id": "corr-001"
            }),
        );
        let err = cloud_event_to_task(&msg).unwrap_err();
        assert!(
            matches!(err, AdapterError::MissingRepository),
            "expected MissingRepository, got: {err}",
        );
    }

    #[test]
    fn test_cloud_event_to_task_with_full_optional_fields_maps_all_fields() {
        let msg = make_cloud_event(
            "xzardgz.technical_review.task",
            serde_json::json!({
                "correlation_id": "corr-full",
                "repository": "https://github.com/example/repo",
                "target_branch": "main",
                "provider": "openai",
                "model": "gpt-4o",
                "dry_run": true,
                "workspace_directory": "/tmp/ws",
                "plugin_config": {"key": "value"},
                "report_formats": ["markdown", "json"],
                "reply_topic": "results-topic",
                "metadata": {"env": "staging"}
            }),
        );
        let task = cloud_event_to_task(&msg).unwrap();
        assert_eq!(task.target_branch, Some("main".to_string()));
        assert_eq!(task.provider, Some("openai".to_string()));
        assert_eq!(task.model, Some("gpt-4o".to_string()));
        assert!(task.dry_run);
        assert_eq!(task.workspace_directory, Some("/tmp/ws".to_string()));
        assert_eq!(task.plugin_config, serde_json::json!({"key": "value"}),);
        assert_eq!(
            task.requested_report_formats,
            vec!["markdown".to_string(), "json".to_string()],
        );
        assert_eq!(task.reply_topic_override, Some("results-topic".to_string()));
        assert_eq!(task.metadata.get("env"), Some(&"staging".to_string()),);
    }

    #[test]
    fn test_cloud_event_to_task_source_is_taken_from_envelope() {
        let msg = make_cloud_event(
            "xzardgz.technical_review.task",
            serde_json::json!({
                "correlation_id": "corr-001",
                "repository": "https://github.com/example/repo"
            }),
        );
        let task = cloud_event_to_task(&msg).unwrap();
        assert_eq!(task.source, "xzepr://ci");
    }

    #[test]
    fn test_cloud_event_to_task_plugin_not_taken_from_payload() {
        let msg = make_cloud_event(
            "xzardgz.technical_review.task",
            serde_json::json!({
                "correlation_id": "corr-001",
                "repository": "https://github.com/example/repo",
                "plugin": "custom-plugin"
            }),
        );
        let task = cloud_event_to_task(&msg).unwrap();
        // plugin must be derived from event_type, never from payload.
        assert_eq!(task.plugin, "technical-review");
    }

    #[test]
    fn test_cloud_event_to_task_dry_run_defaults_to_false() {
        let msg = make_cloud_event(
            "xzardgz.technical_review.task",
            serde_json::json!({
                "correlation_id": "corr-001",
                "repository": "https://github.com/example/repo"
            }),
        );
        let task = cloud_event_to_task(&msg).unwrap();
        assert!(!task.dry_run);
    }

    // -----------------------------------------------------------------------
    // WatcherMessageHandler integration test
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_watcher_message_handler_handle_with_valid_message_dispatches_and_publishes_correlation_id()
     {
        let repo_dir = tempfile::TempDir::new()
            // SAFETY: system temp dir is always writable in test environments.
            .unwrap();
        let ws_dir = tempfile::TempDir::new()
            // SAFETY: system temp dir is always writable in test environments.
            .unwrap();

        let mut config = Config::default();
        config.watcher.result_publish_enabled = true;
        config.workspace.root = ws_dir
            .path()
            .to_str()
            // SAFETY: temp dir path is always valid UTF-8 on supported platforms.
            .unwrap()
            .to_string();
        config.governance.enabled = false;
        config.governance.rules_path = String::new();
        config.reports.formats = vec!["json".to_string()];

        let mut registry = PluginRegistry::new();
        registry.register(Arc::new(TestPlugin));

        let config = Arc::new(config);
        let executor = Arc::new(WatcherExecutor::new(config.clone(), Arc::new(registry)));

        // Empty matcher accepts all tasks.
        let mut matcher_config = MatcherConfig::default();
        matcher_config.event_types.clear();
        matcher_config.plugins.clear();
        let matcher = Arc::new(WatcherMatcher::from_config(&matcher_config));

        let published: Arc<Mutex<Vec<WatcherResultMessage>>> = Arc::new(Mutex::new(Vec::new()));
        let publisher = Arc::new(MockResultPublisher {
            published: Arc::clone(&published),
        });

        let handler = WatcherMessageHandler::new(executor, matcher, publisher);

        let repo_path = repo_dir
            .path()
            .to_str()
            // SAFETY: temp dir path is always valid UTF-8 on supported platforms.
            .unwrap();
        let msg = make_cloud_event(
            "xzardgz.technical_review.task",
            serde_json::json!({
                "correlation_id": "corr-integration-001",
                "repository": repo_path
            }),
        );

        handler.handle(msg).await.unwrap();

        // Assert result was published with the correct correlation_id.
        let results = published.lock().await;
        assert_eq!(results.len(), 1, "expected exactly one published result");
        assert_eq!(results[0].correlation_id, "corr-integration-001");
    }

    #[tokio::test]
    async fn test_watcher_message_handler_handle_with_result_event_skips_silently() {
        let handler = make_simple_handler();
        let msg = make_cloud_event(
            "xzardgz.technical_review.result",
            serde_json::json!({
                "correlation_id": "corr-001",
                "repository": "https://github.com/example/repo"
            }),
        );
        let result = handler.handle(msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_watcher_message_handler_handle_with_missing_correlation_id_skips_with_warning() {
        let handler = make_simple_handler();
        let msg = make_cloud_event(
            "xzardgz.technical_review.task",
            serde_json::json!({
                "repository": "https://github.com/example/repo"
            }),
        );
        let result = handler.handle(msg).await;
        assert!(result.is_ok());
    }
}
