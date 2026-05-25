//! Core [`Provider`] trait definition.
//!
//! Every AI backend (OpenAI, Anthropic, Ollama, Copilot) implements this trait.
//! Compared with the original two-method version, the expanded trait adds
//! credential checking, model discovery, thinking-mode support, and a default
//! `complete_with_thinking` implementation that falls through to `complete`.
//!
//! # Object Safety
//!
//! The trait is object-safe and can be used as `Arc<dyn Provider + Send + Sync>`
//! throughout the pipeline.
//!
//! # Error Conventions
//!
//! All async methods return [`crate::error::Result<T>`].  Use
//! [`PipelineError::Auth`][crate::error::PipelineError::Auth] for 401/403
//! failures and
//! [`PipelineError::Provider`][crate::error::PipelineError::Provider] for API
//! or network failures.

use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;

use super::types::{
    CredentialStatus, Message, ModelMetadata, ProviderMetadata, ThinkingMode, Tool,
};
use crate::error::Result;

/// Core AI provider abstraction.
///
/// Every backend (OpenAI, Anthropic, Ollama, Copilot) implements this trait.
/// The trait is object-safe over `dyn Provider + Send + Sync`.
///
/// # Implementing a provider
///
/// At minimum a provider must implement the five required methods:
/// `provider_name`, `metadata`, `supports_thinking`, `credential_status`,
/// `list_models`, `complete`, and `complete_streaming`.  The default
/// implementation of `complete_with_thinking` delegates to `complete` and
/// ignores `thinking_mode`; providers that support extended reasoning should
/// override it.
///
/// # Error handling
///
/// All async methods return `crate::error::Result<T>`
/// (= `Result<T, PipelineError>`).  Use `PipelineError::Auth` for credential
/// failures, `PipelineError::Provider` for API and network failures.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Provider: Send + Sync {
    /// Returns the canonical provider name (e.g., `"openai"`, `"ollama"`).
    fn provider_name(&self) -> &str;

    /// Returns aggregate metadata about this provider (name, model list, caps).
    ///
    /// The returned [`ProviderMetadata`] reflects provider-level capabilities.
    /// For per-model detail use [`list_models`][Provider::list_models].
    fn metadata(&self) -> ProviderMetadata;

    /// Returns `true` if at least one model on this provider supports thinking.
    ///
    /// This is a fast, synchronous check intended for capability gating before
    /// the more expensive [`list_models`][Provider::list_models] call.
    fn supports_thinking(&self) -> bool;

    /// Checks whether credentials are present without calling the remote API.
    ///
    /// Inspects environment variables and the system keyring and returns:
    /// - [`CredentialStatus::Present`] — credentials found in at least one location.
    /// - [`CredentialStatus::Missing`] — no credentials found anywhere.
    /// - [`CredentialStatus::Unknown`] — status cannot be determined (e.g., keyring unavailable).
    async fn credential_status(&self) -> CredentialStatus;

    /// Lists models available from this provider.
    ///
    /// Implementations SHOULD fall back to compiled-in static metadata when the
    /// provider is offline or unauthenticated, returning `Ok(static_list)`.
    /// The [`MetadataSource`][crate::providers::types::MetadataSource] carried
    /// in the resolution result indicates which source was used.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Provider`][crate::error::PipelineError::Provider]
    /// on unrecoverable API failures where no static fallback is available.
    async fn list_models(&self) -> Result<Vec<ModelMetadata>>;

    /// Sends a chat completion request with optional tool definitions.
    ///
    /// # Arguments
    ///
    /// * `messages` - Ordered conversation history to send to the model.
    /// * `tools` - Tool definitions the model may invoke; pass `&[]` for none.
    ///
    /// # Returns
    ///
    /// The assistant [`Message`] produced by the model.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Auth`][crate::error::PipelineError::Auth] on
    /// 401/403.  Returns
    /// [`PipelineError::Provider`][crate::error::PipelineError::Provider] on
    /// API or network failures.
    async fn complete(&self, messages: &[Message], tools: &[Tool]) -> Result<Message>;

    /// Sends a chat completion request with an explicit thinking mode.
    ///
    /// The default implementation ignores `thinking_mode` and delegates to
    /// [`complete`][Provider::complete].  Providers that support extended
    /// reasoning should override this method to forward the budget to the API.
    ///
    /// Providers that do **NOT** support thinking MUST:
    /// - For [`ThinkingMode::None`] or [`ThinkingMode::Auto`]: call `complete`
    ///   normally.
    /// - For explicit levels (`Low`/`Medium`/`High`/`ExtraHigh`): the caller
    ///   is responsible for resolving the thinking mode via the
    ///   [`ModelResolver`][crate::providers::model_resolution::ModelResolver]
    ///   before calling this method; the default implementation falls through
    ///   to `complete`.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Provider`][crate::error::PipelineError::Provider]
    /// on API or network failures.
    async fn complete_with_thinking(
        &self,
        messages: &[Message],
        tools: &[Tool],
        thinking_mode: ThinkingMode,
    ) -> Result<Message> {
        let _ = thinking_mode;
        self.complete(messages, tools).await
    }

    /// Streams a chat completion, yielding incremental delta messages.
    ///
    /// Each item in the returned stream is a partial assistant [`Message`]
    /// containing the latest token delta.  Callers should concatenate `content`
    /// fields to reconstruct the full response.
    ///
    /// # Arguments
    ///
    /// * `messages` - Ordered conversation history to send to the model.
    /// * `tools` - Tool definitions the model may invoke; pass `&[]` for none.
    ///
    /// # Returns
    ///
    /// A pinned, boxed [`Stream`] that yields `Result<Message>` items until the
    /// model signals completion.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Auth`][crate::error::PipelineError::Auth] on
    /// 401/403.  Returns
    /// [`PipelineError::Provider`][crate::error::PipelineError::Provider] on
    /// API or network failures before the stream begins.  Individual stream
    /// items may also carry [`PipelineError::Provider`] on mid-stream errors.
    async fn complete_streaming(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Message>> + Send>>>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::types::{ModelCapabilities, ProviderCapabilities};

    /// Builds a minimal [`ProviderMetadata`] for test assertions.
    fn make_provider_metadata(name: &str) -> ProviderMetadata {
        ProviderMetadata {
            name: name.to_string(),
            models: vec!["model-a".to_string(), "model-b".to_string()],
            capabilities: ProviderCapabilities {
                streaming: true,
                tools: false,
                vision: false,
            },
        }
    }

    /// Builds a minimal [`ModelMetadata`] for test assertions.
    fn make_model_metadata(id: &str) -> ModelMetadata {
        ModelMetadata::new(id, ModelCapabilities::default())
    }

    // ------------------------------------------------------------------
    // provider_name
    // ------------------------------------------------------------------

    #[test]
    fn test_mock_provider_returns_expected_provider_name() {
        let mut mock = MockProvider::new();
        mock.expect_provider_name()
            .return_const("mock_provider".to_string());
        assert_eq!(mock.provider_name(), "mock_provider");
    }

    #[test]
    fn test_mock_provider_returns_ollama_provider_name() {
        let mut mock = MockProvider::new();
        mock.expect_provider_name()
            .return_const("ollama".to_string());
        assert_eq!(mock.provider_name(), "ollama");
    }

    // ------------------------------------------------------------------
    // supports_thinking
    // ------------------------------------------------------------------

    #[test]
    fn test_mock_provider_supports_thinking_returns_false() {
        let mut mock = MockProvider::new();
        mock.expect_supports_thinking().returning(|| false);
        assert!(!mock.supports_thinking());
    }

    #[test]
    fn test_mock_provider_supports_thinking_returns_true() {
        let mut mock = MockProvider::new();
        mock.expect_supports_thinking().returning(|| true);
        assert!(mock.supports_thinking());
    }

    // ------------------------------------------------------------------
    // metadata
    // ------------------------------------------------------------------

    #[test]
    fn test_mock_provider_metadata_returns_expected_name() {
        let mut mock = MockProvider::new();
        mock.expect_metadata()
            .returning(|| make_provider_metadata("test_backend"));
        let meta = mock.metadata();
        assert_eq!(meta.name, "test_backend");
    }

    #[test]
    fn test_mock_provider_metadata_returns_expected_models() {
        let mut mock = MockProvider::new();
        mock.expect_metadata()
            .returning(|| make_provider_metadata("test_backend"));
        let meta = mock.metadata();
        assert_eq!(meta.models, vec!["model-a", "model-b"]);
    }

    #[test]
    fn test_mock_provider_metadata_streaming_capability_is_true() {
        let mut mock = MockProvider::new();
        mock.expect_metadata()
            .returning(|| make_provider_metadata("test_backend"));
        let meta = mock.metadata();
        assert!(meta.capabilities.streaming);
    }

    // ------------------------------------------------------------------
    // credential_status
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_mock_provider_credential_status_returns_present() {
        let mut mock = MockProvider::new();
        mock.expect_credential_status()
            .returning(|| CredentialStatus::Present);
        assert_eq!(mock.credential_status().await, CredentialStatus::Present);
    }

    #[tokio::test]
    async fn test_mock_provider_credential_status_returns_missing() {
        let mut mock = MockProvider::new();
        mock.expect_credential_status()
            .returning(|| CredentialStatus::Missing);
        assert_eq!(mock.credential_status().await, CredentialStatus::Missing);
    }

    #[tokio::test]
    async fn test_mock_provider_credential_status_returns_unknown() {
        let mut mock = MockProvider::new();
        mock.expect_credential_status()
            .returning(|| CredentialStatus::Unknown);
        assert_eq!(mock.credential_status().await, CredentialStatus::Unknown);
    }

    // ------------------------------------------------------------------
    // list_models
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_mock_provider_list_models_returns_expected_list() {
        let mut mock = MockProvider::new();
        mock.expect_list_models().returning(|| {
            Ok(vec![
                make_model_metadata("model-a"),
                make_model_metadata("model-b"),
            ])
        });
        // SAFETY: mock expectation set above guarantees Ok.
        let models = mock.list_models().await.unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "model-a");
        assert_eq!(models[1].id, "model-b");
    }

    #[tokio::test]
    async fn test_mock_provider_list_models_returns_empty_list() {
        let mut mock = MockProvider::new();
        mock.expect_list_models().returning(|| Ok(vec![]));
        // SAFETY: mock expectation set above guarantees Ok.
        let models = mock.list_models().await.unwrap();
        assert!(models.is_empty());
    }

    #[tokio::test]
    async fn test_mock_provider_list_models_propagates_error() {
        let mut mock = MockProvider::new();
        mock.expect_list_models().returning(|| {
            Err(crate::error::PipelineError::Provider(
                "api unavailable".to_string(),
            ))
        });
        let result = mock.list_models().await;
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // complete
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_mock_provider_complete_returns_expected_message() {
        let mut mock = MockProvider::new();
        mock.expect_complete()
            .returning(|_, _| Ok(Message::assistant("Hello from mock!")));
        // SAFETY: mock expectation set above guarantees Ok.
        let msg = mock.complete(&[], &[]).await.unwrap();
        assert_eq!(msg.content, "Hello from mock!");
    }

    #[tokio::test]
    async fn test_mock_provider_complete_propagates_auth_error() {
        let mut mock = MockProvider::new();
        mock.expect_complete().returning(|_, _| {
            Err(crate::error::PipelineError::Auth(
                "invalid api key".to_string(),
            ))
        });
        let result = mock.complete(&[], &[]).await;
        assert!(matches!(result, Err(crate::error::PipelineError::Auth(_))));
    }
}
