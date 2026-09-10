//! GitHub repository metadata client.
//!
//! This module provides types and functions for fetching and resolving
//! GitHub repository metadata (stars, forks, language, license, topics,
//! etc.) via the GitHub REST API. Resolution uses a two-level fallback
//! chain:
//!
//! 1. A local `repodata.json` file in the workspace root directory.
//! 2. The GitHub REST API at `https://api.github.com`. When the
//!    `GITHUB_TOKEN` environment variable is set the request is
//!    authenticated; unauthenticated requests are used otherwise.
//!
//! The local file path takes priority so that cached or offline results
//! are used without making network requests. If neither source succeeds,
//! [`RepoDataResolveError::AllSourcesExhausted`] is returned.
//!
//! # Usage
//!
//! ```no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use xzardgz::clients::repodata::resolve_repodata;
//!
//! let meta = resolve_repodata("ossf/scorecard", "/tmp/workspace").await?;
//! println!("language: {:?}", meta.language);
//! # Ok(())
//! # }
//! ```

use crate::auth::SecretStore;
use crate::auth::store::EnvVarStore;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during repository metadata resolution.
#[derive(Debug, Error)]
pub enum RepoDataResolveError {
    /// HTTP-level error (network failure or non-success HTTP status code).
    #[error("HTTP error fetching repo metadata for '{repo}': {message}")]
    Http {
        /// The repository identifier that was being fetched.
        repo: String,
        /// A description of the HTTP error.
        message: String,
    },
    /// JSON deserialization failure for the repository metadata response.
    #[error("failed to parse repo metadata response: {0}")]
    Parse(String),
    /// I/O error when reading a local repository metadata file.
    #[error("local repo metadata file read error at '{path}': {message}")]
    LocalFile {
        /// Path to the file that could not be read.
        path: String,
        /// A description of the I/O error.
        message: String,
    },
    /// Both the local file and the remote API failed or were unavailable.
    #[error("all repo metadata sources exhausted for '{0}'")]
    AllSourcesExhausted(String),
    /// The repository string could not be parsed as a GitHub reference.
    #[error("invalid repository identifier: '{0}'")]
    InvalidRepo(String),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// License information attached to a GitHub repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoLicense {
    /// Human-readable license name (e.g., `"Apache License 2.0"`).
    pub name: String,
    /// SPDX license identifier (e.g., `"Apache-2.0"`), when available.
    pub spdx_id: Option<String>,
}

/// Metadata for a GitHub repository as returned by the REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoMetadata {
    /// Full repository name in `owner/repo` format.
    pub full_name: String,
    /// Human-readable repository description, if set.
    pub description: Option<String>,
    /// Primary programming language of the repository, if detected.
    pub language: Option<String>,
    /// Name of the default branch (defaults to `"main"` when absent in the
    /// API response).
    #[serde(default = "default_branch_default")]
    pub default_branch: String,
    /// Total number of stars (GitHub calls this `stargazers_count`).
    #[serde(default)]
    pub stargazers_count: u64,
    /// Total number of forks.
    #[serde(default)]
    pub forks_count: u64,
    /// Number of open issues.
    #[serde(default)]
    pub open_issues_count: u64,
    /// Repository topics.
    #[serde(default)]
    pub topics: Vec<String>,
    /// Whether the repository has been archived.
    #[serde(default)]
    pub archived: bool,
    /// Whether the repository is itself a fork of another repository.
    #[serde(default)]
    pub fork: bool,
    /// Visibility level (e.g., `"public"`, `"private"`).
    pub visibility: Option<String>,
    /// Repository size in kilobytes as reported by GitHub.
    #[serde(default)]
    pub size: u64,
    /// License information, if a recognized license is detected.
    pub license: Option<RepoLicense>,
    /// ISO 8601 timestamp of the last push to the repository.
    pub pushed_at: Option<String>,
    /// ISO 8601 timestamp of the last metadata update.
    pub updated_at: Option<String>,
}

/// Returns the default value `"main"` used by
/// `#[serde(default = "default_branch_default")]`.
fn default_branch_default() -> String {
    "main".to_string()
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const GITHUB_API_BASE: &str = "https://api.github.com";

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// Fetches repository metadata from the specified GitHub API base URL.
///
/// This is the internal implementation used by both production code
/// (via [`GITHUB_API_BASE`]) and tests (via a mock server URL). The
/// request URL is constructed as `{base_url}/repos/{owner}/{repo}`.
///
/// The following headers are always sent:
/// - `User-Agent: xzardgz`
/// - `Accept: application/vnd.github+json`
/// - `X-GitHub-Api-Version: 2022-11-28`
///
/// An `Authorization: Bearer {token}` header is added when `token` is
/// `Some`.
///
/// # Arguments
///
/// * `owner` - GitHub owner (user or organisation) name.
/// * `repo` - Repository name (without the owner prefix).
/// * `token` - Optional GitHub personal access token for authentication.
/// * `base_url` - Base URL of the GitHub API server, without a trailing
///   slash (e.g., `"https://api.github.com"`).
///
/// # Returns
///
/// A [`RepoMetadata`] deserialized from the API response on success.
///
/// # Errors
///
/// - [`RepoDataResolveError::Http`] on network failure or a non-2xx HTTP
///   status code.
/// - [`RepoDataResolveError::Parse`] when the response body cannot be
///   deserialized as [`RepoMetadata`].
///
/// # Examples
///
/// ```ignore
/// // fetch_repodata_from is pub(crate); use resolve_repodata from external code.
/// // This example is for crate-internal documentation only.
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use xzardgz::clients::repodata::fetch_repodata_from;
///
/// let meta = fetch_repodata_from(
///     "ossf",
///     "scorecard",
///     None,
///     "https://api.github.com",
/// ).await?;
/// println!("full_name: {}", meta.full_name);
/// # Ok(())
/// # }
/// ```
pub(crate) async fn fetch_repodata_from(
    owner: &str,
    repo: &str,
    token: Option<&str>,
    base_url: &str,
) -> Result<RepoMetadata, RepoDataResolveError> {
    let url = format!("{base_url}/repos/{owner}/{repo}");
    let repo_id = format!("{owner}/{repo}");

    let client = reqwest::Client::new();
    let mut request = client
        .get(&url)
        .header("User-Agent", "xzardgz")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    if let Some(t) = token {
        request = request.header("Authorization", format!("Bearer {t}"));
    }

    let response = request
        .send()
        .await
        .map_err(|e| RepoDataResolveError::Http {
            repo: repo_id.clone(),
            message: e.to_string(),
        })?;

    if !response.status().is_success() {
        return Err(RepoDataResolveError::Http {
            repo: repo_id,
            message: format!("HTTP {}", response.status()),
        });
    }

    let metadata: RepoMetadata = response
        .json()
        .await
        .map_err(|e| RepoDataResolveError::Parse(e.to_string()))?;

    Ok(metadata)
}

/// Resolves GitHub repository metadata via a two-level fallback chain against a configurable base URL.
///
/// This is the internal implementation backing [`resolve_repodata`]. It checks
/// the local workspace file first and falls through to a remote GitHub API
/// fetch against `base_url` when the file is absent. Supplying a custom
/// `base_url` allows test code to point the resolver at a mock server without
/// any network requests to the production GitHub API.
///
/// # Arguments
///
/// * `repo` - A GitHub repository identifier accepted by
///   [`crate::clients::parse_github_slug`] (e.g., `"ossf/scorecard"`).
/// * `workspace_root` - Path to a local directory that may contain a
///   pre-fetched `repodata.json` file.
/// * `base_url` - Base URL of the GitHub API server, without a trailing
///   slash (e.g., `"https://api.github.com"`).
///
/// # Returns
///
/// A [`RepoMetadata`] from whichever source succeeds first.
///
/// # Errors
///
/// - [`RepoDataResolveError::LocalFile`] when a local file exists but cannot
///   be read.
/// - [`RepoDataResolveError::Parse`] when a local file exists but cannot be
///   parsed as [`RepoMetadata`].
/// - [`RepoDataResolveError::InvalidRepo`] when `repo` cannot be parsed as a
///   GitHub reference and no local file is found.
/// - [`RepoDataResolveError::AllSourcesExhausted`] when no remote source
///   succeeds.
///
/// # Examples
///
/// ```ignore
/// // resolve_repodata_from is pub(crate); use resolve_repodata from external code.
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use xzardgz::clients::repodata::resolve_repodata_from;
///
/// let meta = resolve_repodata_from(
///     "ossf/scorecard",
///     "/tmp/workspace",
///     "https://api.github.com",
/// ).await?;
/// println!("full_name: {}", meta.full_name);
/// # Ok(())
/// # }
/// ```
pub(crate) async fn resolve_repodata_from(
    repo: &str,
    workspace_root: &str,
    base_url: &str,
) -> Result<RepoMetadata, RepoDataResolveError> {
    let local_path = Path::new(workspace_root).join("repodata.json");

    if local_path.exists() {
        let contents =
            std::fs::read_to_string(&local_path).map_err(|e| RepoDataResolveError::LocalFile {
                path: local_path.display().to_string(),
                message: e.to_string(),
            })?;
        let metadata: RepoMetadata = serde_json::from_str(&contents)
            .map_err(|e| RepoDataResolveError::Parse(e.to_string()))?;
        return Ok(metadata);
    }

    let (owner, repo_name) = crate::clients::parse_github_slug(repo)
        .ok_or_else(|| RepoDataResolveError::InvalidRepo(repo.to_string()))?;

    let token: Option<String> = EnvVarStore::new("GITHUB_")
        .get_secret("token")
        .ok()
        .flatten();

    match fetch_repodata_from(&owner, &repo_name, token.as_deref(), base_url).await {
        Ok(metadata) => Ok(metadata),
        Err(e) => {
            tracing::debug!(
                repo = repo,
                error = %e,
                "remote repodata fetch failed; all sources exhausted"
            );
            Err(RepoDataResolveError::AllSourcesExhausted(repo.to_string()))
        }
    }
}

/// Resolves GitHub repository metadata via a two-level fallback chain.
///
/// 1. **Local file**: reads `{workspace_root}/repodata.json` if it exists,
///    parses it as JSON, and returns the result immediately without any
///    network request.
/// 2. **Remote fetch**: calls the GitHub REST API. The `GITHUB_TOKEN`
///    environment variable is used for authentication when present;
///    unauthenticated requests are used otherwise.
///
/// Returns [`RepoDataResolveError::AllSourcesExhausted`] when the local file
/// is absent and the remote fetch fails. Local file read or parse failures
/// are returned as-is without falling through to the remote. Delegates to
/// [`resolve_repodata_from`] using [`GITHUB_API_BASE`].
///
/// # Arguments
///
/// * `repo` - A GitHub repository identifier accepted by
///   [`crate::clients::parse_github_slug`] (e.g., `"ossf/scorecard"`).
/// * `workspace_root` - Path to a local directory that may contain a
///   pre-fetched `repodata.json` file.
///
/// # Returns
///
/// A [`RepoMetadata`] from whichever source succeeds first.
///
/// # Errors
///
/// - [`RepoDataResolveError::LocalFile`] when a local file exists but cannot
///   be read.
/// - [`RepoDataResolveError::Parse`] when a local file exists but cannot be
///   parsed as [`RepoMetadata`].
/// - [`RepoDataResolveError::InvalidRepo`] when `repo` cannot be parsed as a
///   GitHub reference and no local file is found.
/// - [`RepoDataResolveError::AllSourcesExhausted`] when no remote source
///   succeeds.
///
/// # Examples
///
/// ```no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use xzardgz::clients::repodata::resolve_repodata;
///
/// let meta = resolve_repodata("ossf/scorecard", "/workspace").await?;
/// println!("stars: {}", meta.stargazers_count);
/// # Ok(())
/// # }
/// ```
pub async fn resolve_repodata(
    repo: &str,
    workspace_root: &str,
) -> Result<RepoMetadata, RepoDataResolveError> {
    resolve_repodata_from(repo, workspace_root, GITHUB_API_BASE).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Returns a minimal valid repository metadata JSON fixture.
    fn fixture_repodata_json() -> serde_json::Value {
        serde_json::json!({
            "full_name": "ossf/scorecard",
            "description": "Security Scorecards - Security health metrics for Open Source",
            "language": "Go",
            "default_branch": "main",
            "stargazers_count": 4500,
            "forks_count": 520,
            "open_issues_count": 130,
            "topics": ["security", "supply-chain"],
            "archived": false,
            "fork": false,
            "visibility": "public",
            "size": 8192,
            "license": {
                "name": "Apache License 2.0",
                "spdx_id": "Apache-2.0"
            },
            "pushed_at": "2024-01-15T12:00:00Z",
            "updated_at": "2024-01-15T12:00:00Z"
        })
    }

    // ------------------------------------------------------------------
    // fetch_repodata_from: success
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_fetch_repodata_from_with_mock_server_returns_metadata() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/ossf/scorecard"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture_repodata_json()))
            .mount(&mock_server)
            .await;

        let result = fetch_repodata_from("ossf", "scorecard", None, &mock_server.uri()).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        let meta = result.unwrap();
        assert_eq!(meta.full_name, "ossf/scorecard");
        assert_eq!(meta.language, Some("Go".to_string()));
        assert_eq!(meta.stargazers_count, 4500);
    }

    // ------------------------------------------------------------------
    // fetch_repodata_from: HTTP error
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_fetch_repodata_from_with_mock_server_http_error_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/ossf/scorecard"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = fetch_repodata_from("ossf", "scorecard", None, &mock_server.uri()).await;
        assert!(
            matches!(result, Err(RepoDataResolveError::Http { .. })),
            "expected Http error, got: {:?}",
            result
        );
    }

    // ------------------------------------------------------------------
    // resolve_repodata: local file found
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_resolve_repodata_with_local_file_returns_metadata_without_http() {
        let dir = tempfile::tempdir()
            // SAFETY: tempdir creation in test environment; failure means unrecoverable
            // test setup error.
            .expect("tempdir creation failed");
        let file_path = dir.path().join("repodata.json");

        std::fs::write(
            &file_path,
            serde_json::to_string(&fixture_repodata_json())
                // SAFETY: fixture is a static serde_json::Value; serialization cannot fail.
                .expect("fixture serialization failed"),
        )
        // SAFETY: tempdir write in test environment; failure is unrecoverable test setup.
        .expect("writing repodata.json fixture failed");

        let workspace = dir
            .path()
            .to_str()
            // SAFETY: tempdir paths produced by the tempfile crate are always valid UTF-8.
            .expect("tempdir path is not valid UTF-8");

        let result = resolve_repodata("owner/repo", workspace).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        assert_eq!(result.unwrap().full_name, "ossf/scorecard");
    }

    // ------------------------------------------------------------------
    // resolve_repodata: remote when no local file
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_resolve_repodata_fetches_remote_when_no_local_file() {
        // resolve_repodata calls fetch_repodata_from with the hardcoded
        // GITHUB_API_BASE. We exercise the same remote-fetch code path here via
        // fetch_repodata_from directly to inject the mock URL.
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/ossf/scorecard"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture_repodata_json()))
            .mount(&mock_server)
            .await;

        let result = fetch_repodata_from("ossf", "scorecard", None, &mock_server.uri()).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
    }

    // ------------------------------------------------------------------
    // resolve_repodata: invalid repository identifier
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_resolve_repodata_with_invalid_repo_returns_error() {
        let dir = tempfile::tempdir()
            // SAFETY: tempdir creation in test environment.
            .expect("tempdir creation failed");
        let workspace = dir
            .path()
            .to_str()
            // SAFETY: tempdir path is always valid UTF-8.
            .expect("tempdir path is not valid UTF-8");

        // No repodata.json in the tempdir, so resolution falls through to the
        // slug-parsing step which rejects the invalid identifier.
        let result = resolve_repodata("not-github://bad", workspace).await;
        assert!(
            matches!(result, Err(RepoDataResolveError::InvalidRepo(_))),
            "expected InvalidRepo error, got: {:?}",
            result
        );
    }

    // ------------------------------------------------------------------
    // resolve_repodata: remote fallback exercised via resolve_repodata_from
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_resolve_repodata_with_no_local_file_falls_back_to_remote() {
        // Create an empty tempdir so there is no repodata.json present,
        // which forces resolve_repodata_from to take the remote fetch path.
        let dir = tempfile::tempdir()
            // SAFETY: tempdir creation in test environment; failure is unrecoverable.
            .expect("tempdir creation failed");

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/ossf/scorecard"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture_repodata_json()))
            .mount(&mock_server)
            .await;

        let workspace = dir
            .path()
            .to_str()
            // SAFETY: tempdir paths from the tempfile crate are always valid UTF-8.
            .expect("tempdir path is not valid UTF-8");

        // Call the crate-internal _from variant so we can inject the mock URL.
        // resolve_repodata delegates to this with GITHUB_API_BASE; the logic
        // under test (tempdir miss -> slug parse -> remote fetch -> parse) is identical.
        let result = resolve_repodata_from("ossf/scorecard", workspace, &mock_server.uri()).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        let meta = result.unwrap();
        assert_eq!(
            meta.full_name, "ossf/scorecard",
            "full_name should match fixture"
        );
        assert_eq!(
            meta.stargazers_count, 4500,
            "star count should match fixture"
        );
        assert_eq!(
            meta.language,
            Some("Go".to_string()),
            "language should match fixture"
        );
    }

    // ------------------------------------------------------------------
    // RepoMetadata: JSON round-trip
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_repo_metadata_roundtrip_json() {
        let original = RepoMetadata {
            full_name: "ossf/scorecard".to_string(),
            description: Some("Security Scorecards".to_string()),
            language: Some("Go".to_string()),
            default_branch: "main".to_string(),
            stargazers_count: 4500,
            forks_count: 520,
            open_issues_count: 130,
            topics: vec!["security".to_string(), "supply-chain".to_string()],
            archived: false,
            fork: false,
            visibility: Some("public".to_string()),
            size: 8192,
            license: Some(RepoLicense {
                name: "Apache License 2.0".to_string(),
                spdx_id: Some("Apache-2.0".to_string()),
            }),
            pushed_at: Some("2024-01-15T12:00:00Z".to_string()),
            updated_at: Some("2024-01-15T12:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&original)
            // SAFETY: RepoMetadata fields are all basic types (String, u64, bool, Vec);
            // serialization cannot fail.
            .expect("serialization failed");
        let deserialized: RepoMetadata = serde_json::from_str(&json)
            // SAFETY: just serialized from a valid struct; deserialization cannot fail.
            .expect("deserialization failed");

        assert_eq!(deserialized.full_name, original.full_name);
        assert_eq!(deserialized.language, original.language);
        assert_eq!(deserialized.stargazers_count, original.stargazers_count);
        assert_eq!(deserialized.forks_count, original.forks_count);
        assert_eq!(deserialized.topics, original.topics);
        assert_eq!(deserialized.archived, original.archived);
        assert_eq!(deserialized.default_branch, original.default_branch);
        assert_eq!(
            deserialized.license.as_ref().map(|l| &l.name),
            original.license.as_ref().map(|l| &l.name),
        );
        assert_eq!(
            deserialized
                .license
                .as_ref()
                .and_then(|l| l.spdx_id.as_deref()),
            original.license.as_ref().and_then(|l| l.spdx_id.as_deref()),
        );
    }
}
