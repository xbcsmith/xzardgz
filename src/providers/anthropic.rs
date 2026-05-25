//! Anthropic Messages API provider.
//!
//! Implements [`Provider`] for the Anthropic Claude model family, targeting
//! `https://api.anthropic.com/v1/messages`.
//!
//! # Features
//!
//! - Non-streaming completion via the Anthropic Messages API
//! - SSE streaming with `"stream": true`
//! - Extended thinking support via `"thinking"` request block (Claude 3.5+)
//! - Static model table for offline capability resolution

use std::pin::Pin;
use std::time::Duration;

use async_stream::stream;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::warn;

use super::base::Provider;
use super::types::{
    CredentialStatus, FunctionCall, Message, ModelCapabilities, ModelMetadata,
    ProviderCapabilities, ProviderMetadata, Role, ThinkingMode, Tool, ToolCall,
};
use crate::config::{AnthropicConfig, Config, ProviderDefaultsConfig};
use crate::error::{PipelineError, Result};

// ---------------------------------------------------------------------------
// API constants
// ---------------------------------------------------------------------------

const ANTHROPIC_MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION_HEADER: &str = "2023-06-01";

// ---------------------------------------------------------------------------
// Provider struct
// ---------------------------------------------------------------------------

/// Anthropic Claude provider.
///
/// Talks to `https://api.anthropic.com/v1/messages` using the Anthropic
/// Messages API.  Always uses HTTPS; no endpoint validation is required.
///
/// Construct via [`AnthropicProvider::new`] (explicit config) or
/// [`AnthropicProvider::from_config`] (top-level app config).
#[derive(Clone)]
pub struct AnthropicProvider {
    config: AnthropicConfig,
    defaults: ProviderDefaultsConfig,
    client: Client,
}

impl AnthropicProvider {
    /// Creates a new `AnthropicProvider` from explicit configuration values.
    ///
    /// Builds a [`reqwest::Client`] with the timeout from
    /// `defaults.timeout_seconds`.
    ///
    /// # Arguments
    ///
    /// * `config` - Anthropic-specific settings (model, API key env-var).
    /// * `defaults` - Shared provider defaults (timeout, max-tokens, etc.).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::config::{AnthropicConfig, ProviderDefaultsConfig};
    /// use xzardgz::providers::anthropic::AnthropicProvider;
    ///
    /// let provider = AnthropicProvider::new(
    ///     AnthropicConfig::default(),
    ///     ProviderDefaultsConfig::default(),
    /// );
    /// assert_eq!(provider.provider_name_str(), "anthropic");
    /// ```
    pub fn new(config: AnthropicConfig, defaults: ProviderDefaultsConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(defaults.timeout_seconds))
            .build()
            // SAFETY: no custom TLS config is passed; build() can only fail
            // when native-TLS initialisation fails at the OS level, which is
            // not a recoverable application error.
            .expect("failed to build reqwest client");

        Self {
            config,
            defaults,
            client,
        }
    }

    /// Creates an `AnthropicProvider` from the top-level application [`Config`].
    ///
    /// Equivalent to `AnthropicProvider::new(config.anthropic.clone(), config.provider_defaults.clone())`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::config::Config;
    /// use xzardgz::providers::anthropic::AnthropicProvider;
    ///
    /// let provider = AnthropicProvider::from_config(&Config::default());
    /// assert_eq!(provider.provider_name_str(), "anthropic");
    /// ```
    pub fn from_config(config: &Config) -> Self {
        Self::new(config.anthropic.clone(), config.provider_defaults.clone())
    }

    /// Returns the API key by reading the environment variable named in
    /// `self.config.api_key_env`.
    ///
    /// Returns `None` when the variable is unset or contains an empty string.
    pub fn api_key(&self) -> Option<String> {
        std::env::var(&self.config.api_key_env)
            .ok()
            .filter(|k| !k.is_empty())
    }

    /// Returns the short provider name string (always `"anthropic"`).
    ///
    /// Provided as a non-trait helper for use in constructors and tests.
    pub fn provider_name_str(&self) -> &'static str {
        "anthropic"
    }

    /// Returns the static capability table for well-known Anthropic models.
    ///
    /// This list is always returned by [`list_models`][Provider::list_models]
    /// because the Anthropic API does not expose a model listing endpoint.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::providers::anthropic::AnthropicProvider;
    ///
    /// let models = AnthropicProvider::static_anthropic_models();
    /// assert!(models.iter().any(|m| m.id == "claude-3-5-sonnet-latest"));
    /// ```
    pub fn static_anthropic_models() -> Vec<ModelMetadata> {
        vec![
            ModelMetadata::new(
                "claude-opus-4-5",
                ModelCapabilities {
                    supports_tools: true,
                    supports_structured_output: true,
                    supports_thinking: true,
                    supports_streaming: true,
                    supports_vision: true,
                    context_window_tokens: 200_000,
                },
            ),
            ModelMetadata::new(
                "claude-3-5-sonnet-latest",
                ModelCapabilities {
                    supports_tools: true,
                    supports_structured_output: true,
                    supports_thinking: true,
                    supports_streaming: true,
                    supports_vision: true,
                    context_window_tokens: 200_000,
                },
            ),
            ModelMetadata::new(
                "claude-3-5-haiku-latest",
                ModelCapabilities {
                    supports_tools: true,
                    supports_structured_output: true,
                    supports_thinking: false,
                    supports_streaming: true,
                    supports_vision: true,
                    context_window_tokens: 200_000,
                },
            ),
            ModelMetadata::new(
                "claude-3-opus-latest",
                ModelCapabilities {
                    supports_tools: true,
                    supports_structured_output: true,
                    supports_thinking: false,
                    supports_streaming: true,
                    supports_vision: true,
                    context_window_tokens: 200_000,
                },
            ),
        ]
    }

    /// Returns `true` if the configured model supports extended thinking.
    fn model_supports_thinking(&self) -> bool {
        Self::static_anthropic_models()
            .iter()
            .find(|m| m.id == self.config.model)
            .map(|m| m.capabilities.supports_thinking)
            .unwrap_or(false)
    }

    /// Core completion implementation shared by `complete` and
    /// `complete_with_thinking`.
    async fn do_complete(
        &self,
        messages: &[Message],
        tools: &[Tool],
        thinking: Option<AnthropicThinking>,
    ) -> Result<Message> {
        let api_key = self.api_key().ok_or_else(|| {
            PipelineError::Auth(format!(
                "Anthropic API key not found in environment variable '{}'",
                self.config.api_key_env
            ))
        })?;

        // Extract the system message (Anthropic uses a top-level "system" field).
        let system: Option<String> = messages
            .iter()
            .find(|m| m.role == Role::System)
            .map(|m| m.content.clone());

        // Non-system messages are forwarded in the messages array.
        let anthro_messages: Vec<AnthropicMessage> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(to_anthropic_message)
            .collect();

        let anthro_tools: Option<Vec<AnthropicTool>> = if tools.is_empty() {
            None
        } else {
            Some(tools.iter().map(to_anthropic_tool).collect())
        };

        let request = AnthropicRequest {
            model: self.config.model.clone(),
            max_tokens: self.defaults.max_tokens,
            system,
            messages: anthro_messages,
            tools: anthro_tools,
            thinking,
            stream: None,
        };

        let response = self
            .client
            .post(ANTHROPIC_MESSAGES_URL)
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION_HEADER)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| PipelineError::Provider(format!("Anthropic request failed: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            // SAFETY: already in error path; empty body is acceptable.
            let body = response.text().await.unwrap_or_default();
            return Err(PipelineError::Auth(format!(
                "Anthropic authentication error ({status}): {body}"
            )));
        }
        if !status.is_success() {
            // SAFETY: already in error path; empty body is acceptable.
            let body = response.text().await.unwrap_or_default();
            return Err(PipelineError::Provider(format!(
                "Anthropic API error ({status}): {body}"
            )));
        }

        let anthro_response: AnthropicResponse = response.json().await.map_err(|e| {
            PipelineError::Provider(format!("failed to parse Anthropic response: {e}"))
        })?;

        Ok(from_anthropic_response(&anthro_response))
    }
}

// ---------------------------------------------------------------------------
// Wire-format types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<AnthropicThinking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Serialize, Clone)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Serialize, Clone)]
struct AnthropicThinking {
    #[serde(rename = "type")]
    thinking_type: String,
    budget_tokens: u32,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    input: Option<serde_json::Value>,
}

/// SSE data payload for a `content_block_delta` event.
#[derive(Deserialize)]
struct AnthropicStreamData {
    delta: AnthropicStreamDelta,
}

#[derive(Deserialize)]
struct AnthropicStreamDelta {
    #[serde(rename = "type")]
    delta_type: String,
    #[serde(default)]
    text: String,
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

/// Converts a domain [`Message`] to the Anthropic wire format.
fn to_anthropic_message(msg: &Message) -> AnthropicMessage {
    let role = match msg.role {
        Role::User | Role::Tool => "user",
        Role::Assistant => "assistant",
        Role::System => "user", // should be filtered out before calling this
    };
    AnthropicMessage {
        role: role.to_string(),
        content: msg.content.clone(),
    }
}

/// Converts a domain [`Tool`] to the Anthropic wire format.
fn to_anthropic_tool(tool: &Tool) -> AnthropicTool {
    AnthropicTool {
        name: tool.name.clone(),
        description: tool.description.clone(),
        input_schema: tool.parameters.clone(),
    }
}

/// Builds a domain [`Message`] from an Anthropic response body.
fn from_anthropic_response(response: &AnthropicResponse) -> Message {
    // Collect text blocks and tool_use blocks separately.
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    for block in &response.content {
        match block.block_type.as_str() {
            "text" => {
                if !block.text.is_empty() {
                    text_parts.push(block.text.clone());
                }
            }
            "tool_use" => {
                let arguments = block
                    .input
                    .as_ref()
                    .and_then(|v| serde_json::to_string(v).ok())
                    // SAFETY: serde_json::to_string of a Value is infallible
                    // for well-formed JSON; fall back to "{}" on the off chance.
                    .unwrap_or_else(|| "{}".to_string());

                tool_calls.push(ToolCall {
                    id: block.id.clone(),
                    function: FunctionCall {
                        name: block.name.clone(),
                        arguments,
                    },
                });
            }
            // Skip "thinking" and other block types.
            _ => {}
        }
    }

    let content = text_parts.join("");

    if !tool_calls.is_empty() {
        Message {
            role: Role::Assistant,
            content,
            name: None,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    } else {
        Message::assistant(content)
    }
}

// ---------------------------------------------------------------------------
// Provider trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Provider for AnthropicProvider {
    /// Returns the provider name `"anthropic"`.
    fn provider_name(&self) -> &str {
        "anthropic"
    }

    /// Returns aggregate metadata for the Anthropic provider.
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            name: "anthropic".to_string(),
            models: Self::static_anthropic_models()
                .into_iter()
                .map(|m| m.id)
                .collect(),
            capabilities: ProviderCapabilities {
                streaming: true,
                tools: true,
                vision: true,
            },
        }
    }

    /// Returns `true` because claude-opus-4-5 and claude-3-5-sonnet support thinking.
    fn supports_thinking(&self) -> bool {
        true
    }

    /// Checks whether `config.api_key_env` is set in the environment.
    async fn credential_status(&self) -> CredentialStatus {
        match self.api_key() {
            Some(_) => CredentialStatus::Present,
            None => CredentialStatus::Missing,
        }
    }

    /// Returns the static model table (Anthropic has no model listing endpoint).
    ///
    /// # Errors
    ///
    /// Never returns `Err`.
    async fn list_models(&self) -> Result<Vec<ModelMetadata>> {
        Ok(Self::static_anthropic_models())
    }

    /// Sends a non-streaming chat completion to the Anthropic Messages API.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Auth`] on 401/403.
    /// Returns [`PipelineError::Provider`] on other API or network failures.
    async fn complete(&self, messages: &[Message], tools: &[Tool]) -> Result<Message> {
        self.do_complete(messages, tools, None).await
    }

    /// Sends a chat completion with extended thinking when the model supports it.
    ///
    /// For models with `supports_thinking = true`, adds a `"thinking"` block
    /// with the token budget from `thinking_mode.budget_tokens()`.  If the
    /// mode is [`ThinkingMode::None`] or [`ThinkingMode::Auto`], or the model
    /// does not support thinking, falls through to [`complete`].
    ///
    /// # Errors
    ///
    /// Same as [`complete`].
    async fn complete_with_thinking(
        &self,
        messages: &[Message],
        tools: &[Tool],
        thinking_mode: ThinkingMode,
    ) -> Result<Message> {
        if !self.model_supports_thinking() {
            return self.do_complete(messages, tools, None).await;
        }

        let budget = match thinking_mode.budget_tokens() {
            Some(b) => b,
            None => return self.do_complete(messages, tools, None).await,
        };

        let thinking = Some(AnthropicThinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: budget,
        });

        self.do_complete(messages, tools, thinking).await
    }

    /// Streams a chat completion from the Anthropic Messages API via SSE.
    ///
    /// Sends `POST /v1/messages` with `"stream": true` and yields each
    /// `content_block_delta` text delta as a `Message::assistant` item.
    ///
    /// # Errors
    ///
    /// Returns `Err` immediately if the initial request fails or the server
    /// returns a non-2xx status.  Mid-stream errors are yielded as `Err` items.
    async fn complete_streaming(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Message>> + Send>>> {
        let api_key = self.api_key().ok_or_else(|| {
            PipelineError::Auth(format!(
                "Anthropic API key not found in environment variable '{}'",
                self.config.api_key_env
            ))
        })?;

        let system: Option<String> = messages
            .iter()
            .find(|m| m.role == Role::System)
            .map(|m| m.content.clone());

        let anthro_messages: Vec<AnthropicMessage> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(to_anthropic_message)
            .collect();

        let anthro_tools: Option<Vec<AnthropicTool>> = if tools.is_empty() {
            None
        } else {
            Some(tools.iter().map(to_anthropic_tool).collect())
        };

        let request = AnthropicRequest {
            model: self.config.model.clone(),
            max_tokens: self.defaults.max_tokens,
            system,
            messages: anthro_messages,
            tools: anthro_tools,
            thinking: None,
            stream: Some(true),
        };

        let response = self
            .client
            .post(ANTHROPIC_MESSAGES_URL)
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION_HEADER)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                PipelineError::Provider(format!("Anthropic streaming request failed: {e}"))
            })?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            // SAFETY: already in error path; empty body is acceptable.
            let body = response.text().await.unwrap_or_default();
            return Err(PipelineError::Auth(format!(
                "Anthropic authentication error ({status}): {body}"
            )));
        }
        if !status.is_success() {
            // SAFETY: already in error path; empty body is acceptable.
            let body = response.text().await.unwrap_or_default();
            return Err(PipelineError::Provider(format!(
                "Anthropic API error ({status}): {body}"
            )));
        }

        let byte_stream = response.bytes_stream();
        let s = stream! {
            let mut byte_stream = byte_stream;
            // Buffer text across byte chunks to handle SSE events that span
            // multiple network packets.
            let mut buffer = String::new();

            while let Some(chunk) = byte_stream.next().await {
                match chunk {
                    Err(e) => {
                        yield Err(PipelineError::Provider(format!("stream read error: {e}")));
                        return;
                    }
                    Ok(bytes) => match String::from_utf8(bytes.to_vec()) {
                        Err(e) => {
                            yield Err(PipelineError::Provider(
                                format!("UTF-8 decode error: {e}"),
                            ));
                            return;
                        }
                        Ok(text) => buffer.push_str(&text),
                    },
                }

                // Process complete SSE events separated by double newlines.
                while let Some(boundary) = buffer.find("\n\n") {
                    let event_block = buffer[..boundary].to_string();
                    buffer = buffer[boundary + 2..].to_string();

                    let mut event_type: Option<&str> = None;
                    let mut data_line: Option<&str> = None;

                    for line in event_block.lines() {
                        if let Some(e) = line.strip_prefix("event: ") {
                            event_type = Some(e);
                        } else if let Some(d) = line.strip_prefix("data: ") {
                            data_line = Some(d);
                        }
                    }

                    if event_type == Some("content_block_delta")
                        && let Some(data_str) = data_line
                    {
                        match serde_json::from_str::<AnthropicStreamData>(data_str) {
                            Ok(parsed) => {
                                if parsed.delta.delta_type == "text_delta"
                                    && !parsed.delta.text.is_empty()
                                {
                                    yield Ok(Message::assistant(parsed.delta.text));
                                }
                            }
                            Err(e) => {
                                warn!("failed to parse Anthropic SSE delta: {e}");
                            }
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
    use crate::config::{AnthropicConfig, Config, ProviderDefaultsConfig};

    // ------------------------------------------------------------------
    // Construction
    // ------------------------------------------------------------------

    #[test]
    fn test_anthropic_provider_from_config_builds_successfully() {
        let config = Config::default();
        let provider = AnthropicProvider::from_config(&config);
        assert_eq!(provider.provider_name(), "anthropic");
    }

    #[test]
    fn test_anthropic_provider_new_builds_successfully() {
        let config = AnthropicConfig::default();
        let defaults = ProviderDefaultsConfig::default();
        let provider = AnthropicProvider::new(config, defaults);
        assert_eq!(provider.provider_name_str(), "anthropic");
    }

    // ------------------------------------------------------------------
    // Provider trait accessors
    // ------------------------------------------------------------------

    #[test]
    fn test_anthropic_provider_name_returns_anthropic() {
        let provider = AnthropicProvider::from_config(&Config::default());
        assert_eq!(provider.provider_name(), "anthropic");
    }

    #[test]
    fn test_anthropic_provider_supports_thinking_returns_true() {
        let provider = AnthropicProvider::from_config(&Config::default());
        assert!(provider.supports_thinking());
    }

    #[test]
    fn test_anthropic_provider_metadata_name_is_anthropic() {
        let provider = AnthropicProvider::from_config(&Config::default());
        assert_eq!(provider.metadata().name, "anthropic");
    }

    #[test]
    fn test_anthropic_provider_metadata_capabilities_streaming_is_true() {
        let provider = AnthropicProvider::from_config(&Config::default());
        assert!(provider.metadata().capabilities.streaming);
    }

    // ------------------------------------------------------------------
    // Static model table
    // ------------------------------------------------------------------

    #[test]
    fn test_anthropic_static_models_contains_expected_models() {
        let models = AnthropicProvider::static_anthropic_models();
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"claude-opus-4-5"));
        assert!(ids.contains(&"claude-3-5-sonnet-latest"));
        assert!(ids.contains(&"claude-3-5-haiku-latest"));
        assert!(ids.contains(&"claude-3-opus-latest"));
    }

    #[test]
    fn test_anthropic_static_models_claude_sonnet_has_thinking_support() {
        let models = AnthropicProvider::static_anthropic_models();
        let sonnet = models
            .iter()
            .find(|m| m.id == "claude-3-5-sonnet-latest")
            .expect("claude-3-5-sonnet-latest");
        assert!(sonnet.capabilities.supports_thinking);
        assert!(sonnet.capabilities.supports_tools);
        assert!(sonnet.capabilities.supports_streaming);
    }

    #[test]
    fn test_anthropic_static_models_claude_haiku_has_no_thinking_support() {
        let models = AnthropicProvider::static_anthropic_models();
        let haiku = models
            .iter()
            .find(|m| m.id == "claude-3-5-haiku-latest")
            .expect("claude-3-5-haiku-latest");
        assert!(!haiku.capabilities.supports_thinking);
    }

    #[test]
    fn test_anthropic_static_models_claude_opus_has_no_thinking_support() {
        let models = AnthropicProvider::static_anthropic_models();
        let opus = models
            .iter()
            .find(|m| m.id == "claude-3-opus-latest")
            .expect("claude-3-opus-latest");
        assert!(!opus.capabilities.supports_thinking);
    }

    #[test]
    fn test_anthropic_static_models_all_have_large_context_window() {
        let models = AnthropicProvider::static_anthropic_models();
        for model in &models {
            assert_eq!(
                model.capabilities.context_window_tokens, 200_000,
                "{} should have 200k context",
                model.id
            );
        }
    }

    // ------------------------------------------------------------------
    // Credential status
    // ------------------------------------------------------------------

    #[test]
    fn test_anthropic_credential_status_present_when_env_set() {
        temp_env::with_var("ANTHROPIC_API_KEY", Some("sk-ant-test"), || {
            let config = AnthropicConfig::default();
            let defaults = ProviderDefaultsConfig::default();
            let provider = AnthropicProvider::new(config, defaults);
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            let status = rt.block_on(provider.credential_status());
            assert_eq!(status, CredentialStatus::Present);
        });
    }

    #[test]
    fn test_anthropic_credential_status_missing_when_env_absent() {
        temp_env::with_var("ANTHROPIC_API_KEY", None::<&str>, || {
            let config = AnthropicConfig::default();
            let defaults = ProviderDefaultsConfig::default();
            let provider = AnthropicProvider::new(config, defaults);
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            let status = rt.block_on(provider.credential_status());
            assert_eq!(status, CredentialStatus::Missing);
        });
    }

    // ------------------------------------------------------------------
    // list_models
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_anthropic_list_models_returns_static_list() {
        let provider = AnthropicProvider::from_config(&Config::default());
        // SAFETY: list_models for Anthropic always returns Ok.
        let models = provider.list_models().await.unwrap();
        assert!(!models.is_empty());
    }
}
