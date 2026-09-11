# Phase 2: Core Matching Engine

## Overview

Phase 2 adds the three engine modules that translate a compiled `Formula` tree
into a set of `RangeWithMetavars` over a parsed source file. Together with the
Phase 1 foundations, these modules form a complete, testable structural matching
pipeline for Rust source code.

## Deliverables

| Artifact          | Path                                 | Purpose                                                  |
| ----------------- | ------------------------------------ | -------------------------------------------------------- |
| Pattern compiler  | `src/scanner/sast/engine/pattern.rs` | Pattern compilation with cache and ellipsis rewrite      |
| Range algebra     | `src/scanner/sast/engine/range.rs`   | `RangeWithMetavars`, intersect, union, subtract          |
| Formula evaluator | `src/scanner/sast/engine/formula.rs` | Recursive formula evaluation with timeout and truncation |
| Test fixtures     | `testdata/sast/fixtures/`            | Positive and negative Rust source fixtures               |
| Integration tests | `src/scanner/sast/mod.rs`            | End-to-end fixture suite                                 |

## Architecture

```text
RuleIr.formula
    |
    v
eval_formula (formula.rs)
    |
    +-- Leaf(Pattern)  --> PatternCompiler.compile (pattern.rs)
    |                           |
    |                           +--> Pattern::try_new(rewrite_ellipsis(text), lang)
    |                           +--> root.root().find_all(&*compiled_pattern)
    |                           +--> collect RangeWithMetavars from NodeMatch
    |
    +-- And { conjuncts, negations }
    |       |
    |       +--> intersect(range_a, range_b)  (range.rs)
    |       +--> subtract(positives, negatives)  (range.rs)
    |
    +-- Or(children)  --> union(sets)  (range.rs)
    |
    +-- Inside(inner) --> containment filter
```

## Pattern Compilation (`engine/pattern.rs`)

### Ellipsis rewrite

Semgrep patterns use `...` (three dots) as a wildcard for zero-or-more elements.
ast-grep uses `$$$` for the same construct. The `rewrite_ellipsis` function
performs a simple string replacement before pattern compilation:

```text
foo(...)  ->  foo($$$)
fn $F() { ...; $X; ... }  ->  fn $F() { $$$; $X; $$$ }
```

The two-character Rust range operator `..` is unaffected because only the exact
three-character sequence `...` is replaced.

### Pattern cache

`PatternCompiler` holds a
`Mutex<HashMap<(text, lang, strictness), Arc<Pattern>>>`. The cache key uses the
rewritten pattern text, the debug string of the language (e.g. `"Rust"`), and a
`u8` index for strictness. The default strictness is `MatchStrictness::Relaxed`,
which skips comments and anonymous punctuation nodes during matching.

Every call to `PatternCompiler::compile` with the same inputs returns the same
`Arc<Pattern>`, verified by `Arc::ptr_eq` in tests.

## Range Algebra (`engine/range.rs`)

### `RangeWithMetavars`

Represents a matched region `[start, end)` (byte offsets) plus a `BTreeMap` of
metavariable bindings (`"$X" -> "matched_text"`). The type implements `Ord` by
`(start, end)` to allow deterministic sorting.

### `intersect`

Two ranges intersect if they share at least one byte position
(`a.start < b.end && b.start < a.end`). The merged range is
`[max(start), min(end)]`. Bindings are unified: if the same metavariable appears
in both ranges with different texts, the pair is incompatible and `intersect`
returns `None`.

This is the core of `And` evaluation: the formula
`patterns: [pattern: $A, pattern: $B]` means both `$A` and `$B` must match
overlapping code, and the metavariable bindings must be consistent.

### `union`

Flattens multiple range sets, sorts by `(start, end)`, and deduplicates
identical ranges. Ranges with the same `(start, end)` but different bindings are
kept as distinct matches.

### `subtract`

Removes positive ranges that are geometrically fully contained within any
negative range (`n.start <= p.start && p.end <= n.end`). This implements
`pattern-not` semantics: a finding is suppressed if the matching node is fully
covered by a `pattern-not` match.

### Property test

A deterministic loop generates over four million `(positive, negative)` cases
and asserts `subtract(pos, neg).len() <= pos.len()` for every case. This
verifies the soundness invariant: subtraction never adds matches.

## Formula Evaluator (`engine/formula.rs`)

### Timeout enforcement

At the start of each `eval_formula` call, a `std::time::Instant` deadline is
computed from `SastEngineConfig::rule_timeout_ms` (default 5000 ms). Before
every recursive call and before every match iteration in `eval_leaf_pattern`,
the current time is compared to the deadline. If exceeded,
`TruncationReason::Timeout` is returned immediately with the matches collected
so far.

### Match truncation

A `matches_found` counter in `EvalContext` tracks the total number of ranges
produced. When it reaches `SastEngineConfig::max_matches_per_file` (default
100), `TruncationReason::MaxMatchesReached` is returned. This bounds memory
usage for pathological rules on large files.

### `And` evaluation

The `And` variant carries two types of conjuncts:

1. Regular conjuncts (`Formula::Leaf`, `Formula::Or`, etc.) are intersected
   pairwise using `range::intersect`. Starting from the first conjunct's ranges,
   each subsequent conjunct is cross-product intersected with the running
   result.

2. `Inside` conjuncts (`Formula::Inside(inner)`) are evaluated separately. The
   inner formula produces "container" ranges. After computing the regular
   intersection, candidate ranges are filtered to keep only those fully
   contained within at least one container range. This implements
   `pattern-inside` semantics.

`negations` are evaluated and the result is passed to `range::subtract`.

`conditions` and `focus` are preserved in `RangeWithMetavars::bindings` but not
evaluated in Phase 2. Phase 3 adds the metavariable condition evaluators.

### `scan_rule` entry point

`scan_rule(rule, cached_root, compiler, config)` is the primary API for
evaluating a `RuleIr` against a parsed file. It delegates to `eval_formula` with
the rule's formula and id.

## Testdata Fixtures

| File                        | Purpose                                        |
| --------------------------- | ---------------------------------------------- |
| `md5_positive.rs`           | Contains `md5::compute(data)` and `Md5::new()` |
| `md5_negative.rs`           | Uses `Sha256::new()`, no MD5 pattern           |
| `sha1_positive.rs`          | Contains `sha1::Sha1::new()` and `Sha1::new()` |
| `sha1_negative.rs`          | Uses `Sha256::new()`, no SHA-1 pattern         |
| `des_positive.rs`           | Contains `Des::new(key.into())`                |
| `des_negative.rs`           | Uses `Aes256Gcm::new()`, no DES pattern        |
| `ellipsis_arg_positive.rs`  | `foo(1, 2, 3)` matches `foo($$$)`              |
| `ellipsis_arg_negative.rs`  | `bar(42)` does NOT match `foo($$$)`            |
| `ellipsis_stmt_positive.rs` | Function body with multiple let-bindings       |

Fixtures are raw Rust source files parsed only at the syntax level. They
reference crates (e.g. `md5`, `des`) that are not in `Cargo.toml`; tree-sitter
does not perform name resolution, so the fixtures parse cleanly regardless.

## Integration Test Results

The end-to-end fixture suite verifies:

| Test                                              | Result |
| ------------------------------------------------- | ------ |
| MD5 rule matches `md5_positive.rs`                | pass   |
| MD5 rule does not match `md5_negative.rs`         | pass   |
| SHA-1 rule matches `sha1_positive.rs`             | pass   |
| SHA-1 rule does not match `sha1_negative.rs`      | pass   |
| DES rule matches `des_positive.rs`                | pass   |
| DES rule does not match `des_negative.rs`         | pass   |
| `foo($$$)` matches `foo(1, 2, 3)`                 | pass   |
| `foo($$$)` does not match `bar(42)`               | pass   |
| `fn $F() { $$$BODY }` matches function definition | pass   |
| `let $VAR = $VAL` matches let-binding             | pass   |

## Quality Gates

All four mandatory gates pass:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Total: 2617 tests, 0 failures.

## What Remains for Phase 3

Phase 3 adds metavariable condition evaluation (`metavariable-regex`,
`metavariable-comparison`, `metavariable-pattern`) and focus
(`focus-metavariable`). The `conditions` and `focus` fields of `Formula::And`
are already preserved through evaluation and stored in match bindings; Phase 3
adds the filter logic that consumes them.
