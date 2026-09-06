# Phase 1: Turn Budgeting and Strategy Selection Implementation

## Overview

Phase 1 fleshes out the previously-placeholder investigation module with
concrete turn-budget computation and strategy-selection logic. The result is a
deterministic, testable system that scales AI turn allowances with repository
size and matched-file count, and selects between single-session and batched
investigation automatically from configurable thresholds.

## Motivation

Any future `AgentSession`-based plugin investigation needs a non-ad-hoc answer
to "how many turns should this session be allowed?" Without a principled budget,
sessions either run indefinitely or use a fixed cap that is too low for large
repositories and wasteful for small ones.

Phase 1 establishes the budget and strategy contracts so that Phase 2 (batched
sessions with degrade-and-warn) can plug in directly.

## New Type: `ScopeMetrics`

`ScopeMetrics` (`src/investigation/scope.rs`) captures three repository-level
scalars:

| Field                | Type  | Meaning                                    |
| -------------------- | ----- | ------------------------------------------ |
| `total_files`        | `u64` | Total file count in the repository         |
| `total_size_bytes`   | `u64` | Sum of all file sizes in the repository    |
| `matched_file_count` | `u64` | Unique files matching any concern category |

`matched_file_count` is the count of unique paths appearing across
`PluginPreselection`'s concern lists: `risky_pattern_files`,
`secrets_like_files`, `unsafe_rust_files`, `command_execution_files`,
`network_client_files`, and `auth_files`. Files in multiple lists are counted
once. Non-concern lists (`entrypoints`, `public_apis`, `config_surfaces`,
`dependency_manifests`, `test_files`, `missing_test_signals`) are excluded.

### Constructors

| Constructor                                                            | When to use                                                                        |
| ---------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `ScopeMetrics::new(total_files, total_size_bytes, matched_file_count)` | Direct construction, tests                                                         |
| `ScopeMetrics::from_scan_result(scan_result)`                          | Production path after scanning                                                     |
| `ScopeMetrics::from_investigation_scope(scope)`                        | When only a scope is available; `total_files == matched_file_count == scope.len()` |

## New Function: `compute_turn_budget`

`compute_turn_budget(metrics: &ScopeMetrics) -> u32`
(`src/investigation/strategy.rs`)

Implements an additive model:

```text
budget = BASE_TURNS
       + (matched_file_count * PER_FILE_TURNS)
       + (total_files / BREADTH_DIVISOR)
       capped at MAX_TURNS
```

### Constants

| Constant             | Value | Meaning                                     |
| -------------------- | ----- | ------------------------------------------- |
| `BASE_TURNS`         | `5`   | Minimum turns for any investigation         |
| `PER_FILE_TURNS`     | `2`   | Extra turns per concern-matched file        |
| `BREADTH_DIVISOR`    | `100` | 1 extra turn per 100 total repository files |
| `MAX_TURNS`          | `200` | Hard upper cap                              |
| `DEFAULT_BATCH_SIZE` | `10`  | Fallback batch size when `batch_count` is 0 |

### Scaling Examples

| Scope               | `matched` | `total` | Budget |
| ------------------- | --------- | ------- | ------ |
| Small               | 5         | 50      | 15     |
| Medium              | 25        | 500     | 60     |
| Large               | 80        | 2 000   | 185    |
| Very large (capped) | 100       | 10 000  | 200    |

A large scope always produces a materially larger budget than a small one,
satisfying the Phase 1.6 success criterion.

All arithmetic uses saturating operations and
`u32::try_from(…).unwrap_or(u32::MAX)` to prevent overflow. The result is
bounded to `[BASE_TURNS, MAX_TURNS]`.

## New Function: `decide_investigation_strategy`

`decide_investigation_strategy(metrics, threshold_files, threshold_bytes, batch_count) -> InvestigationStrategy`
(`src/investigation/strategy.rs`)

Compares `metrics.matched_file_count` against `threshold_files` and
`metrics.total_size_bytes` against `threshold_bytes`. If either threshold is
exceeded, `BatchedSession` is returned; otherwise `SingleSession`.

### Batch Size Derivation

When batching is selected:

```text
batch_size = ceil(matched_file_count / batch_count)
max_batches = batch_count
clean_verification_turns = 1
```

When `batch_count` is 0, `max_batches = 1` and `batch_size = DEFAULT_BATCH_SIZE`
are used as safe defaults.

### Decision Logic

```text
decide_investigation_strategy(metrics, threshold_files, threshold_bytes, batch_count)
   |
   +-- matched_file_count > threshold_files  ────────────────────────► BatchedSession
   |    OR total_size_bytes > threshold_bytes
   |
   +-- neither threshold exceeded  ─────────────────────────────────► SingleSession
```

## Relationship to Existing Functions

| Function                                   | Module     | What it computes                                             |
| ------------------------------------------ | ---------- | ------------------------------------------------------------ |
| `compute_turn_budget`                      | `strategy` | AI turn allowance (budget)                                   |
| `compute_investigation_turns`              | `batch`    | Number of batches (scheduling)                               |
| `decide_investigation_strategy`            | `strategy` | Strategy selection with thresholds                           |
| `InvestigationStrategy::default_for_scope` | `strategy` | Strategy selection without thresholds (threshold = 20 files) |

These functions serve complementary roles. `compute_turn_budget` answers "how
many turns to allow", while `compute_investigation_turns` answers "how many
batches are needed". `decide_investigation_strategy` is a configurable
alternative to `default_for_scope` for call sites where thresholds come from
plugin configuration.

## Public API Surface (new exports from `investigation::`)

```rust
pub use scope::ScopeMetrics;
pub use strategy::{compute_turn_budget, decide_investigation_strategy};
```

These join the existing re-exports in `src/investigation/mod.rs`.

## Testing

### `ScopeMetrics` tests (`src/investigation/scope.rs` — `mod scope_metrics_tests`)

| Test                                                                                | Verifies                     |
| ----------------------------------------------------------------------------------- | ---------------------------- |
| `test_scope_metrics_new_sets_all_fields`                                            | Direct constructor           |
| `test_scope_metrics_new_with_zero_values_produces_zero_metrics`                     | Zero edge case               |
| `test_scope_metrics_from_investigation_scope_empty_scope_returns_all_zeros`         | Empty scope                  |
| `test_scope_metrics_from_investigation_scope_with_entries_counts_correctly`         | Totals from scope            |
| `test_scope_metrics_from_investigation_scope_total_files_equals_matched_file_count` | Equality invariant           |
| `test_scope_metrics_from_scan_result_empty_scan_result_returns_all_zeros`           | Empty scan result            |
| `test_scope_metrics_from_scan_result_counts_repository_structure`                   | `total_files` from structure |
| `test_scope_metrics_from_scan_result_sums_file_sizes`                               | `total_size_bytes` is sum    |
| `test_scope_metrics_from_scan_result_counts_unique_matched_files`                   | Concern list union           |
| `test_scope_metrics_from_scan_result_deduplicates_files_in_multiple_categories`     | Deduplication                |
| `test_scope_metrics_from_scan_result_non_concern_preselection_fields_not_counted`   | Non-concern exclusion        |

### `compute_turn_budget` tests (`src/investigation/strategy.rs`)

| Test                                                               | Verifies                |
| ------------------------------------------------------------------ | ----------------------- |
| `test_compute_turn_budget_empty_scope_returns_base_turns`          | Floor = BASE_TURNS      |
| `test_compute_turn_budget_small_scope_scales_above_base`           | Small case math         |
| `test_compute_turn_budget_medium_scope_returns_expected_budget`    | Medium case math        |
| `test_compute_turn_budget_large_scope_returns_expected_budget`     | Large case math         |
| `test_compute_turn_budget_very_large_scope_is_capped_at_max_turns` | MAX_TURNS cap           |
| `test_compute_turn_budget_large_scope_exceeds_small_scope`         | Monotonic scaling       |
| `test_compute_turn_budget_medium_scope_exceeds_small_scope`        | Monotonic scaling       |
| `test_compute_turn_budget_is_deterministic_for_same_metrics`       | Determinism             |
| `test_compute_turn_budget_only_matched_files_no_breadth`           | Per-file term isolation |
| `test_compute_turn_budget_only_breadth_no_matched_files`           | Breadth term isolation  |

### `decide_investigation_strategy` tests (`src/investigation/strategy.rs`)

| Test                                                                                  | Verifies                |
| ------------------------------------------------------------------------------------- | ----------------------- |
| `test_decide_investigation_strategy_small_scope_returns_single_session`               | Below both thresholds   |
| `test_decide_investigation_strategy_exactly_at_file_threshold_returns_single_session` | Boundary (not exceeded) |
| `test_decide_investigation_strategy_one_over_file_threshold_returns_batched`          | File threshold          |
| `test_decide_investigation_strategy_over_byte_threshold_returns_batched`              | Byte threshold          |
| `test_decide_investigation_strategy_batched_batch_size_is_ceil_divide`                | Exact division          |
| `test_decide_investigation_strategy_batched_batch_size_rounds_up`                     | Ceiling division        |
| `test_decide_investigation_strategy_zero_batch_count_uses_defaults`                   | Zero fallback           |
| `test_decide_investigation_strategy_large_scope_large_batch_count`                    | Large scale             |
| `test_decide_investigation_strategy_is_deterministic`                                 | Determinism             |

## Files Changed

| File                            | Change type                                                                                     |
| ------------------------------- | ----------------------------------------------------------------------------------------------- |
| `src/investigation/scope.rs`    | Updated — new `ScopeMetrics` struct + 11 tests                                                  |
| `src/investigation/strategy.rs` | Updated — `compute_turn_budget`, `decide_investigation_strategy`, constants, 19 tests           |
| `src/investigation/mod.rs`      | Updated — re-exports for `ScopeMetrics`, `compute_turn_budget`, `decide_investigation_strategy` |

## Integration Path (Phase 1.3)

The new functions are ready for wiring into the `AgentSession`-based plugin
investigation path. Any hardcoded turn cap introduced by the agent tool-calling
integration plan should be replaced with:

```rust
let metrics = ScopeMetrics::from_scan_result(&scan_result);
let turn_budget = compute_turn_budget(&metrics);
let strategy = decide_investigation_strategy(
    &metrics,
    config.investigation_threshold_files.unwrap_or(20),
    config.investigation_threshold_bytes.unwrap_or(10_000_000),
    config.investigation_batch_count.unwrap_or(4),
);
```

The `investigation_threshold_files`, `investigation_threshold_bytes`, and
`investigation_batch_count` configuration fields are planned for Phase 2.
