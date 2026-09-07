# Phase 1.3 / 2.1 / 2.3: Security Review Plugin Investigation Wiring

## Overview

This document describes the changes made to wire the investigation infrastructure
(Phase 1 turn budgeting and Phase 2 batched sessions) into the active execution
path of `SecurityReviewPlugin`.

Prior to this change, the plugin used a hardcoded `config.agent_max_turns` cap
and always ran a single `AgentSession`. The Phase 1 and Phase 2 modules were
fully implemented but never called from `plugin.rs`.

---

## What Changed

### File Modified

`src/plugins/security_review/plugin.rs`

---

## Phase 1.3: Dynamic Turn Budget

The static `config.agent_max_turns as usize` cap in `run()` Step 6 was replaced
with a call to `compute_turn_budget(&metrics)`.

### Before

```rust
let session =
    ctx.build_agent_session(system_prompt, 8192, config.agent_max_turns as usize)?;
```

### After

```rust
let metrics = ScopeMetrics::from_scan_result(&ctx.scan_result);
let turn_budget = compute_turn_budget(&metrics);
// ...
let session = ctx.build_agent_session(system_prompt, 8192, turn_budget as usize)?;
```

`compute_turn_budget` derives a turn limit from the repository's matched-file
count and total byte size, scaling between `BASE_TURNS` (5) and `MAX_TURNS` (30)
based on repository complexity. This ensures large repositories receive more
turns while keeping small repositories efficient.

---

## Phase 2.1: Strategy Selection

`decide_investigation_strategy` is called after computing the turn budget. It
returns either `InvestigationStrategy::SingleSession` or
`InvestigationStrategy::BatchedSession(BatchConfig)` based on three configurable
thresholds read from `SecurityReviewConfig`:

| Config field | Default | Meaning |
|---|---|---|
| `investigation_threshold_files` | 20 | Max matched files before batching |
| `investigation_threshold_bytes` | 10_000_000 | Max total bytes before batching |
| `investigation_batch_count` | 4 | Number of parallel batches |

When neither threshold is exceeded, `SingleSession` is used and the behaviour
is identical to the pre-wiring state (except the turn budget is now dynamic).

---

## Phase 2.3: SecurityReviewBatchSession

`SecurityReviewBatchSession` implements the `BatchSession` trait and bridges the
plugin layer with `BatchedInvestigationRunner`.

Each call to `SecurityReviewBatchSession::run` creates a fresh `AgentSession`
scoped to the files in the supplied `InvestigationBatch`. The session:

1. Rebuilds the active security categories from the plugin config.
2. Builds a user prompt from the batch's file paths.
3. Creates an `AgentContext` with the pre-seeded system prompt.
4. Runs `AgentSession` with the per-batch `turn_budget`.
5. Returns `BatchOutcome` with the raw AI response in `findings`.

Error mapping:

- If the session error message contains `"max turns reached"`, the method
  returns `InvestigationError::TurnBudgetExceeded` so the runner can gracefully
  degrade (emit diagnostic, include partial findings).
- Any other error becomes `InvestigationError::BatchFailed`.

Exhausted-batch diagnostics from the runner are drained into the plugin context
so they appear in the report output and in `WatcherResultMessage.diagnostics`.

---

## Tests Added (Phase 1.3 / 2.3)

Four unit tests and two async integration tests were added:

| Test | Purpose |
|---|---|
| `test_security_review_plugin_scope_metrics_derive_from_scan_result` | Verifies `ScopeMetrics::from_scan_result` is reachable from the plugin layer |
| `test_security_review_plugin_compute_turn_budget_returns_nonzero` | Confirms `compute_turn_budget` returns at least `BASE_TURNS` (5) for an empty repo |
| `test_security_review_plugin_strategy_is_single_session_for_empty_scan` | Empty scan must yield `SingleSession` at default thresholds |
| `test_security_review_plugin_zero_threshold_forces_batched_strategy` | `threshold_files=0` with 1 matched file forces `BatchedSession` |
| `test_security_review_plugin_run_with_single_session_strategy_uses_computed_budget` | End-to-end run with default config completes successfully |
| `test_security_review_plugin_run_with_batched_strategy_empty_scope_completes` | Run with `threshold_files=Some(0)` on an empty scan completes without error |

---

## Helper Update

`make_tool_calling_provider` in the test module was updated from accepting
`&'static str` and returning `MockProvider` to accepting `Vec<String>` and
returning `Arc<dyn Provider + Send + Sync>`. This enables the new tests to
pass providers directly to `make_context` without manual `Arc::new` wrapping,
and supports multi-response mock sequences. All existing test callers were
updated accordingly; test logic is unchanged.

---

## Design Notes

- The `SecurityReviewBatchSession` struct is private to the module. Only
  `BatchedInvestigationRunner` and the `BatchSession` trait interact with it.
- The `ToolRegistry::new()` passed to each `AgentSession` is intentionally
  empty; the security review plugin performs read-only AI analysis without
  calling tools from within batches.
- `compute_turn_budget` is called once per `run()` invocation and the result is
  shared across all batches when `BatchedSession` is chosen.
