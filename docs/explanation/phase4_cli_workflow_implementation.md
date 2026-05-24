# Phase 4: CLI, Command Routing, and Workflow Model Implementation

## Overview

Phase 4 delivers the complete first-release CLI surface and replaces the legacy
Doc Gen-oriented workflow plan model with a plugin-first model. All seven
product commands are routed through thin handlers, and the old action-based plan
format is formally rejected.

---

## What Changed

### CLI Restructure (`src/cli.rs`)

The previous CLI had a shallow, placeholder-only surface. Phase 4 replaces it
with a fully-specified argument tree covering all first-release commands.

**Global flags**

- `--verbose` / `-v` (count): increases log verbosity.
- `--config` / `-c`: path to configuration file override.

**Subcommands**

| Command   | Argument type     | Description                                   |
| --------- | ----------------- | --------------------------------------------- |
| `run`     | `RunArgs`         | Execute a workflow plan or direct plugin call |
| `scan`    | `ScanArgs`        | Scan a repository and emit a scan artifact    |
| `plugin`  | `PluginCommands`  | List, schema, run, validate, or show formats  |
| `watch`   | `WatchArgs`       | Start watcher mode for event-driven execution |
| `auth`    | `AuthCommands`    | Manage provider credentials                   |
| `prompts` | `PromptsCommands` | Manage prompt templates                       |
| `mcp`     | `McpCommands`     | Manage MCP server and tool configuration      |

The top-level `command` field is required (not `Option`). When the binary is
invoked with no subcommand, clap prints help and exits without an error.

**`RunArgs` key fields**

- `--plan` / `-p`: path to a plan file (optional when `--plugin` is given).
- `--plugin`: plugin name for direct invocation without a plan file.
- `--repository` / `-r`: repository path or URL (defaults to `.`).
- `--branch` / `-b`: target branch override.
- `--provider` / `--model`: provider and model overrides.
- `--dry-run` / `-n`: perform a dry run with no side effects.
- `--workspace` / `-w`: workspace directory override.
- `--output-dir` / `-o`: report output directory override.
- `--openai-endpoint`: OpenAI-compatible API endpoint URL.
- `--ollama-host`: Ollama host URL override.
- `--insecure`: allow HTTP (non-TLS) provider endpoints.
- `--scan-artifact`: use an existing scan artifact (skip scanning).
- `--trace-transcript`: enable session transcript tracing.
- `--max-findings`: cap the number of reported findings.
- `--report-format` / `-f`: comma-separated output formats (json, markdown,
  sarif).
- `--resume`: resume from an existing workspace state.

**`AuthProvider` value enum**

`AuthProvider` implements `clap::ValueEnum` so shell completion, error messages,
and parsing are handled by clap automatically. Values: `openai`, `anthropic`,
`copilot`, `ollama`.

**Legacy commands removed**

`chat` and `generate` are not defined. Any attempt to use them returns a clap
parse error.

---

### Workflow Plan Model (`src/workflow/plan.rs`)

The previous `Plan` / `WorkflowStep` / `Action` model is replaced with a
plugin-first schema versioned at `"1"`.

**`WorkflowPlan`**

| Field        | Type                        | Purpose                                          |
| ------------ | --------------------------- | ------------------------------------------------ |
| `version`    | `String`                    | Must equal `"1"`. All other values are rejected. |
| `name`       | `String`                    | Human-readable name. Must not be empty.          |
| `repository` | `String`                    | Path or URL of the target repository.            |
| `branch`     | `Option<String>`            | Target branch (default branch when omitted).     |
| `workspace`  | `Option<String>`            | Workspace directory for intermediate artifacts.  |
| `provider`   | `Option<String>`            | Provider override.                               |
| `model`      | `Option<String>`            | Model identifier override.                       |
| `scan`       | `Option<PlanScanOptions>`   | Scanner configuration overrides.                 |
| `steps`      | `Vec<PluginStep>`           | Plugin execution steps (at least one required).  |
| `reports`    | `Option<PlanReportOptions>` | Report output configuration.                     |
| `dry_run`    | `bool`                      | Skip provider calls and report writes.           |
| `resume`     | `bool`                      | Resume from existing workspace state.            |

**`PluginStep`**

Each step specifies exactly one `plugin` name (e.g., `"technical-review"`) and
optional `config`, `dependencies`, `report_formats`, `max_findings`, and
`severity_threshold`. There is no `action` field. The old action types
(`scan_repository`, `analyze_code`, `execute_command`, `agent_task`) do not
exist in version 1 plans.

**Convenience constructors**

- `WorkflowPlan::direct_plugin_invocation(plugin, repository)`: builds a valid
  single-step plan for use when the user runs `xzardgz run --plugin`.
- `WorkflowPlan::validate()`: enforces version, non-empty name, non-empty steps,
  unique IDs, non-empty plugin names, and resolvable dependency references.
- `WorkflowPlan::is_dry_run()`: returns the `dry_run` field.

---

### Workflow Validator (`src/workflow/validator.rs`)

Three public functions support the three-stage parse pipeline:

**`check_for_legacy_actions(raw_content)`**

Scans raw plan content for YAML (`type: <legacy>`) and JSON
(`"type": "<legacy>"`) patterns before deserialization. Detected legacy types:

- `scan_repository`
- `analyze_code`
- `run_plugin` (old action-type form)
- `execute_command`
- `agent_task`
- `generate_docs`

Returns `PipelineError::Workflow` with a migration hint on detection.

**`validate_plan(plan)`**

Thin wrapper around `WorkflowPlan::validate()`.

**`build_direct_invocation_plan(...)`**

Constructs a `WorkflowPlan` from nine CLI flag values for direct plugin
invocation. Handles report format propagation to both the step and plan-level
`reports` block.

---

### Workflow Parser (`src/workflow/parser.rs`)

All parsers now operate on `WorkflowPlan` and run a three-stage pipeline:

1. `check_for_legacy_actions(raw_content)` - reject on legacy detection.
2. Deserialize YAML or JSON into `WorkflowPlan`.
3. `validate_plan(&plan)` - reject on structural errors.

Supported formats: `yaml`, `yml`, `json`, `md`, `markdown`.

---

### Workflow Executor (`src/workflow/executor.rs`)

The executor is updated to use `WorkflowPlan` and `PluginStep`. The
`execute_step` method dispatches on `step.plugin` (a string) rather than an
`Action` enum. Dry-run support is added via `plan.is_dry_run()`.

---

### Command Handlers (`src/commands/`)

Each handler accepts the matching CLI arg type and routes to the appropriate
module logic. All handlers return `Result<()>` and use `?` for error
propagation.

| Handler      | Signature                           | Key behavior                                             |
| ------------ | ----------------------------------- | -------------------------------------------------------- |
| `run.rs`     | `execute(args: RunArgs)`            | Requires `--plan` or `--plugin`; validates plan; dry-run |
| `scan.rs`    | `execute(args: ScanArgs)`           | Loads config; prints scan parameters                     |
| `plugin.rs`  | `execute(command: PluginCommands)`  | Routes list/schema/run/validate/formats                  |
| `watch.rs`   | `execute(args: WatchArgs)`          | Loads config; applies overrides; honours once/dry-run    |
| `auth.rs`    | `execute(command: AuthCommands)`    | Routes login/logout/status/validate/set-key/remove-key   |
| `prompts.rs` | `execute(command: PromptsCommands)` | Routes export/validate/show-order/list-templates/render  |
| `mcp.rs`     | `execute(command: McpCommands)`     | Routes validate/list-servers/list-tools/test-\*          |

The old `commands::auth::login()` function is removed. The old `CopilotAuth`
import is no longer present in the commands layer.

**`run.rs` validation logic**

```text
if args.plan.is_none() AND args.plugin.is_none():
    return Err(PipelineError::Workflow("run requires either --plan or --plugin"))

if args.plan.is_some():
    read file -> check_for_legacy_actions -> parse -> validate_plan

if args.plugin.is_some():
    build_direct_invocation_plan(...) -> validate_plan
```

---

### Main (`src/main.rs`)

Routing is now a single `match cli.command` with one arm per `Commands` variant.
Each arm passes the arg struct or subcommand enum directly to the corresponding
handler. No intermediate destructuring or legacy fallback arms.

---

### `sample_plan.yaml`

Updated to the version 1 plugin-first format with a single `technical-review`
step, plan-level `reports` configuration, `dry_run: false`, and `resume: false`.

---

## Test Coverage

| Location                               | Tests | Coverage                                      |
| -------------------------------------- | ----- | --------------------------------------------- |
| `src/cli.rs`                           | 43    | All commands, all flags, legacy rejections    |
| `src/workflow/plan.rs`                 | 8     | Validation, dry-run, direct invocation        |
| `src/workflow/validator.rs`            | 6     | Legacy detection, plan builder                |
| `src/workflow/parser.rs`               | 7     | New format parsing, legacy rejection, formats |
| `src/commands/run.rs`                  | 2     | Missing args rejection, direct invocation     |
| `src/commands/scan.rs`                 | 1     | Default args                                  |
| `src/commands/plugin.rs`               | 5     | All subcommands                               |
| `src/commands/watch.rs`                | 3     | dry-run, once, default                        |
| `src/commands/auth.rs`                 | 6     | All subcommands                               |
| `src/commands/prompts.rs`              | 5     | All subcommands                               |
| `src/commands/mcp.rs`                  | 5     | All subcommands                               |
| `tests/unit/cli_tests.rs`              | 10    | Phase 1 contract, legacy rejection            |
| `tests/unit/parser_tests.rs`           | 7     | New format, legacy rejection, format errors   |
| `tests/unit/workflow_validation_tests` | 15    | Full validation, builder, legacy detection    |

**Total: 211 tests (137 lib + 64 integration + 10 doc) - all pass.**

---

## Quality Gate Results

```text
cargo fmt --all                                        PASS
cargo check --all-targets --all-features              PASS (0 warnings)
cargo clippy --all-targets --all-features -- -D warnings   PASS (0 warnings)
cargo test --all-features                             PASS (211 tests, 0 failed)
```

---

## Design Decisions

### CLI arg types passed directly to handlers

Command handlers accept the CLI arg struct (e.g., `RunArgs`) rather than
individual primitive values. This keeps `main.rs` as a one-line-per-command
dispatcher and makes handlers independently testable by constructing arg structs
directly.

### Legacy plan format detection before deserialization

`check_for_legacy_actions` scans raw content before serde runs. This gives clear
migration messages instead of cryptic deserialization failures when users pass
old plan files.

### `run` command: plan-or-plugin required at runtime, not at parse time

The mutual requirement of `--plan` or `--plugin` is enforced in the handler
rather than with clap's `conflicts_with` / `requires` attributes. This
simplifies the CLI definition and produces a cleaner error message.

### `AuthProvider` as `ValueEnum`

Using clap's `ValueEnum` gives shell completions, normalized error messages, and
no manual string-matching in handlers.

### Version 1 plans only

Plans without `version: "1"` are rejected by `WorkflowPlan::validate`. There is
no migration path; users must update plan files manually. This is intentional:
old plan files contained fundamentally different action semantics and cannot be
automatically translated.
