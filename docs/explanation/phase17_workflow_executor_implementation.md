# Phase 17: Workflow Executor Integration

## Overview

Phase 17 introduces the shared workflow execution engine (`WorkflowExecutor`)
that both CLI commands and watcher-triggered tasks use. It wires together all
pipeline stages implemented in previous phases into a single, coherent execution
path. Both the `xzardgz run` CLI command and the `WatcherExecutor` message
handler now call through the same `WorkflowExecutor::execute` entry point.

---

## Key Types

### `ExecutionInput`

An enum that describes the mode of a single pipeline execution. Three variants
are supported:

| Variant       | Description                                              |
| ------------- | -------------------------------------------------------- |
| `LocalPlan`   | Full workflow plan from a plan file or constructed value |
| `WatcherTask` | Triggered by a Kafka watcher task message                |
| `ScanOnly`    | Repository scan without plugin execution                 |

```rust
pub enum ExecutionInput {
    LocalPlan(Box<WorkflowPlan>),
    WatcherTask(Box<WatcherTaskMessage>),
    ScanOnly { repository: String, output_path: Option<String> },
}
```

### `ExecutionResult`

The structured result returned by every `WorkflowExecutor::execute` call.
Consumers check `success` first, then inspect `errors` or `diagnostics` for
detail.

| Field                 | Description                                            |
| --------------------- | ------------------------------------------------------ |
| `workspace_id`        | ULID of the workspace created or resumed for this run  |
| `success`             | `true` when all pipeline stages completed successfully |
| `errors`              | Error messages accumulated during execution            |
| `diagnostics`         | Informational and warning diagnostics                  |
| `scan_artifact_path`  | Path to the persisted scan artifact YAML file          |
| `report_paths`        | Map of step ID to list of written report file paths    |
| `watcher_result`      | Populated for watcher task executions                  |
| `stage_at_completion` | `WorkspaceStage` at the end of execution               |
| `started_at`          | UTC timestamp when execution started                   |
| `completed_at`        | UTC timestamp when execution completed                 |
| `is_dry_run`          | `true` when this was a validation-only run             |

### `WorkflowExecutor`

The central execution engine. It owns an `Arc<Config>` and an
`Arc<PluginRegistry>` and exposes a single async entry point:

```rust
pub async fn execute(&self, input: ExecutionInput) -> Result<ExecutionResult>
```

---

## Execution Stages

A complete `LocalPlan` or `WatcherTask` run progresses through these stages:

1. **Apply config overrides** from the plan (provider, model, workspace fields).
2. **Validate governance** rules (workspace path, plugin names, branch name) via
   `GovernanceChecker`.
3. **Initialize or resume workspace** via `WorkspaceManager::create` or
   `WorkspaceManager::open`.
4. **Dry-run short-circuit**: validate without side effects and return
   immediately if `plan.is_dry_run()` is `true`.
5. **Resolve repository** to a local filesystem path. Phase 17 supports local
   directories only. A clear error is returned for remote URLs.
6. **Git preparation**: open the repository with `GitRepository::open` and
   collect metadata. If the path is not a git repository the stage is skipped
   gracefully.
7. **Scan stage**: run `Scanner::new(config).scan()` or load an existing scan
   artifact when resuming.
8. **Persist scan artifact** to `workspace/scan/artifact.yaml`.
9. **For each plugin step** (in dependency order): a. Transition workspace to
   `WorkspaceStage::PluginRunning`. b. Create provider via
   `ProviderFactory::create_from_config`. c. Build `PluginContext` with config,
   workspace, scan result, provider, tool registry, and governance checker. d.
   Execute the plugin: `plugin.run(ctx).await`. e. Record plugin output and
   numeric score to workspace state. f. Write reports in configured formats to
   `workspace/reports/<step_id>/`. g. Transition workspace to
   `WorkspaceStage::ReportComplete` then `WorkspaceStage::PluginComplete`.
10. **Transition workspace** to `WorkspaceStage::Complete`.
11. **Build `WatcherResultMessage`** if input was a `WatcherTask`.
12. **Return `ExecutionResult`**.

---

## Dry Run Behavior

When `plan.is_dry_run()` returns `true` the executor:

- Creates the workspace normally.
- Validates that the repository path exists locally.
- Validates provider config by constructing the provider object (no API calls).
- Validates each step's plugin is registered via
  `PluginRegistry::validate_plugin_config`.
- Returns `ExecutionResult` with `is_dry_run: true` and `success: true`
  (assuming all validations pass).
- Does NOT run the scanner, call any AI provider, write any reports, or publish
  Kafka messages.

---

## Resume Support

When `plan.resume` is `true`:

- `WorkspaceManager::open` is used to find and load the most recent workspace
  for that repository URL (by SHA-256 hash). A new workspace is created when
  none exists.
- If a scan artifact already exists in the loaded workspace it is deserialized
  with `ScanResult::load_from_str` instead of re-scanning. If loading fails the
  executor falls back to a fresh scan automatically.

Phase 17 does not implement step-level resume: plugin steps that already have
outputs in the workspace state are still re-executed.

---

## Watcher Integration

The `WatcherExecutor` now delegates real plugin execution to `WorkflowExecutor`:

1. `WatcherExecutor` validates the incoming `WatcherTaskMessage` (plugin
   registered and enabled) and short-circuits for dry runs.
2. For real execution it calls
   `workflow_executor.execute(ExecutionInput::WatcherTask(task))`.
3. `WorkflowExecutor::run_watcher_task` converts the `WatcherTaskMessage` to a
   single-step `WorkflowPlan` via `watcher_task_to_plan`, runs `run_plan`, and
   attaches a `WatcherResultMessage` built from the `ExecutionResult`.
4. `WatcherExecutor` publishes the result message if publishing is enabled.

Both code paths (CLI plan execution and watcher task execution) now share the
same pipeline engine, satisfying the Phase 17 requirement.

---

## Report Writing

For each completed plugin step:

- Report formats are resolved in priority order: step config, plan config,
  global `config.reports.formats`.
- Each recognized format (`json`, `markdown`, `sarif`) is written to
  `workspace/reports/<step_id>/<plugin_name>.<ext>`.
- A `ReportEnvelope` is built from the plugin output findings, repository
  metadata, and workspace ID.
- Written paths are recorded in `WorkspaceState::report_paths` and returned in
  `ExecutionResult::report_paths`.

---

## Repository Support

Phase 17 supports local directory paths only. When a remote URL is passed as the
repository value the executor returns `PipelineError::Workflow` with a message
indicating that remote URL cloning is not yet implemented.

---

## Testing

The following test scenarios are covered in `src/workflow/executor.rs`:

| Test name                                                            | Scenario                                        |
| -------------------------------------------------------------------- | ----------------------------------------------- |
| `test_workflow_executor_new_creates_executor`                        | Constructor does not panic                      |
| `test_execute_local_plan_with_success_plugin_returns_success_result` | Full pipeline succeeds end-to-end               |
| `test_execute_local_plan_with_failure_output_records_workspace_id`   | Graceful plugin failure still produces a result |
| `test_execute_scan_only_returns_scan_artifact_path`                  | Scan-only produces an artifact                  |
| `test_execute_scan_only_writes_output_path_when_specified`           | Output path written when provided               |
| `test_execute_dry_run_skips_plugin_execution`                        | Dry run never invokes the plugin                |
| `test_execute_local_plan_resume_loads_existing_scan_artifact`        | Resume reuses existing scan artifact            |
| `test_execute_watcher_task_returns_watcher_result`                   | Watcher input produces a watcher result         |
| `test_execute_watcher_task_sets_correlation_id_in_result`            | Correlation ID propagated correctly             |
| `test_execute_local_plan_transitions_workspace_to_complete`          | Workspace reaches `Complete` stage              |
| `test_execute_local_plan_records_report_paths`                       | Report files written and paths recorded         |
| `test_execute_watcher_task_security_event_maps_to_security_result`   | Security event type mapped to security result   |
| `test_execute_local_plan_unknown_plugin_captures_error_not_panic`    | Unknown plugin error captured, not panicked     |

The watcher executor tests in `src/watcher/executor.rs` were updated to use real
temporary directories for tests that exercise full plugin execution. Tests that
cover validation failure and dry-run behavior continue to work with synthetic
repository URLs because execution is short-circuited before the repository
resolution stage.

---

## Design Decisions

- **Shared engine**: A single `WorkflowExecutor` struct is instantiated by both
  the CLI command handler and the `WatcherExecutor`. No code duplication.
- **Plugin context isolation**: For each plugin step the executor loads a fresh
  `WorkspaceManager` snapshot from disk. This ensures the plugin sees an
  up-to-date workspace state and that any writes the plugin makes are visible to
  the executor after the step completes.
- **Graceful plugin failures**: Plugin steps that return `PluginOutput::failure`
  (not an `Err`) are recorded in the workspace state and do not abort the
  pipeline. The final `ExecutionResult::success` reflects whether all steps
  produced `PluginOutput::completed == true`.
- **Unrecoverable plugin errors**: When `plugin.run(ctx).await` returns `Err`,
  the workspace transitions to `WorkspaceStage::Failed` and the executor returns
  a `PipelineError::Plugin`. This preserves workspace state for post-mortem
  inspection.
