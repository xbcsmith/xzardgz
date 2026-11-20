//! Ruleset fetch layer: local path, Git, URL, and OCI acquisition.
//!
//! Each `fetch_*` function takes a fully-resolved source description and
//! returns a [`FetchResult`] containing the local directory path and any
//! resolved git commit SHA.
//!
//! # Boundary contract
//!
//! This module MUST NOT import from `src/scanner/sast/`. It operates only
//! on bytes, paths, and its own types.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::clients::rulesets::error::AcquisitionError;

// ---------------------------------------------------------------------------
// FetchResult
// ---------------------------------------------------------------------------

/// Result of a successful fetch operation.
///
/// Consumers use [`FetchResult::path`] to locate the downloaded rule files
/// and [`FetchResult::resolved_commit`] to record the pinned SHA in the
/// bundle manifest.
#[derive(Debug, Clone)]
pub struct FetchResult {
    /// Local directory containing the fetched rule files.
    pub path: PathBuf,

    /// Resolved git commit SHA (40-char lowercase hex), or `None` for
    /// non-git sources (path, URL, OCI).
    pub resolved_commit: Option<String>,

    /// Human-readable description of where the content came from.
    ///
    /// For git sources this is the remote URL; for path sources it is the
    /// display form of the local path.
    pub fetched_from: String,
}

// ---------------------------------------------------------------------------
// fetch_path_source
// ---------------------------------------------------------------------------

/// Fetch rules from a local path source.
///
/// Simply validates that the path exists and is a directory, then returns a
/// [`FetchResult`] pointing at it. No files are copied.
///
/// # Arguments
///
/// * `source_id` - Used only in error messages for context.
/// * `path` - Local filesystem path to the directory containing YAML rule files.
///
/// # Returns
///
/// A [`FetchResult`] with `resolved_commit: None` and `fetched_from` set to
/// the display representation of `path`.
///
/// # Errors
///
/// Returns [`AcquisitionError::Io`] if `path` does not exist or is not a
/// directory.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::rulesets::fetch::fetch_path_source;
///
/// // A non-existent path produces an error.
/// let result = fetch_path_source("my-rules", std::path::Path::new("/nonexistent/xzardgz-test"));
/// assert!(result.is_err());
/// ```
pub fn fetch_path_source(source_id: &str, path: &Path) -> Result<FetchResult, AcquisitionError> {
    if !path.exists() {
        return Err(AcquisitionError::Io {
            path: path.display().to_string(),
            cause: format!("path source '{}' does not exist", source_id),
        });
    }
    if !path.is_dir() {
        return Err(AcquisitionError::Io {
            path: path.display().to_string(),
            cause: format!("path source '{}' is not a directory", source_id),
        });
    }
    Ok(FetchResult {
        path: path.to_path_buf(),
        resolved_commit: None,
        fetched_from: path.display().to_string(),
    })
}

// ---------------------------------------------------------------------------
// fetch_git_source
// ---------------------------------------------------------------------------

/// Clone or update a Git repository into `target_dir`.
///
/// Performs a shallow clone (`depth = 1`) of `branch` from `url`. If
/// `target_dir` already contains a repository whose HEAD matches the
/// requested `commit` (or any commit when `commit` is `None`), the
/// function returns immediately without a network fetch.
///
/// When the directory exists but does not match, it is removed and the
/// repository is cloned fresh.
///
/// Submodules are not recursed and symlinks are not followed.
///
/// # Arguments
///
/// * `source_id` - Used only in error messages for context.
/// * `url` - Repository URL (HTTPS or SSH).
/// * `branch` - Branch name to check out.
/// * `commit` - Optional pinned commit SHA. A prefix match is accepted so
///   short SHAs are supported.
/// * `target_dir` - Directory to clone into; created by libgit2 if absent.
///
/// # Returns
///
/// A [`FetchResult`] with `resolved_commit` set to the 40-char HEAD SHA.
///
/// # Errors
///
/// Returns [`AcquisitionError::Git`] if the clone or HEAD resolution fails.
/// Returns [`AcquisitionError::Io`] if a stale directory cannot be removed.
/// Returns [`AcquisitionError::Git`] if `commit` is specified but does not
/// prefix-match the resolved HEAD SHA.
pub fn fetch_git_source(
    source_id: &str,
    url: &str,
    branch: &str,
    commit: Option<&str>,
    target_dir: &Path,
) -> Result<FetchResult, AcquisitionError> {
    // Defensive check: if the target directory already has the right state,
    // skip the network fetch entirely.
    if target_dir.exists() {
        if let Ok(repo) = git2::Repository::open(target_dir)
            && let Ok(head) = repo.head()
            && let Ok(c) = head.peel_to_commit()
        {
            let sha = c.id().to_string();
            let already_good = match commit {
                Some(expected) => sha.starts_with(expected),
                None => true,
            };
            if already_good {
                return Ok(FetchResult {
                    path: target_dir.to_path_buf(),
                    resolved_commit: Some(sha),
                    fetched_from: url.to_string(),
                });
            }
        }
        // Stale directory: remove it so libgit2 can clone into a clean path.
        std::fs::remove_dir_all(target_dir).map_err(|e| AcquisitionError::Io {
            path: target_dir.display().to_string(),
            cause: format!("failed to remove stale checkout: {e}"),
        })?;
    }

    // Ensure parent directories exist before handing control to libgit2.
    if let Some(parent) = target_dir.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| AcquisitionError::Io {
            path: parent.display().to_string(),
            cause: format!("failed to create parent directory: {e}"),
        })?;
    }

    // Build fetch options requesting a shallow clone.
    let mut fetch_opts = git2::FetchOptions::new();
    fetch_opts.depth(1);

    // Clone the repository with the configured options.
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_opts);
    builder.branch(branch);
    let repo = builder
        .clone(url, target_dir)
        .map_err(|e| AcquisitionError::Git {
            source_id: source_id.to_string(),
            cause: format!(
                "failed to clone '{url}' into '{}': {e}",
                target_dir.display()
            ),
        })?;

    // Resolve the HEAD commit SHA.
    let head = repo.head().map_err(|e| AcquisitionError::Git {
        source_id: source_id.to_string(),
        cause: format!("failed to resolve HEAD: {e}"),
    })?;
    let commit_obj = head.peel_to_commit().map_err(|e| AcquisitionError::Git {
        source_id: source_id.to_string(),
        cause: format!("failed to peel HEAD to commit: {e}"),
    })?;
    let sha = commit_obj.id().to_string();

    // Verify the resolved commit matches the pinned SHA if one was specified.
    if let Some(expected) = commit
        && !sha.starts_with(expected)
    {
        return Err(AcquisitionError::Git {
            source_id: source_id.to_string(),
            cause: format!("commit mismatch: expected SHA starting with '{expected}', got '{sha}'"),
        });
    }

    Ok(FetchResult {
        path: target_dir.to_path_buf(),
        resolved_commit: Some(sha),
        fetched_from: url.to_string(),
    })
}

// ---------------------------------------------------------------------------
// count_yaml_files
// ---------------------------------------------------------------------------

/// Count `.yaml` and `.yml` files in a directory tree.
///
/// Walks `dir` recursively using [`walkdir`]. Files that do not have a
/// `.yaml` or `.yml` extension are ignored. Returns `0` when `dir` does not
/// exist or is not a directory.
///
/// Used to populate [`BundleManifest::rule_count`].
///
/// [`BundleManifest::rule_count`]: crate::clients::rulesets::manifest::BundleManifest::rule_count
///
/// # Arguments
///
/// * `dir` - Root directory to walk.
///
/// # Returns
///
/// The number of YAML files found under `dir`.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::rulesets::fetch::count_yaml_files;
///
/// // A non-existent path returns zero without panicking.
/// let count = count_yaml_files(std::path::Path::new("/nonexistent/xzardgz-test"));
/// assert_eq!(count, 0);
/// ```
pub fn count_yaml_files(dir: &Path) -> usize {
    if !dir.is_dir() {
        return 0;
    }
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            matches!(
                e.path().extension().and_then(|s| s.to_str()),
                Some("yaml") | Some("yml")
            )
        })
        .count()
}

// ---------------------------------------------------------------------------
// read_license_file
// ---------------------------------------------------------------------------

/// Attempt to read a LICENSE or COPYING file from a directory.
///
/// Checks the following filenames in order (names are case-sensitive on Unix):
///
/// | Filename       |
/// |----------------|
/// | `LICENSE`      |
/// | `LICENSE.txt`  |
/// | `LICENSE.md`   |
/// | `COPYING`      |
/// | `COPYING.txt`  |
///
/// Returns the full text content of the first file found, or `None` if none
/// of the candidates exist.
///
/// # Arguments
///
/// * `dir` - Directory to search.
///
/// # Returns
///
/// `Some(contents)` if a licence file is found and readable; `None` otherwise.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::rulesets::fetch::read_license_file;
///
/// // No licence file in a non-existent directory.
/// let result = read_license_file(std::path::Path::new("/nonexistent/xzardgz-test"));
/// assert!(result.is_none());
/// ```
pub fn read_license_file(dir: &Path) -> Option<String> {
    const CANDIDATES: &[&str] = &[
        "LICENSE",
        "LICENSE.txt",
        "LICENSE.md",
        "COPYING",
        "COPYING.txt",
    ];
    for &name in CANDIDATES {
        let path = dir.join(name);
        if path.is_file() {
            return std::fs::read_to_string(&path).ok();
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::TempDir;

    use super::*;

    fn make_tempdir() -> TempDir {
        // SAFETY: tempdir creation in tests; failure is unrecoverable test setup.
        tempfile::tempdir().expect("tempdir creation must succeed in tests")
    }

    // -----------------------------------------------------------------------
    // fetch_path_source
    // -----------------------------------------------------------------------

    #[test]
    fn test_fetch_path_source_with_existing_directory_returns_ok() {
        let dir = make_tempdir();
        let result = fetch_path_source("test-source", dir.path());
        assert!(
            result.is_ok(),
            "fetch_path_source must return Ok for an existing directory"
        );
        let fr = result.unwrap();
        assert_eq!(fr.path, dir.path(), "returned path must match the input");
        assert!(
            fr.resolved_commit.is_none(),
            "path source must have no resolved commit"
        );
    }

    #[test]
    fn test_fetch_path_source_with_missing_directory_returns_io_error() {
        let result = fetch_path_source(
            "test-source",
            Path::new("/nonexistent/xzardgz-fetch-path-test-99999"),
        );
        assert!(
            result.is_err(),
            "fetch_path_source must return Err for a missing directory"
        );
        assert!(
            matches!(result.unwrap_err(), AcquisitionError::Io { .. }),
            "error variant must be AcquisitionError::Io"
        );
    }

    // -----------------------------------------------------------------------
    // count_yaml_files
    // -----------------------------------------------------------------------

    #[test]
    fn test_count_yaml_files_with_yaml_files_returns_correct_count() {
        let dir = make_tempdir();
        for name in &["a.yaml", "b.yaml", "c.yaml"] {
            std::fs::write(dir.path().join(name), b"rules: []")
                .expect("test: failed to write yaml file");
        }
        // Also write a non-yaml file to confirm it is not counted.
        std::fs::write(dir.path().join("readme.txt"), b"ignore me")
            .expect("test: failed to write txt file");

        let count = count_yaml_files(dir.path());
        assert_eq!(count, 3, "expected exactly 3 yaml files, got {count}");
    }

    #[test]
    fn test_count_yaml_files_with_empty_directory_returns_zero() {
        let dir = make_tempdir();
        let count = count_yaml_files(dir.path());
        assert_eq!(
            count, 0,
            "expected 0 yaml files in an empty directory, got {count}"
        );
    }

    // -----------------------------------------------------------------------
    // read_license_file
    // -----------------------------------------------------------------------

    #[test]
    fn test_read_license_file_with_license_file_returns_some() {
        let dir = make_tempdir();
        let license_text = "MIT License\nPermission is hereby granted";
        let mut f = std::fs::File::create(dir.path().join("LICENSE"))
            .expect("test: failed to create LICENSE file");
        f.write_all(license_text.as_bytes())
            .expect("test: failed to write LICENSE file");

        let result = read_license_file(dir.path());
        assert!(
            result.is_some(),
            "read_license_file must return Some when a LICENSE file exists"
        );
        assert!(
            result.unwrap().contains("MIT License"),
            "returned content must contain the licence text"
        );
    }

    #[test]
    fn test_read_license_file_with_no_license_file_returns_none() {
        let dir = make_tempdir();
        let result = read_license_file(dir.path());
        assert!(
            result.is_none(),
            "read_license_file must return None when no licence file is present"
        );
    }
}
