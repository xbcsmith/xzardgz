# CLI-to-Workflow-Engine Integration: Phase 1 Implementation

This document records what was actually built for Phase 1 ("Extend the
Executor's Invocation Surface") of
[`cli_workflow_engine_integration_plan.md`](cli_workflow_engine_integration_plan.md),
and where the resulting design diverged from that plan's original assumptions
once the real state of the codebase was checked.

## Summary

Phase 1's stated goal was to give `WorkflowExecutor` one first-class
`ExecutionInput` entry point per CLI-driven invocation shape: `run --plan`,
`run --plugin`, `scan`, and `plugin run <name>`. Before writing any code, the
actual state of `src/workflow/executor.rs` and `src/workflow/plan.rs` was
re-verified against the plan document, since the plan's "Identified Issues"
section had been written against an earlier snapshot of the codebase.

That re-check found that two of the four shapes were already fully implemented:

- `run --plan <file>` -> `ExecutionInput::LocalPlan`, already complete.
- `run --plugin <name>` -> already covered by
  `WorkflowPlan::direct_plugin_invocation`/`build_direct_invocation_plan`
  synthesizing a single-step plan and running it through the same `LocalPlan`
  path. No separate `DirectPlugin` variant was needed or added.
- `scan` -> `ExecutionInput::ScanOnly` already existed, but with a real gap: it
  always called `WorkspaceManager::create` (no resume support) and never ran
  governance checks, unlike `run_plan`.
- `plugin run <name>` -> nothing existed. Its own doc comment
  (`PluginRunArgs`/`PluginCommands::Run`) describes it as running "against a
  workspace directory or existing scan artifact, bypassing the full run
  pipeline" -- a genuinely different shape from `run --plugin`, since it skips
  repository resolution and scanning entirely and operates on
  previously-collected scan data instead.

So the actual Phase 1 work was: (1) bring `ScanOnly` to governance/resume parity
with `LocalPlan`, and (2) add the missing fourth shape as a new
`ExecutionInput::PluginOnly` variant.

## Changes

### `ExecutionInput::ScanOnly`

Added `branch: Option<String>`, `resume: bool`, and `workspace: Option<String>`
fields. `run_scan_only` now runs
`GovernanceChecker::check_workflow_inputs`/`check_workspace_path` before any
workspace or filesystem I/O, and opens an existing workspace via
`WorkspaceManager::open` (loading its recorded scan artifact when present)
instead of always creating a fresh one, mirroring `run_plan`'s existing resume
logic exactly.

### `ExecutionInput::PluginOnly` (new)

Runs a single plugin against previously-collected scan data with no repository
resolution, git metadata collection, or scanning. Backs the
`plugin run --workspace <dir>` / `plugin run --scan-artifact <path>` CLI shape.

Two scan-data sources are supported:

- `workspace_dir: Option<String>` -- an existing workspace directory
  (`{workspace_root}/{workspace_id}`). There is no existing `WorkspaceManager`
  constructor that resolves a single combined directory path into a root+id
  pair, so `run_plugin_only` splits it itself (`Path::parent()` /
  `Path::file_name()`) before calling `WorkspaceManager::load`. When the resumed
  workspace recorded a `local_repository_path`, the plugin's sandbox read root
  includes it, restoring the same source-tree access a normal resumed run would
  have.
- `scan_artifact_path: Option<String>` -- an external scan-artifact YAML file,
  read directly via `ScanResult::load_from_str`. When used without
  `workspace_dir`, a fresh, ephemeral workspace is created under
  `workspace_root` purely to host reports and sandbox state; the plugin has no
  read access to the original repository checkout in this mode, since its
  location is not recorded anywhere. This is a deliberate, documented
  limitation, not an oversight -- there is nowhere else the original checkout
  path could come from.

Exactly one of the two must be supplied; supplying neither returns
`PipelineError::Workflow` before any I/O occurs. When both are supplied,
`scan_artifact_path` takes precedence over the workspace's own recorded
artifact.

Internally, `run_plugin_only` synthesizes a single-step `WorkflowPlan` via
`WorkflowPlan::direct_plugin_invocation` purely so it can reuse the existing
plan-shaped helpers (`execute_dry_run`, `get_report_formats_for_step`,
`write_step_reports`) unchanged, then follows the same governance-check ->
sandbox-build -> plugin-execution -> report-writing sequence as `run_plan`'s
per-step loop, minus the scan and git stages.

### Supporting changes

- `ExecutionResult` gained `#[derive(Debug)]` (required by a test's
  `Result::unwrap_err()` call; it was a pre-existing gap unrelated to any
  specific new field).
- `tests/integration/workflow_tests.rs`'s existing `ExecutionInput::ScanOnly`
  literal was updated for the new fields.

## What is explicitly out of scope

Per the plan document, Phase 1 covers only the executor's invocation surface.
`src/commands/run.rs`, `src/commands/scan.rs`, and `src/commands/plugin.rs` are
unchanged and remain stubs that print "implemented in a later phase" -- wiring
them to call `WorkflowExecutor::execute` with these `ExecutionInput` variants is
Phase 2 ("Wire CLI Handlers to the Executor"), a separate piece of work.

## Testing

Six new tests were added to `src/workflow/executor.rs`'s existing test module,
reusing its established fixtures (`SuccessPlugin`, `make_test_config`,
`make_executor_with_success_plugin`, `make_test_plan`):

- `test_execute_scan_only_resume_loads_existing_scan_artifact`
- `test_execute_scan_only_rejects_invalid_workspace_path_under_governance`
- `test_execute_plugin_only_rejects_when_neither_source_given`
- `test_execute_plugin_only_from_scan_artifact_path_returns_success`
- `test_execute_plugin_only_from_workspace_dir_returns_success`
- `test_execute_plugin_only_dry_run_skips_plugin_execution`

The governance-rejection test's assumption (that an empty `rules_path` with
`enabled = true` still loads embedded default rules, and that a `../`-containing
workspace path trips the workspace no-traversal rule) was verified against
`src/governance/mod.rs` and confirmed by the test actually passing, not assumed.

## Verification

Full quality-gate sequence run in the mandated order, all clean:

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Result: 1437 lib unit tests, 79 workspace integration tests, 26 + other
integration suites, and 380 doctests all passed, 0 failed. Phase 1's literal
success criterion also holds:

```text
grep -rn "WorkspaceManager::\|Scanner::new" src/commands/
```

returns no matches.
