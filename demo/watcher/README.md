# Watcher Demo: Kafka-Backed Workflow Execution

This demo walks through configuring and running the XZardgz watcher in
Kafka-backed event-driven mode. It has three stages:

1. **Offline dry-run** (no Kafka required): validate configuration and matcher
   rules with `--dry-run`.
2. **Live consumer loop** (requires a Kafka broker): start the watcher and
   process incoming `CloudEventMessage` task messages.
3. **Reference task messages**: the `technical_review_task.json` and
   `security_review_task.json` files show the field schema for task messages.

Run all commands from the **repository root**.

## What This Demo Shows

- How to validate watcher configuration offline with `xzardgz watch --dry-run`.
- How the matcher allow-list controls which incoming messages are dispatched.
- How `correlation_id` is threaded from the incoming task payload through to the
  published result message.
- How to publish a test task message to trigger a workflow run.

## Prerequisites

- `xzardgz` installed and on your `PATH`:

  ```bash
  cargo install --path .
  ```

- For Stages 2 and 3 only: a running Kafka broker (see below for a one-command
  local setup).

---

## Stage 1: Offline Dry-Run Validation (no Kafka required)

Validate the watcher configuration and matcher rules without connecting to
Kafka:

```bash
xzardgz watch --dry-run --config demo/watcher/config.yaml
```

Expected output:

```text
Dry run mode: watcher will validate configuration and exit.
  Matcher: 2 event types, 2 plugins configured.
  Executor: once=false, max_concurrent=2, publish=true
```

The matcher summary confirms that two event types and two plugins are
configured. If the matcher were empty the watcher would print a warning -- an
empty matcher rejects all incoming messages.

To test a configuration with an empty matcher:

```bash
xzardgz watch --dry-run --config config.example.yaml
```

---

## Stage 2: Live Consumer Loop (requires Kafka)

### Start a Local Kafka Broker

Use Docker to run a single-node Kafka cluster locally:

```bash
docker run -d \
  --name kafka-demo \
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
kafka-topics.sh \
  --bootstrap-server localhost:9092 \
  --create --topic xzardgz.demo.tasks \
  --partitions 1 --replication-factor 1

kafka-topics.sh \
  --bootstrap-server localhost:9092 \
  --create --topic xzardgz.demo.results \
  --partitions 1 --replication-factor 1
```

### Start the Watcher

```bash
xzardgz watch --config demo/watcher/config.yaml
```

Expected startup output:

```text
Kafka task topic: xzardgz.demo.tasks  result topic: xzardgz.demo.results
Watcher started: once=false, max_concurrent=2, publish=true
```

The watcher is now consuming `xzardgz.demo.tasks` and will publish results to
`xzardgz.demo.results`.

### Publish a Test Task

The watcher expects XZepr-shaped `CloudEventMessage` payloads (CloudEvents 1.0.1
envelope). The `correlation_id` must appear inside `data.events[0].payload` --
it is never read from the envelope-level `id` field.

Publish a minimal technical review task:

```bash
kafka-console-producer \
  --bootstrap-server localhost:9092 \
  --topic xzardgz.demo.tasks <<'EOF'
{"id":"01JWATCHER0000000000DEMO01","type":"xzardgz.technical_review.task","source":"demo","specversion":"1.0.1","success":true,"api_version":"1.0","name":"demo","version":"1.0.0","release":"1.0.0","platform_id":"demo","package":"demo","data":{"events":[{"id":"01JWATCHER0000000000EVT001","name":"demo","version":"1.0.0","release":"1.0.0","platform_id":"demo","package":"demo","description":"demo task","success":true,"created_at":"2024-01-01T00:00:00Z","event_receiver_id":"demo-receiver","payload":{"correlation_id":"demo-corr-001","repository":"https://github.com/pallets/jinja","target_branch":"main","provider":"openai","dry_run":true}}],"event_receivers":[],"event_receiver_groups":[]}}
EOF
```

The `dry_run: true` flag in the payload means the watcher processes the task and
publishes a result without making any AI provider calls.

### Read Results

```bash
kafka-console-consumer \
  --bootstrap-server localhost:9092 \
  --topic xzardgz.demo.results \
  --from-beginning \
  --max-messages 1
```

The result message is a `WatcherResultMessage` JSON payload. The
`correlation_id` field in the result will match `"demo-corr-001"` from the task
payload, confirming end-to-end tracing.

---

## Stage 3: Reference Task Message Schema

The `technical_review_task.json` and `security_review_task.json` files in this
directory show the inner field schema that goes inside `data.events[0].payload`
of a CloudEventMessage.

### `technical_review_task.json`

Key fields:

| Field            | Description                                                 |
| ---------------- | ----------------------------------------------------------- |
| `correlation_id` | Tracing identifier threaded to the result message.          |
| `repository`     | Repository URL or local path to analyse.                    |
| `target_branch`  | Branch to check out. Optional.                              |
| `plugin`         | Plugin identifier: `technical-review` or `security-review`. |
| `dry_run`        | Set `true` to validate without running the plugin.          |
| `provider`       | AI provider override. Optional.                             |

### `security_review_task.json`

Extends the technical review fields with:

| Field                              | Description                                                        |
| ---------------------------------- | ------------------------------------------------------------------ |
| `plugin_config.include_sarif`      | Set `true` to produce SARIF output.                                |
| `plugin_config.severity_threshold` | Minimum severity to include: `low`, `medium`, `high`, `critical`.  |
| `metadata`                         | Arbitrary key-value pairs for CI metadata (PR number, commit SHA). |

---

## Correlation ID Threading

Every workflow run -- watcher-triggered or CLI-triggered -- carries a single
`correlation_id` from trigger to final result:

1. The upstream producer places `correlation_id` inside `payload` when
   publishing to the task topic.
2. The watcher adapter extracts it from `data.events[0].payload.correlation_id`.
3. Messages missing `correlation_id` in the payload are rejected without
   dispatching a workflow run.
4. The ID is stored on `WorkspaceState` and survives `--resume`.
5. Every generated report's metadata block includes it.
6. The published `WatcherResultMessage.correlation_id` matches the original.

---

## Troubleshooting

| Symptom                                        | Cause and fix                                                                |
| ---------------------------------------------- | ---------------------------------------------------------------------------- |
| `matcher is empty, all tasks will be rejected` | Add at least one entry to `matcher.event_types` in `config.yaml`.            |
| Task message ignored silently                  | Check that `data.events[0].payload.correlation_id` is present and non-empty. |
| Connection refused to broker                   | Check `kafka.brokers` address and that the broker container is running.      |
| Result topic empty after task                  | Check `watcher.result_publish_enabled: true` and the result topic name.      |

---

## Further Reading

- [Watcher Mode Reference](../../docs/reference/watcher_mode.md)
- [Kafka Schemas Reference](../../docs/reference/kafka_schemas.md)
- [Setup Watcher Mode How-To](../../docs/how-to/setup_watcher.md)
- [Configuration Reference](../../docs/reference/configuration.md)
