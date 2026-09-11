# Phase 2 SAST Range Algebra Implementation

## Overview

`src/scanner/sast/engine/range.rs` provides the byte-range algebra used by the
SAST formula evaluator. Every pattern match in the AST engine is represented as
a `RangeWithMetavars`: a half-open byte interval `[start, end)` paired with
metavariable bindings. Three set-algebra functions (`intersect`, `union`,
`subtract`) combine match sets when the formula evaluator processes `And`, `Or`,
and negation (`pattern-not`) nodes.

## Core Type

### `MetavarBindings`

A `BTreeMap<String, String>` alias. Keys carry the `$` prefix (e.g. `"$X"`);
values are the bound source text. `BTreeMap` is used rather than `HashMap` for
deterministic serialisation order, which aids debugging and snapshot testing.

### `RangeWithMetavars`

```text
{start: usize, end: usize, bindings: MetavarBindings}
```

Fields are `pub` so the formula evaluator and tests can inspect them directly.

`Ord` and `PartialOrd` are implemented manually (not derived) to sort by
`(start, end)` only, ignoring bindings. This allows ranges to be sorted
deterministically for output and for `dedup` to work correctly. `Hash` is NOT
derived because `BTreeMap` does not implement `Hash`; equality comparisons use
`PartialEq` which considers all three fields.

## Operations

### `intersect(a, b) -> Option<RangeWithMetavars>`

Returns the byte intersection of two ranges with unified bindings.

Two conditions must both hold for a `Some` result:

1. **Overlap**: `a.start < b.end && b.start < a.end` (half-open interval test).
   Adjacent ranges `[0,5)` and `[5,10)` share no bytes and return `None`.

2. **Binding compatibility**: no metavariable may be bound to two different
   texts. If both ranges bind `$X` to different values, `None` is returned.
   Identical bindings on both sides are accepted.

The result range spans `[max(a.start, b.start), min(a.end, b.end)]` with merged
bindings (a's bindings augmented by b's).

The formula evaluator uses `intersect` in `eval_and` to compute the
cross-product of two conjunct match sets.

### `union(sets) -> Vec<RangeWithMetavars>`

Flattens multiple match sets, sorts by `(start, end)`, then calls `dedup` which
uses `PartialEq` (full equality: position AND bindings). Two ranges with the
same byte position but different bindings represent distinct matches and are
kept as separate entries. This is important when an `Or` formula's branches bind
different metavariables at the same location.

### `subtract(positive, negative) -> Vec<RangeWithMetavars>`

Removes from `positive` every range that is fully contained within some range in
`negative`. Containment is purely geometric:
`n.start <= p.start && p.end <= n.end`. Bindings are not compared.

A range that only partially overlaps a negative range is kept. A range that
exactly equals a negative range is removed (equal ranges are fully contained in
each other).

The function short-circuits and returns `positive` unchanged when `negative` is
empty. The result is sorted by `(start, end)`.

The formula evaluator uses `subtract` in `eval_and` to apply `pattern-not`
negations.

## Design Decisions

### Full-equality dedup in `union`

`union` deduplicates by the full `(start, end, bindings)` tuple rather than by
position only. This preserves all distinct binding combinations produced by
different patterns in an `Or` formula. The formula layer is responsible for any
further consolidation if caller semantics require position-only dedup.

### `contains` vs `overlaps` in `subtract`

`subtract` uses geometric containment (`n.contains(p)`) rather than overlap.
Pattern-not semantics in Semgrep-style rules suppress a finding only when the
negative pattern matches the entire candidate node, not merely when it touches
it. A positive match that extends beyond a negative match is intentionally kept.

### Sorting and determinism

Results are always sorted by `(start, end)` so downstream consumers (report
formatters, snapshot tests) receive output in a stable order regardless of the
order in which ast-grep returns matches.

## Testing

The module has 29 unit tests covering:

- `RangeWithMetavars` construction, `overlaps`, `contains`, and `Ord`
- `intersect`: overlapping, non-overlapping, adjacent, fully-contained, binding
  compatibility (disjoint, conflicting, identical, empty)
- `union`: empty input, single-set sort, deduplication of identical ranges,
  preservation of distinct-binding ranges, sorted output
- `subtract`: empty negative, contained removal, non-contained preservation,
  equal-range removal, partial-overlap preservation, multiple negatives, sorted
  output
- A mandatory property test
  (`test_subtract_length_never_exceeds_positive_length_property`) that verifies
  `subtract` never increases the result length across more than 4 million
  generated cases (all combinations of start/end in `0..100` with step sizes 2
  and 3).

Doc-test examples are included on every public item.
