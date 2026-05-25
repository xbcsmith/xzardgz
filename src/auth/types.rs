//! Shared authentication types for all provider auth flows.
//!
//! This module defines [`AuthStatus`], [`CredentialSource`], and the aggregate
//! [`AllProvidersStatus`] used by CLI commands and the `ProviderAuthManager`.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// AuthStatus
// ---------------------------------------------------------------------------

/// Full authentication status for a provider.
///
/// The variants progress from "no credentials at all" through "credentials
/// present but unvalidated" to "credentials verified against the live API".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthStatus {
    /// Credentials are present AND were validated against the API.
    Authenticated {
        /// Where the credential was sourced from.
        source: CredentialSource,
    },
    /// No credentials found in any location.
    NotAuthenticated {
        /// Human-readable reason (safe to display; must not contain secret values).
        reason: String,
    },
    /// Credentials exist but have not been validated (quick status check only).
    CredentialPresent {
        /// Where the credential was sourced from.
        source: CredentialSource,
    },
    /// Status check failed for a transient or non-auth reason.
    Unknown {
        /// Human-readable reason (safe to display; must not contain secret values).
        reason: String,
    },
}

impl AuthStatus {
    /// Returns `true` if credentials are present in some form.
    ///
    /// Returns `true` for both [`AuthStatus::Authenticated`] and
    /// [`AuthStatus::CredentialPresent`]; `false` for all other variants.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::auth::types::{AuthStatus, CredentialSource};
    ///
    /// let authenticated = AuthStatus::Authenticated {
    ///     source: CredentialSource::EnvironmentVariable { name: "MY_KEY".to_string() },
    /// };
    /// assert!(authenticated.has_credentials());
    ///
    /// let absent = AuthStatus::NotAuthenticated { reason: "no key".to_string() };
    /// assert!(!absent.has_credentials());
    /// ```
    pub fn has_credentials(&self) -> bool {
        matches!(
            self,
            Self::Authenticated { .. } | Self::CredentialPresent { .. }
        )
    }

    /// Returns a one-line human-readable status summary.
    ///
    /// The summary is safe to include in logs; it never contains secret values.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::auth::types::{AuthStatus, CredentialSource};
    ///
    /// let status = AuthStatus::NotAuthenticated { reason: "no key found".to_string() };
    /// assert!(status.summary().contains("not authenticated"));
    /// ```
    pub fn summary(&self) -> String {
        match self {
            Self::Authenticated { source } => {
                format!("authenticated ({})", source.label())
            }
            Self::CredentialPresent { source } => {
                format!("credential present ({})", source.label())
            }
            Self::NotAuthenticated { reason } => {
                format!("not authenticated: {reason}")
            }
            Self::Unknown { reason } => {
                format!("unknown: {reason}")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CredentialSource
// ---------------------------------------------------------------------------

/// Where a credential was loaded from.
///
/// Used inside [`AuthStatus`] variants to inform the operator where the
/// active credential originated without exposing the credential value itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialSource {
    /// Loaded from the named environment variable.
    EnvironmentVariable {
        /// The environment variable name — NOT its value.
        name: String,
    },
    /// Loaded from the OS keyring / keychain.
    Keyring {
        /// The keyring service name.
        service: String,
    },
}

impl CredentialSource {
    /// Returns a short human-readable label safe to include in logs.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::auth::types::CredentialSource;
    ///
    /// let src = CredentialSource::EnvironmentVariable { name: "API_KEY".to_string() };
    /// assert_eq!(src.label(), "env:API_KEY");
    ///
    /// let src = CredentialSource::Keyring { service: "my-app".to_string() };
    /// assert_eq!(src.label(), "keyring:my-app");
    /// ```
    pub fn label(&self) -> String {
        match self {
            Self::EnvironmentVariable { name } => format!("env:{name}"),
            Self::Keyring { service } => format!("keyring:{service}"),
        }
    }
}

// ---------------------------------------------------------------------------
// AllProvidersStatus
// ---------------------------------------------------------------------------

/// Aggregate authentication status for all supported providers.
///
/// Returned by [`crate::auth::ProviderAuthManager::status_all`] and printed
/// by the `auth status` CLI command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllProvidersStatus {
    /// Authentication status for the OpenAI provider.
    pub openai: AuthStatus,
    /// Authentication status for the Anthropic provider.
    pub anthropic: AuthStatus,
    /// Authentication status for the Ollama provider.
    pub ollama: AuthStatus,
    /// Authentication status for the GitHub Copilot provider.
    pub copilot: AuthStatus,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // AuthStatus::has_credentials
    // ------------------------------------------------------------------

    #[test]
    fn test_auth_status_has_credentials_returns_true_when_authenticated() {
        let status = AuthStatus::Authenticated {
            source: CredentialSource::EnvironmentVariable {
                name: "TEST_KEY".to_string(),
            },
        };
        assert!(status.has_credentials());
    }

    #[test]
    fn test_auth_status_has_credentials_returns_true_when_credential_present() {
        let status = AuthStatus::CredentialPresent {
            source: CredentialSource::Keyring {
                service: "test-service".to_string(),
            },
        };
        assert!(status.has_credentials());
    }

    #[test]
    fn test_auth_status_has_credentials_returns_false_when_not_authenticated() {
        let status = AuthStatus::NotAuthenticated {
            reason: "no key found".to_string(),
        };
        assert!(!status.has_credentials());
    }

    #[test]
    fn test_auth_status_has_credentials_returns_false_when_unknown() {
        let status = AuthStatus::Unknown {
            reason: "transient error".to_string(),
        };
        assert!(!status.has_credentials());
    }

    // ------------------------------------------------------------------
    // AuthStatus::summary
    // ------------------------------------------------------------------

    #[test]
    fn test_auth_status_summary_authenticated_contains_source_label() {
        let status = AuthStatus::Authenticated {
            source: CredentialSource::EnvironmentVariable {
                name: "MY_KEY".to_string(),
            },
        };
        let summary = status.summary();
        assert!(
            summary.contains("authenticated"),
            "summary should contain 'authenticated', got: {summary}"
        );
        assert!(
            summary.contains("env:MY_KEY"),
            "summary should contain source label, got: {summary}"
        );
    }

    #[test]
    fn test_auth_status_summary_credential_present_contains_source_label() {
        let status = AuthStatus::CredentialPresent {
            source: CredentialSource::Keyring {
                service: "my-service".to_string(),
            },
        };
        let summary = status.summary();
        assert!(
            summary.contains("credential present"),
            "summary should contain 'credential present', got: {summary}"
        );
        assert!(
            summary.contains("keyring:my-service"),
            "summary should contain source label, got: {summary}"
        );
    }

    #[test]
    fn test_auth_status_summary_not_authenticated_contains_reason() {
        let status = AuthStatus::NotAuthenticated {
            reason: "no api key".to_string(),
        };
        let summary = status.summary();
        assert!(
            summary.contains("not authenticated"),
            "summary should contain 'not authenticated', got: {summary}"
        );
        assert!(
            summary.contains("no api key"),
            "summary should contain reason, got: {summary}"
        );
    }

    #[test]
    fn test_auth_status_summary_unknown_contains_reason() {
        let status = AuthStatus::Unknown {
            reason: "network failure".to_string(),
        };
        let summary = status.summary();
        assert!(
            summary.contains("unknown"),
            "summary should contain 'unknown', got: {summary}"
        );
        assert!(
            summary.contains("network failure"),
            "summary should contain reason, got: {summary}"
        );
    }

    // ------------------------------------------------------------------
    // CredentialSource::label
    // ------------------------------------------------------------------

    #[test]
    fn test_credential_source_label_env_var_includes_name() {
        let source = CredentialSource::EnvironmentVariable {
            name: "API_KEY".to_string(),
        };
        assert_eq!(source.label(), "env:API_KEY");
    }

    #[test]
    fn test_credential_source_label_keyring_includes_service() {
        let source = CredentialSource::Keyring {
            service: "my-app".to_string(),
        };
        assert_eq!(source.label(), "keyring:my-app");
    }

    // ------------------------------------------------------------------
    // AllProvidersStatus
    // ------------------------------------------------------------------

    #[test]
    fn test_all_providers_status_fields_are_accessible() {
        let status = AllProvidersStatus {
            openai: AuthStatus::NotAuthenticated {
                reason: "no key".to_string(),
            },
            anthropic: AuthStatus::NotAuthenticated {
                reason: "no key".to_string(),
            },
            ollama: AuthStatus::CredentialPresent {
                source: CredentialSource::EnvironmentVariable {
                    name: "none".to_string(),
                },
            },
            copilot: AuthStatus::Unknown {
                reason: "oauth".to_string(),
            },
        };
        assert!(!status.openai.has_credentials());
        assert!(status.ollama.has_credentials());
        assert!(!status.copilot.has_credentials());
    }
}
