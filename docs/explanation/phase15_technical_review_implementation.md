# Phase 15: Technical Review Plugin Implementation

## Overview

Phase 15 implements the `technical-review` plugin for the XZardgz pipeline. This
plugin performs a multi-dimensional architectural and code quality analysis of a
repository using an AI provider. It reads the scan artifact produced by the
scanner module, selects and prioritizes files, builds structured prompts, calls
the AI provider, parses structured JSON findings, and emits human-readable and
machine-readable reports.

The plugin addresses a core pipeline need: automated technical review at scale.
Manual architectural review is expensive and inconsistent across teams. Driving
AI analysis through a structured 14-dimension framework produces comparable,
repeatable assessments that feed into the broader pipeline risk score and
governance workflow.

---

## Architecture

The `TechnicalReviewPlugin` is a concrete implementation of the `WorkflowPlugin`
trait defined in `src/plugins/trait_def.rs`. It is registered in
`PluginRegistry` under the name `"technical-review"` and can be invoked from two
entry points:

- **CLI**: `xzardgz run --plugin technical-review`
- **Watcher**: a `WatcherTaskMessage` with `event_type` set to
  `xzardgz.technical_review.task` routes through `WatcherExecutor` to the
  registry and then to the plugin.

The plugin is entirely self-contained within `src/plugins/technical_review/`. It
does not modify any shared infrastructure module. All output is produced through
the standard `PluginOutput` and `PluginContext` interfaces established in
Phase 13.

The sub-modules are arranged in a strict layered dependency order:

```text
config.rs       -> (standalone: serde, config crate types)
finding.rs      -> config.rs (FindingSeverity from reports)
prioritizer.rs  -> finding.rs, scanner::ScanResult
prompt.rs       -> config.rs, finding.rs, prioritizer.rs
plugin.rs       -> config.rs, finding.rs, prioritizer.rs, prompt.rs,
                   plugins::PluginContext, plugins::PluginOutput,
                   reports::ReportEnvelope, providers::Provider
```

No sub-module imports from a module above it in this order.

---

## Module Layout

The plugin is implemented across six focused files:

| File                                          | Purpose                                               |
| --------------------------------------------- | ----------------------------------------------------- |
| `src/plugins/technical_review/mod.rs`         | Public API re-exports                                 |
| `src/plugins/technical_review/config.rs`      | `TechnicalReviewConfig` with all configuration fields |
| `src/plugins/technical_review/finding.rs`     | `TechnicalReviewFinding` and severity mapping         |
| `src/plugins/technical_review/prioritizer.rs` | `FilePrioritizer` - file selection from `ScanResult`  |
| `src/plugins/technical_review/prompt.rs`      | Prompt builder for system and user prompts            |
| `src/plugins/technical_review/plugin.rs`      | `TechnicalReviewPlugin` - trait implementation        |

`src/plugins/mod.rs` is updated to declare `pub mod technical_review;` and
re-export `TechnicalReviewPlugin` so callers can register the plugin without
importing sub-paths directly.

---

## Configuration

`TechnicalReviewConfig` extends the base plugin configuration with fields that
control file selection, AI interaction, and output behavior. It derives
`serde::Deserialize`, `serde::Serialize`, `Clone`, and `Debug`.

### Existing Fields

| Field                | Type          | Default               | Purpose                             |
| -------------------- | ------------- | --------------------- | ----------------------------------- |
| `max_findings`       | `u32`         | `25`                  | Cap on total findings in the report |
| `severity_threshold` | `String`      | `"medium"`            | Minimum severity to include         |
| `focus_areas`        | `Vec<String>` | `[]`                  | Subset of dimensions to activate    |
| `report_formats`     | `Vec<String>` | `["markdown","json"]` | Output formats to generate          |

### New Fields Added in Phase 15

| Field                  | Type             | Default | Purpose                                         |
| ---------------------- | ---------------- | ------- | ----------------------------------------------- |
| `enabled`              | `bool`           | `true`  | Master gate - disables plugin when false        |
| `prompt_dir`           | `Option<String>` | `None`  | Override directory for prompt template files    |
| `max_files`            | `u32`            | `50`    | Maximum files selected for analysis             |
| `include_tests`        | `bool`           | `true`  | Whether test files are included in the file set |
| `include_docs`         | `bool`           | `true`  | Whether documentation files are included        |
| `batch_size`           | `u32`            | `10`    | Files per AI request batch                      |
| `model_override`       | `Option<String>` | `None`  | Override the provider's default model           |
| `verification_turns`   | `u32`            | `1`     | Number of AI verification passes after analysis |
| `confidence_threshold` | `f64`            | `0.7`   | Minimum AI confidence to retain a finding       |

`focus_areas` defaults to an empty `Vec`, which the plugin interprets as all 14
dimensions active. When non-empty, only the named dimension keys are evaluated.

---

## Finding Model

`TechnicalReviewFinding` is a plugin-specific finding type that carries richer
context than the generic `PluginFinding` used across all plugins.

### Comparison to PluginFinding

`PluginFinding` in `src/reports/findings.rs` is a general-purpose finding with
fields for kind, title, severity, location, and tags. It is written to
`ReportEnvelope` as the standard findings collection shared by all plugins.

`TechnicalReviewFinding` extends the base concept with fields suited to
architectural analysis:

| Field            | Type              | Description                                       |
| ---------------- | ----------------- | ------------------------------------------------- |
| `category`       | `String`          | Dimension name (e.g. `"architecture"`)            |
| `severity`       | `FindingSeverity` | `Info`, `Low`, `Medium`, `High`, or `Critical`    |
| `file`           | `Option<String>`  | Repository-relative path to the affected file     |
| `line`           | `Option<u32>`     | Line number within the file, when known           |
| `symbol`         | `Option<String>`  | Function, struct, or module name, when applicable |
| `evidence`       | `String`          | What the AI observed in the code                  |
| `impact`         | `String`          | Why the observation matters                       |
| `recommendation` | `String`          | Actionable remediation step                       |
| `confidence`     | `f64`             | AI confidence score in range `[0.0, 1.0]`         |
| `related_files`  | `Vec<String>`     | Other files affected by the same issue            |
| `references`     | `Vec<String>`     | Links to external documentation or standards      |

`TechnicalReviewFinding` is the AI-native representation. The plugin converts
each instance to a `PluginFinding` before calling `output.add_finding()` so the
standard report infrastructure can process it. The `evidence` text becomes the
finding detail field in the generic type.

---

## Review Dimensions

The plugin evaluates 14 dimensions on every analysis run unless restricted by
`focus_areas`. Each dimension maps to a named category string in finding output.

| Dimension                 | Category Key               | What It Evaluates                                          |
| ------------------------- | -------------------------- | ---------------------------------------------------------- |
| Architecture              | `architecture`             | Layering, coupling, module boundaries, cohesion            |
| Modularity                | `modularity`               | Component separation, single-responsibility adherence      |
| Maintainability           | `maintainability`          | Code clarity, naming, complexity, duplication              |
| Error Handling            | `error_handling`           | Consistent error propagation, use of result types          |
| Testing Posture           | `testing_posture`          | Coverage signals, test organization, test isolation        |
| Dependency Hygiene        | `dependency_hygiene`       | Dependency count, pinning, security posture                |
| CLI Usability             | `cli_usability`            | Command structure, flag naming, help text quality          |
| API Usability             | `api_usability`            | Public interface ergonomics, versioning signals            |
| Configuration Ergonomics  | `configuration_ergonomics` | Config schema clarity, defaults, validation                |
| Observability             | `observability`            | Logging, tracing, metrics, structured diagnostics          |
| Documentation Coverage    | `documentation_coverage`   | Doc comment density, example coverage, README completeness |
| Performance Risks         | `performance_risks`        | Allocation patterns, blocking calls, hot-path complexity   |
| Build and Release Hygiene | `build_release_hygiene`    | CI config, release process, artifact reproducibility       |
| Operational Readiness     | `operational_readiness`    | Health checks, graceful shutdown, runbook completeness     |

When `focus_areas` is non-empty, the plugin passes only the listed dimension
keys to the prompt builder. The AI is instructed to restrict its output to those
categories.

---

## File Prioritization

`FilePrioritizer` is a stateless struct that accepts a reference to `ScanResult`
and a reference to `TechnicalReviewConfig`. Its `prioritize` method returns an
ordered `Vec<String>` of repository-relative file paths, capped at
`config.max_files`.

### Selection Tiers

Files are gathered from `ScanResult.plugin_preselection` data and assigned to
priority tiers. Lower tier numbers are selected first when the total file count
exceeds `max_files`.

| Tier | Category               | Source in ScanResult                    | Configurable    |
| ---- | ---------------------- | --------------------------------------- | --------------- |
| 1    | Entrypoints            | `main.rs`, `lib.rs`, entry markers      | No              |
| 2    | Public APIs            | Exported modules, facade files          | No              |
| 3    | Configuration surfaces | Config struct files, schema files       | No              |
| 4    | Key project files      | `README.md`, `LICENSE`, `Cargo.toml`    | No              |
| 5    | High fan-in signals    | Frequently imported or referenced files | No              |
| 6    | Build files            | Build scripts, CI workflow files        | No              |
| 7    | Dependency manifests   | `Cargo.lock`, `package.json`, etc.      | No              |
| 8    | Test files             | Files in `tests/`, `*_test.rs`          | `include_tests` |
| 9    | Documentation files    | Files in `docs/`, `*.md`                | `include_docs`  |

Within each tier, files are sorted alphabetically to ensure deterministic
ordering across identical inputs. When `include_tests` is false, tier 8 is
skipped entirely. When `include_docs` is false, tier 9 is skipped.

---

## Plugin Execution Flow

The `run` method on `TechnicalReviewPlugin` follows a linear sequence of steps.
Each step can fail with a `PipelineError` that propagates via the `Result`
return type.

1. **Check enabled flag** - return early if `config.enabled` is false.
2. **Load configuration** - extract `TechnicalReviewConfig` from the pipeline
   config.
3. **Determine active dimensions** - use all 14 when `focus_areas` is empty;
   otherwise use the intersection of `focus_areas` with known dimension keys.
4. **Prioritize files** - call `FilePrioritizer::prioritize` to produce an
   ordered file list capped at `max_files`.
5. **Load prompts** - read system and user prompt templates from `ctx.prompts`
   or the `prompt_dir` directory on disk.
6. **Build prompt** - call `PromptBuilder::build` with the file list, active
   dimensions, and repository metadata from `ctx.state`.
7. **Call AI provider** - invoke `ctx.provider.complete` for each file batch
   determined by `config.batch_size`.
8. **Parse response** - deserialize the JSON findings array from the AI response
   text.
9. **Filter findings** - drop findings below `confidence_threshold` or below
   `severity_threshold`.
10. **Run verification passes** - if `verification_turns > 1`, repeat steps 7
    through 9 for additional passes, merging and deduplicating findings.
11. **Cap findings** - truncate the final list to `max_findings`, keeping the
    highest severity findings first.
12. **Convert findings** - convert each `TechnicalReviewFinding` to a
    `PluginFinding` and call `output.add_finding()`.
13. **Write reports** - invoke the appropriate writer for each format in
    `report_formats` and record paths via `output.add_report_path()`.
14. **Return output** - return `PluginOutput::success` with findings, report
    paths, and diagnostics.

---

## AI Integration

### Prompt Construction

The `PromptBuilder` produces two strings: a system prompt and a user prompt.

**System prompt** instructs the AI to act as a senior software architect
performing a structured code review. It specifies:

- The output must be valid JSON matching the findings schema.
- Each finding must include all required fields with no omitted keys.
- Confidence scores must reflect genuine uncertainty, not default to `1.0`.
- The review is restricted to the listed dimensions.

**User prompt** contains three sections:

1. Repository metadata: name, primary language, head commit, branch.
2. Active review dimensions: the list of dimension keys to evaluate.
3. File listing: the ordered paths selected by `FilePrioritizer`.

The user prompt does not embed full file contents. The AI reasons about
architectural signals from file paths, naming patterns, and repository
structure. When the AI provider supports tool calls, file content tools can be
injected via `ctx.tool_registry`.

### Response Format

The AI is instructed to return a JSON object with a single `findings` array:

```json
{
  "findings": [
    {
      "category": "architecture",
      "severity": "high",
      "file": "src/main.rs",
      "line": null,
      "symbol": "main",
      "evidence": "Direct subsystem construction with no inversion-of-control boundary.",
      "impact": "Adding a subsystem requires modifying main directly, increasing coupling.",
      "recommendation": "Introduce a dependency injection container or builder pattern.",
      "confidence": 0.85,
      "related_files": ["src/config.rs", "src/runner.rs"],
      "references": ["https://en.wikipedia.org/wiki/Dependency_injection"]
    }
  ]
}
```

### Response Parsing

The plugin calls `serde_json::from_str` on the full AI response. If the response
contains prose surrounding the JSON object, the parser first attempts to extract
the first `{...}` block using a bracket-depth scan. A parse failure is recorded
as a `Diagnostic` at level `Error` and causes the affected batch to be skipped
without failing the entire run.

---

## Report Generation

The plugin writes two output files per run.

### technical_review.md

A human-readable Markdown report rendered by `MarkdownReportWriter`. The
document structure follows the standard `PluginReportFormatter` output with an
additional section grouping findings by dimension category. Each dimension that
has findings is rendered as an H3 subheading followed by a Markdown table.

```text
# Technical Review: <repository-name>

Generated: <RFC 3339 timestamp>
Risk Band: High

## Summary

| Dimension      | Findings | Highest Severity |
|----------------|----------|------------------|
| Architecture   | 3        | High             |
| Error Handling | 1        | Medium           |

## Findings by Dimension

### Architecture

| Severity | File        | Symbol | Evidence                       | Recommendation          |
|----------|-------------|--------|--------------------------------|-------------------------|
| High     | src/main.rs | main   | Direct subsystem construction. | Introduce DI container. |

## Diagnostics

- [INFO] Analysis completed in 4.2s
```

### technical_review.json

A machine-readable report using the shared `ReportEnvelope` format from
`src/reports/envelope.rs`. All `TechnicalReviewFinding` instances are stored in
`envelope.findings` as `PluginFinding` records. The envelope also carries:

- `plugin_name: "technical-review"`
- `risk_band` derived from the highest finding severity
- `provider_metadata` including model ID, latency, and token usage
- `scan_artifact_version` from the `ScanResult` used during analysis

The JSON file is written by `JsonReportWriter` and can be consumed by downstream
services using the `ReportEnvelope` schema documented in the Phase 13 reports
implementation.

---

## Watcher Integration

When a `WatcherTaskMessage` arrives with event type
`xzardgz.technical_review.task`, `WatcherExecutor` routes the task through the
plugin registry using the plugin name `"technical-review"`. The `plugin` field
of `WatcherTaskMessage` must match this name exactly.

`WatcherExecutor.process_task` builds a `PluginContext` from the task fields and
calls `plugin.run(ctx)`. On completion, it constructs a `WatcherResultMessage`
containing:

- `success` - set from `output.completed`
- `findings_summary` - total finding count and `by_severity` breakdown
- `report_paths` - paths from `output.report_paths`
- `risk_band` - from `output.risk_band`
- `diagnostics` - merged from `output.diagnostics`
- `workspace_id` - from the workspace state
- `provider_metadata` - from `output.provider_metadata`

The result message is published to the Kafka result topic by
`KafkaResultPublisher`. If publication fails, `PublishFailureState` records the
failure to disk so the plugin result is not lost.

---

## Testing Strategy

### Unit Tests

Each sub-module has a `#[cfg(test)] mod tests` block with tests following the
`test_<function>_<condition>_<expected>` naming convention.

| Module           | Key Test Areas                                                             |
| ---------------- | -------------------------------------------------------------------------- |
| `config.rs`      | Default values, serde roundtrip, threshold validation                      |
| `finding.rs`     | Constructor, severity mapping, conversion to `PluginFinding`               |
| `prioritizer.rs` | All nine tiers, `include_tests` toggle, `include_docs` toggle, `max_files` |
| `prompt.rs`      | System prompt content, user prompt sections, `focus_areas` filtering       |
| `plugin.rs`      | `enabled=false` skip, full run with `MockProvider`, finding cap, paths     |

### Mock Provider Pattern

Tests use `MockProvider` from `crate::providers::base` to inject canned AI
responses without making network calls. A helper fixture builds a minimal
`PluginContext` with a `tempfile::TempDir` workspace, an empty `ScanResult`, and
a `MockProvider` configured to return a predefined JSON findings payload.

```rust
let mut mock = MockProvider::new();
mock.expect_complete()
    .returning(|_, _| Ok(FIXTURE_FINDINGS_JSON.to_string()));
```

### Integration Tests

A full-pipeline test in `tests/technical_review_integration.rs` exercises the
plugin against a fixture repository scan, verifies that both output files are
written to disk, and checks that the `ReportEnvelope` round-trips through
`ReportEnvelope::load_from_json`.

---

## Success Criteria

| Criterion                                                         | Verification                                                           |
| ----------------------------------------------------------------- | ---------------------------------------------------------------------- |
| Plugin registers as `"technical-review"` and is retrievable       | `PluginRegistry::get("technical-review")` returns the plugin instance  |
| All 14 dimensions appear in prompt when `focus_areas` is empty    | Unit test on `PromptBuilder::build` asserts all dimension keys present |
| `focus_areas` restricts active dimensions                         | Unit test confirms excluded keys are absent from built prompt text     |
| `max_files` cap is respected by `FilePrioritizer`                 | Test with large `ScanResult` verifies result length <= `max_files`     |
| `confidence_threshold` filters low-confidence findings            | Findings at 0.5 confidence with threshold 0.7 produce zero output      |
| Both report files are written on a successful run                 | Integration test asserts file existence and non-zero byte size         |
| `technical_review.json` is a valid `ReportEnvelope`               | `ReportEnvelope::load_from_json` on the written file succeeds          |
| Watcher event `xzardgz.technical_review.task` routes to plugin    | `WatcherExecutor` test with `MockWorkflowPlugin` registered            |
| `WatcherResultMessage` includes findings summary and report paths | Executor test asserts `findings_summary.total > 0` and `report_paths`  |
| `enabled: false` causes plugin to return without calling AI       | Unit test checks `MockProvider::complete` is never called              |
| All four Cargo quality gates pass                                 | `cargo fmt`, `cargo check`, `cargo clippy -D warnings`, `cargo test`   |
