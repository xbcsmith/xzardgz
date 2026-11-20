# Phase 7: Plugin, Tool Executor, and Security Review Wiring Implementation

## Overview

Phase 7 adds three integration surfaces that expose the SAST engine to the rest
of the system:

1. **SastScanPlugin** - a first-class plugin that scores, filters, and reports
   SAST findings using the confidence scoring pipeline.
2. **SastTool** - a tool executor entry that allows the agent harness to invoke
   SAST scans as tool calls during AI-driven workflows.
3. **Security review SAST pre-pass** - an optional pre-pass wired into
   `SecurityReviewPlugin` that folds SAST signals into the finding score before
   the AI completion call is made.

Together these surfaces make SAST results available at three distinct points in
the pipeline: standalone plugin output, structured tool call output, and blended
security review signal input.

## SastScanPlugin

Source: `src/plugins/sast_scan/mod.rs`

The plugin follows the same lifecycle as `SecurityReviewPlugin` and
`TechnicalReviewPlugin`. Its `execute` method performs the following steps in
order.

### Rule resolution

```text
resolve_rules(ResolveConfig { rulesets: config.sast_rulesets, .. })
    -> Vec<NamespacedRule>
```

The resolver checks each entry in `config.sast_rulesets` against the built-in
ruleset registry. Entries prefixed with `builtin:` are loaded from the embedded
ruleset store. Bare paths are loaded from the filesystem as external rulesets.
When `sast_rulesets` is empty the resolver falls back to `builtin:crypto` and
`builtin:security`.

### Engine scan

```text
SastEngine::new(SastEngineConfig { jobs: config.jobs, .. })
    .with_rules(rules)
    .scan(&workspace_root)
    -> SastScanReport { matches: Vec<SastMatch> }
```

The engine is configured from `SastScanConfig` fields. `workspace_root` is the
path supplied by the plugin runner.

### Result construction

Each `SastMatch` is wrapped in a `SastScanResult`:

```text
SastScanResult {
    rule_id:    match.rule_id.clone(),
    severity:   match.severity,
    confidence: match.confidence,
    metadata:   match.metadata.clone(),
    ..
}
```

### Confidence scoring

```text
ConfidenceScorer::new(ScoringConfig {
    review_violations: false,
    ai_confidence_weight: config.ai_confidence_weight,
    ai_analysis_enabled:  config.ai_analysis_enabled,
})
.score(&sast_scan_result)
-> ScoredResult { blended_score, static_score, ai_score, has_violations }
```

`review_violations` is always `false` for this plugin. An `AbsoluteViolation`
floors the static score to `0.0` and short-circuits signal processing.

### Threshold filtering

A result is retained when:

```text
result.has_violations() || result.blended_score >= config.confidence_threshold
```

Results that triggered an `AbsoluteViolation` satisfy `has_violations()` and are
always retained regardless of threshold.

### Report generation

Retained results are sorted by severity (Error before Warning before Info) then
by `rule_id`. The report is rendered as a Markdown string grouped by severity
heading. Each entry includes the rule identifier, matched file path, line range,
and blended confidence score.

### Return value

```text
PluginOutput {
    plugin_name: "sast_scan",
    findings:    Vec<PluginFinding>,
    report:      String,  // Markdown
    metadata:    HashMap<String, Value>,
}
```

## SastScanResult Scoring

Source: `src/plugins/sast_scan/scoring.rs`

`SastScanResult` implements `ScoringInput`. The table below lists every signal
the implementation can emit, in evaluation order.

| Signal variant      | Label / Reason                            | Weight            | Condition                                                            |
| ------------------- | ----------------------------------------- | ----------------- | -------------------------------------------------------------------- |
| `AbsoluteViolation` | `"SAST [{rule_id}]: {cwes}"`              | (floors to `0.0`) | Metadata contains CWE-326, CWE-327, or CWE-328 AND severity == Error |
| `Positive`          | `"high-confidence sast match: {rule_id}"` | `0.3`             | confidence == High AND AbsoluteViolation was NOT emitted             |
| `Negative`          | `"low-confidence sast match: {rule_id}"`  | `0.2`             | confidence == Low                                                    |

Matches with `confidence == Medium` or `confidence == Unknown` emit no signal.

### context_for_ai

`SastScanResult::context_for_ai()` returns a structured string used as
supplemental context in AI completion prompts when `ai_analysis_enabled` is
`true`:

```text
SAST finding: {rule_id} ({severity})
File: {path}:{start_line}-{end_line}
CWEs: {comma-separated CWE list, or "none"}
Message: {rule_message}
```

### plugin_name

`SastScanResult::plugin_name()` returns `"sast_scan"`.

## SastTool

Source: `src/tools/sast.rs`

`SastTool` implements the `ToolExecutor` trait and registers as the
`"sast_scan"` tool in the agent harness tool registry.

### Tool definition

| Parameter            | Type            | Required | Description                                                  |
| -------------------- | --------------- | -------- | ------------------------------------------------------------ |
| `path`               | `string`        | yes      | Workspace root path to scan                                  |
| `rulesets`           | `array[string]` | no       | Ruleset identifiers; defaults to built-in crypto + security  |
| `format`             | `string`        | no       | Output format: `"json"` (default), `"sarif"`, `"cyclonedx"`  |
| `severity_threshold` | `string`        | no       | Minimum severity: `"info"`, `"warning"` (default), `"error"` |
| `jobs`               | `integer`       | no       | Worker thread count; `0` selects automatic (default)         |

### Execute flow

```text
SastTool::execute(params)
    |
    +-- validate path is within allowed workspace roots
    |
    +-- resolve_rules(rulesets)
    |
    +-- SastEngine::new(config).with_rules(rules).scan(path)
    |
    +-- filter matches by severity_threshold
    |
    `-- serialize to requested format -> ToolOutput { content: String }
```

### Boundary enforcement

`SastTool` does not import anything from `src/clients/`. Network calls are the
responsibility of the caller. The tool is a pure scan-and-serialize unit that
operates entirely on the local filesystem.

## SecurityReview SAST Pre-pass

Source: `src/plugins/security_review/mod.rs`

### New SecurityReviewConfig fields

Three fields are added to `SecurityReviewConfig` to control the pre-pass:

| Field                     | Type          | Default     | Description                                                    |
| ------------------------- | ------------- | ----------- | -------------------------------------------------------------- |
| `sast_enabled`            | `bool`        | `false`     | When `true`, run SAST engine before the AI completion call     |
| `sast_rulesets`           | `Vec<String>` | `[]`        | Rulesets to use in the pre-pass; empty falls back to built-ins |
| `sast_severity_threshold` | `String`      | `"warning"` | Minimum SAST severity to fold into signals                     |

### run_sast_prepass

```text
run_sast_prepass(workspace_root, config)
    |
    +-- resolve_rules(config.sast_rulesets)
    |
    +-- SastEngine::new(..).with_rules(rules).scan(workspace_root)
    |
    +-- filter matches by sast_severity_threshold
    |
    `-- for each match:
            score match via SastScanResult scoring rules
            -> Vec<ScoringSignal>
```

### Signal merging

The signals produced by `run_sast_prepass` are prepended to the signal list
before the OSV dependency signals and the AI completion call:

```text
signals = sast_prepass_signals
        + osv_dependency_signals
        + ai_completion_signals
```

The merged list is then folded by `ConfidenceScorer` using the
`SecurityReviewConfig` scoring settings (`review_violations: false`,
`ai_confidence_weight`, `ai_analysis_enabled`). An `AbsoluteViolation` from a
SAST match will floor the finding score to `0.0` and short-circuit the fold,
regardless of subsequent OSV or AI signals.

## Configuration

All new configuration fields with their types and defaults:

| Struct                 | Field                     | Type          | Default     |
| ---------------------- | ------------------------- | ------------- | ----------- |
| `SastScanConfig`       | `confidence_threshold`    | `f64`         | `0.5`       |
| `SastScanConfig`       | `ai_confidence_weight`    | `f64`         | `0.5`       |
| `SastScanConfig`       | `ai_analysis_enabled`     | `bool`        | `false`     |
| `SastScanConfig`       | `sast_rulesets`           | `Vec<String>` | `[]`        |
| `SastScanConfig`       | `jobs`                    | `usize`       | `0`         |
| `SecurityReviewConfig` | `sast_enabled`            | `bool`        | `false`     |
| `SecurityReviewConfig` | `sast_rulesets`           | `Vec<String>` | `[]`        |
| `SecurityReviewConfig` | `sast_severity_threshold` | `String`      | `"warning"` |

## Testing

The following tests are required by AGENTS.md. All tests must follow the naming
convention `test_<function>_<condition>_<expected>`.

### Scoring unit tests

Located in `src/plugins/sast_scan/scoring.rs`:

| Test name                                                       | Coverage                                                                 |
| --------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `test_score_sast_result_cwe_326_error_emits_absolute_violation` | CWE-326 + Error severity produces AbsoluteViolation, static score == 0.0 |
| `test_score_sast_result_high_confidence_emits_positive_signal`  | High confidence without CWE produces Positive(0.3)                       |
| `test_score_sast_result_low_confidence_emits_negative_signal`   | Low confidence produces Negative(0.2)                                    |

### Plugin fixture test

Located in `src/plugins/sast_scan/mod.rs`:

| Test name                                               | Coverage                                                                       |
| ------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `test_sast_scan_plugin_on_fixture_dir_returns_findings` | Full plugin pipeline against a fixture directory containing a known rule match |

### Tool tests

Located in `src/tools/sast.rs`:

| Test name                                                | Coverage                                                         |
| -------------------------------------------------------- | ---------------------------------------------------------------- |
| `test_sast_tool_execute_on_empty_dir_returns_empty_json` | Execute returns valid empty JSON on a directory with no matches  |
| `test_sast_tool_execute_rejects_path_outside_workspace`  | Boundary enforcement returns an error for out-of-workspace paths |

### Security review smoke test

Located in `src/plugins/security_review/mod.rs`:

| Test name                                                      | Coverage                                             |
| -------------------------------------------------------------- | ---------------------------------------------------- |
| `test_security_review_sast_prepass_disabled_skips_sast_engine` | Pre-pass is not invoked when `sast_enabled == false` |
| `test_security_review_sast_prepass_enabled_merges_signals`     | Pre-pass signals appear in the merged signal list    |

### Validation test

Located in `src/plugins/sast_scan/scoring.rs`:

| Test name                                                  | Coverage                                                  |
| ---------------------------------------------------------- | --------------------------------------------------------- |
| `test_score_sast_result_medium_confidence_emits_no_signal` | Medium confidence produces no Positive or Negative signal |

## Deliverables

| File                                                                                   | Status   |
| -------------------------------------------------------------------------------------- | -------- |
| `src/plugins/sast_scan/mod.rs`                                                         | new      |
| `src/plugins/sast_scan/scoring.rs`                                                     | new      |
| `src/tools/sast.rs`                                                                    | new      |
| `src/tools/mod.rs`                                                                     | modified |
| `src/plugins/security_review/mod.rs`                                                   | modified |
| `src/config.rs` (`SecurityReviewConfig` new fields)                                    | modified |
| `docs/reference/confidence_scoring.md` (`sast_scan` signal catalog entry)              | modified |
| `docs/explanation/phase7_plugin_toolexecutor_security_review_wiring_implementation.md` | new      |
