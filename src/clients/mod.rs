//! External data clients for supply-chain signal resolution.
//!
//! This module provides HTTP and file-system backed clients that fetch
//! and cache external supply-chain signals (OpenSSF Scorecard, GitHub
//! repository metadata) for use in pipeline analysis.
//!
//! # Component-Boundary Contract
//!
//! | Rule           | Detail                                               |
//! |----------------|------------------------------------------------------|
//! | May depend on  | `auth`, `config`                                     |
//! | Must NOT call  | `scanner`, `providers`, `agent`                      |
//! | Must NOT be    | called from `tools/`                                 |
//!
//! These rules ensure the data-fetching layer remains a pure leaf in the
//! dependency graph and does not introduce circular imports.

pub mod github;
pub mod repodata;
pub mod scorecard;
pub mod vuln;

pub use github::{GithubPrClient, PrClientError, PrInput, PrOutput};
pub use repodata::{RepoDataResolveError, RepoMetadata};
pub use scorecard::{ScorecardCheck, ScorecardResolveError, ScorecardResult};

// ---------------------------------------------------------------------------
// parse_github_slug
// ---------------------------------------------------------------------------

/// Parses a GitHub repository identifier from various URL and slug formats.
///
/// Supported input formats:
///
/// | Format                              | Example                                |
/// |-------------------------------------|----------------------------------------|
/// | `owner/repo`                        | `ossf/scorecard`                       |
/// | `github.com/owner/repo`             | `github.com/ossf/scorecard`            |
/// | `https://github.com/owner/repo`     | `https://github.com/ossf/scorecard`    |
/// | `https://github.com/owner/repo.git` | `https://github.com/ossf/scorecard.git`|
/// | `git@github.com:owner/repo.git`     | `git@github.com:ossf/scorecard.git`    |
///
/// Returns `None` for non-GitHub hosts, empty owner or repo segments, or
/// unrecognised formats.
///
/// # Arguments
///
/// * `repo` - A GitHub repository identifier in any of the supported formats.
///
/// # Returns
///
/// `Some((owner, repo_name))` on success, or `None` if the input cannot be
/// parsed as a GitHub repository reference.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::parse_github_slug;
///
/// assert_eq!(
///     parse_github_slug("ossf/scorecard"),
///     Some(("ossf".to_string(), "scorecard".to_string()))
/// );
/// assert_eq!(
///     parse_github_slug("https://github.com/ossf/scorecard.git"),
///     Some(("ossf".to_string(), "scorecard".to_string()))
/// );
/// assert_eq!(
///     parse_github_slug("git@github.com:ossf/scorecard.git"),
///     Some(("ossf".to_string(), "scorecard".to_string()))
/// );
/// assert!(parse_github_slug("gitlab.com/owner/repo").is_none());
/// assert!(parse_github_slug("just-a-name").is_none());
/// ```
pub fn parse_github_slug(repo: &str) -> Option<(String, String)> {
    let s = repo.trim();

    // SSH format: git@github.com:owner/repo[.git]
    if let Some(after_colon) = s.strip_prefix("git@github.com:") {
        return extract_owner_repo(after_colon);
    }

    // Reject all other SSH formats (git@otherhost:...).
    if s.starts_with("git@") {
        return None;
    }

    // Strip HTTPS or HTTP scheme when present.
    let without_scheme = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);

    // Strip the github.com host component when present.
    let path = if let Some(after_host) = without_scheme.strip_prefix("github.com/") {
        after_host
    } else {
        // If the first path segment looks like a hostname (contains a dot),
        // the host is not github.com and the input is rejected.
        let first_seg = without_scheme.split('/').next().unwrap_or("");
        if first_seg.contains('.') {
            return None;
        }
        // No hostname detected; treat the whole string as owner/repo.
        without_scheme
    };

    extract_owner_repo(path)
}

/// Extracts an `(owner, repo_name)` pair from a bare `owner/repo[.git]` path.
///
/// Strips trailing slashes and a `.git` suffix before splitting on the first
/// `/`. Returns `None` when either segment is empty or extra path components
/// are present.
fn extract_owner_repo(path: &str) -> Option<(String, String)> {
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let path = path.trim_end_matches('/');

    let (owner, repo_name) = path.split_once('/')?;

    if owner.is_empty() || repo_name.is_empty() || repo_name.contains('/') {
        return None;
    }

    Some((owner.to_string(), repo_name.to_string()))
}

// ---------------------------------------------------------------------------
// ExternalSignals
// ---------------------------------------------------------------------------

/// Resolved external supply-chain signals for a repository.
///
/// Holds the results of scorecard and repository metadata resolution.
/// Both fields are `None` when the corresponding resolver was disabled or
/// all sources were exhausted.
#[derive(Debug, Clone)]
pub struct ExternalSignals {
    /// Resolved OpenSSF Scorecard result, or `None` if unavailable.
    pub scorecard: Option<ScorecardResult>,
    /// Resolved GitHub repository metadata, or `None` if unavailable.
    pub repodata: Option<RepoMetadata>,
}

impl ExternalSignals {
    /// Returns `true` when both `scorecard` and `repodata` are `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::clients::ExternalSignals;
    ///
    /// let empty = ExternalSignals { scorecard: None, repodata: None };
    /// assert!(empty.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.scorecard.is_none() && self.repodata.is_none()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // parse_github_slug: owner/repo
    // ------------------------------------------------------------------

    #[test]
    fn test_parse_github_slug_with_owner_slash_repo_returns_pair() {
        let result = parse_github_slug("ossf/scorecard");
        assert_eq!(result, Some(("ossf".to_string(), "scorecard".to_string())));
    }

    #[test]
    fn test_parse_github_slug_with_github_host_prefix_returns_pair() {
        let result = parse_github_slug("github.com/ossf/scorecard");
        assert_eq!(result, Some(("ossf".to_string(), "scorecard".to_string())));
    }

    #[test]
    fn test_parse_github_slug_with_https_url_returns_pair() {
        let result = parse_github_slug("https://github.com/ossf/scorecard");
        assert_eq!(result, Some(("ossf".to_string(), "scorecard".to_string())));
    }

    #[test]
    fn test_parse_github_slug_with_https_git_url_strips_git_suffix() {
        let result = parse_github_slug("https://github.com/ossf/scorecard.git");
        assert_eq!(result, Some(("ossf".to_string(), "scorecard".to_string())));
    }

    #[test]
    fn test_parse_github_slug_with_ssh_url_returns_pair() {
        let result = parse_github_slug("git@github.com:ossf/scorecard.git");
        assert_eq!(result, Some(("ossf".to_string(), "scorecard".to_string())));
    }

    // ------------------------------------------------------------------
    // parse_github_slug: non-GitHub hosts
    // ------------------------------------------------------------------

    #[test]
    fn test_parse_github_slug_with_non_github_host_returns_none() {
        assert!(parse_github_slug("gitlab.com/ossf/scorecard").is_none());
    }

    #[test]
    fn test_parse_github_slug_with_https_non_github_url_returns_none() {
        assert!(parse_github_slug("https://gitlab.com/ossf/scorecard").is_none());
    }

    #[test]
    fn test_parse_github_slug_with_ssh_non_github_host_returns_none() {
        assert!(parse_github_slug("git@gitlab.com:ossf/scorecard.git").is_none());
    }

    // ------------------------------------------------------------------
    // parse_github_slug: invalid / edge cases
    // ------------------------------------------------------------------

    #[test]
    fn test_parse_github_slug_with_no_slash_returns_none() {
        assert!(parse_github_slug("justarepo").is_none());
    }

    #[test]
    fn test_parse_github_slug_with_empty_owner_returns_none() {
        assert!(parse_github_slug("/repo").is_none());
    }

    #[test]
    fn test_parse_github_slug_with_empty_repo_returns_none() {
        assert!(parse_github_slug("owner/").is_none());
    }

    #[test]
    fn test_parse_github_slug_with_extra_path_segments_returns_none() {
        assert!(parse_github_slug("owner/repo/extra").is_none());
    }

    #[test]
    fn test_parse_github_slug_with_empty_string_returns_none() {
        assert!(parse_github_slug("").is_none());
    }

    #[test]
    fn test_parse_github_slug_with_surrounding_whitespace_returns_pair() {
        let result = parse_github_slug("  ossf/scorecard  ");
        assert_eq!(result, Some(("ossf".to_string(), "scorecard".to_string())));
    }

    #[test]
    fn test_parse_github_slug_with_git_suffix_on_bare_slug_returns_pair() {
        let result = parse_github_slug("ossf/scorecard.git");
        assert_eq!(result, Some(("ossf".to_string(), "scorecard".to_string())));
    }

    #[test]
    fn test_parse_github_slug_with_http_url_returns_pair() {
        let result = parse_github_slug("http://github.com/ossf/scorecard");
        assert_eq!(result, Some(("ossf".to_string(), "scorecard".to_string())));
    }

    // ------------------------------------------------------------------
    // ExternalSignals::is_empty
    // ------------------------------------------------------------------

    #[test]
    fn test_external_signals_is_empty_when_both_fields_are_none() {
        let signals = ExternalSignals {
            scorecard: None,
            repodata: None,
        };
        assert!(signals.is_empty());
    }

    #[test]
    fn test_external_signals_is_not_empty_when_scorecard_is_set() {
        use super::scorecard::{ScorecardRepoInfo, ScorecardResult};

        let signals = ExternalSignals {
            scorecard: Some(ScorecardResult {
                date: "2024-01-01".to_string(),
                repo: ScorecardRepoInfo {
                    name: "github.com/ossf/scorecard".to_string(),
                    commit: None,
                },
                score: 7.5,
                checks: vec![],
            }),
            repodata: None,
        };
        assert!(!signals.is_empty());
    }

    #[test]
    fn test_external_signals_is_not_empty_when_repodata_is_set() {
        use super::repodata::RepoMetadata;

        let signals = ExternalSignals {
            scorecard: None,
            repodata: Some(RepoMetadata {
                full_name: "ossf/scorecard".to_string(),
                description: None,
                language: None,
                default_branch: "main".to_string(),
                stargazers_count: 0,
                forks_count: 0,
                open_issues_count: 0,
                topics: vec![],
                archived: false,
                fork: false,
                visibility: None,
                size: 0,
                license: None,
                pushed_at: None,
                updated_at: None,
            }),
        };
        assert!(!signals.is_empty());
    }
}
