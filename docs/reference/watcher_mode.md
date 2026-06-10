# Watcher Mode Reference

## Overview

Watcher mode is the long-running Kafka-consumer operating mode of XZardgz. In
watcher mode the process subscribes to a Kafka topic, receives CloudEvents-style
task messages, routes each message through the WatcherMatcher, dispatches
accepted tasks to the plugin registry, and publishes a structured result message
to a Kafka result topic when the task completes.

Use watcher mode when you want XZardgz to operate as a continuously-running
service that responds to externally-generated review requests, for example as
part of a CI/CD event pipeline or a security scanning workflow triggered by
repository push events.

Watcher mode is not required for local or scripted use. For those workflows use
the `run` or `scan` commands instead.

## Starting Watcher Mode

```bash
xzardgz watch --config config.yaml
```

### CLI Options

All options override the values in the configuration file for that invocation.

| Option                       | Description                                                            |
| ---------------------------- | ---------------------------------------------------------------------- |
| `--config <PATH>`            | Configuration file to load. Required.                                  |
| `--kafka-brokers <BROKERS>`  | Comma-separated broker list, overrides `kafka.brokers`.                |
| `--input-topic <TOPIC>`      | Task topic name, overrides `topics.task`.                              |
| `--output-topic <TOPIC>`     | Result topic name, overrides `topics.result`.                          |
| `--matcher <PATH>`           | Path to a standalone matcher configuration file.                       |
| `--once`                     | Process one bounded batch then exit.                                   |
| `--max-concurrent-tasks <N>` | Limit concurrent in-flight tasks.                                      |
| `--no-publish-results`       | Disable result publishing. Useful for local validation.                |
| `--dry-run`                  | Print configuration and matcher summary, then exit without connecting. |

### Dry-Run Validation

Pass `--dry-run` to validate configuration without connecting to Kafka:

```bash
xzardgz watch --config config.yaml --dry-run
```

The command prints the resolved Kafka broker list, topic names, matcher
configuration, executor settings, and a warning if the matcher is empty (which
would reject all messages).

## Watcher Configuration

The following configuration sections are relevant to watcher mode. All sections
belong in the top-level `config.yaml` file.

### `watcher` Section

Controls the watcher process itself.

```yaml
watcher:
  enabled: true
  max_concurrent_tasks: 4
  result_publish_enabled: true
  once: false
```

| Field                    | Type | Default | Description                                                             |
| ------------------------ | ---- | ------- | ----------------------------------------------------------------------- |
| `enabled`                | bool | `false` | Whether watcher mode is permitted to start.                             |
| `max_concurrent_tasks`   | int  | `2`     | Maximum number of tasks processed in parallel.                          |
| `result_publish_enabled` | bool | `true`  | Publish result messages to the result topic on completion.              |
| `once`                   | bool | `false` | Process one batch then exit. Useful for smoke tests and scheduled jobs. |

### `kafka` Section

Controls Kafka connectivity. SASL credentials are referenced by environment
variable name, never stored as literal values in the configuration file.

```yaml
kafka:
  brokers:
    - "broker1.example.com:9092"
    - "broker2.example.com:9092"
  group_id: "xzardgz-workflow-harness"
  security_protocol: "SASL_SSL"
  sasl_mechanism: "SCRAM-SHA-512"
  sasl_username_env: "KAFKA_USERNAME"
  sasl_password_env: "KAFKA_PASSWORD"
  ssl_ca_location: "/etc/ssl/certs/kafka-ca.pem"
```

| Field               | Type            | Description                                                    |
| ------------------- | --------------- | -------------------------------------------------------------- |
| `brokers`           | list of strings | One or more Kafka broker addresses in `host:port` format.      |
| `group_id`          | string          | Consumer group identifier.                                     |
| `security_protocol` | string          | One of `PLAINTEXT`, `SSL`, `SASL_PLAINTEXT`, `SASL_SSL`.       |
| `sasl_mechanism`    | string or null  | SASL mechanism, for example `SCRAM-SHA-512` or `PLAIN`.        |
| `sasl_username_env` | string or null  | Name of the environment variable that holds the SASL username. |
| `sasl_password_env` | string or null  | Name of the environment variable that holds the SASL password. |
| `ssl_ca_location`   | string or null  | Path to the CA certificate file for TLS verification.          |

### `topics` Section

Defines the Kafka topic names used by the watcher.

```yaml
topics:
  task: "xzardgz.tasks"
  result: "xzardgz.results"
```

| Field    | Type   | Description                                               |
| -------- | ------ | --------------------------------------------------------- |
| `task`   | string | Topic the watcher reads incoming task messages from.      |
| `result` | string | Topic the watcher publishes completed result messages to. |

### `matcher` Section

Defines which incoming messages the watcher accepts. See the Matcher Rules
section below for detailed semantics.

```yaml
matcher:
  event_types:
    - "xzardgz.technical_review.task"
    - "xzardgz.security_review.task"
  repositories: []
  plugins:
    - "technical-review"
    - "security-review"
  platforms: []
  metadata: {}
```

| Field          | Type                    | Description                                                           |
| -------------- | ----------------------- | --------------------------------------------------------------------- |
| `event_types`  | list of strings         | Accept only these event type strings.                                 |
| `repositories` | list of strings         | Accept only these repository identifiers. Empty means any repository. |
| `plugins`      | list of strings         | Accept only these plugin identifiers. Empty means any plugin.         |
| `platforms`    | list of strings         | Accept only these platform identifiers. Empty means any platform.     |
| `metadata`     | map of string to string | Additional key-value filters. Empty means no metadata filtering.      |

## Matcher Rules

### Reject-by-Default

The WatcherMatcher is reject-by-default. If **all** filter lists (`event_types`,
`repositories`, `plugins`, `platforms`, and `metadata`) are empty, **every
incoming message is rejected**. This prevents accidental processing of every
event on a shared Kafka topic when configuration has not been completed.

To accept any messages, at least one filter list must contain at least one
value.

### Filter Dimensions

Each populated filter list acts as an OR-combined allow-list within its
dimension:

- A message is accepted for the `event_types` dimension if its event type string
  matches **any** value in the `event_types` list.
- A message is accepted for the `repositories` dimension if its repository
  matches **any** value in the `repositories` list.
- A message is accepted for the `plugins` dimension if its plugin identifier
  matches **any** value in the `plugins` list.

An empty list for a dimension means that dimension is not filtered: any value
passes for that dimension.

### AND-Combination Across Dimensions

A message is accepted only when it satisfies **all** populated dimensions.

Example: the following matcher accepts a message only when both the event type
AND the plugin match.

```yaml
matcher:
  event_types:
    - "xzardgz.technical_review.task"
  plugins:
    - "technical-review"
```

A message with event type `xzardgz.technical_review.task` and plugin
`security-review` is rejected because the plugin dimension is not satisfied.

### Unknown Event Types

The watcher parses the `event_type` field of each incoming message using
`WatcherEventType::from_event_str`. Any unknown string returns `None` and the
message is rejected before matcher evaluation begins.

## Watcher Event Types

The following event type string constants are recognized by the watcher.

| String Value                      | Kind   | Description                                       |
| --------------------------------- | ------ | ------------------------------------------------- |
| `xzardgz.technical_review.task`   | Task   | Request a technical review of a repository.       |
| `xzardgz.technical_review.result` | Result | Published result of a completed technical review. |
| `xzardgz.security_review.task`    | Task   | Request a security review of a repository.        |
| `xzardgz.security_review.result`  | Result | Published result of a completed security review.  |

Task messages are consumed by the watcher on the input topic. Result messages
are published by the watcher to the result topic. Sending a result-kind event
type on the task topic is not a supported pattern and results in rejection.

## Health and Readiness

### Verifying Watcher Health

The watcher writes startup information to the log output including resolved
broker addresses, topic names, matcher configuration, and the number of
concurrent task slots. Monitor this output on startup to confirm the process has
connected and is ready to receive messages.

Set `RUST_LOG=info` to see normal operation messages. Set `RUST_LOG=debug` for
detailed per-message routing and publish diagnostics.

### Once Mode for Smoke Tests

Pass `--once` (or set `watcher.once: true`) to process a single bounded batch
and exit. This is useful for verifying connectivity and matcher configuration
without running a long-lived process:

```bash
xzardgz watch --config config.yaml --once
```

Once mode exits cleanly after draining the batch, making it suitable for use in
integration test pipelines and pre-deployment validation steps.

### No-Publish Mode for Local Validation

Pass `--no-publish-results` to process messages without publishing result
messages to Kafka. This allows you to validate routing and plugin dispatch
locally without needing a writable result topic.

```bash
xzardgz watch --config config.yaml --no-publish-results --once
```

## Kubernetes Deployment Notes

### Long-Running Deployment

Deploy the watcher as a long-running `Deployment` or `StatefulSet` with a single
replica per consumer group partition assignment. Set resource limits appropriate
for the number of `max_concurrent_tasks` in-flight at peak load.

```yaml
env:
  - name: KAFKA_USERNAME
    valueFrom:
      secretKeyRef:
        name: kafka-credentials
        key: username
  - name: KAFKA_PASSWORD
    valueFrom:
      secretKeyRef:
        name: kafka-credentials
        key: password
  - name: OPENAI_API_KEY
    valueFrom:
      secretKeyRef:
        name: openai-credentials
        key: api-key
  - name: RUST_LOG
    value: "info"
```

Mount the configuration file as a `ConfigMap` volume or bake it into the
container image. Do not store secrets in the `ConfigMap`; reference them through
`sasl_username_env`, `sasl_password_env`, and `api_key_env` fields that point to
environment variables populated from Kubernetes `Secret` objects.

### Liveness and Readiness

The process exits with a nonzero status on unrecoverable errors such as invalid
configuration, authentication failure, and Kafka connection failure. Configure a
liveness probe that restarts the container if the process exits, and a readiness
probe that delays traffic until the process logs a ready message.

### Scheduled Jobs with Once Mode

For workloads that should process a bounded batch on a schedule rather than run
continuously, deploy as a Kubernetes `CronJob` with `watcher.once: true` or
`--once` in the command line. The process exits cleanly after draining the
batch, which allows the `CronJob` controller to track completion status.

## Troubleshooting

### Empty Matcher Rejects All Messages

**Symptom**: The watcher starts and consumes messages but no tasks are
dispatched to plugins.

**Cause**: The matcher configuration has empty lists for all filter dimensions.

**Fix**: Add at least one entry to `matcher.event_types` in the configuration
file. For example:

```yaml
matcher:
  event_types:
    - "xzardgz.technical_review.task"
    - "xzardgz.security_review.task"
```

The `--dry-run` flag prints a warning when the matcher is empty before any
messages are consumed.

### Unknown Event Types Are Rejected

**Symptom**: Messages appear in the task topic but are not processed.

**Cause**: The `event_type` field in the message does not match any of the four
recognized event type strings.

**Fix**: Verify that the message producer is using the exact string values
listed in the Watcher Event Types table above. Event type strings are
case-sensitive and must not contain extra whitespace.

### SASL Authentication Failure

**Symptom**: The watcher exits at startup with a Kafka authentication error.

**Cause**: The environment variables named in `sasl_username_env` and
`sasl_password_env` are not set, are set to empty strings, or contain incorrect
credentials.

**Fix**: Verify the environment variables are exported in the process
environment before starting the watcher. Run `xzardgz auth status` to confirm
that provider credentials are also valid before starting a full workflow run.

### Publish Failures

**Symptom**: Plugin tasks complete but result messages are not appearing in the
result topic.

**Cause**: Kafka may be temporarily unavailable or the result topic may not
exist.

**Behavior**: The watcher persists failed publish attempts to disk using
`PublishFailureState` so that plugin results are not lost. The failure state
records the error message, the number of publish attempts, and the timestamp of
the last attempt.

**Fix**: Verify that the result topic exists and that the watcher process has
write permission on it. Check the log output for publish error details.
