//! OpenAI API key authentication management.
//!
//! Supports reading keys from environment variables and the OS keyring.
//! Secret values are never logged or included in error messages.
//!
//! # Lookup order
//!
//! 1. Environment variable named by `api_key_env` (default: `OPENAI_API_KEY`).
//! 2. OS keyring entry under service `"xzardgz-openai"`, key `"api-key"`.

use crate::error::Result;

use super::store::{KeyringStore, SecretStore};
use super::types::{AuthStatus, CredentialSource};

/// Keyring service name for OpenAI credentials.
const KEYRING_SERVICE: &str = "xzardgz-openai";

/// Keyring entry key for the OpenAI API key.
const KEYRING_KEY: &str = "api-key";

// ---------------------------------------------------------------------------
// OpenAiAuth
// ---------------------------------------------------------------------------

/// Manages OpenAI API key storage and status queries.
///
/// Checks the configured environment variable first, then falls back to the
/// OS keyring. Secret values are never logged or included in error messages.
pub struct OpenAiAuth {
    /// Name of the environment variable to check for the API key.
    api_key_env: String,
    /// Keyring store used for persistent key storage.
    store: KeyringStore,
}

impl OpenAiAuth {
    /// Creates an `OpenAiAuth` using the given environment variable name.
    ///
    /// # Arguments
    ///
    /// * `api_key_env` - The env var name that holds the API key,
    ///   typically `"OPENAI_API_KEY"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::auth::OpenAiAuth;
    ///
    /// let auth = OpenAiAuth::new("OPENAI_API_KEY");
    /// ```
    pub fn new(api_key_env: impl Into<String>) -> Self {
        Self {
            api_key_env: api_key_env.into(),
            store: KeyringStore::new(KEYRING_SERVICE),
        }
    }

    /// Returns the API key if available, or `None` if not found.
    ///
    /// Checks the environment variable first, then the OS keyring.
    /// The returned value is sensitive — do not log it.
    ///
    /// # Returns
    ///
    /// `Some(key)` when a non-empty key is found, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::auth::OpenAiAuth;
    ///
    /// let auth = OpenAiAuth::new("OPENAI_API_KEY");
    /// let _key = auth.get_key(); // Some(key) or None
    /// ```
    pub fn get_key(&self) -> Option<String> {
        // 1. Environment variable.
        if let Ok(val) = std::env::var(&self.api_key_env)
            && !val.is_empty()
        {
            return Some(val);
        }
        // 2. OS keyring.
        self.store.get_secret(KEYRING_KEY).ok().flatten()
    }

    /// Returns the current [`AuthStatus`] without making any network call.
    ///
    /// Checks the environment variable first, then the OS keyring. No API
    /// validation is performed; the result reflects only whether a credential
    /// exists locally.
    ///
    /// # Returns
    ///
    /// - [`AuthStatus::CredentialPresent`] when a non-empty key is found.
    /// - [`AuthStatus::NotAuthenticated`] when no key is found anywhere.
    /// - [`AuthStatus::Unknown`] when the keyring itself returns an error.
    pub fn status(&self) -> AuthStatus {
        // 1. Environment variable.
        if let Ok(val) = std::env::var(&self.api_key_env)
            && !val.is_empty()
        {
            let _ = val; // bound by pattern; emptiness check was the only needed use
            return AuthStatus::CredentialPresent {
                source: CredentialSource::EnvironmentVariable {
                    name: self.api_key_env.clone(),
                },
            };
        }

        // 2. OS keyring.
        match self.store.get_secret(KEYRING_KEY) {
            Ok(Some(_)) => AuthStatus::CredentialPresent {
                source: CredentialSource::Keyring {
                    service: KEYRING_SERVICE.to_string(),
                },
            },
            Ok(None) => AuthStatus::NotAuthenticated {
                reason: format!(
                    "no API key found in env var '{}' or keyring",
                    self.api_key_env
                ),
            },
            Err(e) => AuthStatus::Unknown {
                reason: e.to_string(),
            },
        }
    }

    /// Stores an API key in the OS keyring.
    ///
    /// # Arguments
    ///
    /// * `key` - The API key to store. Value is not logged.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::PipelineError::Auth`] on keyring backend failure.
    pub fn set_key(&self, key: &str) -> Result<()> {
        self.store.set_secret(KEYRING_KEY, key)
    }

    /// Removes the API key from the OS keyring.
    ///
    /// Succeeds silently if no key is currently stored.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::PipelineError::Auth`] on keyring backend failure.
    pub fn remove_key(&self) -> Result<()> {
        self.store.delete_secret(KEYRING_KEY)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns an `OpenAiAuth` bound to a test-specific env var name that is
    /// guaranteed not to exist in any standard environment or have a keyring
    /// entry under the shared `KEYRING_SERVICE` service.
    ///
    /// Note: if `xzardgz auth set-key openai` has been run previously in the
    /// current user session, the keyring may contain a key and the "no
    /// credentials" assertions in tests below may be skipped with a note.
    fn test_auth(env_var: &str) -> OpenAiAuth {
        OpenAiAuth::new(env_var)
    }

    // ------------------------------------------------------------------
    // status()
    // ------------------------------------------------------------------

    #[test]
    fn test_openai_auth_status_returns_not_authenticated_when_no_credentials() {
        // Use a unique placeholder env var that will never be set in practice.
        // Assumes the keyring service "xzardgz-openai" has no stored key in
        // the current environment (guaranteed in CI).
        let auth = test_auth("XZARDGZ_TEST_OPENAI_NOT_SET_PLACEHOLDER");
        let status = auth.status();
        assert!(
            matches!(status, AuthStatus::NotAuthenticated { .. }),
            "expected NotAuthenticated but got: {:?}",
            status
        );
    }

    #[test]
    fn test_openai_auth_status_returns_credential_present_when_env_set() {
        temp_env::with_var(
            "XZARDGZ_TEST_OPENAI_ENV_KEY_1",
            Some("sk-test-value-openai"),
            || {
                let auth = test_auth("XZARDGZ_TEST_OPENAI_ENV_KEY_1");
                let status = auth.status();
                assert!(
                    status.has_credentials(),
                    "expected has_credentials=true, got: {:?}",
                    status
                );
                assert!(
                    matches!(
                        status,
                        AuthStatus::CredentialPresent {
                            source: CredentialSource::EnvironmentVariable { .. }
                        }
                    ),
                    "expected CredentialPresent(EnvironmentVariable), got: {:?}",
                    status
                );
            },
        );
    }

    // ------------------------------------------------------------------
    // get_key()
    // ------------------------------------------------------------------

    #[test]
    fn test_openai_auth_get_key_returns_env_var_value() {
        temp_env::with_var(
            "XZARDGZ_TEST_OPENAI_ENV_KEY_2",
            Some("sk-test-secret-openai"),
            || {
                let auth = test_auth("XZARDGZ_TEST_OPENAI_ENV_KEY_2");
                let key = auth.get_key();
                assert_eq!(
                    key,
                    Some("sk-test-secret-openai".to_string()),
                    "get_key should return the env var value"
                );
            },
        );
    }

    #[test]
    fn test_openai_auth_get_key_returns_none_when_no_env_var() {
        // Use a unique placeholder guaranteed not to be set.
        // Assumes keyring has no pre-existing key (guaranteed in CI).
        let auth = test_auth("XZARDGZ_TEST_OPENAI_NONE_PLACEHOLDER");
        let key = auth.get_key();
        assert!(
            key.is_none(),
            "get_key should return None when no env var or keyring key is present"
        );
    }

    // ------------------------------------------------------------------
    // set_key / remove_key (smoke test — no assertion on keyring state)
    // ------------------------------------------------------------------

    #[test]
    fn test_openai_auth_remove_key_succeeds_when_no_entry() {
        // delete_credential on a missing entry should succeed silently.
        let auth = test_auth("XZARDGZ_TEST_OPENAI_DUMMY");
        let result = auth.remove_key();
        assert!(
            result.is_ok(),
            "remove_key should succeed even when no keyring entry exists: {:?}",
            result.err()
        );
    }
}
