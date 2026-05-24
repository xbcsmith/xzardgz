# Phase 4 Workflow Plan Model Implementation

## Overview

This document describes the Phase 4 plugin-first workflow plan model
introduced in `src/workflow/`. The previous model was oriented around a
fixed set of action types (`ScanRepository`, `AnalyzeCode`, `RunPlugin`,
`ExecuteCommand`, `AgentTask`). Phase 4 replaces that with a uniform
plugin-first model where every step invokes a named plugin with optional
per-step configuration.

---

## Changed Files

| File | Change |
| --- | --- |
| `src/workflow/plan.rs` | Full rewrite: new `WorkflowPlan`, `PluginStep`, `PlanScanOptions`, `PlanReportOptions` types |
| `src/workflow/validator.rs` | New file: legacy-action detection, plan validation wrapper, direct invocation builder |
| `src/workflow/mod.rs` | Added `pub mod validator` |
| `src/workflow/parser.rs` | Rewritten: uses `WorkflowPlan`, calls legacy guard and validator in every parser |
| `src/workflow/executor.rs` | Rewritten: dispatches on `PluginStep.plugin`, supports dry-run mode |
| `tests/unit/parser_tests.rs` | Rewritten: all tests use the new plan format, `Action` import removed |

---

## New Type Model

### `WorkflowPlan`

The top-level plan struct. Schema version is required and must equal `"1"`.
The struct is serialized/deserialized with `serde` supporting both YAML and
JSON. Key fields:

- `version` - must equal `PLAN_VERSION` (`"1"`); plans with other values are
  rejected at validation time.
- `repository` - required local path or remote URL.
- `steps` - at least one `PluginStep` is required.
- `dry_run` - when `true`, steps are logged but plugins are not invoked.
- `resume` - when `true`, the executor picks up from existing workspace state.
- Optional overrides: `branch`, `workspace`, `provider`, `model`, `scan`,
  `reports`.

### `PluginStep`

The only step type in version-1 plans. Each step names a plugin and may
declare:

- `dependencies` - IDs of steps that must finish before this one starts.
- `config` - arbitrary `serde_json::Value` passed to the plugin.
- `report_formats`, `max_findings`, `severity_threshold` - per-step output
  controls that override plan-level defaults.

### `PlanScanOptions`

Optional scanner configuration overrides: `include_hidden`,
`max_file_size_bytes`, `ignore_patterns`.

### `PlanReportOptions`

Optional report output configuration: `output_dir`, `formats`, `overwrite`.

---

## Validation Pipeline

Every plan goes through a three-stage pipeline before being used:

1. **Legacy action guard** (`check_for_legacy_actions` in `validator.rs`) -
   runs against the raw string before deserialization. Detects YAML pattern
   `type: <legacy>` and JSON pattern `"type": "<legacy>"` for the set:
   `scan_repository`, `analyze_code`, `run_plugin`, `execute_command`,
   `agent_task`, `generate_docs`. Returns a `PipelineError::Workflow` with
   a migration hint on match.

2. **Deserialization** - `serde_yaml` or `serde_json` converts the raw string
   into a `WorkflowPlan`. Required fields (`version`, `name`, `repository`,
   `steps[].id`, `steps[].plugin`) cause a deserialization error if absent.

3. **Structural validation** (`WorkflowPlan::validate`) - checks version
   equality, non-empty name, at least one step, no duplicate step IDs, no
   empty plugin names, and that all dependency references resolve to existing
   step IDs.

The three parsers (`YamlPlanParser`, `JsonPlanParser`, `MarkdownPlanParser`)
all run this full pipeline. The `MarkdownPlanParser` extracts the first fenced
code block then delegates to `parse_plan` which routes to the YAML or JSON
parser.

---

## Executor Changes

`WorkflowExecutor` now accepts a `WorkflowPlan` instead of the old `Plan`. The
`execute_step` method dispatches on `step.plugin` (a plain string) instead of
matching on the `Action` enum. Dry-run mode is checked at the start of each
step: when active, a `[dry-run]` log line is emitted and the step returns
`Ok(())` without invoking any plugin logic.

The dependency-ordering algorithm is unchanged: each iteration collects all
steps whose dependency IDs are in `completed_steps`, executes them, then marks
them complete. A deadlock error is returned if the loop stalls before all steps
finish.

---

## Direct Plugin Invocation

`WorkflowPlan::direct_plugin_invocation(plugin, repository)` creates a
minimal valid plan with a single step. This is used when the user runs
`xzardgz run --plugin <name>` without a plan file.

`build_direct_invocation_plan` in `validator.rs` is the CLI-facing variant
that additionally accepts all common CLI flag overrides (`branch`, `provider`,
`model`, `workspace`, `dry_run`, `max_findings`, `report_formats`).

---

## Test Coverage

### `src/workflow/plan.rs` (inline tests)

- `test_workflow_plan_validate_accepts_valid_plan`
- `test_workflow_plan_validate_rejects_wrong_version`
- `test_workflow_plan_validate_rejects_empty_name`
- `test_workflow_plan_validate_rejects_empty_steps`
- `test_workflow_plan_validate_rejects_duplicate_step_ids`
- `test_workflow_plan_validate_rejects_unknown_dependency`
- `test_workflow_plan_is_dry_run_returns_flag_value`
- `test_direct_plugin_invocation_creates_single_step_plan`

### `src/workflow/validator.rs` (inline tests)

- `test_check_for_legacy_actions_rejects_scan_repository`
- `test_check_for_legacy_actions_rejects_analyze_code`
- `test_check_for_legacy_actions_rejects_generate_docs`
- `test_check_for_legacy_actions_rejects_execute_command`
- `test_check_for_legacy_actions_accepts_plugin_first_content`
- `test_build_direct_invocation_plan_creates_valid_plan`

### `src/workflow/parser.rs` (inline tests)

- `test_yaml_parser_parses_valid_plugin_first_plan`
- `test_json_parser_parses_valid_plugin_first_plan`
- `test_yaml_parser_rejects_legacy_scan_repository_action`
- `test_yaml_parser_rejects_legacy_generate_docs_action`
- `test_yaml_parser_rejects_wrong_version`
- `test_markdown_parser_parses_valid_plan`
- `test_parse_plan_rejects_unsupported_format`

### `tests/unit/parser_tests.rs` (integration tests)

- `test_yaml_parser_parses_plugin_first_plan_correctly`
- `test_json_parser_parses_plugin_first_plan_correctly`
- `test_markdown_parser_parses_plan_from_yaml_fenced_block`
- `test_yaml_parser_rejects_legacy_scan_repository_action`
- `test_yaml_parser_rejects_legacy_generate_docs_action`
- `test_yaml_parser_rejects_wrong_version`
- `test_parse_plan_rejects_unsupported_format`

---

## Design Decisions

### Why detect legacy actions on the raw string

Detecting legacy action types before deserialization provides a clear,
actionable error message. If the old `Action` enum variants were simply
removed from the type and deserialization attempted, the user would receive an
opaque `unknown field` or `unknown variant` error with no migration guidance.
The pre-deserialization guard catches both YAML and JSON forms with targeted
patterns and returns a message that names the specific legacy type and
describes the correct replacement (`plugin: <name>` field).

### Why `validate` is on `WorkflowPlan` and also wrapped in `validator.rs`

`WorkflowPlan::validate` owns the logic so that any code with a plan can
validate it without importing the validator module. `validator::validate_plan`
is a thin wrapper that gives the parser pipeline a uniform calling convention
and makes the three-stage pipeline easy to read in sequence.

### Why `#[allow(clippy::too_many_arguments)]` on `build_direct_invocation_plan`

The nine parameters correspond one-to-one to independent CLI flags. Introducing
a builder struct or a config object would push the complexity boundary
outward without reducing it. The allow attribute is accompanied by a comment
explaining the rationale.
