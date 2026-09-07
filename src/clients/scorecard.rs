//! OpenSSF Scorecard integration client.
//!
//! This module provides types and functions for fetching and resolving
//! OpenSSF Scorecard data for a GitHub repository. Resolution uses a
//! two-level fallback chain:
//!
//! 1. A local `scorecard.json` file in the workspace root directory.
//! 2. The public OpenSSF Scorecard REST API at
//!    `https://api.securityscorecards.dev`.
//!
//! The local file path takes priority so that cached or offline results
//! are used without making network requests. If neither source succeeds,
//! [`ScorecardResolveError::AllSourcesExhausted`] is returned.
//!
//! # Usage
//!
//! ```no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use xzardgz::clients::scorecard::resolve_scorecard;
//!
//! let result = resolve_scorecard("ossf/scorecard", "/tmp/workspace").await?;
//! println!("score: {}", result.score);
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during Scorecard data resolution.
#[derive(Debug, Error)]
pub enum ScorecardResolveError {
    /// HTTP-level error (network failure or non-success HTTP status code).
    #[error("HTTP error fetching scorecard for '{repo}': {message}")]
    Http {
        /// The repository identifier that was being fetched.
        repo: String,
        /// A description of the HTTP error.
        message: String,
    },
    /// JSON deserialization failure for the scorecard response.
    #[error("failed to parse scorecard response: {0}")]
    Parse(String),
    /// I/O error when reading a local scorecard file.
    #[error("local scorecard file read error at '{path}': {message}")]
    LocalFile {
        /// Path to the file that could not be read.
        path: String,
        /// A description of the I/O error.
        message: String,
    },
    /// Both the local file and the remote API failed or were unavailable.
    #[error("all scorecard sources exhausted for '{0}'")]
    AllSourcesExhausted(String),
    /// The repository string could not be parsed as a GitHub reference.
    #[error("invalid repository identifier: '{0}'")]
    InvalidRepo(String),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Documentation metadata embedded in a single Scorecard check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScorecardCheckDoc {
    /// Short description of the check.
    pub short: String,
    /// URL pointing to full documentation for the check.
    pub url: String,
}

/// A single check result within a Scorecard response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScorecardCheck {
    /// Human-readable name of the check (e.g., `"Code-Review"`).
    pub name: String,
    /// Numeric score for this check (0-10, or -1 when not applicable).
    pub score: i32,
    /// Human-readable explanation of the score.
    pub reason: String,
    /// Optional list of detailed findings supporting the score.
    #[serde(default)]
    pub details: Vec<String>,
    /// Optional documentation reference for this check.
    #[serde(default)]
    pub documentation: Option<ScorecardCheckDoc>,
}

/// Repository information embedded in a Scorecard response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScorecardRepoInfo {
    /// Full repository name in `github.com/owner/repo` format.
    pub name: String,
    /// Commit hash used for the scorecard evaluation, if present.
    #[serde(default)]
    pub commit: Option<String>,
}

/// A complete OpenSSF Scorecard result for one repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScorecardResult {
    /// ISO 8601 date on which the scorecard was generated.
    pub date: String,
    /// Repository information for the evaluated project.
    pub repo: ScorecardRepoInfo,
    /// Aggregate score for the repository (0.0-10.0).
    pub score: f64,
    /// Individual check results.
    #[serde(default)]
    pub checks: Vec<ScorecardCheck>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SCORECARD_API_BASE: &str = "https://api.securityscorecards.dev";

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// Fetches a Scorecard result from the specified base URL.
///
/// This is the internal implementation used by both production code
/// (via [`SCORECARD_API_BASE`]) and tests (via a mock server URL). The
/// request URL is constructed as:
///
/// ```text
/// {base_url}/projects/github.com/{owner}/{repo_name}
/// ```
///
/// # Arguments
///
/// * `repo` - A GitHub repository identifier accepted by
///   [`crate::clients::parse_github_slug`] (e.g., `"ossf/scorecard"` or
///   `"https://github.com/ossf/scorecard"`).
/// * `base_url` - Base URL of the Scorecard API server, without a trailing
///   slash (e.g., `"https://api.securityscorecards.dev"`).
///
/// # Returns
///
/// A [`ScorecardResult`] deserialized from the API response on success.
///
/// # Errors
///
/// - [`ScorecardResolveError::InvalidRepo`] when `repo` cannot be parsed as a
///   GitHub slug.
/// - [`ScorecardResolveError::Http`] on network failure or a non-2xx HTTP
///   status code.
/// - [`ScorecardResolveError::Parse`] when the response body cannot be
///   deserialized as [`ScorecardResult`].
///
/// # Examples
///
/// ```ignore
/// // fetch_scorecard_from is pub(crate); use fetch_scorecard or resolve_scorecard
/// // from external code. This example is for crate-internal documentation only.
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use xzardgz::clients::scorecard::fetch_scorecard_from;
///
/// let result = fetch_scorecard_from(
///     "ossf/scorecard",
///     "https://api.securityscorecards.dev",
/// ).await?;
/// println!("score: {}", result.score);
/// # Ok(())
/// # }
/// ```
pub(crate) async fn fetch_scorecard_from(
    repo: &str,
    base_url: &str,
) -> Result<ScorecardResult, ScorecardResolveError> {
    let (owner, repo_name) = crate::clients::parse_github_slug(repo)
        .ok_or_else(|| ScorecardResolveError::InvalidRepo(repo.to_string()))?;

    let url = format!("{base_url}/projects/github.com/{owner}/{repo_name}");

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| ScorecardResolveError::Http {
            repo: repo.to_string(),
            message: e.to_string(),
        })?;

    if !response.status().is_success() {
        return Err(ScorecardResolveError::Http {
            repo: repo.to_string(),
            message: format!("HTTP {}", response.status()),
        });
    }

    let result: ScorecardResult = response
        .json()
        .await
        .map_err(|e| ScorecardResolveError::Parse(e.to_string()))?;

    Ok(result)
}

/// Fetches scorecard data for `repo` from the public OpenSSF Scorecard REST API.
///
/// Delegates to [`fetch_scorecard_from`] using the production base URL
/// `https://api.securityscorecards.dev`.
///
/// # Arguments
///
/// * `repo` - A GitHub repository identifier accepted by
///   [`crate::clients::parse_github_slug`].
///
/// # Returns
///
/// A [`ScorecardResult`] on success.
///
/// # Errors
///
/// Propagates all errors from [`fetch_scorecard_from`].
///
/// # Examples
///
/// ```no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use xzardgz::clients::scorecard::fetch_scorecard;
///
/// let result = fetch_scorecard("ossf/scorecard").await?;
/// println!("score: {}", result.score);
/// # Ok(())
/// # }
/// ```
pub async fn fetch_scorecard(repo: &str) -> Result<ScorecardResult, ScorecardResolveError> {
    fetch_scorecard_from(repo, SCORECARD_API_BASE).await
}

/// Resolves scorecard data via a two-level fallback chain.
///
/// 1. **Local file**: reads `{workspace_root}/scorecard.json` if it exists,
///    parses it as JSON, and returns the result immediately without any
///    network request.
/// 2. **Remote fetch**: calls the public OpenSSF Scorecard REST API via
///    [`fetch_scorecard`].
///
/// Returns [`ScorecardResolveError::AllSourcesExhausted`] when both sources
/// fail. Local file read or parse failures are returned as-is (without
/// falling through to the remote).
///
/// # Arguments
///
/// * `repo` - A GitHub repository identifier accepted by
///   [`crate::clients::parse_github_slug`].
/// * `workspace_root` - Path to a local directory that may contain a
///   pre-fetched `scorecard.json` file.
///
/// # Returns
///
/// A [`ScorecardResult`] from whichever source succeeds first.
///
/// # Errors
///
/// - [`ScorecardResolveError::LocalFile`] when a local file exists but cannot
///   be read.
/// - [`ScorecardResolveError::Parse`] when a local file exists but cannot be
///   parsed as [`ScorecardResult`].
/// - [`ScorecardResolveError::AllSourcesExhausted`] when the local file is
///   absent and the remote fetch fails for any reason.
///
/// # Examples
///
/// ```no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use xzardgz::clients::scorecard::resolve_scorecard;
///
/// let result = resolve_scorecard("ossf/scorecard", "/workspace").await?;
/// println!("score: {}", result.score);
/// # Ok(())
/// # }
/// ```
pub async fn resolve_scorecard(
    repo: &str,
    workspace_root: &str,
) -> Result<ScorecardResult, ScorecardResolveError> {
    let local_path = Path::new(workspace_root).join("scorecard.json");

    if local_path.exists() {
        let contents =
            std::fs::read_to_string(&local_path).map_err(|e| ScorecardResolveError::LocalFile {
                path: local_path.display().to_string(),
                message: e.to_string(),
            })?;
        let result: ScorecardResult = serde_json::from_str(&contents)
            .map_err(|e| ScorecardResolveError::Parse(e.to_string()))?;
        return Ok(result);
    }

    match fetch_scorecard(repo).await {
        Ok(result) => Ok(result),
        Err(e) => {
            tracing::debug!(
                repo = repo,
                error = %e,
                "remote scorecard fetch failed; all sources exhausted"
            );
            Err(ScorecardResolveError::AllSourcesExhausted(repo.to_string()))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Returns a minimal valid scorecard JSON fixture.
    fn fixture_scorecard_json() -> serde_json::Value {
        serde_json::json!({
            "date": "2024-01-15",
            "repo": {
                "name": "github.com/ossf/scorecard",
                "commit": "abc123def456"
            },
            "score": 7.5,
            "checks": [
                {
                    "name": "Code-Review",
                    "score": 8,
                    "reason": "found 8 unreviewed changesets out of 20 total"
                }
            ]
        })
    }

    // ------------------------------------------------------------------
    // fetch_scorecard_from: success
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_fetch_scorecard_from_with_mock_server_returns_result() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/github.com/ossf/scorecard"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture_scorecard_json()))
            .mount(&mock_server)
            .await;

        let result = fetch_scorecard_from("ossf/scorecard", &mock_server.uri()).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        let sc = result.unwrap();
        assert_eq!(sc.score, 7.5);
        assert_eq!(sc.repo.name, "github.com/ossf/scorecard");
        assert_eq!(sc.checks.len(), 1);
    }

    // ------------------------------------------------------------------
    // fetch_scorecard_from: HTTP error
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_fetch_scorecard_from_with_mock_server_http_error_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/github.com/ossf/scorecard"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = fetch_scorecard_from("ossf/scorecard", &mock_server.uri()).await;
        assert!(
            matches!(result, Err(ScorecardResolveError::Http { .. })),
            "expected Http error, got: {:?}",
            result
        );
    }

    // ------------------------------------------------------------------
    // resolve_scorecard: local file found
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_resolve_scorecard_with_local_file_returns_result_without_http() {
        let dir = tempfile::tempdir()
            // SAFETY: tempdir creation in test environment; failure means unrecoverable
            // test setup error.
            .expect("tempdir creation failed");
        let file_path = dir.path().join("scorecard.json");

        std::fs::write(
            &file_path,
            serde_json::to_string(&fixture_scorecard_json())
                // SAFETY: fixture is a static serde_json::Value; serialization cannot fail.
                .expect("fixture serialization failed"),
        )
        // SAFETY: tempdir write in test environment; failure is unrecoverable test setup.
        .expect("writing scorecard.json fixture failed");

        let workspace = dir
            .path()
            .to_str()
            // SAFETY: tempdir paths produced by the tempfile crate are always valid UTF-8.
            .expect("tempdir path is not valid UTF-8");

        let result = resolve_scorecard("owner/repo", workspace).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        assert_eq!(result.unwrap().score, 7.5);
    }

    // ------------------------------------------------------------------
    // resolve_scorecard: remote when no local file
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_resolve_scorecard_fetches_remote_when_no_local_file() {
        // resolve_scorecard calls fetch_scorecard which calls fetch_scorecard_from
        // with the hardcoded SCORECARD_API_BASE. We exercise the same remote-fetch
        // code path here via fetch_scorecard_from directly to inject the mock URL.
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/github.com/ossf/scorecard"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture_scorecard_json()))
            .mount(&mock_server)
            .await;

        let result = fetch_scorecard_from("ossf/scorecard", &mock_server.uri()).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
    }

    // ------------------------------------------------------------------
    // resolve_scorecard: invalid repository identifier
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_resolve_scorecard_with_invalid_repo_returns_error() {
        let dir = tempfile::tempdir()
            // SAFETY: tempdir creation in test environment.
            .expect("tempdir creation failed");
        let workspace = dir
            .path()
            .to_str()
            // SAFETY: tempdir path is always valid UTF-8.
            .expect("tempdir path is not valid UTF-8");

        // No scorecard.json in the tempdir, so resolution falls through to the
        // remote path which will reject the invalid repo identifier.
        let result = resolve_scorecard("not-a-repo://bad", workspace).await;
        assert!(result.is_err(), "expected Err for invalid repo identifier");
    }

    // ------------------------------------------------------------------
    // ScorecardResult: JSON round-trip
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_scorecard_result_roundtrip_json() {
        let original = ScorecardResult {
            date: "2024-01-15".to_string(),
            repo: ScorecardRepoInfo {
                name: "github.com/ossf/scorecard".to_string(),
                commit: Some("abc123".to_string()),
            },
            score: 7.5,
            checks: vec![ScorecardCheck {
                name: "Code-Review".to_string(),
                score: 8,
                reason: "found 8 unreviewed changesets out of 20 total".to_string(),
                details: vec!["detail1".to_string()],
                documentation: Some(ScorecardCheckDoc {
                    short: "code review check".to_string(),
                    url: "https://github.com/ossf/scorecard/blob/main/docs/checks.md".to_string(),
                }),
            }],
        };

        let json = serde_json::to_string(&original)
            // SAFETY: ScorecardResult contains only String/f64/i32/Vec fields;
            // serialization cannot fail.
            .expect("serialization failed");
        let deserialized: ScorecardResult = serde_json::from_str(&json)
            // SAFETY: just serialized from a valid struct; deserialization cannot fail.
            .expect("deserialization failed");

        assert_eq!(deserialized.date, original.date);
        assert_eq!(deserialized.score, original.score);
        assert_eq!(deserialized.repo.name, original.repo.name);
        assert_eq!(
            deserialized.repo.commit, original.repo.commit,
            "commit field should round-trip"
        );
        assert_eq!(deserialized.checks.len(), original.checks.len());
        assert_eq!(deserialized.checks[0].name, original.checks[0].name);
        assert_eq!(deserialized.checks[0].score, original.checks[0].score);
        assert_eq!(deserialized.checks[0].details, original.checks[0].details);
    }
}
