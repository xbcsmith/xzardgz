# Regex Mode Engine Implementation

## Overview

The regex mode scanner is the first functional scanning engine in the SAST
pipeline. It handles rules whose `languages` field is set to `regex` or
`generic`. Unlike the AST-based engine, regex mode applies a `pattern-regex`
expression directly over raw file bytes without invoking any language parser.

This document describes the design decisions, data flow, and security properties
of `src/scanner/sast/engine/regex_mode.rs`.

## Scope

This implementation is Phase 1 of the SAST scanning tool plan. It delivers:

- `src/scanner/sast/engine/regex_mode.rs` - the regex-mode scanner
- `src/scanner/sast/engine/mod.rs` - engine module declarations and re-exports
- `src/scanner/sast/error.rs` - `SastError` and `RuleParseError`
- `src/scanner/sast/rule/ir.rs` - compiled rule IR (`RuleIr`, `Formula`, `Leaf`)
- `src/scanner/sast/rule/metadata.rs` - `Severity`, `Confidence`, `RuleMetadata`
- `src/scanner/sast/rule/mod.rs` - rule submodule declarations
- `src/scanner/sast/mod.rs` - SAST module declarations

## Data Flow

```text
RuleIr (formula: Formula::Leaf(Leaf::Regex("..."))
    |
    v
RegexModeScanner::new()
    |-- extract_regex_pattern() walks formula tree -> &str
    |-- RegexBuilder::new(pattern)
    |       .size_limit(10_485_760)      <- NFA limit
    |       .dfa_size_limit(10_485_760) <- DFA cache limit
    |       .build()
    v
RegexModeScanner { rule_id, compiled: regex::Regex }
    |
    v
scan_bytes(content: &[u8], max_matches: usize)
    |-- String::from_utf8_lossy(content)  -> Cow<str>
    |-- build line_starts[] from content  -> Vec<usize>
    |-- compiled.captures_iter(text).take(max_matches)
    |       for each Captures:
    |           byte_start, byte_end from caps.get(0)
    |           snippet = text[byte_start..byte_end]
    |           line_start = partition_point(|s| s <= byte_start)
    |           line_end   = partition_point(|s| s <= byte_end - 1)
    |           metavariables: {$NAME -> value} for each named group
    v
Vec<RegexMatch>
```

## Formula Tree Traversal

`extract_regex_pattern` performs a depth-first search of the formula tree
looking for the first `Leaf::Regex` node:

| Formula variant         | Behaviour                           |
| ----------------------- | ----------------------------------- |
| `Leaf(Regex(p))`        | Returns `Some(p)`                   |
| `Leaf(Pattern)`         | Returns `None` (not a regex leaf)   |
| `And { conjuncts, .. }` | Searches conjuncts in order         |
| `Or(alts)`              | Searches only the first alternative |
| `Inside(inner)`         | Recurses into the inner formula     |

The choice to search only the first `Or` alternative is intentional: regex mode
rules should use `And` conjuncts for multiple patterns, not `Or`. Deeper search
of `Or` alternatives would silently ignore later alternatives and produce
confusing results.

## ReDoS Mitigation

All regexes are compiled through `RegexBuilder` with explicit size limits:

```rust
const REGEX_SIZE_LIMIT: usize = 10_485_760; // 10 MiB

RegexBuilder::new(pattern)
    .size_limit(REGEX_SIZE_LIMIT)
    .dfa_size_limit(REGEX_SIZE_LIMIT)
    .build()
```

The `size_limit` bounds the NFA instruction count. The `dfa_size_limit` bounds
the lazy DFA cache. Any pattern whose compiled representation would exceed 10
MiB is rejected at rule-load time with `SastError::RegexCompile`, not at scan
time. This means the cost of a pathological regex is paid once during startup,
not once per scanned file.

The `regex` crate uses a Thompson NFA that does not exhibit catastrophic
backtracking. However, patterns with very large alternations can still produce
enormous automata that consume significant memory and CPU during DFA
construction. The 10 MiB limit prevents this class of resource exhaustion.

## UTF-8 Handling

`scan_bytes` accepts raw `&[u8]`. Invalid UTF-8 sequences are replaced with the
Unicode replacement character (U+FFFD) via `String::from_utf8_lossy` before
matching. This means:

- The scanner never panics on arbitrary file content.
- Byte offsets reported in `RegexMatch` refer to positions in the lossy-decoded
  string, which may differ from positions in the original bytes when invalid
  sequences are present.
- For files that are expected to be valid UTF-8 (Rust, YAML, JSON source), the
  offsets are exact.

## Line Number Calculation

Line numbers are computed by maintaining a sorted `Vec<usize>` (`line_starts`)
of the byte offsets at which each line begins. The first entry is always `0`. A
newline at byte `i` causes `i + 1` to be appended.

Given a byte offset, the 1-based line number is found with a binary search:

```rust
line_starts.partition_point(|&s| s <= byte_offset)
```

`partition_point` returns the count of elements satisfying the predicate, which
equals the 1-based line number. This is O(log n) in the number of lines.

The `line_end` of a match uses `byte_end.saturating_sub(1)` as the lookup
offset. This prevents a match that ends exactly at the start of a new line from
being reported as ending on that next line.

## Metavariable Binding

Named capture groups in the regex are bound as `$NAME` metavariables. The group
name is uppercased unconditionally so that `(?P<token>...)` and `(?P<TOKEN>...)`
both produce the key `$TOKEN`. This matches Semgrep's convention.

Unnamed groups (positional groups) are not exposed as metavariables.

## Error Types

| Error variant             | Condition                                          |
| ------------------------- | -------------------------------------------------- |
| `SastError::Internal`     | Formula contains no `Leaf::Regex` node             |
| `SastError::RegexCompile` | Regex syntax error or compiled size exceeds 10 MiB |

## Testing

The unit test suite covers:

| Test category       | Tests                                                     |
| ------------------- | --------------------------------------------------------- |
| Constructor success | valid regex, rule_id propagated                           |
| Constructor failure | no regex leaf, invalid syntax, oversized pattern          |
| Basic matching      | simple match, no-match, byte offsets, snippet content     |
| Metavariables       | single group, multiple groups, uppercase normalisation    |
| max_matches limit   | limit=3 from 8 matches, limit=0 returns empty             |
| UTF-8 handling      | purely invalid bytes produce no matches without panicking |
| Line numbers        | single-line match, multi-line span, first-line match      |
| Formula traversal   | And conjunct, Or first element, Inside wrapper            |
| byte_to_line helper | offset 0, mid-line, line boundary, past last known line   |

All tests follow the `test_<function>_<condition>_<expected>` naming convention.
