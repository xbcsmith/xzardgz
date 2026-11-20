//! Ruleset acquisition and licence policy for the SAST scanner.
//!
//! # Sub-modules
//!
//! | Module      | Purpose                                              |
//! |-------------|------------------------------------------------------|
//! | `error`     | [`AcquisitionError`] — unified error type            |
//! | `license`   | [`LicenseClass`] enum and classification logic       |
//! | `manifest`  | [`BundleManifest`] — post-fetch cache record         |
//! | `source`    | [`RulesetSource`] enum — acquisition strategy        |
//! | `sources`   | [`SourcesManifest`] and [`RulesetSourceEntry`]        |
//!
//! # Component-boundary contract
//!
//! This module must NEVER import from `crate::scanner::sast`.  It operates
//! only on bytes, paths, `serde_yaml::Value`, and its own types.
//!
//! # Backward-compatible surface
//!
//! [`load_manifest`], [`sync_source`], and [`SyncResult`] are retained so
//! that `crate::commands::ruleset` can call them without modification.  They
//! delegate to [`sources::load_sources_manifest`] and validate the entry in
//! the same way as the Phase 7 stub did.

pub mod cache;
pub mod error;
pub mod fetch;
pub mod license;
pub mod manifest;
pub mod source;
pub mod sources;

// ---------------------------------------------------------------------------
// Backward-compatible re-exports
// ---------------------------------------------------------------------------

pub use sources::{RulesetSourceEntry, SourcesManifest};

use std::path::Path;

use crate::error::{PipelineError, Result};

/// Load a sources manifest from a YAML file on disk.
///
/// This is a backward-compatible wrapper around
/// [`sources::load_sources_manifest`] that converts [`error::AcquisitionError`]
/// into [`PipelineError::Sast`] so that `commands::ruleset` can use
/// `crate::error::Result`.
///
/// # Arguments
///
/// * `path` - Filesystem path to the sources manifest YAML file.
///
/// # Errors
///
/// Returns [`PipelineError::Io`] if the file cannot be read.
/// Returns [`PipelineError::Sast`] if the YAML is malformed.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use xzardgz::clients::rulesets::load_manifest;
///
/// let manifest = load_manifest(Path::new("sources.yaml")).unwrap();
/// println!("loaded {} sources", manifest.len());
/// ```
pub fn load_manifest(path: &Path) -> Result<SourcesManifest> {
    sources::load_sources_manifest(path).map_err(|e| PipelineError::Sast(e.to_string()))
}

/// Validation outcome for a single source sync operation.
///
/// The result is always constructed (never an error return) so callers can
/// iterate over all sources and collect results without aborting early.
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// Source identifier from the manifest.
    pub source_id: String,
    /// Whether the validation succeeded.
    pub success: bool,
    /// Licence string from the source entry, or empty if not declared.
    pub license: String,
    /// Human-readable status or failure reason.
    pub message: String,
}

/// Validate a single source entry and return a [`SyncResult`].
///
/// This is a validation stub: it checks that `repository_url` is an HTTPS URL
/// and that `repository_branch` is non-empty.  Actual network operations are
/// deferred to a later phase.
///
/// The function never returns an error; failures are reported inside the
/// returned [`SyncResult`] with `success: false`.
///
/// # Arguments
///
/// * `id`     - The source identifier key from the manifest.
/// * `source` - The [`RulesetSourceEntry`] to validate.
///
/// # Returns
///
/// A [`SyncResult`] describing the validation outcome.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::rulesets::{RulesetSourceEntry, sync_source};
///
/// let entry = RulesetSourceEntry {
///     description: "Example rules".to_string(),
///     repository_url: "https://github.com/example/rules".to_string(),
///     repository_branch: Some("main".to_string()),
///     repository_commit: None,
///     ignored: None,
///     author: Some("Example".to_string()),
///     license: Some("MIT".to_string()),
///     preprocessors: None,
///     enabled: Some(true),
///     subdir: None,
/// };
/// let result = sync_source("example", &entry);
/// assert!(result.success);
/// assert_eq!(result.license, "MIT");
/// ```
pub fn sync_source(id: &str, source: &RulesetSourceEntry) -> SyncResult {
    if !source.repository_url.starts_with("https://") {
        return SyncResult {
            source_id: id.to_string(),
            success: false,
            license: source.license.clone().unwrap_or_default(),
            message: format!(
                "repository_url must be an HTTPS URL, got: {}",
                source.repository_url
            ),
        };
    }

    let branch = source.repository_branch.as_deref().unwrap_or("");
    if branch.is_empty() {
        return SyncResult {
            source_id: id.to_string(),
            success: false,
            license: source.license.clone().unwrap_or_default(),
            message: "repository_branch must not be empty".to_string(),
        };
    }

    let lic = source.license.as_deref().unwrap_or("unknown");
    SyncResult {
        source_id: id.to_string(),
        success: true,
        license: lic.to_string(),
        message: format!("synced {id} \u{2014} {lic}"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_sources_yaml(enabled: bool) -> NamedTempFile {
        let yaml = format!(
            "example:\n  description: \"Example rules\"\n  repository_url: \"https://github.com/example/rules\"\n  repository_branch: \"main\"\n  author: \"Example\"\n  license: \"MIT\"\n  enabled: {enabled}\n"
        );
        let mut f = NamedTempFile::new()
            // SAFETY: test setup; failure is unrecoverable.
            .expect("tempfile creation must succeed");
        f.write_all(yaml.as_bytes())
            .expect("write to tempfile must succeed");
        f
    }

    #[test]
    fn test_load_manifest_with_valid_yaml_returns_sources() {
        let f = write_sources_yaml(false);
        let manifest = load_manifest(f.path()).expect("load_manifest must succeed");
        assert!(manifest.contains_key("example"));
        assert_eq!(manifest["example"].license.as_deref(), Some("MIT"));
    }

    #[test]
    fn test_load_manifest_with_missing_file_returns_error() {
        let result = load_manifest(Path::new(
            "/tmp/xzardgz_test_nonexistent_manifest_8675309.yaml",
        ));
        assert!(result.is_err(), "missing file must return an error");
    }

    #[test]
    fn test_sync_source_with_valid_https_url_returns_success() {
        let f = write_sources_yaml(true);
        let manifest = load_manifest(f.path()).expect("must load");
        let entry = &manifest["example"];
        let result = sync_source("example", entry);
        assert!(result.success, "valid HTTPS source must succeed");
        assert_eq!(result.source_id, "example");
        assert_eq!(result.license, "MIT");
    }

    #[test]
    fn test_sync_source_with_http_url_returns_failure() {
        let entry = RulesetSourceEntry {
            description: "Test".to_string(),
            repository_url: "http://github.com/example/rules".to_string(),
            repository_branch: Some("main".to_string()),
            repository_commit: None,
            ignored: None,
            author: None,
            license: Some("MIT".to_string()),
            preprocessors: None,
            enabled: Some(true),
            subdir: None,
        };
        let result = sync_source("example", &entry);
        assert!(!result.success, "http URL must fail validation");
        assert!(result.message.contains("HTTPS"));
    }

    #[test]
    fn test_sync_source_with_empty_branch_returns_failure() {
        let entry = RulesetSourceEntry {
            description: "Test".to_string(),
            repository_url: "https://github.com/example/rules".to_string(),
            repository_branch: Some(String::new()),
            repository_commit: None,
            ignored: None,
            author: None,
            license: None,
            preprocessors: None,
            enabled: Some(true),
            subdir: None,
        };
        let result = sync_source("example", &entry);
        assert!(!result.success, "empty branch must fail validation");
        assert!(result.message.contains("repository_branch"));
    }

    #[test]
    fn test_sync_source_with_no_branch_returns_failure() {
        let entry = RulesetSourceEntry {
            description: "Test".to_string(),
            repository_url: "https://github.com/example/rules".to_string(),
            repository_branch: None,
            repository_commit: None,
            ignored: None,
            author: None,
            license: None,
            preprocessors: None,
            enabled: Some(true),
            subdir: None,
        };
        let result = sync_source("example", &entry);
        assert!(!result.success, "absent branch must fail validation");
    }
}
