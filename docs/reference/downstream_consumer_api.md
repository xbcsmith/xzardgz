# Downstream Consumer API Reference

This document provides reference information for implementing downstream services that utilize the XZepr consumer library.

## Environment Variables

The following environment variables are used to configure the XZepr consumer and client.

| Variable | Description | Default | Required |
| :--- | :--- | :--- | :--- |
| `XZEPR_KAFKA_BROKERS` | Kafka broker addresses | `localhost:9092` | No |
| `XZEPR_KAFKA_TOPIC` | Topic to consume from | `xzepr.dev.events` | No |
| `XZEPR_KAFKA_GROUP_ID` | Consumer group ID | `xzepr-consumer-{service}` | No |
| `XZEPR_KAFKA_SECURITY_PROTOCOL` | Security protocol | `PLAINTEXT` | No |
| `XZEPR_KAFKA_SASL_MECHANISM` | SASL mechanism | `SCRAM-SHA-256` | For SASL |
| `XZEPR_KAFKA_SASL_USERNAME` | SASL username | - | For SASL |
| `XZEPR_KAFKA_SASL_PASSWORD` | SASL password | - | For SASL |
| `XZEPR_KAFKA_SSL_CA_LOCATION` | CA certificate path | - | For SSL |
| `XZEPR_KAFKA_SSL_CERT_LOCATION` | Client certificate path | - | For mTLS |
| `XZEPR_KAFKA_SSL_KEY_LOCATION` | Client key path | - | For mTLS |
| `XZEPR_API_URL` | XZepr API base URL | `http://localhost:8042` | No |
| `XZEPR_API_TOKEN` | JWT token for authentication | - | Yes |

## Work Lifecycle Events

The standard event types for tracking work lifecycle.

### `work.started`

Emitted when a service begins processing a task.

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

### `work.completed`

Emitted when a service successfully completes a task.

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

### `work.failed`

Emitted when a service fails to complete a task.

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

## Rust Module Reference

### `xzardgz::xzepr::consumer::config`

- **`KafkaConsumerConfig`**: Main configuration struct.
- **`SecurityProtocol`**: Enum for Kafka security protocols (`Plaintext`, `Ssl`, `SaslPlaintext`, `SaslSsl`).
- **`SaslMechanism`**: Enum for SASL mechanisms (`Plain`, `ScramSha256`, `ScramSha512`).

### `xzardgz::xzepr::consumer::client`

- **`XzeprClient`**: Async HTTP client for XZepr API.
  - `discover_or_create_event_receiver`: Auto-registers the service as an event receiver.
  - `post_work_started`: Helper to send `work.started` events.
  - `post_work_completed`: Helper to send `work.completed` or `work.failed` events.

### `xzardgz::xzepr::consumer::kafka`

- **`XzeprConsumer`**: High-level Kafka consumer.
- **`MessageHandler`**: Trait to implement for processing logic.
