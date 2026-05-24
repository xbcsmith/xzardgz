//! Plugin management command handler.
//!
//! This module implements the `plugin` family of subcommands, covering plugin
//! listing, schema introspection, direct plugin execution, configuration
//! validation, and report format discovery.

use crate::cli::{PluginCommands, PluginRunArgs};
use crate::error::{PipelineError, Result};

/// Built-in plugin names registered in the pipeline.
const BUILTIN_PLUGINS: &[&str] = &["technical-review", "security-review"];

/// Executes a plugin subcommand.
///
/// Dispatches to the appropriate handler based on the variant of `command`.
///
/// # Arguments
///
/// * `command` - The plugin subcommand to execute.
///
/// # Errors
///
/// Returns [`PipelineError::Workflow`] when `plugin run` is invoked without
/// either a `--workspace` or `--scan-artifact` argument.
pub async fn execute(command: PluginCommands) -> Result<()> {
    match command {
        PluginCommands::List => {
            println!("Available plugins:");
            for plugin in BUILTIN_PLUGINS {
                println!("  - {}", plugin);
            }
        }
        PluginCommands::Schema { plugin } => {
            println!(
                "Plugin schema for: {}. Schema output is implemented in a later phase.",
                plugin
            );
        }
        PluginCommands::Run(args) => {
            run_plugin(args).await?;
        }
        PluginCommands::Validate { plugin, config } => {
            println!(
                "Validating plugin '{}' configuration. Config path: {:?}",
                plugin, config
            );
        }
        PluginCommands::Formats { plugin } => {
            println!(
                "Report formats for plugin '{}': json, markdown, sarif",
                plugin
            );
        }
    }
    Ok(())
}

/// Runs a plugin directly with the supplied arguments.
///
/// Validates that at least one of `args.workspace` or `args.scan_artifact` is
/// set before proceeding. When `args.dry_run` is `true`, prints a message
/// indicating that no providers will be invoked.
///
/// # Arguments
///
/// * `args` - Parsed arguments from the `plugin run` subcommand.
///
/// # Errors
///
/// Returns [`PipelineError::Workflow`] if neither `--workspace` nor
/// `--scan-artifact` is provided.
async fn run_plugin(args: PluginRunArgs) -> Result<()> {
    if args.workspace.is_none() && args.scan_artifact.is_none() {
        return Err(PipelineError::Workflow(
            "plugin run requires --workspace or --scan-artifact".to_string(),
        ));
    }

    if args.dry_run {
        println!("Dry run mode: no providers will be invoked.");
    }

    println!("Running plugin: {}", args.plugin);

    if let Some(ref ws) = args.workspace {
        println!("Workspace: {}", ws);
    }

    if let Some(ref artifact) = args.scan_artifact {
        println!("Scan artifact: {}", artifact);
    }

    println!("Plugin run execution is implemented in a later phase.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{PluginCommands, PluginRunArgs};

    /// Returns a [`PluginRunArgs`] with no workspace or scan artifact set.
    fn make_minimal_plugin_run_args(plugin: &str) -> PluginRunArgs {
        PluginRunArgs {
            plugin: plugin.to_string(),
            workspace: None,
            scan_artifact: None,
            config: None,
            provider: None,
            model: None,
            dry_run: false,
            output_dir: None,
            report_format: vec![],
        }
    }

    #[tokio::test]
    async fn test_execute_list_returns_ok() {
        let result = execute(PluginCommands::List).await;
        assert!(
            result.is_ok(),
            "plugin list should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_run_rejects_missing_workspace_and_artifact() {
        let args = make_minimal_plugin_run_args("technical-review");
        let result = execute(PluginCommands::Run(args)).await;
        assert!(
            result.is_err(),
            "expected error when workspace and artifact are both absent"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("plugin run requires --workspace or --scan-artifact"),
            "expected missing-workspace error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn test_execute_run_accepts_workspace() {
        let mut args = make_minimal_plugin_run_args("technical-review");
        args.workspace = Some(".xzardgz/workspace".to_string());
        let result = execute(PluginCommands::Run(args)).await;
        assert!(
            result.is_ok(),
            "plugin run with workspace should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_schema_returns_ok() {
        let result = execute(PluginCommands::Schema {
            plugin: "technical-review".to_string(),
        })
        .await;
        assert!(
            result.is_ok(),
            "plugin schema should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_formats_returns_ok() {
        let result = execute(PluginCommands::Formats {
            plugin: "technical-review".to_string(),
        })
        .await;
        assert!(
            result.is_ok(),
            "plugin formats should succeed, got: {:?}",
            result.err()
        );
    }
}
