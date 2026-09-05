# Investigation Turn-Budgeting and Batching Implementation Plan

## Overview

`src/investigation/{batch,scope,strategy}.rs` are thin, currently-unused
helper types. Once plugins drive real multi-turn `AgentSession` investigation
(see the companion agent tool-calling integration plan), large repositories
need a real turn budget and a batching strategy so a single session does not
exhaust its context window or run indefinitely. This plan implements
turn-budget scaling, single-versus-batched session strategy selection, and a
degrade-and-warn behavior when a batch exhausts its turn budget: the run
completes with partial findings rather than failing outright, and a warning
is surfaced in both the plugin's results and the process logs.

## Current State Analysis

### Existing Infrastructure

- `src/investigation/scope.rs`, `batch.rs`, and `strategy.rs` exist as
  placeholder types with minimal logic today.
- `src/scanner/mod.rs` already produces a `PluginPreselection` block (risky /
  secrets-like / unsafe-Rust / command-exec / network / auth file lists) as
  part of `ScanResult`, which is a ready-made source for a matched-file count.
- `src/diagnostics.rs::Diagnostic`/`Diagnostics` already provides a
  structured way to attach warnings to a plugin run's output.

### Identified Issues

- No turn-budget computation exists; any future `AgentSession` usage would
  need either a fixed, un-scaled cap or ad hoc logic per plugin.
- No batching strategy exists for large repositories, so a single session
  investigating a very large or heavily-matched repository risks exceeding
  its turn budget with no defined recovery behavior.

## Implementation Phases

### Phase 1: Turn Budgeting and Strategy Selection

#### 1.1 Foundation Work

Flesh out `InvestigationScope { total_files, total_size_bytes,
matched_file_count }` in `src/investigation/scope.rs`, sourced from
`ScanResult`'s structural totals and `PluginPreselection`'s matched-file
counts.

#### 1.2 Add Foundation Functionality

Implement `compute_investigation_turns(scope) -> u32` in
`src/investigation/strategy.rs` as an additive model: a fixed base
allowance, plus a per-matched-file term, plus a repository-breadth term
scaled by total file count. Implement
`decide_investigation_strategy(scope, threshold_files, threshold_bytes,
batch_count) -> InvestigationStrategy { SingleSession, BatchedSessions {
batch_size } }`.

#### 1.3 Integrate Foundation Work

Wire both functions into the `AgentSession`-based plugin investigation path,
replacing any fixed or hardcoded turn cap introduced by the agent
tool-calling integration plan.

#### 1.4 Testing Requirements

Unit tests asserting the computed turn budget scales with repository size
and matched-file count across small, medium, and large synthetic
`InvestigationScope` fixtures.

#### 1.5 Deliverables

Working `compute_investigation_turns` and `decide_investigation_strategy`.

#### 1.6 Success Criteria

A large synthetic scope yields a materially and deterministically larger
turn budget than a small one.

### Phase 2: Batched Sessions with Degrade-and-Warn

#### 2.1 Feature Work

Implement `src/investigation/batch.rs`'s file-match-map batching (splitting
into `batch_count` chunks), plus a `sequential_batches: bool` config flag for
constrained inference backends. Spawn one `AgentSession` per batch,
concurrent by default via `futures::future::join_all`.

#### 2.2 Integrate Feature

On a batch exhausting its turn budget (`SessionError::MaxTurnsExceeded` or
equivalent), degrade gracefully: contribute that batch's partial (possibly
empty) findings rather than failing the whole plugin run, and record a
`Diagnostic` via the existing `diagnostics.rs` type plus a `tracing::warn!`
log call. Both must be visible — the diagnostic surfaces in the plugin's
JSON/Markdown output and, when run via the watcher, in
`WatcherResultMessage.diagnostics` — not silently dropped.

#### 2.3 Configuration Updates

Add `investigation_threshold_files`, `investigation_threshold_bytes`, and
`investigation_batch_count` as optional plugin configuration fields, falling
back to sensible defaults when unset.

#### 2.4 Testing Requirements

A test that forces a batch to exceed its turn budget (a mocked provider that
never terminates the conversation) and asserts: the run completes rather
than erroring; a diagnostic with a recognizable category appears in the
plugin's output; and the same diagnostic is present in
`WatcherResultMessage.diagnostics` when the run is triggered via the
watcher path.

#### 2.5 Deliverables

Batched investigation sessions with visible, non-silent degradation on turn
exhaustion.

#### 2.6 Success Criteria

No plugin run silently drops findings from an exhausted batch without a
corresponding diagnostic appearing in both the report output and the process
logs.
