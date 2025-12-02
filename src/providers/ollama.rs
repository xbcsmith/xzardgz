use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

use super::base::Provider;
use super::types::{Message, ProviderCapabilities, ProviderMetadata, Role, Tool};
use crate::error::ProviderError;

#[derive(Clone)]
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(base_url: String, model: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            model,
        }
    }

    fn convert_role(role: &Role) -> String {
        match role {
            Role::System => "system".to_string(),
            Role::User => "user".to_string(),
            Role::Assistant => "assistant".to_string(),
            Role::Tool => "tool".to_string(),
        }
    }
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Tool>>,
}

#[derive(Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
    #[allow(dead_code)]
    done: bool,
}

#[async_trait]
impl Provider for OllamaProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            name: "ollama".to_string(),
            models: vec![self.model.clone()],
            capabilities: ProviderCapabilities {
                streaming: true,
                tools: true, // Ollama supports tools in newer versions
                vision: false,
            },
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<Message, ProviderError> {
        let url = format!("{}/api/chat", self.base_url);

        let ollama_messages: Vec<OllamaMessage> = messages
            .iter()
            .map(|m| OllamaMessage {
                role: match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => "system".to_string(),
                    Role::Tool => "tool".to_string(),
                },
                content: m.content.clone(),
            })
            .collect();

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: ollama_messages,
            stream: false,
            tools: if _tools.is_empty() {
                None
            } else {
                Some(_tools.to_vec())
            },
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::Api(format!(
                "Ollama API error: {}",
                response.status()
            )));
        }

        let ollama_response: OllamaResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::Serialization(e.to_string()))?;

        Ok(Message::assistant(&ollama_response.message.content))
    }

    async fn complete_streaming(
        &self,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Message, ProviderError>> + Send>>, ProviderError>
    {
        let url = format!("{}/api/chat", self.base_url);

        let ollama_messages: Vec<OllamaMessage> = messages
            .iter()
            .map(|m| OllamaMessage {
                role: Self::convert_role(&m.role),
                content: m.content.clone(),
            })
            .collect();

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: ollama_messages,
            stream: true,
            tools: if _tools.is_empty() {
                None
            } else {
                Some(_tools.to_vec())
            },
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| ProviderError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::Api(format!(
                "Ollama API error: {}",
                response.status()
            )));
        }

        let stream = response.bytes_stream().map(|result| match result {
            Ok(bytes) => {
                let chunk: serde_json::Result<OllamaResponse> = serde_json::from_slice(&bytes);
                match chunk {
                    Ok(res) => Ok(Message::assistant(&res.message.content)),
                    Err(e) => Err(ProviderError::Api(format!("JSON parse error: {}", e))),
                }
            }
            Err(e) => Err(ProviderError::Api(e.to_string())),
        });

        Ok(Box::pin(stream))
    }
}
