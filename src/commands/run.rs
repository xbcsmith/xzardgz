//! Workflow run command handler.
//!
//! This module implements the `run` subcommand, which executes a workflow
//! plan from a plan file or constructs a direct plugin invocation plan from
//! CLI arguments, then runs it through [`WorkflowExecutor`].

use std::path::Path;
use std::sync::Arc;

use crate::cli::RunArgs;
use crate::config::{Config, ConfigOverrides};
use crate::error::{PipelineError, Result};
use crate::plugins::registry::PluginRegistry;
use crate::workflow::executor::{ExecutionInput, ExecutionResult, WorkflowExecutor};
use crate::workflow::parser::parse_plan;
use crate::workflow::validator::{
    apply_run_overrides, build_direct_invocation_plan, validate_plan,
};

/// Executes a workflow run from either a plan file or a direct plugin
/// invocation.
///
/// Loads configuration via [`Config::load`] and dispatches to
/// [`execute_with`] using the built-in plugin registry
/// ([`PluginRegistry::with_builtins`]). See [`execute_with`] for the full
/// execution logic and for a config/registry-injectable variant suitable for
/// tests and programmatic embedding.
///
/// # Arguments
///
/// * `args` - Parsed CLI arguments from the `run` subcommand.
///
/// # Errors
///
/// Returns [`PipelineError::Config`] if configuration loading fails.
/// See [`execute_with`] for the remaining error conditions.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::cli::RunArgs;
/// use xzardgz::commands::run::execute;
///
/// let args = RunArgs {
///     plan: None,
///     repository: Some(".".to_string()),
///     branch: None,
///     plugin: Some("technical-review".to_string()),
///     provider: None,
///     model: None,
///     dry_run: true,
///     workspace: None,
///     output_dir: None,
///     openai_endpoint: None,
///     ollama_host: None,
///     insecure: false,
///     scan_artifact: None,
///     trace_transcript: false,
///     max_findings: None,
///     report_format: vec![],
///     resume: false,
///     correlation_id: None,
/// };
/// // Call from an async context: let result = execute(args).await;
/// ```
pub async fn execute(args: RunArgs) -> Result<()> {
    let config = Config::load()?;
    execute_with(args, config, PluginRegistry::with_builtins()).await
}

/// Executes a workflow run using an explicitly supplied configuration and
/// plugin registry.
///
/// This is the full implementation behind [`execute`]. It is a separate,
/// fully public function so tests and programmatic embedders can supply a
/// pre-configured [`Config`] (for example, one pointing at a mock provider
/// endpoint) and a custom [`PluginRegistry`] (for example, one containing
/// only fake plugins) instead of depending on [`Config::load`] reading
/// `config.yaml` from the current directory and on the real built-in
/// plugins being registered.
///
/// At least one of `args.plan` or `args.plugin` must be set; supplying
/// neither returns an error immediately, before either `config` or
/// `registry` is used.
///
/// # Dispatch
///
/// | Condition | Execution path |
/// |-----------|-----------------|
/// | `args.plan` is `Some` | Parse and validate the plan file, apply CLI overrides, run as [`ExecutionInput::LocalPlan`] |
/// | `args.plugin` is `Some` and `args.scan_artifact` is `Some` | Run as [`ExecutionInput::PluginOnly`], skipping the scan stage entirely |
/// | `args.plugin` is `Some` and `args.scan_artifact` is `None` | Build a single-step plan via [`build_direct_invocation_plan`], apply CLI overrides, run as [`ExecutionInput::LocalPlan`] |
///
/// CLI overrides for provider, model, workspace, and Ollama host are applied
/// to `config` before execution. `--resume`, `--workspace`, `--output-dir`,
/// `--report-format`, and `--max-findings` are applied onto the constructed
/// or parsed plan via [`apply_run_overrides`].
///
/// # Arguments
///
/// * `args` - Parsed CLI arguments from the `run` subcommand.
/// * `config` - The configuration to execute against.
/// * `registry` - The plugin registry to resolve `args.plugin` (or a plan
///   step's `plugin` field) against.
///
/// # Errors
///
/// Returns [`PipelineError::Workflow`] if neither `--plan` nor `--plugin` is
/// provided, if the plan file cannot be parsed, or if plan validation fails.
/// Returns [`PipelineError::Io`] if the plan file cannot be read from disk.
/// Returns an error if the underlying [`WorkflowExecutor::execute`] call
/// fails, or propagates a summarized error when it returns a result with
/// `success: false` (for example, a plugin step that did not complete).
///
/// # Examples
///
/// ```no_run
/// use xzardgz::cli::RunArgs;
/// use xzardgz::commands::run::execute_with;
/// use xzardgz::config::Config;
/// use xzardgz::plugins::registry::PluginRegistry;
///
/// # async fn run() -> xzardgz::error::Result<()> {
/// let args = RunArgs {
///     plan: None,
///     repository: Some(".".to_string()),
///     branch: None,
///     plugin: Some("technical-review".to_string()),
///     provider: None,
///     model: None,
///     dry_run: true,
///     workspace: None,
///     output_dir: None,
///     openai_endpoint: None,
///     ollama_host: None,
///     insecure: false,
///     scan_artifact: None,
///     trace_transcript: false,
///     max_findings: None,
///     report_format: vec![],
///     resume: false,
///     correlation_id: None,
/// };
/// execute_with(args, Config::default(), PluginRegistry::with_builtins()).await?;
/// # Ok(())
/// # }
/// ```
pub async fn execute_with(
    args: RunArgs,
    mut config: Config,
    registry: PluginRegistry,
) -> Result<()> {
    if args.plan.is_none() && args.plugin.is_none() {
        return Err(PipelineError::Workflow(
            "run requires either --plan <file> or --plugin <name>".to_string(),
        ));
    }

    let overrides = ConfigOverrides {
        provider: args.provider.clone(),
        openai_model: args.model.clone(),
        ollama_model: None,
        ollama_host: args.ollama_host.clone(),
        workspace_root: args.workspace.clone(),
    };
    config.apply_overrides(&overrides);

    let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(registry));

    let result = if let Some(ref path) = args.plan {
        let content = std::fs::read_to_string(path).map_err(PipelineError::Io)?;
        let extension = Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("yaml");

        let mut plan = parse_plan(&content, extension)?;
        validate_plan(&plan)?;
        // Apply caller-supplied correlation_id before creating the workspace.
        plan.correlation_id = args.correlation_id.clone();
        apply_run_overrides(
            &mut plan,
            args.branch.clone(),
            args.workspace.clone(),
            args.dry_run,
            args.resume,
            args.max_findings,
            args.report_format.clone(),
            args.output_dir.clone(),
        );

        executor
            .execute(ExecutionInput::LocalPlan(Box::new(plan)))
            .await?
    } else {
        // Checked by the guard above: at least one of `plan`/`plugin` is
        // `Some`, and this branch only runs when `plan` is `None`.
        let plugin = args
            .plugin
            .clone()
            .expect("args.plugin is Some: checked by the guard above");

        if let Some(ref scan_artifact) = args.scan_artifact {
            // A scan artifact was supplied: skip the scan stage entirely
            // rather than paying for (and requiring) a fresh repository
            // scan the caller has already told us is unnecessary.
            executor
                .execute(ExecutionInput::PluginOnly {
                    plugin,
                    workspace_dir: None,
                    scan_artifact_path: Some(scan_artifact.clone()),
                    workspace_root: args.workspace.clone(),
                    config: None,
                    provider: args.provider.clone(),
                    model: args.model.clone(),
                    dry_run: args.dry_run,
                    report_formats: args.report_format.clone(),
                    output_dir: args.output_dir.clone(),
                    correlation_id: args.correlation_id.clone(),
                })
                .await?
        } else {
            let repository = args.repository.clone().unwrap_or_else(|| ".".to_string());
            let mut plan = build_direct_invocation_plan(
                &plugin,
                &repository,
                args.branch.clone(),
                args.provider.clone(),
                args.model.clone(),
                args.workspace.clone(),
                args.dry_run,
                args.max_findings,
                args.report_format.clone(),
            );
            validate_plan(&plan)?;
            // Apply caller-supplied correlation_id before creating the workspace.
            plan.correlation_id = args.correlation_id.clone();
            // `build_direct_invocation_plan` already applied branch,
            // workspace, dry_run, max_findings, and report_format; only
            // resume and output_dir remain to be applied here.
            apply_run_overrides(
                &mut plan,
                None,
                None,
                false,
                args.resume,
                None,
                vec![],
                args.output_dir.clone(),
            );

            executor
                .execute(ExecutionInput::LocalPlan(Box::new(plan)))
                .await?
        }
    };

    print_execution_result(&result);

    if !result.success {
        return Err(PipelineError::Workflow(if result.errors.is_empty() {
            "run did not complete successfully".to_string()
        } else {
            format!("run failed: {}", result.errors.join("; "))
        }));
    }

    Ok(())
}

/// Prints a human-readable summary of an [`ExecutionResult`] to stdout.
///
/// Shared by every command handler that ultimately calls
/// [`WorkflowExecutor::execute`] (`run`, `scan`, `plugin run`), so the CLI's
/// output shape stays consistent across all three.
///
/// # Arguments
///
/// * `result` - The execution result to summarize.
pub(crate) fn print_execution_result(result: &ExecutionResult) {
    println!("Workspace: {}", result.workspace_id);
    println!("Correlation ID: {}", result.correlation_id);
    if result.is_dry_run {
        println!("Dry run: validation only, no side effects performed.");
    }
    if let Some(ref path) = result.scan_artifact_path {
        println!("Scan artifact: {}", path);
    }
    let mut step_ids: Vec<&String> = result.report_paths.keys().collect();
    step_ids.sort();
    for step_id in step_ids {
        for path in &result.report_paths[step_id] {
            println!("Report ({}): {}", step_id, path);
        }
    }
    for err in &result.errors {
        eprintln!("Error: {}", err);
    }
    println!("Success: {}", result.success);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use tempfile::TempDir;

    use crate::cli::RunArgs;
    use crate::plugins::context::{PluginContext, ToolAccessLevel};
    use crate::plugins::output::PluginOutput;
    use crate::plugins::trait_def::{PluginMetadata, WorkflowPlugin};

    /// Minimal plugin that always returns success without invoking a
    /// provider, so dispatch-logic tests never require network access.
    struct SuccessPlugin;

    #[async_trait]
    impl WorkflowPlugin for SuccessPlugin {
        fn name(&self) -> &str {
            "test-plugin"
        }
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new("test-plugin", "1.0.0", "Test plugin.")
        }
        fn supported_formats(&self) -> Vec<String> {
            vec!["json".to_string()]
        }
        fn required_tool_access(&self) -> ToolAccessLevel {
            ToolAccessLevel::None
        }
        async fn run(&self, _ctx: PluginContext) -> crate::error::Result<PluginOutput> {
            Ok(PluginOutput::success("test plugin succeeded"))
        }
    }

    /// Returns a [`RunArgs`] with all optional fields set to `None` / defaults.
    fn make_empty_run_args() -> RunArgs {
        RunArgs {
            plan: None,
            repository: Some(".".to_string()),
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

    /// Builds a `Config` with governance disabled and a JSON report format,
    /// rooted at `workspace_root`, safe for tests that never invoke a
    /// provider (plugins registered are all fakes).
    fn make_test_config(workspace_root: &str) -> Config {
        let mut config = Config::default();
        config.workspace.root = workspace_root.to_string();
        config.reports.formats = vec!["json".to_string()];
        config.governance.enabled = false;
        config.governance.rules_path = String::new();
        config
    }

    fn registry_with_success_plugin() -> PluginRegistry {
        let mut registry = PluginRegistry::new();
        registry.register(Arc::new(SuccessPlugin));
        registry
    }

    #[tokio::test]
    async fn test_execute_with_rejects_when_neither_plan_nor_plugin_given() {
        let args = make_empty_run_args();
        let result = execute_with(args, Config::default(), PluginRegistry::new()).await;
        assert!(
            result.is_err(),
            "expected error when neither plan nor plugin is given"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("run requires either --plan"),
            "expected missing-plan-and-plugin error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn test_execute_with_direct_plugin_invocation_produces_real_workspace_and_report() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let mut args = make_empty_run_args();
        args.plugin = Some("test-plugin".to_string());
        args.repository = Some(repo_dir.path().to_str().unwrap().to_string());
        args.workspace = Some(ws_dir.path().to_str().unwrap().to_string());

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let result = execute_with(args, config, registry_with_success_plugin()).await;
        assert!(
            result.is_ok(),
            "direct plugin invocation should succeed, got: {:?}",
            result.err()
        );

        // A real workspace directory was created under ws_dir.
        let entries: Vec<_> = std::fs::read_dir(ws_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(
            !entries.is_empty(),
            "expected a workspace directory to be created under {:?}",
            ws_dir.path()
        );

        // A real report file was written somewhere under the workspace.
        let has_report_file = walk_has_extension(ws_dir.path(), "json");
        assert!(
            has_report_file,
            "expected at least one .json report file under {:?}",
            ws_dir.path()
        );
    }

    #[tokio::test]
    async fn test_execute_with_plan_file_mode_produces_real_report() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();
        let plan_dir = TempDir::new().unwrap();

        let plan_yaml = format!(
            "version: \"1\"\nname: file-plan\nrepository: \"{}\"\nworkspace: \"{}\"\nsteps:\n  - id: step1\n    plugin: test-plugin\n    report_formats: [json]\n",
            repo_dir.path().to_string_lossy().replace('\\', "\\\\"),
            ws_dir.path().to_string_lossy().replace('\\', "\\\\"),
        );
        let plan_path = plan_dir.path().join("plan.yaml");
        std::fs::write(&plan_path, plan_yaml).unwrap();

        let mut args = make_empty_run_args();
        args.plan = Some(plan_path.to_str().unwrap().to_string());
        args.repository = None;

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let result = execute_with(args, config, registry_with_success_plugin()).await;
        assert!(
            result.is_ok(),
            "plan-file run should succeed, got: {:?}",
            result.err()
        );
        assert!(walk_has_extension(ws_dir.path(), "json"));
    }

    #[tokio::test]
    async fn test_execute_with_scan_artifact_routes_to_plugin_only() {
        // Produce a real scan artifact first via a real LocalPlan run.
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let mut first_args = make_empty_run_args();
        first_args.plugin = Some("test-plugin".to_string());
        first_args.repository = Some(repo_dir.path().to_str().unwrap().to_string());
        first_args.workspace = Some(ws_dir.path().to_str().unwrap().to_string());
        let config = make_test_config(ws_dir.path().to_str().unwrap());
        execute_with(first_args, config, registry_with_success_plugin())
            .await
            .expect("initial run must succeed to produce a scan artifact");

        let artifact_path = find_file_named(ws_dir.path(), "artifact.yaml")
            .expect("a scan artifact YAML file must exist under the workspace");

        let plugin_ws_dir = TempDir::new().unwrap();
        let mut second_args = make_empty_run_args();
        second_args.plugin = Some("test-plugin".to_string());
        second_args.scan_artifact = Some(artifact_path.to_string_lossy().to_string());
        second_args.workspace = Some(plugin_ws_dir.path().to_str().unwrap().to_string());

        let second_config = make_test_config(plugin_ws_dir.path().to_str().unwrap());
        let result = execute_with(second_args, second_config, registry_with_success_plugin()).await;
        assert!(
            result.is_ok(),
            "scan-artifact plugin-only run should succeed, got: {:?}",
            result.err()
        );
        assert!(walk_has_extension(plugin_ws_dir.path(), "json"));
    }

    #[tokio::test]
    async fn test_execute_with_dry_run_creates_no_report() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let mut args = make_empty_run_args();
        args.plugin = Some("test-plugin".to_string());
        args.repository = Some(repo_dir.path().to_str().unwrap().to_string());
        args.workspace = Some(ws_dir.path().to_str().unwrap().to_string());
        args.dry_run = true;

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let result = execute_with(args, config, registry_with_success_plugin()).await;
        assert!(
            result.is_ok(),
            "dry run should succeed, got: {:?}",
            result.err()
        );
        assert!(
            !walk_has_extension(ws_dir.path(), "json"),
            "dry run must not write any report file"
        );
    }

    #[tokio::test]
    async fn test_execute_with_unregistered_plugin_returns_actionable_error() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let mut args = make_empty_run_args();
        args.plugin = Some("does-not-exist".to_string());
        args.repository = Some(repo_dir.path().to_str().unwrap().to_string());
        args.workspace = Some(ws_dir.path().to_str().unwrap().to_string());

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let result = execute_with(args, config, PluginRegistry::new()).await;
        assert!(
            result.is_err(),
            "expected an error for an unregistered plugin"
        );
    }

    /// Recursively checks whether any file under `root` has the given
    /// extension (without the leading dot).
    fn walk_has_extension(root: &std::path::Path, ext: &str) -> bool {
        find_file_with_extension(root, ext).is_some()
    }

    /// Recursively finds the first file under `root` with the given
    /// extension (without the leading dot).
    fn find_file_with_extension(root: &std::path::Path, ext: &str) -> Option<std::path::PathBuf> {
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Recursively finds the first file under `root` with the exact given
    /// file name.
    fn find_file_named(root: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
                    return Some(path);
                }
            }
        }
        None
    }
}
