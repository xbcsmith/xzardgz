//! Example downstream Rust service consuming XZepr events
//!
//! Run with:
//!   XZEPR_KAFKA_BROKERS=localhost:9092 \
//!   XZEPR_API_URL=http://localhost:8042 \
//!   XZEPR_API_TOKEN=your-jwt-token \
//!   cargo run --example downstream_consumer

use std::sync::Arc;
use tracing::info;
use xzardgz::xzepr::consumer::{
    CloudEventMessage, KafkaConsumerConfig, MessageHandler, XzeprClient, XzeprConsumer,
};

/// Example message handler that processes deployment events
struct DeploymentHandler {
    client: XzeprClient,
    receiver_id: String,
    service_name: String,
}

impl DeploymentHandler {
    async fn new(
        client: XzeprClient,
        service_name: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
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
