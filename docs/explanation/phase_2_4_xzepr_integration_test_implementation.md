# Phase 2.4 XzeprConsumer Integration Test Implementation

## Overview

This document explains the design and implementation of the Phase 2.4
integration test added to `tests/integration/watcher_tests.rs`. The test
exercises one full consume-execute-publish cycle via `XzeprConsumer` with a real
Kafka broker and verifies that a `WatcherResultMessage.correlation_id` survives
the entire pipeline.

## Test: `test_watch_consume_execute_publish_cycle_with_real_kafka`

### Location

`tests/integration/watcher_tests.rs` - the last test in the file, tagged
`#[ignore = "requires a running Kafka broker; set KAFKA_BROKERS env var and run with --include-ignored"]`.

### Purpose

The Phase 2.6 success criterion requires "a new non-dry-run test exercising one
full consume-execute-publish cycle." This test fulfils that requirement by
wiring together all Phase 2 components end-to-end:

```text
FutureProducer --> Kafka topic --> XzeprConsumer --> WatcherMessageHandler
    --> cloud_event_to_task adapter --> WatcherExecutor --> CapturingPublisher
```

### Design Decisions

#### Why `#[ignore]`

A live Kafka broker is required. Running Kafka in CI for every `cargo test`
invocation is expensive and fragile. Marking the test `#[ignore]` keeps it out
of the default test run while making it trivially runnable in environments where
a broker is available:

```bash
KAFKA_BROKERS=localhost:9092 cargo test -- --include-ignored \
    test_watch_consume_execute_publish_cycle_with_real_kafka
```

#### Unique topic name

The topic name is generated with a ULID suffix:

```rust
let task_topic = format!("xzardgz.test.task.{}", ulid::Ulid::new());
```

This prevents cross-test pollution when multiple test runs overlap and avoids
manual topic teardown.

#### `KafkaConsumerConfig::new` instead of `from_app_config`

`from_app_config` hard-codes `auto_offset_reset = "latest"`, which causes the
consumer to miss a message that was produced before it subscribed. The
`KafkaConsumerConfig::new` constructor defaults to `"earliest"`, so the consumer
always reads from the beginning of the (freshly created) topic and picks up the
single produced message even when it subscribes after delivery.

#### Empty `WatcherMatcher`

The default `MatcherConfig` ships with `event_types` containing
`"xzardgz.technical_review.requested"` and
`"xzardgz.security_review.requested"`. The adapter translates the inbound
`"xzardgz.technical_review.task"` CloudEvent into a `WatcherTaskMessage` whose
`event_type.as_str()` returns `"xzardgz.technical_review.task"`. These strings
do not match, so the matcher would silently drop the message.

The test clears both `event_types` and `plugins` to produce an empty matcher
that accepts all tasks:

```rust
let mut matcher_config = MatcherConfig::default();
matcher_config.event_types.clear();
matcher_config.plugins.clear();
let matcher = Arc::new(WatcherMatcher::from_config(&matcher_config));
```

#### Plugin registration

The `cloud_event_to_task` adapter derives the plugin name deterministically from
the event type:

- `"xzardgz.technical_review.task"` maps to plugin `"technical-review"`
- `"xzardgz.security_review.task"` maps to plugin `"security-review"`

The existing `make_registry_with_test_plugin` helper registers `"test-plugin"`,
which would cause a plugin-not-found failure. The test registers a dedicated
`TestPlugin` instance under the `"technical-review"` name:

```rust
let mut registry = PluginRegistry::new();
registry.register(Arc::new(TestPlugin { name: "technical-review" }));
```

`TestPlugin` returns success without making any AI API calls, keeping the test
self-contained.

#### Inline `CapturingPublisher`

The test defines a minimal `ResultPublisher` implementation inside the function
body to avoid polluting the module-level namespace. It captures every published
`WatcherResultMessage` into a `Arc<Mutex<Vec<WatcherResultMessage>>>` so the
assertion can inspect results after the consumer is cancelled by the timeout.

#### Timeout strategy

`XzeprConsumer::run` loops indefinitely. `tokio::time::timeout` cancels the
future after 10 seconds. At that point the single produced message will have
been consumed and published, so the captured vector is non-empty and the
assertion passes.

### Phase 1 field mapping verified

The correlation_id is embedded in `data.events[0].payload.correlation_id` in the
CloudEvent. The `cloud_event_to_task` adapter extracts it into
`WatcherTaskMessage.correlation_id`. The `WatcherExecutor` propagates it
unchanged to `WatcherResultMessage.correlation_id`. The test asserts:

```rust
assert_eq!(results[0].correlation_id, "integ-xzepr-corr-001", ...);
```

## Running the Test

Prerequisites:

- A running Kafka broker (e.g. via Docker:
  `docker run -p 9092:9092 apache/kafka`)
- The `KAFKA_BROKERS` environment variable set to the broker address

```bash
KAFKA_BROKERS=localhost:9092 cargo test --all-features -- \
    --include-ignored test_watch_consume_execute_publish_cycle_with_real_kafka
```

The test creates a unique topic per run; no manual cleanup is required for
correctness, though topic accumulation should be managed in long-lived
environments.
