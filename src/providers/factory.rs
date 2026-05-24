use super::base::Provider;
use super::copilot::CopilotProvider;
use super::ollama::OllamaProvider;
use crate::config::ProviderConfig;
use crate::error::{PipelineError, Result};
use std::sync::Arc;

/// Factory for constructing provider instances from a [`ProviderConfig`].
pub struct ProviderFactory;

impl ProviderFactory {
    /// Creates an `Arc<dyn Provider>` from the given [`ProviderConfig`].
    ///
    /// Returns `PipelineError::Provider` if the `provider_type` field is not recognized.
    pub fn create(config: &ProviderConfig) -> Result<Arc<dyn Provider>> {
        match config.provider_type.as_str() {
            "ollama" => {
                let model = config
                    .model
                    .clone()
                    .unwrap_or_else(|| "qwen2.5-coder".to_string());
                Ok(Arc::new(OllamaProvider::new(
                    "http://localhost:11434".to_string(),
                    model,
                )))
            }
            "copilot" => {
                let model = config.model.clone().unwrap_or_else(|| "gpt-4".to_string());
                Ok(Arc::new(CopilotProvider::new(model)))
            }
            _ => Err(PipelineError::Provider(format!(
                "unknown provider type: {}",
                config.provider_type
            ))),
        }
    }
}
