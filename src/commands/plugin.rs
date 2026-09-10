//! Plugin management command handler.
//!
//! This module implements the `plugin` family of subcommands, covering plugin
//! listing, schema introspection, direct plugin execution, configuration
//! validation, and report format discovery.

use std::sync::Arc;

use crate::cli::{PluginCommands, PluginRunArgs};
use crate::commands::run::print_execution_result;
use crate::config::{Config, ConfigOverrides};
use crate::error::{PipelineError, Result};
use crate::plugins::registry::PluginRegistry;
use crate::workflow::executor::{ExecutionInput, WorkflowExecutor};

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

/// Runs a plugin directly against an existing workspace or scan artifact.
///
/// Loads configuration via [`Config::load`] and dispatches to
/// [`run_plugin_with`] using the built-in plugin registry
/// ([`PluginRegistry::with_builtins`]). See [`run_plugin_with`] for the full
/// execution logic and for a config/registry-injectable variant suitable for
/// tests and programmatic embedding.
///
/// # Arguments
///
/// * `args` - Parsed arguments from the `plugin run` subcommand.
///
/// # Errors
///
/// Returns [`PipelineError::Config`] if configuration loading fails.
/// See [`run_plugin_with`] for the remaining error conditions.
async fn run_plugin(args: PluginRunArgs) -> Result<()> {
    let config = Config::load()?;
    run_plugin_with(args, config, PluginRegistry::with_builtins()).await
}

/// Runs a plugin using an explicitly supplied configuration and plugin
/// registry.
///
/// This is the full implementation behind [`run_plugin`]. It is a separate,
/// fully public function so tests and programmatic embedders can supply a
/// pre-configured [`Config`] (for example, one with governance disabled and
/// a workspace root under a temporary directory) and a custom
/// [`PluginRegistry`] (for example, one containing only fake plugins)
/// instead of depending on [`Config::load`] reading `config.yaml` from the
/// current directory and on the real built-in plugins (which make real
/// network calls to an AI provider) being registered.
///
/// Bypasses the full `run` pipeline entirely: no repository resolution, no
/// git metadata collection, and no repository scan is performed. Scan data
/// is loaded either from an existing workspace's recorded scan artifact
/// (`args.workspace`) or from an externally-supplied scan-artifact YAML file
/// (`args.scan_artifact`).
///
/// At least one of `args.workspace` or `args.scan_artifact` must be set;
/// supplying neither returns an error immediately, before `config` or
/// `registry` is touched, or the plugin config file (if any) is read.
///
/// When `args.config` is set, it is treated as a path to a plugin
/// configuration file (JSON or YAML) which is read from disk and parsed
/// into a [`serde_json::Value`] before being passed to the executor.
///
/// CLI overrides for provider and model are applied to `config` before
/// execution. There is no `plugin run`-specific workspace-root override
/// flag (unlike `run`'s `--workspace`), so `workspace_root` is passed as
/// `None` to [`ExecutionInput::PluginOnly`], falling back to `config`'s own
/// workspace root for any freshly-created ephemeral workspace.
///
/// # Arguments
///
/// * `args` - Parsed arguments from the `plugin run` subcommand.
/// * `config` - The configuration to execute against.
/// * `registry` - The plugin registry to resolve `args.plugin` against.
///
/// # Errors
///
/// Returns [`PipelineError::Workflow`] if neither `--workspace` nor
/// `--scan-artifact` is provided, or if `--config` points to a file that
/// cannot be parsed as JSON or YAML.
/// Returns [`PipelineError::Io`] if the plugin config file cannot be read
/// from disk.
/// Returns an error if the underlying [`WorkflowExecutor::execute`] call
/// fails, or propagates a summarized error when it returns a result with
/// `success: false`.
pub async fn run_plugin_with(
    args: PluginRunArgs,
    mut config: Config,
    registry: PluginRegistry,
) -> Result<()> {
    if args.workspace.is_none() && args.scan_artifact.is_none() {
        return Err(PipelineError::Workflow(
            "plugin run requires --workspace or --scan-artifact".to_string(),
        ));
    }

    let plugin_config = match args.config {
        Some(ref path) => {
            let content = std::fs::read_to_string(path).map_err(PipelineError::Io)?;
            let value = serde_yaml::from_str::<serde_json::Value>(&content).map_err(|e| {
                PipelineError::Workflow(format!("failed to parse plugin config '{}': {}", path, e))
            })?;
            Some(value)
        }
        None => None,
    };

    let overrides = ConfigOverrides {
        provider: args.provider.clone(),
        openai_model: args.model.clone(),
        ollama_model: None,
        ollama_host: None,
        workspace_root: None,
    };
    config.apply_overrides(&overrides);

    let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(registry));

    let result = executor
        .execute(ExecutionInput::PluginOnly {
            plugin: args.plugin.clone(),
            workspace_dir: args.workspace.clone(),
            scan_artifact_path: args.scan_artifact.clone(),
            workspace_root: None,
            config: plugin_config,
            provider: args.provider.clone(),
            model: args.model.clone(),
            dry_run: args.dry_run,
            report_formats: args.report_format.clone(),
            output_dir: args.output_dir.clone(),
            correlation_id: None,
        })
        .await?;

    print_execution_result(&result);

    if !result.success {
        return Err(PipelineError::Workflow(if result.errors.is_empty() {
            "plugin run did not complete successfully".to_string()
        } else {
            format!("plugin run failed: {}", result.errors.join("; "))
        }));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use tempfile::TempDir;

    use crate::cli::{PluginCommands, PluginRunArgs};
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
    async fn test_run_plugin_with_rejects_missing_workspace_and_artifact() {
        let args = make_minimal_plugin_run_args("technical-review");
        let result = run_plugin_with(args, Config::default(), PluginRegistry::new()).await;
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
    async fn test_run_plugin_with_scan_artifact_produces_real_report() {
        let repo_dir = TempDir::new().unwrap();
        let scan_ws_dir = TempDir::new().unwrap();
        let plugin_ws_dir = TempDir::new().unwrap();

        // Produce a real scan artifact first via a real ScanOnly execution.
        let scan_config = make_test_config(scan_ws_dir.path().to_str().unwrap());
        let scan_executor = WorkflowExecutor::new(
            Arc::new(scan_config),
            Arc::new(registry_with_success_plugin()),
        );
        let scan_result = scan_executor
            .execute(ExecutionInput::ScanOnly {
                repository: repo_dir.path().to_str().unwrap().to_string(),
                output_path: None,
                branch: None,
                resume: false,
                workspace: Some(scan_ws_dir.path().to_str().unwrap().to_string()),
                correlation_id: None,
            })
            .await
            .expect("scan-only run must succeed to produce a scan artifact");
        let artifact_path = scan_result
            .scan_artifact_path
            .expect("scan-only run must produce an artifact path");

        let mut args = make_minimal_plugin_run_args("test-plugin");
        args.scan_artifact = Some(artifact_path);
        args.report_format = vec!["json".to_string()];

        let config = make_test_config(plugin_ws_dir.path().to_str().unwrap());
        let result = run_plugin_with(args, config, registry_with_success_plugin()).await;
        assert!(
            result.is_ok(),
            "plugin run from scan artifact should succeed, got: {:?}",
            result.err()
        );

        assert!(
            find_file_with_extension(plugin_ws_dir.path(), "json").is_some(),
            "expected at least one .json report file under {:?}",
            plugin_ws_dir.path()
        );
    }

    #[tokio::test]
    async fn test_run_plugin_with_workspace_dir_produces_real_report() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        // First, a real ScanOnly execution creates a workspace with a
        // recorded scan artifact that plugin run --workspace can resume
        // from.
        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let executor =
            WorkflowExecutor::new(Arc::new(config), Arc::new(registry_with_success_plugin()));
        let scan_result = executor
            .execute(ExecutionInput::ScanOnly {
                repository: repo_dir.path().to_str().unwrap().to_string(),
                output_path: None,
                branch: None,
                resume: false,
                workspace: Some(ws_dir.path().to_str().unwrap().to_string()),
                correlation_id: None,
            })
            .await
            .expect("initial scan-only run must succeed to produce a workspace");

        let workspace_dir = ws_dir
            .path()
            .join(&scan_result.workspace_id)
            .to_string_lossy()
            .to_string();

        let mut args = make_minimal_plugin_run_args("test-plugin");
        args.workspace = Some(workspace_dir);
        args.report_format = vec!["json".to_string()];

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let result = run_plugin_with(args, config, registry_with_success_plugin()).await;
        assert!(
            result.is_ok(),
            "plugin run from workspace dir should succeed, got: {:?}",
            result.err()
        );

        assert!(
            find_file_with_extension(ws_dir.path(), "json").is_some(),
            "expected at least one .json report file under {:?}",
            ws_dir.path()
        );
    }

    #[tokio::test]
    async fn test_run_plugin_with_valid_config_file_succeeds() {
        let repo_dir = TempDir::new().unwrap();
        let scan_ws_dir = TempDir::new().unwrap();
        let plugin_ws_dir = TempDir::new().unwrap();
        let config_dir = TempDir::new().unwrap();

        let scan_config = make_test_config(scan_ws_dir.path().to_str().unwrap());
        let scan_executor = WorkflowExecutor::new(
            Arc::new(scan_config),
            Arc::new(registry_with_success_plugin()),
        );
        let scan_result = scan_executor
            .execute(ExecutionInput::ScanOnly {
                repository: repo_dir.path().to_str().unwrap().to_string(),
                output_path: None,
                branch: None,
                resume: false,
                workspace: Some(scan_ws_dir.path().to_str().unwrap().to_string()),
                correlation_id: None,
            })
            .await
            .expect("scan-only run must succeed to produce a scan artifact");
        let artifact_path = scan_result
            .scan_artifact_path
            .expect("scan-only run must produce an artifact path");

        let config_file_path = config_dir.path().join("plugin-config.yaml");
        std::fs::write(&config_file_path, "max_findings: 5\n").unwrap();

        let mut args = make_minimal_plugin_run_args("test-plugin");
        args.scan_artifact = Some(artifact_path);
        args.config = Some(config_file_path.to_str().unwrap().to_string());
        args.report_format = vec!["json".to_string()];

        let config = make_test_config(plugin_ws_dir.path().to_str().unwrap());
        let result = run_plugin_with(args, config, registry_with_success_plugin()).await;
        assert!(
            result.is_ok(),
            "plugin run with a valid plugin config file should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_plugin_with_invalid_config_file_returns_error() {
        let plugin_ws_dir = TempDir::new().unwrap();
        let config_dir = TempDir::new().unwrap();

        let config_file_path = config_dir.path().join("bad-config.yaml");
        // An unclosed flow sequence is a genuine YAML/JSON syntax error, so
        // this reliably fails to parse regardless of serde_yaml version
        // quirks around bare scalars.
        std::fs::write(&config_file_path, "foo: [1, 2").unwrap();

        let mut args = make_minimal_plugin_run_args("test-plugin");
        // Any workspace/scan-artifact source is enough to get past the
        // initial guard; the config file is parsed before it's used, so
        // even a nonexistent workspace path exercises the failure path.
        args.workspace = Some(plugin_ws_dir.path().to_str().unwrap().to_string());
        args.config = Some(config_file_path.to_str().unwrap().to_string());

        let config = make_test_config(plugin_ws_dir.path().to_str().unwrap());
        let result = run_plugin_with(args, config, registry_with_success_plugin()).await;
        assert!(
            result.is_err(),
            "expected an error for an invalid plugin config file"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains(config_file_path.to_str().unwrap()) || msg.contains("failed to parse"),
            "expected a config-parse-failure error mentioning the file path, got: {msg}"
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
