use super::base::Provider;
use super::copilot_auth::CopilotAuth;
use super::types::{Message, ProviderCapabilities, ProviderMetadata, Role, Tool};
use crate::error::ProviderError;
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

#[derive(Clone)]
pub struct CopilotProvider {
    model: String,
    client: Client,
}

impl CopilotProvider {
    pub fn new(model: String) -> Self {
        Self {
            model,
            client: Client::new(),
        }
    }

    async fn get_token(&self) -> Result<String, ProviderError> {
        // In a real app, we'd share the auth instance or token
        // For now, instantiate auth to get token (it handles caching)
        let auth = CopilotAuth::new()?;
        auth.get_token().await
    }
}

#[derive(Serialize)]
struct CopilotRequest {
    model: String,
    messages: Vec<CopilotMessage>,
    stream: bool,
}

#[derive(Serialize, Deserialize)]
struct CopilotMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct CopilotResponse {
    choices: Vec<CopilotChoice>,
}

#[derive(Deserialize)]
struct CopilotChoice {
    message: CopilotMessage,
}

#[derive(Deserialize)]
struct CopilotStreamResponse {
    choices: Vec<CopilotStreamChoice>,
}

#[derive(Deserialize)]
struct CopilotStreamChoice {
    delta: CopilotMessage,
}

#[async_trait]
impl Provider for CopilotProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            name: "copilot".to_string(),
            models: vec![self.model.clone()],
            capabilities: ProviderCapabilities {
                streaming: true,
                tools: false,
                vision: false,
            },
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<Message, ProviderError> {
        let token = self.get_token().await?;

        let copilot_messages: Vec<CopilotMessage> = messages
            .iter()
            .map(|m| CopilotMessage {
                role: match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => "system".to_string(),
                    Role::Tool => "tool".to_string(),
                },
                content: m.content.clone(),
            })
            .collect();

        let request = CopilotRequest {
            messages: copilot_messages,
            model: self.model.clone(),
            stream: false,
        };

        let response = self
            .client
            .post("https://api.githubcopilot.com/chat/completions")
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "vscode/1.85.0")
            .json(&request)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api(format!(
                "Copilot API error: {}",
                error_text
            )));
        }

        let copilot_response: CopilotResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::Serialization(e.to_string()))?;

        Ok(Message::assistant(
            &copilot_response.choices[0].message.content,
        ))
    }

    async fn complete_streaming(
        &self,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Message, ProviderError>> + Send>>, ProviderError>
    {
        let token = self.get_token().await?;

        let copilot_messages: Vec<CopilotMessage> = messages
            .iter()
            .map(|m| CopilotMessage {
                role: match m.role {
                    Role::System => "system".to_string(),
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::Tool => "tool".to_string(),
                },
                content: m.content.clone(),
            })
            .collect();

        let request = CopilotRequest {
            model: self.model.clone(),
            messages: copilot_messages,
            stream: true,
        };

        let response = self
            .client
            .post("https://api.githubcopilot.com/chat/completions")
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "vscode/1.85.1")
            .json(&request)
            .send()
            .await
            .map_err(|e| ProviderError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::Api(format!(
                "Copilot API error: {}",
                response.status()
            )));
        }

        let stream = response.bytes_stream().map(|result| {
            match result {
                Ok(bytes) => {
                    // Simplified SSE parsing
                    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                        if text.starts_with("data: [DONE]") {
                            return Ok(Message::assistant("")); // End of stream
                        }
                        if let Some(json_str) = text.strip_prefix("data: ") {
                            #[allow(clippy::collapsible_if)]
                            if let Ok(res) = serde_json::from_str::<CopilotStreamResponse>(json_str)
                            {
                                if let Some(choice) = res.choices.first() {
                                    return Ok(Message::assistant(&choice.delta.content));
                                }
                            }
                        }
                    }
                    Ok(Message::assistant("")) // Skip invalid chunks
                }
                Err(e) => Err(ProviderError::Api(e.to_string())),
            }
        });

        Ok(Box::pin(stream))
    }
}
