# Prefilter Implementation

## Overview

The prefilter module (`src/scanner/sast/target/prefilter.rs`) provides a fast
file pre-screening predicate for the SAST scanning pipeline. Before invoking the
tree-sitter parser and the full formula evaluation engine on a source file, the
scanner consults the prefilter to determine whether the raw file bytes contain
any signal that could satisfy a loaded rule. Files that cannot possibly match
are skipped without any AST construction.

This document describes the design decisions, literal extraction algorithm,
formula traversal strategy, and testing approach for the prefilter.

## Scope

This implementation delivers:

- `src/scanner/sast/target/prefilter.rs` — the `Prefilter` struct and
  `extract_literals` helper
- `src/scanner/sast/target/mod.rs` — `target` submodule declaration
- `src/scanner/sast/mod.rs` — `pub mod target` wiring

## Design Goals

### Soundness (no false negatives)

The prefilter must never say "skip" when a rule could in fact match. This is
achieved by:

1. Extracting only positive signals (literals from `pattern:` conjuncts, raw
   patterns from `pattern-regex:`).
2. Ignoring negations entirely — a negation cannot tell us a file is
   unmatchable.
3. Using `always_analyze = true` for any rule that contributes no extractable
   signal, ensuring such rules always trigger full evaluation.

### Efficiency

Two fast bulk-search structures are used:

- **Aho-Corasick** (`aho_corasick::AhoCorasick`) for literal string lookup. All
  extracted literals from all rules are compiled into one multi-pattern
  automaton. A single linear scan of the file bytes answers the question "does
  this file contain any rule literal?".
- **RegexSet** (`regex::RegexSet`) for `pattern-regex` rules. All regex patterns
  are compiled into one set that answers "does any regex match?" in a single
  pass.

Both structures are built once at scanner startup and reused across all files.

## Data Flow

```text
&[RuleIr]
    |
    v
Prefilter::from_rules()
    |
    for each rule:
    |   collect_signals(&formula, &mut literals, &mut regexes)
    |       Formula::Leaf(Pattern(text))  -> extract_literals(text) -> literals
    |       Formula::Leaf(Regex(text))    -> regexes
    |       Formula::And { conjuncts, .. } -> recurse conjuncts only
    |       Formula::Or(children)         -> recurse all children
    |       Formula::Inside(inner)        -> recurse inner
    |
    |   if literals.is_empty() && regexes.is_empty():
    |       always_analyze = true
    |
    v
AhoCorasick::builder().build(&all_literals)   -> Option<AhoCorasick>
RegexSet::new(&all_regexes)                   -> Option<RegexSet>
    |
    v
Prefilter { ac, regex_set, always_analyze }
    |
    v
file_may_match(content: &[u8]) -> bool
    if always_analyze         -> true
    if ac.is_match(content)   -> true
    if rs.is_match(&text)     -> true
    else                      -> false
```

## Literal Extraction Algorithm

Pattern strings such as `"md5::compute($DATA)"` contain metavariable tokens
(`$X`, `$$$`, `$$$BODY`, `...`) that act as wildcards. The useful literal
portions are the segments between those tokens.

### Splitting regex

The `METAVAR_SPLITTER` static compiles once via `OnceLock` and matches, in
priority order:

| Token pattern            | Example      | Description           |
| ------------------------ | ------------ | --------------------- |
| `\$\$\$[A-Z_][A-Z0-9_]*` | `$$$BODY`    | Named multi-metavar   |
| `\$\$\$`                 | `$$$`        | Unnamed multi-metavar |
| `\$[A-Z_][A-Z0-9_]*`     | `$X`, `$RNG` | Single named metavar  |
| `\.\.\.`                 | `...`        | Pattern ellipsis      |

Longest alternatives are listed first to prevent the single-`$` rule from
consuming the leading `$` of a `$$$` token.

### Segment filter

After splitting, each segment is filtered by these rules (applied in order):

1. Strip leading and trailing whitespace.
2. Count non-whitespace characters; discard if fewer than 3.
3. Discard if the segment contains no alphanumeric character.

### Examples

| Pattern                                | Extracted literals                                        |
| -------------------------------------- | --------------------------------------------------------- |
| `md5::compute($DATA)`                  | `["md5::compute("]`                                       |
| `RsaPrivateKey::new(&mut $RNG, $BITS)` | `["RsaPrivateKey::new(&mut"]`                             |
| `Md5::new()`                           | `["Md5::new()"]`                                          |
| `$X`                                   | `None`                                                    |
| `fn $F() {}`                           | `None` (`"fn"` = 2 non-ws; `"() {}"` has no alphanumeric) |
| `foo($$$)`                             | `["foo("]`                                                |
| `fn $F() { $$$BODY }`                  | `None`                                                    |

## Formula Tree Traversal

`collect_signals` walks the formula tree, accumulating literals and regex
patterns:

| Formula variant         | Action                                   |
| ----------------------- | ---------------------------------------- |
| `Leaf(Pattern(text))`   | `extract_literals(text)` into `literals` |
| `Leaf(Regex(text))`     | append `text` to `regexes`               |
| `And { conjuncts, .. }` | recurse into `conjuncts` only            |
| `Or(children)`          | recurse into every child                 |
| `Inside(inner)`         | recurse into `inner`                     |

The `And` variant's `negations`, `conditions`, and `focus` fields are
intentionally ignored. A negation specifies what must _not_ be present, which
cannot be used to prove a file is unmatchable (it could still match the positive
conjuncts and then fail the negation check in the engine). Treating negation
literals as prefilter signals would violate soundness.

## Over-general Rules and `always_analyze`

A rule that produces no literals and no regex patterns (e.g., a formula of
`Leaf(Pattern("$X"))`) cannot be prefiltered. When any such rule is encountered,
`always_analyze` is set to `true` for the entire `Prefilter` instance.
Thereafter, `file_may_match` unconditionally returns `true`, ensuring the rule
is always evaluated against every file.

The cost is that the Aho-Corasick and RegexSet optimisations are bypassed for
the entire scan run. Authors of over-general rules should be aware of this
trade-off.

## Error Handling

Both `AhoCorasick::builder().build()` and `RegexSet::new()` return `Result`.
Errors are absorbed via `.ok()`, producing `None`. In practice, errors arise
only from empty pattern lists (handled before calling the constructors) or from
pathological inputs that are never generated by the rule parser.

## Testing

The unit test suite covers all ten required scenarios plus additional cases:

| Test name                                                           | Scenario                              |
| ------------------------------------------------------------------- | ------------------------------------- |
| `test_prefilter_always_analyze_when_no_literals_extractable`        | Bare `$X` forces `always_analyze`     |
| `test_prefilter_file_may_match_literal_present_returns_true`        | Literal in content returns true       |
| `test_prefilter_file_may_match_literal_absent_returns_false`        | Literal absent returns false          |
| `test_prefilter_file_may_match_always_analyze_returns_true`         | `always_analyze` overrides absence    |
| `test_prefilter_no_rules_returns_false`                             | Empty rule slice always returns false |
| `test_prefilter_regex_rule_matches_raw_content`                     | Regex leaf drives match               |
| `test_prefilter_negation_does_not_affect_prefilter`                 | Negations skipped entirely            |
| `test_prefilter_multiple_rules_any_match_passes`                    | Second-rule literal triggers pass     |
| `test_extract_literals_from_pattern_with_metavar_returns_prefix`    | md5 prefix extracted                  |
| `test_extract_literals_from_bare_metavar_returns_none`              | `$X` produces None                    |
| `test_extract_literals_from_plain_string_returns_whole_string`      | No metavar: whole string kept         |
| `test_extract_literals_fn_pattern_with_short_segments_returns_none` | Short/punct segments dropped          |
| `test_extract_literals_rsa_pattern_returns_long_prefix`             | RSA prefix extracted                  |
| `test_extract_literals_multi_metavar_splits_and_keeps_prefix`       | `$$$` split works                     |
| `test_extract_literals_named_multi_metavar_splits_correctly`        | `$$$BODY` split produces None         |
| `test_extract_literals_ellipsis_stripped_as_splitter`               | `...` acts as split point             |

All tests follow the `test_<function>_<condition>_<expected>` naming convention
required by the project's coding standards.
