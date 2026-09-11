# Phase 2 SAST Formula Engine Implementation

## Overview

This document describes the Phase 2 implementation of the AST-mode SAST formula
evaluator. The primary deliverable is `src/scanner/sast/engine/formula.rs`,
which provides `eval_formula` and `scan_rule` as the main entry points for
evaluating compiled `Formula` trees against parsed source files.

## Files Created or Updated

| File                                 | Status                 | Description                                        |
| ------------------------------------ | ---------------------- | -------------------------------------------------- |
| `src/scanner/sast/engine/formula.rs` | Created                | Formula evaluator, `TruncationReason`, `scan_rule` |
| `src/scanner/sast/engine/pattern.rs` | Present (not modified) | Pattern compiler with caching                      |
| `src/scanner/sast/engine/range.rs`   | Present (not modified) | Range algebra for set operations                   |
| `src/scanner/sast/engine/mod.rs`     | Updated                | Exports `formula`, `pattern`, `range` modules      |

## Architecture

### Evaluation Entry Points

`eval_formula` is the primary public function. It accepts a `Formula` tree, a
parsed `CachedRoot`, a shared `PatternCompiler`, and a `SastEngineConfig`, then
returns a tuple of matched ranges and an optional `TruncationReason`.

`scan_rule` is a convenience wrapper that delegates to `eval_formula` using the
rule's `id` and `formula` fields.

### Internal Context

An `EvalContext` struct is threaded through all recursive calls. It holds:

- A reference to the `PatternCompiler` for reuse across pattern compilations
- A reference to the engine config (timeout and max-matches limits)
- The rule id for error attribution (reserved for Phase 3)
- A precomputed `Instant` deadline for timeout checking
- A start `Instant` for elapsed-ms reporting
- A `matches_found` counter for the per-file match cap

### Formula Evaluation

Each `Formula` variant is handled by a dedicated private function:

#### `Leaf(Pattern(s))`

Compiles the pattern via `PatternCompiler::compile` (which rewrites Semgrep
`...` ellipsis to ast-grep `$$$` multi-metavar syntax). Iterates all tree nodes
using `Node::find_all`, extracts byte ranges and `$NAME` metavar bindings from
each `NodeMatch`. Checks the deadline and match cap before every match
iteration.

#### `Leaf(Regex(_))`

Returns an empty set immediately. Regex-mode patterns are handled exclusively by
`RegexModeScanner`, not by the AST engine.

#### `And { conjuncts, negations, .. }`

1. Splits conjuncts into regular sub-formulas and `Inside(inner)` containers.
2. If no regular conjuncts exist, returns empty (no positive anchor).
3. Evaluates each regular conjunct and builds the result set via pairwise
   intersection (`range::intersect`). Conflicting metavar bindings on the same
   key cause an intersection to return `None`, effectively ruling out that pair.
4. For each `Inside(inner)` conjunct, evaluates `inner` to get container ranges,
   then filters the current result set to keep only candidates fully contained
   within at least one container (`RangeWithMetavars::contains`).
5. Evaluates negations, merges them with `range::union`, then removes
   overlapping positive ranges with `range::subtract`.
6. Metavariable conditions (`metavariable-regex`, `metavariable-pattern`,
   `metavariable-comparison`) and `focus` are explicitly ignored; this is
   deferred to Phase 3.

#### `Or(children)`

Evaluates each child, collects the result sets, then merges with `range::union`.
The `union` implementation deduplicates by `(start, end, bindings)`, so two
alternatives that match at the same location with different bindings both appear
in the output.

#### `Inside(inner)` (standalone)

When `Inside` appears at the top level of a formula (not as a conjunct inside
`And`), evaluation is forwarded directly to `inner`. Containment filtering
semantics apply only in the `And` context.

### Timeout and Truncation

At the start of each `eval_formula` call a deadline `Instant` is computed:

```text + Duration::from_millis(config.rule_timeout_ms)

```

Before every recursive call and before every match iteration the evaluator
checks `Instant::now() > deadline`. If exceeded it returns immediately with
`TruncationReason::Timeout { elapsed_ms }` alongside any results collected so
far.

Similarly, once `matches_found >= config.max_matches_per_file`, the evaluator
stops and returns `TruncationReason::MaxMatchesReached { limit }`.

Both truncation conditions are soft: they return partial results rather than an
error, allowing callers to report what was found.

## Pattern Syntax Notes

The `PatternCompiler::compile` method applies `rewrite_ellipsis` before
compiling, translating Semgrep `...` to ast-grep `$$$`. This means:

- `fn foo(...) {}` becomes `fn foo($$$) {}` (any parameters)
- `foo(...)` becomes `foo($$$)` (any arguments)
- `{ ... }` becomes `{ $$$ }` (any block content)

Named single-capture metavars (`$X`) match exactly one AST node. They cannot
capture multi-token sequences such as a macro argument list. Use `...` (or
`$$$`) for positions that require matching multiple nodes.

## Phase 3 Deferral

The following features are intentionally deferred:

- `metavariable-regex`: regex constraint on a bound metavar
- `metavariable-pattern`: structural constraint on a bound metavar
- `metavariable-comparison`: numeric comparison on a bound metavar
- `focus-metavariable`: narrow the reported range to a metavar's span

Raw bindings are preserved in `RangeWithMetavars::bindings` so that Phase 3 can
apply these constraints without re-running the pattern engine.

## Testing

The test suite in `formula.rs` covers:

- Simple leaf pattern match and no-match
- Regex leaf returns empty (AST-mode ignores regex leaves)
- Or with one matching alternative
- Or with empty children
- And with a single conjunct
- And with a negation that removes a matching candidate
- And with two overlapping conjuncts (intersection)
- And with an Inside containment filter (let declarations inside a function)
- Truncation by max-matches cap
- Truncation by timeout (race-safe: zero timeout, no panic assertion)
- `scan_rule` delegation to `eval_formula`
- And with only Inside conjuncts returns empty
- Metavar binding capture (`$F` bound to function name)
- Conditions in And are ignored without error (Phase 2 compatibility)
