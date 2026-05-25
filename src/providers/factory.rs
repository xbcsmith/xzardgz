//! Provider factory.
//!
//! Creates [`Provider`] instances from application configuration.
//! OpenAI is the primary (default) provider.
//!
//! # Usage
//!
//! ```
//! use xzardgz::config::Config;
//! use xzardgz::providers::ProviderFactory;
//!
//! let config = Config::default();
//! let provider = ProviderFactory::create_from_config(&config)
//!     .expect("default config creates a valid OpenAI provider");
//! assert_eq!(provider.provider_name(), "openai");
//! ```

use std::sync::Arc;

use crate::config::Config;
use crate::error::{PipelineError, Result};

use super::anthropic::AnthropicProvider;
use super::base::Provider;
use super::copilot::CopilotProvider;
use super::ollama::OllamaProvider;
use super::openai::OpenAiProvider;

/// Factory for constructing [`Provider`] instances from [`Config`].
///
/// Use [`ProviderFactory::create_from_config`] to obtain an
/// `Arc<dyn Provider>` whose concrete type is determined by
/// `config.provider.default`.
pub struct ProviderFactory;

impl ProviderFactory {
    /// Returns the canonical default provider name: `"openai"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::providers::ProviderFactory;
    ///
    /// assert_eq!(ProviderFactory::default_provider_name(), "openai");
    /// ```
    pub fn default_provider_name() -> &'static str {
        "openai"
    }

    /// Creates an `Arc<dyn Provider>` from `config.provider.default`.
    ///
    /// Selects the provider backend based on `config.provider.default`:
    ///
    /// | Value           | Provider                          |
    /// |-----------------|-----------------------------------|
    /// | `"openai"` / `""`| [`OpenAiProvider`] (default)     |
    /// | `"anthropic"`   | [`AnthropicProvider`]             |
    /// | `"ollama"`      | [`OllamaProvider`]                |
    /// | `"copilot"`     | [`CopilotProvider`]               |
    /// | anything else   | `Err(PipelineError::Provider)`    |
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Config`] if the OpenAI endpoint is insecure
    /// (see [`OpenAiProvider::new`]).
    ///
    /// Returns [`PipelineError::Provider`] for unrecognised provider names.
    pub fn create_from_config(config: &Config) -> Result<Arc<dyn Provider>> {
        match config.provider.default.as_str() {
            "openai" | "" => Ok(Arc::new(OpenAiProvider::from_config(config)?)),
            "anthropic" => Ok(Arc::new(AnthropicProvider::from_config(config))),
            "ollama" => Ok(Arc::new(OllamaProvider::new(
                config.ollama.host.clone(),
                config.ollama.model.clone(),
            ))),
            "copilot" => Ok(Arc::new(CopilotProvider::new(config.copilot.model.clone()))),
            other => Err(PipelineError::Provider(format!(
                "unknown provider: '{other}'"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    // ------------------------------------------------------------------
    // default_provider_name
    // ------------------------------------------------------------------

    #[test]
    fn test_factory_default_provider_name_is_openai() {
        assert_eq!(ProviderFactory::default_provider_name(), "openai");
    }

    // ------------------------------------------------------------------
    // create_from_config
    // ------------------------------------------------------------------

    #[test]
    fn test_factory_creates_openai_provider_for_openai_config() {
        let mut config = Config::default();
        config.provider.default = "openai".to_string();
        let result = ProviderFactory::create_from_config(&config);
        assert!(result.is_ok(), "expected Ok for openai provider");
        // SAFETY: checked is_ok above.
        assert_eq!(result.unwrap().provider_name(), "openai");
    }

    #[test]
    fn test_factory_openai_is_used_when_provider_is_empty_string() {
        let mut config = Config::default();
        config.provider.default = String::new();
        let result = ProviderFactory::create_from_config(&config);
        assert!(result.is_ok(), "expected Ok for empty provider string");
        // SAFETY: checked is_ok above.
        assert_eq!(result.unwrap().provider_name(), "openai");
    }

    #[test]
    fn test_factory_creates_anthropic_provider_for_anthropic_config() {
        let mut config = Config::default();
        config.provider.default = "anthropic".to_string();
        let result = ProviderFactory::create_from_config(&config);
        assert!(result.is_ok(), "expected Ok for anthropic provider");
        // SAFETY: checked is_ok above.
        assert_eq!(result.unwrap().provider_name(), "anthropic");
    }

    #[test]
    fn test_factory_creates_ollama_provider_for_ollama_config() {
        let mut config = Config::default();
        config.provider.default = "ollama".to_string();
        let result = ProviderFactory::create_from_config(&config);
        assert!(result.is_ok(), "expected Ok for ollama provider");
        // SAFETY: checked is_ok above.
        assert_eq!(result.unwrap().provider_name(), "ollama");
    }

    #[test]
    fn test_factory_creates_copilot_provider_for_copilot_config() {
        let mut config = Config::default();
        config.provider.default = "copilot".to_string();
        let result = ProviderFactory::create_from_config(&config);
        assert!(result.is_ok(), "expected Ok for copilot provider");
        // SAFETY: checked is_ok above.
        assert_eq!(result.unwrap().provider_name(), "copilot");
    }

    #[test]
    fn test_factory_rejects_unknown_provider() {
        let mut config = Config::default();
        config.provider.default = "hypothetical-ai".to_string();
        let result = ProviderFactory::create_from_config(&config);
        assert!(
            matches!(result, Err(PipelineError::Provider(_))),
            "expected Provider error for unknown backend"
        );
    }

    #[test]
    fn test_factory_rejects_unknown_provider_message_contains_name() {
        let mut config = Config::default();
        config.provider.default = "mystery-backend".to_string();
        let result = ProviderFactory::create_from_config(&config);
        // SAFETY: we asserted the error variant above; safe to pattern-match.
        let err_msg = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected Err but got Ok"),
        };
        assert!(
            err_msg.contains("mystery-backend"),
            "error message should contain the unknown provider name, got: {err_msg}"
        );
    }
}
