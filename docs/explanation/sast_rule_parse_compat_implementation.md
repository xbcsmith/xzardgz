# SAST Rule Parse and Compatibility Gate Implementation

## Overview

This document describes the implementation of two modules that form the
front-end of the SAST rule compilation pipeline:

- `src/scanner/sast/rule/compat.rs` -- the compatibility gate
- `src/scanner/sast/rule/parse.rs` -- the rule schema compiler

Together they convert a deserialized `RuleSchema` (produced by serde from a
semgrep-dialect YAML file) into a `CompileOutcome`: either a typed intermediate
representation (`RuleIr`) ready for the engine, or a `Skipped` record explaining
why the rule cannot be executed by this engine version.

## Module Responsibilities

### compat.rs

The compatibility gate is a pure inspection pass. It takes a `RuleSchema` and
returns `Vec<SkipReason>`. An empty vec means the rule is fully supported; any
non-empty vec causes the entire rule to be skipped without partial evaluation.

The gate enforces the following exclusions:

| Construct                   | Skip reason            | Notes                                           |
| --------------------------- | ---------------------- | ----------------------------------------------- |
| `mode: taint`               | `TaintMode`            | Deferred to sast_taint_mode_implementation_plan |
| `mode: join`                | `JoinMode`             | Not planned                                     |
| `mode: extract`             | `ExtractMode`          | Not planned                                     |
| `mode: step`                | `StepMode`             | Not planned                                     |
| `fix-regex` field           | `FixRegex`             | Requires regex substitution engine              |
| `pattern-propagators`       | `PatternPropagators`   | Requires taint plumbing                         |
| Deep expression `<... ...>` | `DeepExpression`       | Requires special AST traversal                  |
| Typed metavar `(T $X)`      | `TypedMetavariable`    | Requires type inference                         |
| `metavariable-analysis`     | `MetavariableAnalysis` | Requires external analyzer                      |
| No supported language       | `NoSupportedLanguage`  | Only Rust, regex, generic supported             |

Reasons are deduplicated before being returned: if two pattern terms both
contain deep expressions, only one `DeepExpression` entry appears. Deduplication
uses `std::mem::discriminant`, which avoids requiring `PartialEq` on
`SkipReason` while correctly handling all variants including
`NoSupportedLanguage(Vec<String>)`.

Pattern string scanning (deep expression and typed metavar detection) is applied
to semgrep `pattern` fields only. `pattern-regex` fields contain literal regular
expressions and are intentionally excluded; the `<...` and `(T $X)` constructs
have unrelated or different meanings inside regular expressions.

### parse.rs

The rule compiler converts a `RuleSchema` to a `CompileOutcome`. Compilation
proceeds in five ordered steps:

1. Run the compatibility gate; return `Skipped` immediately if any reasons are
   found.
2. Validate the rule id against `^[a-zA-Z0-9._-]+$` using a char-by-char scan
   (no `regex` crate dependency introduced).
3. Count formula roots. Exactly one of `pattern`, `patterns`, `pattern-either`,
   or `pattern-regex` must be present.
4. Compile the formula to an `ir::Formula` value.
5. Assemble and return the `RuleIr` node.

## Formula Compilation

The four formula root types compile as follows:

- `pattern: <string>` produces `Formula::Leaf(Leaf::Pattern(s))`
- `pattern-regex: <string>` produces `Formula::Leaf(Leaf::Regex(s))`
- `pattern-either: [terms]` produces `Formula::Or(branches)` where each branch
  is compiled by `compile_term`
- `patterns: [terms]` produces
  `Formula::And { conjuncts, negations, conditions, focus }` via
  `compile_patterns`

### Bucket Distribution in compile_patterns

Each `PatternTerm` inside a `patterns` list may simultaneously contribute to
multiple buckets. Processing is additive, not exclusive:

- A term with `pattern` contributes a conjunct.
- The same term may also have `metavariable-regex`, adding a condition.
- A separate term with only `pattern-not` contributes a negation.
- A term with `focus-metavariable` extends the focus list.

This matches the semgrep semantics where `patterns` is an implicit `AND` over
all its terms, with negation and conditions as annotations on the conjunction.

### Invariants Enforced

- Empty `patterns` list: `Err(Invariant)` with message explaining that the list
  must not be empty.
- `patterns` with no positive term (only `pattern-not` items): `Err(Invariant)`.
- Multiple formula roots (e.g., both `pattern` and `patterns`): `Err(Schema)`.
- Zero formula roots: `Err(Schema)`.
- Invalid rule id: `Err(InvalidId)`.

## Error Handling

Both modules use `thiserror`-derived error types from
`src/scanner/sast/error.rs`. No `unwrap()` or `expect()` calls appear without a
justification comment. The only `unwrap_or_default()` used is on
`Option<String>` when building a `MetavarPattern` condition from a term that has
neither `pattern` nor `pattern_regex` -- the resulting empty string is a safe,
semantically inert default in that context.

## Typed Metavariable Detection

The typed-metavariable check implements the regex
`\([A-Za-z_][A-Za-z0-9_]* \$[A-Z_][A-Z0-9_]*\)` as a manual byte-level state
machine, avoiding a runtime `regex` crate dependency. The algorithm:

1. Scan for `(`.
2. Match `TypeName: [A-Za-z_][A-Za-z0-9_]*`.
3. Expect `$`.
4. Match `METAVAR: [A-Z_][A-Z0-9_]*`.
5. Expect `)`.

On any mismatch the scanner advances `i` by one and retries from the next `(`.

## Test Coverage

### compat.rs (16 tests)

One test per invariant, covering both positive and negative cases:

- Mode tests: `taint`, `join`, `extract`, `step` each skipped; `None` not
  skipped.
- Feature tests: `fix-regex`, `pattern-propagators` (top level), deep expression
  (top-level pattern and inside a patterns term), typed metavar,
  `metavariable-analysis`.
- Language tests: `cobol` triggers `NoSupportedLanguage`; `rust`, `regex`,
  `generic` each pass.
- Deduplication test: two terms both containing deep expressions produce exactly
  one `DeepExpression` reason.

### parse.rs (15 tests)

- Formula root tests: `pattern`, `pattern-regex`, `patterns` with negation,
  `pattern-either`.
- Error tests: no formula root, multiple roots, invalid id (space in id), empty
  id, empty patterns list, patterns with no positive term.
- Compat delegation: taint mode produces `Skipped` outcome, not an error.
- Bucket tests: focus metavariable collected into `And.focus`; metavar-regex
  condition collected into `And.conditions`.
- Integration tests: round-trip YAML parse + compile; malformed YAML returns
  `Yaml` error.

## Relation to Broader SAST Pipeline

The parse/compat layer sits between YAML deserialization and engine execution:

```text
YAML string
    -> serde_yaml -> RuleFile
    -> compile_rule_file -> Vec<Result<CompileOutcome>>
        -> check_compat (compat.rs)    // gate pass
        -> compile_rule (parse.rs)     // schema -> IR
    -> engine (engine/) -> findings
```

Rules that produce `Skipped` outcomes are surfaced in the scan report as
unsupported-rule diagnostics. They do not cause the overall scan to fail.

## Future Work

- Deep expression support requires extending the engine with a recursive descent
  matcher that can match across intervening statements.
- Typed metavariable support requires hooking into the `ast-grep` type resolver,
  which is language-specific.
- Additional mode support (`taint`, `join`) is tracked in
  `sast_taint_mode_implementation_plan.md`.
