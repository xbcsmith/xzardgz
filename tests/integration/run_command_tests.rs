//! Phase 2 integration test: `commands::run` end-to-end against a real plugin.
//!
//! This module exercises the actual `run` CLI command handler (not just
//! `WorkflowExecutor` directly, which `workflow_tests.rs` already covers) —
//! `xzardgz run --plugin technical-review <fixture-repo>` — against a
//! wiremock-stubbed OpenAI endpoint standing in for a real provider, and
//! asserts a real workspace directory and a real report file are produced on
//! disk. This is the "end-to-end CLI test... added under tests/integration/"
//! required by the CLI-to-workflow-engine integration plan's Phase 2.6
//! success criterion.

use std::fs;

use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use xzardgz::cli::RunArgs;
use xzardgz::commands::run::execute_with;
use xzardgz::config::Config;
use xzardgz::plugins::registry::PluginRegistry;

/// Returns a [`RunArgs`] with every optional field defaulted, for tests to
/// override only the fields they care about.
fn make_run_args() -> RunArgs {
    RunArgs {
        plan: None,
        repository: None,
        branch: None,
        plugin: None,
        provider: None,
        model: None,
        dry_run: false,
        workspace: None,
        output_dir: None,
        openai_endpoint: None,
        ollama_host: None,
        insecure: false,
        scan_artifact: None,
        trace_transcript: false,
        max_findings: None,
        report_format: vec![],
        resume: false,
        correlation_id: None,
    }
}

/// Recursively finds the first file under `root` with the exact given file
/// name.
fn find_file_named(root: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                stack.push(entry_path);
            } else if entry_path.file_name().and_then(|n| n.to_str()) == Some(name) {
                return Some(entry_path);
            }
        }
    }
    None
}

/// Verifies that `xzardgz run --plugin technical-review <fixture-repo>`,
/// exercised through the real `commands::run::execute_with` handler and the
/// real `PluginRegistry::with_builtins()` registry, produces a real
/// workspace directory and a real written report on disk when the
/// configured provider (OpenAI-compatible, via `--api-endpoint`-equivalent
/// config) is reachable and returns a well-formed response.
///
/// This is the CLI-level counterpart to `workflow_tests.rs`'s
/// `test_local_technical_review_with_mock_openai`, which already proves the
/// same scenario at the `WorkflowExecutor` level; this test proves the CLI
/// command handler wiring on top of that, per Phase 2.6 of
/// `docs/explanation/cli_workflow_engine_integration_plan.md`.
#[tokio::test]
async fn test_run_command_direct_plugin_invocation_produces_real_report() {
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
    fs::create_dir_all(&repo_dir).unwrap();
    fs::write(
        repo_dir.join("main.rs"),
        "fn main() { println!(\"hello\"); }\n",
    )
    .unwrap();
    let ws_dir = temp.path().join("workspaces");
    fs::create_dir_all(&ws_dir).unwrap();

    let mut config = Config::default();
    config.workspace.root = ws_dir.to_string_lossy().to_string();
    config.reports.formats = vec!["json".to_string()];
    config.governance.enabled = false;
    config.governance.rules_path = String::new();
    config.openai.endpoint = format!("{}/v1", server.uri());
    config.openai.allow_insecure_endpoint = true;
    config.openai.api_key_env = "XZARDGZ_IT_RUN_CMD_KEY".to_string();

    // SAFETY: XZARDGZ_IT_RUN_CMD_KEY is unique to this test file; no other
    // test reads or writes this environment variable concurrently.
    unsafe { std::env::set_var("XZARDGZ_IT_RUN_CMD_KEY", "test-key") };

    let mut args = make_run_args();
    args.plugin = Some("technical-review".to_string());
    args.repository = Some(repo_dir.to_string_lossy().to_string());
    args.workspace = Some(ws_dir.to_string_lossy().to_string());
    args.report_format = vec!["json".to_string()];

    let result = execute_with(args, config, PluginRegistry::with_builtins()).await;

    // SAFETY: same invariant as set_var above.
    unsafe { std::env::remove_var("XZARDGZ_IT_RUN_CMD_KEY") };

    assert!(
        result.is_ok(),
        "run command with technical-review must succeed, got: {:?}",
        result.err()
    );

    // A real workspace directory was created under ws_dir.
    let entries: Vec<_> = fs::read_dir(&ws_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(
        !entries.is_empty(),
        "expected a workspace directory to be created under {:?}",
        ws_dir
    );

    // A real scan artifact was produced.
    assert!(
        find_file_named(&ws_dir, "artifact.yaml").is_some(),
        "expected a scan artifact file under {:?}",
        ws_dir
    );

    // A real report file was written for the technical-review step.
    assert!(
        find_file_named(&ws_dir, "technical_review.json").is_some(),
        "expected a technical_review.json report file under {:?}",
        ws_dir
    );
}

/// Verifies that a `run --plugin <name>` invocation against a plugin name
/// that is not registered fails clearly (a non-`Ok` result with an
/// actionable message), rather than silently succeeding or panicking. This
/// covers the spirit of Phase 2.6's requirement that misconfigured runs
/// surface a clear, actionable CLI error rather than a silent no-op.
#[tokio::test]
async fn test_run_command_unregistered_plugin_returns_actionable_error() {
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();
    let ws_dir = temp.path().join("workspaces");
    fs::create_dir_all(&ws_dir).unwrap();

    let mut config = Config::default();
    config.workspace.root = ws_dir.to_string_lossy().to_string();
    config.governance.enabled = false;
    config.governance.rules_path = String::new();

    let mut args = make_run_args();
    args.plugin = Some("no-such-plugin".to_string());
    args.repository = Some(repo_dir.to_string_lossy().to_string());
    args.workspace = Some(ws_dir.to_string_lossy().to_string());

    let result = execute_with(args, config, PluginRegistry::with_builtins()).await;
    assert!(
        result.is_err(),
        "expected an actionable error for an unregistered plugin name"
    );
}
