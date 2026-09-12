//! Write operations for git repositories.
//!
//! Provides [`GitWriteRepository`], which is distinct from the read-only
//! [`crate::git::ops::GitRepository`] surface to preserve the scan-only
//! sandbox boundary.  Plugins that open a repository for scanning always
//! use `GitRepository`; only the workflow executor's write stage uses
//! `GitWriteRepository`.
//!
//! # Credential resolution for push
//!
//! Push authentication is attempted in the following order:
//!
//! 1. `XZARDGZ_GITHUB_TOKEN` environment variable.
//! 2. OS keyring key `github_token` in service `xzardgz-github`.
//! 3. SSH agent (for SSH remotes).
//!
//! For `file://` remotes (e.g., in tests) no credentials are requested.

use crate::auth::store::{EnvVarStore, KeyringStore, SecretStore};
use crate::error::{PipelineError, Result};
use crate::git::governance::validate_branch_name;
use std::path::Path;

// ---------------------------------------------------------------------------
// GitWriteRepository
// ---------------------------------------------------------------------------

/// A write-capable wrapper around a [`git2::Repository`].
///
/// Distinct from the read-only [`crate::git::ops::GitRepository`] to preserve
/// the scan-only sandbox boundary.  Use this struct only within the workflow
/// executor's write stage; plugins that only scan a repository must use
/// [`crate::git::ops::GitRepository`] instead.
///
/// Construct with [`GitWriteRepository::open`].
pub struct GitWriteRepository {
    repo: git2::Repository,
    author_name: String,
    author_email: String,
}

impl GitWriteRepository {
    // -----------------------------------------------------------------------
    // Constructors
    // -----------------------------------------------------------------------

    /// Opens an existing local git repository at `path` for write operations.
    ///
    /// # Arguments
    ///
    /// * `path` - Filesystem path to the repository working directory or its
    ///   `.git` subdirectory.
    /// * `author_name` - Git author name used when creating commits.
    /// * `author_email` - Git author email used when creating commits.
    ///
    /// # Returns
    ///
    /// A [`GitWriteRepository`] wrapping the opened repository.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Git`] if `path` is not a valid git repository
    /// (e.g. the directory does not exist or contains no `.git` data).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::write::GitWriteRepository;
    ///
    /// let repo = GitWriteRepository::open(
    ///     "/path/to/repo",
    ///     "Alice",
    ///     "alice@example.com",
    /// )
    /// .unwrap();
    /// ```
    pub fn open(
        path: impl AsRef<Path>,
        author_name: impl Into<String>,
        author_email: impl Into<String>,
    ) -> Result<Self> {
        let repo = git2::Repository::open(path.as_ref()).map_err(|e| {
            PipelineError::Git(format!(
                "failed to open repository at {:?}: {}",
                path.as_ref(),
                e
            ))
        })?;
        Ok(Self {
            repo,
            author_name: author_name.into(),
            author_email: author_email.into(),
        })
    }

    // -----------------------------------------------------------------------
    // Naming helpers
    // -----------------------------------------------------------------------

    /// Returns a unique default branch name of the form `xzardgz/<ulid>`.
    ///
    /// The ULID component is lowercased so the branch name passes git's
    /// reference-format rules without modification.  Each call produces a
    /// distinct value because ULIDs encode a millisecond-precision timestamp
    /// plus 80 bits of cryptographic randomness.
    ///
    /// # Returns
    ///
    /// A `String` such as `"xzardgz/01aryz6s41tptp2x5e1z8v1xy8"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::git::write::GitWriteRepository;
    ///
    /// let name = GitWriteRepository::default_branch_name();
    /// assert!(name.starts_with("xzardgz/"));
    /// assert_eq!(name.len(), "xzardgz/".len() + 26);
    /// ```
    pub fn default_branch_name() -> String {
        format!("xzardgz/{}", ulid::Ulid::new().to_string().to_lowercase())
    }

    // -----------------------------------------------------------------------
    // Branch operations
    // -----------------------------------------------------------------------

    /// Creates a new local branch at the current HEAD commit and switches HEAD
    /// to point at that branch.
    ///
    /// The branch name is validated with
    /// [`validate_branch_name`](crate::git::governance::validate_branch_name)
    /// before any git operation is attempted.  The branch is created with
    /// `force = false`, so attempting to create a branch that already exists
    /// returns an error rather than silently overwriting the existing ref.
    ///
    /// # Arguments
    ///
    /// * `name` - The branch name to create, e.g. `"feature/my-work"`.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] if `name` fails git ref-format
    /// validation.
    ///
    /// Returns [`PipelineError::Git`] if:
    /// - The repository has no commits (HEAD is unborn).
    /// - A branch named `name` already exists.
    /// - Any underlying libgit2 operation fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::write::GitWriteRepository;
    ///
    /// let repo = GitWriteRepository::open(
    ///     "/path/to/repo",
    ///     "Alice",
    ///     "alice@example.com",
    /// )
    /// .unwrap();
    /// repo.create_branch("feature/my-work").unwrap();
    /// assert_eq!(
    ///     repo.current_branch().unwrap(),
    ///     Some("feature/my-work".to_string()),
    /// );
    /// ```
    pub fn create_branch(&self, name: &str) -> Result<()> {
        validate_branch_name(name)?;

        let parent_oid = self
            .repo
            .head()
            .ok()
            .and_then(|h| h.target())
            .ok_or_else(|| {
                PipelineError::Git("no commits in repository; cannot create branch".to_string())
            })?;

        let commit = self
            .repo
            .find_commit(parent_oid)
            .map_err(|e| PipelineError::Git(format!("failed to find HEAD commit: {}", e)))?;

        self.repo.branch(name, &commit, false).map_err(|e| {
            PipelineError::Git(format!("failed to create branch '{}': {}", name, e))
        })?;

        self.repo
            .set_head(&format!("refs/heads/{}", name))
            .map_err(|e| PipelineError::Git(format!("failed to set HEAD to '{}': {}", name, e)))?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Commit operations
    // -----------------------------------------------------------------------

    /// Stages the given paths and creates a commit on the current branch.
    ///
    /// Each path may be either relative to the repository workdir, or an
    /// absolute path that resides inside the workdir.  Absolute paths are
    /// stripped of the workdir prefix before being handed to the index,
    /// matching the behaviour of `git add <path>`.
    ///
    /// The commit uses the `author_name` and `author_email` provided at
    /// construction time for both the author and committer signatures.
    ///
    /// # Arguments
    ///
    /// * `paths` - Files to stage.
    /// * `message` - The commit message.
    ///
    /// # Returns
    ///
    /// The full 40-character hexadecimal OID string of the new commit.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Git`] if the repository is bare, if any path
    /// cannot be staged, if tree or signature construction fails, or if the
    /// underlying commit operation fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::write::GitWriteRepository;
    ///
    /// let repo = GitWriteRepository::open(
    ///     "/path/to/repo",
    ///     "Alice",
    ///     "alice@example.com",
    /// )
    /// .unwrap();
    /// let oid = repo.commit_paths(&["README.md"], "initial commit").unwrap();
    /// assert_eq!(oid.len(), 40);
    /// ```
    pub fn commit_paths(&self, paths: &[impl AsRef<Path>], message: &str) -> Result<String> {
        let workdir = self
            .repo
            .workdir()
            .ok_or_else(|| PipelineError::Git("repository is bare; cannot commit".to_string()))?
            .to_path_buf();

        // On macOS, /var is a symlink to /private/var. git2 resolves the workdir
        // to the canonical (real) path while tempfile and std::env produce the
        // unresolved symlink path. Canonicalise once so that strip_prefix
        // compares real paths on every platform.
        let canonical_workdir = std::fs::canonicalize(&workdir).unwrap_or(workdir.clone());

        let mut index = self
            .repo
            .index()
            .map_err(|e| PipelineError::Git(format!("failed to get index: {}", e)))?;

        for p in paths {
            let path = p.as_ref();
            let rel_path = if path.is_absolute() {
                // Resolve symlinks in the supplied path so that the prefix
                // comparison is reliable even when the caller used a symlinked
                // temporary directory (common on macOS).
                let canonical_path =
                    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
                canonical_path
                    .strip_prefix(&canonical_workdir)
                    .map(|rel| rel.to_path_buf())
                    .map_err(|_| {
                        PipelineError::Git(format!(
                            "path {:?} is not inside workdir {:?}",
                            path, canonical_workdir
                        ))
                    })?
            } else {
                path.to_path_buf()
            };
            index.add_path(&rel_path).map_err(|e| {
                PipelineError::Git(format!("failed to stage {:?}: {}", rel_path, e))
            })?;
        }

        index
            .write()
            .map_err(|e| PipelineError::Git(format!("failed to write index: {}", e)))?;

        let tree_id = index
            .write_tree()
            .map_err(|e| PipelineError::Git(format!("failed to write tree: {}", e)))?;

        let tree = self
            .repo
            .find_tree(tree_id)
            .map_err(|e| PipelineError::Git(format!("failed to find tree: {}", e)))?;

        let sig = git2::Signature::now(&self.author_name, &self.author_email)
            .map_err(|e| PipelineError::Git(format!("failed to create signature: {}", e)))?;

        // Resolve the parent commit while avoiding lifetime issues: extract the
        // OID first so that the temporary `Reference` returned by `head()` is
        // dropped before `find_commit` borrows the repository again.
        let parent_oid = self.repo.head().ok().and_then(|h| h.target());
        let parent_commit =
            if let Some(oid) = parent_oid {
                Some(self.repo.find_commit(oid).map_err(|e| {
                    PipelineError::Git(format!("failed to find parent commit: {}", e))
                })?)
            } else {
                None
            };
        let parents: &[&git2::Commit] = match parent_commit.as_ref() {
            Some(c) => &[c],
            None => &[],
        };

        let oid = self
            .repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, parents)
            .map_err(|e| PipelineError::Git(format!("failed to commit: {}", e)))?;

        Ok(oid.to_string())
    }

    // -----------------------------------------------------------------------
    // Remote operations
    // -----------------------------------------------------------------------

    /// Pushes branch `name` to the `origin` remote.
    ///
    /// The branch name is validated with
    /// [`validate_branch_name`](crate::git::governance::validate_branch_name)
    /// before any network operation is attempted.
    ///
    /// Authentication is resolved in the following order:
    ///
    /// 1. `XZARDGZ_GITHUB_TOKEN` environment variable
    ///    (`USER_PASS_PLAINTEXT` credential type with username `x-access-token`).
    /// 2. OS keyring key `github_token` in service `xzardgz-github`
    ///    (`USER_PASS_PLAINTEXT` credential type).
    /// 3. SSH agent (`SSH_KEY` credential type).
    /// 4. libgit2 default credential (e.g. Kerberos/NTLM).
    ///
    /// For `file://` remotes (used in tests) libgit2 does not request
    /// credentials at all, so none of the above steps are invoked.
    ///
    /// # Arguments
    ///
    /// * `name` - The local branch name to push.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] if `name` fails ref-format
    /// validation.
    ///
    /// Returns [`PipelineError::Git`] if the `origin` remote cannot be found,
    /// if authentication fails, or if the push itself fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::write::GitWriteRepository;
    ///
    /// let repo = GitWriteRepository::open(
    ///     "/path/to/repo",
    ///     "Alice",
    ///     "alice@example.com",
    /// )
    /// .unwrap();
    /// repo.push_branch("feature/my-work").unwrap();
    /// ```
    pub fn push_branch(&self, name: &str) -> Result<()> {
        validate_branch_name(name)?;

        let mut remote = self
            .repo
            .find_remote("origin")
            .map_err(|e| PipelineError::Git(format!("failed to find remote 'origin': {}", e)))?;

        let refspec = format!("refs/heads/{0}:refs/heads/{0}", name);
        let token = resolve_github_token();

        let mut callbacks = git2::RemoteCallbacks::new();
        callbacks.credentials(move |_url, username_from_url, allowed_types| {
            if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT)
                && let Some(ref t) = token
            {
                return git2::Cred::userpass_plaintext("x-access-token", t);
            }
            if allowed_types.contains(git2::CredentialType::SSH_KEY) {
                return git2::Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"));
            }
            git2::Cred::default()
        });

        let mut push_opts = git2::PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        remote
            .push(&[&refspec], Some(&mut push_opts))
            .map_err(|e| {
                PipelineError::Git(format!("failed to push '{}' to 'origin': {}", name, e))
            })?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Query operations
    // -----------------------------------------------------------------------

    /// Returns the name of the currently checked-out branch, or `None` if
    /// HEAD is detached or the repository has no commits yet (unborn branch).
    ///
    /// Mirrors the implementation of
    /// [`crate::git::ops::GitRepository::current_branch`] exactly, so both
    /// surfaces return consistent results for the same repository state.
    ///
    /// # Returns
    ///
    /// `Some(branch_name)` when HEAD points at a named branch, or `None` for
    /// detached HEAD or unborn branch state.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Git`] if the HEAD reference cannot be read
    /// for any reason other than an unborn branch.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::git::write::GitWriteRepository;
    ///
    /// let repo = GitWriteRepository::open(
    ///     "/path/to/repo",
    ///     "Alice",
    ///     "alice@example.com",
    /// )
    /// .unwrap();
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
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Resolves a GitHub personal access token from the configured secret stores.
///
/// Tries the `XZARDGZ_GITHUB_TOKEN` environment variable first (via
/// [`EnvVarStore`]), then the OS keyring entry `github_token` in service
/// `xzardgz-github` (via [`KeyringStore`]).
///
/// Returns `None` when neither store contains a token.
fn resolve_github_token() -> Option<String> {
    let store = EnvVarStore::new("XZARDGZ_");
    if let Ok(Some(t)) = store.get_secret("github_token") {
        return Some(t);
    }
    let ks = KeyringStore::new("xzardgz-github");
    ks.get_secret("github_token").ok().flatten()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PipelineError;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Creates a temporary directory containing an initialised git repository
    /// with an initial commit that includes `README.md`.
    ///
    /// Returns both the [`tempfile::TempDir`] (kept alive so the directory is
    /// not deleted during the test) and the raw [`git2::Repository`] for
    /// low-level inspection.
    fn make_local_repo() -> (tempfile::TempDir, git2::Repository) {
        let td = tempfile::tempdir().unwrap(); // SAFETY: test-only
        let repo = git2::Repository::init(td.path()).unwrap(); // SAFETY: test-only

        std::fs::write(td.path().join("README.md"), "# Test Repo").unwrap(); // SAFETY: test-only

        let sig = git2::Signature::now("Test User", "test@example.com").unwrap(); // SAFETY: test-only
        let tree_id = {
            let mut index = repo.index().unwrap(); // SAFETY: test-only
            index.add_path(Path::new("README.md")).unwrap(); // SAFETY: test-only
            index.write().unwrap(); // SAFETY: test-only
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

    /// Creates a local repository wired up to a local bare remote (simulating
    /// a GitHub-style push target without network access).
    ///
    /// Steps performed:
    ///
    /// 1. A bare repository is created in a new tempdir.
    /// 2. A local repository with an initial commit is created via
    ///    [`make_local_repo`].
    /// 3. The bare repo is added as `origin` with a `file://` URL.
    /// 4. The initial commit is pushed so the bare repo is non-empty.
    ///
    /// Returns `(bare_td, local_td, write_repo)`.  Both tempdirs must stay
    /// alive for the duration of the test.
    fn make_write_repo_with_remote() -> (tempfile::TempDir, tempfile::TempDir, GitWriteRepository) {
        let bare_td = tempfile::tempdir().unwrap(); // SAFETY: test-only
        git2::Repository::init_bare(bare_td.path()).unwrap(); // SAFETY: test-only

        let (local_td, local_repo) = make_local_repo();
        let remote_url = format!("file://{}", bare_td.path().display());

        // Capture branch name before borrowing local_repo for the remote.
        let branch_name: String = local_repo
            .head()
            .unwrap() // SAFETY: test-only
            .shorthand()
            .unwrap() // SAFETY: test-only
            .to_owned();

        let refspec = format!("refs/heads/{0}:refs/heads/{0}", branch_name);

        // Add origin and push the initial commit.
        {
            let mut remote = local_repo.remote("origin", &remote_url).unwrap(); // SAFETY: test-only
            remote.push(&[&refspec], None).unwrap(); // SAFETY: test-only
        }

        // Align the bare repo HEAD so that subsequent clones check out the
        // correct default branch regardless of system git configuration.
        {
            let bare = git2::Repository::open(bare_td.path()).unwrap(); // SAFETY: test-only
            bare.set_head(&format!("refs/heads/{}", branch_name))
                .unwrap(); // SAFETY: test-only
        }

        let write_repo =
            GitWriteRepository::open(local_td.path(), "Test User", "test@example.com").unwrap(); // SAFETY: test-only

        (bare_td, local_td, write_repo)
    }

    // -----------------------------------------------------------------------
    // create_branch
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_branch_produces_expected_head() {
        let (td, _raw) = make_local_repo();
        let write_repo =
            GitWriteRepository::open(td.path(), "Test User", "test@example.com").unwrap(); // SAFETY: test-only

        write_repo.create_branch("feature/test-branch").unwrap(); // SAFETY: test-only

        let branch = write_repo.current_branch().unwrap(); // SAFETY: test-only
        assert_eq!(
            branch,
            Some("feature/test-branch".to_string()),
            "HEAD should point at the newly created branch"
        );
    }

    #[test]
    fn test_create_branch_with_invalid_name_returns_governance_error() {
        let (td, _raw) = make_local_repo();
        let write_repo =
            GitWriteRepository::open(td.path(), "Test User", "test@example.com").unwrap(); // SAFETY: test-only

        let result = write_repo.create_branch("bad branch");
        assert!(
            matches!(result, Err(PipelineError::Governance(_))),
            "branch name with space should return Governance error"
        );
    }

    #[test]
    fn test_create_branch_with_duplicate_name_returns_error() {
        let (td, _raw) = make_local_repo();
        let write_repo =
            GitWriteRepository::open(td.path(), "Test User", "test@example.com").unwrap(); // SAFETY: test-only

        write_repo.create_branch("feature/duplicate").unwrap(); // SAFETY: test-only
        let result = write_repo.create_branch("feature/duplicate");
        assert!(
            matches!(result, Err(PipelineError::Git(_))),
            "creating a branch that already exists should return Git error"
        );
    }

    // -----------------------------------------------------------------------
    // default_branch_name
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_branch_name_starts_with_xzardgz_prefix() {
        let name = GitWriteRepository::default_branch_name();
        assert!(
            name.starts_with("xzardgz/"),
            "default branch name should start with 'xzardgz/', got: {}",
            name
        );
    }

    #[test]
    fn test_default_branch_name_has_unique_values() {
        let name1 = GitWriteRepository::default_branch_name();
        let name2 = GitWriteRepository::default_branch_name();
        assert_ne!(
            name1, name2,
            "consecutive default branch names should be unique"
        );
    }

    // -----------------------------------------------------------------------
    // commit_paths
    // -----------------------------------------------------------------------

    #[test]
    fn test_commit_paths_returns_40_char_sha() {
        let (td, _raw) = make_local_repo();
        let write_repo =
            GitWriteRepository::open(td.path(), "Test User", "test@example.com").unwrap(); // SAFETY: test-only

        std::fs::write(td.path().join("test_file.txt"), "test content").unwrap(); // SAFETY: test-only

        let result = write_repo.commit_paths(&[Path::new("test_file.txt")], "add test_file.txt");
        assert!(result.is_ok(), "commit should succeed: {:?}", result.err());
        let oid = result.unwrap();
        assert_eq!(oid.len(), 40, "OID should be 40 characters, got: {}", oid);
    }

    #[test]
    fn test_commit_paths_file_appears_in_committed_tree() {
        let (td, _raw) = make_local_repo();
        let write_repo =
            GitWriteRepository::open(td.path(), "Test User", "test@example.com").unwrap(); // SAFETY: test-only

        std::fs::write(td.path().join("new_file.txt"), "file content").unwrap(); // SAFETY: test-only

        write_repo
            .commit_paths(&[Path::new("new_file.txt")], "add new_file.txt")
            .unwrap(); // SAFETY: test-only

        // Open a fresh handle to inspect the committed tree.
        let raw = git2::Repository::open(td.path()).unwrap(); // SAFETY: test-only
        let commit = raw.head().unwrap().peel_to_commit().unwrap(); // SAFETY: test-only
        let tree = commit.tree().unwrap(); // SAFETY: test-only
        assert!(
            tree.get_name("new_file.txt").is_some(),
            "new_file.txt should be present in the committed tree"
        );
    }

    #[test]
    fn test_commit_paths_with_absolute_path_succeeds() {
        let (td, _raw) = make_local_repo();
        let write_repo =
            GitWriteRepository::open(td.path(), "Test User", "test@example.com").unwrap(); // SAFETY: test-only

        let abs_path = td.path().join("absolute_file.txt");
        std::fs::write(&abs_path, "absolute path content").unwrap(); // SAFETY: test-only

        let result = write_repo.commit_paths(&[&abs_path], "add file via absolute path");
        assert!(
            result.is_ok(),
            "commit with absolute path should succeed: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap().len(), 40);
    }

    // -----------------------------------------------------------------------
    // push_branch
    // -----------------------------------------------------------------------

    #[test]
    fn test_push_branch_to_local_bare_remote_succeeds() {
        let (_bare_td, _local_td, write_repo) = make_write_repo_with_remote();

        write_repo.create_branch("feature/test-push").unwrap(); // SAFETY: test-only

        let result = write_repo.push_branch("feature/test-push");
        assert!(
            result.is_ok(),
            "push to local bare remote should succeed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_push_branch_with_invalid_name_returns_error() {
        let (td, _raw) = make_local_repo();
        let write_repo =
            GitWriteRepository::open(td.path(), "Test User", "test@example.com").unwrap(); // SAFETY: test-only

        let result = write_repo.push_branch("bad branch");
        assert!(
            matches!(result, Err(PipelineError::Governance(_))),
            "invalid branch name should return Governance error"
        );
    }

    #[test]
    fn test_push_branch_nonexistent_branch_returns_error() {
        let (_bare_td, _local_td, write_repo) = make_write_repo_with_remote();

        let result = write_repo.push_branch("feature/nonexistent");
        assert!(
            matches!(result, Err(PipelineError::Git(_))),
            "pushing a branch that does not exist locally should return Git error"
        );
    }

    // -----------------------------------------------------------------------
    // Round-trip: create_branch -> commit_paths -> push_branch -> clone + verify
    // -----------------------------------------------------------------------

    #[test]
    fn test_round_trip_create_commit_push_verify_content() {
        let (bare_td, local_td, write_repo) = make_write_repo_with_remote();

        // Step 1: Create the feature branch.
        write_repo.create_branch("feature/round-trip").unwrap(); // SAFETY: test-only

        // Step 2: Write the analysis file.
        let analysis_file = local_td.path().join("analysis.txt");
        std::fs::write(&analysis_file, "analysis results").unwrap(); // SAFETY: test-only

        // Step 3: Stage and commit.
        let oid = write_repo
            .commit_paths(&[&analysis_file], "add analysis results")
            .unwrap(); // SAFETY: test-only

        // Step 4: Push to origin.
        write_repo.push_branch("feature/round-trip").unwrap(); // SAFETY: test-only

        // Step 5: Clone the bare repo into a fresh directory.
        let clone_td = tempfile::tempdir().unwrap(); // SAFETY: test-only
        let remote_url = format!("file://{}", bare_td.path().display());
        let cloned = git2::Repository::clone(&remote_url, clone_td.path()).unwrap(); // SAFETY: test-only

        // Step 6: Locate the remote tracking branch in the clone.
        let remote_branch = cloned
            .find_branch("origin/feature/round-trip", git2::BranchType::Remote)
            .unwrap(); // SAFETY: test-only

        // Step 7: Peel to commit and verify the OID matches the one returned
        // by commit_paths.
        let remote_commit = remote_branch.get().peel_to_commit().unwrap(); // SAFETY: test-only
        assert_eq!(
            remote_commit.id().to_string(),
            oid,
            "remote commit OID should match the local commit OID"
        );

        // Step 8: Verify the file content in the committed tree.
        let tree = remote_commit.tree().unwrap(); // SAFETY: test-only
        let entry = tree.get_name("analysis.txt").unwrap(); // SAFETY: test-only
        let blob = cloned.find_blob(entry.id()).unwrap(); // SAFETY: test-only
        let content = std::str::from_utf8(blob.content()).unwrap(); // SAFETY: test-only
        assert_eq!(content, "analysis results");
    }
}
