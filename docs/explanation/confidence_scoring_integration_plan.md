# Confidence Scoring Integration Implementation Plan

## Overview

`src/scanner/scoring.rs` defines a generic weighted-average `ConfidenceScorer`/
`ScoringSignal` model, but it has no production callers: `security_review`
filters findings purely on the AI's self-reported `confidence` field
(`plugins/security_review/plugin.rs:203`), never blending in any
scanner-derived signal. This plan replaces the current flat, linear-average
scoring model with a baseline-fold model that supports hard-violation
signals, makes the AI-blend weight configurable from day one, and wires the
result into both shipped plugins' actual finding-filtering logic.

## Current State Analysis

### Existing Infrastructure

- `src/scanner/scoring.rs::ScoringSignal { name, weight, value }` and
  `ConfidenceScorer::score()` compute a weighted average clamped to
  `[0.0, 1.0]`.
- `compute_finding_confidence(ai_confidence, scanner_signals)` treats AI
  confidence as one signal with a hardcoded weight of `2.0`.
- `src/scanner/preselect.rs::PluginContentScanner` already produces
  pattern-based `ScanFinding`s that are a natural source of static scoring
  signals but are never fed into the scorer today.

### Identified Issues

- The current model has no concept of a hard-violation signal (a
  deterministic rule breach that should floor the score), and its
  linear-average shape does not compose naturally with one.
- The AI-confidence weight is a hardcoded constant (`2.0`), not
  configuration, so operators cannot tune how much the AI's opinion counts
  relative to scanner evidence.
- `security_review::plugin::run` never calls `compute_finding_confidence` or
  `ConfidenceScorer` at all — it filters directly on the AI's raw
  self-reported number, so the scoring library exists but has no effect on
  shipped behavior.

## Implementation Phases

### Phase 1: Redesign the Scoring Core

#### 1.1 Foundation Work

Design a `ScoringSignal` enum with three variants — `Positive { label,
weight }`, `Negative { label, weight }`, `AbsoluteViolation { reason }` — and
a `ScoringInput` trait (`signals()`, `context_for_ai()`, `plugin_name()`),
replacing the current flat struct in `src/scanner/scoring.rs`.

#### 1.2 Add Foundation Functionality

Implement `ConfidenceScorer::score()` as a baseline fold: start at a
normalized ceiling, apply `Negative { weight }` as a proportional deduction
and `Positive { weight }` as a proportional recovery, in signal order. An
`AbsoluteViolation` short-circuits the result to a floor value immediately
unless an opt-in `review_violations` mode is active, in which case it is
rewritten to a heavily-weighted `Negative` and folded normally so an AI leg
can still weigh in. Replace the hardcoded `2.0`-weight AI blend with a
configurable `ai_confidence_weight: f32` (default `0.5`, clamped to
`[0.0, 1.0]` at blend time) and an `ai_analysis_enabled: bool` (default
`true`) master switch: `final = static_score * (1.0 - w) + ai_score * w`.
Both fields are introduced now, not deferred — every new `ScoringInput`
consumer must expose them from its first version.

#### 1.3 Integrate Foundation Work

There are no production call sites to migrate today (only the module's own
tests), so this is a clean rewrite. Update `scanner/scoring.rs`'s test module
for the new model: baseline-fold arithmetic, `AbsoluteViolation` short-circuit,
AI-blend clamping, and AI-failure fallback to the static score unchanged.

#### 1.4 Testing Requirements

Cover: empty/zero-weight input, a single `AbsoluteViolation` flooring the
score, `review_violations` mode blending instead of flooring, and a
simulated provider error/malformed JSON always falling back to the static
score.

#### 1.5 Deliverables

A rewritten `scanner/scoring.rs` exposing `ScoringSignal`, `ScoringInput`,
`ConfidenceScorer`, and the `ai_confidence_weight`/`ai_analysis_enabled`
configuration pair.

#### 1.6 Success Criteria

The scorer never panics on empty or degenerate input, and an AI-side failure
of any kind always yields the same result a static-only run would have
produced.

### Phase 2: Wire Scoring into Shipped Plugins

#### 2.1 Feature Work

Implement `ScoringInput` for `SecurityReviewFinding`: map `severity` to
`Negative` signals of increasing weight, and introduce at least one concrete
`AbsoluteViolation` trigger now (e.g. a high-confidence hardcoded-credential
finding), not deferred to a later plugin. Implement the equivalent for
technical-review's finding set.

#### 2.2 Integrate Feature

Replace the raw `f.confidence >= config.confidence_threshold` filter in
`plugins/security_review/plugin.rs:203` with `ConfidenceScorer::score()`'s
blended output, treating the AI's self-reported confidence as one weighted
input to the blend rather than the sole gate.

#### 2.3 Configuration Updates

Add `ai_confidence_weight` and `ai_analysis_enabled` to
`SecurityReviewConfig` and `TechnicalReviewConfig`.

#### 2.4 Testing Requirements

Add plugin-level tests asserting: a provider outage or malformed JSON
response falls back to the static-only score; an `AbsoluteViolation` finding
floors the score as designed; and the configured `ai_confidence_weight`
visibly shifts the blended result in a deterministic, testable way.

#### 2.5 Deliverables

Both plugins score findings via the shared `ConfidenceScorer`, and both JSON
reports expose the static score, the raw AI score, and the blended
`confidence_score` as separate audit fields.

#### 2.6 Success Criteria

`RiskBand` derivation is consistently based on the blended score across both
plugins. Disabling `ai_analysis_enabled` produces identical results to a
build with no configured AI provider at all.

### Phase 3: Document the Signal Catalog

#### 3.1 Feature Work

Add `docs/reference/confidence_scoring.md` cataloging, per plugin, every
`ScoringSignal` it can emit, its weight, and whether `AbsoluteViolation`
review is enabled.

#### 3.2 Integrate Feature

Establish this document as a mandatory update for any future plugin that
computes a `confidence_score` — call this out explicitly in
`src/plugins/AGENTS.md` or an equivalent contributor note.

#### 3.3 Configuration Updates

None.

#### 3.4 Testing Requirements

None (documentation deliverable).

#### 3.5 Deliverables

`docs/reference/confidence_scoring.md` covering `security_review` and
`technical_review`.

#### 3.6 Success Criteria

A future plugin's scoring behavior cannot be considered complete until this
document reflects it, mirroring the review discipline already required by
the repository's own `AGENTS.md`.
