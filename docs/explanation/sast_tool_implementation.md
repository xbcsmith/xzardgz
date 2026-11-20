# Phase 7: SAST Tool Implementation

This document describes the design and implementation of `SastTool`, the Phase 7
agent-callable wrapper around the SAST scanning pipeline.

## Overview

`SastTool` implements the `ToolExecutor` trait and exposes the name `sast_scan`
to the agent tool registry. It allows an AI agent to trigger a full SAST scan
against any directory path and receive the results in a structured format.

## Location

```text
src/tools/sast_tool.rs
```

## Boundary Constraint

`src/tools/sast_tool.rs` must not import from `crate::clients`. It is a pure
orchestration layer that calls into `crate::scanner::sast` only.

## Parameters

| Field                | Type     | Required | Default                                  |
| -------------------- | -------- | -------- | ---------------------------------------- |
| `path`               | string   | yes      | -                                        |
| `rulesets`           | string[] | no       | `["builtin:crypto", "builtin:security"]` |
| `format`             | string   | no       | `"json"`                                 |
| `severity_threshold` | string   | no       | `"warning"`                              |
| `jobs`               | integer  | no       | `0` (automatic)                          |

## Execution Flow

1. Extract `path` from the JSON parameter object; return a failure if absent.
2. Extract optional `rulesets`, defaulting to both embedded builtin sets.
3. Extract optional `format` (`"json"`, `"sarif"`, `"cyclonedx"`).
4. Extract optional `severity_threshold` (`"info"`, `"warning"`, `"error"`).
5. Extract optional `jobs` integer.
6. Build a `ResolveConfig` and call `resolve_rules`.
7. Extract `Vec<RuleIr>` from the returned `Vec<NamespacedRule>`.
8. Build `SastEngineConfig` with the requested `jobs` value.
9. Initialise `SastEngine` and load the compiled rules.
10. Call `engine.scan(Path::new(&path))`.
11. Apply the severity threshold filter to `report.matches`.
12. Truncate `report.matches` to at most 500 findings.
13. Serialise the result:
    - `"sarif"`: `render_sarif(&report)` serialised to JSON.
    - `"cyclonedx"`: `render_cyclonedx(&report)` serialised to JSON.
    - `"json"` (default): `report.matches` serialised as a JSON array.
14. Return `ToolResult::success(serialized)`.

All errors at steps 6, 9, 10, 11, and 13 are returned as
`ToolResult::failure(message)` rather than propagated as `Err`, so the agent
loop can continue without crashing.

## Severity Filtering

The private `severity_passes` function implements a numeric comparison:

```text
info=0 < warning=1 < error=2
```

An unrecognised threshold string defaults to `"warning"` to fail safe.

## Registry Registration

`SastTool` is registered in `build_read_only_registry` inside
`src/tools/registry.rs`. Because `build_read_write_registry` and
`build_subagent_registry` both call `build_read_only_registry` as their base,
all three registry types include `sast_scan` automatically.

## Tests

| Test name                                                       | Purpose                                      |
| --------------------------------------------------------------- | -------------------------------------------- |
| `test_sast_tool_definition_name_is_sast_scan`                   | Verifies the tool name string.               |
| `test_sast_tool_definition_parameters_has_path_property`        | Verifies the JSON Schema has a `path` field. |
| `test_sast_tool_execute_with_valid_empty_path_returns_ok`       | Scans an empty temp directory; expects `Ok`. |
| `test_sast_tool_execute_with_invalid_path_returns_error`        | Non-existent path; expects failure or `[]`.  |
| `test_sast_tool_execute_missing_path_param_returns_failure`     | Empty params; expects `error.is_some()`.     |
| `test_sast_tool_execute_does_not_import_from_clients`           | Documents the clients boundary.              |
| `test_severity_passes_info_threshold_accepts_all_levels`        | Info threshold lets all severities through.  |
| `test_severity_passes_warning_threshold_rejects_info`           | Warning threshold rejects info.              |
| `test_severity_passes_error_threshold_rejects_info_and_warning` | Error threshold rejects info and warning.    |
| `test_severity_passes_unknown_threshold_defaults_to_warning`    | Unknown threshold behaves like warning.      |
