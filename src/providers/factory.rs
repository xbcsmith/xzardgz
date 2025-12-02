use super::base::Provider;
use super::copilot::CopilotProvider;
use super::ollama::OllamaProvider;
use crate::config::ProviderConfig;
use crate::error::ProviderError;

use std::sync::Arc;

pub struct ProviderFactory;

impl ProviderFactory {
    pub fn create(config: &ProviderConfig) -> Result<Arc<dyn Provider>, ProviderError> {
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
            _ => Err(ProviderError::Auth(format!(
                "Unknown provider: {}",
                config.provider_type
            ))),
        }
    }
}
