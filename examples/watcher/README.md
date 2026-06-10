# Watcher Task Examples

These JSON files are example `WatcherTaskMessage` payloads. Publish them to your
watcher input Kafka topic to test watcher mode without a live CI system.

## Files

### `technical_review_task.json`

A task message that requests a technical review of a repository. The watcher
will consume this message, run the `technical-review` plugin, and publish a
result message to the configured result topic.

### `security_review_task.json`

A task message that requests a security review of a repository. Includes
`plugin_config` fields to enable SARIF output and sets a low severity threshold
to surface the broadest set of findings.

## Publishing a Task Message

Use any Kafka producer to publish a message. With the `kafka-console-producer`
CLI:

```bash
kafka-console-producer \
  --bootstrap-server localhost:9092 \
  --topic xzardgz.tasks \
  < examples/watcher/technical_review_task.json
```

With `kcat` (formerly `kafkacat`):

```bash
kcat -P \
  -b localhost:9092 \
  -t xzardgz.tasks \
  examples/watcher/technical_review_task.json
```

After publishing, the watcher process will log acceptance or rejection of the
message based on its matcher configuration.

## Message Fields

| Field                      | Description                                                              |
| -------------------------- | ------------------------------------------------------------------------ |
| `id`                       | Unique message identifier (ULID format).                                 |
| `spec_version`             | Schema version. Use `"1"`.                                               |
| `event_type`               | Determines which plugin the watcher dispatches. See event types below.   |
| `source`                   | Identifies the system that produced the message.                         |
| `repository`               | Repository URL or local path to analyse.                                 |
| `target_branch`            | Branch to check out. Optional.                                           |
| `plugin`                   | Plugin identifier: `technical-review` or `security-review`.              |
| `plugin_config`            | Plugin-specific configuration. Overrides the values in `config.yaml`.    |
| `provider`                 | AI provider override. Optional.                                          |
| `model`                    | Model override. Optional.                                                |
| `dry_run`                  | Set `true` to validate without running the plugin or publishing results. |
| `metadata`                 | Arbitrary key-value pairs for routing or tracing (e.g. PR number).       |
| `requested_report_formats` | Report formats to produce. Optional.                                     |
| `correlation_id`           | Identifier carried through to the result message for tracing. Optional.  |
| `reply_topic_override`     | Publish the result to a different topic than the default. Optional.      |

## Event Types

| Event Type String               | Plugin Dispatched  |
| ------------------------------- | ------------------ |
| `xzardgz.technical_review.task` | `technical-review` |
| `xzardgz.security_review.task`  | `security-review`  |

The matcher in `config.yaml` must include the event type string in
`matcher.event_types` or the message will be rejected.

## Adjusting for Your Environment

Before publishing, update:

- `repository` to point at an accessible repository URL.
- `provider` and `model` if you want to override the defaults in `config.yaml`.
- `metadata` with any tracking identifiers relevant to your workflow.
- `correlation_id` with a unique value if you need to correlate the result
  message back to the originating event.

## Dry Run Testing

Set `"dry_run": true` in the task message to have the watcher process the
message, validate it, and publish a result without actually running the plugin
or making any provider calls. Useful for verifying matcher configuration and
message routing.

## Further Reading

- [Watcher Mode Reference](../../docs/reference/watcher_mode.md)
- [Kafka Schemas Reference](../../docs/reference/kafka_schemas.md)
- [Setup Watcher Mode How-To](../../docs/how-to/setup_watcher.md)
