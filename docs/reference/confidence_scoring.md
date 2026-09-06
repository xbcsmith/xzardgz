# Confidence Scoring Signal Catalog

## Overview

The confidence scoring system uses a baseline-fold model to produce a numeric
confidence score for each finding emitted by a plugin. The model operates in two
stages.

First, the static score is computed by folding a sequence of `ScoringSignal`
variants over a baseline of `1.0`. Each signal either recovers score
proportionally, deducts score proportionally, or floors the score immediately to
`0.0` (an `AbsoluteViolation`). The result is the static score.

Second, the static score is blended with an optional AI-reported confidence
value using a configurable weight `w`. The blended score is what plugins use for
threshold filtering and what appears in JSON reports.

Every finding in a JSON report includes three audit fields: `confidence`
(blended score), `static_score` (pre-blend result), and `ai_score` (raw AI
value, or `null` when unavailable).

## Scoring Mechanics

### Signal Variants

The `ScoringSignal` enum is defined in `src/scanner/scoring.rs` and has three
variants.

| Variant             | Fields                         | Effect                                                  |
| ------------------- | ------------------------------ | ------------------------------------------------------- |
| `Positive`          | `label: String`, `weight: f64` | `score += (1.0 - score) * weight`                       |
| `Negative`          | `label: String`, `weight: f64` | `score *= (1.0 - weight)`                               |
| `AbsoluteViolation` | `reason: String`               | Floors score to `0.0`; short-circuits remaining signals |

### Fold Algorithm

Signals are folded left over a starting score of `1.0`. Processing stops
immediately when an `AbsoluteViolation` is encountered in normal mode.

```rust
// Simplified fold logic
let mut score = 1.0_f64;
for signal in signals {
    match signal {
        ScoringSignal::Positive { weight, .. } => {
            score += (1.0 - score) * weight;
        }
        ScoringSignal::Negative { weight, .. } => {
            score *= 1.0 - weight;
        }
        ScoringSignal::AbsoluteViolation { .. } => {
            score = VIOLATION_FLOOR; // 0.0
            break;
        }
    }
}
```

### Constants

| Constant                  | Value | Meaning                                                                   |
| ------------------------- | ----- | ------------------------------------------------------------------------- |
| `VIOLATION_FLOOR`         | `0.0` | Static score applied when `AbsoluteViolation` fires in normal mode        |
| `VIOLATION_REVIEW_WEIGHT` | `0.9` | Effective `Negative` weight applied to `AbsoluteViolation` in review mode |

### Review Violations Mode

When `ScoringConfig::review_violations` is `true`, an `AbsoluteViolation` is not
treated as a floor. Instead it is folded as `Negative { weight: 0.9 }`, allowing
a non-zero static score to survive. Processing does not short-circuit. The
`reason` string is preserved in the signal for reporting purposes.

### Blend Formula

After the static score is computed, it is blended with an AI-reported confidence
value:

```text
blended = static_score * (1.0 - w) + ai_score * w
```

The weight `w` is taken from `ScoringConfig::ai_confidence_weight` and is
clamped to `[0.0, 1.0]` at blend time.

### AI Failure Fallback

The blend step is skipped entirely and `blended = static_score` when either of
the following conditions holds:

- `ScoringConfig::ai_analysis_enabled` is `false`
- The AI provider returned an error or malformed JSON (`ai_score` is `None`)

In both cases the `ai_score` audit field in the JSON report is `null`.

### ScoringConfig Fields

| Field                  | Type   | Default | Description                                                      |
| ---------------------- | ------ | ------- | ---------------------------------------------------------------- |
| `ai_confidence_weight` | `f64`  | `0.5`   | Blend weight `w`, clamped to `[0.0, 1.0]`                        |
| `ai_analysis_enabled`  | `bool` | `true`  | When `false`, blend is skipped and `blended == static_score`     |
| `review_violations`    | `bool` | `false` | When `true`, `AbsoluteViolation` is folded as a heavy `Negative` |

## Plugin Signal Catalogs

Each plugin implements `ScoringInput` for its finding type. The tables below
catalog every signal a plugin can emit, the condition under which it fires, and
its weight. Signals are listed in fold order: unconditional signals fire first,
then conditional signals.

### security_review

Source: `src/plugins/security_review/finding.rs`

`review_violations` mode: `false`. An `AbsoluteViolation` always floors the
static score to `0.0` for this plugin.

**Unconditional signals (one emitted per finding)**

| Signal variant | Label               | Weight | Condition              |
| -------------- | ------------------- | ------ | ---------------------- |
| `Negative`     | `severity_info`     | `0.1`  | `severity == Info`     |
| `Negative`     | `severity_low`      | `0.2`  | `severity == Low`      |
| `Negative`     | `severity_medium`   | `0.4`  | `severity == Medium`   |
| `Negative`     | `severity_high`     | `0.6`  | `severity == High`     |
| `Negative`     | `severity_critical` | `0.8`  | `severity == Critical` |

**Conditional signals**

| Signal variant      | Reason                                                             | Condition                                                                                                                                                      |
| ------------------- | ------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AbsoluteViolation` | `"critical credential exposure detected in category '<category>'"` | `severity == Critical` AND `category` contains any of: `"secret"`, `"credential"`, `"hardcoded"`, `"api_key"`, `"password"` (case-insensitive substring match) |

**SecurityReviewConfig fields**

| Field                  | Type   | Default |
| ---------------------- | ------ | ------- |
| `confidence_threshold` | `f64`  | `0.5`   |
| `ai_confidence_weight` | `f64`  | `0.5`   |
| `ai_analysis_enabled`  | `bool` | `true`  |

**Filter logic**

A finding is included in the report when:

```text
result.has_violations() || result.blended_score >= confidence_threshold
```

Findings that triggered an `AbsoluteViolation` satisfy `has_violations()` and
bypass the threshold check entirely.

### technical_review

Source: `src/plugins/technical_review/finding.rs`

`review_violations` mode: `false`. An `AbsoluteViolation` always floors the
static score to `0.0` for this plugin.

**Unconditional signals (one emitted per finding)**

| Signal variant | Label               | Weight | Condition              |
| -------------- | ------------------- | ------ | ---------------------- |
| `Negative`     | `severity_info`     | `0.1`  | `severity == Info`     |
| `Negative`     | `severity_low`      | `0.2`  | `severity == Low`      |
| `Negative`     | `severity_medium`   | `0.4`  | `severity == Medium`   |
| `Negative`     | `severity_high`     | `0.6`  | `severity == High`     |
| `Negative`     | `severity_critical` | `0.8`  | `severity == Critical` |

**Conditional signals**

| Signal variant      | Reason                                                     | Condition                                                                                                            |
| ------------------- | ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `AbsoluteViolation` | `"critical dependency violation in category '<category>'"` | `severity == Critical` AND `category` contains `"dependency"` OR `"supply_chain"` (case-insensitive substring match) |

**TechnicalReviewConfig fields**

| Field                  | Type   | Default |
| ---------------------- | ------ | ------- |
| `confidence_threshold` | `f64`  | `0.7`   |
| `ai_confidence_weight` | `f64`  | `0.5`   |
| `ai_analysis_enabled`  | `bool` | `true`  |

**Filter logic**

A finding is included in the report when:

```text
result.has_violations() || result.blended_score >= confidence_threshold
```

Findings that triggered an `AbsoluteViolation` satisfy `has_violations()` and
bypass the threshold check entirely.

## JSON Report Audit Fields

Source: `src/reports/findings.rs` (`PluginFinding`)

Every finding emitted by both plugins includes the following three fields in its
JSON report output.

| Field          | Type            | Always present | Description                                                                                             |
| -------------- | --------------- | -------------- | ------------------------------------------------------------------------------------------------------- |
| `confidence`   | `f64`           | Yes            | Blended score: `static_score * (1 - w) + ai_score * w`. Equals `static_score` when AI blend is skipped. |
| `static_score` | `f64`           | Yes            | Result of the signal fold before AI blend. Defaults to `1.0` for unscored findings.                     |
| `ai_score`     | `f64` or `null` | Yes (nullable) | Raw AI-reported confidence value. `null` when `ai_analysis_enabled` is `false` or the AI call failed.   |

Example JSON output for a finding with a successful AI blend:

```rust
// Rendered as JSON in the report
// {
//   "confidence": 0.325,
//   "static_score": 0.5,
//   "ai_score": 0.15
// }
//
// Derivation: static=0.5, ai=0.15, w=0.5
// blended = 0.5 * 0.5 + 0.15 * 0.5 = 0.325
```

Example JSON output when AI blend is unavailable:

```rust
// {
//   "confidence": 0.5,
//   "static_score": 0.5,
//   "ai_score": null
// }
```

## Adding a New Plugin

Any plugin that implements `ScoringInput` for a new finding type must add its
complete signal catalog to this document before the implementation is considered
complete. This requirement is stated in `src/plugins/AGENTS.md` and applies
without exception.

The catalog entry must document:

- The source file path of the `ScoringInput` implementation
- The `review_violations` mode in effect for the plugin
- Every unconditional signal, its label, and its weight
- Every conditional signal, its full condition, and the exact `reason` string
  used in any `AbsoluteViolation`
- All plugin config fields with their types and defaults
- The filter logic used to include or exclude findings from reports

Omitting any signal from the catalog is a documentation defect. The catalog is
the authoritative source for operators configuring thresholds and for reviewers
auditing why a finding received a particular score.
