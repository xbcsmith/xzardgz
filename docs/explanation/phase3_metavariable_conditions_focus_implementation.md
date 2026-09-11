# Phase 3: Metavariable Conditions and Focus Implementation

## Overview

Phase 3 activates the three metavariable condition types and
`focus-metavariable` that were deferred from Phase 2. The additions are:

- `metavariable-regex`: asserts that the source text bound to a named metavar
  matches (or does not match) a regular expression.
- `metavariable-pattern`: re-parses the source text of a bound metavar and tests
  it against a structural AST pattern, optionally in a different language.
- `metavariable-comparison`: evaluates a numeric comparison whose operands are
  metavar values or numeric literals.
- `focus-metavariable`: narrows the reported match range to the byte span of a
  named metavar rather than the full pattern match span.

Phase 2 preserved raw bindings in `RangeWithMetavars::bindings` for exactly this
purpose. Phase 3 consumes those bindings without re-running the AST pattern
engine.

## Architecture

Conditions and focus are wired into the `eval_and` function in
`src/scanner/sast/engine/formula.rs`. The evaluation order inside `eval_and` is:

1. Conjunct intersection (Phase 2).
2. `Inside` containment filtering (Phase 2).
3. Negation subtraction (Phase 2).
4. Condition evaluation (Phase 3 addition).
5. Focus narrowing (Phase 3 addition).

Steps 4 and 5 consume the `conditions` and `focus` fields of the `Formula::And`
variant, which Phase 2 explicitly ignored via a `// Phase 3 deferred` comment.
No structural changes are made to the `Formula` IR or to the `And` variant
itself; the new code is additive.

## Components

### compare.rs

`src/scanner/sast/engine/compare.rs` is a closed-grammar evaluator for
`metavariable-comparison` expressions. It accepts the `comparison` string from
the rule YAML and evaluates it against a set of resolved metavar values.

The supported grammar is intentionally narrow:

- **Binary comparisons**: `<`, `<=`, `>`, `>=`, `==`, `!=`.
- **Left operand**: a metavariable name (e.g. `$BITS`).
- **Right operand**: a numeric literal (integer or float) or another metavar.
- **`strip` option**: a suffix string to remove before parsing the numeric value
  (e.g. `strip: "K"` turns `"4K"` into `4`).
- **`base` option**: an integer radix for parsing (e.g. `base: 16` for
  hexadecimal input).

Any construct outside this grammar, including boolean operators, function calls,
ternary expressions, and assignment, causes the evaluator to return an
`UnsupportedComparison` error. Rules that produce `UnsupportedComparison` during
compilation are placed in the `Skipped` outcome and excluded from evaluation.
This design fails loudly at load time rather than silently at scan time.

### conditions.rs

`src/scanner/sast/engine/conditions.rs` implements the three condition types and
the focus step.

**`MetavarRegex`**: extracts the `text` field of the named `MetavarValue`,
compiles the regex using the size-limited builder (see Security Notes), and
tests whether the text matches. The `not` flag inverts the result.

**`MetavarPattern`**: extracts the `text` field, re-parses it into an AST using
either the rule's language or the optional `language` override, then runs the
structural pattern match. The recursion depth is capped at 10 to prevent stack
overflow from deeply nested sub-patterns.

**`MetavarComparison`**: delegates to `compare.rs` after resolving each
metavariable operand in the expression to its bound text.

**Focus narrowing**: after all conditions pass, each surviving
`RangeWithMetavars` is replaced by a new range whose `start` and `end` are taken
from the `MetavarValue` for the first listed focus metavar. If the named metavar
is absent from the bindings map, the range is dropped.

## Data Model Change: MetavarValue

In Phase 2 the bindings type is:

```rust
pub type MetavarBindings = BTreeMap<String, String>;
```

Each metavar is stored as its bound source text. This is sufficient for
`metavariable-regex` and `metavariable-pattern`, which only need the text, but
not for `focus-metavariable`, which must replace the overall match range with
the metavar's own byte span.

Phase 3 introduces `MetavarValue` and updates the alias:

```rust
pub struct MetavarValue {
    pub text:  String,
    pub start: usize,
    pub end:   usize,
}

pub type MetavarBindings = BTreeMap<String, MetavarValue>;
```

`text` is the source text the metavar was bound to. `start` and `end` are the
byte offsets of the matched AST node in the source file (half-open interval
`[start, end)`).

All sites that previously read a binding value as a plain `String` now access
`value.text`. The `intersect` function in `range.rs` checks binding
compatibility using `text` equality, unchanged in semantics. The
`eval_leaf_pattern` function in `formula.rs` is updated to populate both `text`
and `start`/`end` from the `NodeMatch` returned by ast-grep.

## Worked Example

The `rust-weak-rsa-key` rule in
`src/scanner/sast/rules/crypto/rust_weak_rsa_key.yaml` is structured as:

```yaml
patterns:
  - pattern: RsaPrivateKey::new(&mut $RNG, $BITS)
  - metavariable-comparison:
      metavariable: $BITS
      comparison: $BITS < 2048
  - focus-metavariable: $BITS
```

**Positive case (`testdata/sast/fixtures/rsa_weak_positive.rs`):**

```rust
let key = RsaPrivateKey::new(&mut rng, 1024).unwrap();
```

1. The pattern `RsaPrivateKey::new(&mut $RNG, $BITS)` matches the call
   expression. `$BITS` is bound to
   `MetavarValue { text: "1024", start: .., end: .. }`.
2. `metavariable-comparison` evaluates `1024 < 2048`, which is `true`. The
   candidate range survives the condition step.
3. `focus-metavariable: $BITS` replaces the reported range with the byte span of
   `1024`. The finding points at the key-size argument, not the entire call.

**Negative case (`testdata/sast/fixtures/rsa_weak_negative.rs`):**

```rust
let key = RsaPrivateKey::new(&mut rng, 2048).unwrap();
```

1. The pattern matches. `$BITS` is bound to `MetavarValue { text: "2048", .. }`.
2. `metavariable-comparison` evaluates `2048 < 2048`, which is `false`. The
   candidate range is dropped.
3. No finding is reported.

## Security Notes

The closed-grammar design in `compare.rs` ensures that the `comparison` string
from rule YAML is never passed to an interpreter or `eval` function. The
evaluator is a single-pass recursive descent parser that recognises only a
narrow token set. Anything outside that set is rejected with
`UnsupportedComparison` at rule-load time. This eliminates the code-injection
risk that would arise from dynamically evaluating arbitrary expressions.

The recursion depth for `metavariable-pattern` is limited to 10 levels. Each
re-parse of a bound metavar may itself trigger further condition evaluation if
the sub-rule also contains `metavariable-pattern` clauses. The depth counter is
carried in the evaluation context and returns an error when exceeded, preventing
crafted rules from triggering a stack overflow.

The `metavariable-regex` evaluator builds regex objects through a size-limited
builder that rejects patterns whose compiled representation exceeds a fixed byte
cap. This prevents regex denial-of-service from pathologically complex patterns
in untrusted rule files. The cap is configurable via `SastEngineConfig`.

## Testing

Integration tests for Phase 3 live in `src/scanner/sast/mod.rs` alongside the
Phase 2 integration suite:

- `test_fixture_rsa_weak_rule_matches_positive_fixture`: evaluates
  `rust-weak-rsa-key` against `testdata/sast/fixtures/rsa_weak_positive.rs` and
  asserts that at least one match is produced and that the reported range covers
  only the `1024` token.
- `test_fixture_rsa_weak_rule_does_not_match_negative_fixture`: evaluates
  against `testdata/sast/fixtures/rsa_weak_negative.rs` and asserts zero
  matches.

Unit tests for the new modules:

- `compare.rs`: integer literal comparisons, floating-point literal comparisons,
  base and strip preprocessing, metavar-to-metavar comparisons, all six
  operators, unsupported construct rejection, and numeric boundary values.
- `conditions.rs`: each of the three condition types in isolation, `not` flag
  inversion for `MetavarRegex`, focus narrowing for a single metavar, focus when
  multiple metavars are bound, and no-op behaviour when both `conditions` and
  `focus` are empty.

## Related Phases

Phase 2 implements the `eval_formula` evaluator and establishes the `conditions`
and `focus` fields on `Formula::And`. That implementation is described in
`docs/explanation/phase2_sast_formula_engine_implementation.md`. The Phase 2
bindings infrastructure and explicit deferral comments are the direct entry
points for the Phase 3 additions.

Phase 4 will implement taint tracking. The per-node span data introduced in the
`MetavarValue` type provides the byte-level location information that the taint
engine requires to trace data flow across call boundaries.
