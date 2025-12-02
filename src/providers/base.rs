use crate::error::ProviderError;
use crate::providers::types::{Message, ProviderMetadata, Tool};
use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;

#[async_trait]
pub trait Provider: Send + Sync {
    fn metadata(&self) -> ProviderMetadata;

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<Message, ProviderError>;

    async fn complete_streaming(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Message, ProviderError>> + Send>>, ProviderError>;
}
