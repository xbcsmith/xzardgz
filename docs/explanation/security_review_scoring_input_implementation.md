# SecurityReviewFinding ScoringInput Implementation

## Overview

This document describes the implementation of the `ScoringInput` trait for
`SecurityReviewFinding` in
`src/plugins/security_review/finding.rs`.

The `ScoringInput` trait, defined in `src/scanner/scoring.rs`, provides a
uniform interface through which the confidence-scoring engine can extract
signals from any finding type without depending on its concrete type.

## What Was Added

### Module-Level Helper Functions

Two private helper functions were added immediately after the
`impl SecurityReviewFinding` block and before the `impl ScoringInput` block.

#### `severity_to_negative_weight(severity: FindingSeverity) -> f64`

Maps each `FindingSeverity` variant to a monotonically increasing negative
weight:

| Severity | Weight |
| -------- | ------ |
| Info     | 0.1    |
| Low      | 0.2    |
| Medium   | 0.4    |
| High     | 0.6    |
| Critical | 0.8    |

The weights are calibrated so that the deduction from the baseline confidence
score grows proportionally with severity.

#### `is_credential_category(category: &str) -> bool`

Returns `true` when the lower-cased category string contains any of:
`"secret"`, `"credential"`, `"hardcoded"`, `"api_key"`, or `"password"`.

This predicate guards the `AbsoluteViolation` signal path so that only
findings that are both `Critical` in severity AND describe a genuine
credential-exposure category produce an absolute scoring violation.

### ScoringInput Implementation

`impl crate::scanner::scoring::ScoringInput for SecurityReviewFinding` was
added with three methods:

#### `signals(&self) -> Vec<ScoringSignal>`

Always emits one `Negative` signal whose label is `"severity_<level>"` and
whose weight comes from `severity_to_negative_weight`.

Additionally emits one `AbsoluteViolation` signal when both conditions hold:

- `self.severity == FindingSeverity::Critical`
- `is_credential_category(&self.category)` returns `true`

The `AbsoluteViolation` reason string identifies the offending category so
downstream tooling and log output can attribute the violation clearly.

#### `context_for_ai(&self) -> String`

Returns a multiline string in the format:

```text
Category: <category>
Severity: <severity>
Evidence: <evidence>
Exploitability: <exploitability>
Impact: <impact>
```

This string is intended for injection into AI prompt templates and is
deliberately human-readable.

#### `plugin_name(&self) -> &str`

Returns the static string `"security_review"`, matching the plugin's
registered identifier.

## Signal Logic Summary

```text
Finding
  +-- always: Negative { label: "severity_<level>", weight: <0.1..0.8> }
  +-- if Critical AND credential category:
        AbsoluteViolation { reason: "critical credential exposure detected in category '<cat>'" }
```

## Tests Added

Seven unit tests were added inside the existing `mod tests` block under the
`// -------  ScoringInput -------` section comment:

| Test | Assertion |
| ---- | --------- |
| `test_scoring_input_signals_info_severity_emits_small_negative` | `Info` produces weight 0.1 |
| `test_scoring_input_signals_critical_non_credential_emits_single_negative` | `Critical` + non-credential category produces only one signal with weight 0.8 |
| `test_scoring_input_signals_critical_secrets_exposure_emits_violation` | `"secrets_exposure"` + Critical produces two signals, second is `AbsoluteViolation` |
| `test_scoring_input_signals_critical_credential_category_emits_violation` | `"hardcoded_credential"` + Critical produces `AbsoluteViolation` |
| `test_scoring_input_plugin_name_is_security_review` | `plugin_name()` returns `"security_review"` |
| `test_scoring_input_context_for_ai_contains_category_and_evidence` | Context string contains category and evidence substrings |
| `test_scoring_input_severity_weights_are_monotonically_increasing` | Weights for all five severities are strictly increasing |

## Design Decisions

- Full paths (`crate::scanner::scoring::ScoringInput`, etc.) are used in the
  `impl` block header and method bodies rather than a module-level `use`
  statement, keeping the scoring dependency localised to the implementation
  block.
- The helper functions are private (`fn`, not `pub fn`) because they encode
  domain-specific calibration details that should not form part of the public
  API.
- No new dependencies were introduced; this builds entirely on types already
  present in Phase 1.
