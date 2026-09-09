//! OSV (Open Source Vulnerabilities) API client with three-level fallback.
//!
//! Provides [`OsvClient`], which queries `POST https://api.osv.dev/v1/query`
//! using a cascading strategy:
//!
//! 1. **PURL query**: when `dep.purl` is set, sends a PURL-based request
//!    (including `dep.version` when present).  Returns immediately on
//!    non-empty results.
//! 2. **Name + ecosystem query**: when step 1 found nothing or `dep.purl` was
//!    absent, and both `dep.name` (non-empty) and `dep.ecosystem` are set,
//!    sends a name+ecosystem request (including `dep.version` when present).
//!    Returns immediately on non-empty results.
//! 3. **Commit query**: when steps 1 and 2 found nothing and `dep.commit` is
//!    set, sends a commit-hash-based request.
//!
//! An HTTP error at any step is returned immediately as
//! [`VulnClientError::Http`]; empty results advance to the next fallback.

use async_trait::async_trait;
use serde::Deserialize;

use crate::clients::vuln::{
    VulnClientError, VulnerabilityQuery, VulnerabilityRecord, VulnerabilitySource,
};

pub mod scoring;

// ---------------------------------------------------------------------------
// OsvClient
// ---------------------------------------------------------------------------

/// HTTP client for the OSV vulnerability API.
///
/// Use [`OsvClient::new`] for production (real endpoint) or
/// [`OsvClient::with_base_url`] in tests to direct requests at a mock server.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::vuln::OsvClient;
///
/// let client = OsvClient::new();
/// // Use client.query(&dep).await in async context.
/// ```
#[derive(Debug)]
pub struct OsvClient {
    client: reqwest::Client,
    base_url: String,
}

impl OsvClient {
    /// Creates a new `OsvClient` pointed at the production OSV API endpoint.
    ///
    /// # Returns
    ///
    /// A new [`OsvClient`] using `https://api.osv.dev` as the base URL.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::clients::vuln::OsvClient;
    ///
    /// let client = OsvClient::new();
    /// ```
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://api.osv.dev".to_string(),
        }
    }

    /// Creates a new `OsvClient` using a custom base URL.
    ///
    /// Intended for testing with a mock HTTP server.
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL string without a trailing slash
    ///   (e.g. `"http://127.0.0.1:8080"`).
    ///
    /// # Returns
    ///
    /// A new [`OsvClient`] that directs all requests to `base_url`.
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
        }
    }

    /// Sends a `POST /v1/query` request with the given JSON body.
    ///
    /// # Errors
    ///
    /// - [`VulnClientError::Http`] when the network call fails or the response
    ///   status is not 2xx.
    /// - [`VulnClientError::Parse`] when the response body cannot be
    ///   deserialized.
    async fn send_query(
        &self,
        body: serde_json::Value,
    ) -> Result<Vec<VulnerabilityRecord>, VulnClientError> {
        let url = format!("{}/v1/query", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| VulnClientError::Http {
                url: url.clone(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(VulnClientError::Http {
                url,
                message: format!("HTTP {}", response.status()),
            });
        }

        let osv_resp: OsvResponse = response
            .json()
            .await
            .map_err(|e| VulnClientError::Parse(e.to_string()))?;

        Ok(osv_resp.vulns.unwrap_or_default())
    }

    /// Builds a PURL-based OSV request body.
    fn build_purl_body(purl: &str, version: Option<&str>) -> serde_json::Value {
        let mut body = serde_json::json!({
            "package": { "purl": purl }
        });
        if let Some(v) = version {
            body["version"] = serde_json::Value::String(v.to_string());
        }
        body
    }

    /// Builds a name + ecosystem OSV request body.
    fn build_name_ecosystem_body(
        name: &str,
        ecosystem: &str,
        version: Option<&str>,
    ) -> serde_json::Value {
        let mut body = serde_json::json!({
            "package": { "name": name, "ecosystem": ecosystem }
        });
        if let Some(v) = version {
            body["version"] = serde_json::Value::String(v.to_string());
        }
        body
    }

    /// Builds a commit-hash OSV request body.
    fn build_commit_body(commit: &str) -> serde_json::Value {
        serde_json::json!({ "commit": commit })
    }
}

impl Default for OsvClient {
    /// Returns a default `OsvClient` using the production OSV API endpoint.
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Private deserialization shape for the OSV query response
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct OsvResponse {
    vulns: Option<Vec<VulnerabilityRecord>>,
}

// ---------------------------------------------------------------------------
// VulnerabilitySource impl
// ---------------------------------------------------------------------------

#[async_trait]
impl VulnerabilitySource for OsvClient {
    /// Queries the OSV API using a three-level fallback strategy.
    ///
    /// 1. PURL query (when `dep.purl` is set).
    /// 2. Name + ecosystem query (when step 1 is empty or absent and both
    ///    `dep.name` and `dep.ecosystem` are available).
    /// 3. Commit query (when steps 1 and 2 are empty and `dep.commit` is set).
    ///
    /// Returns `Ok(vec![])` when all applicable strategies yield no results.
    ///
    /// # Errors
    ///
    /// Returns [`VulnClientError::Http`] immediately if any HTTP call fails.
    /// Returns [`VulnClientError::Parse`] on JSON deserialization failure.
    async fn query(
        &self,
        dep: &VulnerabilityQuery,
    ) -> Result<Vec<VulnerabilityRecord>, VulnClientError> {
        // Step 1 - PURL
        if let Some(ref purl) = dep.purl {
            let body = OsvClient::build_purl_body(purl, dep.version.as_deref());
            let results = self.send_query(body).await?;
            if !results.is_empty() {
                return Ok(results);
            }
        }

        // Step 2 - name + ecosystem
        if !dep.name.is_empty()
            && let Some(ref ecosystem) = dep.ecosystem
        {
            let body =
                OsvClient::build_name_ecosystem_body(&dep.name, ecosystem, dep.version.as_deref());
            let results = self.send_query(body).await?;
            if !results.is_empty() {
                return Ok(results);
            }
        }

        // Step 3 - commit hash
        if let Some(ref commit) = dep.commit {
            let body = OsvClient::build_commit_body(commit);
            let results = self.send_query(body).await?;
            if !results.is_empty() {
                return Ok(results);
            }
        }

        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clients::vuln::{VulnClientError, VulnerabilityQuery};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // ------------------------------------------------------------------
    // test_osv_client_query_with_purl_returns_vulns
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_osv_client_query_with_purl_returns_vulns() {
        let mock_server = MockServer::start().await;
        // SAFETY: test fixture must be present for this test to be meaningful
        let fixture = std::fs::read_to_string("testdata/osv.dev.results.json")
            .expect("testdata/osv.dev.results.json must exist");

        Mock::given(method("POST"))
            .and(path("/v1/query"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&mock_server)
            .await;

        let client = OsvClient::with_base_url(mock_server.uri());
        let query = VulnerabilityQuery {
            name: "jinja2".to_string(),
            version: Some("2.9.6".to_string()),
            ecosystem: None,
            purl: Some("pkg:pypi/jinja2".to_string()),
            commit: None,
        };

        let result = client.query(&query).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        let records = result.unwrap();
        assert!(
            !records.is_empty(),
            "expected non-empty vulnerability records"
        );
        assert_eq!(records[0].id, "GHSA-462w-v97r-4m45");
    }

    // ------------------------------------------------------------------
    // test_osv_client_query_with_purl_empty_falls_back_to_name_ecosystem
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_osv_client_query_with_purl_empty_falls_back_to_name_ecosystem() {
        let mock_server = MockServer::start().await;
        // SAFETY: test fixture must be present for this test to be meaningful
        let fixture = std::fs::read_to_string("testdata/osv.dev.results.json")
            .expect("testdata/osv.dev.results.json must exist");

        // Mount the fixture response first (lower mount priority, unlimited).
        // This becomes the fallback when the empty mock is exhausted.
        Mock::given(method("POST"))
            .and(path("/v1/query"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&mock_server)
            .await;

        // Mount the empty response second (higher mount priority, consumed once).
        // wiremock evaluates most-recently-mounted mocks first.
        Mock::given(method("POST"))
            .and(path("/v1/query"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"vulns": []}"#))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        let client = OsvClient::with_base_url(mock_server.uri());
        let query = VulnerabilityQuery {
            name: "jinja2".to_string(),
            version: Some("2.9.6".to_string()),
            ecosystem: Some("PyPI".to_string()),
            purl: Some("pkg:pypi/jinja2".to_string()),
            commit: None,
        };

        let result = client.query(&query).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        assert!(
            !result.unwrap().is_empty(),
            "expected non-empty fallback results from name+ecosystem query"
        );
    }

    // ------------------------------------------------------------------
    // test_osv_client_query_all_empty_returns_ok_empty
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_osv_client_query_all_empty_returns_ok_empty() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/query"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"vulns": []}"#))
            .mount(&mock_server)
            .await;

        let client = OsvClient::with_base_url(mock_server.uri());
        let query = VulnerabilityQuery {
            name: "nonexistent".to_string(),
            version: None,
            ecosystem: Some("PyPI".to_string()),
            purl: None,
            commit: None,
        };

        let result = client.query(&query).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        assert!(result.unwrap().is_empty(), "expected empty result vec");
    }

    // ------------------------------------------------------------------
    // test_osv_client_query_http_error_returns_err
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_osv_client_query_http_error_returns_err() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/query"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let client = OsvClient::with_base_url(mock_server.uri());
        let query = VulnerabilityQuery {
            name: "nonexistent".to_string(),
            version: None,
            ecosystem: Some("PyPI".to_string()),
            purl: None,
            commit: None,
        };

        let result = client.query(&query).await;
        assert!(
            matches!(result, Err(VulnClientError::Http { .. })),
            "expected VulnClientError::Http, got: {:?}",
            result
        );
    }
}
