//! Workflow run command handler.
//!
//! This module implements the `run` subcommand, which executes a workflow plan
//! from a plan file or constructs a direct plugin invocation plan from CLI
//! arguments.

use crate::cli::RunArgs;
use crate::config::{Config, ConfigOverrides};
use crate::error::{PipelineError, Result};
use crate::workflow::parser::parse_plan;
use crate::workflow::validator::{build_direct_invocation_plan, validate_plan};
use std::path::Path;

/// Executes a workflow run from either a plan file or a direct plugin
/// invocation.
///
/// When `args.plan` is provided, the plan file is read, parsed, and validated
/// before printing a summary of what would be executed. When `args.plugin` is
/// provided instead, a single-step [`crate::workflow::plan::WorkflowPlan`] is
/// constructed programmatically and validated.
///
/// At least one of `args.plan` or `args.plugin` must be set; supplying neither
/// returns an error immediately.
///
/// CLI overrides for provider, model, workspace, and Ollama host are applied
/// to the loaded configuration before the plan is processed.
///
/// # Arguments
///
/// * `args` - Parsed CLI arguments from the `run` subcommand.
///
/// # Errors
///
/// Returns [`PipelineError::Workflow`] if neither `--plan` nor `--plugin` is
/// provided, if the plan file cannot be parsed, or if plan validation fails.
/// Returns [`PipelineError::Config`] if configuration loading fails.
/// Returns [`PipelineError::Io`] if the plan file cannot be read from disk.
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
///     dry_run: false,
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
/// };
/// // Call from an async context: let result = execute(args).await;
/// ```
pub async fn execute(args: RunArgs) -> Result<()> {
    if args.plan.is_none() && args.plugin.is_none() {
        return Err(PipelineError::Workflow(
            "run requires either --plan <file> or --plugin <name>".to_string(),
        ));
    }

    let mut config = Config::load()?;

    let overrides = ConfigOverrides {
        provider: args.provider.clone(),
        openai_model: args.model.clone(),
        ollama_model: None,
        ollama_host: args.ollama_host.clone(),
        workspace_root: args.workspace.clone(),
    };
    config.apply_overrides(&overrides);

    if let Some(ref path) = args.plan {
        let content = std::fs::read_to_string(path).map_err(crate::error::PipelineError::Io)?;

        let extension = Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("yaml");

        let plan = parse_plan(&content, extension)?;
        validate_plan(&plan)?;

        if args.dry_run || plan.is_dry_run() {
            println!("Dry run mode: no providers will be invoked.");
        }

        println!("Plan: {}", plan.name);
        println!("Repository: {}", plan.repository);
        println!("Steps: {}", plan.steps.len());
        println!("Plan execution is implemented in a later phase.");
    } else if let Some(ref plugin) = args.plugin {
        let repository = args.repository.as_deref().unwrap_or(".");

        let plan = build_direct_invocation_plan(
            plugin,
            repository,
            args.branch.clone(),
            args.provider.clone(),
            args.model.clone(),
            args.workspace.clone(),
            args.dry_run,
            args.max_findings,
            args.report_format.clone(),
        );

        validate_plan(&plan)?;

        if args.dry_run || plan.is_dry_run() {
            println!("Dry run mode: no providers will be invoked.");
        }

        println!("Plugin: {}", plugin);
        println!("Repository: {}", plan.repository);
        println!("Plan execution is implemented in a later phase.");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::RunArgs;

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
        }
    }

    #[tokio::test]
    async fn test_execute_rejects_when_neither_plan_nor_plugin_given() {
        let args = make_empty_run_args();
        let result = execute(args).await;
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
    async fn test_execute_accepts_direct_plugin_invocation() {
        let mut args = make_empty_run_args();
        args.plugin = Some("technical-review".to_string());
        let result = execute(args).await;
        assert!(
            result.is_ok(),
            "direct plugin invocation should succeed, got: {:?}",
            result.err()
        );
    }
}
