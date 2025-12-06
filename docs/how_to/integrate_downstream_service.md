# How-To: Integrate a Downstream Service

This guide details how to implement a downstream service that consumes events from XZepr and reports work status back.

## Prerequisites

- Rust 1.75+
- Access to XZepr Kafka and API
- A valid XZepr API Token

## Step 1: Add Dependencies

Add the necessary dependencies to your `Cargo.toml`:

```bash
cargo add xzardgz
cargo add tokio --features full
cargo add async-trait
cargo add serde_json
cargo add tracing
cargo add tracing-subscriber
```

> **Note**: Ensure you point to the correct version or path of `xzardgz` if it's not yet published.

## Step 2: Define Your Handler

Create a struct to hold your client and configuration, and implement the `MessageHandler` trait.

```rust
use std::sync::Arc;
use xzardgz::xzepr::consumer::{
    CloudEventMessage, MessageHandler, XzeprClient,
};

struct MyServiceHandler {
    client: XzeprClient,
    receiver_id: String,
    service_name: String,
}

#[async_trait::async_trait]
impl MessageHandler for MyServiceHandler {
    async fn handle(
        &self,
        message: CloudEventMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 1. Filter events
        if !message.event_type.starts_with("deployment.") {
            return Ok(());
        }

        // 2. Generate Work ID
        let work_id = format!("work-{}", message.id);

        // 3. Signal Start
        self.client.post_work_started(
            &self.receiver_id,
            &work_id,
            "my-work-type",
            "1.0.0",
            &message.platform_id,
            &self.service_name,
            serde_json::json!({ "source_event": message.id }),
        ).await?;

        // 4. Do Work
        // ... perform your logic here ...
        let success = true;

        // 5. Signal Completion
        self.client.post_work_completed(
            &self.receiver_id,
            &work_id,
            "my-work-type",
            "1.0.0",
            &message.platform_id,
            &self.service_name,
            success,
            serde_json::json!({ "result": "done" }),
        ).await?;

        Ok(())
    }
}
```

## Step 3: Register and Run

In your `main` function, initialize the components and start the consumer loop.

```rust
use std::sync::Arc;
use xzardgz::xzepr::consumer::{
    KafkaConsumerConfig, XzeprClient, XzeprConsumer,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let service_name = "my-service";

    // 1. Load Config
    let kafka_config = KafkaConsumerConfig::from_env(service_name)?;
    let client = XzeprClient::from_env()?;

    // 2. Register Receiver
    let receiver_id = client.discover_or_create_event_receiver(
        "my-service-receiver",
        "worker",
        "1.0.0",
        "My downstream worker",
        serde_json::json!({"type": "object"}),
    ).await?;

    // 3. Create Consumer & Handler
    let consumer = XzeprConsumer::new(kafka_config)?;
    let handler = Arc::new(MyServiceHandler {
        client,
        receiver_id,
        service_name: service_name.into(),
    });

    // 4. Run
    consumer.run(handler).await?;

    Ok(())
}
```

## Error Handling Best Practices

- **Retries**: The `XzeprConsumer` will not automatically retry failed messages if the handler returns an error. Implement internal retries for transient failures within `handle`.
- **Dead Letters**: For unprocessable messages, log the error and return `Ok(())` to commit the offset and move on, effectively dropping the message to avoid blocking the partition.
- **Circuit Breakers**: Use circuit breakers for external service calls to prevent cascading failures.

## Testing

For integration testing:
1. Use `docker-compose` to spin up a local Kafka and XZepr instance.
2. Produce test messages to the `xzepr.dev.events` topic.
3. Assert that your service creates the expected `work.*` events using the XZepr API.
