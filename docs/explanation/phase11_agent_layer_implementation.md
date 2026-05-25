# Phase 11: Agent Layer, Sandbox, and Tools Implementation

## Overview

Phase 11 delivers three tightly coupled components that together form the
sandboxed agent execution layer:

1. **Sandbox** (`src/tools/sandbox.rs`) - `PathValidator` enforcing read and
   write zone restrictions.
2. **Tools** (`src/tools/`) - ten sandboxed filesystem tools, updated
   `ToolExecutor` trait, and registry builder functions.
3. **Agent session** (`src/agent/`) - `AgentContext` with workspace metadata and
   `AgentSession` orchestrating the bounded, transcript-enabled provider-tool
   loop.

All components satisfy the Phase 11 acceptance criteria:

- Plugins can use tools safely within configured zones.
- Tool failures are observable and non-fatal to the agent loop.
- Review plugins cannot modify repositories by default.

## Architecture

```text
AgentSession
  |
  |-- AgentContext (messages, workspace metadata, trace settings)
  |
  |-- Arc<dyn Provider> (complete / complete_with_thinking)
  |
  |-- Arc<ToolRegistry>
       |
       |-- ReadFileTool -----> PathValidator (read zones only)
       |-- ListDirectoryTool -> PathValidator (read zones only)
       |-- WriteFileTool ----> PathValidator (write zones required)
       |-- ...
```

## Component Details

### PathValidator (src/tools/sandbox.rs)

`PathValidator` enforces filesystem access restrictions before any tool touches
the filesystem. It holds two sets of canonicalized zone paths:

- **Read zones** - directories accessible to read-only tools.
- **Write zones** - directories accessible to write tools.

Security properties:

| Threat                      | Mitigation                                                                       |
| --------------------------- | -------------------------------------------------------------------------------- |
| Path traversal (`../`)      | `reject_traversal` scans `Component::ParentDir` before canonicalization          |
| Symlink escape              | `std::fs::canonicalize` resolves all symlinks; zone check runs on canonical path |
| Absolute path outside zones | Zone membership check on canonical path rejects non-members                      |
| Unauthorized writes         | Empty write zones cause all write attempts to fail immediately                   |

Zone paths are canonicalized at `PathValidator::new` construction time, not
per-call. This handles macOS where `/tmp` resolves to `/private/tmp` and ensures
comparisons are always between canonical paths.

### ToolExecutor Trait Update (src/tools/mod.rs)

The `ToolExecutor` trait gained a required `tool_definition(&self) -> Tool`
method:

```xzardgz/src/tools/mod.rs#L1-1

```

Every executor returns its own definition, enabling self-registration through
`ToolRegistry::register_executor`. All existing implementors (`ReadFileTool`,
`WriteFileTool`, `GitStatusTool`) were updated.

### File Tools (src/tools/file_ops.rs)

All tools accept `Arc<PathValidator>` at construction and route every path
argument through the validator before performing I/O.

**Read-only tools** (used in read-only registries):

| Tool                        | Tool Name                 | Purpose                                          |
| --------------------------- | ------------------------- | ------------------------------------------------ |
| `ReadFileTool`              | `read_file`               | Read full file contents                          |
| `ListDirectoryTool`         | `list_directory`          | List directory entries as `type:name` lines      |
| `SearchFileContentsTool`    | `search_file_contents`    | Return matching lines with `L{n}: {line}` format |
| `FindFilesByGlobTool`       | `find_files_by_glob`      | Walk a tree and match filenames by glob pattern  |
| `ReadScanArtifactTool`      | `read_scan_artifact`      | Read a scan artifact YAML file verbatim          |
| `ReadWorkspaceMetadataTool` | `read_workspace_metadata` | Parse workspace YAML and return pretty JSON      |

**Write tools** (only in read-write registries):

| Tool                      | Tool Name               | Purpose                                       |
| ------------------------- | ----------------------- | --------------------------------------------- |
| `WriteFileTool`           | `write_file`            | Write content to a file (creates parent dirs) |
| `CreateDirectoryTool`     | `create_directory`      | Create directory tree idempotently            |
| `WriteReportArtifactTool` | `write_report_artifact` | Write a report artifact file                  |
| `AppendDiagnosticTool`    | `append_diagnostic`     | Append a timestamped YAML diagnostic entry    |

### Registry Builders (src/tools/registry.rs)

Four builder functions construct pre-populated registries for different plugin
contexts:

- `build_read_only_registry(validator)` - six read-only tools; suitable for
  review plugins that must not modify the repository.
- `build_read_write_registry(validator)` - read-only plus four write tools;
  suitable for plugins that generate reports or diagnostics.
- `build_subagent_registry(validator)` - currently identical to read-write;
  reserved for subagent delegation tools when that capability ships.
- `build_mcp_augmented_registry(validator, mcp_tools)` - read-write plus
  externally supplied MCP tools registered as first-class executors.

The `build_read_only_registry` validator should be constructed with
`PathValidator::read_only(read_zones)`, which sets an empty write zone list.
This ensures write tools in any augmented registry cannot write unless the
validator explicitly permits it.

### AgentContext (src/agent/context.rs)

`AgentContext` extends the message-window management of `ConversationContext`
with session-scoped metadata needed by plugins and the orchestration layer:

| Field                | Type                       | Purpose                                    |
| -------------------- | -------------------------- | ------------------------------------------ |
| `workspace_id`       | `Option<String>`           | Workspace identifier                       |
| `workspace_root`     | `Option<PathBuf>`          | Workspace root for tool scoping            |
| `scan_artifact_path` | `Option<PathBuf>`          | Path to the scan artifact YAML             |
| `plugin_metadata`    | `HashMap<String, Value>`   | Plugin-specific key-value data             |
| `provider_metadata`  | `Option<ProviderMetadata>` | Cached capability information              |
| `trace_enabled`      | `bool`                     | Whether trace logging is active            |
| `step_id`            | `Option<String>`           | Step identifier for transcript namespacing |

The builder pattern (`with_workspace`, `with_scan_artifact`, `with_step_id`,
etc.) allows partial initialization without exposing mutable setters.

`ConversationContext` is unchanged and continues to serve the legacy `Agent` and
`AgentExecutor` types.

### AgentSession (src/agent/session.rs)

`AgentSession` is the primary orchestration type for sandboxed, bounded agent
execution. Its `run` method:

1. Checks `provider.metadata().capabilities.tools` at entry. Providers without
   tool support are routed to `run_single_turn`, which sends a plain completion
   with no tool definitions.

2. Adds the user message to context and, if transcript persistence is enabled,
   writes the first JSONL line before entering the loop.

3. Iterates up to `max_turns` (default: 10). Each turn:

   - Snapshots messages and tool definitions while briefly holding the context
     mutex, then releases the lock before the async provider call.
   - Dispatches tool calls via `ToolExecutionDispatcher`.
   - Routes both `Err(e)` responses and `Ok(result)` values with a non-None
     `error` field through `record_tool_failure`. The session continues; the
     error is forwarded to the model as a `Role::Tool` message.
   - Writes each message to the JSONL transcript in append mode.
   - Returns the assistant content on the first turn with no tool calls.

4. Returns `PipelineError::Agent("max turns reached")` after exhausting the turn
   budget.

#### Tool Failure Tracking

`record_tool_failure` stores per-tool failure counts in a
`Mutex<HashMap<String, usize>>`. When a tool's count reaches
`repeated_failure_threshold` (default: 3), a structured `tracing::warn!` is
emitted with `tool` and `count` fields for log aggregation.

#### Transcript Format

Each message is written as a compact JSON object on its own line (JSONL). The
transcript file is opened with `create(true).append(true)` so multiple session
runs accumulate rather than truncate prior content.

#### Lock Strategy

Mutexes on `context` and `failure_counts` are held only for the minimum
necessary scope - never across an `.await` point. This prevents deadlocks during
provider calls and filesystem I/O.

## Deliverables

| Deliverable                | Location                |
| -------------------------- | ----------------------- |
| PathValidator sandbox      | `src/tools/sandbox.rs`  |
| Updated ToolExecutor trait | `src/tools/mod.rs`      |
| Ten sandboxed file tools   | `src/tools/file_ops.rs` |
| Registry builder functions | `src/tools/registry.rs` |
| AgentContext               | `src/agent/context.rs`  |
| AgentSession               | `src/agent/session.rs`  |
| Doc ops module placeholder | `src/tools/doc_ops.rs`  |

## Test Coverage

Phase 11 adds the following test categories:

| Category                              | Tests Added | Location      |
| ------------------------------------- | ----------- | ------------- |
| PathValidator traversal rejection     | 2           | `sandbox.rs`  |
| PathValidator zone membership         | 4           | `sandbox.rs`  |
| PathValidator symlink handling (unix) | 2           | `sandbox.rs`  |
| File tools (read-only)                | 6           | `file_ops.rs` |
| File tools (write)                    | 5           | `file_ops.rs` |
| Registry builders                     | 5           | `registry.rs` |
| AgentContext builders and message ops | 6           | `context.rs`  |
| AgentSession run loop                 | 6           | `session.rs`  |

All 734 unit tests, 79 integration tests, and 180 doctests pass.

## Success Criteria Verification

| Criterion                                  | Status                                                      |
| ------------------------------------------ | ----------------------------------------------------------- |
| Plugins can use tools safely               | `PathValidator` enforces zones on every call                |
| Tool failures are observable and non-fatal | `record_tool_failure` + `tracing::warn!`; loop continues    |
| Review plugins cannot modify repositories  | `build_read_only_registry` + `PathValidator::read_only`     |
| Agent max-turn behavior bounded            | `with_max_turns`; returns `PipelineError::Agent` on exhaust |
| Path traversal rejected                    | `reject_traversal` checks before canonicalization           |
| Symlink escapes rejected                   | `canonicalize` resolves all symlinks before zone check      |
| Transcript persistence                     | `with_transcript` + JSONL append mode                       |
| Single-turn fallback                       | `run_single_turn` when `capabilities.tools == false`        |
