# Workspace Model Reference

## Overview

A workspace is an isolated working directory created for each pipeline run. It
stores the scan artifact, plugin reports, workspace state, and optional
transcripts for a single execution. Workspaces make pipeline runs reproducible,
resumable, and debuggable.

Each workspace is identified by a ULID and lives under the configured workspace
root directory (default: `.xzardgz/workspaces`).

## WorkspaceState Schema

`WorkspaceState` is persisted as `state.json` inside the workspace directory
after each significant pipeline event.

| Field                 | Type              | Description                                                  |
| --------------------- | ----------------- | ------------------------------------------------------------ |
| `workspace_id`        | string (ULID)     | Unique workspace identifier.                                 |
| `repository_name`     | string            | Short name of the repository being analyzed.                 |
| `repository_url`      | string (optional) | Clone URL of the repository, when applicable.                |
| `head_commit`         | string (optional) | HEAD commit SHA at the time of scanning.                     |
| `target_branch`       | string (optional) | Target branch name.                                          |
| `scan_artifact_path`  | string (optional) | Path to the scan artifact JSON file.                         |
| `scan_completed_at`   | string (optional) | ISO 8601 timestamp when scanning completed.                  |
| `provider_name`       | string (optional) | Resolved provider name, e.g. `openai`.                       |
| `model_id`            | string (optional) | Resolved model identifier, e.g. `gpt-4.1-mini`.              |
| `provider_metadata`   | object (optional) | Structured provider diagnostics from capability resolution.  |
| `plugin_name`         | string (optional) | Identifier of the active plugin, e.g. `technical-review`.    |
| `plugin_started_at`   | string (optional) | ISO 8601 timestamp when the plugin step started.             |
| `plugin_completed_at` | string (optional) | ISO 8601 timestamp when the plugin step completed.           |
| `plugin_status`       | string (enum)     | Current plugin execution status.                             |
| `report_paths`        | object            | Map of report format to list of output file paths.           |
| `diagnostics`         | array             | Structured diagnostics collected during the run.             |
| `plugin_scores`       | object            | Map of step identifier to numeric score (0.0 to 1.0).        |
| `watcher_task_id`     | string (optional) | Task identifier set by the watcher when run in watcher mode. |
| `final_status`        | string (enum)     | Final pipeline outcome.                                      |

### `plugin_status` values

| Value       | Meaning                                                |
| ----------- | ------------------------------------------------------ |
| `pending`   | Plugin step is queued but has not started.             |
| `running`   | Plugin step is currently executing.                    |
| `completed` | Plugin step finished successfully.                     |
| `failed`    | Plugin step encountered an error and did not complete. |

### `final_status` values

| Value     | Meaning                                                     |
| --------- | ----------------------------------------------------------- |
| `success` | All pipeline steps completed without error.                 |
| `failed`  | One or more pipeline steps failed.                          |
| `dry_run` | Pipeline was executed in dry-run mode; no reports produced. |

## Workspace Directory Layout

```text
.xzardgz/
  workspaces/
    <workspace_id>/
      state.json                        WorkspaceState persisted to disk
      scan/
        scan.json                       Scan artifact for this workspace
      reports/
        technical_review.md             Technical review Markdown report
        technical_review.json           Technical review JSON report
        security_review.md              Security review Markdown report
        security_review.json            Security review JSON report
        security_review.sarif.json      Security review SARIF report
      transcripts/                      Optional; created when trace_transcript.enabled is true
      publish_failure/                  Optional; created when a Kafka publish fails in watcher mode
  scan/
    scan.json                           Global scan output (scan_output.path)
  reports/                              Global report output directory
  model_metadata.json                   Model capability cache
  prompts/                              Custom prompt template overrides
  transcripts/                          Global transcripts directory
```

Each workspace directory is named after its ULID. This means multiple runs
produce independent workspace directories, allowing historical comparisons.

## Resume Behavior

When `workspace.resume` is `true` and a workspace already exists for the current
repository and run configuration, the pipeline resumes from the last
successfully completed step rather than starting over.

Resume conditions:

- A workspace directory with a matching identifier exists.
- `state.json` is present and readable.
- `final_status` is not `failed`, or `keep_failed` is `false`.

When resuming, the pipeline reads `state.json` to determine which steps have
already completed and skips them. Only incomplete or failed steps are re-run.

To force a fresh run, remove the workspace directory or set `workspace.resume`
to `false`.

## Idempotency Rules

The following steps are subject to idempotency checks on resume.

| Step             | Skip Condition                                                              |
| ---------------- | --------------------------------------------------------------------------- |
| `scan`           | `scan_artifact_path` is set in state and the file exists on disk.           |
| `plugin` step    | `plugin_status` is `completed` for the matching plugin and step identifier. |
| `report` writing | Report file exists at the path recorded in `report_paths`.                  |

Idempotency prevents redundant provider calls and avoids re-charging API usage
for work that already completed in a prior run.

## Configuration Reference

The `workspace` section in `config.yaml` controls workspace behavior.

| Field         | Type   | Default                 | Description                                                      |
| ------------- | ------ | ----------------------- | ---------------------------------------------------------------- |
| `root`        | string | `".xzardgz/workspaces"` | Directory where workspace subdirectories are created.            |
| `resume`      | bool   | `true`                  | Resume from a prior workspace when one exists.                   |
| `keep_failed` | bool   | `true`                  | Preserve failed workspace directories for post-mortem debugging. |

Environment variable override: `XZARDGZ_WORKSPACE` sets `root` at runtime.

```yaml
workspace:
  root: ".xzardgz/workspaces"
  resume: true
  keep_failed: true
```

## Workspace Lifecycle

```text
created
  |
  v
scanning          (scan step running)
  |
  v
scan_complete     (scan artifact written, scan_artifact_path set in state)
  |
  v
plugin_running    (plugin_status = running)
  |
  +-------> failed   (plugin_status = failed, final_status = failed)
  |
  v
plugin_complete   (plugin_status = completed)
  |
  v
reports_written   (report_paths populated)
  |
  v
completed         (final_status = success)
```

At each transition, `state.json` is written to disk. If the process is
interrupted, the next run reads the last written state and resumes from the
appropriate point.

## Publish Failure State

When the pipeline runs in watcher mode and a Kafka publish fails, the failed
message payload is written to the `publish_failure/` subdirectory inside the
workspace. This allows manual inspection and resubmission without losing the
result data.

The `PublishFailureState` includes:

- `task_id`: the watcher task identifier that produced the result
- `topic`: the Kafka topic the publish was attempted on
- `payload`: the serialized result message
- `failed_at`: ISO 8601 timestamp of the failure
- `error`: error message from the publish attempt

## Debugging

### Inspecting workspace state

Read `state.json` directly to inspect the last persisted state:

```bash
cat .xzardgz/workspaces/<workspace_id>/state.json
```

The file is formatted JSON. Key fields to check:

- `plugin_status`: confirms whether the plugin completed or failed
- `final_status`: confirms the overall pipeline outcome
- `diagnostics`: structured list of warnings and errors collected during the run
- `report_paths`: confirms which report files were written and where

### `keep_failed` behavior

When `keep_failed` is `true`, failed workspace directories are retained on disk
after a failed run. This preserves partial scan artifacts, logs, and state for
debugging. Set `keep_failed` to `false` to automatically clean up failed
workspaces.

```yaml
workspace:
  keep_failed: true
```

### Forcing a fresh run

To discard an existing workspace and start from scratch:

```bash
rm -rf .xzardgz/workspaces/<workspace_id>
xzardgz plan run --plan sample_plan.yaml
```

Alternatively, set `workspace.resume: false` in `config.yaml` to always create a
new workspace on each run.

### Listing workspaces

```bash
ls -1 .xzardgz/workspaces/
```

Each directory name is a ULID. ULIDs are lexicographically sortable by creation
time, so the most recent workspace appears last.
