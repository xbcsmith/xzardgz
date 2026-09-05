//! Repository scan command handler.
//!
//! This module implements the `scan` subcommand, which triggers a structured
//! scan of a target repository and writes the resulting artifact to disk for
//! later consumption by the `run` or `plugin run` commands. Execution is
//! delegated entirely to [`WorkflowExecutor`] via
//! [`ExecutionInput::ScanOnly`], which creates or resumes a workspace, runs
//! the scanner, and persists the scan artifact.

use std::sync::Arc;

use crate::cli::ScanArgs;
use crate::commands::run::print_execution_result;
use crate::config::Config;
use crate::error::{PipelineError, Result};
use crate::plugins::registry::PluginRegistry;
use crate::workflow::executor::{ExecutionInput, WorkflowExecutor};

/// Executes the repository scan command.
///
/// Loads configuration via [`Config::load`] and dispatches to
/// [`execute_with`]. See [`execute_with`] for the full execution logic and
/// for a config-injectable variant suitable for tests and programmatic
/// embedding.
///
/// # Arguments
///
/// * `args` - Parsed CLI arguments from the `scan` subcommand.
///
/// # Errors
///
/// Returns [`PipelineError::Config`] if configuration loading fails.
/// See [`execute_with`] for the remaining error conditions.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::cli::ScanArgs;
/// use xzardgz::commands::scan::execute;
///
/// let args = ScanArgs {
///     repository: ".".to_string(),
///     branch: None,
///     workspace: None,
///     output: None,
///     format: None,
///     overwrite: false,
///     resume: false,
/// };
/// // Call from an async context: let result = execute(args).await;
/// ```
pub async fn execute(args: ScanArgs) -> Result<()> {
    let config = Config::load()?;
    execute_with(args, config).await
}

/// Executes a repository scan using an explicitly supplied configuration.
///
/// This is the full implementation behind [`execute`]. It is a separate,
/// fully public function so tests and programmatic embedders can supply a
/// pre-configured [`Config`] (for example, one pointing at a temporary
/// workspace root) instead of depending on [`Config::load`] reading
/// `config.yaml` from the current directory.
///
/// The scan never invokes a plugin, so an empty [`PluginRegistry`] is used
/// internally when constructing the [`WorkflowExecutor`]; no plugin lookup
/// ever occurs on the `ScanOnly` execution path.
///
/// # Scope note: `--format` and `--overwrite` are currently no-ops
///
/// `args.format` and `args.overwrite` are accepted for CLI compatibility but
/// are **not** consumed by [`ExecutionInput::ScanOnly`] today: the executor
/// always writes the scan artifact as YAML, and always overwrites any
/// existing file at the output path unconditionally. This is a pre-existing
/// gap in the executor, not something this function attempts to work around;
/// implementing JSON conversion or overwrite-protection here would duplicate
/// (and potentially diverge from) logic that belongs in the executor.
///
/// # Arguments
///
/// * `args` - Parsed CLI arguments from the `scan` subcommand.
/// * `config` - The configuration to execute against.
///
/// # Errors
///
/// Returns [`PipelineError::Workflow`] if `args.repository` does not resolve
/// to an accessible local directory, if governance checks reject the
/// repository, branch, or workspace path, or if the underlying scan result
/// reports `success: false` (summarizing `result.errors`, or a generic
/// message when `result.errors` is empty). Also propagates any error raised
/// by workspace creation/resumption or by the scanner itself.
///
/// # Examples
///
/// ```
/// use xzardgz::cli::ScanArgs;
/// use xzardgz::commands::scan::execute_with;
/// use xzardgz::config::Config;
///
/// # async fn run() -> xzardgz::error::Result<()> {
/// let args = ScanArgs {
///     repository: ".".to_string(),
///     branch: None,
///     workspace: None,
///     output: None,
///     format: None,
///     overwrite: false,
///     resume: false,
/// };
/// // `Config::default()` is used here purely to compile a doctest; a real
/// // scan requires a real filesystem repository and workspace root.
/// let _ = args;
/// let _ = Config::default();
/// # Ok(())
/// # }
/// ```
pub async fn execute_with(args: ScanArgs, config: Config) -> Result<()> {
    let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(PluginRegistry::new()));

    let result = executor
        .execute(ExecutionInput::ScanOnly {
            repository: args.repository.clone(),
            output_path: args.output.clone(),
            branch: args.branch.clone(),
            resume: args.resume,
            workspace: args.workspace.clone(),
        })
        .await?;

    print_execution_result(&result);

    if !result.success {
        return Err(PipelineError::Workflow(if result.errors.is_empty() {
            "scan did not complete successfully".to_string()
        } else {
            format!("scan failed: {}", result.errors.join("; "))
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
    use tempfile::TempDir;

    /// Returns a [`ScanArgs`] with all optional fields set to `None` /
    /// defaults, targeting `repository`.
    fn make_scan_args(repository: &str) -> ScanArgs {
        ScanArgs {
            repository: repository.to_string(),
            branch: None,
            workspace: None,
            output: None,
            format: None,
            overwrite: false,
            resume: false,
        }
    }

    /// Builds a `Config` with governance disabled, rooted at
    /// `workspace_root`, safe for hermetic tests that never touch the real
    /// crate working directory.
    fn make_test_config(workspace_root: &str) -> Config {
        let mut config = Config::default();
        config.workspace.root = workspace_root.to_string();
        config.governance.enabled = false;
        config.governance.rules_path = String::new();
        config
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

    #[tokio::test]
    async fn test_execute_with_real_scan_produces_workspace_and_artifact() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let mut args = make_scan_args(repo_dir.path().to_str().unwrap());
        args.workspace = Some(ws_dir.path().to_str().unwrap().to_string());

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let result = execute_with(args, config).await;
        assert!(
            result.is_ok(),
            "scan should succeed for a real temp repository, got: {:?}",
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

        // A real scan artifact was written somewhere under the workspace.
        let artifact = find_file_named(ws_dir.path(), "artifact.yaml");
        assert!(
            artifact.is_some(),
            "expected an artifact.yaml file somewhere under {:?}",
            ws_dir.path()
        );
        let artifact_path = artifact.unwrap();
        let metadata = std::fs::metadata(&artifact_path).unwrap();
        assert!(
            metadata.len() > 0,
            "expected scan artifact at {:?} to be non-empty",
            artifact_path
        );
    }

    #[tokio::test]
    async fn test_execute_with_output_writes_explicit_artifact_copy() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();
        let out_dir = TempDir::new().unwrap();
        let out_file = out_dir.path().join("scan-artifact.yaml");

        let mut args = make_scan_args(repo_dir.path().to_str().unwrap());
        args.workspace = Some(ws_dir.path().to_str().unwrap().to_string());
        args.output = Some(out_file.to_string_lossy().to_string());

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let result = execute_with(args, config).await;
        assert!(
            result.is_ok(),
            "scan with --output should succeed, got: {:?}",
            result.err()
        );

        assert!(
            out_file.exists(),
            "expected explicit output path {:?} to exist",
            out_file
        );
        let metadata = std::fs::metadata(&out_file).unwrap();
        assert!(
            metadata.len() > 0,
            "expected explicit output artifact at {:?} to be non-empty",
            out_file
        );
    }

    #[tokio::test]
    async fn test_execute_with_resume_against_existing_workspace_succeeds() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        // First scan: creates the workspace and its scan artifact.
        let first_args = {
            let mut a = make_scan_args(repo_dir.path().to_str().unwrap());
            a.workspace = Some(ws_dir.path().to_str().unwrap().to_string());
            a
        };
        let first_config = make_test_config(ws_dir.path().to_str().unwrap());
        execute_with(first_args, first_config)
            .await
            .expect("initial scan must succeed to set up the resume scenario");

        // Second scan: resume = true against the same repository/workspace.
        let mut second_args = make_scan_args(repo_dir.path().to_str().unwrap());
        second_args.workspace = Some(ws_dir.path().to_str().unwrap().to_string());
        second_args.resume = true;

        let second_config = make_test_config(ws_dir.path().to_str().unwrap());
        let result = execute_with(second_args, second_config).await;
        assert!(
            result.is_ok(),
            "resumed scan should succeed, got: {:?}",
            result.err()
        );

        // The scan artifact still exists on disk after the resumed run.
        let artifact = find_file_named(ws_dir.path(), "artifact.yaml");
        assert!(
            artifact.is_some(),
            "expected an artifact.yaml file to still exist under {:?} after resume",
            ws_dir.path()
        );
    }

    #[tokio::test]
    async fn test_execute_with_nonexistent_repository_path_returns_err() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();
        let missing_repo = repo_dir.path().join("does-not-exist");

        let mut args = make_scan_args(missing_repo.to_str().unwrap());
        args.workspace = Some(ws_dir.path().to_str().unwrap().to_string());

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let result = execute_with(args, config).await;
        assert!(
            result.is_err(),
            "expected an error for a repository path that does not exist"
        );
    }
}
