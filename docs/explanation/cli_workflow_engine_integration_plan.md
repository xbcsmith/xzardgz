# CLI-to-Workflow-Engine Integration Implementation Plan

## Overview

`src/workflow/executor.rs::WorkflowExecutor` is a fully implemented, heavily
tested pipeline: it resumes-or-creates a workspace, runs governance checks,
scans the repository, executes plugin steps in dependency order, writes
reports, and assembles watcher results. Despite this, the CLI command
handlers in `src/commands/{run,scan,plugin}.rs` do not call it at all — they
print "implemented in a later phase" and return `Ok(())`. This plan wires
every non-watcher CLI entry point through `WorkflowExecutor`, and establishes
`WorkflowExecutor` as the single, sole place that owns workspace resolution,
scanning, and plugin dispatch, so no command handler (or anything else)
invokes those pieces independently.

## Current State Analysis

### Existing Infrastructure

- `WorkflowExecutor::run_plan` (`src/workflow/executor.rs:320-399`) already
  resumes via `WorkspaceManager::open` when `plan.resume` is set, or creates a
  fresh workspace via `WorkspaceManager::create` otherwise, and already loads
  a cached scan artifact instead of re-scanning when one is present.
- `WorkflowExecutor` already builds a correctly-scoped `PathValidator`/
  `ToolRegistry` per plugin step (`workflow/executor.rs:478-489`) and already
  handles dependency-ordered plugin execution with deadlock detection
  (`workflow/executor.rs:429-451`).
- `src/watcher/executor.rs::WatcherExecutor::process_task` already delegates
  correctly to `WorkflowExecutor::execute(ExecutionInput::WatcherTask(...))`,
  proving the executor's API surface already supports at least one
  non-CLI-plan invocation shape.

### Identified Issues

- `commands::run::execute` (`src/commands/run.rs:66-130`) parses/validates a
  plan or builds a direct-plugin invocation, prints a summary, and never
  calls `WorkflowExecutor`.
- `commands::scan::execute` (`src/commands/scan.rs:24-49`) only echoes
  arguments.
- `commands::plugin::run_plugin` (`src/commands/plugin.rs`) only prints a
  stub message.
- No `ExecutionInput` variant currently exists (as far as CLI wiring is
  concerned) for a scan-only run or a single ad hoc plugin invocation
  supplied via `--plugin` without a full YAML plan; this plan adds what is
  missing rather than assuming it already exists.

## Implementation Phases

### Phase 1: Extend the Executor's Invocation Surface

#### 1.1 Foundation Work

Inventory every CLI-driven invocation shape that must map onto
`WorkflowExecutor`: `run --plan <file>` (full plan), `run --plugin <name>`
(direct single-plugin run), `scan` (scan-only, no plugin execution), and
`plugin run <name>` (equivalent to direct single-plugin run via a different
CLI surface).

#### 1.2 Add Foundation Functionality

Add any missing `ExecutionInput` variants (e.g. `ExecutionInput::DirectPlugin`,
`ExecutionInput::ScanOnly`) to `src/workflow/executor.rs` so every shape above
has one first-class executor entry point.

#### 1.3 Integrate Foundation Work

Ensure `WorkflowExecutor` — not the command layer — resolves
create-vs-resume workspace state, applies governance checks, and owns
diagnostics collection for every one of these entry points. This is a
deliberate centralization: no command handler, and no future call site
outside `workflow/executor.rs`, should construct a `WorkspaceManager`,
`Scanner`, or invoke a plugin's `run()` directly.

#### 1.4 Testing Requirements

Add unit tests per new `ExecutionInput` variant, reusing the existing
`TestPlugin`/mocked-provider fixtures already present in
`workflow/executor.rs`'s test module.

#### 1.5 Deliverables

`WorkflowExecutor` exposes one execution entry point per CLI shape.

#### 1.6 Success Criteria

`grep -rn "WorkspaceManager::\|Scanner::new" src/commands/` (excluding test
modules) returns nothing — every workspace/scan operation triggered from the
CLI flows through `WorkflowExecutor`.

### Phase 2: Wire CLI Handlers to the Executor

#### 2.1 Feature Work

Replace `commands::run::execute`'s stub branches (plan mode and
direct-plugin mode) with calls into the corresponding `WorkflowExecutor`
entry point from Phase 1.

#### 2.2 Integrate Feature

Replace `commands::scan::execute` and `commands::plugin::run_plugin`
similarly. Remove every "implemented in a later phase" print statement from
these three command handlers.

#### 2.3 Configuration Updates

Ensure `--resume`, `--workspace`, `--output-dir`, `--report-format`, and
`--max-findings` CLI flags map onto `ExecutionInput`/`ConfigOverrides` fields
consumed exclusively inside `WorkflowExecutor`.

#### 2.4 Testing Requirements

Rewrite `commands::run`/`scan`/`plugin` tests — which today only assert
`Ok(())` — to assert real workspace and report side effects against a temp
directory, mirroring the test style already used in
`workflow/executor.rs`.

#### 2.5 Deliverables

All three commands produce real scan/plugin/report output when run against a
fixture repository.

#### 2.6 Success Criteria

An end-to-end CLI test (e.g. `xzardgz run --plugin security-review
<fixture-repo>`) produces a real workspace directory with a written report,
added under `tests/integration/`. Running the configured provider without
tool-calling support (see the companion agent tool-calling plan) surfaces a
clear, actionable CLI error rather than a silent no-op.
