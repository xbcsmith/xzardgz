# Downstream Kafka Consumer Implementation Plan

---

## Implementation Prompt

Copy and paste this prompt into a new chat session with the plan document attached:

```
I need you to implement the Downstream Kafka Consumer plan from the attached document. This plan enables downstream services (Rust and Python) to consume CloudEvents messages from XZepr's Kafka topics, process work, and post status events back to XZepr.

Key requirements:
- Rust consumer with embedded code (not separate crate)
- Python consumer using httpx and async exclusively
- Event receiver discovery/creation via XZepr API
- Work lifecycle events: work.started, work.completed, work.failed
- Consumer group naming: xzepr-consumer-{service-name} with override option
- SASL/SCRAM-SHA-256 authentication support for Kafka

Please implement the phases in order:
1. Phase 1: Rust Consumer Implementation
2. Phase 2: Python Consumer Implementation
3. Phase 3: Integration Examples
4. Phase 4: Documentation

Follow the AGENTS.md rules for code quality, testing (>80% coverage), and documentation. Run cargo fmt, cargo check, cargo clippy, and cargo test after Rust implementation.

Start with Phase 1, Task 1.1: CloudEvents Message Types for Rust.
```

---

## Overview

This plan provides implementation guidance for downstream services to consume
CloudEvents messages from XZepr's Kafka topics, process work, and post status
events back to XZepr. The implementation covers both Rust and Python services
with embedded consumer code (not separate SDK crates).

Downstream services will:

1. Authenticate to Kafka using SASL/SCRAM or SSL/TLS
2. Consume CloudEvents messages from XZepr topics
3. Parse and understand message data
4. Trigger work based on event content
5. Register/discover their own event receiver in XZepr
6. POST lifecycle events back to XZepr (`work.started`, `work.completed`, `work.failed`)

## Current State Analysis

### Existing Infrastructure

XZepr provides the following infrastructure that downstream services will integrate with:

- **Kafka Producer**: `KafkaEventPublisher` in `src/infrastructure/messaging/producer.rs`
- **CloudEvents Format**: `CloudEventMessage` struct in `src/infrastructure/messaging/cloudevents.rs`
- **Kafka Authentication**: `KafkaAuthConfig` with SASL/SCRAM support in `src/infrastructure/messaging/config.rs`
- **REST API**: Event creation endpoint at `POST /api/v1/events`
- **Event Receivers API**: `POST /api/v1/receivers` for registration, `GET /api/v1/receivers` for discovery

### Message Format

XZepr publishes CloudEvents 1.0.1 compatible messages with this structure:

```json
{
  "success": true,
  "id": "01JXXXXXXXXXXXXXXXXXXXXXXX",
  "specversion": "1.0.1",
  "type": "deployment.success",
  "source": "xzepr.event.receiver.01JXXXXXXXXXXXXXXXXXXXXXXX",
  "api_version": "v1",
  "name": "deployment.success",
  "version": "1.0.0",
  "release": "1.0.0-rc.1",
  "platform_id": "kubernetes",
  "package": "myapp",
  "data": {
    "events": [...],
    "event_receivers": [...],
    "event_receiver_groups": [...]
  }
}
```

### Identified Requirements

1. Downstream services need embedded consumer code (not separate crate/package)
2. Services must register their own event receiver or discover existing ones
3. Consumer group naming defaults to `xzepr-consumer-{service-name}` with override option
4. Work lifecycle events: `work.started`, `work.completed`, `work.failed`
5. Support both Rust and Python implementations

## Implementation Phases

### Phase 1: Rust Consumer Implementation

#### Task 1.1: CloudEvents Message Types

Create Rust structs to deserialize CloudEvents messages from XZepr.

**File**: `src/xzepr/consumer/message.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// CloudEvents 1.0.1 message from XZepr
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudEventMessage {
    /// Indicates if the event represents success
    pub success: bool,

    /// Unique event identifier (ULID)
    pub id: String,

    /// CloudEvents specification version
    pub specversion: String,

    /// Event type/name
    #[serde(rename = "type")]
    pub event_type: String,

    /// Event source URI
    pub source: String,

    /// XZepr API version
    pub api_version: String,

    /// Event name
    pub name: String,

    /// Event version
    pub version: String,

    /// Release identifier
    pub release: String,

    /// Platform identifier
    pub platform_id: String,

    /// Package name
    pub package: String,

    /// Event payload data
    pub data: CloudEventData,
}

/// Data payload containing entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudEventData {
    /// Events in this message
    pub events: Vec<EventEntity>,

    /// Event receivers in this message
    pub event_receivers: Vec<EventReceiverEntity>,

    /// Event receiver groups in this message
    pub event_receiver_groups: Vec<EventReceiverGroupEntity>,
}

/// Event entity from XZepr
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEntity {
    pub id: String,
    pub name: String,
    pub version: String,
    pub release: String,
    pub platform_id: String,
    pub package: String,
    pub description: String,
    pub payload: JsonValue,
    pub success: bool,
    pub event_receiver_id: String,
    pub created_at: DateTime<Utc>,
}

/// Event receiver entity from XZepr
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventReceiverEntity {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub receiver_type: String,
    pub version: String,
    pub description: String,
    pub schema: JsonValue,
    pub fingerprint: String,
    pub created_at: DateTime<Utc>,
}

/// Event receiver group entity from XZepr
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventReceiverGroupEntity {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub group_type: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    pub event_receiver_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

#### Task 1.2: Kafka Consumer Configuration

Create configuration structs for Kafka consumer with authentication support.

**File**: `src/xzepr/consumer/config.rs`

```rust
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Missing required configuration: {0}")]
    MissingConfig(String),

    #[error("Invalid security protocol: {0}")]
    InvalidSecurityProtocol(String),

    #[error("Invalid SASL mechanism: {0}")]
    InvalidSaslMechanism(String),
}

/// Security protocol for Kafka connection
#[derive(Debug, Clone, Default)]
pub enum SecurityProtocol {
    #[default]
    Plaintext,
    Ssl,
    SaslPlaintext,
    SaslSsl,
}

impl SecurityProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plaintext => "PLAINTEXT",
            Self::Ssl => "SSL",
            Self::SaslPlaintext => "SASL_PLAINTEXT",
            Self::SaslSsl => "SASL_SSL",
        }
    }
}

/// SASL authentication mechanism
#[derive(Debug, Clone, Default)]
pub enum SaslMechanism {
    Plain,
    #[default]
    ScramSha256,
    ScramSha512,
}

impl SaslMechanism {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plain => "PLAIN",
            Self::ScramSha256 => "SCRAM-SHA-256",
            Self::ScramSha512 => "SCRAM-SHA-512",
        }
    }
}

/// SASL authentication configuration
#[derive(Debug, Clone)]
pub struct SaslConfig {
    pub mechanism: SaslMechanism,
    pub username: String,
    pub password: String,
}

/// SSL/TLS configuration
#[derive(Debug, Clone)]
pub struct SslConfig {
    pub ca_location: Option<String>,
    pub certificate_location: Option<String>,
    pub key_location: Option<String>,
}

/// Kafka consumer configuration
#[derive(Debug, Clone)]
pub struct KafkaConsumerConfig {
    /// Kafka broker addresses (comma-separated)
    pub brokers: String,

    /// Topic to consume from
    pub topic: String,

    /// Consumer group ID (defaults to xzepr-consumer-{service_name})
    pub group_id: String,

    /// Service name for identification
    pub service_name: String,

    /// Security protocol
    pub security_protocol: SecurityProtocol,

    /// SASL configuration (required for SASL protocols)
    pub sasl_config: Option<SaslConfig>,

    /// SSL configuration (required for SSL protocols)
    pub ssl_config: Option<SslConfig>,

    /// Auto offset reset policy
    pub auto_offset_reset: String,

    /// Enable auto commit
    pub enable_auto_commit: bool,

    /// Session timeout
    pub session_timeout: Duration,
}

impl KafkaConsumerConfig {
    /// Create new configuration with defaults
    pub fn new(brokers: &str, topic: &str, service_name: &str) -> Self {
        let group_id = format!("xzepr-consumer-{}", service_name);
        Self {
            brokers: brokers.to_string(),
            topic: topic.to_string(),
            group_id,
            service_name: service_name.to_string(),
            security_protocol: SecurityProtocol::default(),
            sasl_config: None,
            ssl_config: None,
            auto_offset_reset: "earliest".to_string(),
            enable_auto_commit: true,
            session_timeout: Duration::from_secs(30),
        }
    }

    /// Set custom consumer group ID
    pub fn with_group_id(mut self, group_id: &str) -> Self {
        self.group_id = group_id.to_string();
        self
    }

    /// Configure SASL/SCRAM-SHA-256 authentication
    pub fn with_sasl_scram_sha256(
        mut self,
        username: &str,
        password: &str,
    ) -> Self {
        self.security_protocol = SecurityProtocol::SaslSsl;
        self.sasl_config = Some(SaslConfig {
            mechanism: SaslMechanism::ScramSha256,
            username: username.to_string(),
            password: password.to_string(),
        });
        self
    }

    /// Configure SSL/TLS
    pub fn with_ssl(mut self, ca_location: &str) -> Self {
        self.ssl_config = Some(SslConfig {
            ca_location: Some(ca_location.to_string()),
            certificate_location: None,
            key_location: None,
        });
        self
    }

    /// Load configuration from environment variables
    pub fn from_env(service_name: &str) -> Result<Self, ConfigError> {
        let brokers = std::env::var("XZEPR_KAFKA_BROKERS")
            .unwrap_or_else(|_| "localhost:9092".to_string());

        let topic = std::env::var("XZEPR_KAFKA_TOPIC")
            .unwrap_or_else(|_| "xzepr.dev.events".to_string());

        let group_id = std::env::var("XZEPR_KAFKA_GROUP_ID")
            .unwrap_or_else(|_| format!("xzepr-consumer-{}", service_name));

        let mut config = Self::new(&brokers, &topic, service_name)
            .with_group_id(&group_id);

        // Load security protocol
        let protocol = std::env::var("XZEPR_KAFKA_SECURITY_PROTOCOL")
            .unwrap_or_else(|_| "PLAINTEXT".to_string());

        config.security_protocol = match protocol.to_uppercase().as_str() {
            "PLAINTEXT" => SecurityProtocol::Plaintext,
            "SSL" => SecurityProtocol::Ssl,
            "SASL_PLAINTEXT" => SecurityProtocol::SaslPlaintext,
            "SASL_SSL" => SecurityProtocol::SaslSsl,
            _ => return Err(ConfigError::InvalidSecurityProtocol(protocol)),
        };

        // Load SASL config if needed
        if matches!(
            config.security_protocol,
            SecurityProtocol::SaslPlaintext | SecurityProtocol::SaslSsl
        ) {
            let username = std::env::var("XZEPR_KAFKA_SASL_USERNAME")
                .map_err(|_| ConfigError::MissingConfig("XZEPR_KAFKA_SASL_USERNAME".to_string()))?;
            let password = std::env::var("XZEPR_KAFKA_SASL_PASSWORD")
                .map_err(|_| ConfigError::MissingConfig("XZEPR_KAFKA_SASL_PASSWORD".to_string()))?;

            let mechanism = std::env::var("XZEPR_KAFKA_SASL_MECHANISM")
                .unwrap_or_else(|_| "SCRAM-SHA-256".to_string());

            let sasl_mechanism = match mechanism.to_uppercase().as_str() {
                "PLAIN" => SaslMechanism::Plain,
                "SCRAM-SHA-256" => SaslMechanism::ScramSha256,
                "SCRAM-SHA-512" => SaslMechanism::ScramSha512,
                _ => return Err(ConfigError::InvalidSaslMechanism(mechanism)),
            };

            config.sasl_config = Some(SaslConfig {
                mechanism: sasl_mechanism,
                username,
                password,
            });
        }

        // Load SSL config if needed
        if matches!(
            config.security_protocol,
            SecurityProtocol::Ssl | SecurityProtocol::SaslSsl
        ) {
            let ca_location = std::env::var("XZEPR_KAFKA_SSL_CA_LOCATION").ok();
            if ca_location.is_some() {
                config.ssl_config = Some(SslConfig {
                    ca_location,
                    certificate_location: std::env::var("XZEPR_KAFKA_SSL_CERT_LOCATION").ok(),
                    key_location: std::env::var("XZEPR_KAFKA_SSL_KEY_LOCATION").ok(),
                });
            }
        }

        Ok(config)
    }
}
```

#### Task 1.3: Kafka Consumer Implementation

Create the Kafka consumer that processes CloudEvents messages.

**File**: `src/xzepr/consumer/kafka.rs`

```rust
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::message::Message;
use rdkafka::ClientContext;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::config::KafkaConsumerConfig;
use super::message::CloudEventMessage;

#[derive(Error, Debug)]
pub enum ConsumerError {
    #[error("Kafka error: {0}")]
    Kafka(#[from] rdkafka::error::KafkaError),

    #[error("Deserialization error: {0}")]
    Deserialization(#[from] serde_json::Error),

    #[error("Consumer not running")]
    NotRunning,
}

/// Custom context for logging
struct XzeprConsumerContext;

impl ClientContext for XzeprConsumerContext {}

impl rdkafka::consumer::ConsumerContext for XzeprConsumerContext {
    fn pre_rebalance(&self, rebalance: &rdkafka::consumer::Rebalance) {
        info!("Pre-rebalance: {:?}", rebalance);
    }

    fn post_rebalance(&self, rebalance: &rdkafka::consumer::Rebalance) {
        info!("Post-rebalance: {:?}", rebalance);
    }

    fn commit_callback(
        &self,
        result: rdkafka::error::KafkaResult<()>,
        _offsets: &rdkafka::TopicPartitionList,
    ) {
        match result {
            Ok(_) => debug!("Offsets committed successfully"),
            Err(e) => warn!("Error committing offsets: {}", e),
        }
    }
}

/// Handler trait for processing CloudEvents messages
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    /// Process a CloudEvents message
    ///
    /// Return Ok(()) to acknowledge the message, Err to skip/retry
    async fn handle(&self, message: CloudEventMessage) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// XZepr Kafka consumer
pub struct XzeprConsumer {
    consumer: StreamConsumer<XzeprConsumerContext>,
    topic: String,
    service_name: String,
}

impl XzeprConsumer {
    /// Create a new consumer from configuration
    pub fn new(config: KafkaConsumerConfig) -> Result<Self, ConsumerError> {
        let mut client_config = ClientConfig::new();

        client_config
            .set("bootstrap.servers", &config.brokers)
            .set("group.id", &config.group_id)
            .set("auto.offset.reset", &config.auto_offset_reset)
            .set("enable.auto.commit", config.enable_auto_commit.to_string())
            .set("session.timeout.ms", config.session_timeout.as_millis().to_string())
            .set("client.id", format!("xzepr-consumer-{}", config.service_name));

        // Apply security protocol
        client_config.set("security.protocol", config.security_protocol.as_str());

        // Apply SASL configuration
        if let Some(sasl) = &config.sasl_config {
            client_config
                .set("sasl.mechanism", sasl.mechanism.as_str())
                .set("sasl.username", &sasl.username)
                .set("sasl.password", &sasl.password);
        }

        // Apply SSL configuration
        if let Some(ssl) = &config.ssl_config {
            if let Some(ca) = &ssl.ca_location {
                client_config.set("ssl.ca.location", ca);
            }
            if let Some(cert) = &ssl.certificate_location {
                client_config.set("ssl.certificate.location", cert);
            }
            if let Some(key) = &ssl.key_location {
                client_config.set("ssl.key.location", key);
            }
        }

        let consumer: StreamConsumer<XzeprConsumerContext> = client_config
            .create_with_context(XzeprConsumerContext)?;

        Ok(Self {
            consumer,
            topic: config.topic,
            service_name: config.service_name,
        })
    }

    /// Subscribe to the configured topic
    pub fn subscribe(&self) -> Result<(), ConsumerError> {
        self.consumer.subscribe(&[&self.topic])?;
        info!(
            service = %self.service_name,
            topic = %self.topic,
            "Subscribed to Kafka topic"
        );
        Ok(())
    }

    /// Run the consumer with the given message handler
    pub async fn run<H: MessageHandler + 'static>(
        &self,
        handler: Arc<H>,
    ) -> Result<(), ConsumerError> {
        use futures::StreamExt;

        self.subscribe()?;

        info!(
            service = %self.service_name,
            "Starting message consumption"
        );

        let stream = self.consumer.stream();
        tokio::pin!(stream);

        while let Some(result) = stream.next().await {
            match result {
                Ok(message) => {
                    let payload = match message.payload_view::<str>() {
                        Some(Ok(s)) => s,
                        Some(Err(e)) => {
                            error!("Error deserializing message payload: {}", e);
                            continue;
                        }
                        None => {
                            warn!("Empty message payload");
                            continue;
                        }
                    };

                    match serde_json::from_str::<CloudEventMessage>(payload) {
                        Ok(event) => {
                            debug!(
                                event_id = %event.id,
                                event_type = %event.event_type,
                                "Processing CloudEvent"
                            );

                            if let Err(e) = handler.handle(event).await {
                                error!("Error handling message: {}", e);
                                // Continue processing other messages
                            }
                        }
                        Err(e) => {
                            error!("Error parsing CloudEvent: {}", e);
                            debug!("Raw payload: {}", payload);
                        }
                    }

                    // Commit offset after processing
                    if let Err(e) = self.consumer.commit_message(&message, CommitMode::Async) {
                        warn!("Error committing offset: {}", e);
                    }
                }
                Err(e) => {
                    error!("Kafka error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Run the consumer and send messages to a channel
    pub async fn run_with_channel(
        &self,
        sender: mpsc::Sender<CloudEventMessage>,
    ) -> Result<(), ConsumerError> {
        use futures::StreamExt;

        self.subscribe()?;

        let stream = self.consumer.stream();
        tokio::pin!(stream);

        while let Some(result) = stream.next().await {
            match result {
                Ok(message) => {
                    let payload = match message.payload_view::<str>() {
                        Some(Ok(s)) => s,
                        Some(Err(_)) | None => continue,
                    };

                    if let Ok(event) = serde_json::from_str::<CloudEventMessage>(payload) {
                        if sender.send(event).await.is_err() {
                            info!("Channel closed, stopping consumer");
                            break;
                        }

                        if let Err(e) = self.consumer.commit_message(&message, CommitMode::Async) {
                            warn!("Error committing offset: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Kafka error: {}", e);
                }
            }
        }

        Ok(())
    }
}
```

#### Task 1.4: XZepr API Client

Create an HTTP client for posting events back to XZepr.

**File**: `src/xzepr/consumer/client.rs`

```rust
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;
use tracing::{debug, error, info};

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Request to create an event receiver
#[derive(Debug, Serialize)]
pub struct CreateEventReceiverRequest {
    pub name: String,
    #[serde(rename = "type")]
    pub receiver_type: String,
    pub version: String,
    pub description: String,
    pub schema: JsonValue,
}

/// Response from creating an event receiver
#[derive(Debug, Deserialize)]
pub struct CreateEventReceiverResponse {
    pub data: String,
}

/// Event receiver entity from list response
#[derive(Debug, Deserialize)]
pub struct EventReceiverResponse {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub receiver_type: String,
    pub version: String,
    pub description: String,
    pub schema: JsonValue,
    pub fingerprint: String,
    pub created_at: String,
}

/// Paginated response wrapper
#[derive(Debug, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationMeta,
}

/// Pagination metadata
#[derive(Debug, Deserialize)]
pub struct PaginationMeta {
    pub limit: usize,
    pub offset: usize,
    pub total: usize,
    pub has_more: bool,
}

/// Request to create an event
#[derive(Debug, Serialize)]
pub struct CreateEventRequest {
    pub name: String,
    pub version: String,
    pub release: String,
    pub platform_id: String,
    pub package: String,
    pub description: String,
    pub payload: JsonValue,
    pub success: bool,
    pub event_receiver_id: String,
}

/// Response from creating an event
#[derive(Debug, Deserialize)]
pub struct CreateEventResponse {
    pub data: String,
}

/// XZepr API client configuration
#[derive(Debug, Clone)]
pub struct XzeprClientConfig {
    /// Base URL of the XZepr API
    pub base_url: String,

    /// JWT token for authentication
    pub token: String,

    /// Request timeout in seconds
    pub timeout_secs: u64,
}

impl XzeprClientConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> Result<Self, ClientError> {
        let base_url = std::env::var("XZEPR_API_URL")
            .unwrap_or_else(|_| "http://localhost:8042".to_string());

        let token = std::env::var("XZEPR_API_TOKEN")
            .map_err(|_| ClientError::Authentication("XZEPR_API_TOKEN not set".to_string()))?;

        Ok(Self {
            base_url,
            token,
            timeout_secs: 30,
        })
    }
}

/// XZepr API client for downstream services
pub struct XzeprClient {
    client: Client,
    config: XzeprClientConfig,
}

impl XzeprClient {
    /// Create a new XZepr client
    pub fn new(config: XzeprClientConfig) -> Result<Self, ClientError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()?;

        Ok(Self { client, config })
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self, ClientError> {
        let config = XzeprClientConfig::from_env()?;
        Self::new(config)
    }

    /// Build a request with authentication
    fn build_request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.config.base_url, path);
        self.client
            .request(method, &url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Content-Type", "application/json")
    }

    /// Create a new event receiver
    pub async fn create_event_receiver(
        &self,
        request: CreateEventReceiverRequest,
    ) -> Result<String, ClientError> {
        let response = self
            .build_request(reqwest::Method::POST, "/api/v1/receivers")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            let result: CreateEventReceiverResponse = response.json().await?;
            info!(
                receiver_id = %result.data,
                name = %request.name,
                "Created event receiver"
            );
            Ok(result.data)
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(ClientError::Api {
                status: status.as_u16(),
                message: body,
            })
        }
    }

    /// List event receivers with optional filters
    pub async fn list_event_receivers(
        &self,
        name_filter: Option<&str>,
        receiver_type: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<PaginatedResponse<EventReceiverResponse>, ClientError> {
        let mut query = vec![
            ("limit", limit.to_string()),
            ("offset", offset.to_string()),
        ];

        if let Some(name) = name_filter {
            query.push(("name", name.to_string()));
        }
        if let Some(rtype) = receiver_type {
            query.push(("type", rtype.to_string()));
        }

        let response = self
            .build_request(reqwest::Method::GET, "/api/v1/receivers")
            .query(&query)
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            let result: PaginatedResponse<EventReceiverResponse> = response.json().await?;
            Ok(result)
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(ClientError::Api {
                status: status.as_u16(),
                message: body,
            })
        }
    }

    /// Get an event receiver by ID
    pub async fn get_event_receiver(
        &self,
        id: &str,
    ) -> Result<EventReceiverResponse, ClientError> {
        let response = self
            .build_request(reqwest::Method::GET, &format!("/api/v1/receivers/{}", id))
            .send()
            .await?;

        let status = response.status();
        match status {
            StatusCode::OK => {
                let result: EventReceiverResponse = response.json().await?;
                Ok(result)
            }
            StatusCode::NOT_FOUND => Err(ClientError::NotFound(format!("Event receiver {}", id))),
            _ => {
                let body = response.text().await.unwrap_or_default();
                Err(ClientError::Api {
                    status: status.as_u16(),
                    message: body,
                })
            }
        }
    }

    /// Discover an existing event receiver by name, or create if not found
    pub async fn discover_or_create_event_receiver(
        &self,
        name: &str,
        receiver_type: &str,
        version: &str,
        description: &str,
        schema: JsonValue,
    ) -> Result<String, ClientError> {
        // First, try to find existing receiver by name
        let receivers = self
            .list_event_receivers(Some(name), Some(receiver_type), 10, 0)
            .await?;

        // Check for exact match
        for receiver in &receivers.data {
            if receiver.name == name && receiver.receiver_type == receiver_type {
                info!(
                    receiver_id = %receiver.id,
                    name = %name,
                    "Discovered existing event receiver"
                );
                return Ok(receiver.id.clone());
            }
        }

        // Not found, create new receiver
        info!(name = %name, "Event receiver not found, creating new one");
        let request = CreateEventReceiverRequest {
            name: name.to_string(),
            receiver_type: receiver_type.to_string(),
            version: version.to_string(),
            description: description.to_string(),
            schema,
        };

        self.create_event_receiver(request).await
    }

    /// Create a new event
    pub async fn create_event(&self, request: CreateEventRequest) -> Result<String, ClientError> {
        let response = self
            .build_request(reqwest::Method::POST, "/api/v1/events")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            let result: CreateEventResponse = response.json().await?;
            debug!(
                event_id = %result.data,
                event_name = %request.name,
                "Created event"
            );
            Ok(result.data)
        } else {
            let body = response.text().await.unwrap_or_default();
            error!(
                status = status.as_u16(),
                body = %body,
                "Failed to create event"
            );
            Err(ClientError::Api {
                status: status.as_u16(),
                message: body,
            })
        }
    }

    /// Post a work started event
    pub async fn post_work_started(
        &self,
        receiver_id: &str,
        work_id: &str,
        work_name: &str,
        version: &str,
        platform_id: &str,
        package: &str,
        payload: JsonValue,
    ) -> Result<String, ClientError> {
        let request = CreateEventRequest {
            name: format!("{}.started", work_name),
            version: version.to_string(),
            release: version.to_string(),
            platform_id: platform_id.to_string(),
            package: package.to_string(),
            description: format!("Work started: {} ({})", work_name, work_id),
            payload: serde_json::json!({
                "work_id": work_id,
                "status": "started",
                "started_at": chrono::Utc::now().to_rfc3339(),
                "details": payload
            }),
            success: true,
            event_receiver_id: receiver_id.to_string(),
        };

        self.create_event(request).await
    }

    /// Post a work completed event
    pub async fn post_work_completed(
        &self,
        receiver_id: &str,
        work_id: &str,
        work_name: &str,
        version: &str,
        platform_id: &str,
        package: &str,
        success: bool,
        payload: JsonValue,
    ) -> Result<String, ClientError> {
        let status_suffix = if success { "completed" } else { "failed" };
        let request = CreateEventRequest {
            name: format!("{}.{}", work_name, status_suffix),
            version: version.to_string(),
            release: version.to_string(),
            platform_id: platform_id.to_string(),
            package: package.to_string(),
            description: format!("Work {}: {} ({})", status_suffix, work_name, work_id),
            payload: serde_json::json!({
                "work_id": work_id,
                "status": status_suffix,
                "completed_at": chrono::Utc::now().to_rfc3339(),
                "success": success,
                "details": payload
            }),
            success,
            event_receiver_id: receiver_id.to_string(),
        };

        self.create_event(request).await
    }
}
```

#### Task 1.5: Module Organization

Create the module structure for the consumer.

**File**: `src/xzepr/consumer/mod.rs`

````rust
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
//! use your_service::xzepr::consumer::{
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
//!         self.client.post_work_started(
//!             &self.receiver_id,
//!             &message.id,
//!             "my-work",
//!             "1.0.0",
//!             "kubernetes",
//!             "my-service",
//!             serde_json::json!({}),
//!         ).await?;
//!
//!         // Do work...
//!
//!         // Post work completed
//!         self.client.post_work_completed(
//!             &self.receiver_id,
//!             &message.id,
//!             "my-work",
//!             "1.0.0",
//!             "kubernetes",
//!             "my-service",
//!             true,
//!             serde_json::json!({"result": "success"}),
//!         ).await?;
//!
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize consumer
//!     let config = KafkaConsumerConfig::from_env("my-service")?;
//!     let consumer = XzeprConsumer::new(config)?;
//!
//!     // Initialize XZepr client
//!     let client = XzeprClient::from_env()?;
//!
//!     // Register/discover event receiver
//!     let receiver_id = client.discover_or_create_event_receiver(
//!         "my-service-receiver",
//!         "worker",
//!         "1.0.0",
//!         "Event receiver for my-service",
//!         serde_json::json!({"type": "object"}),
//!     ).await?;
//!
//!     // Create handler
//!     let handler = Arc::new(MyHandler { client, receiver_id });
//!
//!     // Run consumer
//!     consumer.run(handler).await?;
//!
//!     Ok(())
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
````

#### Task 1.6: Testing Requirements

Unit tests for:

- Configuration parsing from environment variables
- CloudEvents message deserialization
- Client request building
- Error handling

Integration tests for:

- Kafka consumer with test containers
- XZepr API client with mock server

#### Task 1.7: Deliverables

- `src/xzepr/consumer/message.rs` - CloudEvents message types
- `src/xzepr/consumer/config.rs` - Kafka consumer configuration
- `src/xzepr/consumer/kafka.rs` - Kafka consumer implementation
- `src/xzepr/consumer/client.rs` - XZepr API client
- `src/xzepr/consumer/mod.rs` - Module exports

#### Task 1.8: Success Criteria

- Consumer can authenticate to Kafka using SASL/SCRAM-SHA-256
- CloudEvents messages are correctly deserialized
- Handler trait allows custom message processing
- XZepr client can create/discover event receivers
- Work lifecycle events are posted correctly

---

### Phase 2: Python Consumer Implementation

#### Task 2.1: CloudEvents Message Types

Create Python dataclasses for CloudEvents messages.

**File**: `xzepr_consumer/message.py`

```python
# SPDX-FileCopyrightText: 2025 Brett Smith <xbcsmith@gmail.com>
# SPDX-License-Identifier: Apache-2.0

"""CloudEvents message types for XZepr."""

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Dict, List, Optional
import json


@dataclass
class EventEntity:
    """Event entity from XZepr."""

    id: str
    name: str
    version: str
    release: str
    platform_id: str
    package: str
    description: str
    payload: Dict[str, Any]
    success: bool
    event_receiver_id: str
    created_at: datetime

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "EventEntity":
        """Create from dictionary."""
        return cls(
            id=data["id"],
            name=data["name"],
            version=data["version"],
            release=data["release"],
            platform_id=data["platform_id"],
            package=data["package"],
            description=data["description"],
            payload=data["payload"],
            success=data["success"],
            event_receiver_id=data["event_receiver_id"],
            created_at=datetime.fromisoformat(data["created_at"].replace("Z", "+00:00")),
        )


@dataclass
class EventReceiverEntity:
    """Event receiver entity from XZepr."""

    id: str
    name: str
    receiver_type: str
    version: str
    description: str
    schema: Dict[str, Any]
    fingerprint: str
    created_at: datetime

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "EventReceiverEntity":
        """Create from dictionary."""
        return cls(
            id=data["id"],
            name=data["name"],
            receiver_type=data.get("type", data.get("receiver_type", "")),
            version=data["version"],
            description=data["description"],
            schema=data["schema"],
            fingerprint=data["fingerprint"],
            created_at=datetime.fromisoformat(data["created_at"].replace("Z", "+00:00")),
        )


@dataclass
class EventReceiverGroupEntity:
    """Event receiver group entity from XZepr."""

    id: str
    name: str
    group_type: str
    version: str
    description: str
    enabled: bool
    event_receiver_ids: List[str]
    created_at: datetime
    updated_at: datetime

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "EventReceiverGroupEntity":
        """Create from dictionary."""
        return cls(
            id=data["id"],
            name=data["name"],
            group_type=data.get("type", data.get("group_type", "")),
            version=data["version"],
            description=data["description"],
            enabled=data["enabled"],
            event_receiver_ids=data["event_receiver_ids"],
            created_at=datetime.fromisoformat(data["created_at"].replace("Z", "+00:00")),
            updated_at=datetime.fromisoformat(data["updated_at"].replace("Z", "+00:00")),
        )


@dataclass
class CloudEventData:
    """Data payload for CloudEvents messages."""

    events: List[EventEntity] = field(default_factory=list)
    event_receivers: List[EventReceiverEntity] = field(default_factory=list)
    event_receiver_groups: List[EventReceiverGroupEntity] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "CloudEventData":
        """Create from dictionary."""
        return cls(
            events=[EventEntity.from_dict(e) for e in data.get("events", [])],
            event_receivers=[
                EventReceiverEntity.from_dict(r) for r in data.get("event_receivers", [])
            ],
            event_receiver_groups=[
                EventReceiverGroupEntity.from_dict(g)
                for g in data.get("event_receiver_groups", [])
            ],
        )


@dataclass
class CloudEventMessage:
    """CloudEvents 1.0.1 message from XZepr."""

    success: bool
    id: str
    specversion: str
    event_type: str
    source: str
    api_version: str
    name: str
    version: str
    release: str
    platform_id: str
    package: str
    data: CloudEventData

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "CloudEventMessage":
        """Create from dictionary."""
        return cls(
            success=data["success"],
            id=data["id"],
            specversion=data["specversion"],
            event_type=data["type"],
            source=data["source"],
            api_version=data["api_version"],
            name=data["name"],
            version=data["version"],
            release=data["release"],
            platform_id=data["platform_id"],
            package=data["package"],
            data=CloudEventData.from_dict(data["data"]),
        )

    @classmethod
    def from_json(cls, json_str: str) -> "CloudEventMessage":
        """Create from JSON string."""
        return cls.from_dict(json.loads(json_str))

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary."""
        return {
            "success": self.success,
            "id": self.id,
            "specversion": self.specversion,
            "type": self.event_type,
            "source": self.source,
            "api_version": self.api_version,
            "name": self.name,
            "version": self.version,
            "release": self.release,
            "platform_id": self.platform_id,
            "package": self.package,
            "data": {
                "events": [vars(e) for e in self.data.events],
                "event_receivers": [vars(r) for r in self.data.event_receivers],
                "event_receiver_groups": [vars(g) for g in self.data.event_receiver_groups],
            },
        }
```

#### Task 2.2: Kafka Consumer Configuration

Create configuration classes for Kafka consumer.

**File**: `xzepr_consumer/config.py`

```python
# SPDX-FileCopyrightText: 2025 Brett Smith <xbcsmith@gmail.com>
# SPDX-License-Identifier: Apache-2.0

"""Configuration for XZepr Kafka consumer."""

import os
from dataclasses import dataclass, field
from enum import Enum
from typing import Optional


class SecurityProtocol(Enum):
    """Kafka security protocol."""

    PLAINTEXT = "PLAINTEXT"
    SSL = "SSL"
    SASL_PLAINTEXT = "SASL_PLAINTEXT"
    SASL_SSL = "SASL_SSL"


class SaslMechanism(Enum):
    """SASL authentication mechanism."""

    PLAIN = "PLAIN"
    SCRAM_SHA_256 = "SCRAM-SHA-256"
    SCRAM_SHA_512 = "SCRAM-SHA-512"


@dataclass
class SaslConfig:
    """SASL authentication configuration."""

    mechanism: SaslMechanism
    username: str
    password: str


@dataclass
class SslConfig:
    """SSL/TLS configuration."""

    ca_location: Optional[str] = None
    certificate_location: Optional[str] = None
    key_location: Optional[str] = None


@dataclass
class KafkaConsumerConfig:
    """Kafka consumer configuration."""

    brokers: str
    topic: str
    service_name: str
    group_id: str = ""
    security_protocol: SecurityProtocol = SecurityProtocol.PLAINTEXT
    sasl_config: Optional[SaslConfig] = None
    ssl_config: Optional[SslConfig] = None
    auto_offset_reset: str = "earliest"
    enable_auto_commit: bool = True
    session_timeout_ms: int = 30000

    def __post_init__(self):
        """Set default group_id if not provided."""
        if not self.group_id:
            self.group_id = f"xzepr-consumer-{self.service_name}"

    @classmethod
    def from_env(cls, service_name: str) -> "KafkaConsumerConfig":
        """Load configuration from environment variables."""
        brokers = os.environ.get("XZEPR_KAFKA_BROKERS", "localhost:9092")
        topic = os.environ.get("XZEPR_KAFKA_TOPIC", "xzepr.dev.events")
        group_id = os.environ.get(
            "XZEPR_KAFKA_GROUP_ID", f"xzepr-consumer-{service_name}"
        )

        # Security protocol
        protocol_str = os.environ.get("XZEPR_KAFKA_SECURITY_PROTOCOL", "PLAINTEXT")
        security_protocol = SecurityProtocol(protocol_str.upper())

        # SASL configuration
        sasl_config = None
        if security_protocol in (
            SecurityProtocol.SASL_PLAINTEXT,
            SecurityProtocol.SASL_SSL,
        ):
            mechanism_str = os.environ.get(
                "XZEPR_KAFKA_SASL_MECHANISM", "SCRAM-SHA-256"
            )
            sasl_config = SaslConfig(
                mechanism=SaslMechanism(mechanism_str.upper()),
                username=os.environ["XZEPR_KAFKA_SASL_USERNAME"],
                password=os.environ["XZEPR_KAFKA_SASL_PASSWORD"],
            )

        # SSL configuration
        ssl_config = None
        if security_protocol in (SecurityProtocol.SSL, SecurityProtocol.SASL_SSL):
            ca_location = os.environ.get("XZEPR_KAFKA_SSL_CA_LOCATION")
            if ca_location:
                ssl_config = SslConfig(
                    ca_location=ca_location,
                    certificate_location=os.environ.get(
                        "XZEPR_KAFKA_SSL_CERT_LOCATION"
                    ),
                    key_location=os.environ.get("XZEPR_KAFKA_SSL_KEY_LOCATION"),
                )

        return cls(
            brokers=brokers,
            topic=topic,
            service_name=service_name,
            group_id=group_id,
            security_protocol=security_protocol,
            sasl_config=sasl_config,
            ssl_config=ssl_config,
        )

    def to_confluent_config(self) -> dict:
        """Convert to confluent-kafka configuration dictionary."""
        config = {
            "bootstrap.servers": self.brokers,
            "group.id": self.group_id,
            "auto.offset.reset": self.auto_offset_reset,
            "enable.auto.commit": self.enable_auto_commit,
            "session.timeout.ms": self.session_timeout_ms,
            "client.id": f"xzepr-consumer-{self.service_name}",
            "security.protocol": self.security_protocol.value,
        }

        if self.sasl_config:
            config["sasl.mechanism"] = self.sasl_config.mechanism.value
            config["sasl.username"] = self.sasl_config.username
            config["sasl.password"] = self.sasl_config.password

        if self.ssl_config:
            if self.ssl_config.ca_location:
                config["ssl.ca.location"] = self.ssl_config.ca_location
            if self.ssl_config.certificate_location:
                config["ssl.certificate.location"] = self.ssl_config.certificate_location
            if self.ssl_config.key_location:
                config["ssl.key.location"] = self.ssl_config.key_location

        return config
```

#### Task 2.3: Kafka Consumer Implementation

Create the Kafka consumer class.

**File**: `xzepr_consumer/consumer.py`

```python
# SPDX-FileCopyrightText: 2025 Brett Smith <xbcsmith@gmail.com>
# SPDX-License-Identifier: Apache-2.0

"""Kafka consumer for XZepr CloudEvents messages."""

import logging
from abc import ABC, abstractmethod
from typing import Callable, Optional

from confluent_kafka import Consumer, KafkaError, KafkaException

from .config import KafkaConsumerConfig
from .message import CloudEventMessage

logger = logging.getLogger(__name__)


class MessageHandler(ABC):
    """Abstract base class for message handlers."""

    @abstractmethod
    async def handle(self, message: CloudEventMessage) -> None:
        """
        Handle a CloudEvents message asynchronously.

        Args:
            message: The CloudEvents message to process

        Raises:
            Exception: If message processing fails
        """
        pass


class XzeprConsumer:
    """Kafka consumer for XZepr CloudEvents messages."""

    def __init__(self, config: KafkaConsumerConfig):
        """
        Initialize the consumer.

        Args:
            config: Kafka consumer configuration
        """
        self.config = config
        self._consumer: Optional[Consumer] = None
        self._running = False

    def _create_consumer(self) -> Consumer:
        """Create the Kafka consumer instance."""
        return Consumer(self.config.to_confluent_config())

    def subscribe(self) -> None:
        """Subscribe to the configured topic."""
        if self._consumer is None:
            self._consumer = self._create_consumer()

        self._consumer.subscribe([self.config.topic])
        logger.info(
            "Subscribed to topic %s with group %s",
            self.config.topic,
            self.config.group_id,
        )

    async def run(
        self,
        handler: MessageHandler,
        poll_timeout: float = 1.0,
    ) -> None:
        """
        Run the consumer with a message handler asynchronously.

        Args:
            handler: Handler for processing messages
            poll_timeout: Timeout in seconds for polling
        """
        self.subscribe()
        self._running = True

        logger.info("Starting message consumption for service %s", self.config.service_name)

        try:
            while self._running:
                msg = self._consumer.poll(poll_timeout)

                if msg is None:
                    continue

                if msg.error():
                    if msg.error().code() == KafkaError._PARTITION_EOF:
                        logger.debug(
                            "End of partition %s [%d] at offset %d",
                            msg.topic(),
                            msg.partition(),
                            msg.offset(),
                        )
                    else:
                        raise KafkaException(msg.error())
                    continue

                try:
                    value = msg.value()
                    if value is None:
                        logger.warning("Empty message payload")
                        continue

                    payload = value.decode("utf-8")
                    event = CloudEventMessage.from_json(payload)

                    logger.debug(
                        "Processing CloudEvent %s (type: %s)",
                        event.id,
                        event.event_type,
                    )

                    await handler.handle(event)

                except Exception as e:
                    logger.error("Error processing message: %s", e)
                    # Continue processing other messages

        finally:
            self.close()

    async def run_with_callback(
        self,
        callback: Callable[[CloudEventMessage], None],
        poll_timeout: float = 1.0,
    ) -> None:
        """
        Run the consumer with an async callback function.

        Args:
            callback: Async function to call for each message
            poll_timeout: Timeout in seconds for polling
        """

        class CallbackHandler(MessageHandler):
            def __init__(self, cb: Callable[[CloudEventMessage], None]):
                self.cb = cb

            async def handle(self, message: CloudEventMessage) -> None:
                await self.cb(message)

        await self.run(CallbackHandler(callback), poll_timeout)

    def stop(self) -> None:
        """Stop the consumer."""
        self._running = False
        logger.info("Consumer stop requested")

    def close(self) -> None:
        """Close the consumer."""
        if self._consumer:
            self._consumer.close()
            self._consumer = None
            logger.info("Consumer closed")
```

#### Task 2.4: XZepr API Client

Create the HTTP client for XZepr API.

**File**: `xzepr_consumer/client.py`

```python
# SPDX-FileCopyrightText: 2025 Brett Smith <xbcsmith@gmail.com>
# SPDX-License-Identifier: Apache-2.0

"""XZepr API client for downstream services."""

import logging
import os
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import httpx

logger = logging.getLogger(__name__)


class XzeprClientError(Exception):
    """Base exception for XZepr client errors."""

    pass


class XzeprApiError(XzeprClientError):
    """API error with status code."""

    def __init__(self, status_code: int, message: str):
        self.status_code = status_code
        self.message = message
        super().__init__(f"API error ({status_code}): {message}")


class XzeprAuthError(XzeprClientError):
    """Authentication error."""

    pass


class XzeprNotFoundError(XzeprClientError):
    """Resource not found error."""

    pass


@dataclass
class EventReceiverResponse:
    """Event receiver from API response."""

    id: str
    name: str
    receiver_type: str
    version: str
    description: str
    schema: Dict[str, Any]
    fingerprint: str
    created_at: str


@dataclass
class XzeprClientConfig:
    """XZepr API client configuration."""

    base_url: str
    token: str
    timeout: int = 30

    @classmethod
    def from_env(cls) -> "XzeprClientConfig":
        """Load configuration from environment variables."""
        base_url = os.environ.get("XZEPR_API_URL", "http://localhost:8042")
        token = os.environ.get("XZEPR_API_TOKEN")

        if not token:
            raise XzeprAuthError("XZEPR_API_TOKEN environment variable not set")

        return cls(base_url=base_url, token=token)


class XzeprClient:
    """XZepr API client for downstream services (async)."""

    def __init__(self, config: XzeprClientConfig):
        """
        Initialize the client.

        Args:
            config: Client configuration
        """
        self.config = config
        self._client: Optional[httpx.AsyncClient] = None

    async def __aenter__(self) -> "XzeprClient":
        """Async context manager entry."""
        await self._ensure_client()
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        """Async context manager exit."""
        await self.close()

    async def _ensure_client(self) -> httpx.AsyncClient:
        """Ensure the HTTP client is initialized."""
        if self._client is None:
            self._client = httpx.AsyncClient(
                base_url=self.config.base_url,
                headers={
                    "Authorization": f"Bearer {self.config.token}",
                    "Content-Type": "application/json",
                },
                timeout=self.config.timeout,
            )
        return self._client

    async def close(self) -> None:
        """Close the HTTP client."""
        if self._client is not None:
            await self._client.aclose()
            self._client = None

    @classmethod
    def from_env(cls) -> "XzeprClient":
        """Create client from environment variables."""
        config = XzeprClientConfig.from_env()
        return cls(config)

    async def _request(
        self,
        method: str,
        path: str,
        json: Optional[Dict] = None,
        params: Optional[Dict] = None,
    ) -> httpx.Response:
        """Make an async HTTP request."""
        client = await self._ensure_client()
        response = await client.request(
            method,
            path,
            json=json,
            params=params,
        )
        return response

    async def create_event_receiver(
        self,
        name: str,
        receiver_type: str,
        version: str,
        description: str,
        schema: Dict[str, Any],
    ) -> str:
        """
        Create a new event receiver.

        Args:
            name: Receiver name
            receiver_type: Type of receiver
            version: Version string
            description: Description
            schema: JSON schema for validation

        Returns:
            The created receiver ID
        """
        response = await self._request(
            "POST",
            "/api/v1/receivers",
            json={
                "name": name,
                "type": receiver_type,
                "version": version,
                "description": description,
                "schema": schema,
            },
        )

        if response.is_success:
            result = response.json()
            receiver_id = result["data"]
            logger.info("Created event receiver %s (name: %s)", receiver_id, name)
            return receiver_id
        else:
            raise XzeprApiError(response.status_code, response.text)

    async def list_event_receivers(
        self,
        name: Optional[str] = None,
        receiver_type: Optional[str] = None,
        limit: int = 50,
        offset: int = 0,
    ) -> List[EventReceiverResponse]:
        """
        List event receivers with optional filters.

        Args:
            name: Filter by name
            receiver_type: Filter by type
            limit: Maximum results
            offset: Pagination offset

        Returns:
            List of event receivers
        """
        params = {"limit": limit, "offset": offset}
        if name:
            params["name"] = name
        if receiver_type:
            params["type"] = receiver_type

        response = await self._request("GET", "/api/v1/receivers", params=params)

        if response.is_success:
            result = response.json()
            return [
                EventReceiverResponse(
                    id=r["id"],
                    name=r["name"],
                    receiver_type=r.get("type", r.get("receiver_type", "")),
                    version=r["version"],
                    description=r["description"],
                    schema=r["schema"],
                    fingerprint=r["fingerprint"],
                    created_at=r["created_at"],
                )
                for r in result["data"]
            ]
        else:
            raise XzeprApiError(response.status_code, response.text)

    async def get_event_receiver(self, receiver_id: str) -> EventReceiverResponse:
        """
        Get an event receiver by ID.

        Args:
            receiver_id: The receiver ID

        Returns:
            The event receiver
        """
        response = await self._request("GET", f"/api/v1/receivers/{receiver_id}")

        if response.is_success:
            r = response.json()
            return EventReceiverResponse(
                id=r["id"],
                name=r["name"],
                receiver_type=r.get("type", r.get("receiver_type", "")),
                version=r["version"],
                description=r["description"],
                schema=r["schema"],
                fingerprint=r["fingerprint"],
                created_at=r["created_at"],
            )
        elif response.status_code == 404:
            raise XzeprNotFoundError(f"Event receiver {receiver_id} not found")
        else:
            raise XzeprApiError(response.status_code, response.text)

    async def discover_or_create_event_receiver(
        self,
        name: str,
        receiver_type: str,
        version: str,
        description: str,
        schema: Dict[str, Any],
    ) -> str:
        """
        Discover an existing event receiver by name, or create if not found.

        Args:
            name: Receiver name
            receiver_type: Type of receiver
            version: Version string
            description: Description
            schema: JSON schema for validation

        Returns:
            The receiver ID (existing or newly created)
        """
        # First, try to find existing receiver by name
        receivers = await self.list_event_receivers(name=name, receiver_type=receiver_type)

        # Check for exact match
        for receiver in receivers:
            if receiver.name == name and receiver.receiver_type == receiver_type:
                logger.info(
                    "Discovered existing event receiver %s (name: %s)",
                    receiver.id,
                    name,
                )
                return receiver.id

        # Not found, create new receiver
        logger.info("Event receiver %s not found, creating new one", name)
        return await self.create_event_receiver(
            name=name,
            receiver_type=receiver_type,
            version=version,
            description=description,
            schema=schema,
        )

    async def create_event(
        self,
        name: str,
        version: str,
        release: str,
        platform_id: str,
        package: str,
        description: str,
        payload: Dict[str, Any],
        success: bool,
        event_receiver_id: str,
    ) -> str:
        """
        Create a new event.

        Args:
            name: Event name
            version: Event version
            release: Release identifier
            platform_id: Platform identifier
            package: Package name
            description: Event description
            payload: Event payload
            success: Success status
            event_receiver_id: Associated receiver ID

        Returns:
            The created event ID
        """
        response = await self._request(
            "POST",
            "/api/v1/events",
            json={
                "name": name,
                "version": version,
                "release": release,
                "platform_id": platform_id,
                "package": package,
                "description": description,
                "payload": payload,
                "success": success,
                "event_receiver_id": event_receiver_id,
            },
        )

        if response.is_success:
            result = response.json()
            event_id = result["data"]
            logger.debug("Created event %s (name: %s)", event_id, name)
            return event_id
        else:
            logger.error(
                "Failed to create event: %s %s", response.status_code, response.text
            )
            raise XzeprApiError(response.status_code, response.text)

    async def post_work_started(
        self,
        receiver_id: str,
        work_id: str,
        work_name: str,
        version: str,
        platform_id: str,
        package: str,
        payload: Optional[Dict[str, Any]] = None,
    ) -> str:
        """
        Post a work started event.

        Args:
            receiver_id: Event receiver ID
            work_id: Unique work identifier
            work_name: Name of the work
            version: Version string
            platform_id: Platform identifier
            package: Package name
            payload: Additional payload data

        Returns:
            The created event ID
        """
        now = datetime.now(timezone.utc).isoformat()
        return await self.create_event(
            name=f"{work_name}.started",
            version=version,
            release=version,
            platform_id=platform_id,
            package=package,
            description=f"Work started: {work_name} ({work_id})",
            payload={
                "work_id": work_id,
                "status": "started",
                "started_at": now,
                "details": payload or {},
            },
            success=True,
            event_receiver_id=receiver_id,
        )

    async def post_work_completed(
        self,
        receiver_id: str,
        work_id: str,
        work_name: str,
        version: str,
        platform_id: str,
        package: str,
        success: bool,
        payload: Optional[Dict[str, Any]] = None,
    ) -> str:
        """
        Post a work completed event.

        Args:
            receiver_id: Event receiver ID
            work_id: Unique work identifier
            work_name: Name of the work
            version: Version string
            platform_id: Platform identifier
            package: Package name
            success: Whether work completed successfully
            payload: Additional payload data

        Returns:
            The created event ID
        """
        status_suffix = "completed" if success else "failed"
        now = datetime.now(timezone.utc).isoformat()
        return await self.create_event(
            name=f"{work_name}.{status_suffix}",
            version=version,
            release=version,
            platform_id=platform_id,
            package=package,
            description=f"Work {status_suffix}: {work_name} ({work_id})",
            payload={
                "work_id": work_id,
                "status": status_suffix,
                "completed_at": now,
                "success": success,
                "details": payload or {},
            },
            success=success,
            event_receiver_id=receiver_id,
        )
```

#### Task 2.5: Package Structure

Create the Python package structure.

**File**: `xzepr_consumer/__init__.py`

```python
# SPDX-FileCopyrightText: 2025 Brett Smith <xbcsmith@gmail.com>
# SPDX-License-Identifier: Apache-2.0

"""XZepr Consumer - Kafka consumer for XZepr CloudEvents messages."""

from .client import (
    XzeprClient,
    XzeprClientConfig,
    XzeprClientError,
    XzeprApiError,
    XzeprAuthError,
    XzeprNotFoundError,
)
from .config import (
    KafkaConsumerConfig,
    SaslConfig,
    SaslMechanism,
    SecurityProtocol,
    SslConfig,
)
from .consumer import MessageHandler, XzeprConsumer
from .message import (
    CloudEventData,
    CloudEventMessage,
    EventEntity,
    EventReceiverEntity,
    EventReceiverGroupEntity,
)

__all__ = [
    # Client
    "XzeprClient",
    "XzeprClientConfig",
    "XzeprClientError",
    "XzeprApiError",
    "XzeprAuthError",
    "XzeprNotFoundError",
    # Config
    "KafkaConsumerConfig",
    "SaslConfig",
    "SaslMechanism",
    "SecurityProtocol",
    "SslConfig",
    # Consumer
    "MessageHandler",
    "XzeprConsumer",
    # Message
    "CloudEventData",
    "CloudEventMessage",
    "EventEntity",
    "EventReceiverEntity",
    "EventReceiverGroupEntity",
]

__version__ = "0.1.0"
```

**File**: `pyproject.toml`

```toml
[build-system]
requires = ["setuptools>=61.0", "wheel"]
build-backend = "setuptools.build_meta"

[project]
name = "xzepr-consumer"
version = "0.1.0"
description = "Kafka consumer for XZepr CloudEvents messages"
readme = "README.md"
license = {text = "Apache-2.0"}
authors = [
    {name = "Brett Smith", email = "xbcsmith@gmail.com"}
]
requires-python = ">=3.9"
classifiers = [
    "Development Status :: 4 - Beta",
    "Intended Audience :: Developers",
    "License :: OSI Approved :: Apache Software License",
    "Programming Language :: Python :: 3",
    "Programming Language :: Python :: 3.9",
    "Programming Language :: Python :: 3.10",
    "Programming Language :: Python :: 3.11",
    "Programming Language :: Python :: 3.12",
]
dependencies = [
    "confluent-kafka>=2.0.0",
    "httpx>=0.25.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=7.0.0",
    "pytest-cov>=4.0.0",
    "responses>=0.23.0",
    "mypy>=1.0.0",
    "ruff>=0.1.0",
]

[tool.setuptools.packages.find]
where = ["."]

[tool.ruff]
line-length = 100
target-version = "py39"

[tool.ruff.lint]
select = ["E", "F", "I", "W"]

[tool.mypy]
python_version = "3.9"
strict = true

[tool.pytest.ini_options]
testpaths = ["tests"]
addopts = "-v --cov=xzepr_consumer --cov-report=term-missing"
```

#### Task 2.6: Testing Requirements

Unit tests for:

- Configuration parsing from environment variables
- CloudEvents message deserialization
- Client request building
- Error handling

Integration tests for:

- Kafka consumer with test containers
- XZepr API client with responses mock

#### Task 2.7: Deliverables

- `xzepr_consumer/__init__.py` - Package exports
- `xzepr_consumer/message.py` - CloudEvents message types
- `xzepr_consumer/config.py` - Kafka consumer configuration
- `xzepr_consumer/consumer.py` - Kafka consumer implementation
- `xzepr_consumer/client.py` - XZepr API client
- `pyproject.toml` - Package configuration

#### Task 2.8: Success Criteria

- Consumer can authenticate to Kafka using SASL/SCRAM-SHA-256
- CloudEvents messages are correctly deserialized
- Handler class allows custom message processing
- XZepr client can create/discover event receivers
- Work lifecycle events are posted correctly

---

### Phase 3: Integration Examples

#### Task 3.1: Rust Integration Example

**File**: `examples/downstream_consumer.rs`

```rust
//! Example downstream Rust service consuming XZepr events
//!
//! Run with:
//!   XZEPR_KAFKA_BROKERS=localhost:9092 \
//!   XZEPR_API_URL=http://localhost:8042 \
//!   XZEPR_API_TOKEN=your-jwt-token \
//!   cargo run --example downstream_consumer

use std::sync::Arc;
use tracing::{error, info};

// Import from your service's xzepr consumer module
use your_service::xzepr::consumer::{
    CloudEventMessage, KafkaConsumerConfig, MessageHandler, XzeprClient, XzeprConsumer,
};

/// Example message handler that processes deployment events
struct DeploymentHandler {
    client: XzeprClient,
    receiver_id: String,
    service_name: String,
}

impl DeploymentHandler {
    async fn new(client: XzeprClient, service_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Discover or create our event receiver
        let receiver_id = client
            .discover_or_create_event_receiver(
                &format!("{}-receiver", service_name),
                "worker",
                "1.0.0",
                &format!("Event receiver for {} downstream service", service_name),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "work_id": {"type": "string"},
                        "status": {"type": "string"}
                    }
                }),
            )
            .await?;

        Ok(Self {
            client,
            receiver_id,
            service_name: service_name.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl MessageHandler for DeploymentHandler {
    async fn handle(
        &self,
        message: CloudEventMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Filter for deployment events we care about
        if !message.event_type.starts_with("deployment.") {
            return Ok(());
        }

        info!(
            event_id = %message.id,
            event_type = %message.event_type,
            "Processing deployment event"
        );

        // Generate a work ID based on the incoming event
        let work_id = format!("work-{}", message.id);

        // Post work started event
        self.client
            .post_work_started(
                &self.receiver_id,
                &work_id,
                "deployment-processing",
                "1.0.0",
                &message.platform_id,
                &self.service_name,
                serde_json::json!({
                    "source_event_id": message.id,
                    "source_event_type": message.event_type,
                }),
            )
            .await?;

        // Simulate doing work
        info!(work_id = %work_id, "Starting deployment processing work");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Determine success (in real code, this would be based on actual work result)
        let success = message.success;

        // Post work completed event
        self.client
            .post_work_completed(
                &self.receiver_id,
                &work_id,
                "deployment-processing",
                "1.0.0",
                &message.platform_id,
                &self.service_name,
                success,
                serde_json::json!({
                    "source_event_id": message.id,
                    "processed_at": chrono::Utc::now().to_rfc3339(),
                }),
            )
            .await?;

        info!(
            work_id = %work_id,
            success = success,
            "Completed deployment processing"
        );

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("info,rdkafka=warn")
        .init();

    let service_name = "my-downstream-service";

    // Load Kafka consumer configuration from environment
    let kafka_config = KafkaConsumerConfig::from_env(service_name)?;

    info!(
        brokers = %kafka_config.brokers,
        topic = %kafka_config.topic,
        group_id = %kafka_config.group_id,
        "Initializing Kafka consumer"
    );

    // Create Kafka consumer
    let consumer = XzeprConsumer::new(kafka_config)?;

    // Create XZepr API client
    let client = XzeprClient::from_env()?;

    // Create handler with receiver registration
    let handler = DeploymentHandler::new(client, service_name).await?;

    info!("Starting consumer loop");

    // Run consumer
    consumer.run(Arc::new(handler)).await?;

    Ok(())
}
```

#### Task 3.2: Python Integration Example

**File**: `examples/downstream_consumer.py`

```python
#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2025 Brett Smith <xbcsmith@gmail.com>
# SPDX-License-Identifier: Apache-2.0

"""
Example downstream Python service consuming XZepr events.

Run with:
    XZEPR_KAFKA_BROKERS=localhost:9092 \
    XZEPR_API_URL=http://localhost:8042 \
    XZEPR_API_TOKEN=your-jwt-token \
    python examples/downstream_consumer.py
"""

import asyncio
import logging
import uuid
from datetime import datetime, timezone

from xzepr_consumer import (
    CloudEventMessage,
    KafkaConsumerConfig,
    MessageHandler,
    XzeprClient,
    XzeprConsumer,
)

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
)
logger = logging.getLogger(__name__)


class DeploymentHandler(MessageHandler):
    """Handler for processing deployment events."""

    def __init__(self, client: XzeprClient, receiver_id: str, service_name: str):
        self.client = client
        self.receiver_id = receiver_id
        self.service_name = service_name

    async def handle(self, message: CloudEventMessage) -> None:
        """Process a deployment event asynchronously."""
        # Filter for deployment events we care about
        if not message.event_type.startswith("deployment."):
            return

        logger.info(
            "Processing deployment event: %s (type: %s)",
            message.id,
            message.event_type,
        )

        # Generate a work ID based on the incoming event
        work_id = f"work-{message.id}"

        # Post work started event
        await self.client.post_work_started(
            receiver_id=self.receiver_id,
            work_id=work_id,
            work_name="deployment-processing",
            version="1.0.0",
            platform_id=message.platform_id,
            package=self.service_name,
            payload={
                "source_event_id": message.id,
                "source_event_type": message.event_type,
            },
        )

        # Simulate doing work
        logger.info("Starting deployment processing work: %s", work_id)
        await asyncio.sleep(2)

        # Determine success (in real code, this would be based on actual work result)
        success = message.success

        # Post work completed event
        await self.client.post_work_completed(
            receiver_id=self.receiver_id,
            work_id=work_id,
            work_name="deployment-processing",
            version="1.0.0",
            platform_id=message.platform_id,
            package=self.service_name,
            success=success,
            payload={
                "source_event_id": message.id,
                "processed_at": datetime.now(timezone.utc).isoformat(),
            },
        )

        logger.info(
            "Completed deployment processing: %s (success: %s)",
            work_id,
            success,
        )


async def main():
    """Main entry point."""
    service_name = "my-downstream-service"

    # Load Kafka consumer configuration from environment
    kafka_config = KafkaConsumerConfig.from_env(service_name)

    logger.info(
        "Initializing Kafka consumer: brokers=%s, topic=%s, group_id=%s",
        kafka_config.brokers,
        kafka_config.topic,
        kafka_config.group_id,
    )

    # Create XZepr API client with async context manager
    async with XzeprClient.from_env() as client:
        # Discover or create event receiver
        receiver_id = await client.discover_or_create_event_receiver(
            name=f"{service_name}-receiver",
            receiver_type="worker",
            version="1.0.0",
            description=f"Event receiver for {service_name} downstream service",
            schema={
                "type": "object",
                "properties": {
                    "work_id": {"type": "string"},
                    "status": {"type": "string"},
                },
            },
        )

        logger.info("Using event receiver: %s", receiver_id)

        # Create handler
        handler = DeploymentHandler(client, receiver_id, service_name)

        # Create and run consumer
        consumer = XzeprConsumer(kafka_config)

        logger.info("Starting consumer loop")

        try:
            await consumer.run(handler)
        except KeyboardInterrupt:
            logger.info("Shutting down consumer")
            consumer.stop()


if __name__ == "__main__":
    asyncio.run(main())
```

#### Task 3.3: Deliverables

- Rust example: `examples/downstream_consumer.rs`
- Python example: `examples/downstream_consumer.py`

#### Task 3.4: Success Criteria

- Examples compile/run without errors
- Examples demonstrate full workflow: consume, process, post events
- Examples include proper error handling and logging

---

### Phase 4: Documentation

#### Task 4.1: Environment Variables Reference

Create documentation for all supported environment variables.

| Variable                        | Description                  | Default                    | Required |
| ------------------------------- | ---------------------------- | -------------------------- | -------- |
| `XZEPR_KAFKA_BROKERS`           | Kafka broker addresses       | `localhost:9092`           | No       |
| `XZEPR_KAFKA_TOPIC`             | Topic to consume from        | `xzepr.dev.events`         | No       |
| `XZEPR_KAFKA_GROUP_ID`          | Consumer group ID            | `xzepr-consumer-{service}` | No       |
| `XZEPR_KAFKA_SECURITY_PROTOCOL` | Security protocol            | `PLAINTEXT`                | No       |
| `XZEPR_KAFKA_SASL_MECHANISM`    | SASL mechanism               | `SCRAM-SHA-256`            | For SASL |
| `XZEPR_KAFKA_SASL_USERNAME`     | SASL username                | -                          | For SASL |
| `XZEPR_KAFKA_SASL_PASSWORD`     | SASL password                | -                          | For SASL |
| `XZEPR_KAFKA_SSL_CA_LOCATION`   | CA certificate path          | -                          | For SSL  |
| `XZEPR_KAFKA_SSL_CERT_LOCATION` | Client certificate path      | -                          | For mTLS |
| `XZEPR_KAFKA_SSL_KEY_LOCATION`  | Client key path              | -                          | For mTLS |
| `XZEPR_API_URL`                 | XZepr API base URL           | `http://localhost:8042`    | No       |
| `XZEPR_API_TOKEN`               | JWT token for authentication | -                          | Yes      |

#### Task 4.2: Work Lifecycle Events Schema

Document the standard event types for work lifecycle:

**work.started**

```json
{
  "name": "{work_name}.started",
  "payload": {
    "work_id": "unique-work-identifier",
    "status": "started",
    "started_at": "2025-01-15T10:30:00Z",
    "details": {}
  },
  "success": true
}
```

**work.completed**

```json
{
  "name": "{work_name}.completed",
  "payload": {
    "work_id": "unique-work-identifier",
    "status": "completed",
    "completed_at": "2025-01-15T10:35:00Z",
    "success": true,
    "details": {}
  },
  "success": true
}
```

**work.failed**

```json
{
  "name": "{work_name}.failed",
  "payload": {
    "work_id": "unique-work-identifier",
    "status": "failed",
    "completed_at": "2025-01-15T10:35:00Z",
    "success": false,
    "details": {
      "error": "Error message",
      "error_code": "ERR_001"
    }
  },
  "success": false
}
```

#### Task 4.3: How-To Guide

**File**: `docs/how-to/integrate_downstream_service.md`

Create a step-by-step guide covering:

1. Setting up Kafka consumer authentication
2. Implementing message handler
3. Registering event receiver
4. Posting work lifecycle events
5. Error handling best practices
6. Testing strategies

#### Task 4.4: Deliverables

- `docs/how-to/integrate_downstream_service.md`
- `docs/reference/downstream_consumer_api.md`
- Updated `docs/explanation/cloudevents_compatibility.md`

#### Task 4.5: Success Criteria

- Documentation follows Diataxis framework
- All environment variables documented
- Code examples are complete and runnable
- Error scenarios are covered

---

## Summary

This implementation plan provides:

1. **Rust Consumer Implementation** (Phase 1)

   - Embedded consumer code with CloudEvents message types
   - Kafka consumer with SASL/SCRAM authentication
   - XZepr API client for event receiver registration and event posting
   - Work lifecycle event helpers

2. **Python Consumer Implementation** (Phase 2)

   - Python package with dataclasses for CloudEvents messages
   - Kafka consumer using confluent-kafka
   - XZepr API client using requests
   - Same work lifecycle patterns as Rust

3. **Integration Examples** (Phase 3)

   - Complete working examples for both Rust and Python
   - Demonstrates full workflow from consumption to event posting

4. **Documentation** (Phase 4)
   - Environment variable reference
   - Work lifecycle event schema
   - Step-by-step integration guide

## Dependencies

### Rust

- `rdkafka` - Kafka client
- `reqwest` - HTTP client
- `serde` / `serde_json` - Serialization
- `chrono` - Date/time handling
- `thiserror` - Error types
- `async-trait` - Async trait support
- `tracing` - Logging
- `tokio` - Async runtime

### Python

- `confluent-kafka>=2.0.0` - Kafka client
- `requests>=2.28.0` - HTTP client

## References

- [CloudEvents Specification 1.0.1](https://github.com/cloudevents/spec/blob/v1.0.1/spec.md)
- XZepr CloudEvents format: `docs/explanation/cloudevents_compatibility.md`
- XZepr Kafka authentication: `docs/explanation/kafka_sasl_scram_authentication_plan.md`
- XZepr REST API: `src/api/rest/events.rs`
