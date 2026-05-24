//! Git metadata types for repository state.
//!
//! Provides [`GitMetadata`], which carries all relevant git information
//! collected from a repository checkout. This struct is populated by
//! [`crate::git::ops::GitRepository::metadata`] and consumed by the workspace
//! state builder and scan artifact pipeline stages.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// GitMetadata
// ---------------------------------------------------------------------------

/// All git-related metadata collected from a single repository checkout.
///
/// Every field that may be absent (no remote, no commits, detached HEAD) is
/// modelled as `Option<String>` so callers can distinguish "not present" from
/// "empty string".  The `is_dirty` flag follows the same semantics as
/// `git status --porcelain` with untracked files excluded.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::git::metadata::GitMetadata;
///
/// let meta = GitMetadata::new(
///     Some("https://github.com/example/repo.git".to_string()),
///     Some("abc123hash".to_string()),
///     "/home/user/projects/repo".to_string(),
///     Some("main".to_string()),
///     Some("main".to_string()),
///     Some("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string()),
///     false,
/// );
///
/// assert_eq!(meta.local_repository_path, "/home/user/projects/repo");
/// assert!(!meta.is_dirty);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitMetadata {
    /// Remote origin URL of the repository.
    ///
    /// `None` for purely local repositories that have no configured remote
    /// (i.e. `git remote get-url origin` would fail).
    pub repository_url: Option<String>,

    /// SHA-256 hex digest of the normalized remote origin URL.
    ///
    /// Computed by normalizing the URL (stripping credentials, trailing
    /// slashes, and `.git` suffix) then hashing with SHA-256.
    /// `None` when [`repository_url`](Self::repository_url) is `None`.
    pub repository_hash: Option<String>,

    /// Absolute path to the local working directory of the repository.
    ///
    /// Always populated; defaults to an empty string via [`Default`].
    pub local_repository_path: String,

    /// Name of the currently checked-out branch.
    ///
    /// `None` when the repository is in detached-HEAD state (e.g. after a
    /// tag checkout or a `git checkout <sha>` invocation).
    pub branch_name: Option<String>,

    /// Branch requested by the calling workflow or CLI invocation.
    ///
    /// This may differ from [`branch_name`](Self::branch_name) when, for
    /// example, the workflow requests `main` but the checkout is currently on
    /// a feature branch.
    pub target_branch: Option<String>,

    /// 40-character hexadecimal SHA-1 of the HEAD commit.
    ///
    /// `None` when the repository has no commits yet (a freshly initialised
    /// repository with no history).
    pub head_commit: Option<String>,

    /// Whether the working directory contains staged or unstaged changes to
    /// tracked files.
    ///
    /// Untracked-only repositories are **not** considered dirty; only
    /// modifications to already-tracked files (staged via `git add` or
    /// modified but unstaged) set this flag to `true`.
    pub is_dirty: bool,
}

impl GitMetadata {
    /// Creates a new [`GitMetadata`] with all fields explicitly specified.
    ///
    /// # Arguments
    ///
    /// * `repository_url` - Remote origin URL, or `None` for local-only
    ///   repositories.
    /// * `repository_hash` - SHA-256 hex digest of the normalized URL, or
    ///   `None` when no remote is configured.
    /// * `local_repository_path` - Absolute path to the working directory.
    /// * `branch_name` - Current branch name, or `None` in detached-HEAD
    ///   state.
    /// * `target_branch` - Branch requested by the workflow or CLI, or
    ///   `None`.
    /// * `head_commit` - 40-char hex SHA-1 of HEAD, or `None` if the
    ///   repository has no commits.
    /// * `is_dirty` - `true` if tracked files have staged or unstaged
    ///   changes.
    ///
    /// # Returns
    ///
    /// A fully populated [`GitMetadata`] instance.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::metadata::GitMetadata;
    ///
    /// let meta = GitMetadata::new(
    ///     None,
    ///     None,
    ///     "/tmp/my_repo".to_string(),
    ///     Some("feature-x".to_string()),
    ///     None,
    ///     Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string()),
    ///     true,
    /// );
    ///
    /// assert!(meta.is_dirty);
    /// assert_eq!(meta.branch_name.as_deref(), Some("feature-x"));
    /// ```
    pub fn new(
        repository_url: Option<String>,
        repository_hash: Option<String>,
        local_repository_path: String,
        branch_name: Option<String>,
        target_branch: Option<String>,
        head_commit: Option<String>,
        is_dirty: bool,
    ) -> Self {
        Self {
            repository_url,
            repository_hash,
            local_repository_path,
            branch_name,
            target_branch,
            head_commit,
            is_dirty,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_metadata_new_sets_all_fields() {
        let url = Some("https://github.com/example/repo.git".to_string());
        let hash = Some("abc123deadbeef".to_string());
        let path = "/tmp/repo".to_string();
        let branch = Some("main".to_string());
        let target = Some("main".to_string());
        let commit = Some("a".repeat(40));
        let dirty = true;

        let meta = GitMetadata::new(
            url.clone(),
            hash.clone(),
            path.clone(),
            branch.clone(),
            target.clone(),
            commit.clone(),
            dirty,
        );

        assert_eq!(meta.repository_url, url);
        assert_eq!(meta.repository_hash, hash);
        assert_eq!(meta.local_repository_path, path);
        assert_eq!(meta.branch_name, branch);
        assert_eq!(meta.target_branch, target);
        assert_eq!(meta.head_commit, commit);
        assert_eq!(meta.is_dirty, dirty);
    }

    #[test]
    fn test_git_metadata_default_has_empty_string_path() {
        let meta = GitMetadata::default();

        assert_eq!(meta.local_repository_path, "");
        assert!(meta.repository_url.is_none());
        assert!(meta.repository_hash.is_none());
        assert!(meta.branch_name.is_none());
        assert!(meta.target_branch.is_none());
        assert!(meta.head_commit.is_none());
        assert!(!meta.is_dirty);
    }

    #[test]
    fn test_git_metadata_serializes_to_yaml() {
        let meta = GitMetadata::new(
            Some("https://github.com/example/repo.git".to_string()),
            Some("deadbeefcafe1234".to_string()),
            "/tmp/repo".to_string(),
            Some("main".to_string()),
            Some("main".to_string()),
            Some("a".repeat(40)),
            false,
        );

        // SAFETY: test-only, struct is known-valid and serde_yaml supports it.
        let yaml = serde_yaml::to_string(&meta).expect("serialization must succeed");

        assert!(yaml.contains("repository_url"));
        assert!(yaml.contains("local_repository_path"));
        assert!(yaml.contains("is_dirty"));
        assert!(yaml.contains("/tmp/repo"));
        assert!(yaml.contains("https://github.com/example/repo.git"));

        // Round-trip: deserialize back and verify key fields survive intact.
        // SAFETY: test-only, yaml was just produced by a known-valid struct.
        let restored: GitMetadata =
            serde_yaml::from_str(&yaml).expect("deserialization must succeed");
        assert_eq!(restored.local_repository_path, "/tmp/repo");
        assert_eq!(
            restored.repository_url.as_deref(),
            Some("https://github.com/example/repo.git")
        );
        assert_eq!(
            restored.repository_hash.as_deref(),
            Some("deadbeefcafe1234")
        );
        assert!(!restored.is_dirty);
    }
}
