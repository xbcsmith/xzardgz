# Phase 2: Batched Sessions with Degrade-and-Warn Implementation

## Overview

Phase 2 builds the orchestration layer for multi-batch AI investigation. Given a
set of candidate files split across `N` batches, a `BatchedInvestigationRunner`
dispatches each batch to a `BatchSession` implementation — concurrently by
default, sequentially on request — and degrades gracefully when any batch
exhausts its turn budget: partial findings are preserved, a
`DiagnosticCategory::Plugin` warning is recorded, and `tracing::warn!` is
emitted. The run always completes rather than failing.

## Motivation

The Phase 1 building blocks (`compute_turn_budget`,
`decide_investigation_strategy`, `ScopeMetrics`) establish what the budget and
strategy should be. Phase 2 implements the actual execution and recovery
contract so that:

- No findings from an exhausted batch are silently dropped.
- Every budget exhaustion produces a `Diagnostic` visible in both the plugin
  output and `WatcherResultMessage.diagnostics`.
- Constrained inference backends can opt into sequential dispatch with a single
  config flag.

## New Types

### `InvestigationError` (`src/investigation/batch.rs`)

A `thiserror`-based error enum with two variants:

| Variant              | When                                      | Carries                                                      |
| -------------------- | ----------------------------------------- | ------------------------------------------------------------ |
| `TurnBudgetExceeded` | Session consumed all allocated turns      | `batch_index`, `turn_limit`, `partial_findings: Vec<String>` |
| `BatchFailed`        | Any other non-recoverable session failure | `batch_index`, `message: String`                             |

`partial_findings` on `TurnBudgetExceeded` ensures findings collected before
exhaustion are not lost.

### `BatchOutcome` (`src/investigation/batch.rs`)

Per-batch result returned by a `BatchSession`:

| Field                   | Type          | Meaning                                |
| ----------------------- | ------------- | -------------------------------------- |
| `batch_index`           | `usize`       | 0-based index of this batch            |
| `total_batches`         | `usize`       | Total batch count in the run           |
| `findings`              | `Vec<String>` | Findings collected (may be partial)    |
| `diagnostics`           | `Diagnostics` | Any diagnostics produced by this batch |
| `turn_budget_exhausted` | `bool`        | `true` when budget was consumed        |

### `InvestigationOutcome` (`src/investigation/batch.rs`)

Aggregate result across all batches returned by
`BatchedInvestigationRunner::run`:

| Field               | Type          | Meaning                                  |
| ------------------- | ------------- | ---------------------------------------- |
| `findings`          | `Vec<String>` | All findings from all batches            |
| `diagnostics`       | `Diagnostics` | All diagnostics from all batches         |
| `batches_completed` | `usize`       | Batches that finished without exhaustion |
| `batches_exhausted` | `usize`       | Batches that exhausted their budget      |
| `total_batches`     | `usize`       | Total batch count                        |

Key methods:

- `merge_batch_outcome(batch)` — accumulates a `BatchOutcome`
- `is_partial() -> bool` — true when any batch exhausted its budget
- `is_empty() -> bool` — true when no findings were collected
- `into_watcher_diagnostics() -> Vec<Diagnostic>` — drains diagnostics for
  `WatcherResultMessage.diagnostics`

### `BatchSession` trait (`src/investigation/batch.rs`)

```rust
#[async_trait]
pub trait BatchSession: Send + Sync {
    async fn run(
        &self,
        batch: &InvestigationBatch,
        turn_budget: u32,
    ) -> Result<BatchOutcome, InvestigationError>;
}
```

Implementors wrap the actual AI provider call. The contract:

- Return `Ok(BatchOutcome)` on success.
- Return `Err(TurnBudgetExceeded { partial_findings })` when all turns are
  consumed, carrying any findings collected before exhaustion.
- Return `Err(BatchFailed { message })` for any other unrecoverable failure.

### `BatchedInvestigationRunner<S>` (`src/investigation/batch.rs`)

Generic over any `S: BatchSession + 'static`. Constructed with a session and a
`BatchConfig`. Its single public method is:

```rust
pub async fn run(
    &self,
    scope: &InvestigationScope,
    turn_budget: u32,
) -> InvestigationOutcome
```

The method never returns `Err` — all failures are converted to diagnostics.

## `BatchConfig` changes

A `sequential_batches: bool` field was added (default: `false`).

| `sequential_batches` | Behavior                                                              |
| -------------------- | --------------------------------------------------------------------- |
| `false` (default)    | All batch futures driven concurrently via `futures::future::join_all` |
| `true`               | Batches run one-at-a-time for constrained inference backends          |

The existing 3-parameter
`BatchConfig::new(max_batches, batch_size, clean_verification_turns)` signature
is unchanged. A `with_sequential(bool) -> Self` builder method was added.

## Degrade-and-Warn Behavior

The private `resolve_batch_outcome` function implements the recovery contract.
For each batch result from the session:

```text
session.run(batch, turn_budget)
    |
    +-- Ok(BatchOutcome) ──────────────────────────────────────────► pass through
    |
    +-- Err(TurnBudgetExceeded { partial_findings, .. }) ──────────► tracing::warn!
    |                                                                 DiagnosticCategory::Plugin warning
    |                                                                 BatchOutcome { turn_budget_exhausted: true,
    |                                                                                findings: partial_findings }
    |
    +-- Err(BatchFailed { message, .. }) ──────────────────────────► tracing::warn!
                                                                      DiagnosticCategory::Plugin warning
                                                                      BatchOutcome { turn_budget_exhausted: false,
                                                                                     findings: vec![] }
```

In both failure cases the run continues with remaining batches. The aggregate
`InvestigationOutcome.is_partial()` returns `true` only when at least one batch
was `TurnBudgetExceeded`.

## Watcher Integration Path

`InvestigationOutcome::into_watcher_diagnostics()` is the integration point
between the investigation layer and the watcher message layer:

```rust
let outcome = runner.run(&scope, turn_budget).await;

// Drain diagnostics into WatcherResultMessage.
let watcher_diags = outcome.into_watcher_diagnostics();
result_message.diagnostics.extend(watcher_diags);
```

After this, `result_message.diagnostics` contains a
`DiagnosticCategory::Plugin`-level `Warning` for every exhausted batch,
satisfying the Phase 2.6 success criterion that no findings are silently dropped
without a visible diagnostic.

## Configuration Fields Added

Both `SecurityReviewConfig` and `TechnicalReviewConfig` in `src/config.rs`
received three optional fields:

| Field                           | Type          | Default                   | Meaning                                              |
| ------------------------------- | ------------- | ------------------------- | ---------------------------------------------------- |
| `investigation_threshold_files` | `Option<u64>` | `None` (effective: 20)    | Matched-file count above which batching is triggered |
| `investigation_threshold_bytes` | `Option<u64>` | `None` (effective: 10 MB) | Repo byte count above which batching is triggered    |
| `investigation_batch_count`     | `Option<u32>` | `None` (effective: 4)     | Desired number of concurrent batches                 |

All fields are `#[serde(default)]` and deserialize to `None` when absent from
the YAML config, so no existing configuration files need updating.

Call sites use these with:

```rust
let threshold_files = config.investigation_threshold_files.unwrap_or(20);
let threshold_bytes = config.investigation_threshold_bytes.unwrap_or(10_000_000);
let batch_count = config.investigation_batch_count.unwrap_or(4) as usize;

let strategy = decide_investigation_strategy(
    &metrics,
    threshold_files,
    threshold_bytes,
    batch_count,
);
```

## Test Coverage

### `batch.rs` — Phase 2 tests (appended to existing `mod tests`)

| Group                             | Count | What is tested                                                                               |
| --------------------------------- | ----- | -------------------------------------------------------------------------------------------- |
| `BatchConfig::sequential_batches` | 4     | Default false, builder sets/clears                                                           |
| `InvestigationError`              | 3     | Display text, partial findings preserved                                                     |
| `BatchOutcome`                    | 2     | Field construction                                                                           |
| `InvestigationOutcome`            | 5     | Merge, counters, `is_partial`, `is_empty`, `into_watcher_diagnostics`                        |
| Runner concurrent                 | 8     | Success, exhaustion (completes, diagnostic, message, partial findings), failure, empty scope |
| Runner sequential                 | 2     | Completion, same diagnostics as concurrent                                                   |
| Watcher integration               | 1     | Diagnostics drain into `WatcherResultMessage.diagnostics`                                    |

Total new async tests: 11 (`#[tokio::test]`). Total new sync tests: 14.

### `config.rs` — Phase 2 tests

| Count | What is tested                                                            |
| ----- | ------------------------------------------------------------------------- |
| 4     | `TechnicalReviewConfig` threshold fields default to `None` and can be set |
| 4     | `SecurityReviewConfig` threshold fields default to `None` and can be set  |

## Files Changed

| File                         | Change type                                                                                                                                                                       |
| ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/investigation/batch.rs` | Updated — `sequential_batches`, `InvestigationError`, `BatchOutcome`, `InvestigationOutcome`, `BatchSession`, `BatchedInvestigationRunner`, `resolve_batch_outcome`, 25 new tests |
| `src/investigation/mod.rs`   | Updated — re-exports for all new public types                                                                                                                                     |
| `src/config.rs`              | Updated — 3 investigation fields on each plugin config, 8 new tests                                                                                                               |

## Success Criteria Check

| Criterion                                                     | Met by                                                                                                             |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Run completes rather than erroring on budget exhaustion       | `resolve_batch_outcome` converts `TurnBudgetExceeded` to `BatchOutcome`, never propagates error                    |
| Diagnostic with `Plugin` category appears in plugin output    | `DiagnosticCategory::Plugin` warning in `BatchOutcome.diagnostics`, merged into `InvestigationOutcome.diagnostics` |
| Same diagnostic present in `WatcherResultMessage.diagnostics` | `into_watcher_diagnostics()` + `result_message.diagnostics.extend(...)` verified in watcher integration test       |
| No silent drops                                               | `partial_findings` field on `TurnBudgetExceeded` ensures pre-exhaustion findings are included                      |
