//! Phase 19 integration tests: SARIF report generation and format validation.
//!
//! Tests in this module exercise the SARIF output path of
//! `SecurityReviewPlugin` through a full `WorkflowExecutor` run backed by a
//! wiremock OpenAI server.  Three scenarios are covered:
//!
//! 1. A finding-bearing response produces a valid SARIF 2.1.0 file on disk.
//! 2. A critical-severity finding is mapped to `"level": "error"` in SARIF.
//! 3. When SARIF output is disabled no `.sarif.json` path appears in the
//!    execution result.

use std::collections::HashMap;
use std::fs;
use std::sync::Arc;

use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use xzardgz::config::Config;
use xzardgz::plugins::registry::PluginRegistry;
use xzardgz::plugins::security_review::SecurityReviewPlugin;
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
/// formats.  Callers override additional fields as needed for each test.
fn make_test_config(workspace_root: &str) -> Config {
    let mut config = Config::default();
    config.workspace.root = workspace_root.to_string();
    config.reports.formats = vec!["json".to_string()];
    config.governance.enabled = false;
    config.governance.rules_path = String::new();
    config
}

/// Builds a single-step `WorkflowPlan` that runs `security-review` with
/// markdown, JSON, and SARIF output formats.
///
/// # Arguments
///
/// * `repo_path` - Absolute path to the repository directory.
/// * `workspace_dir` - Absolute path to the workspace directory.
///
/// # Returns
///
/// A `WorkflowPlan` whose single step requests all three output formats so
/// that the executor's `write_step_reports` generates a SARIF file.
fn make_security_plan(repo_path: &str, workspace_dir: &str) -> WorkflowPlan {
    WorkflowPlan {
        version: PLAN_VERSION.to_string(),
        name: "sarif-integration-test-plan".to_string(),
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
            plugin: "security-review".to_string(),
            config: None,
            dependencies: vec![],
            report_formats: Some(vec![
                "markdown".to_string(),
                "json".to_string(),
                "sarif".to_string(),
            ]),
            max_findings: None,
            severity_threshold: None,
        }],
        reports: None,
        dry_run: false,
        resume: false,
        correlation_id: None,
    }
}

/// Returns the mock OpenAI response containing a single high-severity
/// security finding.
///
/// The response is formatted as an OpenAI chat-completions reply with the
/// assistant content containing a `{"findings":[...]}` JSON object.
fn high_severity_finding_response() -> serde_json::Value {
    json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "{\"findings\":[{\"category\":\"secrets\",\"severity\":\"high\",\"file\":\"src/main.rs\",\"line\":10,\"symbol\":null,\"evidence\":\"Possible credential reference\",\"exploitability\":\"medium\",\"impact\":\"data exposure\",\"remediation\":\"use env vars\",\"confidence\":0.9,\"cwe\":\"CWE-798\",\"owasp\":\"A07:2021\",\"false_positive_notes\":null,\"sarif_help_uri\":null}]}"
            },
            "finish_reason": "stop"
        }]
    })
}

/// Returns the mock OpenAI response containing a single critical-severity
/// security finding.
///
/// A critical finding must be mapped to `"level": "error"` in the SARIF
/// output per the SARIF 2.1.0 severity mapping rules.
fn critical_severity_finding_response() -> serde_json::Value {
    json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "{\"findings\":[{\"category\":\"secrets\",\"severity\":\"critical\",\"file\":\"src/main.rs\",\"line\":10,\"symbol\":null,\"evidence\":\"Possible credential reference\",\"exploitability\":\"high\",\"impact\":\"data exposure\",\"remediation\":\"use env vars\",\"confidence\":0.9,\"cwe\":\"CWE-798\",\"owasp\":\"A07:2021\",\"false_positive_notes\":null,\"sarif_help_uri\":null}]}"
            },
            "finish_reason": "stop"
        }]
    })
}

/// Returns the first path in `report_paths` that ends with `.sarif.json`, or
/// `None` if no such path exists.
///
/// # Arguments
///
/// * `report_paths` - The `report_paths` map from an `ExecutionResult`.
///
/// # Returns
///
/// `Some(path_string)` if a SARIF file is present; `None` otherwise.
fn find_sarif_path(report_paths: &HashMap<String, Vec<String>>) -> Option<String> {
    report_paths
        .values()
        .flat_map(|paths| paths.iter())
        .find(|p| p.ends_with(".sarif.json"))
        .cloned()
}

// ---------------------------------------------------------------------------
// Test 1: SecurityReviewPlugin generates a SARIF file
// ---------------------------------------------------------------------------

/// Verifies that `SecurityReviewPlugin` produces a valid SARIF 2.1.0 file
/// when the AI returns a finding-bearing response.
///
/// The test checks that:
/// - `result.report_paths` contains a path ending in `.sarif.json`
/// - The file exists on disk
/// - The file is valid JSON
/// - The top-level `"version"` field equals `"2.1.0"`
/// - The top-level `"runs"` field is a JSON array
#[tokio::test]
async fn test_security_review_generates_sarif_file() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(high_severity_finding_response()))
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
    config.openai.api_key_env = "XZARDGZ_IT_SARIF_KEY_GEN".to_string();
    config.security_review.include_sarif = true;
    config.security_review.report_formats = vec![
        "markdown".to_string(),
        "json".to_string(),
        "sarif".to_string(),
    ];
    config.security_review.fail_on_critical = false;

    // SAFETY: XZARDGZ_IT_SARIF_KEY_GEN is unique to this test; no other test
    // reads or writes this environment variable concurrently.
    unsafe { std::env::set_var("XZARDGZ_IT_SARIF_KEY_GEN", "test-sarif-key-gen") };

    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(SecurityReviewPlugin));
    let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(registry));

    let plan = make_security_plan(&repo_dir.to_string_lossy(), &ws_dir.to_string_lossy());

    let result = executor
        .execute(ExecutionInput::LocalPlan(Box::new(plan)))
        .await
        .expect("security-review execution must succeed");

    // SAFETY: same invariant as set_var above.
    unsafe { std::env::remove_var("XZARDGZ_IT_SARIF_KEY_GEN") };

    let sarif_path = find_sarif_path(&result.report_paths)
        .expect("result.report_paths must contain a .sarif.json path");

    assert!(
        std::path::Path::new(&sarif_path).exists(),
        "SARIF file must exist on disk at: {}",
        sarif_path
    );

    let sarif_content =
        fs::read_to_string(&sarif_path).expect("SARIF file must be readable as UTF-8");
    let sarif: serde_json::Value =
        serde_json::from_str(&sarif_content).expect("SARIF file must be valid JSON");

    assert_eq!(
        sarif["version"], "2.1.0",
        "SARIF version field must be '2.1.0'"
    );
    assert!(
        sarif["runs"].is_array(),
        "SARIF 'runs' field must be a JSON array"
    );
}

// ---------------------------------------------------------------------------
// Test 2: SARIF severity mapping -- critical finding maps to "error"
// ---------------------------------------------------------------------------

/// Verifies that a critical-severity finding is mapped to `"level": "error"`
/// in the SARIF 2.1.0 output.
///
/// SARIF severity mapping applied by `SarifReportWriter`:
///
/// | Plugin severity   | SARIF level |
/// |-------------------|-------------|
/// | `critical`        | `"error"`   |
/// | `high`            | `"error"`   |
/// | `medium`          | `"warning"` |
/// | `low` / `info`    | `"note"`    |
///
/// The test asserts that at least one SARIF result carries `"level": "error"`.
#[tokio::test]
async fn test_security_review_sarif_severity_mapping() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(critical_severity_finding_response()),
        )
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
    config.openai.api_key_env = "XZARDGZ_IT_SARIF_KEY_SEV".to_string();
    config.security_review.include_sarif = true;
    config.security_review.report_formats = vec![
        "markdown".to_string(),
        "json".to_string(),
        "sarif".to_string(),
    ];
    // Disable fail_on_critical so the executor marks the run as successful
    // even though a critical finding is present.
    config.security_review.fail_on_critical = false;
    // Lower the threshold to "info" to ensure the critical finding is not
    // filtered out during severity thresholding.
    config.security_review.severity_threshold = "info".to_string();

    // SAFETY: XZARDGZ_IT_SARIF_KEY_SEV is unique to this test; no other test
    // reads or writes this environment variable concurrently.
    unsafe { std::env::set_var("XZARDGZ_IT_SARIF_KEY_SEV", "test-sarif-key-sev") };

    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(SecurityReviewPlugin));
    let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(registry));

    let plan = make_security_plan(&repo_dir.to_string_lossy(), &ws_dir.to_string_lossy());

    let result = executor
        .execute(ExecutionInput::LocalPlan(Box::new(plan)))
        .await
        .expect("security-review execution must succeed");

    // SAFETY: same invariant as set_var above.
    unsafe { std::env::remove_var("XZARDGZ_IT_SARIF_KEY_SEV") };

    let sarif_path = find_sarif_path(&result.report_paths)
        .expect("result.report_paths must contain a .sarif.json path");

    let sarif_content =
        fs::read_to_string(&sarif_path).expect("SARIF file must be readable as UTF-8");
    let sarif: serde_json::Value =
        serde_json::from_str(&sarif_content).expect("SARIF file must be valid JSON");

    let runs = sarif["runs"]
        .as_array()
        .expect("SARIF 'runs' must be a JSON array");

    // Collect every "level" value across all runs and all results.
    let mut has_error_level = false;
    for run in runs {
        if let Some(results) = run["results"].as_array() {
            for result_entry in results {
                if result_entry["level"].as_str() == Some("error") {
                    has_error_level = true;
                    break;
                }
            }
        }
        if has_error_level {
            break;
        }
    }

    assert!(
        has_error_level,
        "critical finding must produce at least one SARIF result with level 'error'; \
         SARIF content: {}",
        sarif_content
    );
}

// ---------------------------------------------------------------------------
// Test 3: Without SARIF config, no .sarif.json appears in report_paths
// ---------------------------------------------------------------------------

/// Verifies that when SARIF output is disabled (both via config and step
/// formats) no `.sarif.json` path appears in `result.report_paths`.
///
/// The security review config is set to generate only markdown and JSON, and
/// the plan step explicitly requests only those two formats.
#[tokio::test]
async fn test_security_review_without_sarif_does_not_create_sarif_file() {
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
    fs::write(repo_dir.join("main.rs"), "fn main() {}\n").unwrap();
    let ws_dir = temp.path().join("workspaces");
    fs::create_dir_all(&ws_dir).unwrap();

    let mut config = make_test_config(ws_dir.to_str().expect("workspace dir must be valid UTF-8"));
    config.openai.endpoint = format!("{}/v1", server.uri());
    config.openai.allow_insecure_endpoint = true;
    config.openai.api_key_env = "XZARDGZ_IT_SARIF_KEY_NOSARIF".to_string();
    // Disable SARIF at both config and format level.
    config.security_review.include_sarif = false;
    config.security_review.report_formats = vec!["markdown".to_string(), "json".to_string()];

    // SAFETY: XZARDGZ_IT_SARIF_KEY_NOSARIF is unique to this test; no other
    // test reads or writes this environment variable concurrently.
    unsafe { std::env::set_var("XZARDGZ_IT_SARIF_KEY_NOSARIF", "test-sarif-key-no") };

    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(SecurityReviewPlugin));
    let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(registry));

    // Build the plan with only markdown and JSON in the step's report_formats
    // so the executor's write_step_reports also skips SARIF generation.
    let plan = WorkflowPlan {
        version: PLAN_VERSION.to_string(),
        name: "sarif-disabled-test-plan".to_string(),
        description: None,
        repository: repo_dir.to_string_lossy().to_string(),
        branch: None,
        workspace: Some(ws_dir.to_string_lossy().to_string()),
        provider: None,
        model: None,
        scan: None,
        steps: vec![PluginStep {
            id: "step1".to_string(),
            description: None,
            plugin: "security-review".to_string(),
            config: None,
            dependencies: vec![],
            report_formats: Some(vec!["markdown".to_string(), "json".to_string()]),
            max_findings: None,
            severity_threshold: None,
        }],
        reports: None,
        dry_run: false,
        resume: false,
        correlation_id: None,
    };

    let result = executor
        .execute(ExecutionInput::LocalPlan(Box::new(plan)))
        .await
        .expect("security-review execution must succeed");

    // SAFETY: same invariant as set_var above.
    unsafe { std::env::remove_var("XZARDGZ_IT_SARIF_KEY_NOSARIF") };

    let sarif_path = find_sarif_path(&result.report_paths);
    assert!(
        sarif_path.is_none(),
        "no .sarif.json must appear in report_paths when SARIF is disabled; \
         report_paths: {:?}",
        result.report_paths
    );
}
