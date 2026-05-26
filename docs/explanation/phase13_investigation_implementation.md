# Phase 13: Investigation Module Implementation

## Summary

Phase 13 adds `src/investigation/`, a self-contained module that governs the
investigation phase of plugin execution. The module defines how a set of
candidate files is represented, grouped into bounded batches, and dispatched to
AI-driven plugin sessions.

---

## Design Goals

- Deterministic output: all ordering is path-based so repeated invocations
  produce identical batch assignments.
- Bounded resource use: `BatchConfig` caps both per-batch file counts and the
  total number of batches, preventing unbounded AI session chains.
- Zero-cost composition: builder methods on `FileMatchEntry` keep construction
  concise without hidden allocations.
- Full serializability: all public types derive `serde::Serialize` and
  `serde::Deserialize`, allowing scope and strategy snapshots to be persisted to
  YAML or JSON alongside workspace state.

---

## Module Layout

```text
src/investigation/
    mod.rs       - module doc, submodule declarations, crate-level re-exports
    scope.rs     - FileMatchEntry, InvestigationScope
    batch.rs     - BatchConfig, InvestigationBatch, split_into_batches,
                   compute_investigation_turns
    strategy.rs  - InvestigationStrategy
```

---

## Public API

### `scope.rs`

#### `FileMatchEntry`

Represents a single candidate file.

| Field        | Type             | Purpose                                    |
| ------------ | ---------------- | ------------------------------------------ |
| `path`       | `String`         | Repository-relative path (forward slashes) |
| `language`   | `Option<String>` | Detected language, if known                |
| `size_bytes` | `u64`            | File size                                  |
| `categories` | `Vec<String>`    | Scanner preselection tags                  |

Builder methods (`with_language`, `with_size`, `with_categories`) follow the
consuming-builder pattern and can be chained.

#### `InvestigationScope`

A `HashMap<String, FileMatchEntry>` wrapper that provides:

- `insert` / `remove` / `get` for individual entries
- `paths()` / `entries()` returning lexicographically sorted views
- `filter_by_category(category)` and `filter_by_language(language)` producing
  new scopes without mutating the original
- `total_bytes()` for size-budget checks
- `Default` implementation (empty scope)

### `batch.rs`

#### `BatchConfig`

Controls batch generation.

| Field                      | Default | Purpose                                |
| -------------------------- | ------- | -------------------------------------- |
| `max_batches`              | 10      | Hard cap on number of batches produced |
| `batch_size`               | 20      | Maximum files per batch                |
| `clean_verification_turns` | 1       | Verification passes after each batch   |

#### `InvestigationBatch`

A named slice of the full scope with a 0-based `index` and a `total_batches`
count so callers can emit progress messages without extra state.

#### `split_into_batches(scope, config) -> Vec<InvestigationBatch>`

1. Calls `scope.entries()` to obtain a path-sorted `Vec<&FileMatchEntry>`.
2. Chunks the slice at `config.batch_size` using the standard library
   `slice::chunks` iterator.
3. Takes at most `config.max_batches` chunks.
4. Wraps each chunk into an `InvestigationBatch` with the correct index and
   shared `total_batches` count.
5. Returns an empty `Vec` for an empty scope or a zero `batch_size`.

#### `compute_investigation_turns(scope, config) -> usize`

Computes the batch count using `usize::div_ceil` for exact ceiling division,
then clamps to `config.max_batches`. Returns `0` for empty or zero-size-batch
edge cases.

### `strategy.rs`

#### `InvestigationStrategy`

```rust
enum InvestigationStrategy {
    SingleSession,
    BatchedSession(BatchConfig),
}
```

| Method                     | Behaviour                                                                                |
| -------------------------- | ---------------------------------------------------------------------------------------- |
| `default_for_scope(scope)` | `SingleSession` for <= 20 files, `BatchedSession(default)` otherwise                     |
| `batch_config()`           | `None` / `Some(&BatchConfig)`                                                            |
| `is_batched()`             | `false` / `true`                                                                         |
| `estimated_turns(scope)`   | `1` for `SingleSession`; delegates to `compute_investigation_turns` for `BatchedSession` |

---

## Key Design Decisions

### Why `HashMap` inside `InvestigationScope`?

Path-keyed deduplication is the primary use-case: scanner preselection may emit
the same path through multiple hooks. The `HashMap` gives O(1) insert with
automatic last-write-wins semantics. Sorted access is always available via
`paths()` and `entries()`, which pay the sorting cost only on demand.

### Why sort inside `entries()` rather than keeping a `BTreeMap`?

`HashMap` provides O(1) random access (used by `get`, `insert`, `remove`) which
is more common than full iteration. Sorting on each `entries()` call is
acceptable because batch splitting happens once per plugin invocation, not in
hot loops.

### Why `div_ceil` instead of manual ceiling arithmetic?

`usize::div_ceil` was stabilized in Rust 1.73.0 and is the idiomatic form.
Clippy lint `manual_div_ceil` flags the manual equivalent, so using the built-in
avoids a warning.

### Why `SingleSession` for <= 20 files?

Twenty files is an empirically conservative estimate of what fits comfortably in
a typical AI context window alongside a full system prompt and structured output
instructions. Callers can override this by constructing
`BatchedSession(BatchConfig::new(...))` directly.

---

## Test Coverage

| File          | Tests  | Key scenarios                                                                                                                                    |
| ------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `scope.rs`    | 30     | builder chain, insert/replace, remove, get, sorted views, category filter, language filter (case-insensitive), total_bytes                       |
| `batch.rs`    | 19     | edge cases (empty scope, zero batch_size), single/multi-batch splits, max_batches cap, sort determinism, idempotency, turns matching batch count |
| `strategy.rs` | 14 + 6 | boundary at 20 files, default config verification, `batch_config` accessor, `is_batched`, `estimated_turns` for single and batched with cap      |

All 69 unit tests pass under `cargo test --all-features`.

---

## Files Created

| Path                            | Purpose                                                                                  |
| ------------------------------- | ---------------------------------------------------------------------------------------- |
| `src/investigation/mod.rs`      | Module declarations and crate-level re-exports                                           |
| `src/investigation/scope.rs`    | `FileMatchEntry`, `InvestigationScope`                                                   |
| `src/investigation/batch.rs`    | `BatchConfig`, `InvestigationBatch`, `split_into_batches`, `compute_investigation_turns` |
| `src/investigation/strategy.rs` | `InvestigationStrategy`                                                                  |
| `src/lib.rs`                    | Added `pub mod investigation;`                                                           |

---

## Quality Gate Results

```text
cargo fmt --all                                    -- ok
cargo check --all-targets --all-features           -- ok
cargo clippy --all-targets --all-features -- -D warnings  -- ok
cargo test --all-features                          -- 832 tests: ok
```
