//! GitHub Copilot provider.
//!
//! Implements [`Provider`] for the GitHub Copilot chat completions endpoint
//! (`api.githubcopilot.com`), using OAuth tokens managed by [`CopilotAuth`].
//!
//! # Features
//!
//! - Non-streaming completion via `POST /chat/completions`
//! - SSE streaming with `"stream": true`
//! - Credential status via keyring inspection (no network call)
//! - Static model list (Copilot does not expose a model listing endpoint)

use std::pin::Pin;
use std::time::Duration;

use async_stream::stream;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::base::Provider;
use super::copilot_auth::CopilotAuth;
use super::types::{
    CredentialStatus, Message, ModelCapabilities, ModelMetadata, ProviderCapabilities,
    ProviderMetadata, Role, ThinkingMode, Tool,
};
use crate::error::{PipelineError, Result};

// ---------------------------------------------------------------------------
// API constants
// ---------------------------------------------------------------------------

const COPILOT_COMPLETIONS_URL: &str = "https://api.githubcopilot.com/chat/completions";
const KEYRING_SERVICE: &str = "xzardgz-copilot";
const KEYRING_USER: &str = "oauth-token";

// ---------------------------------------------------------------------------
// Provider struct
// ---------------------------------------------------------------------------

/// GitHub Copilot chat completions provider.
///
/// Uses OAuth device-flow tokens stored in the system keyring to authenticate
/// with `api.githubcopilot.com`.
///
/// Construct via [`CopilotProvider::new`].
#[derive(Clone)]
pub struct CopilotProvider {
    model: String,
    client: Client,
}

impl CopilotProvider {
    /// Creates a new `CopilotProvider`.
    ///
    /// # Arguments
    ///
    /// * `model` - Model identifier (e.g. `"gpt-4o"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::providers::copilot::CopilotProvider;
    ///
    /// let provider = CopilotProvider::new("gpt-4o".to_string());
    /// assert_eq!(provider.provider_name_str(), "copilot");
    /// ```
    pub fn new(model: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            // SAFETY: no custom TLS config; build() only fails on OS-level
            // TLS initialisation failure.
            .expect("failed to build reqwest client");

        Self { model, client }
    }

    /// Returns the short provider name string (always `"copilot"`).
    pub fn provider_name_str(&self) -> &'static str {
        "copilot"
    }

    /// Obtains a GitHub Copilot OAuth token.
    ///
    /// Tries the keyring first; if absent, initiates the OAuth device flow.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Auth`] when the token cannot be obtained.
    async fn get_token(&self) -> Result<String> {
        let auth = CopilotAuth::new()?;
        let token = auth.get_token().await?;
        Ok(token)
    }

    /// Returns the static list of models supported by GitHub Copilot.
    ///
    /// Copilot does not expose a model listing endpoint so this table is
    /// used by [`list_models`][Provider::list_models].
    pub fn static_copilot_models() -> Vec<ModelMetadata> {
        vec![ModelMetadata::new(
            "gpt-4o",
            ModelCapabilities {
                supports_tools: false,
                supports_structured_output: false,
                supports_thinking: false,
                supports_streaming: true,
                supports_vision: false,
                context_window_tokens: 128_000,
            },
        )]
    }
}

// ---------------------------------------------------------------------------
// Wire-format types
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Provider trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Provider for CopilotProvider {
    /// Returns the provider name `"copilot"`.
    fn provider_name(&self) -> &str {
        "copilot"
    }

    /// Returns aggregate metadata for the Copilot provider.
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

    /// Returns `false`; Copilot does not support extended thinking.
    fn supports_thinking(&self) -> bool {
        false
    }

    /// Checks for a Copilot OAuth token in the system keyring.
    ///
    /// - [`CredentialStatus::Present`] — token found in the keyring.
    /// - [`CredentialStatus::Missing`] — no token entry in the keyring.
    /// - [`CredentialStatus::Unknown`] — keyring unavailable or lookup error.
    async fn credential_status(&self) -> CredentialStatus {
        match keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
            Ok(entry) => match entry.get_password() {
                Ok(pw) if !pw.is_empty() => CredentialStatus::Present,
                Ok(_) => CredentialStatus::Missing,
                Err(_) => CredentialStatus::Missing,
            },
            Err(_) => CredentialStatus::Unknown,
        }
    }

    /// Returns the static Copilot model list.
    ///
    /// # Errors
    ///
    /// Never returns `Err`.
    async fn list_models(&self) -> Result<Vec<ModelMetadata>> {
        Ok(Self::static_copilot_models())
    }

    /// Sends a non-streaming chat completion to the Copilot API.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Auth`] when the OAuth token cannot be obtained.
    /// Returns [`PipelineError::Provider`] on API or network failures.
    async fn complete(&self, messages: &[Message], _tools: &[Tool]) -> Result<Message> {
        let token = self.get_token().await?;

        let copilot_messages: Vec<CopilotMessage> = messages
            .iter()
            .map(|m| CopilotMessage {
                role: match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                }
                .to_string(),
                content: m.content.clone(),
            })
            .collect();

        let request = CopilotRequest {
            model: self.model.clone(),
            messages: copilot_messages,
            stream: false,
        };

        let response = self
            .client
            .post(COPILOT_COMPLETIONS_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "vscode/1.85.0")
            .json(&request)
            .send()
            .await
            .map_err(|e| PipelineError::Provider(format!("Copilot request failed: {e}")))?;

        if !response.status().is_success() {
            // SAFETY: already in error path; empty body is acceptable.
            let error_text = response.text().await.unwrap_or_default();
            return Err(PipelineError::Provider(format!(
                "Copilot API error: {error_text}"
            )));
        }

        let copilot_response: CopilotResponse = response.json().await.map_err(|e| {
            PipelineError::Provider(format!("failed to parse Copilot response: {e}"))
        })?;

        let content = copilot_response
            .choices
            .first()
            .map(|c| c.message.content.as_str())
            .unwrap_or("");

        Ok(Message::assistant(content))
    }

    /// Ignores `thinking_mode` and delegates to [`complete`][CopilotProvider::complete].
    ///
    /// Copilot does not support extended thinking.
    ///
    /// # Errors
    ///
    /// Same as [`complete`][CopilotProvider::complete].
    async fn complete_with_thinking(
        &self,
        messages: &[Message],
        tools: &[Tool],
        thinking_mode: ThinkingMode,
    ) -> Result<Message> {
        let _ = thinking_mode;
        self.complete(messages, tools).await
    }

    /// Streams a chat completion from the Copilot API via SSE.
    ///
    /// Yields each non-empty text delta as a `Message::assistant` item.
    ///
    /// # Errors
    ///
    /// Returns `Err` immediately if the initial request fails or the server
    /// returns a non-2xx status.  Mid-stream errors are yielded as `Err` items.
    async fn complete_streaming(
        &self,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Message>> + Send>>> {
        let token = self.get_token().await?;

        let copilot_messages: Vec<CopilotMessage> = messages
            .iter()
            .map(|m| CopilotMessage {
                role: match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                }
                .to_string(),
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
            .post(COPILOT_COMPLETIONS_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "vscode/1.85.1")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                PipelineError::Provider(format!("Copilot streaming request failed: {e}"))
            })?;

        if !response.status().is_success() {
            return Err(PipelineError::Provider(format!(
                "Copilot API error: {}",
                response.status()
            )));
        }

        let byte_stream = response.bytes_stream();
        let s = stream! {
            let mut byte_stream = byte_stream;
            let mut buffer = String::new();

            while let Some(chunk) = byte_stream.next().await {
                match chunk {
                    Err(e) => {
                        yield Err(PipelineError::Provider(format!("stream read error: {e}")));
                        return;
                    }
                    Ok(bytes) => match String::from_utf8(bytes.to_vec()) {
                        Err(e) => {
                            yield Err(PipelineError::Provider(format!("UTF-8 decode error: {e}")));
                            return;
                        }
                        Ok(text) => buffer.push_str(&text),
                    },
                }

                // Parse SSE lines.
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() || line == "data: [DONE]" {
                        continue;
                    }

                    if let Some(json_str) = line.strip_prefix("data: ")
                        && let Ok(res) =
                            serde_json::from_str::<CopilotStreamResponse>(json_str)
                        && let Some(choice) = res.choices.first()
                    {
                        let content = &choice.delta.content;
                        if !content.is_empty() {
                            yield Ok(Message::assistant(content.as_str()));
                        }
                    }
                }
            }
        };

        Ok(Box::pin(s))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider() -> CopilotProvider {
        CopilotProvider::new("gpt-4o".to_string())
    }

    // ------------------------------------------------------------------
    // provider_name
    // ------------------------------------------------------------------

    #[test]
    fn test_copilot_provider_name_returns_copilot() {
        let provider = make_provider();
        assert_eq!(provider.provider_name(), "copilot");
    }

    #[test]
    fn test_copilot_provider_name_str_returns_copilot() {
        let provider = make_provider();
        assert_eq!(provider.provider_name_str(), "copilot");
    }

    // ------------------------------------------------------------------
    // supports_thinking
    // ------------------------------------------------------------------

    #[test]
    fn test_copilot_provider_supports_thinking_returns_false() {
        let provider = make_provider();
        assert!(!provider.supports_thinking());
    }

    // ------------------------------------------------------------------
    // metadata
    // ------------------------------------------------------------------

    #[test]
    fn test_copilot_metadata_name_is_copilot() {
        let provider = make_provider();
        assert_eq!(provider.metadata().name, "copilot");
    }

    #[test]
    fn test_copilot_metadata_contains_configured_model() {
        let provider = make_provider();
        assert!(provider.metadata().models.contains(&"gpt-4o".to_string()));
    }

    #[test]
    fn test_copilot_metadata_tools_is_false() {
        let provider = make_provider();
        assert!(!provider.metadata().capabilities.tools);
    }

    // ------------------------------------------------------------------
    // static_copilot_models
    // ------------------------------------------------------------------

    #[test]
    fn test_copilot_static_models_contains_gpt4o() {
        let models = CopilotProvider::static_copilot_models();
        assert!(models.iter().any(|m| m.id == "gpt-4o"));
    }

    #[test]
    fn test_copilot_static_models_gpt4o_supports_streaming() {
        let models = CopilotProvider::static_copilot_models();
        let gpt4o = models.iter().find(|m| m.id == "gpt-4o").expect("gpt-4o");
        assert!(gpt4o.capabilities.supports_streaming);
    }

    // ------------------------------------------------------------------
    // list_models
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_copilot_list_models_returns_static_list() {
        let provider = make_provider();
        // SAFETY: list_models for Copilot always returns Ok.
        let models = provider.list_models().await.unwrap();
        assert!(!models.is_empty());
    }
}
