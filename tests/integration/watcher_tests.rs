//! Phase 19 integration tests for watcher executor functionality.
//!
//! Exercises end-to-end flows through [`WatcherExecutor`] using in-memory mocks
//! for the result publisher and a minimal [`WorkflowPlugin`] implementation that
//! returns success without making any AI API calls or requiring network access.
//!
//! Each test stands alone: it constructs its own config, registry, executor, and
//! publisher from scratch so failures are isolated to a single test scenario.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tempfile::TempDir;
use tokio::sync::Mutex;
use xzardgz::config::Config;
use xzardgz::error::{PipelineError, Result};
use xzardgz::plugins::context::{PluginContext, ToolAccessLevel};
use xzardgz::plugins::output::PluginOutput;
use xzardgz::plugins::registry::PluginRegistry;
use xzardgz::plugins::trait_def::{PluginMetadata, WorkflowPlugin};
use xzardgz::watcher::event_type::WatcherEventType;
use xzardgz::watcher::executor::WatcherExecutor;
use xzardgz::watcher::publisher::{PublishFailureState, ResultPublisher};
use xzardgz::watcher::result::WatcherResultMessage;
use xzardgz::watcher::task::{WATCHER_TASK_VERSION, WatcherTaskMessage};

// ---------------------------------------------------------------------------
// TestPlugin
// ---------------------------------------------------------------------------

/// Minimal [`WorkflowPlugin`] that returns success without invoking any AI provider.
///
/// Used in watcher integration tests that need a registered, working plugin
/// without triggering real API calls or requiring a valid API key.
/// Provider construction still succeeds (no HTTP calls are made), and since
/// `run` never calls `provider.complete()`, no API key is required.
struct TestPlugin {
    name: &'static str,
}

#[async_trait]
impl WorkflowPlugin for TestPlugin {
    fn name(&self) -> &str {
        self.name
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            self.name,
            "1.0.0",
            "Minimal test plugin for integration tests.",
        )
    }

    fn supported_formats(&self) -> Vec<String> {
        vec!["markdown".to_string()]
    }

    fn required_tool_access(&self) -> ToolAccessLevel {
        ToolAccessLevel::None
    }

    async fn run(&self, _ctx: PluginContext) -> Result<PluginOutput> {
        Ok(PluginOutput::success("test success"))
    }
}

// ---------------------------------------------------------------------------
// MockPublisher
// ---------------------------------------------------------------------------

/// In-memory [`ResultPublisher`] for integration tests.
///
/// Records every successfully published result in `calls`. Construct with
/// [`MockPublisher::new_failing`] to simulate a Kafka broker error on every
/// publish call.
struct MockPublisher {
    should_fail: bool,
    calls: Mutex<Vec<WatcherResultMessage>>,
}

impl MockPublisher {
    /// Creates a [`MockPublisher`] that accepts all publish calls.
    fn new() -> Arc<Self> {
        Arc::new(Self {
            should_fail: false,
            calls: Mutex::new(Vec::new()),
        })
    }

    /// Creates a [`MockPublisher`] that always returns a Kafka error.
    fn new_failing() -> Arc<Self> {
        Arc::new(Self {
            should_fail: true,
            calls: Mutex::new(Vec::new()),
        })
    }

    /// Returns the list of result messages that were successfully published.
    async fn published(&self) -> Vec<WatcherResultMessage> {
        self.calls.lock().await.clone()
    }
}

#[async_trait]
impl ResultPublisher for MockPublisher {
    async fn publish(&self, result: &WatcherResultMessage) -> Result<()> {
        if self.should_fail {
            return Err(PipelineError::Kafka("mock failure".to_string()));
        }
        self.calls.lock().await.push(result.clone());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Builds a watcher-suitable [`Config`] with governance disabled and a single
/// JSON report format.
///
/// Governance is disabled to avoid attempting to parse the project-root
/// `AGENTS.md` file (which is Markdown, not YAML) as governance rules.
/// The JSON format ensures no report formatter fails for an unsupported format.
fn make_test_config(publish_enabled: bool) -> Arc<Config> {
    let mut config = Config::default();
    config.watcher.result_publish_enabled = publish_enabled;
    config.watcher.once = true;
    config.watcher.max_concurrent_tasks = 1;
    config.reports.formats = vec!["json".to_string()];
    config.governance.enabled = false;
    config.governance.rules_path = String::new();
    Arc::new(config)
}

/// Builds a [`PluginRegistry`] with a single `TestPlugin` registered under the
/// name `"test-plugin"`.
fn make_registry_with_test_plugin() -> Arc<PluginRegistry> {
    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(TestPlugin {
        name: "test-plugin",
    }));
    Arc::new(registry)
}

/// Builds a dry-run [`WatcherTaskMessage`] for `plugin` with `correlation_id`.
fn make_dry_run_task(plugin: &str, correlation_id: &str) -> WatcherTaskMessage {
    WatcherTaskMessage {
        id: "integ-task-001".to_string(),
        version: WATCHER_TASK_VERSION.to_string(),
        spec_version: "1.0".to_string(),
        event_type: WatcherEventType::TechnicalReviewTask,
        source: "integration-test".to_string(),
        repository: ".".to_string(),
        target_branch: None,
        provider: None,
        model: None,
        plugin: plugin.to_string(),
        plugin_config: serde_json::json!({}),
        dry_run: true,
        workspace_directory: None,
        metadata: HashMap::new(),
        requested_report_formats: vec![],
        correlation_id: correlation_id.to_string(),
        reply_topic_override: None,
    }
}

/// Builds a full-execution (non-dry-run) [`WatcherTaskMessage`].
///
/// `repo_path` is passed to the scanner (an empty temp directory is sufficient).
/// `workspace_path` is used as the workspace root for the run.
fn make_full_execution_task(
    plugin: &str,
    repo_path: &str,
    workspace_path: &str,
) -> WatcherTaskMessage {
    WatcherTaskMessage {
        id: "integ-task-002".to_string(),
        version: WATCHER_TASK_VERSION.to_string(),
        spec_version: "1.0".to_string(),
        event_type: WatcherEventType::TechnicalReviewTask,
        source: "integration-test".to_string(),
        repository: repo_path.to_string(),
        target_branch: None,
        provider: None,
        model: None,
        plugin: plugin.to_string(),
        plugin_config: serde_json::json!({}),
        dry_run: false,
        workspace_directory: Some(workspace_path.to_string()),
        metadata: HashMap::new(),
        requested_report_formats: vec!["json".to_string()],
        correlation_id: "corr-full-001".to_string(),
        reply_topic_override: None,
    }
}

/// Creates a temporary directory for a single test.
fn temp_dir() -> TempDir {
    // SAFETY: TempDir::new() only fails on OS-level resource exhaustion,
    // which does not occur in a standard CI test environment.
    TempDir::new().expect("SAFETY: OS can create temp directory in test environment")
}

/// Extracts the UTF-8 path string from a `TempDir`.
fn path_str(dir: &TempDir) -> &str {
    // SAFETY: OS temp directories are created under paths that are always
    // valid UTF-8 on the Linux, macOS, and Windows targets of this crate.
    dir.path()
        .to_str()
        .expect("SAFETY: temp dir path is valid UTF-8 on all supported platforms")
}

/// Builds a minimal [`WatcherResultMessage`] for publisher persistence tests.
fn make_dummy_result() -> WatcherResultMessage {
    WatcherResultMessage::new(
        "result-dummy-001",
        WatcherEventType::TechnicalReviewResult,
        "xzardgz-watcher",
        "github.com/test/repo",
        "test-plugin",
        "ws-dummy-001",
        "corr-dummy-001",
        "task-dummy-001",
        Utc::now(),
    )
}

// ---------------------------------------------------------------------------
// Test 1 -- dry-run with publish disabled returns success without publishing
// ---------------------------------------------------------------------------

/// A dry-run task with `result_publish_enabled = false` returns success and
/// does not invoke the publisher.
///
/// The validation passes (plugin is registered), dry-run short-circuits plugin
/// execution, and the result is returned without calling the publisher because
/// `result_publish_enabled = false`.
#[tokio::test]
async fn test_watcher_dry_run_returns_success_without_publish() {
    let executor = WatcherExecutor::new(make_test_config(false), make_registry_with_test_plugin());
    let task = make_dry_run_task("test-plugin", "corr-001");
    let publisher = MockPublisher::new();

    let result = executor
        .process_task(task, &*publisher)
        .await
        // SAFETY: a dry-run task with a registered plugin cannot return a
        // pipeline-level Err; task-level failures surface in result.errors.
        .expect("SAFETY: process_task must not return Err for a well-formed dry-run task");

    assert!(result.success, "dry-run result must be marked successful");
    assert!(
        result.errors.is_empty(),
        "dry-run result must have no errors; got: {:?}",
        result.errors
    );

    let calls = publisher.published().await;
    assert_eq!(
        calls.len(),
        0,
        "publisher must not be called when result_publish_enabled = false"
    );
}

// ---------------------------------------------------------------------------
// Test 2 -- dry-run with publish enabled records exactly one publish call
// ---------------------------------------------------------------------------

/// A dry-run task with `result_publish_enabled = true` triggers exactly one
/// publish call and returns a success result.
///
/// Verifies that the executor correctly respects the `result_publish_enabled`
/// flag and forwards the dry-run success result to the publisher.
#[tokio::test]
async fn test_watcher_dry_run_publishes_when_enabled() {
    let executor = WatcherExecutor::new(make_test_config(true), make_registry_with_test_plugin());
    let task = make_dry_run_task("test-plugin", "corr-002");
    let publisher = MockPublisher::new();

    let result = executor
        .process_task(task, &*publisher)
        .await
        // SAFETY: dry-run task with known plugin; cannot return pipeline Err.
        .expect("SAFETY: process_task must not return Err for a well-formed dry-run task");

    assert!(result.success, "dry-run result must be marked successful");

    let calls = publisher.published().await;
    assert_eq!(
        calls.len(),
        1,
        "publisher must receive exactly one result when result_publish_enabled = true"
    );
}

// ---------------------------------------------------------------------------
// Test 3 -- unknown plugin returns failure result without publishing
// ---------------------------------------------------------------------------

/// A task referencing an unregistered plugin returns a failure result and does
/// not invoke the publisher.
///
/// The executor validates the plugin name before execution. When the registry
/// returns `PluginNotFound`, the error is embedded into `result.errors` and the
/// method returns early, bypassing the publish step entirely.
#[tokio::test]
async fn test_watcher_unknown_plugin_returns_failure_without_publish() {
    let empty_registry = Arc::new(PluginRegistry::new());
    let executor = WatcherExecutor::new(make_test_config(false), empty_registry);
    let task = WatcherTaskMessage {
        id: "integ-unknown-001".to_string(),
        version: WATCHER_TASK_VERSION.to_string(),
        spec_version: "1.0".to_string(),
        event_type: WatcherEventType::TechnicalReviewTask,
        source: "integration-test".to_string(),
        repository: ".".to_string(),
        target_branch: None,
        provider: None,
        model: None,
        plugin: "nonexistent".to_string(),
        plugin_config: serde_json::json!({}),
        dry_run: false,
        workspace_directory: None,
        metadata: HashMap::new(),
        requested_report_formats: vec![],
        correlation_id: "corr-003".to_string(),
        reply_topic_override: None,
    };
    let publisher = MockPublisher::new();

    let result = executor
        .process_task(task, &*publisher)
        .await
        // SAFETY: validation failures are embedded in result.errors, not
        // propagated as Err; this call cannot return a pipeline-level error.
        .expect(
            "SAFETY: process_task must not return Err; validation errors go into result.errors",
        );

    assert!(
        !result.success,
        "result must not be successful for an unregistered plugin"
    );
    assert!(
        !result.errors.is_empty(),
        "result.errors must contain the plugin-not-found message"
    );

    let calls = publisher.published().await;
    assert_eq!(
        calls.len(),
        0,
        "publisher must not be called for a validation failure"
    );
}

// ---------------------------------------------------------------------------
// Test 4 -- full (non-dry-run) execution with TestPlugin returns success
// ---------------------------------------------------------------------------

/// A full (non-dry-run) execution with `TestPlugin` produces a success result.
///
/// `TestPlugin` does not call the AI provider, so this test is self-contained
/// and does not require network access or an API key.  The scanner walks an
/// empty temp directory which is a valid (empty) repository.
#[tokio::test]
async fn test_watcher_process_task_with_success_plugin() {
    let repo_dir = temp_dir();
    let ws_dir = temp_dir();

    let executor = WatcherExecutor::new(make_test_config(false), make_registry_with_test_plugin());
    let task = make_full_execution_task("test-plugin", path_str(&repo_dir), path_str(&ws_dir));
    let publisher = MockPublisher::new();

    let result = executor
        .process_task(task, &*publisher)
        .await
        // SAFETY: full execution with TestPlugin and two writable temp dirs
        // cannot produce a pipeline-level Err.
        .expect("SAFETY: process_task must not return Err for a well-formed full-execution task");

    assert!(
        result.success,
        "full execution with TestPlugin must succeed; errors: {:?}",
        result.errors
    );
}

// ---------------------------------------------------------------------------
// Test 5 -- PublishFailureState round-trips through persist and load
// ---------------------------------------------------------------------------

/// [`PublishFailureState`] written via `persist` can be recovered via `load`
/// with all recorded fields intact.
///
/// Verifies that the attempt count and last error message survive a full
/// serialize-to-disk / deserialize-from-disk cycle.
#[test]
fn test_publish_failure_state_persists_and_loads() {
    let tmp = temp_dir();
    let path = tmp.path().join("failure_state.json");

    let result = make_dummy_result();
    let mut state = PublishFailureState::new("task-persist-001", result);
    state.record_failure("test error");

    // SAFETY: the temp directory is always writable in a standard test environment.
    state
        .persist(&path)
        .expect("SAFETY: persist must succeed on a writable temp dir");

    assert!(path.exists(), "failure state file must exist after persist");

    // SAFETY: we just wrote valid JSON to this path; reading and parsing
    // the same file immediately afterward cannot fail.
    let loaded = PublishFailureState::load(&path)
        .expect("SAFETY: load must succeed for a file we just wrote")
        .expect("SAFETY: load must return Some when the file exists with valid JSON");

    assert!(
        loaded.attempts > 0,
        "loaded state must have at least one recorded attempt"
    );
    assert_eq!(
        loaded.last_error.as_deref(),
        Some("test error"),
        "loaded last_error must match the string passed to record_failure"
    );
    assert_eq!(
        loaded.task_id, "task-persist-001",
        "loaded task_id must match the value supplied to PublishFailureState::new"
    );
}

// ---------------------------------------------------------------------------
// Test 6 -- process_task_with_publish_failure_tracking saves state on error
// ---------------------------------------------------------------------------

/// When publishing fails, `process_task_with_publish_failure_tracking` writes a
/// failure state file and still returns the task result as `Ok`.
///
/// A successful plugin execution must never be silently lost due to a transient
/// Kafka outage; the failure state file allows an operator or retry daemon to
/// republish the result without re-executing the (potentially expensive) plugin.
#[tokio::test]
async fn test_process_task_with_publish_failure_tracking_saves_state() {
    let tmp = temp_dir();
    let failure_path = tmp.path().join("publish_failure.json");

    let executor = WatcherExecutor::new(make_test_config(true), make_registry_with_test_plugin());
    let task = make_dry_run_task("test-plugin", "corr-006");
    let publisher = MockPublisher::new_failing();

    let result = executor
        .process_task_with_publish_failure_tracking(task, &*publisher, Some(&failure_path))
        .await
        // SAFETY: this method always returns Ok; publish errors are recorded in
        // the failure state file rather than propagated as a pipeline Err.
        .expect("SAFETY: process_task_with_publish_failure_tracking must not return Err");

    assert!(
        result.success,
        "task result must be successful despite publish failure; errors: {:?}",
        result.errors
    );
    assert!(
        failure_path.exists(),
        "failure state file must be created at failure_path when publishing fails"
    );
}

// ---------------------------------------------------------------------------
// Test 7 -- result carries the exact correlation ID from the originating task
// ---------------------------------------------------------------------------

/// The result message carries the exact `correlation_id` from the originating
/// task message.
///
/// The correlation ID must survive the entire executor pipeline path so that
/// downstream consumers can link the result back to the original task without
/// relying on any other shared identifier.
#[tokio::test]
async fn test_watcher_result_has_correct_correlation_id() {
    let executor = WatcherExecutor::new(make_test_config(false), make_registry_with_test_plugin());
    let task = make_dry_run_task("test-plugin", "test-corr-123");
    let publisher = MockPublisher::new();

    let result = executor
        .process_task(task, &*publisher)
        .await
        // SAFETY: dry-run task with known plugin; cannot return pipeline Err.
        .expect("SAFETY: process_task must not return Err for a well-formed dry-run task");

    assert_eq!(
        result.correlation_id, "test-corr-123",
        "result.correlation_id must equal the correlation_id on the originating task"
    );
}
