//! Watcher task executor.
//!
//! [`WatcherExecutor`] is the core processing engine for a single watcher task.
//! It validates the incoming [`WatcherTaskMessage`], optionally short-circuits
//! for dry runs, runs the plugin stub (full execution wired in Phase 17), and
//! returns a [`WatcherResultMessage`] to the caller.  Publishing the result
//! back to Kafka is either done inline via [`WatcherExecutor::process_task`]
//! or with failure tracking via
//! [`WatcherExecutor::process_task_with_publish_failure_tracking`].

use std::sync::Arc;

use chrono::Utc;
use tracing::warn;
use ulid::Ulid;

use crate::config::Config;
use crate::diagnostics::{Diagnostic, DiagnosticCategory};
use crate::error::Result;
use crate::plugins::registry::PluginRegistry;
use crate::watcher::event_type::WatcherEventType;
use crate::watcher::publisher::{PublishFailureState, ResultPublisher};
use crate::watcher::result::WatcherResultMessage;
use crate::watcher::task::WatcherTaskMessage;

// ---------------------------------------------------------------------------
// WatcherExecutor
// ---------------------------------------------------------------------------

/// Handles the core processing of a single watcher task.
///
/// The executor is decoupled from Kafka transport.  It receives a
/// [`WatcherTaskMessage`], validates the plugin, optionally short-circuits for
/// dry runs, runs the plugin (stub in Phase 14; full execution in Phase 17),
/// and returns a [`WatcherResultMessage`].  Publishing the result to Kafka is
/// the caller's responsibility or is optionally done inline when
/// `config.watcher.result_publish_enabled` is `true`.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use xzardgz::config::Config;
/// use xzardgz::plugins::registry::PluginRegistry;
/// use xzardgz::watcher::executor::WatcherExecutor;
///
/// let config = Arc::new(Config::default());
/// let registry = Arc::new(PluginRegistry::new());
/// let executor = WatcherExecutor::new(config, registry);
/// assert!(!executor.result_publish_enabled());
/// ```
pub struct WatcherExecutor {
    config: Arc<Config>,
    plugin_registry: Arc<PluginRegistry>,
}

impl WatcherExecutor {
    /// Creates a new [`WatcherExecutor`] with the given config and plugin registry.
    ///
    /// # Arguments
    ///
    /// * `config` - The effective pipeline configuration.
    /// * `plugin_registry` - The plugin registry to look up plugins from.
    ///
    /// # Returns
    ///
    /// A new `WatcherExecutor`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use xzardgz::config::Config;
    /// use xzardgz::plugins::registry::PluginRegistry;
    /// use xzardgz::watcher::executor::WatcherExecutor;
    ///
    /// let config = Arc::new(Config::default());
    /// let registry = Arc::new(PluginRegistry::new());
    /// let executor = WatcherExecutor::new(config, registry);
    /// assert_eq!(executor.max_concurrent_tasks(), 2);
    /// ```
    pub fn new(config: Arc<Config>, plugin_registry: Arc<PluginRegistry>) -> Self {
        Self {
            config,
            plugin_registry,
        }
    }

    /// Returns `true` if the watcher is configured to exit after one batch.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use xzardgz::config::Config;
    /// use xzardgz::plugins::registry::PluginRegistry;
    /// use xzardgz::watcher::executor::WatcherExecutor;
    ///
    /// let config = Arc::new(Config::default());
    /// let registry = Arc::new(PluginRegistry::new());
    /// let executor = WatcherExecutor::new(config, registry);
    /// assert!(!executor.once_mode_enabled());
    /// ```
    pub fn once_mode_enabled(&self) -> bool {
        self.config.watcher.once
    }

    /// Returns the maximum number of pipeline tasks to run concurrently.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use xzardgz::config::Config;
    /// use xzardgz::plugins::registry::PluginRegistry;
    /// use xzardgz::watcher::executor::WatcherExecutor;
    ///
    /// let config = Arc::new(Config::default());
    /// let registry = Arc::new(PluginRegistry::new());
    /// let executor = WatcherExecutor::new(config, registry);
    /// assert_eq!(executor.max_concurrent_tasks(), 2);
    /// ```
    pub fn max_concurrent_tasks(&self) -> u32 {
        self.config.watcher.max_concurrent_tasks
    }

    /// Returns `true` if result publishing to Kafka is enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use xzardgz::config::Config;
    /// use xzardgz::plugins::registry::PluginRegistry;
    /// use xzardgz::watcher::executor::WatcherExecutor;
    ///
    /// let config = Arc::new(Config::default());
    /// let registry = Arc::new(PluginRegistry::new());
    /// let executor = WatcherExecutor::new(config, registry);
    /// // Default config has result_publish_enabled = true.
    /// assert!(executor.result_publish_enabled());
    /// ```
    pub fn result_publish_enabled(&self) -> bool {
        self.config.watcher.result_publish_enabled
    }

    /// Validates that the task's plugin is registered, enabled, and has a
    /// valid configuration.
    ///
    /// # Arguments
    ///
    /// * `task` - The task message to validate.
    ///
    /// # Returns
    ///
    /// `Ok(())` when the plugin passes all checks.
    ///
    /// # Errors
    ///
    /// - [`crate::error::PipelineError::PluginNotFound`] if the plugin is not
    ///   registered.
    /// - [`crate::error::PipelineError::Plugin`] if the plugin is disabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use std::sync::Arc;
    /// use xzardgz::config::Config;
    /// use xzardgz::plugins::registry::PluginRegistry;
    /// use xzardgz::watcher::executor::WatcherExecutor;
    /// use xzardgz::watcher::task::{WatcherTaskMessage, WATCHER_TASK_VERSION};
    /// use xzardgz::watcher::event_type::WatcherEventType;
    ///
    /// let config = Arc::new(Config::default());
    /// let registry = Arc::new(PluginRegistry::new());
    /// let executor = WatcherExecutor::new(config, registry);
    ///
    /// let task = WatcherTaskMessage {
    ///     id: "t1".to_string(),
    ///     version: WATCHER_TASK_VERSION.to_string(),
    ///     spec_version: "1.0".to_string(),
    ///     event_type: WatcherEventType::TechnicalReviewTask,
    ///     source: "ci".to_string(),
    ///     repository: "github.com/org/repo".to_string(),
    ///     target_branch: None,
    ///     provider: None,
    ///     model: None,
    ///     plugin: "nonexistent_plugin".to_string(),
    ///     plugin_config: serde_json::json!({}),
    ///     dry_run: false,
    ///     workspace_directory: None,
    ///     metadata: HashMap::new(),
    ///     requested_report_formats: vec![],
    ///     correlation_id: "c1".to_string(),
    ///     reply_topic_override: None,
    /// };
    ///
    /// assert!(executor.validate_task(&task).is_err());
    /// ```
    pub fn validate_task(&self, task: &WatcherTaskMessage) -> Result<()> {
        // Checks both "not found" and "disabled" via PluginRegistry::get.
        self.plugin_registry.get(&task.plugin)?;
        self.plugin_registry
            .validate_plugin_config(&task.plugin, &task.plugin_config)?;
        Ok(())
    }

    /// Processes a task through the plugin registry and returns a result.
    ///
    /// This is the core processing method.  It validates the task, runs the
    /// plugin stub (full execution in Phase 17), and optionally publishes the
    /// result inline when `config.watcher.result_publish_enabled` is `true`.
    ///
    /// If the task fails validation, a failure result is returned immediately
    /// without publishing.
    ///
    /// If result publishing fails, the error is propagated.  Use
    /// [`process_task_with_publish_failure_tracking`][Self::process_task_with_publish_failure_tracking]
    /// to persist publish failures instead of propagating them.
    ///
    /// # Arguments
    ///
    /// * `task` - The inbound task message to process.
    /// * `publisher` - The result publisher to use if publishing is enabled.
    ///
    /// # Returns
    ///
    /// A [`WatcherResultMessage`] reflecting the outcome of the run.
    ///
    /// # Errors
    ///
    /// Returns an error only if result publishing fails.  Plugin validation
    /// failures and stub errors are captured inside the returned
    /// `WatcherResultMessage` (i.e. `result.success = false`, `result.errors`
    /// populated).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use xzardgz::config::Config;
    /// use xzardgz::plugins::registry::PluginRegistry;
    /// use xzardgz::watcher::executor::WatcherExecutor;
    /// // See unit tests for a runnable async example.
    /// ```
    pub async fn process_task(
        &self,
        task: WatcherTaskMessage,
        publisher: &dyn ResultPublisher,
    ) -> Result<WatcherResultMessage> {
        let started_at = Utc::now();
        let (result, should_publish) = self.build_result_for_task(&task, started_at);

        if should_publish && self.config.watcher.result_publish_enabled {
            publisher.publish(&result).await?;
        }

        Ok(result)
    }

    /// Processes a task and publishes the result, tracking publish failures.
    ///
    /// Behaves identically to [`process_task`][Self::process_task] for task
    /// execution.  The difference is in publish failure handling: if
    /// publishing fails and `failure_path` is `Some`, a
    /// [`PublishFailureState`] is persisted to `failure_path` so the result
    /// can be retried later.  The method always returns `Ok(result)` regardless
    /// of publish outcome so that a successful plugin result is never lost.
    ///
    /// # Arguments
    ///
    /// * `task` - The inbound task message to process.
    /// * `publisher` - The result publisher to use if publishing is enabled.
    /// * `failure_path` - Optional file path for persisting publish failure
    ///   state.
    ///
    /// # Returns
    ///
    /// `Ok(WatcherResultMessage)` regardless of publish outcome.
    ///
    /// # Errors
    ///
    /// This method does not propagate publish errors; they are tracked in the
    /// failure state file.  Task execution errors are returned inside the
    /// result message (not as `Err`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use xzardgz::config::Config;
    /// use xzardgz::plugins::registry::PluginRegistry;
    /// use xzardgz::watcher::executor::WatcherExecutor;
    /// // See unit tests for a runnable async example.
    /// ```
    pub async fn process_task_with_publish_failure_tracking(
        &self,
        task: WatcherTaskMessage,
        publisher: &dyn ResultPublisher,
        failure_path: Option<&std::path::Path>,
    ) -> Result<WatcherResultMessage> {
        let started_at = Utc::now();
        let task_id = task.id.clone();
        let (result, should_publish) = self.build_result_for_task(&task, started_at);

        if should_publish
            && self.config.watcher.result_publish_enabled
            && let Err(pub_err) = publisher.publish(&result).await
            && let Some(path) = failure_path
        {
            let mut failure_state = PublishFailureState::new(task_id.clone(), result.clone());
            failure_state.record_failure(pub_err.to_string());
            if let Err(persist_err) = failure_state.persist(path) {
                warn!(
                    task_id = %task_id,
                    error = %persist_err,
                    "failed to persist publish failure state after publish error"
                );
            }
        }

        Ok(result)
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Builds a [`WatcherResultMessage`] for `task`, validating and running
    /// the plugin stub.
    ///
    /// Returns the result and a boolean `should_publish` flag:
    /// - `false` on validation failure (we do not publish failure results from
    ///   invalid tasks in the base flow).
    /// - `true` otherwise.
    fn build_result_for_task(
        &self,
        task: &WatcherTaskMessage,
        started_at: chrono::DateTime<Utc>,
    ) -> (WatcherResultMessage, bool) {
        let result_id = Ulid::new().to_string();

        let result_event_type = match task.event_type {
            WatcherEventType::TechnicalReviewTask | WatcherEventType::TechnicalReviewResult => {
                WatcherEventType::TechnicalReviewResult
            }
            WatcherEventType::SecurityReviewTask | WatcherEventType::SecurityReviewResult => {
                WatcherEventType::SecurityReviewResult
            }
        };

        // Stub workspace ID; real construction happens in Phase 17.
        let workspace_id = Ulid::new().to_string();

        let mut result = WatcherResultMessage::new(
            result_id,
            result_event_type,
            "xzardgz-watcher",
            task.repository.clone(),
            task.plugin.clone(),
            workspace_id,
            task.correlation_id.clone(),
            task.id.clone(),
            started_at,
        );
        result.target_branch = task.target_branch.clone();

        // Step 1: Validate the task.
        if let Err(e) = self.validate_task(task) {
            result.errors.push(e.to_string());
            return (result, false);
        }

        // Step 2: Dry-run short-circuit.
        if task.dry_run {
            result.diagnostics.push(Diagnostic::info(
                DiagnosticCategory::Plugin,
                format!("dry-run: plugin '{}' was not executed", task.plugin),
            ));
            result = result.success_result();
            return (result, true);
        }

        // Step 3: Stub plugin execution (full wiring in Phase 17).
        result.diagnostics.push(Diagnostic::info(
            DiagnosticCategory::Plugin,
            format!(
                "plugin '{}' dispatched via watcher (full execution in Phase 17)",
                task.plugin
            ),
        ));
        result = result.success_result();

        (result, true)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use chrono::Utc;
    use tokio::sync::Mutex;

    use crate::error::{PipelineError, Result};
    use crate::plugins::context::{PluginContext, ToolAccessLevel};
    use crate::plugins::output::PluginOutput;
    use crate::plugins::registry::PluginRegistry;
    use crate::plugins::trait_def::{PluginMetadata, WorkflowPlugin};
    use crate::watcher::event_type::WatcherEventType;
    use crate::watcher::result::WatcherResultMessage;
    use crate::watcher::task::{WATCHER_TASK_VERSION, WatcherTaskMessage};

    // ------------------------------------------------------------------
    // TestPlugin - minimal WorkflowPlugin for executor tests
    // ------------------------------------------------------------------

    /// Minimal concrete [`WorkflowPlugin`] used for executor tests.
    struct TestPlugin {
        plugin_name: String,
    }

    impl TestPlugin {
        fn new(name: impl Into<String>) -> Arc<Self> {
            Arc::new(Self {
                plugin_name: name.into(),
            })
        }
    }

    #[async_trait]
    impl WorkflowPlugin for TestPlugin {
        fn name(&self) -> &str {
            &self.plugin_name
        }

        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(&self.plugin_name, "0.1.0", "Test plugin for executor tests")
        }

        fn supported_formats(&self) -> Vec<String> {
            vec!["markdown".to_string()]
        }

        fn required_tool_access(&self) -> ToolAccessLevel {
            ToolAccessLevel::None
        }

        async fn run(&self, _ctx: PluginContext) -> Result<PluginOutput> {
            Ok(PluginOutput::success("test complete"))
        }
    }

    // ------------------------------------------------------------------
    // MockResultPublisher
    // ------------------------------------------------------------------

    /// In-memory result publisher for executor tests.
    struct MockResultPublisher {
        published: Mutex<Vec<WatcherResultMessage>>,
        should_fail: bool,
    }

    impl MockResultPublisher {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                published: Mutex::new(vec![]),
                should_fail: false,
            })
        }

        fn new_failing() -> Arc<Self> {
            Arc::new(Self {
                published: Mutex::new(vec![]),
                should_fail: true,
            })
        }

        async fn get_published(&self) -> Vec<WatcherResultMessage> {
            self.published.lock().await.clone()
        }
    }

    #[async_trait]
    impl ResultPublisher for MockResultPublisher {
        async fn publish(&self, result: &WatcherResultMessage) -> Result<()> {
            if self.should_fail {
                return Err(PipelineError::Kafka("mock publish failed".to_string()));
            }
            self.published.lock().await.push(result.clone());
            Ok(())
        }
    }

    // ------------------------------------------------------------------
    // Test helpers
    // ------------------------------------------------------------------

    fn make_mock_plugin(name: impl Into<String>) -> Arc<TestPlugin> {
        TestPlugin::new(name)
    }

    fn make_test_registry() -> Arc<PluginRegistry> {
        let mut registry = PluginRegistry::new();
        registry.register(make_mock_plugin("technical_review"));
        registry.register(make_mock_plugin("security_review"));
        Arc::new(registry)
    }

    fn make_test_config(publish_enabled: bool) -> Arc<Config> {
        let mut config = Config::default();
        config.watcher.result_publish_enabled = publish_enabled;
        Arc::new(config)
    }

    fn make_test_task(plugin: &str, dry_run: bool) -> WatcherTaskMessage {
        WatcherTaskMessage {
            id: "test-task-001".to_string(),
            version: WATCHER_TASK_VERSION.to_string(),
            spec_version: "1.0".to_string(),
            event_type: WatcherEventType::TechnicalReviewTask,
            source: "test-ci".to_string(),
            repository: "github.com/test/repo".to_string(),
            target_branch: Some("main".to_string()),
            provider: None,
            model: None,
            plugin: plugin.to_string(),
            plugin_config: serde_json::json!({}),
            dry_run,
            workspace_directory: None,
            metadata: HashMap::new(),
            requested_report_formats: vec!["markdown".to_string()],
            correlation_id: "corr-001".to_string(),
            reply_topic_override: None,
        }
    }

    fn make_dummy_result() -> WatcherResultMessage {
        WatcherResultMessage::new(
            "result-dummy-001",
            WatcherEventType::TechnicalReviewResult,
            "xzardgz-watcher",
            "github.com/test/repo",
            "technical_review",
            "ws-dummy-001",
            "corr-001",
            "task-001",
            Utc::now(),
        )
    }

    // ------------------------------------------------------------------
    // validate_task tests
    // ------------------------------------------------------------------

    #[test]
    fn test_watcher_executor_validate_task_unknown_plugin_returns_err() {
        let registry = Arc::new(PluginRegistry::new());
        let executor = WatcherExecutor::new(make_test_config(false), registry);
        let task = make_test_task("unknown_plugin", false);
        let result = executor.validate_task(&task);
        assert!(result.is_err());
        assert!(matches!(result, Err(PipelineError::PluginNotFound { .. })));
    }

    #[test]
    fn test_watcher_executor_validate_task_known_plugin_returns_ok() {
        let executor = WatcherExecutor::new(make_test_config(false), make_test_registry());
        let task = make_test_task("technical_review", false);
        assert!(executor.validate_task(&task).is_ok());
    }

    #[test]
    fn test_watcher_executor_validate_task_disabled_plugin_returns_err() {
        let mut registry = PluginRegistry::new();
        registry.register(make_mock_plugin("disabled_plugin"));
        registry.disable("disabled_plugin");
        let executor = WatcherExecutor::new(make_test_config(false), Arc::new(registry));
        let task = make_test_task("disabled_plugin", false);
        let result = executor.validate_task(&task);
        assert!(result.is_err());
        assert!(matches!(result, Err(PipelineError::Plugin(_))));
    }

    // ------------------------------------------------------------------
    // process_task tests
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_watcher_executor_process_task_dry_run_does_not_call_plugin() {
        let executor = WatcherExecutor::new(make_test_config(true), make_test_registry());
        let task = make_test_task("technical_review", true);
        let publisher = MockResultPublisher::new();

        let result = executor.process_task(task, &*publisher).await.unwrap();

        assert!(result.success);
        // Verify the dry-run diagnostic is present.
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("dry-run"))
        );

        // Result was published.
        let published = publisher.get_published().await;
        assert_eq!(published.len(), 1);
    }

    #[tokio::test]
    async fn test_watcher_executor_process_task_unknown_plugin_returns_failure_result() {
        let executor =
            WatcherExecutor::new(make_test_config(false), Arc::new(PluginRegistry::new()));
        let task = make_test_task("unknown_plugin", false);
        let publisher = MockResultPublisher::new();

        let result = executor.process_task(task, &*publisher).await.unwrap();

        assert!(!result.success);
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("unknown_plugin"));
    }

    #[tokio::test]
    async fn test_watcher_executor_process_task_success_publishes_result() {
        let executor = WatcherExecutor::new(make_test_config(true), make_test_registry());
        let task = make_test_task("technical_review", false);
        let publisher = MockResultPublisher::new();

        let result = executor.process_task(task, &*publisher).await.unwrap();

        assert!(result.success);
        let published = publisher.get_published().await;
        assert_eq!(published.len(), 1);
    }

    #[tokio::test]
    async fn test_watcher_executor_process_task_success_result_has_correct_correlation_id() {
        let executor = WatcherExecutor::new(make_test_config(false), make_test_registry());
        let task = make_test_task("technical_review", false);
        let publisher = MockResultPublisher::new();

        let result = executor.process_task(task, &*publisher).await.unwrap();

        assert_eq!(result.correlation_id, "corr-001");
        assert_eq!(result.original_task_id, "test-task-001");
    }

    #[tokio::test]
    async fn test_watcher_executor_process_task_publish_disabled_does_not_publish() {
        let executor = WatcherExecutor::new(make_test_config(false), make_test_registry());
        let task = make_test_task("technical_review", false);
        let publisher = MockResultPublisher::new();

        executor.process_task(task, &*publisher).await.unwrap();

        let published = publisher.get_published().await;
        assert_eq!(published.len(), 0);
    }

    // ------------------------------------------------------------------
    // process_task_with_publish_failure_tracking tests
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_watcher_executor_process_task_with_failure_tracking_returns_ok_on_publish_error()
    {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let failure_path = tmp.path().join("failure.json");

        let executor = WatcherExecutor::new(make_test_config(true), make_test_registry());
        let task = make_test_task("technical_review", false);
        let publisher = MockResultPublisher::new_failing();

        let result = executor
            .process_task_with_publish_failure_tracking(task, &*publisher, Some(&failure_path))
            .await
            .unwrap();

        // Result returned successfully despite publish failure.
        assert!(result.success);
        // Failure state was persisted.
        assert!(failure_path.exists());
    }

    #[tokio::test]
    async fn test_watcher_executor_process_task_with_failure_tracking_succeeds_normally() {
        let executor = WatcherExecutor::new(make_test_config(true), make_test_registry());
        let task = make_test_task("technical_review", false);
        let publisher = MockResultPublisher::new();

        let result = executor
            .process_task_with_publish_failure_tracking(task, &*publisher, None)
            .await
            .unwrap();

        assert!(result.success);
        let published = publisher.get_published().await;
        assert_eq!(published.len(), 1);
    }

    // ------------------------------------------------------------------
    // once_mode_enabled / max_concurrent_tasks / result_publish_enabled
    // ------------------------------------------------------------------

    #[test]
    fn test_watcher_executor_once_mode_enabled_default_is_false() {
        let executor =
            WatcherExecutor::new(Arc::new(Config::default()), Arc::new(PluginRegistry::new()));
        assert!(!executor.once_mode_enabled());
    }

    #[test]
    fn test_watcher_executor_max_concurrent_tasks_default_is_two() {
        let executor =
            WatcherExecutor::new(Arc::new(Config::default()), Arc::new(PluginRegistry::new()));
        assert_eq!(executor.max_concurrent_tasks(), 2);
    }

    #[test]
    fn test_watcher_executor_result_publish_enabled_default_is_true() {
        let executor =
            WatcherExecutor::new(Arc::new(Config::default()), Arc::new(PluginRegistry::new()));
        assert!(executor.result_publish_enabled());
    }

    #[test]
    fn test_watcher_executor_once_mode_enabled_reflects_config() {
        let mut config = Config::default();
        config.watcher.once = true;
        let executor = WatcherExecutor::new(Arc::new(config), Arc::new(PluginRegistry::new()));
        assert!(executor.once_mode_enabled());
    }

    // ------------------------------------------------------------------
    // PublishFailureState tests (covered via executor module)
    // ------------------------------------------------------------------

    #[test]
    fn test_publish_failure_state_new_sets_fields() {
        let result = make_dummy_result();
        let state = PublishFailureState::new("task-001", result.clone());
        assert_eq!(state.task_id, "task-001");
        assert_eq!(state.attempts, 0);
        assert!(state.last_error.is_none());
        assert!(state.last_attempt_at.is_none());
    }

    #[test]
    fn test_publish_failure_state_record_failure_increments_attempts() {
        let result = make_dummy_result();
        let mut state = PublishFailureState::new("task-001", result);
        state.record_failure("first error");
        assert_eq!(state.attempts, 1);
        assert_eq!(state.last_error.as_deref(), Some("first error"));
        assert!(state.last_attempt_at.is_some());

        state.record_failure("second error");
        assert_eq!(state.attempts, 2);
        assert_eq!(state.last_error.as_deref(), Some("second error"));
    }

    #[test]
    fn test_publish_failure_state_to_json_from_json_roundtrip() {
        let result = make_dummy_result();
        let mut state = PublishFailureState::new("task-rt-001", result);
        state.record_failure("test error");

        // SAFETY: well-formed struct; serialization cannot fail.
        let json = state.to_json().unwrap();
        // SAFETY: we just serialized this string.
        let restored = PublishFailureState::from_json(&json).unwrap();
        assert_eq!(restored.task_id, state.task_id);
        assert_eq!(restored.attempts, state.attempts);
        assert_eq!(restored.last_error, state.last_error);
    }

    #[test]
    fn test_publish_failure_state_persist_and_load_roundtrip() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("failure.json");

        let result = make_dummy_result();
        let mut state = PublishFailureState::new("task-persist-001", result);
        state.record_failure("network error");

        // SAFETY: temp dir is writable in standard test environments.
        state.persist(&path).unwrap();
        assert!(path.exists());

        // SAFETY: we just wrote the file; it exists and contains valid JSON.
        let loaded = PublishFailureState::load(&path).unwrap().unwrap();
        assert_eq!(loaded.task_id, "task-persist-001");
        assert_eq!(loaded.attempts, 1);
    }
}
