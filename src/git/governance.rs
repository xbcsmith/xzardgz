//! Git governance validation functions.
//!
//! Validates branch names, repository paths, and remote URLs to prevent
//! misuse and enforce governance rules. All destructive operations must
//! pass these checks before execution.

use crate::error::{PipelineError, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Maximum allowed length for a git branch name.
const MAX_BRANCH_NAME_LEN: usize = 250;

/// Characters that are unconditionally forbidden inside a git branch name.
const FORBIDDEN_CHARS: &[char] = &[' ', '~', '^', ':', '?', '*', '\\', '['];

/// URL scheme prefixes accepted by [`validate_remote_url`].
const ALLOWED_URL_PREFIXES: &[&str] = &["https://", "http://", "git@", "ssh://", "git://"];

/// Validates a git branch name against git-check-ref-format rules.
///
/// Enforces a subset of the rules defined by `git check-ref-format` to
/// prevent branch names that would be rejected by git or that could cause
/// confusion in automated pipelines.
///
/// # Arguments
///
/// * `name` - The branch name to validate.
///
/// # Returns
///
/// `Ok(())` when the name is valid.
///
/// # Errors
///
/// Returns [`PipelineError::Governance`] when any of the following rules is
/// violated:
///
/// - The name is empty.
/// - The name exceeds 250 characters.
/// - The name contains a space, tilde, caret, colon, question mark,
///   asterisk, backslash, or open bracket.
/// - The name contains `..` (double dot).
/// - The name starts with `/`.
/// - The name ends with `/`.
/// - The name ends with `.` (single dot).
/// - The name ends with `.lock`.
/// - The name contains ASCII control characters (code points < 32 or == 127).
///
/// # Examples
///
/// ```
/// use xzardgz::git::governance::validate_branch_name;
/// assert!(validate_branch_name("main").is_ok());
/// assert!(validate_branch_name("feature/my-feature").is_ok());
/// assert!(validate_branch_name("").is_err());
/// assert!(validate_branch_name("bad branch").is_err());
/// ```
pub fn validate_branch_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(PipelineError::Governance(
            "branch name must not be empty".to_string(),
        ));
    }

    if name.len() > MAX_BRANCH_NAME_LEN {
        return Err(PipelineError::Governance(format!(
            "branch name exceeds maximum length of {} characters",
            MAX_BRANCH_NAME_LEN
        )));
    }

    for ch in FORBIDDEN_CHARS {
        if name.contains(*ch) {
            return Err(PipelineError::Governance(format!(
                "branch name must not contain '{}'",
                ch
            )));
        }
    }

    if name.contains("..") {
        return Err(PipelineError::Governance(
            "branch name must not contain '..'".to_string(),
        ));
    }

    if name.starts_with('/') {
        return Err(PipelineError::Governance(
            "branch name must not start with '/'".to_string(),
        ));
    }

    if name.ends_with('/') {
        return Err(PipelineError::Governance(
            "branch name must not end with '/'".to_string(),
        ));
    }

    if name.ends_with('.') {
        return Err(PipelineError::Governance(
            "branch name must not end with '.'".to_string(),
        ));
    }

    if name.ends_with(".lock") {
        return Err(PipelineError::Governance(
            "branch name must not end with '.lock'".to_string(),
        ));
    }

    for ch in name.chars() {
        let code = ch as u32;
        if code < 32 || code == 127 {
            return Err(PipelineError::Governance(
                "branch name must not contain ASCII control characters".to_string(),
            ));
        }
    }

    Ok(())
}

/// Validates that a local path is suitable as a repository path.
///
/// Checks that the given path exists and points to a directory. This
/// validation should be performed before attempting any git operations on
/// the path.
///
/// # Arguments
///
/// * `path` - The filesystem path to validate.
///
/// # Returns
///
/// `Ok(())` when the path exists and is a directory.
///
/// # Errors
///
/// Returns [`PipelineError::Governance`] when:
///
/// - The path does not exist on the filesystem.
/// - The path exists but is not a directory (e.g. it is a regular file).
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use xzardgz::git::governance::validate_repository_path;
///
/// assert!(validate_repository_path(Path::new("/tmp")).is_ok());
/// assert!(validate_repository_path(Path::new("/nonexistent_xzardgz_path")).is_err());
/// ```
pub fn validate_repository_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(PipelineError::Governance(format!(
            "repository path does not exist: {}",
            path.display()
        )));
    }

    if !path.is_dir() {
        return Err(PipelineError::Governance(format!(
            "repository path is not a directory: {}",
            path.display()
        )));
    }

    Ok(())
}

/// Validates a remote git URL.
///
/// Checks that the URL is non-empty and uses a recognised git transport
/// scheme. Bare file paths and other unsupported schemes are rejected.
///
/// # Arguments
///
/// * `url` - The remote URL string to validate.
///
/// # Returns
///
/// `Ok(())` when the URL is non-empty and starts with a supported scheme.
///
/// # Errors
///
/// Returns [`PipelineError::Governance`] when:
///
/// - The URL is empty.
/// - The URL does not start with `https://`, `http://`, `git@`, `ssh://`,
///   or `git://`.
///
/// # Examples
///
/// ```
/// use xzardgz::git::governance::validate_remote_url;
/// assert!(validate_remote_url("https://github.com/example/repo").is_ok());
/// assert!(validate_remote_url("git@github.com:example/repo.git").is_ok());
/// assert!(validate_remote_url("").is_err());
/// assert!(validate_remote_url("/local/path").is_err());
/// ```
pub fn validate_remote_url(url: &str) -> Result<()> {
    if url.is_empty() {
        return Err(PipelineError::Governance(
            "remote URL must not be empty".to_string(),
        ));
    }

    let is_valid = ALLOWED_URL_PREFIXES
        .iter()
        .any(|prefix| url.starts_with(prefix));

    if !is_valid {
        return Err(PipelineError::Governance(format!(
            "remote URL has unsupported scheme; must start with one of: {}",
            ALLOWED_URL_PREFIXES.join(", ")
        )));
    }

    Ok(())
}

/// Normalizes a repository URL for consistent hashing and comparison.
///
/// The following transformations are applied in order:
///
/// 1. Trim trailing whitespace.
/// 2. Strip trailing `/` characters.
/// 3. Strip a trailing `.git` suffix if present.
/// 4. Convert to lowercase.
///
/// This function never fails.
///
/// # Arguments
///
/// * `url` - The URL string to normalize.
///
/// # Returns
///
/// The normalized URL as an owned [`String`].
///
/// # Examples
///
/// ```
/// use xzardgz::git::governance::normalize_url;
/// assert_eq!(
///     normalize_url("https://github.com/example/repo.git"),
///     "https://github.com/example/repo"
/// );
/// assert_eq!(
///     normalize_url("https://github.com/example/repo/"),
///     "https://github.com/example/repo"
/// );
/// assert_eq!(
///     normalize_url("HTTPS://GITHUB.COM/Example/Repo"),
///     "https://github.com/example/repo"
/// );
/// ```
pub fn normalize_url(url: &str) -> String {
    let without_trailing_ws = url.trim_end();
    let without_trailing_slashes = without_trailing_ws.trim_end_matches('/');
    let without_git_suffix = without_trailing_slashes
        .strip_suffix(".git")
        .unwrap_or(without_trailing_slashes);
    without_git_suffix.to_lowercase()
}

/// Returns the 64-character lower-hex SHA-256 hash of the normalized URL.
///
/// The URL is first passed through [`normalize_url`] before hashing, so
/// equivalent URLs that differ only in trailing slashes, a `.git` suffix,
/// or letter case will all produce the same hash.
///
/// # Arguments
///
/// * `url` - The URL string to hash.
///
/// # Returns
///
/// A 64-character lowercase hexadecimal string representing the SHA-256
/// digest of the normalized URL.
///
/// # Examples
///
/// ```
/// use xzardgz::git::governance::hash_url;
///
/// let h = hash_url("https://github.com/example/repo");
/// assert_eq!(h.len(), 64);
/// // Normalization means the .git suffix does not change the hash.
/// assert_eq!(h, hash_url("https://github.com/example/repo.git"));
/// ```
pub fn hash_url(url: &str) -> String {
    let normalized = normalize_url(url);
    let digest = Sha256::digest(normalized.as_bytes());
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // ------------------------------------------------------------------
    // validate_branch_name
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_branch_name_with_main_succeeds() {
        assert!(validate_branch_name("main").is_ok());
    }

    #[test]
    fn test_validate_branch_name_with_feature_slash_name_succeeds() {
        assert!(validate_branch_name("feature/my-feature").is_ok());
    }

    #[test]
    fn test_validate_branch_name_with_empty_string_returns_error() {
        let result = validate_branch_name("");
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    #[test]
    fn test_validate_branch_name_with_space_returns_error() {
        let result = validate_branch_name("bad branch");
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    #[test]
    fn test_validate_branch_name_with_double_dot_returns_error() {
        let result = validate_branch_name("feature..name");
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    #[test]
    fn test_validate_branch_name_with_tilde_returns_error() {
        let result = validate_branch_name("branch~1");
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    #[test]
    fn test_validate_branch_name_with_caret_returns_error() {
        let result = validate_branch_name("branch^1");
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    #[test]
    fn test_validate_branch_name_with_leading_slash_returns_error() {
        let result = validate_branch_name("/feature");
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    #[test]
    fn test_validate_branch_name_with_trailing_slash_returns_error() {
        let result = validate_branch_name("feature/");
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    #[test]
    fn test_validate_branch_name_with_trailing_dot_returns_error() {
        let result = validate_branch_name("feature.");
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    #[test]
    fn test_validate_branch_name_with_dot_lock_suffix_returns_error() {
        let result = validate_branch_name("feature.lock");
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    #[test]
    fn test_validate_branch_name_exceeding_250_chars_returns_error() {
        let long_name = "a".repeat(251);
        let result = validate_branch_name(&long_name);
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    #[test]
    fn test_validate_branch_name_with_control_char_returns_error() {
        // 0x01 is a control character (code point 1 < 32).
        let result = validate_branch_name("branch\x01name");
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    // ------------------------------------------------------------------
    // validate_repository_path
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_repository_path_with_existing_dir_succeeds() {
        // expect: tempdir() only fails on catastrophic OS errors; safe in tests.
        let dir = tempdir().expect("failed to create temporary directory for test");
        assert!(validate_repository_path(dir.path()).is_ok());
    }

    #[test]
    fn test_validate_repository_path_with_nonexistent_path_returns_error() {
        let path = Path::new("/nonexistent_xzardgz_governance_test_path_d3f4");
        let result = validate_repository_path(path);
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    #[test]
    fn test_validate_repository_path_with_file_returns_error() {
        use std::io::Write;
        // expect: tempdir()/File::create/write only fail on catastrophic OS errors; safe in tests.
        let dir = tempdir().expect("failed to create temporary directory for test");
        let file_path = dir.path().join("governance_test_file.txt");
        let mut f =
            std::fs::File::create(&file_path).expect("failed to create temporary test file");
        f.write_all(b"data")
            .expect("failed to write to temporary test file");
        let result = validate_repository_path(&file_path);
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    // ------------------------------------------------------------------
    // validate_remote_url
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_remote_url_with_https_succeeds() {
        assert!(validate_remote_url("https://github.com/example/repo").is_ok());
    }

    #[test]
    fn test_validate_remote_url_with_git_at_succeeds() {
        assert!(validate_remote_url("git@github.com:example/repo.git").is_ok());
    }

    #[test]
    fn test_validate_remote_url_with_ssh_succeeds() {
        assert!(validate_remote_url("ssh://git@github.com/example/repo.git").is_ok());
    }

    #[test]
    fn test_validate_remote_url_with_empty_string_returns_error() {
        let result = validate_remote_url("");
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    #[test]
    fn test_validate_remote_url_with_file_path_returns_error() {
        let result = validate_remote_url("/local/path/to/repo");
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    #[test]
    fn test_validate_remote_url_with_bare_name_returns_error() {
        let result = validate_remote_url("myrepo");
        assert!(matches!(result, Err(PipelineError::Governance(_))));
    }

    // ------------------------------------------------------------------
    // normalize_url
    // ------------------------------------------------------------------

    #[test]
    fn test_normalize_url_strips_trailing_git_suffix() {
        assert_eq!(
            normalize_url("https://github.com/example/repo.git"),
            "https://github.com/example/repo"
        );
    }

    #[test]
    fn test_normalize_url_strips_trailing_slash() {
        assert_eq!(
            normalize_url("https://github.com/example/repo/"),
            "https://github.com/example/repo"
        );
    }

    #[test]
    fn test_normalize_url_converts_to_lowercase() {
        assert_eq!(
            normalize_url("HTTPS://GITHUB.COM/Example/Repo"),
            "https://github.com/example/repo"
        );
    }

    #[test]
    fn test_normalize_url_handles_url_without_git_suffix() {
        assert_eq!(
            normalize_url("https://github.com/example/repo"),
            "https://github.com/example/repo"
        );
    }

    #[test]
    fn test_normalize_url_strips_both_git_and_slash() {
        // Order: trim whitespace -> trim trailing '/' -> strip '.git' -> lowercase.
        // "repo.git/" -> strip '/' -> "repo.git" -> strip '.git' -> "repo".
        assert_eq!(
            normalize_url("https://github.com/example/repo.git/"),
            "https://github.com/example/repo"
        );
    }

    // ------------------------------------------------------------------
    // hash_url
    // ------------------------------------------------------------------

    #[test]
    fn test_hash_url_returns_64_char_hex() {
        let h = hash_url("https://github.com/example/repo");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_url_is_deterministic() {
        let url = "https://github.com/example/repo";
        assert_eq!(hash_url(url), hash_url(url));
    }

    #[test]
    fn test_hash_url_differs_for_different_urls() {
        let h1 = hash_url("https://github.com/example/repo1");
        let h2 = hash_url("https://github.com/example/repo2");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_url_normalizes_before_hashing() {
        let base = hash_url("https://github.com/example/repo");
        // Trailing .git suffix should not change the hash.
        assert_eq!(base, hash_url("https://github.com/example/repo.git"));
        // Uppercase should not change the hash.
        assert_eq!(base, hash_url("HTTPS://GITHUB.COM/EXAMPLE/REPO"));
        // Trailing slash should not change the hash.
        assert_eq!(base, hash_url("https://github.com/example/repo/"));
    }
}
