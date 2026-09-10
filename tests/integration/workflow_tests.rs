//! Phase 19 integration tests: workflow executor end-to-end validation.
//!
//! Tests in this module exercise `WorkflowExecutor` through realistic
//! scenarios: scan-only runs, dry-run plan validation, provider-backed plugin
//! execution via a wiremock OpenAI server, unknown plugin error capture, and
//! a custom plugin that does not invoke the AI provider.

use std::fs;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use xzardgz::config::Config;
use xzardgz::error::Result;
use xzardgz::plugins::context::{PluginContext, ToolAccessLevel};
use xzardgz::plugins::output::PluginOutput;
use xzardgz::plugins::registry::PluginRegistry;
use xzardgz::plugins::technical_review::TechnicalReviewPlugin;
use xzardgz::plugins::trait_def::{PluginMetadata, WorkflowPlugin};
use xzardgz::workflow::executor::{ExecutionInput, WorkflowExecutor};
use xzardgz::workflow::plan::{PLAN_VERSION, PluginStep, WorkflowPlan};

// ---------------------------------------------------------------------------
// Shared test helpers
// ---------------------------------------------------------------------------

/// Builds a test-safe `Config` with governance disabled and JSON-only reports.
///
/// # Arguments
///
/// * `workspace_root` - Path used as the workspace root directory.
///
/// # Returns
///
/// A `Config` with governance disabled, no rules path, and JSON-only report
/// formats, suitable for integration tests that do not test governance rules.
fn make_test_config(workspace_root: &str) -> Config {
    let mut config = Config::default();
    config.workspace.root = workspace_root.to_string();
    config.reports.formats = vec!["json".to_string()];
    config.governance.enabled = false;
    config.governance.rules_path = String::new();
    config
}

/// Builds a single-step `WorkflowPlan` for integration testing.
///
/// # Arguments
///
/// * `plugin` - Plugin name for the single step.
/// * `repo_path` - Absolute path to the repository directory.
/// * `workspace_dir` - Absolute path to the workspace directory.
/// * `dry_run` - Whether to enable dry-run mode on the plan.
///
/// # Returns
///
/// A `WorkflowPlan` with one step that requests JSON output and no
/// dependencies.
fn make_plan(plugin: &str, repo_path: &str, workspace_dir: &str, dry_run: bool) -> WorkflowPlan {
    WorkflowPlan {
        version: PLAN_VERSION.to_string(),
        name: "integration-test-plan".to_string(),
        description: None,
        repository: repo_path.to_string(),
        branch: None,
        workspace: Some(workspace_dir.to_string()),
        provider: None,
        model: None,
        scan: None,
        steps: vec![PluginStep {
            id: "step1".to_string(),
            description: None,
            plugin: plugin.to_string(),
            config: None,
            dependencies: vec![],
            report_formats: Some(vec!["json".to_string()]),
            max_findings: None,
            severity_threshold: None,
        }],
        reports: None,
        dry_run,
        resume: false,
        correlation_id: None,
    }
}

// ---------------------------------------------------------------------------
// Test 1: ScanOnly produces a scan artifact
// ---------------------------------------------------------------------------

/// Verifies that a scan-only execution produces a persisted scan artifact.
///
/// A `ScanOnly` run invokes the repository scanner and writes a YAML artifact
/// to the workspace.  No plugins are invoked and no reports are written.
///
/// Asserts:
/// - `result.success == true`
/// - `result.scan_artifact_path.is_some()`
#[tokio::test]
async fn test_scan_only_workflow_produces_scan_artifact() {
    // SAFETY: TempDir::new only fails on OS-level failure in a test environment.
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");
    // SAFETY: repo_dir is under a fresh TempDir; directory creation cannot fail.
    fs::create_dir_all(&repo_dir).unwrap();
    fs::write(repo_dir.join("main.rs"), "fn main() {}\n").unwrap();
    let ws_dir = temp.path().join("workspaces");
    fs::create_dir_all(&ws_dir).unwrap();

    let config = make_test_config(ws_dir.to_str().expect("workspace dir must be valid UTF-8"));
    let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(PluginRegistry::new()));

    let result = executor
        .execute(ExecutionInput::ScanOnly {
            repository: repo_dir.to_string_lossy().to_string(),
            output_path: None,
            branch: None,
            resume: false,
            workspace: None,
            correlation_id: None,
        })
        .await
        .expect("scan-only execution must succeed");

    assert!(
        result.success,
        "expected success=true; errors: {:?}",
        result.errors
    );
    assert!(
        result.scan_artifact_path.is_some(),
        "scan artifact path must be set after a scan-only run"
    );
}

// ---------------------------------------------------------------------------
// Test 2: Dry-run plan skips plugin execution
// ---------------------------------------------------------------------------

/// Verifies that a dry-run plan does not invoke any registered plugin.
///
/// `PanicPlugin::run` panics unconditionally.  If the executor invokes it
/// during a dry run the test would fail with a panic, proving that dry-run
/// mode correctly short-circuits before plugin execution.
///
/// Asserts:
/// - `result.is_dry_run == true`
/// - `result.success == true`
#[tokio::test]
async fn test_dry_run_plan_skips_plugin_execution() {
    /// Plugin whose `run` method panics unconditionally.
    ///
    /// Used to assert that dry-run mode never reaches plugin execution.
    struct PanicPlugin;

    #[async_trait]
    impl WorkflowPlugin for PanicPlugin {
        fn name(&self) -> &str {
            "panic-plugin"
        }

        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(
                "panic-plugin",
                "1.0.0",
                "Plugin that must never be invoked.",
            )
        }

        fn supported_formats(&self) -> Vec<String> {
            vec![]
        }

        fn required_tool_access(&self) -> ToolAccessLevel {
            ToolAccessLevel::None
        }

        async fn run(&self, _ctx: PluginContext) -> Result<PluginOutput> {
            panic!("PanicPlugin::run must not be called during a dry run");
        }
    }

    // SAFETY: TempDir::new only fails on OS-level failure in a test environment.
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");
    // SAFETY: repo_dir is under a fresh TempDir; directory creation cannot fail.
    fs::create_dir_all(&repo_dir).unwrap();
    fs::write(repo_dir.join("main.rs"), "fn main() {}\n").unwrap();
    let ws_dir = temp.path().join("workspaces");
    fs::create_dir_all(&ws_dir).unwrap();

    let config = make_test_config(ws_dir.to_str().expect("workspace dir must be valid UTF-8"));
    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(PanicPlugin));
    let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(registry));

    let plan = make_plan(
        "panic-plugin",
        &repo_dir.to_string_lossy(),
        &ws_dir.to_string_lossy(),
        true,
    );

    let result = executor
        .execute(ExecutionInput::LocalPlan(Box::new(plan)))
        .await
        .expect("dry-run execution must not return Err");

    assert!(result.is_dry_run, "result must be flagged as a dry run");
    // If PanicPlugin::run had been called, the test would already have panicked.
    assert!(result.success, "dry-run must complete with success=true");
}

// ---------------------------------------------------------------------------
// Test 3: LocalPlan with TechnicalReviewPlugin against mock OpenAI
// ---------------------------------------------------------------------------

/// Exercises `TechnicalReviewPlugin` end-to-end against a wiremock server
/// that returns an empty findings response.
///
/// The wiremock server stubs POST `/v1/chat/completions` with a well-formed
/// OpenAI response containing an empty `findings` array.
///
/// Asserts:
/// - `result.success == true`
/// - `result.errors` is empty
/// - `result.scan_artifact_path.is_some()`
#[tokio::test]
async fn test_local_technical_review_with_mock_openai() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "{\"findings\":[]}"
                },
                "finish_reason": "stop"
            }]
        })))
        .mount(&server)
        .await;

    // SAFETY: TempDir::new only fails on OS-level failure in a test environment.
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");
    // SAFETY: repo_dir is under a fresh TempDir; directory creation cannot fail.
    fs::create_dir_all(&repo_dir).unwrap();
    fs::write(
        repo_dir.join("main.rs"),
        "fn main() { println!(\"hello\"); }\n",
    )
    .unwrap();
    let ws_dir = temp.path().join("workspaces");
    fs::create_dir_all(&ws_dir).unwrap();

    let mut config = make_test_config(ws_dir.to_str().expect("workspace dir must be valid UTF-8"));
    config.openai.endpoint = format!("{}/v1", server.uri());
    config.openai.allow_insecure_endpoint = true;
    config.openai.api_key_env = "XZARDGZ_IT_WF_KEY_TECH".to_string();

    // SAFETY: XZARDGZ_IT_WF_KEY_TECH is unique to this test; no other test
    // reads or writes this environment variable concurrently.
    unsafe { std::env::set_var("XZARDGZ_IT_WF_KEY_TECH", "test-key-tech") };

    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(TechnicalReviewPlugin));
    let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(registry));

    let plan = make_plan(
        "technical-review",
        &repo_dir.to_string_lossy(),
        &ws_dir.to_string_lossy(),
        false,
    );

    let result = executor
        .execute(ExecutionInput::LocalPlan(Box::new(plan)))
        .await
        .expect("technical-review plan execution must succeed");

    // SAFETY: same invariant as set_var above.
    unsafe { std::env::remove_var("XZARDGZ_IT_WF_KEY_TECH") };

    assert!(
        result.success,
        "technical-review execution must succeed; errors: {:?}",
        result.errors
    );
    assert!(
        result.errors.is_empty(),
        "no errors expected from technical-review run; got: {:?}",
        result.errors
    );
    assert!(
        result.scan_artifact_path.is_some(),
        "scan artifact must be produced by the technical-review run"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Unknown plugin records an error without panicking
// ---------------------------------------------------------------------------

/// Verifies that referencing an unregistered plugin produces a structured
/// error in `result.errors` without panicking or returning `Err`.
///
/// The executor must:
/// - return `Ok(ExecutionResult)` (not `Err`)
/// - set `result.success = false`
/// - include an error message that references the missing plugin name
#[tokio::test]
async fn test_local_plan_with_unknown_plugin_records_error() {
    // SAFETY: TempDir::new only fails on OS-level failure in a test environment.
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");
    // SAFETY: repo_dir is under a fresh TempDir; directory creation cannot fail.
    fs::create_dir_all(&repo_dir).unwrap();
    fs::write(repo_dir.join("main.rs"), "fn main() {}\n").unwrap();
    let ws_dir = temp.path().join("workspaces");
    fs::create_dir_all(&ws_dir).unwrap();

    let config = make_test_config(ws_dir.to_str().expect("workspace dir must be valid UTF-8"));
    // Empty registry -- "nonexistent-plugin" is not registered.
    let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(PluginRegistry::new()));

    let plan = make_plan(
        "nonexistent-plugin",
        &repo_dir.to_string_lossy(),
        &ws_dir.to_string_lossy(),
        false,
    );

    let result = executor
        .execute(ExecutionInput::LocalPlan(Box::new(plan)))
        .await
        .expect("execution must return Ok even when the plugin is not found");

    assert!(
        !result.success,
        "plan with an unknown plugin must not be marked successful"
    );
    assert!(
        !result.errors.is_empty(),
        "errors must capture the missing-plugin message"
    );

    let error_text = result.errors.join(" ");
    assert!(
        error_text.contains("nonexistent"),
        "error message must reference the unknown plugin name; got: {:?}",
        result.errors
    );
}

// ---------------------------------------------------------------------------
// Test 5: Custom SuccessPlugin executes without calling the provider
// ---------------------------------------------------------------------------

/// Verifies that a custom plugin that never invokes the AI provider can
/// complete successfully within the full executor pipeline.
///
/// A dummy API key env var is set so that provider construction at Stage 11
/// succeeds; `do_complete` is never called because `SuccessPlugin::run`
/// returns immediately.
///
/// Asserts:
/// - `result.success == true`
#[tokio::test]
async fn test_direct_success_plugin_execution() {
    /// Minimal plugin that returns a successful output without calling the
    /// AI provider.
    struct SuccessPlugin;

    #[async_trait]
    impl WorkflowPlugin for SuccessPlugin {
        fn name(&self) -> &str {
            "success-plugin"
        }

        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(
                "success-plugin",
                "1.0.0",
                "Test plugin that always succeeds without provider calls.",
            )
        }

        fn supported_formats(&self) -> Vec<String> {
            vec!["json".to_string()]
        }

        fn required_tool_access(&self) -> ToolAccessLevel {
            ToolAccessLevel::None
        }

        async fn run(&self, _ctx: PluginContext) -> Result<PluginOutput> {
            Ok(PluginOutput::success("success-plugin completed"))
        }
    }

    // SAFETY: TempDir::new only fails on OS-level failure in a test environment.
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");
    // SAFETY: repo_dir is under a fresh TempDir; directory creation cannot fail.
    fs::create_dir_all(&repo_dir).unwrap();
    fs::write(repo_dir.join("main.rs"), "fn main() {}\n").unwrap();
    let ws_dir = temp.path().join("workspaces");
    fs::create_dir_all(&ws_dir).unwrap();

    let mut config = make_test_config(ws_dir.to_str().expect("workspace dir must be valid UTF-8"));
    // The executor constructs a provider at Stage 11 before running plugins.
    // Setting a dummy key ensures the provider factory succeeds even though
    // SuccessPlugin never calls do_complete.
    config.openai.api_key_env = "XZARDGZ_IT_WF_KEY_SUCCESS".to_string();

    // SAFETY: XZARDGZ_IT_WF_KEY_SUCCESS is unique to this test; no other test
    // reads or writes this environment variable concurrently.
    unsafe { std::env::set_var("XZARDGZ_IT_WF_KEY_SUCCESS", "fake-key-success") };

    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(SuccessPlugin));
    let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(registry));

    let plan = make_plan(
        "success-plugin",
        &repo_dir.to_string_lossy(),
        &ws_dir.to_string_lossy(),
        false,
    );

    let result = executor
        .execute(ExecutionInput::LocalPlan(Box::new(plan)))
        .await
        .expect("success-plugin plan execution must succeed");

    // SAFETY: same invariant as set_var above.
    unsafe { std::env::remove_var("XZARDGZ_IT_WF_KEY_SUCCESS") };

    assert!(
        result.success,
        "success-plugin execution must succeed; errors: {:?}",
        result.errors
    );
}
