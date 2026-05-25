//! Provider authentication management for the XZardgz pipeline.
//!
//! This module provides per-provider auth helpers and a high-level
//! [`ProviderAuthManager`] used by CLI commands.
//!
//! # Submodules
//!
//! | Module      | Provider   | Notes                               |
//! |-------------|------------|-------------------------------------|
//! | `openai`    | OpenAI     | API key via env var or keyring      |
//! | `anthropic` | Anthropic  | API key via env var or keyring      |
//! | `ollama`    | Ollama     | No credentials; host reachability   |
//! | `store`     | (shared)   | [`SecretStore`] trait + backends    |
//! | `types`     | (shared)   | [`AuthStatus`], [`CredentialSource`]|
//!
//! # Design
//!
//! - Auth helpers read credentials from environment variables first, then from
//!   the OS keyring. Secret values are never logged or included in errors.
//! - [`ProviderAuthManager`] aggregates all helpers and exposes a single
//!   [`ProviderAuthManager::status_all`] call for the `auth status` CLI command.
//! - OpenAI is shown first in all status output per the Phase 9 spec.

pub mod anthropic;
pub mod ollama;
pub mod openai;
pub mod store;
pub mod types;

pub use anthropic::AnthropicAuth;
pub use ollama::OllamaAuth;
pub use openai::OpenAiAuth;
pub use store::{EnvVarStore, KeyringStore, SecretStore};
pub use types::{AllProvidersStatus, AuthStatus, CredentialSource};

use crate::config::Config;

// ---------------------------------------------------------------------------
// ProviderAuthManager
// ---------------------------------------------------------------------------

/// High-level authentication manager for all supported providers.
///
/// Constructed from a [`Config`] and used by CLI auth commands to check,
/// set, and remove credentials for each provider.
///
/// OpenAI is treated as the primary provider and is listed first in all
/// status output per the Phase 9 spec.
pub struct ProviderAuthManager {
    /// Auth helper for the OpenAI provider.
    pub openai: OpenAiAuth,
    /// Auth helper for the Anthropic provider.
    pub anthropic: AnthropicAuth,
    /// Auth/reachability helper for the Ollama provider.
    pub ollama: OllamaAuth,
}

impl ProviderAuthManager {
    /// Creates a `ProviderAuthManager` from pipeline configuration.
    ///
    /// Each auth helper is initialised with the relevant sub-config fields:
    /// - `config.openai.api_key_env` for [`OpenAiAuth`].
    /// - `config.anthropic.api_key_env` for [`AnthropicAuth`].
    /// - `config.ollama.host` for [`OllamaAuth`].
    ///
    /// # Arguments
    ///
    /// * `config` - Reference to the pipeline [`Config`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::auth::ProviderAuthManager;
    /// use xzardgz::config::Config;
    ///
    /// let manager = ProviderAuthManager::from_config(&Config::default());
    /// ```
    pub fn from_config(config: &Config) -> Self {
        Self {
            openai: OpenAiAuth::new(config.openai.api_key_env.clone()),
            anthropic: AnthropicAuth::new(config.anthropic.api_key_env.clone()),
            ollama: OllamaAuth::new(config.ollama.host.clone()),
        }
    }

    /// Returns the [`AuthStatus`] for all supported providers.
    ///
    /// OpenAI is listed first as the primary provider.
    /// Copilot authentication is managed by its own OAuth flow in
    /// `src/providers/copilot_auth.rs`; its status is reported as
    /// [`AuthStatus::Unknown`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::auth::ProviderAuthManager;
    /// use xzardgz::config::Config;
    ///
    /// let manager = ProviderAuthManager::from_config(&Config::default());
    /// let all = manager.status_all();
    /// let _ = all.openai.summary();
    /// ```
    pub fn status_all(&self) -> AllProvidersStatus {
        AllProvidersStatus {
            openai: self.openai.status(),
            anthropic: self.anthropic.status(),
            ollama: self.ollama.status(),
            copilot: AuthStatus::Unknown {
                reason: "copilot auth managed separately via OAuth".to_string(),
            },
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
    // ProviderAuthManager::from_config
    // ------------------------------------------------------------------

    #[test]
    fn test_provider_auth_manager_from_config_creates_successfully() {
        let config = Config::default();
        // Construction must not panic or fail.
        let manager = ProviderAuthManager::from_config(&config);
        // Verify the hosts/env-var names round-trip through from_config.
        assert_eq!(manager.ollama.host(), config.ollama.host);
    }

    // ------------------------------------------------------------------
    // ProviderAuthManager::status_all
    // ------------------------------------------------------------------

    #[test]
    fn test_provider_auth_manager_status_all_includes_all_providers() {
        let config = Config::default();
        let manager = ProviderAuthManager::from_config(&config);
        let all = manager.status_all();

        // Every field must be reachable (no panic) and produce a summary string.
        let _ = all.openai.summary();
        let _ = all.anthropic.summary();
        let _ = all.ollama.summary();
        let _ = all.copilot.summary();

        // Ollama always reports CredentialPresent.
        assert!(all.ollama.has_credentials());

        // Copilot is reported as Unknown (OAuth-managed).
        assert!(matches!(all.copilot, AuthStatus::Unknown { .. }));
    }

    #[test]
    fn test_provider_auth_manager_status_all_openai_shows_not_authenticated_when_no_env() {
        // Clear the default OPENAI_API_KEY so we get a deterministic result.
        // Assumes the keyring service "xzardgz-openai" has no key in CI.
        temp_env::with_var("OPENAI_API_KEY", None::<&str>, || {
            let config = Config::default();
            let manager = ProviderAuthManager::from_config(&config);
            let all = manager.status_all();
            // With no env var and no keyring key, should be NotAuthenticated.
            assert!(
                matches!(all.openai, AuthStatus::NotAuthenticated { .. }),
                "expected NotAuthenticated for openai when no env var, got: {:?}",
                all.openai
            );
        });
    }

    #[test]
    fn test_provider_auth_manager_status_all_copilot_is_unknown() {
        let manager = ProviderAuthManager::from_config(&Config::default());
        let all = manager.status_all();
        assert!(
            matches!(all.copilot, AuthStatus::Unknown { .. }),
            "copilot status should always be Unknown (OAuth-managed)"
        );
    }
}
