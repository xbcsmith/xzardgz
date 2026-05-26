# Phase 13: Reports Module Implementation

## Overview

Phase 13 implements the `src/reports/` module for XZardgz. This module provides
the complete infrastructure for producing structured analysis reports from
plugin runs. It introduces a four-tier risk classification system, a pluggable
formatter abstraction, and three concrete output formats: Markdown, JSON, and
SARIF 2.1.0.

## Files Created

| File                       | Purpose                                                        |
| -------------------------- | -------------------------------------------------------------- |
| `src/reports/mod.rs`       | Module root: submodule declarations and crate-level re-exports |
| `src/reports/risk_band.rs` | `RiskBand` enum with confidence-score derivation               |
| `src/reports/findings.rs`  | `PluginFinding` and `PluginFindings` collection types          |
| `src/reports/envelope.rs`  | `ReportEnvelope` top-level report container                    |
| `src/reports/formatter.rs` | `ReportFormat`, `PluginReportFormatter` trait, path validation |
| `src/reports/json.rs`      | `JsonReportWriter` - pretty-printed JSON output                |
| `src/reports/markdown.rs`  | `MarkdownReportWriter` - human-readable Markdown output        |
| `src/reports/sarif.rs`     | `SarifReportWriter` and SARIF 2.1.0 wire types                 |

## Files Modified

| File         | Change                   |
| ------------ | ------------------------ |
| `src/lib.rs` | Added `pub mod reports;` |

## Architecture

### Risk Classification (`risk_band.rs`)

`RiskBand` is a four-variant enum ordered `Low < Medium < High < Critical`. The
derived `Ord` implementation enables direct comparison. Two derivation paths are
provided:

- `RiskBand::from_confidence(score: f64)` maps an AI confidence score in
  `[0.0, 1.0]` to a band using fixed thresholds:

  - `score < 0.25` => `Low`
  - `score < 0.50` => `Medium`
  - `score < 0.75` => `High`
  - `score >= 0.75` => `Critical`

- `PluginFindings::to_risk_band()` maps the highest `FindingSeverity` across all
  findings to the corresponding band (both `Info` and `Low` map to
  `RiskBand::Low`).

### Finding Types (`findings.rs`)

`PluginFinding` represents one issue surfaced by a plugin. It uses a builder
pattern to attach optional fields without cluttering the required constructor:

```rust
PluginFinding::new("sql_injection", "SQL Injection", "...", FindingSeverity::High, 0.88)
    .with_location("src/db.rs", Some(42))
    .with_tags(vec!["owasp".to_string()])
```

`PluginFindings` is an ordered collection of `PluginFinding`s with methods for
severity filtering (`by_severity`), max-severity queries (`highest_severity`),
and risk-band derivation (`to_risk_band`).

### Report Envelope (`envelope.rs`)

`ReportEnvelope` is the single self-describing JSON artifact written at the end
of a plugin run. It bundles:

- Provenance: `report_id`, `plugin_name`, `workspace_id`, `repository_name`,
  `repository_url`, `head_commit`, `generated_at`
- Analysis output: `findings` (`PluginFindings`), `diagnostics`
  (`Vec<Diagnostic>`), `risk_band` (`Option<RiskBand>`)
- Provider context: `provider_metadata`, `model_id`, `scan_artifact_version`

The schema version constant `REPORT_ENVELOPE_VERSION = "1"` allows consumers to
reject incompatible versions without attempting partial parsing.

Key methods:

- `ReportEnvelope::new` - sets `version`, current UTC timestamp, empty
  findings/diagnostics, all optionals to `None`
- `to_json` / `load_from_json` - serde round-trip via `serde_json`
- `write_to_file` - creates parent directories, writes pretty-printed JSON

### Formatter Abstraction (`formatter.rs`)

`PluginReportFormatter` is a `Send + Sync` trait with two methods:

```rust
fn format_name(&self) -> &str;
fn write(&self, envelope: &ReportEnvelope, path: &Path) -> Result<()>;
```

All implementations must call `validate_report_path(path)?` before any
file-system operation. `validate_report_path` checks that the path has both a
non-empty filename component and a parent directory component, returning
`PipelineError::Report` on failure.

`ReportFormat` is an enum naming the three supported formats. It carries
`extension()`, `as_str()`, and a case-insensitive `from_str()` constructor
(permitted ambiguity suppressed with `#[allow(clippy::should_implement_trait)]`
because the method returns `Option<Self>` rather than `Result<Self, Err>`).

### JSON Writer (`json.rs`)

`JsonReportWriter` is the simplest formatter: it validates the path and
delegates to `ReportEnvelope::write_to_file`. The output is identical to a
direct `to_json()` call and is suitable for downstream programmatic consumption.

### Markdown Writer (`markdown.rs`)

`MarkdownReportWriter` renders a complete Markdown document via
`std::fmt::Write` (`write!` / `writeln!` macros on a `String`). The private
`render` method builds the document in this order:

1. H1 heading with plugin name
2. Generated-at timestamp (RFC 3339 formatted)
3. Repository name and URL (if present)
4. HEAD commit (if present)
5. Workspace ID
6. Risk band label (if present)
7. H2 Findings section: Markdown table with Severity, Kind, Title, Location
   columns; shows `*No findings recorded.*` when the collection is empty
8. H2 Diagnostics section: bullet list of `[LEVEL] message (context)`; omitted
   entirely when `diagnostics` is empty

The `fmt_err` helper function maps the theoretically unreachable
`std::fmt::Error` from writing to a `String` to a `PipelineError::Report`.

### SARIF Writer (`sarif.rs`)

`SarifReportWriter` converts findings to SARIF 2.1.0. The mapping is:

| `FindingSeverity`   | SARIF `level` |
| ------------------- | ------------- |
| `Critical` / `High` | `"error"`     |
| `Medium`            | `"warning"`   |
| `Low` / `Info`      | `"note"`      |

Rules are collected from findings, deduplicated by `kind` (first-seen title
wins), and sorted alphabetically by `id` for deterministic output.

SARIF requires camelCase JSON field names. All structs that contain multi-word
field names use `#[serde(rename_all = "camelCase")]`:

- `SarifRule.short_description` => `shortDescription`
- `SarifResult.rule_id` => `ruleId`
- `SarifLocation.physical_location` => `physicalLocation`
- `SarifPhysicalLocation.artifact_location` => `artifactLocation`
- `SarifRegion.start_line` => `startLine`

## Design Decisions

### Separate `risk_band` and `findings` modules

`RiskBand` is independent of `PluginFindings` and can be used anywhere a risk
classification is needed without pulling in the full findings type hierarchy.
The derivation logic lives in `PluginFindings::to_risk_band()`.

### `validate_report_path` as a free function in `formatter.rs`

Placing the path validator in the formatter module ensures all writers share a
single validation contract. The function is re-exported from `mod.rs` so callers
outside the formatter can use it directly.

### `ReportEnvelope::write_to_file` vs `PluginReportFormatter::write`

`write_to_file` is a lower-level convenience on the envelope itself (used by
`JsonReportWriter`). The `PluginReportFormatter::write` contract adds the
`validate_report_path` guard on top. Writers for non-JSON formats (Markdown,
SARIF) implement their own file-writing logic to avoid going through JSON
serialization.

### `#[allow(clippy::should_implement_trait)]` on `ReportFormat::from_str`

The method is specified to return `Option<Self>` rather than
`Result<Self, Err>`, which is intentional: callers should match on `None` rather
than propagate an error for an unrecognised format string. Implementing
`std::str::FromStr` would require a different return type and a separate error
type, adding noise for a trivial case-insensitive lookup.

## Test Coverage

A total of 113 tests were added across the module:

| File           | Unit tests | Doc tests |
| -------------- | ---------- | --------- |
| `risk_band.rs` | 18         | 4         |
| `findings.rs`  | 16         | 9         |
| `envelope.rs`  | 15         | 5         |
| `formatter.rs` | 18         | 5         |
| `json.rs`      | 5          | 1         |
| `markdown.rs`  | 15         | 1         |
| `sarif.rs`     | 16         | 1         |

All tests follow the `test_<function>_<condition>_<expected>` naming convention
required by AGENTS.md.
