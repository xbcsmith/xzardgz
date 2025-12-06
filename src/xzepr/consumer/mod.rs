//! XZepr Consumer Module
//!
//! This module provides functionality for downstream services to:
//! - Consume CloudEvents messages from XZepr Kafka topics
//! - Parse and process event data
//! - Post work lifecycle events back to XZepr
//!
//! # Example
//!
//! ```rust,no_run
//! use xzardgz::xzepr::consumer::{
//!     KafkaConsumerConfig, XzeprConsumer, XzeprClient, MessageHandler,
//!     CloudEventMessage,
//! };
//! use std::sync::Arc;
//!
//! struct MyHandler {
//!     client: XzeprClient,
//!     receiver_id: String,
//! }
//!
//! #[async_trait::async_trait]
//! impl MessageHandler for MyHandler {
//!     async fn handle(&self, message: CloudEventMessage) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!         // Post work started
//!         let _ = self.client.post_work_started(
//!             &self.receiver_id,
//!             &message.id,
//!             "my-work",
//!             "1.0.0",
//!             "kubernetes",
//!             "my-service",
//!             serde_json::json!({}),
//!         ).await;
//!
//!         // Do work...
//!
//!         Ok(())
//!     }
//! }
//! ```

pub mod client;
pub mod config;
pub mod kafka;
pub mod message;

pub use client::{
    ClientError, CreateEventReceiverRequest, CreateEventRequest, XzeprClient, XzeprClientConfig,
};
pub use config::{KafkaConsumerConfig, SaslConfig, SaslMechanism, SecurityProtocol, SslConfig};
pub use kafka::{ConsumerError, MessageHandler, XzeprConsumer};
pub use message::{CloudEventData, CloudEventMessage, EventEntity, EventReceiverEntity};
