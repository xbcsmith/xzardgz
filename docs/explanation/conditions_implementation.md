# Conditions Module Implementation

## Overview

`src/scanner/sast/engine/conditions.rs` implements metavariable condition
evaluation for the SAST engine. It is the Phase 3 filtering layer: after
`eval_formula` (Phase 2) produces a raw set of `RangeWithMetavars` values,
`apply_conditions` and `apply_focus` narrow that set to only the matches that
satisfy all declared constraints.

## Components

### `ConditionError`

A `thiserror`-derived error enum with five variants split into two categories:

**Rule-level errors** (bubble up, abort rule evaluation):

- `UnsupportedComparison(String)` - comparison expression uses unsupported
  grammar constructs (e.g. `+`, `/`, parentheses).
- `RecursionLimitExceeded` - `metavariable-pattern` nesting reached ten levels.
- `RegexCompile(String)` - regex is syntactically invalid or exceeds the 10 MiB
  NFA/DFA size limit.

**Range-level errors** (filter the current range, continue processing):

- `UnboundMetavar(String)` - a referenced metavariable had no binding.
- `TypeMismatch(String)` - bound text could not be parsed as a number.

### `MAX_RECURSION_DEPTH`

Private constant (`10`) capping the recursive depth of `eval_metavar_pattern`.
Child modules in the same file can access it directly as
`super::MAX_RECURSION_DEPTH`.

### `eval_metavar_regex`

Builds `^(?:{regex_str})$` to force a full-string match (anchored), then
evaluates it against the bound text. The `not` flag inverts the result with an
XOR. `RegexBuilder` size limits (10 MiB NFA and 10 MiB DFA) match those used by
`RegexModeScanner`, bounding ReDoS risk.

### `eval_metavar_pattern`

Re-parses the bound text as an AST via `SupportLang::ast_grep` (from the
`LanguageExt` trait), then checks whether `pattern_str` matches anywhere in that
re-parsed tree using `PatternCompiler`. The language is resolved from the
optional `language` string via `SupportLang::from_str`, defaulting to
`SupportLang::Rust` for unknown or absent values.

### `eval_metavar_comparison`

Thin wrapper around `eval_comparison` from `engine::compare`. Translates
`CompareError` variants to the corresponding `ConditionError` variants. The
`_metavar` parameter is carried for API consistency with
`Condition::MetavarComparison` but is not used in the body (the comparison
expression string directly references metavariable names).

### `apply_conditions`

Iterates over each `RangeWithMetavars` and evaluates every `Condition` in order
with short-circuit on first `false`. Error handling policy:

| Error                                                             | Action                                 |
| ----------------------------------------------------------------- | -------------------------------------- |
| `UnsupportedComparison`, `RecursionLimitExceeded`, `RegexCompile` | Return `Err` immediately               |
| `UnboundMetavar`, `TypeMismatch`                                  | Treat as `false`; drop range, continue |

### `apply_focus`

Narrows each range to the intersection of the byte extents of one or more named
focus metavariables. The intersection is computed as `[max(starts), min(ends))`.
Ranges where any focus variable is unbound or where the intersection is empty
(`start > end`) are dropped. The result is sorted by `(start, end)` using the
`Ord` implementation on `RangeWithMetavars`.

## Design Decisions

**Anchoring in `eval_metavar_regex`**: Semgrep semantics require a full-string
match for `metavariable-regex`, not a substring search. The `^(?:...)$` wrapper
guarantees this regardless of whether the caller supplies anchors.

**10 MiB size limits**: Consistent with `RegexModeScanner`. The limit applies to
the compiled NFA program (`size_limit`) and the lazy DFA cache
(`dfa_size_limit`). Exceeding either causes `RegexBuilder::build` to return an
error, which surfaces as `ConditionError::RegexCompile`.

**`LanguageExt` trait import**: The `ast_grep` method on `SupportLang` is
provided by `ast_grep_core::tree_sitter::LanguageExt`. This trait must be in
scope for `lang.ast_grep(text)` to resolve.

**`apply_focus` early return for empty `focus_vars`**: When `focus_vars` is
empty the function returns the original `ranges` vector unchanged without
iterating. This avoids the `usize::MAX` sentinel logic entirely for the common
case.

## Testing

All 20 required tests pass. The oversized-regex test generates 100,000
alternation branches (`(?:test0)|(?:test1)|...|(?:test99999)`) which reliably
exceeds the 10 MiB NFA size limit, causing `RegexBuilder::build` to return an
error.

## Module Declaration

`pub mod conditions;` was added to `src/scanner/sast/engine/mod.rs` to make the
module part of the compilation unit. Integration of `apply_conditions` and
`apply_focus` into the formula evaluator (`formula.rs`) is deferred to a
subsequent phase.
