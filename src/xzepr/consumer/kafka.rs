use rdkafka::ClientContext;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{BaseConsumer, CommitMode, Consumer, StreamConsumer};
use rdkafka::message::Message;
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
    fn pre_rebalance(
        &self,
        _base_consumer: &BaseConsumer<Self>,
        rebalance: &rdkafka::consumer::Rebalance,
    ) {
        info!("Pre-rebalance: {:?}", rebalance);
    }

    fn post_rebalance(
        &self,
        _base_consumer: &BaseConsumer<Self>,
        rebalance: &rdkafka::consumer::Rebalance,
    ) {
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
    async fn handle(
        &self,
        message: CloudEventMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
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
            .set(
                "session.timeout.ms",
                config.session_timeout.as_millis().to_string(),
            )
            .set(
                "client.id",
                format!("xzepr-consumer-{}", config.service_name),
            );

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

        let consumer: StreamConsumer<XzeprConsumerContext> =
            client_config.create_with_context(XzeprConsumerContext)?;

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
