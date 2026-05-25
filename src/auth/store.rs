//! Secret storage abstraction with keyring and environment-variable backends.
//!
//! The [`SecretStore`] trait defines the interface. Two concrete implementations
//! are provided:
//!
//! - [`KeyringStore`]: persists secrets in the OS keyring / keychain.
//! - [`EnvVarStore`]: reads secrets from environment variables (read-only).
//!
//! Neither implementation ever logs or includes secret values in error messages.

use crate::error::{PipelineError, Result};

// ---------------------------------------------------------------------------
// SecretStore trait
// ---------------------------------------------------------------------------

/// A named secret storage backend.
///
/// Implementations MUST NOT log or include secret values in error messages.
/// Error messages should use generic phrasing such as `"keyring write error"`.
pub trait SecretStore: Send + Sync {
    /// Returns the service/namespace identifier for this store.
    fn service_name(&self) -> &str;

    /// Retrieves the secret for `key`.
    ///
    /// # Returns
    ///
    /// `Ok(Some(value))` when found, `Ok(None)` when absent, or
    /// `Err(PipelineError::Auth)` on unexpected backend failure.
    fn get_secret(&self, key: &str) -> Result<Option<String>>;

    /// Stores `value` under `key`, replacing any existing secret.
    ///
    /// # Errors
    ///
    /// Returns `PipelineError::Auth` on backend failure.
    fn set_secret(&self, key: &str, value: &str) -> Result<()>;

    /// Deletes the secret for `key`. Succeeds silently when absent.
    ///
    /// # Errors
    ///
    /// Returns `PipelineError::Auth` on backend failure (not on missing key).
    fn delete_secret(&self, key: &str) -> Result<()>;
}

// ---------------------------------------------------------------------------
// KeyringStore
// ---------------------------------------------------------------------------

/// OS keyring-backed secret store using the `keyring` crate.
///
/// On macOS this uses the system Keychain via the `apple-native` feature.
/// On Windows it uses the Windows Credential Manager via `windows-native`.
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    /// Creates a `KeyringStore` for the given service name.
    ///
    /// # Arguments
    ///
    /// * `service` - The keyring service name, e.g. `"xzardgz-openai"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::auth::store::KeyringStore;
    ///
    /// let store = KeyringStore::new("my-app");
    /// ```
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
}

impl SecretStore for KeyringStore {
    fn service_name(&self) -> &str {
        &self.service
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(&self.service, key)
            .map_err(|e| PipelineError::Auth(format!("keyring entry error: {e}")))?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            // Treat all retrieval failures (including "not found") as absent.
            Err(_) => Ok(None),
        }
    }

    fn set_secret(&self, key: &str, value: &str) -> Result<()> {
        let entry = keyring::Entry::new(&self.service, key)
            .map_err(|e| PipelineError::Auth(format!("keyring entry error: {e}")))?;
        entry
            .set_password(value)
            .map_err(|e| PipelineError::Auth(format!("keyring write error: {e}")))
    }

    fn delete_secret(&self, key: &str) -> Result<()> {
        let entry = keyring::Entry::new(&self.service, key)
            .map_err(|e| PipelineError::Auth(format!("keyring entry error: {e}")))?;
        // Treat delete failures (including "not found") as success.
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(_) => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// EnvVarStore
// ---------------------------------------------------------------------------

/// Environment-variable backed store (read-only; no write/delete support).
///
/// Used as a fallback when the keyring is unavailable or for providers that
/// expect secrets only in environment variables (e.g., CI pipelines).
///
/// The environment variable name is constructed by concatenating `prefix` and
/// `key`, then converting to uppercase. For example, prefix `"XZARDGZ_"` with
/// key `"api_key"` reads the variable `XZARDGZ_API_KEY`.
pub struct EnvVarStore {
    /// Prefix prepended to every key when looking up env vars.
    prefix: String,
}

impl EnvVarStore {
    /// Creates an `EnvVarStore` with the given prefix.
    ///
    /// # Arguments
    ///
    /// * `prefix` - Prepended to every key before env var lookup, e.g. `"XZARDGZ_"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::auth::store::EnvVarStore;
    ///
    /// let store = EnvVarStore::new("XZARDGZ_");
    /// ```
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    /// Builds the fully-qualified environment variable name for `key`.
    ///
    /// The result is always uppercased so that `"api_key"` and `"API_KEY"` both
    /// resolve to the same env var.
    fn env_var_name(&self, key: &str) -> String {
        let combined = format!("{}{}", self.prefix, key);
        combined.to_uppercase()
    }
}

impl SecretStore for EnvVarStore {
    fn service_name(&self) -> &str {
        &self.prefix
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>> {
        Ok(std::env::var(self.env_var_name(key)).ok())
    }

    fn set_secret(&self, _key: &str, _value: &str) -> Result<()> {
        Err(PipelineError::Auth("EnvVarStore is read-only".to_string()))
    }

    fn delete_secret(&self, _key: &str) -> Result<()> {
        Err(PipelineError::Auth("EnvVarStore is read-only".to_string()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // KeyringStore
    // ------------------------------------------------------------------

    #[test]
    fn test_keyring_store_service_name_returns_configured_name() {
        let store = KeyringStore::new("my-service");
        assert_eq!(store.service_name(), "my-service");
    }

    // ------------------------------------------------------------------
    // EnvVarStore::service_name
    // ------------------------------------------------------------------

    #[test]
    fn test_env_var_store_service_name_returns_prefix() {
        let store = EnvVarStore::new("XZARDGZ_");
        assert_eq!(store.service_name(), "XZARDGZ_");
    }

    // ------------------------------------------------------------------
    // EnvVarStore::get_secret
    // ------------------------------------------------------------------

    #[test]
    fn test_env_var_store_get_secret_returns_none_when_missing() {
        // Use a unique name guaranteed not to be set in any environment.
        let store = EnvVarStore::new("XZARDGZ_TEST_NEVER_SET_BQWERTY_");
        let result = store.get_secret("api_key");
        assert!(
            result.is_ok(),
            "get_secret should not fail: {:?}",
            result.err()
        );
        assert!(
            result.unwrap().is_none(),
            "get_secret should return None for unset env var"
        );
    }

    #[test]
    fn test_env_var_store_get_secret_returns_value_when_set() {
        // env_var_name = "XZARDGZ_TEST_STORE_" + "api_key" uppercased = "XZARDGZ_TEST_STORE_API_KEY"
        temp_env::with_var(
            "XZARDGZ_TEST_STORE_API_KEY",
            Some("test-secret-value"),
            || {
                let store = EnvVarStore::new("XZARDGZ_TEST_STORE_");
                let result = store.get_secret("api_key");
                assert!(
                    result.is_ok(),
                    "get_secret should not fail: {:?}",
                    result.err()
                );
                assert_eq!(
                    result.unwrap(),
                    Some("test-secret-value".to_string()),
                    "get_secret should return the env var value"
                );
            },
        );
    }

    // ------------------------------------------------------------------
    // EnvVarStore::set_secret (read-only)
    // ------------------------------------------------------------------

    #[test]
    fn test_env_var_store_set_secret_returns_error() {
        let store = EnvVarStore::new("XZARDGZ_");
        let result = store.set_secret("api_key", "some-value");
        assert!(
            result.is_err(),
            "set_secret should return error for read-only store"
        );
        assert!(
            matches!(result, Err(PipelineError::Auth(_))),
            "error should be PipelineError::Auth"
        );
    }

    // ------------------------------------------------------------------
    // EnvVarStore::delete_secret (read-only)
    // ------------------------------------------------------------------

    #[test]
    fn test_env_var_store_delete_secret_returns_error() {
        let store = EnvVarStore::new("XZARDGZ_");
        let result = store.delete_secret("api_key");
        assert!(
            result.is_err(),
            "delete_secret should return error for read-only store"
        );
        assert!(
            matches!(result, Err(PipelineError::Auth(_))),
            "error should be PipelineError::Auth"
        );
    }

    // ------------------------------------------------------------------
    // env_var_name uppercasing
    // ------------------------------------------------------------------

    #[test]
    fn test_env_var_store_name_is_uppercased() {
        let store = EnvVarStore::new("myapp_");
        // env_var_name("my_key") -> "MYAPP_MY_KEY"
        temp_env::with_var("MYAPP_MY_KEY", Some("uppercased"), || {
            let result = store.get_secret("my_key");
            assert_eq!(result.unwrap(), Some("uppercased".to_string()));
        });
    }
}
