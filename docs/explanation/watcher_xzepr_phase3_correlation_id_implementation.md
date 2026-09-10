# Phase 3: Correlation ID for Every Run

## Overview

Phase 3 of the Watcher/XZepr Integration Plan threads a single `correlation_id`
through every workflow run from trigger to final report. Before this phase, the
correlation identifier existed only on the Kafka watcher path
(`WatcherTaskMessage.correlation_id` and `WatcherResultMessage.correlation_id`).
CLI-triggered `run` and `scan` invocations had no such identifier at all, and no
run identifier appeared in persisted `WorkspaceState` or generated reports.

## What Changed

### `src/workspace/state.rs`

Added `correlation_id: String` to `WorkspaceState`:

```rust
#[serde(default)]
pub correlation_id: String,
```

The `#[serde(default)]` attribute ensures that state files written before Phase 3
(which lack the field) deserialise with an empty string rather than failing.
`WorkspaceState::new()` accepts the value as a required parameter so callers
always set it explicitly rather than relying on any post-construction mutation.

### `src/workspace/mod.rs`

Updated `WorkspaceManager::create()` to accept `correlation_id: Option<String>`.
When `None` is supplied, a fresh ULID is generated. Updated `WorkspaceManager::open()`
to accept the same `Option<String>` and forward it to the internal `create()` call
that fires when no existing workspace matches. When `open()` finds an existing
workspace, the loaded `state.correlation_id` is used unchanged (this is the
`--resume` preservation guarantee).

### `src/workflow/plan.rs`

Added `correlation_id: Option<String>` to `WorkflowPlan`:

```rust
#[serde(default)]
pub correlation_id: Option<String>,
```

CLI-supplied values arrive here before the executor creates the workspace.
Watcher-triggered runs have this field set by `watcher_task_to_plan()` directly
from `WatcherTaskMessage.correlation_id`.

### `src/workflow/executor.rs`

**`ExecutionResult`** — added `pub correlation_id: String` so every caller
of `WorkflowExecutor::execute()` can retrieve the run's identifier.

**`ExecutionInput::ScanOnly` and `PluginOnly`** — added
`correlation_id: Option<String>` to both variants so `scan` and plugin-only
invocations can receive a caller-supplied identifier.

**`run_plan()`** — correlation_id resolution logic (in priority order):

1. `plan.correlation_id` — explicit value supplied by the caller or CLI.
2. For a resumed workspace, `workspace.state.correlation_id` — the value
   persisted from the original run. This is the guarantee that
   `--resume` preserves the id.
3. For a pre-Phase-3 workspace opened via resume that has an empty
   `correlation_id` field, a fresh ULID is generated (backfill path).
4. When creating a fresh workspace with no supplied value, a fresh ULID
   is generated inside `WorkspaceManager::create()`.

**`run_watcher_task()`** — sets `plan.correlation_id = Some(task.correlation_id.clone())`
so the watcher task's identifier propagates into `run_plan()` and is persisted
on `WorkspaceState`.

**`write_step_reports()`** — sets `envelope.correlation_id` from
`workspace.state.correlation_id` so every generated report carries the identifier.

**`execute_dry_run()`**, **`run_scan_only()`**, **`run_plugin_only()`**, and
**`run_create_pr()`** — all set `correlation_id` on the returned `ExecutionResult`.

**`watcher_task_to_plan()`** — sets `correlation_id: Some(task.correlation_id.clone())`
in the constructed `WorkflowPlan` literal.

### `src/reports/envelope.rs`

Added `correlation_id: Option<String>` to `ReportEnvelope`:

```rust
#[serde(default)]
pub correlation_id: Option<String>,
```

Reports produced before Phase 3 are backwards-compatible (the field
deserialises as `None`). The executor populates it from
`workspace.state.correlation_id` when writing step reports.

### `src/cli.rs`

Added `--correlation-id` to both `RunArgs` and `ScanArgs`:

```
--correlation-id <STRING>
    Optional correlation identifier for this run.
    When omitted, a ULID is generated automatically.
```

### `src/commands/run.rs` and `src/commands/scan.rs`

Threaded `args.correlation_id` into `WorkflowPlan.correlation_id` (for plan
and direct-plugin invocations) and into `ExecutionInput::PluginOnly.correlation_id`
and `ExecutionInput::ScanOnly.correlation_id` respectively.

Added `Correlation ID: {result.correlation_id}` to `print_execution_result()`
output so operators can copy the id for downstream tracing.

## Resumption Semantics

When `--resume` is set, `WorkspaceManager::open()` finds the most recently
created workspace whose `repository_hash` matches the requested repository.
That workspace's `state.yaml` already carries `correlation_id` from the
original run. The executor reads it directly from `workspace.state.correlation_id`
without overwriting it, so the same identifier appears in all subsequent stages
of the resumed run.

For workspaces created before Phase 3 (missing the `correlation_id` field), the
executor backfills the value with either the caller-supplied id or a newly
generated ULID and persists it immediately so subsequent phases of the same
resume are consistent.

## Success Criteria Verification

| Criterion | Implementation |
| --- | --- |
| Empty or missing severity always scores `0.0` | (Phase 4 / not applicable here) |
| CLI `run` produces a non-empty `correlation_id` | `run_plan()` generates ULID when none supplied |
| `--resume` preserves the original `correlation_id` | workspace state carries it; executor reads, not overwrites |
| Same id in `WorkspaceState`, reports, and `WatcherResultMessage` | Persisted on state at creation; envelope populated from state; watcher result built from task id |
| `--correlation-id` CLI override accepted | `RunArgs` and `ScanArgs` both expose the flag |

## Testing

| Test | Location | Assertion |
| --- | --- | --- |
| `test_execute_local_plan_produces_non_empty_correlation_id` | `workflow/executor.rs` | `result.correlation_id` is non-empty for a run with no supplied id |
| `test_execute_local_plan_resume_preserves_original_correlation_id` | `workflow/executor.rs` | Second run with `resume=true` returns the same id as the first run |
| `test_execute_watcher_task_sets_correlation_id_in_result` | `workflow/executor.rs` | Existing test; watcher result carries task correlation_id |
| `test_workspace_state_load_from_legacy_yaml_without_correlation_id_field` | `workspace/state.rs` | Pre-Phase-3 state YAML deserialises with empty string |
| `test_new_creates_state_with_correct_fields` | `workspace/state.rs` | `correlation_id` is set from the constructor argument |
| CLI flag tests | `cli.rs` | `--correlation-id test-run-cid` is parsed into `RunArgs.correlation_id` |

## Quality Gate Results

All four quality gates passed:

```text
cargo fmt --all                                ok
cargo check --all-targets --all-features       ok
cargo clippy --all-targets --all-features \
  -- -D warnings                               ok
cargo test --all-features                      2375 tests passed, 0 failed
```
