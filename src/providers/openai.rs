//! OpenAI chat completions provider.
//!
//! Implements [`Provider`] for OpenAI-compatible chat completion endpoints,
//! including the official `api.openai.com` and compatible self-hosted proxies.
//!
//! # Features
//!
//! - Full [`complete`][Provider::complete] via `POST /chat/completions`
//! - Streaming via SSE with `"stream": true`
//! - Thinking-mode support for `o1` and `o3-mini` via `reasoning_effort`
//! - Static model table for offline capability resolution
//! - Silent fallback to static table when the `/models` endpoint is unreachable

use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

use async_stream::stream;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::base::Provider;
use super::types::{
    CredentialStatus, FunctionCall, Message, ModelCapabilities, ModelMetadata,
    ProviderCapabilities, ProviderMetadata, Role, ThinkingMode, Tool, ToolCall,
};
use crate::config::{Config, OpenAiConfig, ProviderDefaultsConfig};
use crate::error::{PipelineError, Result};

// ---------------------------------------------------------------------------
// Provider struct
// ---------------------------------------------------------------------------

/// OpenAI chat completions provider.
///
/// Supports any endpoint that speaks the OpenAI chat completions protocol,
/// including `api.openai.com` and compatible proxies.
///
/// Construct via [`OpenAiProvider::new`] (explicit config) or
/// [`OpenAiProvider::from_config`] (top-level app config).
#[derive(Clone)]
pub struct OpenAiProvider {
    config: OpenAiConfig,
    defaults: ProviderDefaultsConfig,
    client: Client,
}

impl OpenAiProvider {
    /// Creates a new `OpenAiProvider` from explicit configuration values.
    ///
    /// Validates that the endpoint uses HTTPS unless
    /// `config.allow_insecure_endpoint` is `true`, then builds a
    /// [`reqwest::Client`] with the timeout from `defaults.timeout_seconds`.
    ///
    /// # Arguments
    ///
    /// * `config` - OpenAI-specific settings (endpoint, model, key env-var).
    /// * `defaults` - Shared provider defaults (timeout, max-tokens, etc.).
    ///
    /// # Returns
    ///
    /// `Ok(Self)` on success.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Config`] when the endpoint is `http://` and
    /// `allow_insecure_endpoint` is `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::config::{OpenAiConfig, ProviderDefaultsConfig};
    /// use xzardgz::providers::openai::OpenAiProvider;
    ///
    /// let provider = OpenAiProvider::new(
    ///     OpenAiConfig::default(),
    ///     ProviderDefaultsConfig::default(),
    /// ).expect("valid config");
    /// assert_eq!(provider.provider_name_str(), "openai");
    /// ```
    pub fn new(config: OpenAiConfig, defaults: ProviderDefaultsConfig) -> Result<Self> {
        if !config.allow_insecure_endpoint && config.endpoint.starts_with("http://") {
            return Err(PipelineError::Config(format!(
                "insecure HTTP endpoint '{}' is not allowed; \
                 set allow_insecure_endpoint = true to override",
                config.endpoint
            )));
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(defaults.timeout_seconds))
            .build()
            // SAFETY: we pass no custom TLS config; build() only fails when
            // native-TLS initialisation itself fails, which is an environment
            // problem, not a logic error.
            .map_err(|e| PipelineError::Provider(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            config,
            defaults,
            client,
        })
    }

    /// Creates an `OpenAiProvider` from the top-level application [`Config`].
    ///
    /// Equivalent to `OpenAiProvider::new(config.openai.clone(), config.provider_defaults.clone())`.
    ///
    /// # Errors
    ///
    /// Propagates errors from [`OpenAiProvider::new`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::config::Config;
    /// use xzardgz::providers::openai::OpenAiProvider;
    ///
    /// let provider = OpenAiProvider::from_config(&Config::default())
    ///     .expect("default config is valid");
    /// ```
    pub fn from_config(config: &Config) -> Result<Self> {
        Self::new(config.openai.clone(), config.provider_defaults.clone())
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

    /// Returns the short provider name string (always `"openai"`).
    ///
    /// Provided as a non-trait helper for use in constructors and tests.
    pub fn provider_name_str(&self) -> &'static str {
        "openai"
    }

    /// Returns the static capability table for well-known OpenAI models.
    ///
    /// This list is used when the live `/models` endpoint is unavailable or
    /// the API key is absent.  It is also used to enrich live model listings
    /// with capability data.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::providers::openai::OpenAiProvider;
    ///
    /// let models = OpenAiProvider::static_openai_models();
    /// assert!(!models.is_empty());
    /// assert!(models.iter().any(|m| m.id == "gpt-4o"));
    /// ```
    pub fn static_openai_models() -> Vec<ModelMetadata> {
        vec![
            ModelMetadata::new(
                "gpt-4.1",
                ModelCapabilities {
                    supports_tools: true,
                    supports_structured_output: true,
                    supports_thinking: false,
                    supports_streaming: true,
                    supports_vision: true,
                    context_window_tokens: 128_000,
                },
            ),
            ModelMetadata::new(
                "gpt-4.1-mini",
                ModelCapabilities {
                    supports_tools: true,
                    supports_structured_output: true,
                    supports_thinking: false,
                    supports_streaming: true,
                    supports_vision: true,
                    context_window_tokens: 128_000,
                },
            ),
            ModelMetadata::new(
                "gpt-4o",
                ModelCapabilities {
                    supports_tools: true,
                    supports_structured_output: true,
                    supports_thinking: false,
                    supports_streaming: true,
                    supports_vision: true,
                    context_window_tokens: 128_000,
                },
            ),
            ModelMetadata::new(
                "gpt-4o-mini",
                ModelCapabilities {
                    supports_tools: true,
                    supports_structured_output: true,
                    supports_thinking: false,
                    supports_streaming: true,
                    supports_vision: false,
                    context_window_tokens: 128_000,
                },
            ),
            ModelMetadata::new(
                "o1",
                ModelCapabilities {
                    supports_tools: true,
                    supports_structured_output: true,
                    supports_thinking: true,
                    supports_streaming: false,
                    supports_vision: false,
                    context_window_tokens: 200_000,
                },
            ),
            ModelMetadata::new(
                "o3-mini",
                ModelCapabilities {
                    supports_tools: true,
                    supports_structured_output: true,
                    supports_thinking: true,
                    supports_streaming: false,
                    supports_vision: false,
                    context_window_tokens: 200_000,
                },
            ),
        ]
    }

    /// Returns the static model table indexed by model ID for O(1) lookup.
    fn static_models_map() -> HashMap<String, ModelMetadata> {
        Self::static_openai_models()
            .into_iter()
            .map(|m| (m.id.clone(), m))
            .collect()
    }

    /// Returns `true` if the configured model supports thinking (reasoning effort).
    fn model_supports_thinking(&self) -> bool {
        Self::static_models_map()
            .get(&self.config.model)
            .map(|m| m.capabilities.supports_thinking)
            .unwrap_or(false)
    }

    /// Core non-streaming completion implementation shared by `complete` and
    /// `complete_with_thinking`.
    async fn do_complete(
        &self,
        messages: &[Message],
        tools: &[Tool],
        reasoning_effort: Option<String>,
    ) -> Result<Message> {
        let api_key = self.api_key().ok_or_else(|| {
            PipelineError::Auth(format!(
                "OpenAI API key not found in environment variable '{}'",
                self.config.api_key_env
            ))
        })?;

        let url = format!("{}/chat/completions", self.config.endpoint);
        let oai_messages: Vec<OaiMessage> = messages.iter().map(to_oai_message).collect();

        let (oai_tools, tool_choice) = if tools.is_empty() {
            (None, None)
        } else {
            (
                Some(tools.iter().map(to_oai_tool).collect::<Vec<_>>()),
                Some("auto".to_string()),
            )
        };

        let request = OaiRequest {
            model: self.config.model.clone(),
            messages: oai_messages,
            max_tokens: self.defaults.max_tokens,
            tools: oai_tools,
            tool_choice,
            stream: None,
            reasoning_effort,
        };

        let response = self
            .client
            .post(&url)
            .bearer_auth(&api_key)
            .json(&request)
            .send()
            .await
            .map_err(|e| PipelineError::Provider(format!("OpenAI request failed: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            // SAFETY: already in error path; empty body is acceptable.
            let body = response.text().await.unwrap_or_default();
            return Err(PipelineError::Auth(format!(
                "OpenAI authentication error ({status}): {body}"
            )));
        }
        if !status.is_success() {
            // SAFETY: already in error path; empty body is acceptable.
            let body = response.text().await.unwrap_or_default();
            return Err(PipelineError::Provider(format!(
                "OpenAI API error ({status}): {body}"
            )));
        }

        let oai_response: OaiResponse = response.json().await.map_err(|e| {
            PipelineError::Provider(format!("failed to parse OpenAI response: {e}"))
        })?;

        let choice = oai_response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| PipelineError::Provider("OpenAI returned no choices".to_string()))?;

        Ok(from_oai_message(&choice.message))
    }
}

// ---------------------------------------------------------------------------
// Wire-format types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct OaiRequest {
    model: String,
    messages: Vec<OaiMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OaiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct OaiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OaiToolCall>>,
}

#[derive(Serialize, Deserialize, Clone)]
struct OaiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OaiFunctionCall,
}

#[derive(Serialize, Deserialize, Clone)]
struct OaiFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct OaiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OaiToolFunction,
}

#[derive(Serialize)]
struct OaiToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Deserialize)]
struct OaiResponse {
    choices: Vec<OaiChoice>,
}

#[derive(Deserialize)]
struct OaiChoice {
    message: OaiMessage,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OaiStreamResponse {
    choices: Vec<OaiStreamChoice>,
}

#[derive(Deserialize)]
struct OaiStreamChoice {
    delta: OaiStreamDelta,
}

#[derive(Deserialize)]
struct OaiStreamDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct OaiModelList {
    data: Vec<OaiModelEntry>,
}

#[derive(Deserialize)]
struct OaiModelEntry {
    id: String,
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

/// Converts a domain [`Message`] to the OpenAI wire format.
fn to_oai_message(msg: &Message) -> OaiMessage {
    let role = match msg.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    };
    OaiMessage {
        role: role.to_string(),
        content: Some(msg.content.clone()),
        tool_calls: msg.tool_calls.as_ref().map(|tc| {
            tc.iter()
                .map(|t| OaiToolCall {
                    id: t.id.clone(),
                    call_type: "function".to_string(),
                    function: OaiFunctionCall {
                        name: t.function.name.clone(),
                        arguments: t.function.arguments.clone(),
                    },
                })
                .collect()
        }),
    }
}

/// Converts a domain [`Tool`] to the OpenAI wire format.
fn to_oai_tool(tool: &Tool) -> OaiTool {
    OaiTool {
        tool_type: "function".to_string(),
        function: OaiToolFunction {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: tool.parameters.clone(),
        },
    }
}

/// Converts an OpenAI response message to a domain [`Message`].
fn from_oai_message(oai: &OaiMessage) -> Message {
    // If the model requested tool calls, build a tool-call message.
    if let Some(tc) = &oai.tool_calls
        && !tc.is_empty()
    {
        return Message {
            role: Role::Assistant,
            content: String::new(),
            name: None,
            tool_calls: Some(
                tc.iter()
                    .map(|t| ToolCall {
                        id: t.id.clone(),
                        function: FunctionCall {
                            name: t.function.name.clone(),
                            arguments: t.function.arguments.clone(),
                        },
                    })
                    .collect(),
            ),
            tool_call_id: None,
        };
    }
    let content = oai.content.as_deref().unwrap_or("").to_string();
    Message::assistant(content)
}

// ---------------------------------------------------------------------------
// Provider trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Provider for OpenAiProvider {
    /// Returns the provider name `"openai"`.
    fn provider_name(&self) -> &str {
        "openai"
    }

    /// Returns aggregate metadata for the OpenAI provider.
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            name: "openai".to_string(),
            models: Self::static_openai_models()
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

    /// Returns `true` because `o1` and `o3-mini` support reasoning effort.
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

    /// Lists available models.
    ///
    /// Attempts `GET {endpoint}/models` with the configured API key.  On any
    /// failure (missing key, network error, non-2xx) silently returns the
    /// static model table.
    ///
    /// # Errors
    ///
    /// Never returns `Err` — failures fall back to the static list.
    async fn list_models(&self) -> Result<Vec<ModelMetadata>> {
        let api_key = match self.api_key() {
            Some(k) => k,
            None => {
                debug!("no OpenAI API key present; returning static model list");
                return Ok(Self::static_openai_models());
            }
        };

        let url = format!("{}/models", self.config.endpoint);
        let response = match self.client.get(&url).bearer_auth(&api_key).send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("failed to fetch OpenAI models ({e}); using static list");
                return Ok(Self::static_openai_models());
            }
        };

        if !response.status().is_success() {
            warn!(
                "OpenAI /models returned {}; using static list",
                response.status()
            );
            return Ok(Self::static_openai_models());
        }

        let model_list: OaiModelList = match response.json().await {
            Ok(l) => l,
            Err(e) => {
                warn!("failed to parse OpenAI /models response ({e}); using static list");
                return Ok(Self::static_openai_models());
            }
        };

        let static_map = Self::static_models_map();
        let models = model_list
            .data
            .into_iter()
            .map(|entry| {
                static_map
                    .get(&entry.id)
                    .cloned()
                    .unwrap_or_else(|| ModelMetadata::new(entry.id, ModelCapabilities::default()))
            })
            .collect();

        Ok(models)
    }

    /// Sends a non-streaming chat completion.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Auth`] on 401/403.
    /// Returns [`PipelineError::Provider`] on other API or network failures.
    async fn complete(&self, messages: &[Message], tools: &[Tool]) -> Result<Message> {
        self.do_complete(messages, tools, None).await
    }

    /// Sends a chat completion with an optional reasoning-effort hint.
    ///
    /// For models that support thinking (`o1`, `o3-mini`), maps
    /// [`ThinkingMode`] to OpenAI's `reasoning_effort` field:
    ///
    /// | ThinkingMode      | reasoning_effort |
    /// |-------------------|-----------------|
    /// | `None` / `Auto`   | *(omitted)*     |
    /// | `Low`             | `"low"`         |
    /// | `Medium`          | `"medium"`      |
    /// | `High`/`ExtraHigh`| `"high"`        |
    ///
    /// For models that do not support thinking, delegates to [`complete`].
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

        let effort = match thinking_mode {
            ThinkingMode::None | ThinkingMode::Auto => {
                return self.do_complete(messages, tools, None).await;
            }
            ThinkingMode::Low => "low",
            ThinkingMode::Medium => "medium",
            ThinkingMode::High | ThinkingMode::ExtraHigh => "high",
        };

        self.do_complete(messages, tools, Some(effort.to_string()))
            .await
    }

    /// Streams a chat completion via SSE.
    ///
    /// Sends `POST {endpoint}/chat/completions` with `"stream": true` and
    /// yields each non-empty text delta as a `Message::assistant` item.
    ///
    /// # Errors
    ///
    /// Returns `Err` immediately if the initial request fails or the server
    /// returns a non-2xx status.  Mid-stream network errors are yielded as
    /// `Err` items in the stream.
    async fn complete_streaming(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Message>> + Send>>> {
        let api_key = self.api_key().ok_or_else(|| {
            PipelineError::Auth(format!(
                "OpenAI API key not found in environment variable '{}'",
                self.config.api_key_env
            ))
        })?;

        let url = format!("{}/chat/completions", self.config.endpoint);
        let oai_messages: Vec<OaiMessage> = messages.iter().map(to_oai_message).collect();

        let (oai_tools, tool_choice) = if tools.is_empty() {
            (None, None)
        } else {
            (
                Some(tools.iter().map(to_oai_tool).collect::<Vec<_>>()),
                Some("auto".to_string()),
            )
        };

        let request = OaiRequest {
            model: self.config.model.clone(),
            messages: oai_messages,
            max_tokens: self.defaults.max_tokens,
            tools: oai_tools,
            tool_choice,
            stream: Some(true),
            reasoning_effort: None,
        };

        let response = self
            .client
            .post(&url)
            .bearer_auth(&api_key)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                PipelineError::Provider(format!("OpenAI streaming request failed: {e}"))
            })?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            // SAFETY: already in error path; empty body is acceptable.
            let body = response.text().await.unwrap_or_default();
            return Err(PipelineError::Auth(format!(
                "OpenAI authentication error ({status}): {body}"
            )));
        }
        if !status.is_success() {
            // SAFETY: already in error path; empty body is acceptable.
            let body = response.text().await.unwrap_or_default();
            return Err(PipelineError::Provider(format!(
                "OpenAI API error ({status}): {body}"
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

                // Process complete SSE lines terminated by '\n'.
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() || line == "data: [DONE]" {
                        continue;
                    }

                    if let Some(json_str) = line.strip_prefix("data: ")
                        && let Ok(parsed) =
                            serde_json::from_str::<OaiStreamResponse>(json_str)
                    {
                        for choice in parsed.choices {
                            if let Some(content) = choice.delta.content
                                && !content.is_empty()
                            {
                                yield Ok(Message::assistant(content));
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
    use crate::config::{Config, OpenAiConfig, ProviderDefaultsConfig};

    // ------------------------------------------------------------------
    // Construction
    // ------------------------------------------------------------------

    #[test]
    fn test_openai_provider_from_config_succeeds_with_default_config() {
        let config = Config::default();
        let result = OpenAiProvider::from_config(&config);
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
    }

    #[test]
    fn test_openai_provider_rejects_http_endpoint_when_insecure_not_allowed() {
        let config = OpenAiConfig {
            endpoint: "http://localhost:8080/v1".to_string(),
            allow_insecure_endpoint: false,
            ..Default::default()
        };
        let result = OpenAiProvider::new(config, ProviderDefaultsConfig::default());
        assert!(
            matches!(result, Err(PipelineError::Config(_))),
            "expected Config error for http endpoint"
        );
    }

    #[test]
    fn test_openai_provider_allows_http_endpoint_when_insecure_allowed() {
        let config = OpenAiConfig {
            endpoint: "http://localhost:8080/v1".to_string(),
            allow_insecure_endpoint: true,
            ..Default::default()
        };
        let result = OpenAiProvider::new(config, ProviderDefaultsConfig::default());
        assert!(
            result.is_ok(),
            "expected Ok for insecure endpoint when allowed"
        );
    }

    // ------------------------------------------------------------------
    // Credential status
    // ------------------------------------------------------------------

    #[test]
    fn test_openai_provider_credential_status_present_when_env_var_set() {
        temp_env::with_var("OPENAI_API_KEY", Some("sk-test-key"), || {
            let config = OpenAiConfig::default();
            let defaults = ProviderDefaultsConfig::default();
            // SAFETY: default config is always valid.
            let provider = OpenAiProvider::new(config, defaults).unwrap();
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            let status = rt.block_on(provider.credential_status());
            assert_eq!(status, CredentialStatus::Present);
        });
    }

    #[test]
    fn test_openai_provider_credential_status_missing_when_env_var_absent() {
        temp_env::with_var("OPENAI_API_KEY", None::<&str>, || {
            let config = OpenAiConfig::default();
            let defaults = ProviderDefaultsConfig::default();
            // SAFETY: default config is always valid.
            let provider = OpenAiProvider::new(config, defaults).unwrap();
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            let status = rt.block_on(provider.credential_status());
            assert_eq!(status, CredentialStatus::Missing);
        });
    }

    // ------------------------------------------------------------------
    // Static model table
    // ------------------------------------------------------------------

    #[test]
    fn test_openai_static_models_contains_expected_models() {
        let models = OpenAiProvider::static_openai_models();
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"gpt-4.1"));
        assert!(ids.contains(&"gpt-4.1-mini"));
        assert!(ids.contains(&"gpt-4o"));
        assert!(ids.contains(&"gpt-4o-mini"));
        assert!(ids.contains(&"o1"));
        assert!(ids.contains(&"o3-mini"));
    }

    #[test]
    fn test_openai_static_models_gpt4o_has_tool_support() {
        let models = OpenAiProvider::static_openai_models();
        let gpt4o = models.iter().find(|m| m.id == "gpt-4o").expect("gpt-4o");
        assert!(gpt4o.capabilities.supports_tools);
        assert!(gpt4o.capabilities.supports_streaming);
        assert!(gpt4o.capabilities.supports_vision);
    }

    #[test]
    fn test_openai_static_models_o1_has_thinking_support() {
        let models = OpenAiProvider::static_openai_models();
        let o1 = models.iter().find(|m| m.id == "o1").expect("o1");
        assert!(o1.capabilities.supports_thinking);
    }

    #[test]
    fn test_openai_static_models_gpt4o_mini_has_no_vision() {
        let models = OpenAiProvider::static_openai_models();
        let mini = models
            .iter()
            .find(|m| m.id == "gpt-4o-mini")
            .expect("gpt-4o-mini");
        assert!(!mini.capabilities.supports_vision);
    }

    #[test]
    fn test_openai_static_models_o3_mini_has_thinking_support() {
        let models = OpenAiProvider::static_openai_models();
        let o3 = models.iter().find(|m| m.id == "o3-mini").expect("o3-mini");
        assert!(o3.capabilities.supports_thinking);
        assert!(!o3.capabilities.supports_streaming);
    }

    // ------------------------------------------------------------------
    // Provider trait accessors
    // ------------------------------------------------------------------

    #[test]
    fn test_openai_provider_name_returns_openai() {
        // SAFETY: default config is always valid.
        let provider = OpenAiProvider::from_config(&Config::default()).unwrap();
        assert_eq!(provider.provider_name(), "openai");
    }

    #[test]
    fn test_openai_provider_supports_thinking_returns_true() {
        // SAFETY: default config is always valid.
        let provider = OpenAiProvider::from_config(&Config::default()).unwrap();
        assert!(provider.supports_thinking());
    }

    #[test]
    fn test_openai_provider_metadata_name_is_openai() {
        // SAFETY: default config is always valid.
        let provider = OpenAiProvider::from_config(&Config::default()).unwrap();
        assert_eq!(provider.metadata().name, "openai");
    }

    #[test]
    fn test_openai_provider_metadata_capabilities_streaming_is_true() {
        // SAFETY: default config is always valid.
        let provider = OpenAiProvider::from_config(&Config::default()).unwrap();
        assert!(provider.metadata().capabilities.streaming);
    }

    #[test]
    fn test_openai_provider_metadata_capabilities_tools_is_true() {
        // SAFETY: default config is always valid.
        let provider = OpenAiProvider::from_config(&Config::default()).unwrap();
        assert!(provider.metadata().capabilities.tools);
    }
}
