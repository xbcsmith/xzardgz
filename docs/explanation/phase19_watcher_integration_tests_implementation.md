# Phase 19 Watcher Integration Tests Implementation

## Overview

Phase 19 adds a dedicated integration test suite for the watcher executor
subsystem. The tests live in `tests/integration/watcher_tests.rs` and are wired
into the `integration` test binary via `tests/integration.rs`.

The goal is to exercise end-to-end flows through `WatcherExecutor` using real
public APIs, in-memory mocks, and temporary directories, confirming that the
executor routes tasks correctly across every significant code path without
requiring network access, Kafka brokers, or AI API keys.

## Files Added

| Path                                                                   | Purpose                                                       |
| ---------------------------------------------------------------------- | ------------------------------------------------------------- |
| `tests/integration.rs`                                                 | Test binary entry point; declares the `watcher_tests` module. |
| `tests/integration/watcher_tests.rs`                                   | Seven integration tests for `WatcherExecutor`.                |
| `docs/explanation/phase19_watcher_integration_tests_implementation.md` | This document.                                                |

## Test Infrastructure

### TestPlugin

A minimal `WorkflowPlugin` that returns `PluginOutput::success("test success")`
without calling the AI provider. Provider construction still succeeds (it builds
a `reqwest` client with no HTTP calls), and because `run` never calls
`provider.complete()`, no API key is required.

### MockPublisher

An in-memory `ResultPublisher` backed by a
`tokio::sync::Mutex<Vec<WatcherResultMessage>>`. Two constructors are provided:

- `MockPublisher::new()` -- accepts all publish calls and records the result.
- `MockPublisher::new_failing()` -- returns `PipelineError::Kafka` on every
  call.

Using `tokio::sync::Mutex` instead of `std::sync::Mutex` avoids holding a
synchronous lock guard across an async await point.

### Helper functions

- `make_test_config(publish_enabled)` -- builds a `Config` with governance
  disabled (avoids YAML-parsing `AGENTS.md`), JSON report format, and
  `watcher.once = true`.
- `make_registry_with_test_plugin()` -- registers `TestPlugin` under the name
  `"test-plugin"`.
- `make_dry_run_task(plugin, correlation_id)` -- produces a `WatcherTaskMessage`
  with `dry_run = true`.
- `make_full_execution_task(plugin, repo_path, workspace_path)` -- produces a
  `WatcherTaskMessage` with `dry_run = false` and both path fields set.
- `temp_dir()` / `path_str(dir)` -- safe wrappers around `tempfile::TempDir`.
- `make_dummy_result()` -- constructs a minimal `WatcherResultMessage` for
  persistence-only tests.

## Tests

### test_watcher_dry_run_returns_success_without_publish

Verifies that a dry-run task with `result_publish_enabled = false` returns a
success result and never calls the publisher. Exercises the validation-pass and
dry-run short-circuit paths.

### test_watcher_dry_run_publishes_when_enabled

Verifies that a dry-run task with `result_publish_enabled = true` records
exactly one publish call. Confirms that the executor gates the publish call on
the config flag.

### test_watcher_unknown_plugin_returns_failure_without_publish

Verifies that a task referencing an unregistered plugin produces a non-success
result with a non-empty `errors` list and does not invoke the publisher.
Exercises the validation-failure early-return path.

### test_watcher_process_task_with_success_plugin

Verifies that a full (non-dry-run) execution through `WorkflowExecutor` with
`TestPlugin` produces a success result. Uses two separate temp directories for
the repository and workspace. No network or AI access required.

### test_publish_failure_state_persists_and_loads

Verifies that `PublishFailureState::persist` writes a JSON file to disk and that
`PublishFailureState::load` reads it back with `attempts > 0` and the correct
`last_error`. Exercises the JSON serialization round-trip on disk.

### test_process_task_with_publish_failure_tracking_saves_state

Verifies that `process_task_with_publish_failure_tracking` returns `Ok` with a
success result even when the publisher always fails, and that a failure state
file is created at the supplied `failure_path`. Confirms that a successful
plugin run is never discarded due to a transient Kafka outage.

### test_watcher_result_has_correct_correlation_id

Verifies that `result.correlation_id` equals the `correlation_id` set on the
originating task. Exercises the provenance-propagation logic in
`build_result_for_task`.

## Design Decisions

### Governance disabled in test config

The default `GovernanceConfig` sets `rules_path` to the project root, where
`AGENTS.md` is a Markdown file. Attempting to parse it as YAML governance rules
causes a parse error. All test configs set `governance.enabled = false` and
`governance.rules_path = String::new()` to use the embedded fallback defaults
instead.

### tokio::sync::Mutex in MockPublisher

The `ResultPublisher::publish` method is `async`. Using `std::sync::Mutex`
across an await point causes a compile error in async contexts (the guard is not
`Send`). `tokio::sync::Mutex` is the correct choice here and matches the pattern
used in the executor's own unit tests.

### Separation of binary entry point from test module

Following the established pattern of `tests/unit.rs` + `tests/unit/`, the
integration tests use `tests/integration.rs` as the Cargo test binary entry
point and `tests/integration/watcher_tests.rs` as the module. This keeps the
binary entry point minimal and allows additional integration test modules to be
added later without restructuring the directory layout.
