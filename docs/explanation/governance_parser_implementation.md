# Governance Parser Implementation

## Overview

`src/governance/parser.rs` adds a Markdown parser that converts an `AGENTS.md`
file into structured `GovernanceRule` values. It uses `pulldown-cmark` (v0.12),
added to `Cargo.toml` as a new direct dependency in this phase.

## Public API

| Symbol                                                  | Kind     | Purpose                                    |
| ------------------------------------------------------- | -------- | ------------------------------------------ |
| `parse_agents_md(content: &str) -> Vec<GovernanceRule>` | function | Top-level entry point                      |
| `infer_enforcement(text: &str) -> EnforcementLevel`     | function | Keyword-based enforcement classification   |
| `heading_to_slug(heading: &str) -> String`              | function | Stable lowercase ID slug from heading text |

The private helper `contains_word(text_upper, word)` performs byte-level
whole-word matching on an already-uppercased string so that partial matches such
as "MAY" inside "MANDATORY" are rejected.

## Parsing Strategy

The parser drives `pulldown_cmark::Parser` in a single forward pass and
maintains a small state machine:

- `in_heading` / `heading_text` — accumulate text between `Start(Heading)` and
  `End(Heading)` events.
- `item_depth` — depth counter incremented by `Start(Item)` and decremented by
  `End(Item)`; only depth-1 items produce rules (nested items are ignored).
- `item_text` — accumulates text, inline code, and break events for the current
  top-level item.
- `heading_item_count` — 1-based counter that resets on each new heading; used
  to generate the `.<n>` suffix of the rule ID.

List items with no text after trimming are silently skipped.

## Rule ID Scheme

Rule IDs follow the pattern `agents_md.<heading_slug>.<n>`:

- `<heading_slug>` is produced by `heading_to_slug`: lowercase, collapse
  consecutive non-alphanumeric characters into a single `_`, strip any leading
  or trailing `_`.
- `<n>` is a 1-based integer that resets for each heading.

Example: the second item under `## Critical Rules` gets id
`agents_md.critical_rules.2`.

## Enforcement Inference

`infer_enforcement` uppercases the item text and calls `contains_word` for each
keyword:

| Whole-word match     | Enforcement   |
| -------------------- | ------------- |
| `MUST` or `REQUIRED` | `Required`    |
| `MAY` or `OPTIONAL`  | `Optional`    |
| None of the above    | `Recommended` |

The whole-word check prevents false positives such as "MAY" in "MANDATORY" or
"MUST" in "MUSTARD".

## Module Registration

`pub mod parser;` was added to `src/governance/mod.rs` alongside the existing
`loader`, `rules`, and `validator` modules. No re-exports were added to the
top-level `pub use` block; callers reference items as
`xzardgz::governance::parser::*`.

## Testing

28 unit tests cover:

- Empty input and no-heading input return empty vecs.
- Heading-with-no-list returns empty.
- Single and multi-item extraction with correct descriptions.
- Rule ID slug and counter correctness across headings.
- All enforcement keyword variants (`MUST`, `REQUIRED`, `MAY`, `OPTIONAL`).
- Whole-word boundary rejection (`MANDATORY` does not match `MAY`).
- Case-insensitive keyword matching (`must`, `may`).
- Numbered list items are extracted.
- Inline code (`` `backtick` ``) is preserved in descriptions.
- Nested list items are not promoted to rules.
- A smoke test against the real `AGENTS.md` via `include_str!` asserts non-empty
  output with non-empty ids and descriptions.
- `heading_to_slug` edge cases: lowercase, special chars, leading/trailing
  separators, consecutive separators.

All 401 project tests pass after this change.
