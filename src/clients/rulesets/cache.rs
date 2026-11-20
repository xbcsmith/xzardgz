//! Content-addressed on-disk cache for fetched rulesets.
//!
//! Rulesets are cached under `<cache_root>/<source_id>/<commit>/`. A cache
//! entry is considered valid when that directory exists and contains a
//! `bundle_manifest.yaml` file written by [`write_cached_manifest`].
//!
//! # Boundary contract
//!
//! This module MUST NOT import from `src/scanner/sast/`. It operates only
//! on bytes, paths, and its own types.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::clients::rulesets::error::AcquisitionError;
use crate::clients::rulesets::manifest::BundleManifest;

// ---------------------------------------------------------------------------
// Cache root
// ---------------------------------------------------------------------------

/// Returns the cache directory root for xzardgz rulesets.
///
/// The path is constructed as `<cache_dir>/xzardgz/rulesets/` where
/// `<cache_dir>` is obtained from [`dirs::cache_dir`]. If `dirs::cache_dir`
/// returns `None`, the function falls back to `$HOME/.cache` via
/// [`dirs::home_dir`], and finally to a relative `.cache` directory.
///
/// # Returns
///
/// A [`PathBuf`] pointing at the xzardgz rulesets cache directory. The
/// directory is not created by this function.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::rulesets::cache::rulesets_cache_root;
///
/// let root = rulesets_cache_root();
/// // The tail components are always the same regardless of OS.
/// assert!(root.ends_with("xzardgz/rulesets"));
/// ```
pub fn rulesets_cache_root() -> PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".cache")
    });
    base.join("xzardgz").join("rulesets")
}

// ---------------------------------------------------------------------------
// cache_path_for
// ---------------------------------------------------------------------------

/// Returns the cache directory path for a specific source at a specific commit.
///
/// Path: `<cache_root>/<source_id>/<commit>/`
///
/// The directory is not created by this function.
///
/// # Arguments
///
/// * `source_id` - The source identifier from the sources manifest.
/// * `commit` - The resolved git commit SHA (or any string key for non-git sources).
///
/// # Returns
///
/// A [`PathBuf`] for the cache directory of `(source_id, commit)`.
pub fn cache_path_for(source_id: &str, commit: &str) -> PathBuf {
    rulesets_cache_root().join(source_id).join(commit)
}

// ---------------------------------------------------------------------------
// is_cached
// ---------------------------------------------------------------------------

/// Returns `true` when a valid cached copy exists for `(source_id, commit)`.
///
/// A cached copy is valid when the directory returned by [`cache_path_for`]
/// exists and contains a `bundle_manifest.yaml` file.
///
/// # Arguments
///
/// * `source_id` - The source identifier from the sources manifest.
/// * `commit` - The resolved git commit SHA.
///
/// # Returns
///
/// `true` if the cache entry is present and valid; `false` otherwise.
pub fn is_cached(source_id: &str, commit: &str) -> bool {
    let path = cache_path_for(source_id, commit);
    path.is_dir() && path.join("bundle_manifest.yaml").is_file()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read a [`BundleManifest`] from the given cache directory.
///
/// Returns `None` if the `bundle_manifest.yaml` file is absent or cannot
/// be parsed.
fn read_manifest_from_dir(dir: &Path) -> Option<BundleManifest> {
    let path = dir.join("bundle_manifest.yaml");
    let content = std::fs::read_to_string(&path).ok()?;
    serde_yaml::from_str(&content).ok()
}

/// Serialise `manifest` to `bundle_manifest.yaml` inside `dir`.
///
/// Creates `dir` and any missing parent directories. Returns
/// [`AcquisitionError::Io`] if the directory or file cannot be written, and
/// [`AcquisitionError::ManifestParse`] if serialization fails.
fn write_manifest_to_dir(dir: &Path, manifest: &BundleManifest) -> Result<(), AcquisitionError> {
    std::fs::create_dir_all(dir).map_err(|e| AcquisitionError::Io {
        path: dir.display().to_string(),
        cause: format!("failed to create cache directory: {e}"),
    })?;
    let content = serde_yaml::to_string(manifest)
        .map_err(|e| AcquisitionError::ManifestParse(e.to_string()))?;
    let manifest_path = dir.join("bundle_manifest.yaml");
    std::fs::write(&manifest_path, content).map_err(|e| AcquisitionError::Io {
        path: manifest_path.display().to_string(),
        cause: format!("failed to write bundle_manifest.yaml: {e}"),
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// read_cached_manifest
// ---------------------------------------------------------------------------

/// Read and deserialise the [`BundleManifest`] from the cache.
///
/// Returns `None` if the cache entry does not exist or the manifest cannot
/// be parsed.
///
/// # Arguments
///
/// * `source_id` - The source identifier from the sources manifest.
/// * `commit` - The resolved git commit SHA.
///
/// # Returns
///
/// `Some(manifest)` on success, `None` if absent or parse fails.
pub fn read_cached_manifest(source_id: &str, commit: &str) -> Option<BundleManifest> {
    let dir = cache_path_for(source_id, commit);
    read_manifest_from_dir(&dir)
}

// ---------------------------------------------------------------------------
// write_cached_manifest
// ---------------------------------------------------------------------------

/// Write a [`BundleManifest`] to the cache directory for `(source_id, commit)`.
///
/// Creates the cache directory and any missing parents. The manifest is
/// written to `bundle_manifest.yaml` within the directory.
///
/// # Arguments
///
/// * `source_id` - The source identifier from the sources manifest.
/// * `commit` - The resolved git commit SHA.
/// * `manifest` - The manifest to persist.
///
/// # Errors
///
/// Returns [`AcquisitionError::Io`] if the directory or file cannot be
/// created. Returns [`AcquisitionError::ManifestParse`] if the manifest
/// cannot be serialized.
pub fn write_cached_manifest(
    source_id: &str,
    commit: &str,
    manifest: &BundleManifest,
) -> Result<(), AcquisitionError> {
    let dir = cache_path_for(source_id, commit);
    write_manifest_to_dir(&dir, manifest)
}

// ---------------------------------------------------------------------------
// compute_directory_digest
// ---------------------------------------------------------------------------

/// Compute a SHA-256 hex digest of a directory's contents.
///
/// Walks the directory recursively, collects all file paths, sorts them by
/// relative path for determinism, then feeds the concatenation of
/// `<relative_path>\0<file_bytes>` for each file into a SHA-256 hasher.
///
/// # Arguments
///
/// * `dir` - Root directory to hash.
///
/// # Returns
///
/// A 64-character lowercase hex string representing the SHA-256 digest.
///
/// # Errors
///
/// Returns [`AcquisitionError::Io`] if a directory entry cannot be read or
/// a file cannot be opened.
pub fn compute_directory_digest(dir: &Path) -> Result<String, AcquisitionError> {
    // Collect all file paths first so we can sort them for determinism.
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry.map_err(|e| AcquisitionError::Io {
            path: dir.display().to_string(),
            cause: format!("directory walk error: {e}"),
        })?;
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort();

    let mut hasher = Sha256::new();
    for path in &files {
        let rel = path
            .strip_prefix(dir)
            .map_err(|e| AcquisitionError::Io {
                path: path.display().to_string(),
                cause: format!("path strip error: {e}"),
            })?
            .to_string_lossy();
        let content = std::fs::read(path).map_err(|e| AcquisitionError::Io {
            path: path.display().to_string(),
            cause: format!("failed to read file: {e}"),
        })?;
        hasher.update(rel.as_bytes());
        hasher.update(b"\0");
        hasher.update(&content);
    }

    let result = hasher.finalize();
    let hex: String = result.iter().map(|b| format!("{b:02x}")).collect();
    Ok(hex)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::clients::rulesets::license::LicenseClass;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_tempdir() -> TempDir {
        // SAFETY: tempdir creation in tests; failure is unrecoverable test setup.
        tempfile::tempdir().expect("tempdir creation must succeed in tests")
    }

    fn make_test_manifest() -> BundleManifest {
        BundleManifest {
            source_id: "test-source".to_string(),
            resolved_commit: Some("abc123def456".to_string()),
            declared_license: Some("MIT".to_string()),
            observed_license: None,
            license_class: LicenseClass::Permissive,
            content_digest: "deadbeef".to_string(),
            rule_count: 5,
            fetched_at: "2024-01-01T00:00:00Z".to_string(),
            allowed_by_policy: true,
        }
    }

    /// Return a cache path rooted at `root` rather than the system cache dir.
    ///
    /// Used in tests to avoid touching real OS directories.
    fn cache_path_for_with_root(root: &Path, source_id: &str, commit: &str) -> PathBuf {
        root.join(source_id).join(commit)
    }

    // -----------------------------------------------------------------------
    // is_cached
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_is_cached_returns_false_when_not_present() {
        // A commit SHA that is astronomically unlikely to be in any real cache.
        let result = is_cached(
            "xzardgz-test-nonexistent-source-99999",
            "0000000000000000000000000000000000000000",
        );
        assert!(
            !result,
            "is_cached must return false for a non-existent cache entry"
        );
    }

    // -----------------------------------------------------------------------
    // write / read roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_write_then_read_manifest_roundtrip() {
        let tmp = make_tempdir();
        let cache_dir = cache_path_for_with_root(tmp.path(), "test-source", "abc123");
        let manifest = make_test_manifest();

        write_manifest_to_dir(&cache_dir, &manifest).expect("write_manifest_to_dir must succeed");

        let restored =
            read_manifest_from_dir(&cache_dir).expect("read_manifest_from_dir must return Some");

        assert_eq!(
            restored.source_id, manifest.source_id,
            "source_id must round-trip correctly"
        );
        assert_eq!(
            restored.rule_count, manifest.rule_count,
            "rule_count must round-trip correctly"
        );
        assert_eq!(
            restored.allowed_by_policy, manifest.allowed_by_policy,
            "allowed_by_policy must round-trip correctly"
        );
        assert_eq!(
            restored.content_digest, manifest.content_digest,
            "content_digest must round-trip correctly"
        );
    }

    // -----------------------------------------------------------------------
    // compute_directory_digest
    // -----------------------------------------------------------------------

    #[test]
    fn test_compute_directory_digest_same_contents_produces_same_digest() {
        let dir1 = make_tempdir();
        let dir2 = make_tempdir();

        std::fs::write(dir1.path().join("rules.yaml"), b"rules: []")
            .expect("test: write dir1 file must succeed");
        std::fs::write(dir2.path().join("rules.yaml"), b"rules: []")
            .expect("test: write dir2 file must succeed");

        let digest1 = compute_directory_digest(dir1.path()).expect("digest must succeed for dir1");
        let digest2 = compute_directory_digest(dir2.path()).expect("digest must succeed for dir2");

        assert_eq!(
            digest1, digest2,
            "identical directory contents must produce identical digests"
        );
    }

    #[test]
    fn test_compute_directory_digest_different_contents_produces_different_digest() {
        let dir1 = make_tempdir();
        let dir2 = make_tempdir();

        std::fs::write(dir1.path().join("rules.yaml"), b"rules: [one]")
            .expect("test: write dir1 file must succeed");
        std::fs::write(dir2.path().join("rules.yaml"), b"rules: [two]")
            .expect("test: write dir2 file must succeed");

        let digest1 = compute_directory_digest(dir1.path()).expect("digest must succeed for dir1");
        let digest2 = compute_directory_digest(dir2.path()).expect("digest must succeed for dir2");

        assert_ne!(
            digest1, digest2,
            "different directory contents must produce different digests"
        );
    }
}
