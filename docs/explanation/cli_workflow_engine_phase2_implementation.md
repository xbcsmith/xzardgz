# CLI-to-Workflow-Engine Integration: Phase 2 Implementation

This document records what was built for Phase 2 ("Wire CLI Handlers to the
Executor") of
[`cli_workflow_engine_integration_plan.md`](cli_workflow_engine_integration_plan.md),
following on from
[`cli_workflow_engine_phase1_implementation.md`](cli_workflow_engine_phase1_implementation.md).

## Summary

`src/commands/run.rs`, `src/commands/scan.rs`, and `src/commands/plugin.rs`
(specifically its `run_plugin` handler) previously parsed and validated CLI
arguments, printed a summary, and returned `Ok(())` without ever calling
`WorkflowExecutor`. All three now build a real `WorkflowExecutor` and dispatch
to the appropriate `ExecutionInput` variant from Phase 1 (`LocalPlan`,
`ScanOnly`, `PluginOnly`), producing real workspaces and real report files on
disk.

## A shared design pattern, established once and reused by all three files

Every rewritten command handler follows the same split:

- A thin `pub async fn execute(args) -> Result<()>` (or `run_plugin` for the
  `plugin run` subcommand) that calls `Config::load()` and, where a plugin is
  involved, `PluginRegistry::with_builtins()`, then delegates to...
- A fully `pub async fn execute_with(args, config, ...)` (`run_plugin_with` for
  `plugin.rs`) that takes the config (and registry, where relevant) as
  parameters instead of constructing them internally.

This is not a test-only shim -- `execute_with`/`run_plugin_with` are the real,
documented, public entry points that do all the work; the thin wrapper is sugar
over them. The split exists because `Config::load()` reads a fixed `config.yaml`
path relative to the current working directory with no override mechanism, and
the real plugins (`PluginRegistry::with_builtins()`) make genuine network calls
to an AI provider -- neither is acceptable inside a hermetic test. Injecting
both lets tests exercise the real command logic end-to-end against a temp
workspace and either a fake plugin or a `wiremock`-stubbed provider endpoint,
rather than only testing argument parsing.

`crate::commands::run::print_execution_result` was made `pub(crate)` so all
three handlers share one human-readable result-printing implementation instead
of three slightly-diverging copies.

## A gap discovered and fixed along the way: `PluginRegistry` never registered real plugins

`src/plugins/registry.rs`'s own module doc comment claimed "Built-in plugins are
registered at construction," but `PluginRegistry::new()` has always returned an
empty registry -- nothing in the codebase, in any non-test code path, ever
registered `TechnicalReviewPlugin` or `SecurityReviewPlugin`. Without fixing
this, wiring the CLI to the executor would have made
`xzardgz run --plugin technical-review` fail with "plugin not found" for every
real invocation.

Rather than change `PluginRegistry::new()`'s behavior (which many existing tests
depend on being empty), a new `PluginRegistry::with_builtins()` associated
function was added, registering both built-in plugins under their canonical
names. The module doc comment was corrected to describe the actual, now-accurate
contract. All three command handlers use `with_builtins()`; `new()` remains
empty and is used only by tests that register fakes.

## A second gap discovered and fixed: `--output-dir` was silently inert

`ExecutionInput::PluginOnly` (from Phase 1) added an `output_dir` field per
Phase 2.3's requirement that `--output-dir` map onto an executor-consumed field.
While wiring it, `write_step_reports` (the function both `run_plan` and
`run_plugin_only` share for writing report files) turned out to always write
into `workspace.paths.step_reports_dir(&step.id)` regardless of
`plan.reports.output_dir` -- the field was structurally present on
`WorkflowPlan` but never read anywhere. This meant `--output-dir` would have
been a silent no-op for every invocation shape, including the already-existing
`run --plan`/`run --plugin` paths, not just the new `PluginOnly` variant.
`write_step_reports` now prefers `plan.reports.output_dir` when set, falling
back to the workspace default otherwise -- fixed once, centrally, benefiting
every execution path rather than only the new one.

## `commands::run`

- `execute_with` dispatches on `args.plan`/`args.plugin`/`args.scan_artifact`: a
  plan file always runs as `LocalPlan`; a bare `--plugin` builds a single-step
  plan via the existing `build_direct_invocation_plan` and also runs as
  `LocalPlan`; `--plugin` combined with `--scan-artifact` runs as `PluginOnly`
  instead, skipping the scan stage entirely rather than paying for (and
  requiring) a fresh scan the caller has already said is unnecessary.
- A new `apply_run_overrides` function (`src/workflow/validator.rs`) applies
  `--branch`, `--workspace`, `--dry-run`, `--resume`, `--max-findings`,
  `--report-format`, and `--output-dir` onto an already-parsed-or-built
  `WorkflowPlan` in place, used uniformly for both the plan-file and
  direct-plugin-invocation paths (the latter also still passes most of these
  through `build_direct_invocation_plan`'s own parameters for its pre-existing
  call sites' sake; `apply_run_overrides` additionally covers `--resume` and
  `--output-dir`, which that function does not accept).
- `--openai-endpoint`, `--insecure`,
  `--scan-artifact`-as-a-standalone-flag-on-`--plan`-mode, and
  `--trace-transcript` remain unwired, matching Phase 2.3's explicit flag list
  (`--resume`, `--workspace`, `--output-dir`, `--report-format`,
  `--max-findings`) -- these four were pre-existing no-ops before this phase and
  are not claimed to be fixed here.

## `commands::scan`

- `execute_with` maps `ScanArgs` onto `ExecutionInput::ScanOnly` directly (1:1
  field mapping, including the `resume` field added to `ScanArgs` in this phase
  specifically so Phase 1's resume support on `ScanOnly` is actually reachable
  from the CLI).
- `args.format` (json/yaml artifact format) and `args.overwrite` remain
  documented no-ops: `ExecutionInput::ScanOnly` always writes YAML and always
  overwrites unconditionally. Neither is in Phase 2.3's flag list; implementing
  them would have been scope creep into the executor's scan-artifact-writing
  logic, which this phase does not touch.

## `commands::plugin` (`plugin run`)

- `run_plugin_with` maps `PluginRunArgs` onto `ExecutionInput::PluginOnly`.
  `args.config` is treated as a file path (per its own doc comment) and parsed
  via `serde_yaml::from_str::<serde_json::Value>`, which handles both YAML and
  JSON content since JSON is a syntactic subset of YAML.
- `PluginCommands::List`, `Schema`, `Validate`, and `Formats` are deliberately
  untouched -- only `Run` (`run_plugin`) was in Phase 2's scope, per the plan's
  own "Identified Issues" section. `Schema` still prints "implemented in a later
  phase"; this is a known, pre-existing, intentionally out-of-scope gap, not an
  oversight.

## Testing

Each command file's own `#[cfg(test)] mod tests` was rewritten to use
`tempfile::TempDir` for every repository and workspace path (never `"."` or a
real crate-relative path) and to assert real filesystem side effects (a
workspace directory exists, a report or artifact file exists and is non-empty)
rather than only `result.is_ok()`, per Phase 2.4. Tests that exercise a plugin
use a local fake `WorkflowPlugin` (`SuccessPlugin`) via `PluginRegistry::new()`,
never `with_builtins()`, so no unit test makes a network call.

A new integration test file,
[`tests/integration/run_command_tests.rs`](../../tests/integration/run_command_tests.rs)
(registered in `tests/integration.rs`), satisfies Phase 2.6's literal success
criterion: `test_run_command_direct_plugin_invocation_produces_real_report`
calls the real `commands::run::execute_with` with the real
`PluginRegistry::with_builtins()` (i.e. the actual `TechnicalReviewPlugin`, not
a fake) against a `wiremock`-stubbed OpenAI-compatible endpoint, mirroring the
exact pattern already proven in `tests/integration/workflow_tests.rs`'s
`test_local_technical_review_with_mock_openai` (request/response shape,
`config.openai.allow_insecure_endpoint = true`, a test-unique `api_key_env`
name). It asserts a real workspace directory, a real `artifact.yaml`, and a real
`technical_review.json` report file all exist on disk afterward. A second test,
`test_run_command_unregistered_plugin_returns_actionable_error`, covers the
"clear, actionable CLI error rather than a silent no-op" half of Phase 2.6's
success criterion for the one failure mode that exists today.

### The "provider without tool-calling support" clause is not yet applicable

Phase 2.6 also states that running the configured provider without tool-calling
support should surface a clear, actionable CLI error. Neither
`TechnicalReviewPlugin` nor `SecurityReviewPlugin` calls
`Provider::chat_with_tools` or checks `capabilities.tools` anywhere today --
both call `Provider::complete` directly, single-shot, and work identically
regardless of a provider's tool-calling support. There is consequently no
current scenario where a lack of tool-calling support causes a silent no-op to
fix. This clause becomes actionable once the companion agent tool-calling
integration plan lands (which makes tool-calling a hard requirement for plugin
execution); it is out of scope for this phase and is not claimed to be
implemented here.

## Verification

Full quality-gate sequence run in the mandated order, all clean:

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Result: 1453 lib unit tests, 79 workspace tests, 28 integration tests
(`integration.rs`, up from 26 before this phase), and 385 doctests all passed, 0
failed. Both of Phase 1's and Phase 2's literal success criteria hold:

```text
grep -rn "WorkspaceManager::\|Scanner::new" src/commands/
```

returns no matches, and the new `tests/integration/run_command_tests.rs` test
exercises a real `run --plugin` invocation producing a real workspace directory
with a written report, as required.
