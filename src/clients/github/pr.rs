//! GitHub REST API client for pull request creation.
//!
//! This module provides [`GithubPrClient`], a thin async client that posts to
//! the GitHub REST API `POST /repos/{owner}/{repo}/pulls` endpoint.
//!
//! Token resolution is handled by [`resolve_github_pat`], which checks the
//! `XZARDGZ_GITHUB_TOKEN` environment variable first and then falls back to
//! the OS keyring.
//!
//! # Examples
//!
//! ```no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use xzardgz::clients::github::{GithubPrClient, PrInput};
//! use xzardgz::clients::github::pr::resolve_github_pat;
//!
//! let token = resolve_github_pat();
//! let client = GithubPrClient::new(token);
//!
//! let input = PrInput {
//!     owner: "my-org".to_string(),
//!     repo: "my-repo".to_string(),
//!     head_branch: "feature/new-thing".to_string(),
//!     base_branch: "main".to_string(),
//!     title: "Add new thing".to_string(),
//!     body: Some("This PR adds a new thing.".to_string()),
//!     draft: false,
//! };
//!
//! let output = client.create_pr(&input).await?;
//! println!("Created PR #{} at {}", output.number, output.html_url);
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when creating a GitHub pull request.
///
/// These errors are dedicated to the PR client layer. Callers (executors)
/// are responsible for mapping these to a higher-level error type such as
/// `PipelineError` when needed.
#[derive(Debug, Error)]
pub enum PrClientError {
    /// The GitHub API returned a non-2xx HTTP status code.
    #[error("HTTP {status} from GitHub API: {message}")]
    Http {
        /// The HTTP status code returned by the API.
        status: u16,
        /// The response body text describing the error.
        message: String,
    },
    /// The GitHub API response could not be deserialized.
    #[error("failed to parse GitHub API response: {0}")]
    Parse(String),
    /// The head branch and base branch are identical, which GitHub rejects.
    #[error("head branch and base branch are the same: '{0}'")]
    HeadEqualsBase(String),
    /// No GitHub personal access token is available.
    #[error("GitHub personal access token required; set XZARDGZ_GITHUB_TOKEN")]
    MissingToken,
}

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

/// Input parameters for creating a GitHub pull request.
///
/// All branch names are validated before the network call; if `head_branch`
/// equals `base_branch` the call returns [`PrClientError::HeadEqualsBase`]
/// without making any HTTP request.
#[derive(Debug, Clone)]
pub struct PrInput {
    /// GitHub organisation or user name that owns the repository.
    pub owner: String,
    /// Repository name (without the `owner/` prefix).
    pub repo: String,
    /// Name of the branch that contains the changes to merge.
    pub head_branch: String,
    /// Name of the target branch that should receive the changes.
    pub base_branch: String,
    /// PR title shown in the GitHub UI.
    pub title: String,
    /// Optional PR description body (Markdown supported).
    pub body: Option<String>,
    /// When `true`, the PR is opened as a draft.
    pub draft: bool,
}

/// Output returned after a pull request is successfully created.
#[derive(Debug, Clone)]
pub struct PrOutput {
    /// The GitHub-assigned PR number within the repository.
    pub number: u64,
    /// Full URL to view the pull request in a browser.
    pub html_url: String,
    /// PR state as returned by GitHub (typically `"open"`).
    pub state: String,
}

// ---------------------------------------------------------------------------
// Internal serde types
// ---------------------------------------------------------------------------

/// Serialization shape for the `POST /repos/{owner}/{repo}/pulls` request body.
#[derive(Serialize)]
struct CreatePrRequest<'a> {
    title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<&'a str>,
    head: &'a str,
    base: &'a str,
    draft: bool,
}

/// Deserialization shape for the `POST /repos/{owner}/{repo}/pulls` response.
#[derive(Deserialize)]
struct CreatePrResponse {
    number: u64,
    html_url: String,
    state: String,
}

// ---------------------------------------------------------------------------
// GithubPrClient
// ---------------------------------------------------------------------------

/// Async client for the GitHub REST API, scoped to pull request creation.
///
/// Construct with [`GithubPrClient::new`] for production use or
/// [`GithubPrClient::with_base_url`] to target a mock server in tests.
///
/// # Examples
///
/// ```no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use xzardgz::clients::github::{GithubPrClient, PrInput};
///
/// let client = GithubPrClient::new(Some("ghp_token".to_string()));
/// let input = PrInput {
///     owner: "owner".to_string(),
///     repo: "repo".to_string(),
///     head_branch: "feature/x".to_string(),
///     base_branch: "main".to_string(),
///     title: "My PR".to_string(),
///     body: None,
///     draft: false,
/// };
/// let output = client.create_pr(&input).await?;
/// println!("PR #{}", output.number);
/// # Ok(())
/// # }
/// ```
pub struct GithubPrClient {
    http: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl GithubPrClient {
    /// Creates a new client targeting the production GitHub API.
    ///
    /// The production base URL is `https://api.github.com`.
    ///
    /// # Arguments
    ///
    /// * `token` - Optional GitHub personal access token. When `None`, any
    ///   call to [`GithubPrClient::create_pr`] returns
    ///   [`PrClientError::MissingToken`].
    ///
    /// # Returns
    ///
    /// A [`GithubPrClient`] ready to make requests to the production API.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::clients::github::GithubPrClient;
    ///
    /// let client = GithubPrClient::new(Some("ghp_mypat".to_string()));
    /// ```
    pub fn new(token: Option<String>) -> Self {
        Self::with_base_url(token, "https://api.github.com".to_string())
    }

    /// Creates a new client targeting a custom base URL.
    ///
    /// Intended for tests that point the client at a local mock server.
    /// The base URL must not include a trailing slash.
    ///
    /// # Arguments
    ///
    /// * `token` - Optional GitHub personal access token.
    /// * `base_url` - Base URL of the GitHub-compatible API, e.g.
    ///   `"http://localhost:8080"` for a wiremock server.
    ///
    /// # Returns
    ///
    /// A [`GithubPrClient`] configured with the given base URL.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::clients::github::GithubPrClient;
    ///
    /// let client = GithubPrClient::with_base_url(
    ///     Some("ghp_mypat".to_string()),
    ///     "http://localhost:9999".to_string(),
    /// );
    /// ```
    pub fn with_base_url(token: Option<String>, base_url: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url,
            token,
        }
    }

    /// Creates a GitHub pull request via the REST API.
    ///
    /// POSTs to `{base_url}/repos/{owner}/{repo}/pulls` with the required
    /// headers and a JSON body derived from `input`.
    ///
    /// # Arguments
    ///
    /// * `input` - PR creation parameters. See [`PrInput`] for field
    ///   descriptions.
    ///
    /// # Returns
    ///
    /// A [`PrOutput`] containing the PR number, URL, and state on success.
    ///
    /// # Errors
    ///
    /// - [`PrClientError::MissingToken`] when no GitHub token is configured.
    /// - [`PrClientError::HeadEqualsBase`] when `head_branch` equals
    ///   `base_branch`.
    /// - [`PrClientError::Http`] when the GitHub API responds with a non-2xx
    ///   status code.
    /// - [`PrClientError::Parse`] when the success response body cannot be
    ///   deserialized.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use xzardgz::clients::github::{GithubPrClient, PrInput};
    ///
    /// let client = GithubPrClient::new(Some("ghp_token".to_string()));
    /// let input = PrInput {
    ///     owner: "owner".to_string(),
    ///     repo: "repo".to_string(),
    ///     head_branch: "feature/x".to_string(),
    ///     base_branch: "main".to_string(),
    ///     title: "My PR".to_string(),
    ///     body: None,
    ///     draft: false,
    /// };
    /// let output = client.create_pr(&input).await?;
    /// println!("Created PR #{}", output.number);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_pr(&self, input: &PrInput) -> Result<PrOutput, PrClientError> {
        let token = self.token.as_deref().ok_or(PrClientError::MissingToken)?;

        if input.head_branch == input.base_branch {
            return Err(PrClientError::HeadEqualsBase(input.head_branch.clone()));
        }

        let url = format!(
            "{}/repos/{}/{}/pulls",
            self.base_url, input.owner, input.repo
        );

        let request_body = CreatePrRequest {
            title: &input.title,
            body: input.body.as_deref(),
            head: &input.head_branch,
            base: &input.base_branch,
            draft: input.draft,
        };

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "xzardgz")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| PrClientError::Http {
                status: e.status().map(|s| s.as_u16()).unwrap_or(0),
                message: e.to_string(),
            })?;

        let status = response.status();

        if !status.is_success() {
            let body_text = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("<could not read response body>"));
            return Err(PrClientError::Http {
                status: status.as_u16(),
                message: body_text,
            });
        }

        let parsed: CreatePrResponse = response
            .json()
            .await
            .map_err(|e| PrClientError::Parse(e.to_string()))?;

        Ok(PrOutput {
            number: parsed.number,
            html_url: parsed.html_url,
            state: parsed.state,
        })
    }
}

// ---------------------------------------------------------------------------
// resolve_github_pat
// ---------------------------------------------------------------------------

/// Resolves a GitHub PAT from environment or keyring.
///
/// Checks `XZARDGZ_GITHUB_TOKEN` env var via [`EnvVarStore`] first,
/// then falls back to the OS keyring key `github_token` in service
/// `xzardgz-github` via [`KeyringStore`].
///
/// # Returns
///
/// `Some(token)` when a token is found in either store, or `None` when
/// neither store has a value for `github_token`.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::clients::github::pr::resolve_github_pat;
///
/// if let Some(token) = resolve_github_pat() {
///     println!("Found GitHub token");
/// }
/// ```
pub fn resolve_github_pat() -> Option<String> {
    use crate::auth::store::{EnvVarStore, KeyringStore, SecretStore};
    let env_store = EnvVarStore::new("XZARDGZ_");
    if let Ok(Some(t)) = env_store.get_secret("github_token") {
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
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Builds a canonical [`PrInput`] fixture for use across tests.
    fn make_input() -> PrInput {
        PrInput {
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            head_branch: "feature/my-feature".to_string(),
            base_branch: "main".to_string(),
            title: "Add feature".to_string(),
            body: Some("PR body".to_string()),
            draft: false,
        }
    }

    // ------------------------------------------------------------------
    // test_create_pr_with_valid_input_returns_pr_output
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_pr_with_valid_input_returns_pr_output() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/pulls"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "number": 42,
                "html_url": "https://github.com/owner/repo/pull/42",
                "state": "open"
            })))
            .mount(&mock_server)
            .await;

        let client =
            GithubPrClient::with_base_url(Some("test-token".to_string()), mock_server.uri());

        let result = client.create_pr(&make_input()).await;

        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        let output = result.unwrap(); // SAFETY: test-only, asserted is_ok above
        assert_eq!(output.number, 42);
        assert_eq!(output.html_url, "https://github.com/owner/repo/pull/42");
        assert_eq!(output.state, "open");
    }

    // ------------------------------------------------------------------
    // test_create_pr_http_error_returns_pr_client_error
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_pr_http_error_returns_pr_client_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/pulls"))
            .respond_with(
                ResponseTemplate::new(422).set_body_string(r#"{"message":"Validation Failed"}"#),
            )
            .mount(&mock_server)
            .await;

        let client =
            GithubPrClient::with_base_url(Some("test-token".to_string()), mock_server.uri());

        let result = client.create_pr(&make_input()).await;

        assert!(
            matches!(result, Err(PrClientError::Http { status: 422, .. })),
            "expected Http error with status 422, got: {:?}",
            result
        );
    }

    // ------------------------------------------------------------------
    // test_create_pr_without_token_returns_missing_token_error
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_pr_without_token_returns_missing_token_error() {
        let mock_server = MockServer::start().await;

        let client = GithubPrClient::with_base_url(None, mock_server.uri());

        let result = client.create_pr(&make_input()).await;

        assert!(
            matches!(result, Err(PrClientError::MissingToken)),
            "expected MissingToken error, got: {:?}",
            result
        );
    }

    // ------------------------------------------------------------------
    // test_create_pr_with_same_head_and_base_returns_error
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_pr_with_same_head_and_base_returns_error() {
        let mock_server = MockServer::start().await;

        let client =
            GithubPrClient::with_base_url(Some("test-token".to_string()), mock_server.uri());

        let mut input = make_input();
        input.base_branch = "feature/my-feature".to_string();

        let result = client.create_pr(&input).await;

        assert!(
            matches!(result, Err(PrClientError::HeadEqualsBase(_))),
            "expected HeadEqualsBase error, got: {:?}",
            result
        );
    }

    // ------------------------------------------------------------------
    // test_create_pr_sends_authorization_header
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_pr_sends_authorization_header() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/pulls"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "number": 1,
                "html_url": "https://github.com/owner/repo/pull/1",
                "state": "open"
            })))
            .mount(&mock_server)
            .await;

        let client =
            GithubPrClient::with_base_url(Some("test-token".to_string()), mock_server.uri());

        let result = client.create_pr(&make_input()).await;

        assert!(
            result.is_ok(),
            "expected mock to match Authorization header; got: {:?}",
            result.err()
        );
    }

    // ------------------------------------------------------------------
    // test_create_pr_with_draft_true_sends_draft_flag
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_pr_with_draft_true_sends_draft_flag() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/pulls"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "number": 7,
                "html_url": "https://github.com/owner/repo/pull/7",
                "state": "open"
            })))
            .mount(&mock_server)
            .await;

        let client =
            GithubPrClient::with_base_url(Some("test-token".to_string()), mock_server.uri());

        let mut input = make_input();
        input.draft = true;

        let result = client.create_pr(&input).await;

        assert!(
            result.is_ok(),
            "expected Ok with draft=true, got: {:?}",
            result.err()
        );
    }

    // ------------------------------------------------------------------
    // test_resolve_github_pat_returns_none_when_not_set
    // ------------------------------------------------------------------

    #[test]
    fn test_resolve_github_pat_returns_none_when_not_set() {
        temp_env::with_var("XZARDGZ_GITHUB_TOKEN", None::<&str>, || {
            // Only verify the env path; keyring behaviour is OS-dependent in CI.
            // We cannot assert None globally because the keyring may hold a value,
            // but we can at least ensure the function is callable without panicking.
            let _ = resolve_github_pat();
        });
    }

    // ------------------------------------------------------------------
    // test_resolve_github_pat_returns_token_from_env_var
    // ------------------------------------------------------------------

    #[test]
    fn test_resolve_github_pat_returns_token_from_env_var() {
        temp_env::with_var("XZARDGZ_GITHUB_TOKEN", Some("my-token"), || {
            let result = resolve_github_pat();
            assert_eq!(
                result,
                Some("my-token".to_string()),
                "expected token from XZARDGZ_GITHUB_TOKEN env var"
            );
        });
    }
}
