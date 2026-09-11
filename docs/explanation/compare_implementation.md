# Compare Module Implementation

## Overview

This document describes the implementation of
`src/scanner/sast/engine/compare.rs`, the closed-grammar comparison expression
evaluator added as part of Phase 3 of the SAST engine. It also records the
coordinated changes to `range.rs` and `formula.rs` needed to introduce the
`MetavarValue` type.

## Files Created or Updated

| File                                 | Status  | Description                                            |
| ------------------------------------ | ------- | ------------------------------------------------------ |
| `src/scanner/sast/engine/compare.rs` | Created | Comparison expression evaluator and public API         |
| `src/scanner/sast/engine/range.rs`   | Updated | Added `MetavarValue` struct; updated `MetavarBindings` |
| `src/scanner/sast/engine/formula.rs` | Updated | Updated binding collection to produce `MetavarValue`   |
| `src/scanner/sast/engine/mod.rs`     | Updated | Exports `compare` module and new `MetavarValue` type   |

## Motivation

Phase 3 of the SAST engine adds support for `metavariable-comparison`, a Semgrep
rule condition that numerically compares a bound metavariable against a literal
or another metavariable. To make this safe and auditable, the engine uses a
closed grammar: only a strict subset of expressions is accepted. Any expression
that falls outside the grammar causes the parent rule to be skipped rather than
failing silently or panicking.

## MetavarValue Type Change

Previously `MetavarBindings` mapped metavariable names to plain `String` values.
Phase 3 requires the engine to know not only the bound text but also the byte
offsets of the matched node in the source file (for future `focus-metavariable`
and span-reporting features). `MetavarValue` captures all three:

```rust
pub struct MetavarValue {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

pub type MetavarBindings = BTreeMap<String, MetavarValue>;
```

The `intersect` function in `range.rs` now compares only the `text` field when
checking binding compatibility. This preserves the original semantic (two
bindings are compatible when they agree on the captured text) while
accommodating the new positional metadata.

## Supported Grammar

```text
expr     := or_expr EOF
or_expr  := and_expr ('or' and_expr)*
and_expr := not_expr ('and' not_expr)*
not_expr := 'not' not_expr | cmp_expr
cmp_expr := value (cmp_op value)?
cmp_op   := '<' | '<=' | '>' | '>=' | '==' | '!='
value    := METAVAR | NUMBER
METAVAR  := '$' [A-Z_][A-Z0-9_]*
NUMBER   := decimal integer or float literal
```

Everything outside this grammar produces `CompareError::UnsupportedConstruct`,
including arithmetic operators (`+`, `-`, `*`, `/`), parentheses, brackets,
string literals, and unknown identifiers.

## Architecture

### Lexer (`tokenize`)

The lexer is a single-pass character iterator that emits tokens or returns
`CompareError::UnsupportedConstruct` on the first rejected character. The token
set is minimal:

- `Token::MetaVar(String)` — `$[A-Z_][A-Z0-9_]*`
- `Token::Number(f64)` — decimal integer or float
- `Token::And`, `Token::Or`, `Token::Not` — lowercase keywords
- `Token::Lt`, `Token::Lte`, `Token::Gt`, `Token::Gte`, `Token::Eq`,
  `Token::Neq` — comparison operators

### Recursive-Descent Parser (`Parser`)

The parser implements the grammar directly as a set of mutually recursive
methods on the `Parser<'a>` struct. Parsing and evaluation are fused in a single
pass: each parse method returns the `bool` result of evaluating its
sub-expression rather than building an intermediate AST.

Two modes are supported:

- **Evaluation mode** (`Parser::for_eval`): metavariable names are resolved from
  the supplied `MetavarBindings`.
- **Validation mode** (`Parser::for_validate`): every metavariable reference
  returns dummy `0.0`; no bindings are needed. Used at rule compile time.

### Metavariable Resolution (`resolve_to_f64`)

Conversion of bound text to `f64` follows the `CompareOptions`:

1. If `strip` is `true`, trailing alphabetic characters and ASCII whitespace are
   removed from the right end of the text (e.g. `"1024k"` becomes `"1024"`).
2. If `base` is `Some(b)`, the text is parsed as a signed integer in base `b`
   via `i64::from_str_radix`, then cast to `f64`.
3. Otherwise the text is parsed as a decimal float via `str::parse::<f64>()`.

Failure at any step returns `CompareError::TypeMismatch`.

### Operator Precedence

Precedence is encoded structurally in the grammar:

```text
not  >  and  >  or
```

`not` binds most tightly (prefix unary), followed by `and` (left-to-right), then
`or` (left-to-right). This matches Python's operator precedence, which Semgrep
uses as a reference.

## Error Handling

`CompareError` has three variants:

| Variant                | Meaning                                        | Caller action  |
| ---------------------- | ---------------------------------------------- | -------------- |
| `UnsupportedConstruct` | Expression outside supported grammar           | Skip the rule  |
| `UnboundMetavar`       | Metavar referenced but not present in bindings | Skip the match |
| `TypeMismatch`         | Bound text could not be parsed as a number     | Skip the match |

None of these variants are fatal; the engine continues scanning with other rules
and matches.

## Testing

The test suite in `compare.rs` covers all 16 required cases:

- All six comparison operators (`<`, `<=`, `>`, `>=`, `==`, `!=`) at true and
  false boundaries
- `and`-higher-than-`or` operator precedence (bindings chosen to produce
  different results under the two interpretations)
- `strip` option removing a trailing alphabetic suffix
- `base` option parsing hexadecimal text (`"1000"` as `0x1000 = 4096`)
- `TypeMismatch` for non-numeric bound text
- `UnsupportedConstruct` for division, addition, and parentheses
- `validate_comparison` accepting a valid expression
- `validate_comparison` rejecting an arithmetic expression

The updated `range.rs` test suite verifies that `MetavarValue` bindings compare
correctly in `intersect`, `union`, and the `RangeWithMetavars` struct.
