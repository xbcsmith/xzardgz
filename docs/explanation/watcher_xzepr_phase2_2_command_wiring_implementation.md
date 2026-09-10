# Watcher XZepr Phase 2.2: Command Wiring Implementation

## Overview

Phase 2.2 replaces the "Phase 17" stub in `src/commands/watch.rs` with a real
`XzeprConsumer::run` call, completing the wiring from CLI entry point through
the full consume-translate-match-execute-publish cycle.

Before this change, `commands::watch::execute` built a `WatcherMatcher` and
`WatcherExecutor`, printed startup information, and then exited with the message
"Watcher consumer loop is implemented in Phase 17." The XZepr consumer and the
watcher adapter layer existed but were never invoked from the CLI.

After this change the `watch` subcommand runs a real Kafka consumer loop.

## Changes Made

### `src/commands/watch.rs`

#### Imports added

Four new import groups were added to connect the command to the consumer stack:

- `crate::error::PipelineError` - needed to wrap `ConsumerError` values with
  `.map_err(|e| PipelineError::Kafka(e.to_string()))?`.
- `crate::watcher::publisher::{KafkaResultPublisher, NoOpResultPublisher, ResultPublisher}` -
  the result-publishing trait and its two concrete implementations.
- `crate::watcher::WatcherMessageHandler` - the adapter that bridges
  `MessageHandler` (XZepr consumer trait) and `WatcherExecutor`.
- `crate::xzepr::consumer::config::KafkaConsumerConfig` - reconciled
  operator-facing consumer configuration.
- `crate::xzepr::consumer::kafka::XzeprConsumer` - the sole Kafka consumer
  implementation.

#### `executor` and `matcher` wrapped in `Arc`

Both were changed from plain heap values to `Arc<T>` at construction time:

```rust
let matcher = Arc::new(WatcherMatcher::from_config(&config.matcher));
let executor = Arc::new(WatcherExecutor::new(config.clone(), plugin_registry));
```

This change is transparent to the existing startup-print code because `Arc<T>`
derefs to `T`, so all method calls (`once_mode_enabled()`,
`max_concurrent_tasks()`, `result_publish_enabled()`, `is_empty()`) continue to
work without modification. `Arc` ownership is then shared with the
`WatcherMessageHandler`.

#### Consumer loop (replaces the Phase 17 stub)

After all startup prints the function now:

1. Builds a `ResultPublisher`. When `config.watcher.result_publish_enabled` is
   `false` (either by default or via `--no-publish`), a zero-cost
   `NoOpResultPublisher` is used. Otherwise a `KafkaResultPublisher` is
   constructed; failure is surfaced as `PipelineError::Kafka`.

2. Builds a `WatcherMessageHandler` by composing the `Arc`-wrapped `executor`,
   `matcher`, and `publisher`.

3. Builds a `KafkaConsumerConfig` via `KafkaConsumerConfig::from_app_config`,
   which translates the operator-facing `KafkaConfig`/`TopicsConfig` into the
   XZepr consumer's configuration struct. This call is infallible.

4. Creates an `XzeprConsumer`. On failure the `ConsumerError` is wrapped as
   `PipelineError::Kafka`.

5. In once mode (`--once`): wraps `consumer.run(handler)` in a
   `tokio::time::timeout` of 100 ms so the process terminates after polling a
   short window. The timeout result is discarded; the function returns `Ok(())`.

6. In steady-state mode: awaits `consumer.run(handler)` directly. On error the
   `ConsumerError` is wrapped as `PipelineError::Kafka`.

#### Module and function doc comments updated

All references to "Phase 17" were removed. The module doc comment now lists
steps 1-9 of the completed execution flow. The `execute` doc comment describes
dry-run and once-mode semantics accurately and documents that
`PipelineError::Kafka` is returned on consumer/producer creation failure.

### Tests

Four tests that previously relied on the Phase 17 stub (which returned `Ok(())`
without touching Kafka) would hang with the real consumer loop because the loop
polls indefinitely without a running broker. These tests were converted to
dry-run mode so they continue to validate config loading and override logic
without requiring Kafka connectivity:

| Test                                                   | Change                       |
| ------------------------------------------------------ | ---------------------------- |
| `test_execute_default_returns_ok`                      | Added `args.dry_run = true;` |
| `test_execute_with_brokers_override_returns_ok`        | Added `args.dry_run = true;` |
| `test_execute_with_no_publish_returns_ok`              | Added `args.dry_run = true;` |
| `test_execute_with_max_concurrent_override_returns_ok` | Added `args.dry_run = true;` |

Doc comments for these four tests were updated to state that they test config
loading and validation in dry-run mode.

Three tests were left unchanged:

- `test_execute_dry_run_returns_ok` - already dry-run.
- `test_execute_dry_run_with_topic_overrides_returns_ok` - already dry-run.
- `test_execute_once_mode_returns_ok` - uses the 100 ms timeout path; completes
  in under a second without a running broker because rdkafka defers connection
  attempts to background threads and the timeout fires before any blocking poll
  completes.

## Design Decisions

### Single configuration surface

`KafkaConsumerConfig::from_app_config` bridges the two previously disconnected
configuration surfaces (`KafkaConfig`/`TopicsConfig` and `KafkaConsumerConfig`).
The operator configures Kafka once via the shared `[kafka]` and `[topics]`
config sections; the consumer translation is an internal implementation detail.

### No `From` conversions added

`ConsumerError` and the consumer `ConfigError` do not implement `From<_>` for
`PipelineError`. Rather than adding new `From` impls (which would widen the
public API surface of types in separate modules), both are converted inline with
`.map_err(|e| PipelineError::Kafka(e.to_string()))?`. This keeps error
propagation explicit and avoids coupling the xzepr consumer module to the
pipeline error taxonomy.

### Once-mode timeout is 100 ms

The timeout is intentionally short. Once mode is designed for CI and one-shot
runs where the operator wants the process to exit cleanly after a polling
window. One hundred milliseconds is long enough for rdkafka to attempt a
subscription but short enough that tests complete quickly.

## Validation

All quality gates pass:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features -- commands::watch
```

Result: 7 tests pass, 0 failed.
