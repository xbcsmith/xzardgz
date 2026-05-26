# Phase 13: Plugin Runtime, Reports, and Investigation Module

## Overview

Phase 13 adds three major systems to the XZardgz pipeline:

1. **Plugin Runtime** (`src/plugins/`) - A trait-based plugin abstraction with
   context, output, metadata, and a registry for registering and dispatching
   named plugins.
2. **Report Infrastructure** (`src/reports/`) - Versioned, validated, and
   persisted plugin reports in Markdown, JSON, and SARIF 2.1.0 formats with a
   structured envelope and risk band classification.
3. **Investigation Module** (`src/investigation/`) - File scope management and
   strategy selection for single-session or batched-session investigation of
   large repositories.

Scoring integration is provided by `compute_finding_confidence` in
`src/scanner/scoring.rs` and the `record_plugin_score` method added to
`WorkspaceManager`.

---

## Module Layout

```text
src/
  plugins/
    mod.rs          - public API re-exports
    context.rs      - PluginContext, ToolAccessLevel
    output.rs       - PluginOutput, TokenUsage
    registry.rs     - PluginRegistry
    trait_def.rs    - WorkflowPlugin trait, PluginMetadata
  reports/
    mod.rs          - public API re-exports
    risk_band.rs    - RiskBand enum (Low / Medium / High / Critical)
    findings.rs     - PluginFinding, PluginFindings
    envelope.rs     - ReportEnvelope, REPORT_ENVELOPE_VERSION
    formatter.rs    - ReportFormat, PluginReportFormatter trait, validate_report_path
    markdown.rs     - MarkdownReportWriter
    json.rs         - JsonReportWriter
    sarif.rs        - SarifReportWriter (SARIF 2.1.0)
  investigation/
    mod.rs          - public API re-exports
    scope.rs        - FileMatchEntry, InvestigationScope
    batch.rs        - BatchConfig, InvestigationBatch, split_into_batches,
                      compute_investigation_turns
    strategy.rs     - InvestigationStrategy
```

---

## Plugin Runtime Design

### WorkflowPlugin Trait

`WorkflowPlugin` is an `async_trait` object-safe trait annotated with
`#[cfg_attr(test, mockall::automock)]` to enable mock generation in tests:

```rust
#[async_trait]
pub trait WorkflowPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn metadata(&self) -> PluginMetadata;
    fn supported_formats(&self) -> Vec<String>;
    fn required_tool_access(&self) -> ToolAccessLevel;
    async fn run(&self, ctx: PluginContext) -> Result<PluginOutput>;
}
```

### PluginContext

`PluginContext` carries all pipeline resources into a plugin run:

- `config: Arc<Config>` - effective pipeline configuration
- `workspace: Arc<WorkspaceManager>` - workspace paths and state manager
- `state: WorkspaceState` - current state snapshot
- `scan_result: ScanResult` - repository scan artifact
- `provider: Arc<dyn Provider + Send + Sync>` - AI provider
- `tool_registry: ToolRegistry` - available tools
- `governance: GovernanceChecker` - policy enforcement
- `diagnostics: Diagnostics` - accumulates diagnostics during the run
- `watcher_task_id: Option<String>` - set when triggered by a watcher message
- `prompts: HashMap<String, String>` - pre-loaded prompt templates

Builder methods (`with_watcher_task_id`, `with_prompts`) enable fluent
construction.

### PluginOutput

`PluginOutput` is returned by every plugin run and carries:

- `summary: String` - human-readable result summary
- `written_files: Vec<String>` - all files written by this run
- `findings: Vec<PluginFinding>` - findings produced during analysis
- `completed: bool` - whether the run finished successfully
- `diagnostics: Diagnostics` - structured diagnostics
- `scores: HashMap<String, f64>` - named numeric scores
- `risk_band: Option<RiskBand>` - automatically updated on `add_finding`
- `report_paths: HashMap<String, Vec<String>>` - paths by format label
- `provider_metadata: Option<ProviderMetadata>` - AI provider metadata
- `token_usage: Option<TokenUsage>` - input/output token counts

The `add_finding` method automatically recomputes `risk_band` by calling
`PluginFindings::to_risk_band()` across all current findings.

### PluginRegistry

`PluginRegistry` maps plugin names to `Arc<dyn WorkflowPlugin>` instances.

Key behaviors:

- `register(plugin)` - registers by `plugin.name()`
- `get(name)` - returns `Err(PipelineError::Plugin)` for disabled plugins,
  `Err(PipelineError::PluginNotFound)` for unknown plugins
- `disable(name)` - marks a plugin as disabled without unregistering it
- `list_plugins()` - returns sorted `Vec<PluginMetadata>`
- `validate_plugin_config(name, config)` - validates config against the plugin

---

## Report Infrastructure Design

### RiskBand

`RiskBand` is a four-tier enum with `Low < Medium < High < Critical` ordering
(derived `Ord`). Two derivation paths are provided:

- `RiskBand::from_confidence(score: f64)` - maps AI confidence to a band
  (thresholds: 0.25 / 0.50 / 0.75)
- `PluginFindings::to_risk_band()` - maps the highest finding severity to a band

### ReportEnvelope

`ReportEnvelope` is the single JSON artifact persisted per plugin run. It
includes:

- Provenance: `report_id`, `generated_at`, `plugin_name`, `repository_name`,
  `repository_url`, `head_commit`, `workspace_id`, `scan_artifact_version`
- Analysis: `provider_metadata`, `model_id`, `findings`, `diagnostics`,
  `risk_band`
- Scores: `scores: HashMap<String, f64>` (added in Phase 13 for score inclusion
  in JSON reports)

`ReportEnvelope::write_to_file` creates parent directories and writes
pretty-printed JSON. `validate_report_path` is called by every writer before
filesystem access.

### Writers

| Writer                 | Format   | Extension     |
| ---------------------- | -------- | ------------- |
| `MarkdownReportWriter` | Markdown | `.md`         |
| `JsonReportWriter`     | JSON     | `.json`       |
| `SarifReportWriter`    | SARIF    | `.sarif.json` |

All writers implement `PluginReportFormatter` and call `validate_report_path`
before writing.

The SARIF writer produces SARIF 2.1.0 output with:

- `camelCase` field names via `#[serde(rename_all = "camelCase")]`
- Severity-to-level mapping: Critical/High => `"error"`, Medium => `"warning"`,
  Low/Info => `"note"`
- Deduplicated rule list sorted by ID

---

## Investigation Module Design

### InvestigationScope

`InvestigationScope` is a `HashMap<String, FileMatchEntry>` keyed by
repository-relative path. It provides:

- `insert`, `remove`, `get` for individual entry management
- `paths()` / `entries()` - always sorted by path for deterministic output
- `filter_by_category(category)` - returns a new scope with only matching
  entries
- `filter_by_language(language)` - case-insensitive language filter
- `total_bytes()` - aggregate size across all entries

### InvestigationStrategy

Two strategies are available:

- `InvestigationStrategy::SingleSession` - all files in one AI session (used
  when scope has at most 20 files)
- `InvestigationStrategy::BatchedSession(BatchConfig)` - files split into
  batches, each in its own session (used for larger scopes)

`InvestigationStrategy::default_for_scope` auto-selects based on scope size.

### Batching

`split_into_batches` partitions a scope into deterministically ordered
`InvestigationBatch` slices. Files are sorted by path before chunking to
guarantee identical output for identical inputs regardless of HashMap iteration
order.

`BatchConfig` controls:

- `max_batches` (default: 10) - cap on number of batches
- `batch_size` (default: 20) - files per batch
- `clean_verification_turns` (default: 1) - verification passes after each batch

---

## Scoring Integration

### compute_finding_confidence

Added to `src/scanner/scoring.rs`:

```rust
pub fn compute_finding_confidence(
    ai_confidence: f64,
    scanner_signals: &[ScoringSignal],
) -> f64
```

The AI confidence signal carries weight `2.0` (double a standard scanner signal
weight of `1.0`), giving the AI report roughly twice the influence of any single
static analysis signal. The result is clamped to `[0.0, 1.0]`.

### record_plugin_score

Added to `WorkspaceManager`:

```rust
pub fn record_plugin_score(&mut self, step_id: &str, score: f64) -> Result<()>
```

Stores the score in `WorkspaceState::plugin_scores` under `step_id` and persists
to disk. Repeated calls replace the previous score for the same step.

---

## Dependency Directions

```text
plugins    -> reports    (PluginFinding, RiskBand used in PluginOutput)
plugins    -> workspace  (WorkspaceManager, WorkspaceState in PluginContext)
plugins    -> scanner    (ScanResult in PluginContext)
plugins    -> providers  (Provider trait, ProviderMetadata)
plugins    -> tools      (ToolRegistry)
plugins    -> governance (GovernanceChecker)
plugins    -> config     (Config)
plugins    -> diagnostics (Diagnostics)

reports    -> scanner    (FindingSeverity reused in PluginFinding)
reports    -> diagnostics (Diagnostic in ReportEnvelope)
reports    -> providers  (ProviderMetadata in ReportEnvelope)

investigation -> (standalone: serde, std collections only)

scanner/scoring -> (standalone: serde only)
workspace  -> scanner    (ScanResult)
workspace  -> providers  (ResolvedModel)
```

No circular dependencies are introduced.

---

## Success Criteria Verification

| Criterion                                                    | Status                                                                                                             |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| A mock plugin can run from local and watcher execution paths | Registry dispatches `MockWorkflowPlugin`; `PluginContext` carries `watcher_task_id`                                |
| Plugin reports are versioned, validated, and persisted       | `REPORT_ENVELOPE_VERSION = "1"` enforced; `validate_report_path` guards all writers; `write_to_file` persists JSON |
| Large repositories can be investigated in bounded batches    | `split_into_batches` caps output at `max_batches`; `default_for_scope` auto-selects batching for scopes > 20 files |

---

## Related Documentation

- `docs/explanation/phase13_plugins_implementation.md` - plugin module detail
- `docs/explanation/phase13_reports_implementation.md` - report infrastructure
  detail
- `docs/explanation/phase13_investigation_implementation.md` - investigation
  module detail
