//! Ruleset management command handler.
//!
//! Implements `ruleset list`, `ruleset sync`, `ruleset doctor`, and
//! `ruleset assess`. All subcommands are offline-safe: a missing or
//! unreachable source always produces a warning and continues rather than
//! aborting the command.
//!
//! # Execution model
//!
//! Each public `execute_*` function accepts parsed CLI args and a [`Config`]
//! reference so it can be tested without touching the filesystem for
//! configuration. The top-level [`execute`] function loads [`Config`] and
//! dispatches to the appropriate inner handler.

use std::path::PathBuf;

use crate::cli::{
    RulesetAssessArgs, RulesetCommands, RulesetDoctorArgs, RulesetListArgs, RulesetSyncArgs,
};
use crate::clients::rulesets::{SourcesManifest, load_manifest, sync_source};
use crate::config::Config;
use crate::error::{PipelineError, Result};

// ---------------------------------------------------------------------------
// Top-level dispatcher
// ---------------------------------------------------------------------------

/// Execute the `ruleset` command group.
///
/// Loads [`Config`] from the default path (`config.yaml`) and dispatches to
/// the appropriate subcommand handler. Use the `execute_*` functions directly
/// when injecting a configuration for testing.
///
/// # Arguments
///
/// * `command` - Ruleset subcommand variant parsed from the CLI.
///
/// # Errors
///
/// Returns [`PipelineError::Config`] if the configuration file exists but
/// cannot be parsed. See individual `execute_*` functions for further error
/// conditions.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::cli::{RulesetCommands, RulesetListArgs};
/// use xzardgz::commands::ruleset::execute;
///
/// # #[tokio::main]
/// # async fn main() {
/// let args = RulesetListArgs { sources_file: None };
/// // execute(RulesetCommands::List(args)).await.unwrap();
/// # }
/// ```
pub async fn execute(command: RulesetCommands) -> Result<()> {
    let config = Config::load()?;
    match command {
        RulesetCommands::List(args) => execute_list(args, &config).await,
        RulesetCommands::Sync(args) => execute_sync(args, &config).await,
        RulesetCommands::Doctor(args) => execute_doctor(args, &config).await,
        RulesetCommands::Assess(args) => execute_assess(args, &config).await,
    }
}

// ---------------------------------------------------------------------------
// Manifest resolution helper
// ---------------------------------------------------------------------------

/// Resolve the sources manifest file path from CLI args and config fallback.
///
/// Resolution order (first Some wins):
/// 1. `args_file` (explicit `--sources-file` flag)
/// 2. `config_file` (from `config.sast.ruleset_sources_file`)
/// 3. `~/.config/xzardgz/sources.yaml` if the file exists on disk
///
/// Returns `None` when no path is found.
fn resolve_sources_path(
    args_file: Option<&PathBuf>,
    config_file: Option<&PathBuf>,
) -> Option<PathBuf> {
    if let Some(p) = args_file {
        return Some(p.clone());
    }
    if let Some(p) = config_file {
        return Some(p.clone());
    }
    // Try the default user config dir.
    dirs::config_dir()
        .map(|d| d.join("xzardgz").join("sources.yaml"))
        .filter(|p| p.exists())
}

/// Load the manifest from `path`, or return an empty manifest with a warning.
///
/// This function never returns an error: failures are reported via
/// `tracing::warn!` so the calling subcommand can still complete with `Ok(())`.
fn load_manifest_or_empty(path: Option<PathBuf>) -> SourcesManifest {
    match path {
        None => {
            tracing::warn!(
                "no sources manifest found; \
                 create one or run `xzardgz ruleset sync` with --sources-file"
            );
            SourcesManifest::new()
        }
        Some(p) => match load_manifest(&p) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    path = %p.display(),
                    error = %e,
                    "failed to load sources manifest; proceeding with empty manifest"
                );
                SourcesManifest::new()
            }
        },
    }
}

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

/// Execute `ruleset list`.
///
/// Loads the sources manifest and prints a table with columns:
/// `SOURCE_ID | ENABLED | LICENCE | URL`.
/// When no manifest is found the command prints a notice and returns `Ok(())`.
///
/// # Arguments
///
/// * `args` - Parsed CLI arguments for `ruleset list`.
/// * `config` - Loaded pipeline configuration.
///
/// # Errors
///
/// This function does not currently return errors; all manifest loading
/// failures are degraded gracefully with a warning.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::cli::RulesetListArgs;
/// use xzardgz::commands::ruleset::execute_list;
/// use xzardgz::config::Config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let args = RulesetListArgs { sources_file: None };
/// let config = Config::default();
/// execute_list(args, &config).await.unwrap();
/// # }
/// ```
pub async fn execute_list(args: RulesetListArgs, config: &Config) -> Result<()> {
    let path = resolve_sources_path(
        args.sources_file.as_ref(),
        config.sast.ruleset_sources_file.as_ref(),
    );
    let manifest = load_manifest_or_empty(path);

    if manifest.is_empty() {
        println!("no ruleset sources found");
        return Ok(());
    }

    // Print a simple text table.
    println!("{:<20} {:<8} {:<20} URL", "SOURCE_ID", "ENABLED", "LICENCE");
    println!("{}", "-".repeat(80));

    let mut ids: Vec<&String> = manifest.keys().collect();
    ids.sort();
    for id in ids {
        let src = &manifest[id];
        println!(
            "{:<20} {:<8} {:<20} {}",
            id,
            if src.is_enabled() { "yes" } else { "no" },
            src.license.as_deref().unwrap_or("unknown"),
            src.repository_url,
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// sync
// ---------------------------------------------------------------------------

/// Execute `ruleset sync`.
///
/// Loads the manifest, then syncs either a single named source or all enabled
/// sources. For each source the result of [`sync_source`] is printed.
/// Per-source failures are degraded to warnings so the command continues.
///
/// # Arguments
///
/// * `args` - Parsed CLI arguments for `ruleset sync`.
/// * `config` - Loaded pipeline configuration.
///
/// # Errors
///
/// Returns [`PipelineError::Sast`] if a specific `source` ID was requested
/// but is not present in the manifest.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::cli::RulesetSyncArgs;
/// use xzardgz::commands::ruleset::execute_sync;
/// use xzardgz::config::Config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let args = RulesetSyncArgs { source: None, sources_file: None };
/// let config = Config::default();
/// execute_sync(args, &config).await.unwrap();
/// # }
/// ```
pub async fn execute_sync(args: RulesetSyncArgs, config: &Config) -> Result<()> {
    let path = resolve_sources_path(
        args.sources_file.as_ref(),
        config.sast.ruleset_sources_file.as_ref(),
    );
    let manifest = load_manifest_or_empty(path);

    // If a specific source was requested, validate it exists.
    if let Some(ref source_id) = args.source {
        let source = manifest.get(source_id).ok_or_else(|| {
            PipelineError::Sast(format!(
                "source '{source_id}' not found in manifest; \
                 run `ruleset list` to see available sources"
            ))
        })?;
        let result = sync_source(source_id, source);
        print_sync_result(&result);
        return Ok(());
    }

    // Sync all enabled sources.
    let enabled: Vec<(&String, _)> = {
        let mut v: Vec<(&String, &_)> = manifest
            .iter()
            .filter(|(_, src)| src.is_enabled())
            .collect();
        v.sort_by_key(|(id, _)| *id);
        v
    };

    if enabled.is_empty() {
        println!("no enabled sources found; set enabled: true in your sources.yaml");
        return Ok(());
    }

    for (id, source) in enabled {
        let result = sync_source(id, source);
        if !result.success {
            tracing::warn!(source_id = %id, reason = %result.message, "sync failed");
        }
        print_sync_result(&result);
    }

    Ok(())
}

/// Print a formatted sync result line.
fn print_sync_result(result: &crate::clients::rulesets::SyncResult) {
    if result.success {
        println!("[OK]   {} — {}", result.source_id, result.license);
    } else {
        println!("[WARN] {} — {}", result.source_id, result.message);
    }
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

/// Execute `ruleset doctor`.
///
/// Validates each source in the manifest for:
/// - `repository_url` being an HTTPS URL
/// - `repository_branch` being non-empty
///
/// Issues are reported as warnings; the command always returns `Ok(())`.
///
/// # Arguments
///
/// * `args` - Parsed CLI arguments for `ruleset doctor`.
/// * `config` - Loaded pipeline configuration.
///
/// # Errors
///
/// This function does not return errors; all issues are reported via
/// `tracing::warn!` and printed to stdout.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::cli::RulesetDoctorArgs;
/// use xzardgz::commands::ruleset::execute_doctor;
/// use xzardgz::config::Config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let args = RulesetDoctorArgs { sources_file: None };
/// let config = Config::default();
/// execute_doctor(args, &config).await.unwrap();
/// # }
/// ```
pub async fn execute_doctor(args: RulesetDoctorArgs, config: &Config) -> Result<()> {
    let path = resolve_sources_path(
        args.sources_file.as_ref(),
        config.sast.ruleset_sources_file.as_ref(),
    );
    let manifest = load_manifest_or_empty(path);

    if manifest.is_empty() {
        println!("no ruleset sources to validate");
        return Ok(());
    }

    let mut issues = 0usize;
    let mut ids: Vec<&String> = manifest.keys().collect();
    ids.sort();

    for id in ids {
        let src = &manifest[id];
        if !src.repository_url.starts_with("https://") {
            tracing::warn!(source_id = %id, url = %src.repository_url, "URL is not HTTPS");
            println!(
                "[WARN] {id}: repository_url is not an HTTPS URL: {}",
                src.repository_url
            );
            issues += 1;
        }
        if src.repository_branch.as_deref().unwrap_or("").is_empty() {
            tracing::warn!(source_id = %id, "repository_branch is empty");
            println!("[WARN] {id}: repository_branch must not be empty");
            issues += 1;
        }
    }

    if issues == 0 {
        println!("all {} sources passed validation", manifest.len());
    } else {
        println!("{issues} issue(s) found across {} sources", manifest.len());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// assess
// ---------------------------------------------------------------------------

/// Execute `ruleset assess`.
///
/// Looks up the named source in the manifest, runs [`sync_source`] to
/// validate it, and reports the outcome. Returns an error if the source is
/// not found in the manifest.
///
/// # Arguments
///
/// * `args` - Parsed CLI arguments for `ruleset assess`.
/// * `config` - Loaded pipeline configuration.
///
/// # Errors
///
/// Returns [`PipelineError::Sast`] if `args.source` is not present in the
/// manifest.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::cli::RulesetAssessArgs;
/// use xzardgz::commands::ruleset::execute_assess;
/// use xzardgz::config::Config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let args = RulesetAssessArgs {
///     source: "hashicorp".to_string(),
///     sources_file: None,
/// };
/// let config = Config::default();
/// // execute_assess(args, &config).await.unwrap();
/// # }
/// ```
pub async fn execute_assess(args: RulesetAssessArgs, config: &Config) -> Result<()> {
    let path = resolve_sources_path(
        args.sources_file.as_ref(),
        config.sast.ruleset_sources_file.as_ref(),
    );
    let manifest = load_manifest_or_empty(path);

    let source = manifest.get(&args.source).ok_or_else(|| {
        PipelineError::Sast(format!(
            "source '{}' not found in manifest; \
             run `ruleset list` to see available sources",
            args.source
        ))
    })?;

    let result = sync_source(&args.source, source);

    println!("assess: {}", args.source);
    println!("  description : {}", source.description);
    println!("  url         : {}", source.repository_url);
    println!(
        "  branch      : {}",
        source.repository_branch.as_deref().unwrap_or("(none)")
    );
    println!(
        "  licence     : {}",
        source.license.as_deref().unwrap_or("unknown")
    );
    println!("  enabled     : {}", source.is_enabled());
    println!(
        "  validation  : {}",
        if result.success { "passed" } else { "failed" }
    );
    if !result.success {
        println!("  reason      : {}", result.message);
    }
    // Rule count is a stub; actual parsing is deferred to a later phase.
    println!("  yaml files  : 0 (fetch not yet implemented)");

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Build a minimal sources.yaml with the given enabled state for each source.
    fn write_sources_yaml(enabled: bool) -> NamedTempFile {
        let yaml = format!(
            r#"
example:
  description: "Example rules"
  repository_url: "https://github.com/example/rules"
  repository_branch: "main"
  author: "Example"
  license: "MIT"
  enabled: {enabled}
"#
        );
        let mut file = NamedTempFile::new()
            // SAFETY: tempfile creation in test; failure is unrecoverable test setup.
            .expect("tempfile creation must succeed");
        file.write_all(yaml.as_bytes())
            .expect("write to tempfile must succeed");
        file
    }

    #[tokio::test]
    async fn test_execute_list_with_missing_sources_file_returns_ok() {
        let args = RulesetListArgs {
            sources_file: Some(PathBuf::from(
                "/tmp/xzardgz_test_missing_list_sources_8675309.yaml",
            )),
        };
        let config = Config::default();
        let result = execute_list(args, &config).await;
        assert!(
            result.is_ok(),
            "list with a missing sources file must return Ok, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_list_with_no_sources_file_returns_ok() {
        let args = RulesetListArgs { sources_file: None };
        let mut config = Config::default();
        // Point config at a path that does not exist so we get an empty manifest.
        config.sast.ruleset_sources_file = Some(PathBuf::from(
            "/tmp/xzardgz_test_missing_config_sources_8675309.yaml",
        ));
        let result = execute_list(args, &config).await;
        assert!(
            result.is_ok(),
            "list with no sources file must return Ok, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_list_with_valid_sources_file_returns_ok() {
        let file = write_sources_yaml(false);
        let args = RulesetListArgs {
            sources_file: Some(file.path().to_path_buf()),
        };
        let config = Config::default();
        let result = execute_list(args, &config).await;
        assert!(
            result.is_ok(),
            "list with valid sources file must return Ok, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_doctor_with_missing_sources_file_returns_ok() {
        let args = RulesetDoctorArgs {
            sources_file: Some(PathBuf::from(
                "/tmp/xzardgz_test_missing_doctor_sources_8675309.yaml",
            )),
        };
        let config = Config::default();
        let result = execute_doctor(args, &config).await;
        assert!(
            result.is_ok(),
            "doctor with a missing sources file must return Ok, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_doctor_with_valid_sources_returns_ok() {
        let file = write_sources_yaml(false);
        let args = RulesetDoctorArgs {
            sources_file: Some(file.path().to_path_buf()),
        };
        let config = Config::default();
        let result = execute_doctor(args, &config).await;
        assert!(
            result.is_ok(),
            "doctor with valid sources must return Ok, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_sync_with_valid_sources_file_and_no_enabled_sources_returns_ok() {
        // All sources disabled; sync should complete with Ok without attempting any fetch.
        let file = write_sources_yaml(false);
        let args = RulesetSyncArgs {
            source: None,
            sources_file: Some(file.path().to_path_buf()),
        };
        let config = Config::default();
        let result = execute_sync(args, &config).await;
        assert!(
            result.is_ok(),
            "sync with no enabled sources must return Ok, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_sync_with_enabled_source_returns_ok() {
        let file = write_sources_yaml(true);
        let args = RulesetSyncArgs {
            source: None,
            sources_file: Some(file.path().to_path_buf()),
        };
        let config = Config::default();
        let result = execute_sync(args, &config).await;
        assert!(
            result.is_ok(),
            "sync with enabled source must return Ok, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_sync_with_named_source_returns_ok() {
        let file = write_sources_yaml(true);
        let args = RulesetSyncArgs {
            source: Some("example".to_string()),
            sources_file: Some(file.path().to_path_buf()),
        };
        let config = Config::default();
        let result = execute_sync(args, &config).await;
        assert!(
            result.is_ok(),
            "sync with named existing source must return Ok, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_sync_with_nonexistent_named_source_returns_err() {
        let file = write_sources_yaml(false);
        let args = RulesetSyncArgs {
            source: Some("does-not-exist".to_string()),
            sources_file: Some(file.path().to_path_buf()),
        };
        let config = Config::default();
        let result = execute_sync(args, &config).await;
        assert!(
            result.is_err(),
            "sync with nonexistent named source must return Err"
        );
    }

    #[tokio::test]
    async fn test_execute_assess_with_nonexistent_source_id_returns_err() {
        // The manifest exists but the requested source ID is not in it.
        let file = write_sources_yaml(false);
        let args = RulesetAssessArgs {
            source: "nonexistent-source-id".to_string(),
            sources_file: Some(file.path().to_path_buf()),
        };
        let config = Config::default();
        let result = execute_assess(args, &config).await;
        assert!(
            result.is_err(),
            "assess with nonexistent source ID must return Err"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("nonexistent-source-id"),
            "error message must contain the missing source id"
        );
    }

    #[tokio::test]
    async fn test_execute_assess_with_valid_source_returns_ok() {
        let file = write_sources_yaml(false);
        let args = RulesetAssessArgs {
            source: "example".to_string(),
            sources_file: Some(file.path().to_path_buf()),
        };
        let config = Config::default();
        let result = execute_assess(args, &config).await;
        assert!(
            result.is_ok(),
            "assess with valid source must return Ok, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_doctor_with_no_sources_file_returns_ok() {
        let args = RulesetDoctorArgs { sources_file: None };
        let mut config = Config::default();
        config.sast.ruleset_sources_file = Some(PathBuf::from(
            "/tmp/xzardgz_test_doctor_no_config_sources_8675309.yaml",
        ));
        let result = execute_doctor(args, &config).await;
        assert!(result.is_ok(), "doctor with no sources file must return Ok");
    }
}
