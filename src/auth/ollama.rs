//! Ollama host validation.
//!
//! Ollama does not require authentication — it runs as a local server with no
//! credential system. This module provides a host reachability check and a
//! status helper that always reports "available" for the auth status display.

use crate::error::{PipelineError, Result};

use super::types::{AuthStatus, CredentialSource};

// ---------------------------------------------------------------------------
// OllamaAuth
// ---------------------------------------------------------------------------

/// Ollama authentication helper (host validation only).
///
/// Ollama requires no credentials. The only meaningful check is whether the
/// local server is reachable. `status()` always returns
/// [`AuthStatus::CredentialPresent`] to indicate the provider is usable.
pub struct OllamaAuth {
    /// Base URL of the Ollama server, e.g. `"http://localhost:11434"`.
    host: String,
}

impl OllamaAuth {
    /// Creates an `OllamaAuth` for the given Ollama base URL.
    ///
    /// # Arguments
    ///
    /// * `host` - Base URL of the Ollama server, e.g. `"http://localhost:11434"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::auth::OllamaAuth;
    ///
    /// let auth = OllamaAuth::new("http://localhost:11434");
    /// assert_eq!(auth.host(), "http://localhost:11434");
    /// ```
    pub fn new(host: impl Into<String>) -> Self {
        Self { host: host.into() }
    }

    /// Returns the configured Ollama host URL.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::auth::OllamaAuth;
    ///
    /// let auth = OllamaAuth::new("http://localhost:11434");
    /// assert_eq!(auth.host(), "http://localhost:11434");
    /// ```
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns [`AuthStatus::CredentialPresent`] unconditionally.
    ///
    /// Ollama requires no credentials; this method exists so that
    /// [`crate::auth::ProviderAuthManager::status_all`] can report a uniform
    /// status for all providers.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::auth::OllamaAuth;
    /// use xzardgz::auth::types::AuthStatus;
    ///
    /// let auth = OllamaAuth::new("http://localhost:11434");
    /// assert!(auth.status().has_credentials());
    /// ```
    pub fn status(&self) -> AuthStatus {
        AuthStatus::CredentialPresent {
            source: CredentialSource::EnvironmentVariable {
                name: "none (ollama requires no auth)".to_string(),
            },
        }
    }

    /// Checks that the Ollama server is reachable by GET-ing `{host}/api/tags`.
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if the server responds with a 2xx status.
    /// - `Ok(false)` on connection refused, timeout, or any network error.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Provider`] only for unexpected non-network
    /// errors (e.g. URL parse failure).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::auth::OllamaAuth;
    ///
    /// # async fn run() {
    /// let auth = OllamaAuth::new("http://localhost:11434");
    /// let reachable = auth.check_reachable().await.unwrap_or(false);
    /// println!("Ollama reachable: {reachable}");
    /// # }
    /// ```
    pub async fn check_reachable(&self) -> Result<bool> {
        let url = format!("{}/api/tags", self.host);
        match reqwest::get(&url).await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(e) if e.is_connect() || e.is_timeout() => Ok(false),
            Err(e) => {
                // For all other network errors (DNS, TLS, etc.), treat as
                // unreachable rather than propagating.
                tracing::debug!("ollama reachability check failed: {e}");
                Err(PipelineError::Provider(format!(
                    "ollama reachability check failed: {e}"
                )))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // host()
    // ------------------------------------------------------------------

    #[test]
    fn test_ollama_auth_host_returns_configured_host() {
        let auth = OllamaAuth::new("http://localhost:11434");
        assert_eq!(
            auth.host(),
            "http://localhost:11434",
            "host() should return the value passed to new()"
        );
    }

    #[test]
    fn test_ollama_auth_host_returns_custom_host() {
        let auth = OllamaAuth::new("http://192.168.1.100:11434");
        assert_eq!(auth.host(), "http://192.168.1.100:11434");
    }

    // ------------------------------------------------------------------
    // status()
    // ------------------------------------------------------------------

    #[test]
    fn test_ollama_auth_status_always_returns_credential_present() {
        let auth = OllamaAuth::new("http://localhost:11434");
        let status = auth.status();
        assert!(
            status.has_credentials(),
            "Ollama status should always report credentials present"
        );
        assert!(
            matches!(status, AuthStatus::CredentialPresent { .. }),
            "Ollama status should be CredentialPresent, got: {:?}",
            status
        );
    }

    #[test]
    fn test_ollama_auth_status_summary_contains_credential_present() {
        let auth = OllamaAuth::new("http://localhost:11434");
        let summary = auth.status().summary();
        assert!(
            summary.contains("credential present"),
            "summary should contain 'credential present', got: {summary}"
        );
    }
}
