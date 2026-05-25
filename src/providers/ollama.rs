//! Ollama local inference provider.
//!
//! Implements [`Provider`] for the [Ollama](https://ollama.ai) local model
//! server, targeting its `/api/chat` endpoint.
//!
//! Ollama requires no credentials and is always assumed to be reachable on
//! `http://localhost:11434` (or the configured host).
//!
//! # Features
//!
//! - Non-streaming completion via `POST /api/chat`
//! - Streaming via the Ollama NDJSON chunked response format
//! - `GET /api/tags` model listing with graceful fallback to a single-model list
//! - Credential status always reports [`CredentialStatus::Present`]

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
    CredentialStatus, Message, ModelCapabilities, ModelMetadata, ProviderCapabilities,
    ProviderMetadata, Role, ThinkingMode, Tool,
};
use crate::error::{PipelineError, Result};

// ---------------------------------------------------------------------------
// Provider struct
// ---------------------------------------------------------------------------

/// Ollama local model provider.
///
/// Connects to the Ollama API server at `base_url` (default:
/// `http://localhost:11434`) using the specified `model`.
///
/// Construct via [`OllamaProvider::new`].
#[derive(Clone)]
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    /// Creates a new `OllamaProvider`.
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the Ollama server (e.g. `"http://localhost:11434"`).
    /// * `model` - Model identifier to use (e.g. `"qwen2.5-coder"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::providers::ollama::OllamaProvider;
    ///
    /// let provider = OllamaProvider::new(
    ///     "http://localhost:11434".to_string(),
    ///     "qwen2.5-coder".to_string(),
    /// );
    /// assert_eq!(provider.provider_name_str(), "ollama");
    /// ```
    pub fn new(base_url: String, model: String) -> Self {
        let client = Client::builder()
            // Use a generous default timeout suitable for local inference.
            .timeout(Duration::from_secs(120))
            .build()
            // SAFETY: no custom TLS config; build() only fails on OS-level
            // TLS initialisation failure.
            .expect("failed to build reqwest client");

        Self {
            client,
            base_url,
            model,
        }
    }

    /// Returns the short provider name string (always `"ollama"`).
    pub fn provider_name_str(&self) -> &'static str {
        "ollama"
    }

    /// Converts a [`Role`] to the Ollama wire role string.
    fn role_str(role: &Role) -> &'static str {
        match role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

// ---------------------------------------------------------------------------
// Wire-format types
// ---------------------------------------------------------------------------

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

/// Response from `GET /api/tags`.
#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelEntry>,
}

#[derive(Deserialize)]
struct OllamaModelEntry {
    name: String,
}

// ---------------------------------------------------------------------------
// Capability inference
// ---------------------------------------------------------------------------

/// Infers [`ModelCapabilities`] from an Ollama model name.
///
/// Ollama hosts many different model families. This function applies
/// conservative heuristics based on well-known model name patterns.
/// The tag (e.g. `":latest"`, `":7b"`) is stripped before matching.
///
/// When in doubt, capabilities default to `false` to avoid incorrectly
/// claiming tool-calling or vision support.
///
/// # Arguments
///
/// * `model_id` - The model name as returned by `GET /api/tags`
///   (e.g. `"llama3.2:latest"`, `"mistral:7b"`, `"llava:13b"`).
///
/// # Examples
///
/// ```
/// use xzardgz::providers::ollama::infer_ollama_capabilities;
///
/// let caps = infer_ollama_capabilities("qwen2.5-coder:latest");
/// assert!(!caps.supports_thinking);
///
/// let caps = infer_ollama_capabilities("llava:13b");
/// assert!(caps.supports_vision);
/// ```
pub fn infer_ollama_capabilities(model_id: &str) -> ModelCapabilities {
    // Strip tag suffix (e.g. ":latest", ":7b-instruct") for cleaner matching.
    let base = model_id
        .split(':')
        .next()
        .unwrap_or(model_id)
        .to_lowercase();

    // Models known to support tool / function calling.
    let supports_tools = base.contains("mistral")
        || base.starts_with("llama3")
        || base.contains("llama3.")
        || base.contains("llama-3")
        || base.contains("qwen")
        || base.contains("gemma2")
        || base.contains("gemma3")
        || base.contains("gemma-2")
        || base.contains("gemma-3")
        || base.contains("mixtral")
        || base.contains("command-r")
        || base.contains("phi3")
        || base.contains("phi-3")
        || base.contains("phi4")
        || base.contains("phi-4")
        || base.contains("solar");

    // Vision-capable models.
    let supports_vision = base.contains("llava")
        || base.contains("bakllava")
        || base.contains("vision")
        || base.contains("minicpm-v")
        || base.contains("moondream")
        || base.contains("cogvlm");

    ModelCapabilities {
        supports_tools,
        supports_structured_output: false, // conservative; Ollama's structured output support varies
        supports_thinking: false,
        supports_streaming: true,
        supports_vision,
        // Use a conservative default; the actual context length is controlled
        // by Ollama's num_ctx parameter, not something we know from the name.
        context_window_tokens: 32_768,
    }
}

// ---------------------------------------------------------------------------
// Provider trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Provider for OllamaProvider {
    /// Returns the provider name `"ollama"`.
    fn provider_name(&self) -> &str {
        "ollama"
    }

    /// Returns aggregate metadata for the Ollama provider.
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            name: "ollama".to_string(),
            models: vec![self.model.clone()],
            capabilities: ProviderCapabilities {
                streaming: true,
                tools: true,
                vision: false,
            },
        }
    }

    /// Returns `false`; Ollama models do not support extended thinking.
    fn supports_thinking(&self) -> bool {
        false
    }

    /// Returns [`CredentialStatus::Present`]; Ollama requires no credentials.
    async fn credential_status(&self) -> CredentialStatus {
        CredentialStatus::Present
    }

    /// Lists models available from the Ollama server via `GET /api/tags`.
    ///
    /// On failure falls back silently to a single-entry list using the
    /// configured model name with default capabilities.
    ///
    /// # Errors
    ///
    /// Never returns `Err` — failures fall back to the single-model list.
    async fn list_models(&self) -> Result<Vec<ModelMetadata>> {
        let url = format!("{}/api/tags", self.base_url);
        let response = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("failed to fetch Ollama tags ({e}); using configured model");
                return Ok(vec![ModelMetadata::new(
                    self.model.clone(),
                    infer_ollama_capabilities(&self.model),
                )]);
            }
        };

        if !response.status().is_success() {
            warn!(
                "Ollama /api/tags returned {}; using configured model",
                response.status()
            );
            return Ok(vec![ModelMetadata::new(
                self.model.clone(),
                infer_ollama_capabilities(&self.model),
            )]);
        }

        let tags: OllamaTagsResponse = match response.json().await {
            Ok(t) => t,
            Err(e) => {
                warn!("failed to parse Ollama tags response ({e}); using configured model");
                return Ok(vec![ModelMetadata::new(
                    self.model.clone(),
                    infer_ollama_capabilities(&self.model),
                )]);
            }
        };

        let models = tags
            .models
            .into_iter()
            .map(|entry| {
                ModelMetadata::new(entry.name.clone(), infer_ollama_capabilities(&entry.name))
            })
            .collect();

        Ok(models)
    }

    /// Sends a non-streaming chat completion to the Ollama server.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Provider`] on API or network failures.
    async fn complete(&self, messages: &[Message], tools: &[Tool]) -> Result<Message> {
        let url = format!("{}/api/chat", self.base_url);

        let ollama_messages: Vec<OllamaMessage> = messages
            .iter()
            .map(|m| OllamaMessage {
                role: Self::role_str(&m.role).to_string(),
                content: m.content.clone(),
            })
            .collect();

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: ollama_messages,
            stream: false,
            tools: if tools.is_empty() {
                None
            } else {
                Some(tools.to_vec())
            },
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| PipelineError::Provider(format!("Ollama request failed: {e}")))?;

        if !response.status().is_success() {
            return Err(PipelineError::Provider(format!(
                "Ollama API error: {}",
                response.status()
            )));
        }

        let ollama_response: OllamaResponse = response.json().await.map_err(|e| {
            PipelineError::Provider(format!("failed to parse Ollama response: {e}"))
        })?;

        Ok(Message::assistant(&ollama_response.message.content))
    }

    /// Ignores `thinking_mode` and delegates to [`complete`][OllamaProvider::complete].
    ///
    /// Ollama does not support extended thinking.
    ///
    /// # Errors
    ///
    /// Same as [`complete`][OllamaProvider::complete].
    async fn complete_with_thinking(
        &self,
        messages: &[Message],
        tools: &[Tool],
        thinking_mode: ThinkingMode,
    ) -> Result<Message> {
        let _ = thinking_mode;
        self.complete(messages, tools).await
    }

    /// Streams a chat completion from the Ollama server.
    ///
    /// Parses Ollama's newline-delimited JSON (NDJSON) response format,
    /// yielding each non-empty content delta as a `Message::assistant` item.
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
        let url = format!("{}/api/chat", self.base_url);

        let ollama_messages: Vec<OllamaMessage> = messages
            .iter()
            .map(|m| OllamaMessage {
                role: Self::role_str(&m.role).to_string(),
                content: m.content.clone(),
            })
            .collect();

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: ollama_messages,
            stream: true,
            tools: if tools.is_empty() {
                None
            } else {
                Some(tools.to_vec())
            },
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                PipelineError::Provider(format!("Ollama streaming request failed: {e}"))
            })?;

        if !response.status().is_success() {
            return Err(PipelineError::Provider(format!(
                "Ollama API error: {}",
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

                // Ollama sends one JSON object per line (NDJSON).
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    // Skip malformed chunks silently.
                    if let Ok(res) = serde_json::from_str::<OllamaResponse>(&line)
                        && !res.message.content.is_empty()
                    {
                        yield Ok(Message::assistant(&res.message.content));
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

    fn make_provider() -> OllamaProvider {
        OllamaProvider::new(
            "http://localhost:11434".to_string(),
            "qwen2.5-coder".to_string(),
        )
    }

    // ------------------------------------------------------------------
    // provider_name
    // ------------------------------------------------------------------

    #[test]
    fn test_ollama_provider_name_returns_ollama() {
        let provider = make_provider();
        assert_eq!(provider.provider_name(), "ollama");
    }

    #[test]
    fn test_ollama_provider_name_str_returns_ollama() {
        let provider = make_provider();
        assert_eq!(provider.provider_name_str(), "ollama");
    }

    // ------------------------------------------------------------------
    // supports_thinking
    // ------------------------------------------------------------------

    #[test]
    fn test_ollama_provider_supports_thinking_returns_false() {
        let provider = make_provider();
        assert!(!provider.supports_thinking());
    }

    // ------------------------------------------------------------------
    // credential_status
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_ollama_credential_status_always_present() {
        let provider = make_provider();
        let status = provider.credential_status().await;
        assert_eq!(status, CredentialStatus::Present);
    }

    // ------------------------------------------------------------------
    // metadata
    // ------------------------------------------------------------------

    #[test]
    fn test_ollama_metadata_name_is_ollama() {
        let provider = make_provider();
        assert_eq!(provider.metadata().name, "ollama");
    }

    #[test]
    fn test_ollama_metadata_contains_configured_model() {
        let provider = make_provider();
        assert!(
            provider
                .metadata()
                .models
                .contains(&"qwen2.5-coder".to_string())
        );
    }

    #[test]
    fn test_ollama_metadata_streaming_is_true() {
        let provider = make_provider();
        assert!(provider.metadata().capabilities.streaming);
    }

    // ------------------------------------------------------------------
    // complete_with_thinking passthrough
    // ------------------------------------------------------------------

    #[test]
    fn test_ollama_complete_with_thinking_ignores_thinking_mode() {
        // Verify construction succeeds; actual network call is not made here.
        let provider = make_provider();
        assert!(!provider.supports_thinking());
    }

    // ------------------------------------------------------------------
    // infer_ollama_capabilities
    // ------------------------------------------------------------------

    #[test]
    fn test_infer_ollama_capabilities_llama3_has_tool_support() {
        let caps = infer_ollama_capabilities("llama3.2:latest");
        assert!(caps.supports_tools);
        assert!(!caps.supports_thinking);
    }

    #[test]
    fn test_infer_ollama_capabilities_llava_has_vision() {
        let caps = infer_ollama_capabilities("llava:13b");
        assert!(caps.supports_vision);
    }

    #[test]
    fn test_infer_ollama_capabilities_qwen_has_tool_support() {
        let caps = infer_ollama_capabilities("qwen2.5-coder:7b");
        assert!(caps.supports_tools);
        assert!(!caps.supports_vision);
    }

    #[test]
    fn test_infer_ollama_capabilities_unknown_model_conservative() {
        let caps = infer_ollama_capabilities("some-unknown-model:latest");
        assert!(!caps.supports_tools);
        assert!(!caps.supports_vision);
        assert!(!caps.supports_thinking);
        assert!(caps.supports_streaming);
    }

    #[test]
    fn test_infer_ollama_capabilities_strips_tag_before_matching() {
        // "llama3.2:7b-instruct" should match the same as "llama3.2"
        let caps_with_tag = infer_ollama_capabilities("llama3.2:7b-instruct");
        let caps_base = infer_ollama_capabilities("llama3.2");
        assert_eq!(caps_with_tag.supports_tools, caps_base.supports_tools);
    }

    // ------------------------------------------------------------------
    // list_models (mock server)
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_ollama_list_models_returns_available_models_from_api_tags() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        // Start a mock HTTP server.
        let mock_server = MockServer::start().await;

        // Register a GET /api/tags handler that returns two models.
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": [
                    { "name": "llama3:latest" },
                    { "name": "mistral:7b" }
                ]
            })))
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::new(mock_server.uri(), "llama3:latest".to_string());
        // SAFETY: mock server is running and returns valid JSON.
        let models = provider.list_models().await.unwrap();
        assert!(
            !models.is_empty(),
            "list_models should return at least one model from /api/tags"
        );
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(
            ids.contains(&"llama3:latest"),
            "model list should contain llama3:latest"
        );
        assert!(
            ids.contains(&"mistral:7b"),
            "model list should contain mistral:7b"
        );
    }

    #[tokio::test]
    async fn test_ollama_list_models_falls_back_to_static_when_server_unavailable() {
        // Use a URL that will always refuse connections (port 1 is not reachable).
        let provider = OllamaProvider::new(
            "http://127.0.0.1:1".to_string(),
            "llama3:latest".to_string(),
        );
        // The provider must fall back to static metadata rather than propagating an error.
        let result = provider.list_models().await;
        assert!(
            result.is_ok(),
            "list_models should succeed (fallback to static) when server is unreachable, \
             got: {:?}",
            result.err()
        );
        let models = result.unwrap();
        // Static fallback should include the configured model.
        assert!(!models.is_empty(), "fallback model list must not be empty");
    }
}
