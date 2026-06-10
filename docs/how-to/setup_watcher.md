# Set Up Watcher Mode

## Problem

You want XZardgz to process repository analysis tasks from a Kafka topic
automatically, without invoking the CLI manually for each repository.

## Prerequisites

- A running Kafka cluster (local or managed)
- An OpenAI API key or a configured provider (see
  `docs/how-to/configure_providers.md`)
- XZardgz installed (`cargo install --path .`)

## Solution

### 1. Configure Kafka

Add the `kafka`, `topics`, and `watcher` sections to `config.yaml`. For a local
cluster without authentication:

```yaml
kafka:
  brokers:
    - "localhost:9092"
  group_id: "xzardgz-workflow-harness"
  security_protocol: "PLAINTEXT"
  sasl_mechanism: null
  sasl_username_env: null
  sasl_password_env: null
  ssl_ca_location: null

topics:
  task: "xzardgz.tasks"
  result: "xzardgz.results"

watcher:
  enabled: true
  max_concurrent_tasks: 2
  result_publish_enabled: true
  once: false
```

For a cloud-hosted cluster with SASL/SSL authentication, set
`security_protocol: "SASL_SSL"`, `sasl_mechanism: "PLAIN"`, and point
`sasl_username_env` and `sasl_password_env` at environment variable names rather
than literal credential values:

```yaml
kafka:
  brokers:
    - "kafka.example.com:9092"
  group_id: "xzardgz-workflow-harness"
  security_protocol: "SASL_SSL"
  sasl_mechanism: "PLAIN"
  sasl_username_env: "KAFKA_SASL_USERNAME"
  sasl_password_env: "KAFKA_SASL_PASSWORD"
```

At runtime the watcher reads the credentials from those environment variables.
The credential values are never stored in the config file.

### 2. Configure the Matcher

The matcher controls which incoming task messages are processed. An empty
matcher rejects all messages. Add at least one entry to `event_types` to enable
routing:

```yaml
matcher:
  event_types:
    - "xzardgz.technical_review.requested"
    - "xzardgz.security_review.requested"
  repositories: []
  plugins:
    - "technical-review"
    - "security-review"
  platforms: []
  metadata: {}
```

An empty `repositories` list means any repository URL is accepted. To restrict
processing to specific repositories, add their URLs to the list:

```yaml
matcher:
  event_types:
    - "xzardgz.technical_review.requested"
  repositories:
    - "https://github.com/myorg/my-repo"
```

The `plugins.enabled` list in the root configuration must include every plugin
that the matcher can route to:

```yaml
plugins:
  default: "technical-review"
  enabled:
    - "technical-review"
    - "security-review"
```

### 3. Validate Configuration

Check the configuration and matcher rules without connecting to Kafka or
consuming any messages:

```bash
xzardgz watch --dry-run --config config.yaml
```

The command prints the matcher summary and executor settings, then exits 0. It
prints a warning if the matcher is empty and would reject all tasks.

### 4. Start the Watcher

```bash
KAFKA_SASL_USERNAME=myuser KAFKA_SASL_PASSWORD=mypassword \
  xzardgz watch --config config.yaml
```

The watcher subscribes to the topic named in `topics.task` and begins processing
incoming messages. Results are published to the topic named in `topics.result`
after each task completes.

For a local cluster without SASL, the credential environment variables are not
needed:

```bash
xzardgz watch --config config.yaml
```

### 5. Send a Test Task

Publish a minimal task message to the Kafka task topic using your preferred
Kafka client. A reference example is at
`examples/watcher/technical_review_task.json`:

```json
{
  "id": "01JTEST00000000000000TECH1",
  "spec_version": "1.0",
  "event_type": "xzardgz.technical_review.requested",
  "source": "xzardgz/examples",
  "repository": "https://github.com/example/my-repo",
  "target_branch": "main",
  "plugin": "technical-review",
  "plugin_config": {},
  "provider": null,
  "model": null,
  "dry_run": false,
  "workspace_directory": null,
  "metadata": {},
  "requested_report_formats": ["markdown", "json"],
  "correlation_id": null,
  "reply_topic_override": null
}
```

Publish this JSON to the topic named in `topics.task` (default:
`xzardgz.tasks`). The `event_type` must match a value in `matcher.event_types`
for the message to be accepted.

### 6. Monitor Results

Watch log output at the `info` level to observe task dispatch and completion:

```bash
RUST_LOG=info xzardgz watch --config config.yaml
```

Read processed results from the topic named in `topics.result` (default:
`xzardgz.results`). Each result message is a JSON payload containing:

- `success` - whether the plugin run completed without errors
- `findings_summary` - total finding count and a breakdown by severity
- `report_paths` - paths to the written report files in the workspace
- `correlation_id` - echoes the `correlation_id` from the original task
- `errors` and `diagnostics` - failure details when `success` is false

## Configuration Example

Complete watcher configuration sections for a cloud-hosted Kafka cluster:

```yaml
kafka:
  brokers:
    - "kafka.example.com:9092"
  group_id: "xzardgz-workflow-harness"
  security_protocol: "SASL_SSL"
  sasl_mechanism: "PLAIN"
  sasl_username_env: "KAFKA_SASL_USERNAME"
  sasl_password_env: "KAFKA_SASL_PASSWORD"
  ssl_ca_location: null

topics:
  task: "xzardgz.tasks"
  result: "xzardgz.results"

matcher:
  event_types:
    - "xzardgz.technical_review.requested"
    - "xzardgz.security_review.requested"
  repositories: []
  plugins:
    - "technical-review"
    - "security-review"
  platforms: []
  metadata: {}

watcher:
  enabled: true
  max_concurrent_tasks: 2
  result_publish_enabled: true
  once: false
```

## Matcher Rules

The matcher applies a logical AND across all populated dimensions. Within each
dimension, the values are OR-combined: a task passes a dimension if it matches
any value in that dimension's list.

| Dimension      | Empty behaviour               | Non-empty behaviour                              |
| -------------- | ----------------------------- | ------------------------------------------------ |
| `event_types`  | Reject all (if all are empty) | Accept if the task `event_type` is in the list   |
| `repositories` | Accept any repository         | Accept only if the repository URL is in the list |
| `plugins`      | Accept any plugin             | Accept only if the plugin name is in the list    |
| `platforms`    | Accept any platform           | Accept only if the platform value is in the list |

When `event_types`, `repositories`, and `plugins` are all empty the matcher is
considered empty and every message is rejected. This is a safety feature that
prevents accidental processing of all events on a shared Kafka topic. At least
one `event_types` entry is required to enable any routing.

## Once Mode

Set `watcher.once: true` in `config.yaml` or pass `--once` on the command line
to process one batch of messages and exit:

```bash
xzardgz watch --once --config config.yaml
```

Once mode is useful for:

- Scheduled batch runs via a cron job or Kubernetes `CronJob`
- Smoke testing the watcher configuration in CI
- Bounded processing in environments where long-running daemon processes are not
  permitted

## Troubleshooting

| Symptom                                        | Cause and fix                                                                     |
| ---------------------------------------------- | --------------------------------------------------------------------------------- |
| `matcher is empty, all tasks will be rejected` | Add at least one entry to `matcher.event_types` in `config.yaml`                  |
| Connection refused to Kafka broker             | Check `kafka.brokers` addresses and `kafka.security_protocol`                     |
| SASL authentication failure                    | Verify `sasl_username_env` and `sasl_password_env` name the correct env vars      |
| Task rejected: unknown plugin                  | Add the plugin name to `plugins.enabled` and `matcher.plugins`                    |
| No results published to result topic           | Check `watcher.result_publish_enabled: true` and the result topic name            |
| Tasks received but immediately rejected        | The `event_type` in the message does not match any entry in `matcher.event_types` |
