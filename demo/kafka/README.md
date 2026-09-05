# Kafka Configuration Example

`kafka_config.yaml` contains annotated configuration snippets for XZardgz
watcher mode. Copy the relevant sections into your `config.yaml`.

## What Is Included

The file contains four configuration sections:

| Section   | Purpose                                                     |
| --------- | ----------------------------------------------------------- |
| `kafka`   | Kafka broker connectivity and authentication.               |
| `topics`  | Input task topic and output result topic names.             |
| `matcher` | Rules that control which task messages the watcher accepts. |
| `watcher` | Concurrency, result publishing, and once-mode settings.     |

## How to Use

1. Open `examples/kafka/kafka_config.yaml` and identify the sections you need.
2. Copy them into your `config.yaml`, merging with any existing content.
3. Adjust the values for your environment (see below).
4. Validate the configuration with a dry run:

```bash
xzardgz watch --dry-run --config config.yaml
```

## Development Setup (PLAINTEXT)

The active `kafka` section in the file targets a local broker with no
authentication:

```yaml
kafka:
  brokers:
    - "localhost:9092"
  group_id: "xzardgz-workflow-harness"
  security_protocol: "PLAINTEXT"
```

This is suitable for local development with a broker started via Docker Compose
or a similar tool.

## Production Setup (SASL/SSL)

A commented-out block shows a production configuration using SASL/SSL:

```yaml
# kafka:
#   brokers:
#     - "kafka-broker-1.internal:9093"
#   security_protocol: "SASL_SSL"
#   sasl_mechanism: "PLAIN"
#   sasl_username_env: "KAFKA_SASL_USERNAME"
#   sasl_password_env: "KAFKA_SASL_PASSWORD"
#   ssl_ca_location: "/etc/ssl/certs/ca-bundle.crt"
```

Uncomment and fill in the broker addresses. Set the environment variables named
by `sasl_username_env` and `sasl_password_env` in the shell or container where
the watcher runs. Never place credential values directly in the configuration
file.

## Matcher Configuration

The `matcher` section controls which messages the watcher accepts:

```yaml
matcher:
  event_types:
    - "xzardgz.technical_review.task"
    - "xzardgz.security_review.task"
  plugins:
    - "technical-review"
    - "security-review"
```

An empty `matcher` (all lists empty) rejects all messages. Add at least one
`event_type` to allow any messages through. The `repositories` list can be used
to restrict processing to specific repositories; leave it empty to accept all.

## Supported Security Protocols

| Value            | When to Use                                                       |
| ---------------- | ----------------------------------------------------------------- |
| `PLAINTEXT`      | Local development, internal networks without encryption.          |
| `SSL`            | Encrypted connections without SASL authentication.                |
| `SASL_PLAINTEXT` | SASL authentication without TLS (not recommended for production). |
| `SASL_SSL`       | SASL authentication over TLS. Recommended for production.         |

## Starting a Local Kafka Broker

For local development, start a single-node Kafka cluster with Docker:

```bash
docker run -d \
  --name kafka \
  -p 9092:9092 \
  -e KAFKA_ENABLE_KRAFT=yes \
  -e KAFKA_CFG_NODE_ID=1 \
  -e KAFKA_CFG_PROCESS_ROLES=broker,controller \
  -e KAFKA_CFG_LISTENERS=PLAINTEXT://:9092,CONTROLLER://:9093 \
  -e KAFKA_CFG_ADVERTISED_LISTENERS=PLAINTEXT://localhost:9092 \
  -e KAFKA_CFG_CONTROLLER_QUORUM_VOTERS=1@localhost:9093 \
  -e KAFKA_CFG_CONTROLLER_LISTENER_NAMES=CONTROLLER \
  bitnami/kafka:latest
```

Create the task and result topics:

```bash
kafka-topics.sh --bootstrap-server localhost:9092 \
  --create --topic xzardgz.tasks --partitions 1 --replication-factor 1

kafka-topics.sh --bootstrap-server localhost:9092 \
  --create --topic xzardgz.results --partitions 1 --replication-factor 1
```

## Further Reading

- [Watcher Mode Reference](../../docs/reference/watcher_mode.md)
- [Kafka Schemas Reference](../../docs/reference/kafka_schemas.md)
- [Setup Watcher Mode How-To](../../docs/how-to/setup_watcher.md)
- [Configuration Reference](../../docs/reference/configuration.md)
