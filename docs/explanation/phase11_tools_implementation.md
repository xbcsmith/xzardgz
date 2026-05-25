# Phase 11 Tools Implementation

## Overview

Phase 11 completes the `src/tools/` layer by adding a path validation sandbox,
ten sandboxed filesystem tools, registry builder functions, and the
`tool_definition` method on the `ToolExecutor` trait.

## What Was Implemented

### New file: `src/tools/sandbox.rs`

Provides `PathValidator`, a struct that enforces read and write zone
restrictions before any filesystem operation is permitted.

Key design decisions:

- Zone paths are canonicalized at construction time. On macOS `/tmp` is a
  symlink to `/private/tmp`; canonicalizing at construction ensures that
  validated paths (also canonicalized) match correctly.
- `validate_read` calls `std::fs::canonicalize` which resolves symlinks. A
  symlink that escapes the zone will resolve to a canonical path outside the
  zone and be rejected.
- `validate_write` canonicalizes the parent directory and joins the filename.
  This allows writing to files that do not yet exist as long as their immediate
  parent exists.
- `reject_traversal` scans path components for `Component::ParentDir` and
  rejects any path containing `..` before canonicalization takes place.
- `read_only` is a convenience constructor that produces an empty write zone
  list, causing all write attempts to fail immediately.

### Updated file: `src/tools/mod.rs`

- Added `tool_definition(&self) -> Tool` to the `ToolExecutor` trait. Every
  executor must now return its own definition, enabling self-registration via
  `ToolRegistry::register_executor`.
- Added `pub mod sandbox;` and `pub mod doc_ops;` declarations.

### Rewritten file: `src/tools/file_ops.rs`

All tools were rewritten to accept `Arc<PathValidator>` and implement the
updated `ToolExecutor` trait.

Read-only tools:

| Tool                        | Name                      | Description                                    |
| --------------------------- | ------------------------- | ---------------------------------------------- |
| `ReadFileTool`              | `read_file`               | Read the full contents of a file               |
| `ListDirectoryTool`         | `list_directory`          | List directory entries as `type:name`          |
| `SearchFileContentsTool`    | `search_file_contents`    | Return matching lines with `L{n}: {line}`      |
| `FindFilesByGlobTool`       | `find_files_by_glob`      | Walk a tree and match filenames against a glob |
| `ReadScanArtifactTool`      | `read_scan_artifact`      | Read a scan artifact YAML file verbatim        |
| `ReadWorkspaceMetadataTool` | `read_workspace_metadata` | Parse YAML and return pretty JSON              |

Write tools:

| Tool                      | Name                    | Description                                   |
| ------------------------- | ----------------------- | --------------------------------------------- |
| `WriteFileTool`           | `write_file`            | Write content to a file                       |
| `CreateDirectoryTool`     | `create_directory`      | Create a directory with `create_dir_all`      |
| `WriteReportArtifactTool` | `write_report_artifact` | Write a report artifact file                  |
| `AppendDiagnosticTool`    | `append_diagnostic`     | Append a YAML diagnostic entry with timestamp |

The `FindFilesByGlobTool` uses `ignore::WalkBuilder` for directory traversal and
a custom `matches_pattern` function for glob matching. The pattern is applied to
the filename only. Supported wildcards: `*` matches any sequence of characters;
exact strings without `*` require an exact match.

`AppendDiagnosticTool` formats entries as:

```/dev/null/diagnostic.yaml#L1-3
- message: "..."
  level: "warning"
  timestamp: "2024-..."
```

### Updated file: `src/tools/registry.rs`

- Added `register_executor` convenience method that calls `tool_definition()` on
  the executor and delegates to `register`.
- Added four builder functions:
  - `build_read_only_registry` - six read-only tools
  - `build_read_write_registry` - read-only plus four write tools
  - `build_subagent_registry` - currently identical to read-write
  - `build_mcp_augmented_registry` - read-write plus caller-supplied MCP tools

### Updated file: `src/tools/git_ops.rs`

Added `tool_definition(&self) -> Tool` to `GitStatusTool`'s `ToolExecutor` impl,
delegating to the existing static `definition()` method.

### Updated file: `src/tools/doc_ops.rs`

Added module-level doc comment. Reserved for future document-oriented tools.

### Updated file: `tests/unit/tool_tests.rs`

Updated integration tests to use the new `PathValidator`-based API and
`register_executor` instead of the old unit-struct approach.

## Quality Gate Results

All four gates passed with zero failures:

- `cargo fmt --all` - no formatting changes required
- `cargo check --all-targets --all-features` - zero errors
- `cargo clippy --all-targets --all-features -- -D warnings` - zero warnings
- `cargo test --all-features` - 722 unit tests + 79 integration tests + 179
  doctests all passed

## Issues Encountered

Two issues required fixing after the initial implementation:

1. The existing `tests/unit/tool_tests.rs` used the old unit-struct API
   (`ReadFileTool::definition()`, `Arc::new(ReadFileTool)`). It was updated to
   use `PathValidator` and `register_executor`.

2. Clippy flagged three nested `if let` blocks in `WriteFileTool`,
   `WriteReportArtifactTool`, and `AppendDiagnosticTool`. These were collapsed
   into let-chains using the Rust 2024 edition syntax
   (`if let Some(x) = y && let Err(e) = z`).
