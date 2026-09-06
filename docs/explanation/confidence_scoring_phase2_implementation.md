# Confidence Scoring Phase 2 Implementation

This document describes the changes made during Phase 2 of the confidence
scoring integration plan, as defined in
`docs/explanation/confidence_scoring_integration_plan.md`. Phase 2 wires the
`ConfidenceScorer` built in Phase 1 into the two shipped plugins --
`security_review` and `technical_review` -- replacing the previous raw-AI
confidence gate with a blended score that combines static signal evidence and
AI-reported confidence in a configurable ratio.

## Overview

Before Phase 2, both plugins filtered findings by comparing the AI self-reported
`confidence` field directly against `confidence_threshold`. The
`ConfidenceScorer` existed but had no production callers. After Phase 2:

- Each finding is scored by `ConfidenceScorer::score()` before filtering.
- The filter passes a finding when it has an `AbsoluteViolation` signal OR its
  blended score meets `confidence_threshold`.
- Both plugins' JSON reports expose three audit fields: the static fold score,
  the raw AI score, and the final blended confidence score.
- Both configs gain `ai_confidence_weight` and `ai_analysis_enabled` fields that
  operators can tune without rebuilding.

## Changes Made

### `src/reports/findings.rs`

Two new fields were added to `PluginFinding` to carry scoring audit data:

| Field          | Type          | Default | Description                                                      |
| -------------- | ------------- | ------- | ---------------------------------------------------------------- |
| `static_score` | `f64`         | `1.0`   | Baseline-fold result before the AI blend step                    |
| `ai_score`     | `Option<f64>` | `None`  | Raw AI-reported confidence; `None` when AI is disabled or failed |

A new builder method was also added:

```rust
pub fn with_scoring(mut self, result: &ScoringResult) -> Self {
    self.confidence = result.blended_score;
    self.static_score = result.static_score;
    self.ai_score = result.ai_score;
    self
}
```

Callers chain this after `to_plugin_finding()` to propagate all three audit
values into the finding in one step. The `confidence` field now holds the
blended score; reading it as a raw AI number is no longer valid for findings
processed by the scorer.

Seven unit tests cover `with_scoring`: blended confidence is set, static score
is set, AI score is forwarded, `None` AI score round-trips, default values are
preserved for unscored findings, and serde round-trip.

### `src/config.rs`

Both `SecurityReviewConfig` and `TechnicalReviewConfig` gained two new fields:

| Field                  | Type   | Default | Description                                                                 |
| ---------------------- | ------ | ------- | --------------------------------------------------------------------------- |
| `ai_confidence_weight` | `f64`  | `0.5`   | Blend weight `w` in `blended = static * (1 - w) + ai * w`                   |
| `ai_analysis_enabled`  | `bool` | `true`  | Master switch; when `false`, blended equals static and `ai_score` is `None` |

A shared helper function was added:

```rust
fn default_ai_confidence_weight() -> f64 {
    0.5
}
```

Both `Default` implementations and both serde default annotations reference this
helper, keeping the default value in one place.

### `src/plugins/security_review/finding.rs`

`SecurityReviewFinding` now implements `ScoringInput`. The severity-to-weight
mapping is:

| Severity | Signal type | Weight |
| -------- | ----------- | ------ |
| Info     | Negative    | 0.1    |
| Low      | Negative    | 0.2    |
| Medium   | Negative    | 0.4    |
| High     | Negative    | 0.6    |
| Critical | Negative    | 0.8    |

An `AbsoluteViolation` signal is emitted in addition to the `Negative` signal
when severity is `Critical` and the category string contains any of:
`"credential"`, `"secret"`, `"api_key"`, or `"hardcoded"`. This models the
policy that a confirmed hardcoded-credential finding must always reach the
reviewer regardless of score, while still recording a floor score.

Seven tests cover: each severity emits the expected weight, a
critical-credential category emits both a `Negative` and an `AbsoluteViolation`,
a critical non-credential category emits only a `Negative`, `plugin_name()`
returns `"security_review"`, and `context_for_ai()` contains the category and
evidence strings.

### `src/plugins/technical_review/finding.rs`

`TechnicalReviewFinding` now implements `ScoringInput` using the same severity
weights as the security review implementation. An `AbsoluteViolation` signal is
emitted when severity is `Critical` and the category string contains
`"dependency"` or `"supply_chain"`, representing the policy that a critical
supply-chain or dependency vulnerability must not be gated out by score alone.

Seven tests mirror the security review coverage, adapted to the
dependency-category trigger.

### `src/plugins/security_review/report.rs`

`SecurityReviewJsonReport::write` now accepts `&[PluginFinding]` instead of
`&[SecurityReviewFinding]`. The caller (the plugin's `run()` method) builds
pre-scored `PluginFinding` values and passes them directly to the JSON writer,
so the three audit fields appear in the report output without any additional
conversion. Markdown and SARIF reports continue to accept
`&[SecurityReviewFinding]` and remain unchanged.

### `src/plugins/technical_review/report.rs`

`TechnicalReviewJsonReport::write` received the same signature change:
`&[PluginFinding]` instead of `&[TechnicalReviewFinding]`, for the same reason.

### `src/plugins/security_review/plugin.rs`

The `run()` method now follows this sequence after parsing AI findings:

1. Construct `ScoringConfig` from the plugin config:

```rust
let scoring_cfg = ScoringConfig {
    ai_confidence_weight: config.ai_confidence_weight,
    ai_analysis_enabled: config.ai_analysis_enabled,
    review_violations: false,
};
let scorer = ConfidenceScorer::new(scoring_cfg);
```

1. Score each finding, keeping the `ScoringResult` alongside the raw finding:

```rust
let scored: Vec<(SecurityReviewFinding, ScoringResult)> = parsed_findings
    .into_iter()
    .map(|f| {
        let ai_score = Some(f.confidence);
        let result = scorer.score(&f, ai_score);
        (f, result)
    })
    .collect();
```

1. Filter using the blended score, with `AbsoluteViolation` findings bypassing
   the threshold:

```rust
scored.retain(|(_, result)| {
    result.has_violations() || result.blended_score >= config.confidence_threshold
});
```

1. Build `PluginFinding` values with all three audit fields:

```rust
let pf = f.to_plugin_finding().with_scoring(&result);
```

Three new integration tests were added:

- `test_security_review_plugin_absolute_violation_finding_always_included`: uses
  `confidence_threshold: 0.99` with a critical-credential finding; asserts the
  finding is included and that the blended confidence is lower than the raw AI
  confidence.
- `test_security_review_plugin_ai_weight_shifts_blended_confidence`: runs the
  plugin twice with `ai_confidence_weight: 0.1` and `0.9`; asserts the higher
  weight produces a higher blended confidence.
- `test_security_review_plugin_ai_disabled_blended_equals_static_score`: uses
  `ai_analysis_enabled: false`; asserts `ai_score` is `None` and blended equals
  static within floating-point tolerance.

### `src/plugins/technical_review/plugin.rs`

The same scoring wiring was applied to `TechnicalReviewPlugin::run()`.

The test helper `make_context` was refactored to delegate to a new
`make_context_with_config(root, scan, Option<TechnicalReviewConfig>, provider)`
function that applies an optional config override to `Config::default()`.
`make_context` itself continues to exist as a one-liner wrapper that passes
`None`.

The pre-existing test
`test_technical_review_plugin_run_with_mock_provider_with_findings_returns_success`
was updated to pass `confidence_threshold: 0.0`. The test verifies end-to-end
finding flow from the AI response through to `PluginOutput`; using the default
threshold of `0.7` would exclude a `severity: "high"` finding whose blended
score (`0.625`) falls below it, which is correct behavior but would hide the
test's actual intent.

Three new integration tests mirror the security review set, adapted to the
dependency-category `AbsoluteViolation` trigger.

## Design Decisions

### AbsoluteViolation findings bypass the threshold

Findings with `result.has_violations() == true` pass the confidence filter
unconditionally. This is an intentional policy: a deterministic rule breach
(hardcoded credential, critical dependency vulnerability) must reach the
reviewer regardless of the blended score. The blended score is still computed
and recorded; it is simply not used as a gate for these findings.

### JSON reports carry three distinct audit fields

Exposing `static_score`, `ai_score`, and `confidence` (blended) as separate
fields gives operators the information needed to audit why a finding was
included or excluded. A finding with a high AI score and a low static score
indicates a potential false positive from the AI that static rules would rate
skeptically. The separation also enables operators to verify the blend
arithmetic independently.

### AI disabled or failed always produces the static score

When `ai_analysis_enabled` is `false`, or when `ai_score` passed to
`scorer.score()` is `None`, the `blend()` method returns `static_score`
unchanged. The `ScoringResult.ai_score` field is set to `None` in both cases.
This means a build with no configured AI provider produces bit-for-bit identical
`confidence` values to a build where `ai_analysis_enabled` is explicitly set to
`false`.

### Markdown and SARIF reports retain raw-finding inputs

Only the JSON report was changed to accept `PluginFinding`. Markdown and SARIF
reports still receive the original typed finding slices. This keeps the rich
domain-specific formatting logic (severity labels, exploitability fields, SARIF
rule metadata) in the reports that need it, without coupling those formats to
the `PluginFinding` abstraction.

## Test Coverage Summary

| Area                                    | New tests |
| --------------------------------------- | --------- |
| `PluginFinding::with_scoring`           | 7         |
| `SecurityReviewFinding` `ScoringInput`  | 7         |
| `TechnicalReviewFinding` `ScoringInput` | 7         |
| `security_review` plugin integration    | 3         |
| `technical_review` plugin integration   | 3         |
| Total                                   | 27        |

All existing tests continue to pass. The one pre-existing test updated
(`test_technical_review_plugin_run_with_mock_provider_with_findings_returns_success`)
was corrected rather than worked around: its `confidence_threshold` was set to
`0.0` to match its stated intent of verifying finding flow, not threshold
filtering.

## Files Changed

| File                                      | Change                                                                     |
| ----------------------------------------- | -------------------------------------------------------------------------- |
| `src/reports/findings.rs`                 | Added `static_score`, `ai_score`, `with_scoring`                           |
| `src/config.rs`                           | Added `ai_confidence_weight`, `ai_analysis_enabled` to both plugin configs |
| `src/plugins/security_review/finding.rs`  | Added `ScoringInput` implementation                                        |
| `src/plugins/technical_review/finding.rs` | Added `ScoringInput` implementation                                        |
| `src/plugins/security_review/report.rs`   | JSON report accepts `&[PluginFinding]`                                     |
| `src/plugins/technical_review/report.rs`  | JSON report accepts `&[PluginFinding]`                                     |
| `src/plugins/security_review/plugin.rs`   | Scoring wiring, 3 integration tests                                        |
| `src/plugins/technical_review/plugin.rs`  | Scoring wiring, helper refactor, 3 integration tests                       |

## Next Steps

Phase 3 adds `docs/reference/confidence_scoring.md`, which catalogs every
`ScoringSignal` emitted by each plugin, its weight, and whether
`AbsoluteViolation` review mode is available. That document is designated a
mandatory update for any future plugin that computes a `confidence_score`.
