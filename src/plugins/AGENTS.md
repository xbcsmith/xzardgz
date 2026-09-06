# Plugin Subsystem Agent Guidelines

See the root `AGENTS.md` for general coding standards, error handling rules, and
quality gates that apply to all code in this project. The rules here are
additive and specific to the plugin subsystem.

## Confidence Scoring Requirements

### ScoringInput Trait

Every plugin that produces a `confidence_score` on its finding type MUST
implement `ScoringInput` for that type. The trait is defined in
`src/scanner/scoring.rs` and requires three methods:

```rust
fn signals(&self) -> Vec<ScoringSignal>;
fn context_for_ai(&self) -> String;
fn plugin_name(&self) -> &str;
```

- `signals()` returns the full list of `ScoringSignal` variants the finding
  carries. The scorer iterates these to compute the static confidence.
- `context_for_ai()` returns the string sent to the AI leg when
  `ai_analysis_enabled` is true. Make it specific enough for the AI to reason
  about the finding without additional context.
- `plugin_name()` returns a stable, lowercase identifier for the plugin. This
  value appears in logs and in the signal catalog.

Do not implement `ScoringInput` on a type that does not carry findings.
Implementing it on an intermediate or internal type causes signal accounting
errors.

### Signal Catalog Update

`docs/reference/confidence_scoring.md` MUST be updated before a plugin's scoring
implementation is considered complete. A pull request that adds or changes
scoring without updating this file will be rejected.

Add a new subsection under `## Plugin Signal Catalogs` with the following
information:

- Every `ScoringSignal` variant the plugin can emit
- The label or reason string for each signal
- The weight for each `Positive` or `Negative` signal
- The trigger condition for each `AbsoluteViolation` signal
- Whether `review_violations` mode is enabled for the plugin and the reason for
  that choice

### Configuration Requirements

Any new plugin config struct that uses `ConfidenceScorer` MUST include these two
fields from its first version:

```rust
pub ai_confidence_weight: f64,
pub ai_analysis_enabled: bool,
```

Do not add them in a follow-up. Omitting them from the initial struct means any
config files written against that version are missing required fields and must
be migrated.

`review_violations` MUST be set explicitly in the plugin and documented in the
signal catalog. Do not rely on the default. The default is `false` because
`AbsoluteViolation` findings bypass the confidence threshold entirely and floor
the score to `0.0`. Review mode should only be enabled when an AI leg is
required to qualify a hard violation before it reaches reviewers. If you enable
it, document the justification in the signal catalog entry.

### Required Tests

Every plugin scoring implementation requires these three tests at minimum.

**AbsoluteViolation bypasses the threshold:**

```rust
#[test]
fn test_absolute_violation_passes_filter_regardless_of_threshold() {
    // Construct a finding that emits an AbsoluteViolation signal.
    // Set confidence_threshold to 1.0 so nothing passes on score alone.
    // Assert the finding is retained after the filter.
}
```

**`ai_confidence_weight` shifts blended confidence:**

```rust
#[test]
fn test_ai_confidence_weight_shifts_blended_score() {
    // Run scoring twice with the same static signals but different
    // ai_confidence_weight values (e.g. 0.0 vs 0.5).
    // Assert the blended scores differ and move in the expected direction.
}
```

**`ai_analysis_enabled: false` disables the AI leg:**

```rust
#[test]
fn test_ai_analysis_disabled_yields_none_ai_score_and_blended_equals_static() {
    // Run scoring with ai_analysis_enabled: false.
    // Assert ai_score is None.
    // Assert blended_score equals static_score.
}
```

Name tests using the pattern `test_<function>_<condition>_<expected>` as
required by the root `AGENTS.md`.

### Filter Pattern

Use this exact pattern in `run()` to apply the confidence threshold while
preserving `AbsoluteViolation` findings:

```rust
scored.retain(|(_, result)| {
    result.has_violations() || result.blended_score >= config.confidence_threshold
});
```

Do not inline a different condition. `has_violations()` is the canonical check
for whether a result carries any `AbsoluteViolation`-derived finding. Using a
manual flag or re-checking signal variants in `run()` is incorrect and will
diverge from the scorer's own accounting.
