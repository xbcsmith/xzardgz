# Confidence Scoring: Phase 1 Implementation

This document records what was actually built for Phase 1 ("Redesign the Scoring
Core") of
[`confidence_scoring_integration_plan.md`](confidence_scoring_integration_plan.md).
It covers the motivation for the rewrite, every new type and constant
introduced, the algorithms in detail, the configuration surface, and what the
test suite now asserts.

## Summary

`src/scanner/scoring.rs` was completely rewritten. The old linear-average model
(a flat `ScoringSignal { name, weight, value }` struct with a hardcoded AI
weight of `2.0`) was replaced by a baseline-fold model built around three
cooperating public types -- `ScoringSignal` (enum), `ScoringInput` (trait),
`ConfidenceScorer` (struct) -- and two supporting value types -- `ScoringConfig`
and `ScoringResult`.

The rewrite had no production call sites to migrate. Phase 1 is therefore a
clean break: all changes are internal to `src/scanner/scoring.rs` and the
re-export list in `src/scanner/mod.rs`. No plugin code was changed.

## Previous State and Motivation

### What existed before

- `ScoringSignal { name: String, weight: f64, value: f64 }` -- a flat struct
  with three fields.
- `ScoringInput { signals: Vec<ScoringSignal> }` -- a thin container, not a
  trait.
- `ConfidenceScorer` -- a stateless unit struct.
- `ConfidenceScorer::score()` -- computed a weighted average clamped to
  `[0.0, 1.0]`.
- `compute_finding_confidence(ai_confidence, scanner_signals)` -- a convenience
  function that treated the AI score as one signal with a hardcoded weight of
  `2.0`.

### Identified problems

1. **No hard-violation concept.** A deterministic rule breach (such as a
   hardcoded credential) could not floor the score; it was merely another
   weighted input to the average.
2. **Hardcoded AI weight.** The `2.0` constant in `compute_finding_confidence`
   could not be changed by operators or tests without modifying source code.
3. **No production call sites.** The `security_review` plugin filtered findings
   on the AI's raw self-reported `confidence` field and never called
   `ConfidenceScorer` or `compute_finding_confidence` at all. The library
   existed but had no effect on shipped behavior.

## New API

### Constants

| Constant                  | Value | Meaning                                                                      |
| ------------------------- | ----- | ---------------------------------------------------------------------------- |
| `VIOLATION_FLOOR`         | `0.0` | Score applied when an `AbsoluteViolation` is encountered in default mode     |
| `VIOLATION_REVIEW_WEIGHT` | `0.9` | Effective `Negative` weight used for a violation in `review_violations` mode |

Both constants are re-exported from `src/scanner/mod.rs` alongside the public
types.

### `ScoringSignal` (enum)

Replaces the old flat struct. Three variants:

```rust
pub enum ScoringSignal {
    Positive { label: String, weight: f64 },
    Negative { label: String, weight: f64 },
    AbsoluteViolation { reason: String },
}
```

The arithmetic applied by each variant during the baseline fold is:

| Variant               | Effect on `score`                                            |
| --------------------- | ------------------------------------------------------------ |
| `Negative { weight }` | `score *= 1.0 - weight.clamp(0.0, 1.0)`                      |
| `Positive { weight }` | `score += (1.0 - score) * weight.clamp(0.0, 1.0)`            |
| `AbsoluteViolation`   | floors to `VIOLATION_FLOOR` and returns early (default mode) |

Both `weight` fields are clamped inside the scorer, not at construction time, so
out-of-range values in serialized data do not panic.

Methods:

- `display_label() -> &str` -- returns the `label` for `Positive`/`Negative`, or
  `reason` for `AbsoluteViolation`.
- `is_absolute_violation() -> bool` -- returns `true` only for the
  `AbsoluteViolation` variant.

### `ScoringInput` (trait)

Replaces the old `ScoringInput` struct. Finding types implement this trait to
participate in the scoring pipeline.

```rust
pub trait ScoringInput {
    fn signals(&self) -> Vec<ScoringSignal>;
    fn context_for_ai(&self) -> String;
    fn plugin_name(&self) -> &str;
}
```

- `signals` returns the ordered signal list for the baseline fold.
- `context_for_ai` returns a human-readable string for inclusion in an AI
  prompt.
- `plugin_name` identifies the plugin that produced the item.

Every consumer of this trait must expose `ai_confidence_weight` and
`ai_analysis_enabled` from its first version, not deferred to a later phase.

### `ScoringConfig` (new struct)

Controls the AI blend leg and violation handling.

```rust
pub struct ScoringConfig {
    pub ai_confidence_weight: f64,   // default 0.5
    pub ai_analysis_enabled: bool,   // default true
    pub review_violations: bool,     // default false
}
```

| Field                  | Default | Description                                                                                |
| ---------------------- | ------- | ------------------------------------------------------------------------------------------ |
| `ai_confidence_weight` | `0.5`   | Weight of the AI score in the final blend; clamped to `[0.0, 1.0]` at blend time           |
| `ai_analysis_enabled`  | `true`  | Master switch; when `false`, the blend step is skipped entirely                            |
| `review_violations`    | `false` | When `true`, `AbsoluteViolation` signals are folded as `Negative(0.9)` instead of flooring |

`ScoringConfig` implements `Default`, `Debug`, `Clone`, `Serialize`, and
`Deserialize`.

### `ScoringResult` (new struct)

The output record of a full `ConfidenceScorer::score` call.

```rust
pub struct ScoringResult {
    pub static_score: f64,
    pub ai_score: Option<f64>,
    pub blended_score: f64,
    pub violation_reasons: Vec<String>,
}
```

| Field               | Description                                                                          |
| ------------------- | ------------------------------------------------------------------------------------ |
| `static_score`      | Score produced by the baseline fold alone; always in `[0.0, 1.0]`                    |
| `ai_score`          | Raw AI-reported confidence if AI analysis was active and succeeded; `None` otherwise |
| `blended_score`     | Final score after the AI blend; equals `static_score` when AI is absent or disabled  |
| `violation_reasons` | Reason strings from every `AbsoluteViolation` encountered during the fold            |

`ScoringResult` provides one method:

- `has_violations() -> bool` -- returns `true` when `violation_reasons` is
  non-empty.

The separation of `static_score`, `ai_score`, and `blended_score` as distinct
fields provides a complete audit trail for every scoring decision and is carried
directly into plugin-level JSON reports in Phase 2.

### `ConfidenceScorer` (redesigned)

Was a stateless unit struct. Now holds a public `config: ScoringConfig` field.

```rust
pub struct ConfidenceScorer {
    pub config: ScoringConfig,
}
```

Constructors:

- `ConfidenceScorer::new(config: ScoringConfig) -> Self`
- `ConfidenceScorer::with_defaults() -> Self` -- equivalent to
  `new(ScoringConfig::default())`

Methods:

- `score_static(&self, signals: &[ScoringSignal]) -> (f64, Vec<String>)` -- pure
  baseline fold; returns the clamped static score and any violation reasons.
- `blend(&self, static_score: f64, ai_score: Option<f64>) -> f64` -- AI blend
  step; returns `static_score` unchanged when AI is disabled or the score is
  `None`.
- `score(&self, input: &dyn ScoringInput, ai_score: Option<f64>) -> ScoringResult`
  -- full pipeline; calls `score_static` then `blend` and packs results into a
  `ScoringResult`.

## Algorithms

### Baseline fold

`score_static` starts at `1.0` and processes each signal in insertion order.

```text
score = 1.0
for each signal:
    Negative { weight }:
        score = score * (1.0 - weight.clamp(0.0, 1.0))
    Positive { weight }:
        score = score + (1.0 - score) * weight.clamp(0.0, 1.0)
    AbsoluteViolation { reason }:
        push reason to violation_reasons
        if review_violations == false:
            return (VIOLATION_FLOOR, violation_reasons)   // short-circuit
        else:
            score = score * (1.0 - VIOLATION_REVIEW_WEIGHT)   // fold as Negative(0.9)
return (score.clamp(0.0, 1.0), violation_reasons)
```

**Empty signals** return `(1.0, [])`. No detections means full confidence: the
repository or finding is considered clean until evidence says otherwise.

#### Worked example -- compound signals

Starting score: `1.0`

1. `Negative { weight: 0.4 }` -- score after: `1.0 * 0.6 = 0.6`
2. `Positive { weight: 0.2 }` -- score after:
   `0.6 + (1.0 - 0.6) * 0.2 = 0.6 + 0.08 = 0.68`
3. `Negative { weight: 0.3 }` -- score after: `0.68 * 0.7 = 0.476`

Final static score: `0.476`

Note that multiple `Negative` signals compound: applying `Negative(0.4)` twice
yields `1.0 * 0.6 * 0.6 = 0.36`, not `1.0 * 0.2 = 0.8` (i.e., the effects are
multiplicative, not additive). Likewise, a `Positive` applied at the ceiling
(`score = 1.0`) has zero effect, because `(1.0 - 1.0) * w = 0`.

#### Worked example -- `AbsoluteViolation` in default mode

Signals: `[Negative(0.3), AbsoluteViolation("cred_leak"), Positive(0.5)]`

1. `Negative(0.3)` -- score: `0.7`
2. `AbsoluteViolation("cred_leak")` -- `review_violations = false`, so return
   immediately.
   - `static_score = 0.0`, `violation_reasons = ["cred_leak"]`
3. `Positive(0.5)` -- never reached.

The short-circuit ensures that no subsequent signal can rescue a hard violation.

#### Worked example -- `AbsoluteViolation` in `review_violations` mode

Same signals, but `review_violations = true`:

1. `Negative(0.3)` -- score: `0.7`
2. `AbsoluteViolation("cred_leak")` -- folded as `Negative(0.9)`:
   `0.7 * 0.1 = 0.07`
3. `Positive(0.5)` -- score: `0.07 + (1.0 - 0.07) * 0.5 = 0.07 + 0.465 = 0.535`

`static_score = 0.535`, `violation_reasons = ["cred_leak"]`.

The AI blend step can now move the final result, while `has_violations()` still
returns `true` so callers know a deterministic rule breach was present.

### AI blend

`blend` applies the following formula when `ai_analysis_enabled` is `true` and
`ai_score` is `Some(ai)`:

```text
w = ai_confidence_weight.clamp(0.0, 1.0)
blended = static_score * (1.0 - w) + ai_score * w
blended = blended.clamp(0.0, 1.0)
```

At the default weight of `0.5`:

```text
static = 0.6,  ai = 1.0  =>  0.6 * 0.5 + 1.0 * 0.5 = 0.8
static = 0.6,  ai = 0.2  =>  0.6 * 0.5 + 0.2 * 0.5 = 0.4
```

At `w = 0.0` the AI score has no effect; the blended result equals
`static_score`. At `w = 1.0` only the AI score matters.

Any AI-side failure collapses to a `None` pass-through: `blend(static, None)`
always returns `static`. This covers provider outages, malformed JSON responses,
and any other upstream parse error. This is success criterion 1.6 from the
integration plan.

When `ai_analysis_enabled` is `false`, `blend` returns `static_score` regardless
of what `ai_score` contains, and `ScoringResult.ai_score` is set to `None` so
the field never carries a stale value in the audit record.

## Configuration Reference

```rust
let cfg = ScoringConfig {
    ai_confidence_weight: 0.3,   // AI leg contributes 30 %
    ai_analysis_enabled: true,
    review_violations: false,    // AbsoluteViolation hard-floors
};
let scorer = ConfidenceScorer::new(cfg);
```

| Scenario                                     | Recommended settings                                    |
| -------------------------------------------- | ------------------------------------------------------- |
| Production, default behavior                 | `ScoringConfig::default()`                              |
| Disable AI entirely (offline / cost control) | `ai_analysis_enabled: false`                            |
| Raise AI influence above scanner evidence    | `ai_confidence_weight: 0.7` or higher                   |
| Allow AI to review absolute violations       | `review_violations: true`                               |
| Static-only scoring, no AI leg               | `ai_analysis_enabled: false, ai_confidence_weight: 0.0` |

## Test Coverage

38 unit tests in `src/scanner/scoring.rs` under `#[cfg(test)]`. Tests are
organized into groups:

### `ScoringSignal` methods (6 tests)

- `display_label` returns the `label` field for `Positive` and `Negative`
  variants.
- `display_label` returns the `reason` field for `AbsoluteViolation`.
- `is_absolute_violation` returns `true` only for `AbsoluteViolation`.
- `is_absolute_violation` returns `false` for `Positive` and `Negative`.

### `ScoringConfig` defaults (3 tests)

- `ai_confidence_weight` defaults to `0.5`.
- `ai_analysis_enabled` defaults to `true`.
- `review_violations` defaults to `false`.

### `ScoringResult` (2 tests)

- `has_violations` returns `false` when `violation_reasons` is empty.
- `has_violations` returns `true` when `violation_reasons` is non-empty.

### `ConfidenceScorer` construction (2 tests)

- `new` stores config fields verbatim.
- `with_defaults` produces the same result as `new(ScoringConfig::default())`.

### `score_static` -- baseline fold arithmetic (10 tests)

- Empty signals return `1.0` with no violations.
- Zero-weight `Negative` has no effect on the score.
- Zero-weight `Positive` has no effect on the score.
- Single `Negative` proportionally reduces the score.
- `Positive` applied at ceiling (`score = 1.0`) has no effect.
- `Negative` followed by `Positive` partially recovers the score.
- Multiple `Negative` signals compound multiplicatively.
- Multiple `Positive` signals compound (each narrows the remaining gap to
  `1.0`).

### `score_static` -- `AbsoluteViolation` (5 tests)

- `AbsoluteViolation` floors the score to `VIOLATION_FLOOR` in default mode.
- `AbsoluteViolation` records its reason in `violation_reasons`.
- Short-circuit: signals after `AbsoluteViolation` are never applied in default
  mode.
- `review_violations` mode folds instead of flooring; score is
  `> VIOLATION_FLOOR`.
- `review_violations` mode still records the reason in `violation_reasons`.
- Multiple violations in `review_violations` mode compound their `Negative(0.9)`
  penalties.

### `blend` (6 tests)

- `None` AI score returns `static_score` unchanged.
- `ai_analysis_enabled = false` returns `static_score` regardless of the AI
  score provided.
- Default weight `0.5` produces the correct arithmetic result.
- `ai_confidence_weight = 0.0` uses only the static score.
- `ai_confidence_weight = 1.0` uses only the AI score.
- An out-of-range weight is clamped before blending; result stays in
  `[0.0, 1.0]`.

### `score` (full pipeline, 8 tests)

- No AI score: `blended_score == static_score`.
- Valid AI score: blended score matches the expected arithmetic.
- `AbsoluteViolation` floors `static_score` and propagates through to
  `blended_score`.
- `None` AI score (simulated provider error) falls back to the static score
  unchanged.
- Empty signals return full confidence with no violations.
- `review_violations` mode: AI can move the blended score above
  `VIOLATION_FLOOR` even when a violation was present.
- `ai_analysis_enabled = false`: `ScoringResult.ai_score` is `None` even when a
  score was passed in.
- Configurable `ai_confidence_weight` visibly shifts the blended result in a
  deterministic, numerically exact way.

## Files Changed

| File                     | Change                                                                                                           |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| `src/scanner/scoring.rs` | Complete rewrite: old struct-based model replaced with the baseline-fold model described here                    |
| `src/scanner/mod.rs`     | Re-export list updated to include `ScoringConfig`, `ScoringResult`, `VIOLATION_FLOOR`, `VIOLATION_REVIEW_WEIGHT` |

No other files were modified. No call sites outside the scoring module itself
required updating because the old API had no production callers.

## What Was Removed

| Removed item                                          | Reason                                                                                                             |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `ScoringSignal { name, weight, value }` struct        | Replaced by the enum; `value` field served no purpose in the old weighted average                                  |
| `ScoringInput { signals: Vec<ScoringSignal> }` struct | Replaced by the `ScoringInput` trait                                                                               |
| `compute_finding_confidence()` function               | Hardcoded `2.0` AI weight; functionality subsumed by `ConfidenceScorer::score` with a configurable `ScoringConfig` |
| `ConfidenceScorer` as a unit struct                   | Now holds `ScoringConfig`                                                                                          |

## Next Steps: Phase 2

Phase 2 wires the new scoring core into the shipped plugins:

1. Implement `ScoringInput` for `SecurityReviewFinding`: map `severity` to
   `Negative` signals and define at least one concrete `AbsoluteViolation`
   trigger (e.g. a high-confidence hardcoded-credential finding).
2. Implement the equivalent for the technical-review plugin's finding type.
3. Replace the raw `f.confidence >= config.confidence_threshold` filter in
   `plugins/security_review/plugin.rs` with `ConfidenceScorer::score()`,
   treating the AI's self-reported confidence as the `ai_score` input.
4. Add `ai_confidence_weight` and `ai_analysis_enabled` to
   `SecurityReviewConfig` and `TechnicalReviewConfig`.
5. Expose `static_score`, `ai_score`, and `blended_score` as distinct fields in
   both plugins' JSON report output.

Success criterion for Phase 2: `RiskBand` derivation is consistently based on
the blended score across both plugins, and disabling `ai_analysis_enabled`
produces identical results to a build with no configured AI provider.
