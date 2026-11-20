# Phase 7 SAST Scan Plugin Implementation

## Overview

Phase 7 wires the SAST engine built in earlier phases into the plugin subsystem.
The result is a fully-registered `WorkflowPlugin` that the pipeline runner can
invoke through `PluginRegistry::with_builtins()`.

## Files Changed

| File                                                  | Change                                                   |
| ----------------------------------------------------- | -------------------------------------------------------- |
| `src/plugins/sast_scan/scoring.rs`                    | New: `SastScanResult` implementing `ScoringInput`        |
| `src/plugins/sast_scan/report.rs`                     | New: Markdown report builder                             |
| `src/plugins/sast_scan/plugin.rs`                     | New: `SastScanPlugin` implementing `WorkflowPlugin`      |
| `src/plugins/sast_scan/mod.rs`                        | Updated: exports the three new modules                   |
| `src/plugins/registry.rs`                             | Updated: registers `SastScanPlugin` in `with_builtins()` |
| `docs/explanation/sast_scan_phase7_implementation.md` | New: this document                                       |

## Design

### Scoring (`scoring.rs`)

`SastScanResult` aggregates the SAST matches, ruleset identifiers, scan root,
and duration from a completed scan. It implements `ScoringInput` with the
following signal rules (one signal per `SastMatch`):

1. `AbsoluteViolation` when `severity == Error` AND `metadata.cwe` contains
   `CWE-326`, `CWE-327`, or `CWE-328`. The match emits no further signal.
2. `Positive { weight: 0.3 }` for `Confidence::High` matches that did not
   trigger a violation.
3. `Negative { weight: 0.2 }` for `Confidence::Low` matches.
4. `Confidence::Medium` and `Confidence::Unknown` produce no signal; the
   starting score of `1.0` passes through unchanged.

`review_violations` is `false`. An `AbsoluteViolation` always floors the static
score to `0.0` and bypasses the confidence threshold check in `run()`.

### Report (`report.rs`)

`build_sast_markdown_report` renders a `SastScanResult` and its `ScoringResult`
into a Markdown document with three sections:

1. Header block with scan root, duration, match count, and confidence scores.
2. Summary table (sorted by severity: Error, Warning, Info) with file, rule ID,
   severity, confidence, and message columns.
3. Detailed findings per match, grouped in severity order, each showing file
   location, message, snippet, and metavariable bindings.

### Plugin (`plugin.rs`)

`SastScanPlugin` follows the standard `WorkflowPlugin` execution contract:

```text
enabled? -> resolve_rules -> SastEngine::new -> with_rules -> scan
         -> severity filter -> max_findings cap -> SastScanResult
         -> ConfidenceScorer::score (ai_score=None in Phase 7)
         -> threshold filter -> build_sast_markdown_report
         -> write reports/sast_scan.md -> PluginOutput
```

Soft failures (`PluginOutput::failure`) are returned for all recoverable errors
(rule resolution, engine construction, scan failure, I/O). The plugin does not
perform any AI provider calls in Phase 7.

### Registry (`registry.rs`)

`PluginRegistry::with_builtins()` now registers three plugins:

- `technical-review` (`TechnicalReviewPlugin`)
- `security-review` (`SecurityReviewPlugin`)
- `sast_scan` (`SastScanPlugin`)

## Confidence Scoring Integration

See `docs/reference/confidence_scoring.md` for the full signal catalog. The
`sast_scan` section documents every signal the plugin can emit, the conditions
that trigger each one, and the filter logic used in `run()`.

## Tests Added

### `scoring.rs` (10 tests)

- `test_sast_scan_result_high_confidence_emits_positive_signal`
- `test_sast_scan_result_low_confidence_emits_negative_signal`
- `test_sast_scan_result_medium_confidence_emits_no_signal`
- `test_sast_scan_result_cwe327_error_emits_absolute_violation`
- `test_sast_scan_result_cwe327_warning_does_not_emit_absolute_violation`
- `test_sast_scan_result_plugin_name_is_sast_scan`
- `test_sast_scan_result_context_for_ai_contains_match_count`
- `test_absolute_violation_passes_filter_regardless_of_threshold`
- `test_ai_confidence_weight_shifts_blended_score`
- `test_ai_analysis_disabled_yields_none_ai_score_and_blended_equals_static`

### `report.rs` (5 tests)

- `test_build_sast_markdown_report_contains_rule_id`
- `test_build_sast_markdown_report_with_zero_matches_has_summary`
- `test_build_sast_markdown_report_contains_scan_root`
- `test_build_sast_markdown_report_metavariables_appear_when_present`
- `test_build_sast_markdown_report_error_severity_appears_before_warning`

### `plugin.rs` (8 tests)

- `test_sast_scan_plugin_name_returns_sast_scan`
- `test_sast_scan_plugin_supported_formats_includes_markdown_json_sarif`
- `test_sast_scan_plugin_required_tool_access_is_read_only`
- `test_severity_passes_threshold_error_passes_warning_threshold`
- `test_severity_passes_threshold_info_does_not_pass_warning_threshold`
- `test_severity_passes_threshold_info_passes_info_threshold`
- `test_severity_passes_threshold_warning_does_not_pass_error_threshold`
- `test_severity_passes_threshold_unknown_string_defaults_to_warning`
- `test_sast_scan_plugin_run_disabled_returns_success`
- `test_sast_scan_plugin_run_on_fixture_workspace_produces_markdown_report`

## Limitations and Follow-up

- Phase 7 does not write JSON or SARIF reports. These formats are listed in
  `supported_formats()` for forward compatibility and will be implemented in a
  follow-up phase.
- The AI score leg is wired but always receives `None` in Phase 7. Set
  `ai_analysis_enabled: true` and wire a real provider call to activate it.
- The `sast_scan` plugin does not yet emit `PluginFinding` objects. The
  `PluginOutput::add_finding` path and risk band computation will be added when
  the SARIF output format is implemented.
