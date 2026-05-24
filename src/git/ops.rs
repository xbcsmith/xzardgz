//! Git repository operations.
//!
//! Provides [`GitRepository`], a safe wrapper around [`git2::Repository`] that
//! exposes the operations required by the XZardgz pipeline: opening or cloning
//! a repository, querying HEAD state, detecting dirty working directories,
//! checking out branches, and collecting a full [`GitMetadata`] snapshot.
//!
//! All mutating operations are non-destructive by design:
//! - Clone uses the standard (non-force) protocol.
//! - Checkout uses [`git2::build::CheckoutBuilder::safe`], which aborts
//!   rather than overwriting local modifications.

use crate::error::{PipelineError, Result};
use crate::git::governance::{normalize_url, validate_remote_url};
use crate::git::metadata::GitMetadata;
use git2::{BranchType, Repository, StatusOptions};
use sha2::{Digest, Sha256};
use std::path::Path;

// ---------------------------------------------------------------------------
// GitRepository
// ---------------------------------------------------------------------------

/// A safe, pipeline-oriented wrapper around a [`git2::Repository`].
///
/// Construct with [`GitRepository::open`] for an existing checkout or
/// [`GitRepository::clone_repo`] to fetch from a remote. All operations that
/// could fail return [`crate::error::Result`].
pub struct GitRepository {
    repo: Repository,
}

impl GitRepository {
    // -----------------------------------------------------------------------
    // Constructors
    // -----------------------------------------------------------------------

    /// Opens an existing local git repository at `path`.
    ///
    /// # Arguments
    ///
    /// * `path` - Filesystem path to the repository working directory or its
    ///   `.git` subdirectory.
    ///
    /// # Returns
    ///
    /// A [`GitRepository`] wrapping the opened repository.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Git`] if `path` is not a valid git repository
    /// (e.g. the directory does not exist or contains no `.git` data).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::ops::GitRepository;
    ///
    /// let repo = GitRepository::open("/path/to/repo").unwrap();
    /// ```
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let repo = Repository::open(path.as_ref()).map_err(|e| {
            PipelineError::Git(format!(
                "failed to open repository at {:?}: {}",
                path.as_ref(),
                e
            ))
        })?;
        Ok(Self { repo })
    }

    /// Returns `true` if `path` contains a git repository.
    ///
    /// This function never returns an error; a failed open attempt is silently
    /// converted to `false`.
    ///
    /// # Arguments
    ///
    /// * `path` - Filesystem path to inspect.
    ///
    /// # Returns
    ///
    /// `true` if a git repository exists at `path`, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::ops::GitRepository;
    ///
    /// let is_repo = GitRepository::is_repository("/path/to/repo");
    /// assert!(is_repo);
    /// ```
    pub fn is_repository(path: impl AsRef<Path>) -> bool {
        Repository::open(path.as_ref()).is_ok()
    }

    /// Clones a remote repository from `url` into `dest_path`.
    ///
    /// The remote URL is validated by
    /// [`validate_remote_url`](crate::git::governance::validate_remote_url)
    /// before any network operation is attempted.  The clone is performed in
    /// safe (non-force) mode; no destructive flags are used.
    ///
    /// # Arguments
    ///
    /// * `url` - Remote URL to clone from (HTTPS or SSH).
    /// * `dest_path` - Local destination path for the new checkout.
    ///
    /// # Returns
    ///
    /// A [`GitRepository`] wrapping the newly cloned repository.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] if the URL fails governance
    /// validation.  Returns [`PipelineError::Git`] if the clone operation
    /// itself fails (network error, missing credentials, etc.).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::ops::GitRepository;
    ///
    /// let repo = GitRepository::clone_repo(
    ///     "https://github.com/example/repo.git",
    ///     "/tmp/cloned_repo",
    /// )
    /// .unwrap();
    /// ```
    pub fn clone_repo(url: &str, dest_path: impl AsRef<Path>) -> Result<Self> {
        validate_remote_url(url)?;
        let repo = Repository::clone(url, dest_path.as_ref())
            .map_err(|e| PipelineError::Git(format!("failed to clone '{}': {}", url, e)))?;
        Ok(Self { repo })
    }

    // -----------------------------------------------------------------------
    // Read-only queries
    // -----------------------------------------------------------------------

    /// Returns the name of the currently checked-out branch.
    ///
    /// # Returns
    ///
    /// `Some(branch_name)` when the repository is on a named branch, or
    /// `None` when HEAD is detached (e.g. after `git checkout <sha>`).
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Git`] if the HEAD reference cannot be read
    /// for any reason other than an unborn branch (empty repository).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::ops::GitRepository;
    ///
    /// let repo = GitRepository::open("/path/to/repo").unwrap();
    /// if let Some(branch) = repo.current_branch().unwrap() {
    ///     println!("on branch: {}", branch);
    /// }
    /// ```
    pub fn current_branch(&self) -> Result<Option<String>> {
        let head = match self.repo.head() {
            Ok(h) => h,
            // An unborn branch means the repo has been initialised but has no
            // commits yet; HEAD points to refs/heads/main (or similar) but the
            // ref does not exist.  Treat this as detached (None).
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => return Ok(None),
            Err(e) => {
                return Err(PipelineError::Git(format!("failed to read HEAD: {}", e)));
            }
        };

        if head.is_branch() {
            Ok(head.shorthand().map(str::to_string))
        } else {
            Ok(None)
        }
    }

    /// Returns the 40-character hexadecimal SHA-1 of the HEAD commit.
    ///
    /// # Returns
    ///
    /// The full 40-character lowercase hex SHA-1 string of the HEAD commit.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Git`] if the repository has no commits yet,
    /// if HEAD cannot be resolved, or if the HEAD reference is symbolic
    /// rather than pointing directly at an object.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::ops::GitRepository;
    ///
    /// let repo = GitRepository::open("/path/to/repo").unwrap();
    /// let sha = repo.head_commit().unwrap();
    /// assert_eq!(sha.len(), 40);
    /// ```
    pub fn head_commit(&self) -> Result<String> {
        let head = self
            .repo
            .head()
            .map_err(|e| PipelineError::Git(format!("failed to resolve HEAD: {}", e)))?;

        let oid = head.target().ok_or_else(|| {
            PipelineError::Git("HEAD is symbolic and has no direct target".to_string())
        })?;

        Ok(oid.to_string())
    }

    /// Returns `true` if the working directory has staged or unstaged changes
    /// to tracked files.
    ///
    /// Untracked-only repositories are **not** considered dirty.  Only
    /// modifications to already-tracked files (staged via `git add` or
    /// modified but unstaged) count towards the dirty flag.
    ///
    /// # Returns
    ///
    /// `true` if there are staged or unstaged modifications to tracked files,
    /// `false` if the working tree is clean (or contains only untracked files).
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Git`] if the status query fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::ops::GitRepository;
    ///
    /// let repo = GitRepository::open("/path/to/repo").unwrap();
    /// if repo.is_dirty().unwrap() {
    ///     println!("repository has uncommitted changes");
    /// }
    /// ```
    pub fn is_dirty(&self) -> Result<bool> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(false).include_ignored(false);

        let statuses = self
            .repo
            .statuses(Some(&mut opts))
            .map_err(|e| PipelineError::Git(format!("failed to query repository status: {}", e)))?;

        Ok(!statuses.is_empty())
    }

    // -----------------------------------------------------------------------
    // Mutations (non-destructive)
    // -----------------------------------------------------------------------

    /// Checks out an existing local branch by name.
    ///
    /// This is a **non-destructive** safe checkout.  It does **not** create
    /// new branches.  The branch must already exist locally.  If the working
    /// tree has conflicting modifications, git2 will return an error rather
    /// than overwriting them.
    ///
    /// # Arguments
    ///
    /// * `name` - The local branch name to check out (e.g. `"main"`).
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Git`] if the branch does not exist locally,
    /// if the reference name is invalid, if `set_head` fails, or if the safe
    /// checkout cannot complete due to local conflicts.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::ops::GitRepository;
    ///
    /// let repo = GitRepository::open("/path/to/repo").unwrap();
    /// repo.checkout_branch("main").unwrap();
    /// ```
    pub fn checkout_branch(&self, name: &str) -> Result<()> {
        let branch = self
            .repo
            .find_branch(name, BranchType::Local)
            .map_err(|e| PipelineError::Git(format!("branch '{}' not found: {}", name, e)))?;

        let refname = branch.get().name().ok_or_else(|| {
            PipelineError::Git(format!(
                "branch '{}' has an invalid UTF-8 reference name",
                name
            ))
        })?;

        self.repo
            .set_head(refname)
            .map_err(|e| PipelineError::Git(format!("failed to set HEAD to '{}': {}", name, e)))?;

        let mut checkout = git2::build::CheckoutBuilder::default();
        checkout.safe();

        self.repo
            .checkout_head(Some(&mut checkout))
            .map_err(|e| PipelineError::Git(format!("failed to checkout '{}': {}", name, e)))?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------------

    /// Returns the absolute path to the repository working directory.
    ///
    /// For non-bare repositories this is the directory that contains the
    /// `.git` subdirectory (the checkout root).  For bare repositories this
    /// falls back to the git data directory itself, though bare repositories
    /// are not a supported use-case for this wrapper.
    ///
    /// # Returns
    ///
    /// A reference to the [`Path`] of the working directory.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::ops::GitRepository;
    ///
    /// let repo = GitRepository::open("/path/to/repo").unwrap();
    /// println!("workdir: {}", repo.path().display());
    /// ```
    pub fn path(&self) -> &Path {
        // SAFETY: workdir() returns None only for bare repositories.
        // This wrapper does not support bare repositories; open() succeeds for
        // them but all subsequent operations would be meaningless.  Falling
        // back to repo.path() (the .git directory) is a safe no-op fallback.
        self.repo.workdir().unwrap_or_else(|| self.repo.path())
    }

    /// Collects all git metadata for this repository into a [`GitMetadata`]
    /// snapshot.
    ///
    /// # Arguments
    ///
    /// * `target_branch` - The branch requested by the calling workflow or
    ///   CLI.  This is recorded as-is and may differ from the currently
    ///   checked-out branch.
    ///
    /// # Returns
    ///
    /// A fully populated [`GitMetadata`] instance.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Git`] if the dirty-state query or the branch
    /// query fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::ops::GitRepository;
    ///
    /// let repo = GitRepository::open("/path/to/repo").unwrap();
    /// let meta = repo.metadata(Some("main")).unwrap();
    /// println!("branch: {:?}", meta.branch_name);
    /// ```
    pub fn metadata(&self, target_branch: Option<&str>) -> Result<GitMetadata> {
        let repository_url = self
            .repo
            .find_remote("origin")
            .ok()
            .and_then(|r| r.url().map(str::to_string));

        let repository_hash = if let Some(ref url) = repository_url {
            let normalized = normalize_url(url);
            let mut hasher = Sha256::new();
            hasher.update(normalized.as_bytes());
            let digest = hasher.finalize();
            Some(format!("{:x}", digest))
        } else {
            None
        };

        let local_repository_path = self.path().to_string_lossy().to_string();
        let branch_name = self.current_branch()?;
        let head_commit = self.head_commit().ok();
        let is_dirty = self.is_dirty()?;

        Ok(GitMetadata::new(
            repository_url,
            repository_hash,
            local_repository_path,
            branch_name,
            target_branch.map(str::to_string),
            head_commit,
            is_dirty,
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helper
    // -----------------------------------------------------------------------

    /// Creates a temporary directory, initialises a git repository inside it,
    /// and commits an empty tree so HEAD resolves to a real commit.
    ///
    /// Returns both the [`tempfile::TempDir`] (kept alive for the test
    /// duration) and the raw [`git2::Repository`] for low-level manipulation.
    fn make_test_repo() -> (tempfile::TempDir, git2::Repository) {
        let td = tempfile::tempdir().unwrap(); // SAFETY: test-only
        let repo = git2::Repository::init(td.path()).unwrap(); // SAFETY: test-only
        let sig = git2::Signature::now("Test User", "test@example.com").unwrap(); // SAFETY: test-only
        let tree_id = {
            let mut index = repo.index().unwrap(); // SAFETY: test-only
            index.write_tree().unwrap() // SAFETY: test-only
        };
        {
            // Scope tree so it is dropped before repo is moved into the return tuple.
            let tree = repo.find_tree(tree_id).unwrap(); // SAFETY: test-only
            repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
                .unwrap(); // SAFETY: test-only
        }
        (td, repo)
    }

    // -----------------------------------------------------------------------
    // open()
    // -----------------------------------------------------------------------

    #[test]
    fn test_open_valid_repository_succeeds() {
        let (td, _repo) = make_test_repo();
        let result = GitRepository::open(td.path());
        assert!(result.is_ok(), "open should succeed for a valid git repo");
    }

    #[test]
    fn test_open_non_repository_path_returns_git_error() {
        let td = tempfile::tempdir().unwrap(); // SAFETY: test-only
        let result = GitRepository::open(td.path());
        assert!(
            matches!(result, Err(PipelineError::Git(_))),
            "open should return Git error for a non-repo directory"
        );
    }

    // -----------------------------------------------------------------------
    // is_repository()
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_repository_returns_true_for_git_repo() {
        let (td, _repo) = make_test_repo();
        assert!(
            GitRepository::is_repository(td.path()),
            "is_repository should be true for an initialised repo"
        );
    }

    #[test]
    fn test_is_repository_returns_false_for_non_repo() {
        let td = tempfile::tempdir().unwrap(); // SAFETY: test-only
        assert!(
            !GitRepository::is_repository(td.path()),
            "is_repository should be false for a plain directory"
        );
    }

    // -----------------------------------------------------------------------
    // head_commit()
    // -----------------------------------------------------------------------

    #[test]
    fn test_head_commit_returns_40_char_sha_after_commit() {
        let (td, _repo) = make_test_repo();
        let git_repo = GitRepository::open(td.path()).unwrap(); // SAFETY: test-only
        let sha = git_repo.head_commit().unwrap(); // SAFETY: test-only
        assert_eq!(
            sha.len(),
            40,
            "expected a 40-char hex SHA-1, got: '{}'",
            sha
        );
    }

    #[test]
    fn test_head_commit_returns_error_on_empty_repo() {
        let td = tempfile::tempdir().unwrap(); // SAFETY: test-only
        let _raw_repo = git2::Repository::init(td.path()).unwrap(); // SAFETY: test-only
        let git_repo = GitRepository::open(td.path()).unwrap(); // SAFETY: test-only
        let result = git_repo.head_commit();
        assert!(
            result.is_err(),
            "head_commit should error on a repo with no commits"
        );
    }

    // -----------------------------------------------------------------------
    // current_branch()
    // -----------------------------------------------------------------------

    #[test]
    fn test_current_branch_returns_master_or_main_on_fresh_repo() {
        let (td, _repo) = make_test_repo();
        let git_repo = GitRepository::open(td.path()).unwrap(); // SAFETY: test-only
        let branch = git_repo.current_branch().unwrap(); // SAFETY: test-only
        assert!(
            branch.is_some(),
            "a fresh repo with one commit should have a branch name"
        );
    }

    // -----------------------------------------------------------------------
    // is_dirty()
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_dirty_returns_false_on_clean_repo() {
        let (td, _repo) = make_test_repo();
        let git_repo = GitRepository::open(td.path()).unwrap(); // SAFETY: test-only
        assert!(
            !git_repo.is_dirty().unwrap(), // SAFETY: test-only
            "a freshly committed repo should not be dirty"
        );
    }

    #[test]
    fn test_is_dirty_returns_true_with_staged_changes() {
        let (td, raw_repo) = make_test_repo();

        // Write a new file and stage it.
        let file_path = td.path().join("test.txt");
        std::fs::write(&file_path, "hello").unwrap(); // SAFETY: test-only
        let mut index = raw_repo.index().unwrap(); // SAFETY: test-only
        index.add_path(std::path::Path::new("test.txt")).unwrap(); // SAFETY: test-only
        index.write().unwrap(); // SAFETY: test-only

        let git_repo = GitRepository::open(td.path()).unwrap(); // SAFETY: test-only
        assert!(
            git_repo.is_dirty().unwrap(), // SAFETY: test-only
            "repo with a staged file should be dirty"
        );
    }

    #[test]
    fn test_is_dirty_returns_false_with_only_untracked_file() {
        let (td, _repo) = make_test_repo();

        // Write a file without staging it.
        let file_path = td.path().join("untracked.txt");
        std::fs::write(&file_path, "not staged").unwrap(); // SAFETY: test-only

        let git_repo = GitRepository::open(td.path()).unwrap(); // SAFETY: test-only
        assert!(
            !git_repo.is_dirty().unwrap(), // SAFETY: test-only
            "repo with only an untracked file should not be dirty"
        );
    }

    // -----------------------------------------------------------------------
    // checkout_branch()
    // -----------------------------------------------------------------------

    #[test]
    fn test_checkout_branch_returns_error_for_nonexistent_branch() {
        let (td, _repo) = make_test_repo();
        let git_repo = GitRepository::open(td.path()).unwrap(); // SAFETY: test-only
        let result = git_repo.checkout_branch("no-such-branch");
        assert!(
            matches!(result, Err(PipelineError::Git(_))),
            "checkout_branch should return Git error for a missing branch"
        );
    }

    // -----------------------------------------------------------------------
    // path()
    // -----------------------------------------------------------------------

    #[test]
    fn test_path_returns_repository_workdir() {
        let (td, _repo) = make_test_repo();
        let git_repo = GitRepository::open(td.path()).unwrap(); // SAFETY: test-only
        let workdir = git_repo.path();

        // Canonicalize both paths before comparing so that macOS symlinks
        // (e.g. /var -> /private/var inside tempdir paths) do not cause a
        // spurious mismatch.  Both paths are guaranteed to exist at this point.
        // SAFETY: test-only, paths are created by tempfile and git2.
        let wd_canonical = workdir.canonicalize().unwrap();
        let td_canonical = td.path().canonicalize().unwrap();
        assert_eq!(
            wd_canonical,
            td_canonical,
            "workdir '{}' should match the tempdir '{}'",
            workdir.display(),
            td.path().display()
        );
    }

    // -----------------------------------------------------------------------
    // metadata()
    // -----------------------------------------------------------------------

    #[test]
    fn test_metadata_contains_local_path() {
        let (td, _repo) = make_test_repo();
        let git_repo = GitRepository::open(td.path()).unwrap(); // SAFETY: test-only
        let meta = git_repo.metadata(Some("main")).unwrap(); // SAFETY: test-only

        assert!(
            !meta.local_repository_path.is_empty(),
            "metadata local_repository_path must not be empty"
        );
        let td_str = td.path().to_string_lossy();
        assert!(
            meta.local_repository_path
                .trim_end_matches('/')
                .contains(td_str.trim_end_matches('/')),
            "local_repository_path '{}' should contain the tempdir '{}'",
            meta.local_repository_path,
            td_str
        );
    }
}
