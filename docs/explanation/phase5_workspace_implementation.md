# Phase 5 Workspace Management Implementation

## Overview

Phase 5 delivers the workspace management layer for the XZardgz pipeline. A
workspace is an isolated directory on disk that contains every artifact produced
by a single pipeline execution: the cloned repository, scan output, plugin
results, reports, transcripts, diagnostics, and watcher snapshots.

The deliverables for Phase 5 are:

| File                            | Role                                                    |
| ------------------------------- | ------------------------------------------------------- |
| `src/workspace/id.rs`           | ULID generation, SHA-256 repository hashing, timestamps |
| `src/workspace/stage.rs`        | `WorkspaceStage` enum and transition predicates         |
| `src/workspace/paths.rs`        | Deterministic directory and file path helpers           |
| `src/workspace/state.rs`        | `WorkspaceState` serializable model and YAML I/O        |
| `src/workspace/mod.rs`          | `WorkspaceManager` public API and module re-exports     |
| `tests/unit/workspace_tests.rs` | 15 integration tests                                    |

---

## Module Structure

### id.rs

Provides three categories of utility:

- **ULID generation**: `new_workspace_id()` returns a 26-character upper-case
  ULID string. ULIDs are monotonically increasing and lexicographically
  sortable, so workspace directories sort by creation time without any
  additional metadata.
- **Repository hashing**: `hash_repository(repo)` returns a 64-character
  lower-hex SHA-256 digest of the repository URL or path string. The digest is
  used as a stable, compact key for locating existing workspaces for the same
  repository.
- **Timestamps**: `now_utc()` and `now_rfc3339()` return the current UTC time as
  `chrono::DateTime<Utc>` and as an RFC 3339 string respectively.

The module also exports `WORKSPACE_STATE_VERSION`, a `&str` constant set to
`"1"`, which is embedded in every state file for forward-compatibility
detection.

### stage.rs

Defines `WorkspaceStage`, the discriminated union that tracks where the pipeline
is at any given moment. The full variant set is:

```text
Initializing
Scanning
ScanComplete
PluginRunning  { step_id: String }
PluginComplete { step_id: String }
ReportWriting
ReportComplete
Publishing
Complete
Failed         { stage: String, reason: String }
```

The enum derives `Serialize`/`Deserialize` with
`#[serde(tag = "kind", rename_all = "snake_case")]`, so YAML serialization
produces a mapping with a `kind` key and any variant-specific fields alongside
it.

Predicate methods on the enum:

| Method          | Returns `true` when                           |
| --------------- | --------------------------------------------- |
| `is_complete()` | variant is `Complete`                         |
| `is_failed()`   | variant is `Failed`                           |
| `is_terminal()` | `is_complete()` or `is_failed()` is true      |
| `label()`       | snake_case string name of the current variant |

### paths.rs

`WorkspacePaths` encapsulates all path derivation for a single workspace rooted
at `<workspace_root>/<workspace_id>/`. The public API:

```text
new(workspace_root, workspace_id)  -> WorkspacePaths
state_file()                       -> PathBuf  (<root>/state.yaml)
repo_dir()                         -> PathBuf  (<root>/repo/)
scan_dir()                         -> PathBuf  (<root>/scan/)
scan_artifact()                    -> PathBuf  (<root>/scan/artifact.yaml)
plugin_dir(step_id)                -> PathBuf  (<root>/plugins/<step_id>/)
plugin_output(step_id)             -> PathBuf  (<root>/plugins/<step_id>/output.yaml)
reports_dir()                      -> PathBuf  (<root>/reports/)
step_reports_dir(step_id)          -> PathBuf  (<root>/reports/<step_id>/)
transcripts_dir()                  -> PathBuf  (<root>/transcripts/)
step_transcript(step_id)           -> PathBuf  (<root>/transcripts/<step_id>.yaml)
diagnostics_dir()                  -> PathBuf  (<root>/diagnostics/)
diagnostics_file()                 -> PathBuf  (<root>/diagnostics/diagnostics.yaml)
watcher_dir()                      -> PathBuf  (<root>/watcher/)
watcher_task_snapshot()            -> PathBuf  (<root>/watcher/task.yaml)
watcher_result_snapshot()          -> PathBuf  (<root>/watcher/result.yaml)
create_all()                       -> Result<()>
```

`create_all` uses `std::fs::create_dir_all` for every directory, so it is
idempotent.

### state.rs

Defines two serializable types.

`PluginOutputRecord` captures the result of one plugin step execution. It stores
the step ID, plugin name, optional output file path, completion timestamp,
success flag, and a list of diagnostic messages. All optional fields carry
`#[serde(default)]` so records written by earlier versions of the code can be
deserialized without error.

`WorkspaceState` is the top-level document written to `state.yaml`. See the
field inventory section below for the full list.

Two methods are provided:

- `WorkspaceState::new(...)` constructs a fresh state in `Initializing` stage.
- `WorkspaceState::load_from_str(content)` deserializes from a YAML string,
  returning `PipelineError::Workspace` on failure.
- `WorkspaceState::to_yaml(&self)` serializes to a YAML string, returning
  `PipelineError::Workspace` on failure.

### mod.rs

Declares the four submodules and re-exports the key public types at the
`xzardgz::workspace` level:

```rust
pub use paths::WorkspacePaths;
pub use stage::WorkspaceStage;
pub use state::{PluginOutputRecord, WorkspaceState};
```

Also defines `WorkspaceManager`, the primary public interface. See the API
section below.

---

## WorkspaceStage Model and Transitions

The typical pipeline progression is linear:

```text
Initializing
  -> Scanning
  -> ScanComplete
  -> PluginRunning { step_id }
  -> PluginComplete { step_id }
  -> ReportWriting
  -> ReportComplete
  -> Publishing
  -> Complete
```

Any stage can transition to `Failed { stage, reason }`. The `stage` field
records the name of the stage that was active when the failure occurred; the
`reason` field records a human-readable description.

`WorkspaceManager::transition` is idempotent: calling it with the current stage
simply refreshes the timestamp recorded in `stage_timestamps`.

---

## WorkspaceState Field Inventory

| Field                       | Type                                  | Default         | Purpose                                     |
| --------------------------- | ------------------------------------- | --------------- | ------------------------------------------- |
| `version`                   | `String`                              | `"1"`           | Schema version for forward-compat detection |
| `workspace_id`              | `String`                              | generated       | ULID unique identifier                      |
| `repository_url`            | `String`                              | required        | Source repository URL or local path         |
| `repository_hash`           | `String`                              | computed        | SHA-256 of `repository_url`                 |
| `local_repository_path`     | `Option<String>`                      | `None`          | Path after checkout                         |
| `branch_name`               | `Option<String>`                      | `None`          | Currently checked-out branch                |
| `target_branch`             | `Option<String>`                      | caller-supplied | Requested branch                            |
| `current_stage`             | `WorkspaceStage`                      | `Initializing`  | Active pipeline stage                       |
| `scan_artifact_path`        | `Option<String>`                      | `None`          | Path to scan output YAML                    |
| `scan_artifact_version`     | `Option<String>`                      | `None`          | Version in the artifact                     |
| `scan_artifact_created_at`  | `Option<DateTime<Utc>>`               | `None`          | Artifact creation time                      |
| `scan_artifact_head_commit` | `Option<String>`                      | `None`          | HEAD commit at scan time                    |
| `plugin_outputs`            | `HashMap<String, PluginOutputRecord>` | empty           | Per-step plugin results                     |
| `written_files`             | `Vec<String>`                         | empty           | All files written (audit trail)             |
| `stage_timestamps`          | `HashMap<String, DateTime<Utc>>`      | empty           | Stage entry times                           |
| `created_at`                | `DateTime<Utc>`                       | `now_utc()`     | Workspace creation time                     |
| `updated_at`                | `DateTime<Utc>`                       | `now_utc()`     | Last state mutation time                    |
| `report_paths`              | `HashMap<String, Vec<String>>`        | empty           | Per-step report files                       |
| `plugin_scores`             | `HashMap<String, f64>`                | empty           | Per-step numeric scores                     |
| `plugin_diagnostics`        | `HashMap<String, Vec<String>>`        | empty           | Per-step messages                           |
| `watcher_task_id`           | `Option<String>`                      | `None`          | Watcher origin task ID                      |
| `watcher_result_published`  | `bool`                                | `false`         | Whether result was published                |

---

## Directory Layout

A workspace rooted at `<workspace_root>/<id>/` has the following structure:

```text
<workspace_root>/
  <workspace_id>/
    state.yaml
    repo/
    scan/
      artifact.yaml
    plugins/
      <step_id>/
        output.yaml
    reports/
      <step_id>/
    transcripts/
      <step_id>.yaml
    diagnostics/
      diagnostics.yaml
    watcher/
      task.yaml
      result.yaml
```

All directories are created by `WorkspacePaths::create_all`, which is called
automatically by `WorkspaceManager::create`.

---

## WorkspaceManager Public API

### create

```rust
pub fn create(
    workspace_root: &str,
    repository_url: &str,
    target_branch: Option<String>,
    watcher_task_id: Option<String>,
) -> Result<Self>
```

Generates a new ULID workspace ID, computes `hash_repository(repository_url)`,
constructs `WorkspaceState::new(...)`, builds a `WorkspacePaths`, calls
`create_all()` to create all subdirectories, writes the initial `state.yaml`,
and returns the manager.

### load

```rust
pub fn load(workspace_root: &str, workspace_id: &str) -> Result<Self>
```

Builds a `WorkspacePaths` from the arguments, reads
`<workspace_root>/<workspace_id>/state.yaml`, and deserializes it via
`WorkspaceState::load_from_str`. Returns `PipelineError::Workspace` if the file
is absent or unparseable.

### open

```rust
pub fn open(
    workspace_root: &str,
    repository_url: &str,
    target_branch: Option<String>,
) -> Result<Self>
```

Computes `hash_repository(repository_url)` then scans `workspace_root` with
`std::fs::read_dir`. For each entry that is a directory it attempts
`WorkspaceManager::load`; entries that fail to load are silently skipped. Loaded
workspaces whose `state.repository_hash` matches are collected, sorted
descending by `workspace_id` (ULID chronological order), and the first result is
returned. If no match is found, `create` is called with the supplied arguments.
If `read_dir` returns `NotFound`, `create` is called immediately rather than
returning an error.

### save

```rust
pub fn save(&self) -> Result<()>
```

Serializes `self.state` to YAML and overwrites `state.yaml`. All mutating
methods call `save` before returning.

### transition

```rust
pub fn transition(&mut self, stage: WorkspaceStage) -> Result<()>
```

Inserts `(stage.label(), now_utc())` into `stage_timestamps`, sets
`current_stage = stage`, refreshes `updated_at`, then calls `save`.

### record_scan_artifact

```rust
pub fn record_scan_artifact(
    &mut self,
    path: String,
    version: Option<String>,
    head_commit: Option<String>,
) -> Result<()>
```

Sets `scan_artifact_path`, `scan_artifact_version`, `scan_artifact_head_commit`,
and `scan_artifact_created_at`, then delegates to
`transition(WorkspaceStage::ScanComplete)` which saves state.

### record_plugin_output

```rust
pub fn record_plugin_output(
    &mut self,
    step_id: &str,
    plugin: &str,
    output_path: Option<String>,
    success: bool,
    diagnostics: Vec<String>,
) -> Result<()>
```

Replaces `plugin_diagnostics[step_id]` with the supplied diagnostics list,
constructs a `PluginOutputRecord` with `completed_at = Some(now_utc())`, and
inserts it into `plugin_outputs[step_id]` (replacing any previous record for the
same step). Updates `updated_at` and saves.

### add_report_path

```rust
pub fn add_report_path(&mut self, step_id: &str, path: String) -> Result<()>
```

Appends `path` to `report_paths[step_id]`, creating the list entry if it does
not yet exist. Updates `updated_at` and saves.

### mark_published

```rust
pub fn mark_published(&mut self) -> Result<()>
```

Sets `watcher_result_published = true`, updates `updated_at`, and saves.
Idempotent.

---

## Idempotency Rules

| Operation                          | How idempotency is achieved                                                             |
| ---------------------------------- | --------------------------------------------------------------------------------------- |
| `WorkspacePaths::create_all`       | Uses `std::fs::create_dir_all` which succeeds if the directory already exists           |
| `transition`                       | Overwrites the stage and refreshes the timestamp; no guard on the previous value        |
| `record_plugin_output`             | Uses `HashMap::insert` which replaces an existing key                                   |
| `record_plugin_output` diagnostics | Uses `HashMap::insert` which replaces the diagnostics list                              |
| `mark_published`                   | Sets a `bool` flag; calling it when already `true` is a no-op (aside from `updated_at`) |
| `open`                             | Returns the most recent existing workspace if one matches; does not create a duplicate  |

---

## ULID-based Workspace IDs

ULIDs (Universally Unique Lexicographically Sortable Identifiers) are used as
workspace identifiers for two reasons:

1. **Sortability**: ULIDs embed the creation timestamp in the first 10
   characters of their 26-character encoding. Lexicographic sort of ULID strings
   is equivalent to chronological sort without any secondary metadata.

2. **Uniqueness**: The remaining 16 characters are random, giving a collision
   probability below one in a trillion even when many workspaces are created
   within the same millisecond.

The `open` function exploits ULID sortability to select the most recently
created workspace when multiple workspaces exist for the same repository URL
without requiring any timestamp comparisons.

---

## Repository Hashing

`hash_repository(repo)` computes a SHA-256 digest of the repository URL or local
path string. The digest is used as a stable, deterministic key that allows
`open` to locate all workspaces for a given repository by scanning the workspace
root directory and comparing state file hashes.

SHA-256 was chosen because:

- It is collision-resistant for string inputs of this length.
- The output is a fixed-length 64-character hex string that is safe to use
  directly as a comparison key.
- The `sha2` crate is already a dependency of the project.

---

## RFC 3339 Timestamps

All `DateTime<Utc>` fields in `WorkspaceState` are serialized by `serde` with
the `chrono` `serde` feature as RFC 3339 strings (e.g.
`2024-01-15T10:30:00.123456789+00:00`). This format is:

- Human-readable in the state YAML file.
- Unambiguous (always UTC, always timezone-qualified).
- Round-trip stable: parsing a serialized timestamp and re-serializing it
  produces the same string.

---

## Test Coverage

### `src/workspace/id.rs` (inline, 7 tests)

| Test name                                           | What is covered            |
| --------------------------------------------------- | -------------------------- |
| `test_new_workspace_id_returns_26_char_string`      | ULID length                |
| `test_new_workspace_id_generates_unique_values`     | ULID uniqueness            |
| `test_hash_repository_is_deterministic`             | SHA-256 determinism        |
| `test_hash_repository_returns_64_char_hex`          | SHA-256 length and charset |
| `test_hash_repository_differs_for_different_inputs` | SHA-256 distinctness       |
| `test_now_rfc3339_contains_rfc3339_markers`         | RFC 3339 format            |
| `test_workspace_state_version_is_one`               | version constant value     |

### `src/workspace/state.rs` (inline, 7 tests)

| Test name                                         | What is covered                  |
| ------------------------------------------------- | -------------------------------- |
| `test_new_creates_state_with_correct_fields`      | Constructor field population     |
| `test_new_sets_initializing_stage`                | Initial stage value              |
| `test_new_sets_version_to_state_version_constant` | Schema version                   |
| `test_to_yaml_produces_valid_yaml`                | YAML serialization output        |
| `test_load_from_str_round_trips_state`            | Serialization round-trip         |
| `test_load_from_str_rejects_invalid_yaml`         | Error on malformed YAML          |
| `test_plugin_output_record_defaults`              | Serde `#[serde(default)]` fields |

### `src/workspace/mod.rs` (inline, 17 tests)

| Test name                                              | What is covered                 |
| ------------------------------------------------------ | ------------------------------- |
| `test_create_creates_workspace_directory`              | Directory created on disk       |
| `test_create_saves_initial_state_file`                 | State file created and readable |
| `test_load_reads_saved_state`                          | Load by ID restores state       |
| `test_save_and_load_roundtrip`                         | Save/load round-trip fidelity   |
| `test_transition_updates_current_stage`                | Stage mutation                  |
| `test_transition_records_timestamp`                    | Timestamp recorded in map       |
| `test_transition_is_idempotent`                        | Repeated transition is safe     |
| `test_record_scan_artifact_sets_path_and_stage`        | Scan artifact fields + stage    |
| `test_record_plugin_output_stores_record`              | Record insertion and fields     |
| `test_record_plugin_output_replaces_on_rerun`          | Replace semantics               |
| `test_add_report_path_appends_to_step_list`            | Append accumulation             |
| `test_mark_published_sets_flag`                        | Flag set to true                |
| `test_mark_published_is_idempotent`                    | Repeated call is safe           |
| `test_open_creates_workspace_when_none_exists`         | Create path of open             |
| `test_open_resumes_existing_workspace`                 | Resume path of open             |
| `test_is_failed_returns_true_when_stage_is_failed`     | Failed predicate                |
| `test_is_complete_returns_true_when_stage_is_complete` | Complete predicate              |

### `tests/unit/workspace_tests.rs` (integration, 15 tests)

| Test name                                                           | What is covered                 |
| ------------------------------------------------------------------- | ------------------------------- |
| `test_workspace_create_produces_valid_state_file`                   | End-to-end create and parse     |
| `test_workspace_load_by_id_returns_correct_state`                   | Load-by-ID contract             |
| `test_workspace_open_creates_new_when_no_match`                     | Open-create path                |
| `test_workspace_open_resumes_most_recent_matching_workspace`        | Open-resume path                |
| `test_workspace_transition_to_scanning`                             | Transition persists to disk     |
| `test_workspace_transition_to_failed_preserves_plugin_outputs`      | State durability through Failed |
| `test_workspace_record_scan_artifact_sets_scan_complete_stage`      | Scan artifact and stage         |
| `test_workspace_record_plugin_output_can_be_replaced`               | Replace semantics on rerun      |
| `test_workspace_add_report_path_accumulates`                        | Report path accumulation        |
| `test_workspace_mark_published_is_idempotent`                       | Published flag idempotency      |
| `test_workspace_state_serializes_rfc3339_timestamps`                | Timestamp round-trip            |
| `test_workspace_failed_stage_preserves_diagnostics`                 | Diagnostics survive Failed      |
| `test_workspace_id_generation_creates_unique_ids`                   | ULID uniqueness (10 IDs)        |
| `test_workspace_repository_hash_is_stable_across_create_and_reload` | Hash stability                  |
| `test_workspace_paths_create_all_is_idempotent`                     | Directory creation idempotency  |

---

## Quality Gate Results

This section is a placeholder to be filled in after Track A and Track B outputs
are assembled and the following commands are run in order:

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Results will be recorded here once the full assembly pass completes.
