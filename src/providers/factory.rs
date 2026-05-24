use super::base::Provider;
use super::copilot::CopilotProvider;
use super::ollama::OllamaProvider;
use crate::config::Config;
use crate::error::{PipelineError, Result};
use std::sync::Arc;

/// Factory for constructing provider instances from a [`Config`].
///
/// Use [`ProviderFactory::create_from_config`] to obtain an `Arc<dyn Provider>`
/// whose concrete type is determined by `config.provider.default`.
pub struct ProviderFactory;

impl ProviderFactory {
    /// Creates an `Arc<dyn Provider>` from the given [`Config`].
    ///
    /// Reads `config.provider.default` to select the provider backend:
    ///
    /// - `"ollama"` — uses the Ollama local inference server at
    ///   `config.ollama.host` with model `config.ollama.model`.
    /// - `"copilot"` — uses GitHub Copilot via OAuth token with model
    ///   `config.copilot.model`.
    /// - `"openai"` — returns a [`PipelineError::Provider`] indicating
    ///   the OpenAI provider is not yet implemented.
    /// - `"anthropic"` — returns a [`PipelineError::Provider`] indicating
    ///   the Anthropic provider is not yet implemented.
    /// - Any other value — returns a [`PipelineError::Provider`] with the
    ///   unrecognized provider name.
    pub fn create_from_config(config: &Config) -> Result<Arc<dyn Provider>> {
        match config.provider.default.as_str() {
            "ollama" => Ok(Arc::new(OllamaProvider::new(
                config.ollama.host.clone(),
                config.ollama.model.clone(),
            ))),
            "copilot" => Ok(Arc::new(CopilotProvider::new(
                config.copilot.model.clone(),
            ))),
            "openai" => Err(PipelineError::Provider(
                "openai provider is not yet implemented; use XZARDGZ_PROVIDER=ollama or XZARDGZ_PROVIDER=copilot"
                    .to_string(),
            )),
            "anthropic" => Err(PipelineError::Provider(
                "anthropic provider is not yet implemented".to_string(),
            )),
            _ => Err(PipelineError::Provider(format!(
                "unknown provider: '{}'",
                config.provider.default
            ))),
        }
    }
}
