# Phase 14: Watcher Mode and Kafka Result Publishing

## Overview

Phase 14 adds the watcher mode infrastructure to the XZardgz pipeline. The
watcher consumes CloudEvents-style task messages from a Kafka topic, routes them
through a matcher, dispatches them to the plugin registry, and publishes
structured result messages back to a Kafka result topic.

The implementation is split into five focused sub-modules within `src/watcher/`
plus an updated `src/commands/watch.rs` that wires the new components together.

---

## Module Layout

```text
src/watcher/
  mod.rs          - public API re-exports
  event_type.rs   - WatcherEventType enum + EVENT_* string constants
  task.rs         - WatcherTaskMessage (incoming CloudEvents envelope)
  result.rs       - WatcherResultMessage + FindingsSummary (outgoing result)
  matcher.rs      - WatcherMatcher (reject-by-default filter)
  publisher.rs    - ResultPublisher trait, KafkaResultPublisher,
                    PublishFailureState
  executor.rs     - WatcherExecutor (core processing loop, decoupled from I/O)
```

---

## Event Types

Four event types are defined for the first release:

| Constant                        | String                            | Kind   |
| ------------------------------- | --------------------------------- | ------ |
| `EVENT_TECHNICAL_REVIEW_TASK`   | `xzardgz.technical_review.task`   | Task   |
| `EVENT_TECHNICAL_REVIEW_RESULT` | `xzardgz.technical_review.result` | Result |
| `EVENT_SECURITY_REVIEW_TASK`    | `xzardgz.security_review.task`    | Task   |
| `EVENT_SECURITY_REVIEW_RESULT`  | `xzardgz.security_review.result`  | Result |

`WatcherEventType::from_event_str` returns `None` for any unknown string,
providing safe rejection of unrecognized event types at the edge.

---

## Watcher Task Message

`WatcherTaskMessage` is the deserialized incoming task. It follows a
CloudEvents-inspired envelope and carries:

- `id`, `spec_version`, `event_type`, `source` - CloudEvents provenance fields
- `repository`, `target_branch`, `plugin`, `plugin_config` - analysis target
- `provider`, `model` - optional AI provider overrides
- `dry_run`, `workspace_directory` - execution modifiers
- `metadata`, `requested_report_formats` - routing and output configuration
- `correlation_id`, `reply_topic_override` - reply routing

---

## Watcher Result Message

`WatcherResultMessage` is published to the Kafka result topic after each task.
It carries:

- Full provenance: `correlation_id`, `original_task_id`, `workspace_id`,
  `started_at`, `completed_at`
- Result data: `success`, `errors`, `diagnostics`, `findings_summary`,
  `risk_band`, `sarif_path`
- Workspace artifacts: `workspace_path`, `scan_artifact_path`, `report_paths`
- AI metadata: `provider_metadata`, `model_id`

`FindingsSummary` provides `total` finding count and `by_severity` breakdown
(e.g. `{"critical": 2, "high": 5}`).

---

## Matcher

`WatcherMatcher` implements reject-by-default routing:

- If **all** filter lists (`event_types`, `repositories`, `plugins`) are empty,
  **all tasks are rejected** (not accepted). This prevents accidental processing
  of every event on a shared Kafka topic.
- Non-empty lists act as OR-combined allow-lists within each dimension.
- All populated dimensions must be satisfied (AND-combined across dimensions).

```text
event_types: ["xzardgz.technical_review.task"]
plugins:     ["technical_review"]
```

This accepts tasks where both the event type AND the plugin match.

---

## Publisher

The `ResultPublisher` trait enables test-time mocking:

```rust
#[async_trait]
pub trait ResultPublisher: Send + Sync {
    async fn publish(&self, result: &WatcherResultMessage) -> Result<()>;
}
```

`KafkaResultPublisher` is the production implementation backed by
`rdkafka::producer::FutureProducer`. It reads SASL credentials from the
environment variables named in `KafkaConfig.sasl_username_env` and
`sasl_password_env`.

### Publish Failure Tracking

`PublishFailureState` persists a failed publish attempt to disk so the expensive
plugin result is not lost even if Kafka is temporarily unavailable:

- `record_failure(error)` - increments `attempts`, records `last_error` and
  `last_attempt_at`
- `persist(path)` - writes JSON to `path` (creates parent directories)
- `load(path)` - reads JSON from `path`; returns `None` when absent

`WatcherExecutor::process_task_with_publish_failure_tracking` uses this to
persist state on publish failure without losing the plugin result.

---

## Executor

`WatcherExecutor` is the core processing unit, decoupled from Kafka transport:

```rust
pub struct WatcherExecutor {
    config: Arc<Config>,
    plugin_registry: Arc<PluginRegistry>,
}
```

The `process_task` flow:

1. Record `started_at = Utc::now()`.
2. Map the task event type to the corresponding result type.
3. Call `validate_task` to check plugin registration and config.
4. If validation fails: populate `result.errors`, return failure result (not
   published from base flow).
5. If `dry_run`: add informational diagnostic, mark success, publish result.
6. Stub plugin dispatch: mark success, add phase-17 diagnostic, publish result.
7. If `result_publish_enabled`: call `publisher.publish(result)`.

Configuration accessors:

- `once_mode_enabled()` - reads `config.watcher.once`
- `max_concurrent_tasks()` - reads `config.watcher.max_concurrent_tasks`
- `result_publish_enabled()` - reads `config.watcher.result_publish_enabled`

---

## Watch Command Integration

`src/commands/watch.rs` now:

1. Applies all CLI overrides to config (brokers, topics, max_concurrent,
   no_publish, once).
2. Builds a `PluginRegistry` (empty until Phase 17).
3. Constructs a `WatcherMatcher` from `config.matcher`.
4. Constructs a `WatcherExecutor`.
5. In `--dry-run` mode: prints matcher and executor configuration summary, warns
   if matcher is empty, and returns early.
6. Otherwise: prints startup information and the Phase 17 placeholder message.

---

## Dependency Directions

```text
watcher::event_type -> (standalone: serde, std)
watcher::task       -> watcher::event_type
watcher::result     -> watcher::event_type, reports::risk_band,
                       providers::types, diagnostics
watcher::matcher    -> watcher::task, config::MatcherConfig
watcher::publisher  -> watcher::result, config::{KafkaConfig, TopicsConfig},
                       error, rdkafka
watcher::executor   -> watcher::{task, result, publisher, event_type},
                       plugins::registry, config, diagnostics, ulid
commands::watch     -> watcher::{executor, matcher}, plugins::registry, config
```

No circular dependencies are introduced.

---

## Testing Coverage

| Test category                                                               | Count |
| --------------------------------------------------------------------------- | ----- |
| `event_type` - variants, as_str, from_event_str, is_task, is_result         | 15+   |
| `task` - constructor, from_json/to_json roundtrip, rejection                | 10+   |
| `result` - constructor, success_result, with_error, FindingsSummary         | 20+   |
| `matcher` - is_empty, reject-by-default, event/repo/plugin/platform filters | 15+   |
| `publisher` - PublishFailureState CRUD, persist/load, to_json/from_json     | 15+   |
| `executor` - validate, dry_run, unknown plugin, publish tracking            | 15+   |
| `commands::watch` - dry_run, once, default, overrides                       | 7     |

---

## Success Criteria Verification

| Criterion                                                            | Status                                                                                                               |
| -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `xzardgz watch` can process technical and security review tasks      | `WatcherExecutor` dispatches both event types; stub result always succeeds                                           |
| All successful and failed watcher executions publish result messages | `process_task` publishes when `result_publish_enabled`; `process_task_with_publish_failure_tracking` tracks failures |
| Empty matcher config processes no events                             | `WatcherMatcher::matches` returns `false` when `is_empty()`                                                          |

---

## Notes on Phase 17 Integration

The executor currently returns a stub `PluginOutput` (plugin dispatch
diagnostic) rather than running the full plugin. Full `PluginContext`
construction and plugin invocation is wired in Phase 17 (Workflow Executor
Integration). The watcher executor structure and interfaces are designed to
accept a real plugin context with minimal changes.
