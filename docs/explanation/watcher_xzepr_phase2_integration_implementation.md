# Watcher XZepr Phase 2 Integration Implementation

## Overview

Phase 2 of the Watcher/XZepr Integration connects the XZepr Kafka consumer to the
watcher execution pipeline. The central artifact is `src/watcher/adapter.rs`, which
provides the `cloud_event_to_task` translation function and the `WatcherMessageHandler`
concrete implementation of the `MessageHandler` trait.

## Components Added

### `src/watcher/adapter.rs` (new file)

Implements the Phase 1 integration boundary specification:

- `AdapterError` - typed error enum covering all four Phase 1 rejection rules.
- `cloud_event_to_task` - pure function mapping a `CloudEventMessage` envelope to a
  `WatcherTaskMessage` DTO. Derives the plugin name deterministically from the event
  type; never reads a `plugin` key from the payload.
- `WatcherMessageHandler` - concrete `MessageHandler` that orchestrates the full
  consume-translate-match-execute-publish cycle. Always returns `Ok(())` so the
  consumer loop is never interrupted by individual message failures.

### `src/watcher/publisher.rs` (modified)

Added `NoOpResultPublisher`, a zero-overhead `ResultPublisher` implementation that
discards all results. Used in contexts where a concrete publisher is required by the
type system but publishing is disabled by configuration.

### `src/watcher/mod.rs` (modified)

- Declared `pub mod adapter;`.
- Added `WatcherMessageHandler` and `NoOpResultPublisher` to the module re-exports.
- Expanded the architecture table in the module-level doc comment to include the
  `adapter` component.

## Translation Rules

The `cloud_event_to_task` function enforces the following field mapping from a
`CloudEventMessage` to a `WatcherTaskMessage`:

| Target field | Source |
|---|---|
| `event_type` | Parsed from `msg.event_type` via `WatcherEventType::from_event_str` |
| `plugin` | Derived from `event_type` variant; never from payload |
| `source` | `msg.source` (envelope field) |
| `correlation_id` | `data.events[0].payload["correlation_id"]` (required) |
| `repository` | `data.events[0].payload["repository"]` (required) |
| `target_branch` | `data.events[0].payload["target_branch"]` (optional) |
| `provider` | `data.events[0].payload["provider"]` (optional) |
| `model` | `data.events[0].payload["model"]` (optional) |
| `dry_run` | `data.events[0].payload["dry_run"]` (optional, default `false`) |
| `workspace_directory` | `data.events[0].payload["workspace_directory"]` (optional) |
| `plugin_config` | `data.events[0].payload["plugin_config"]` (optional, default `null`) |
| `requested_report_formats` | `data.events[0].payload["report_formats"]` (optional array) |
| `reply_topic_override` | `data.events[0].payload["reply_topic"]` (optional) |
| `metadata` | `data.events[0].payload["metadata"]` (optional string-string object) |
| `id` | Fresh ULID generated at translation time |
| `version` | `WATCHER_TASK_VERSION` constant |
| `spec_version` | `"1.0"` |

## Rejection Rules

A `CloudEventMessage` is silently dropped (Kafka offset still committed) when any of
the following conditions apply:

1. `event_type` is unrecognised or is a result variant (`is_task()` returns `false`).
   This is the normal path for result events published by other watcher instances.
2. `data.events` is empty.
3. `payload["correlation_id"]` is absent, `null`, or an empty string.
4. `payload["repository"]` is absent, `null`, or an empty string.
5. `WatcherMatcher` does not accept the translated task (allow-list filtering).

Rejections for rule 1 are logged at `debug` level. All other rejections are logged at
`warn` level with the originating `AdapterError`.

## Handler Lifecycle

```
XZepr consumer
    |
    | CloudEventMessage
    v
WatcherMessageHandler::handle()
    |
    +-- cloud_event_to_task()
    |       |
    |       +-- UnacceptedEventType -> debug log, Ok(())
    |       +-- other AdapterError  -> warn log,  Ok(())
    |
    +-- WatcherMatcher::matches()
    |       |
    |       +-- false -> debug log, Ok(())
    |
    +-- WatcherExecutor::process_task()
    |       |
    |       +-- Err -> warn log, Ok(())
    |
    v
Ok(())
```

The handler never propagates errors to the caller. This design ensures that a single
malformed or unroutable message cannot stop the consumer loop.

## Test Coverage

`adapter.rs` includes 15 tests:

- Tests 1-12 cover all `cloud_event_to_task` acceptance and rejection paths, including
  the complete optional-fields mapping and the invariant that `plugin` is always derived
  from `event_type`.
- Test 13 is a full integration test (`test_watcher_message_handler_handle_with_valid_message_dispatches_and_publishes_correlation_id`)
  that exercises the end-to-end path from `handle()` through the executor to the
  `MockResultPublisher`, asserting the published `correlation_id` is preserved.
- Tests 14-15 cover the handler's silent-drop behaviour for result events and missing
  required fields.
