# Phase 2: Derived/Enrichment Rules Implementation

## Overview

Phase 2 extends the governance rule loader with a derived enrichment step. When
a repository has no `AGENTS.md` (the pure embedded-defaults case), the loader
now appends a supplemental set of language-specific governance rules tagged with
`RuleSource::Derived`. When an `AGENTS.md` is found and parsed, no derived
enrichment is performed — repository rules take full precedence.

## Motivation

The embedded defaults cover universal security invariants (path traversal, HTTPS
enforcement, plugin naming). They are intentionally language-agnostic. A
repository that has no `AGENTS.md` gets no guidance about language-level best
practices. Phase 2 fills this gap automatically, without requiring every
repository maintainer to write or maintain a governance file.

## Architecture

The governance loader now follows a three-way decision tree:

```text
load_for_config(config)
   |
   +-- rules_path empty? ──────────────────────────────────────────► embedded_defaults
   |                                                                  + derive_from_context
   +-- rules_path non-empty AND file exists? ──────────────────────► load_from_agents_md
   |                                                                  + merge_with_defaults
   |                                                                  (NO enrichment)
   +-- rules_path non-empty AND file absent? ──────────────────────► embedded_defaults
                                                                      + derive_from_context
```

The key invariant: derived enrichment is appended if and only if no repository
`AGENTS.md` file is present. An empty `AGENTS.md` still suppresses enrichment
because the file was found and the repository has taken ownership of its
governance configuration.

## New Module: `governance::enrichment`

`src/governance/enrichment.rs` provides:

```rust
pub fn derive_from_context(language: &str, _frameworks: &[&str]) -> Vec<GovernanceRule>
```

Currently supported languages:

| Language   | Trigger             | Rules generated |
| ---------- | ------------------- | --------------- |
| `"rust"`   | `Cargo.toml` in CWD | 8 rules         |
| all others | (no manifest)       | 0 rules         |

The `_frameworks` parameter is accepted for future specialisation (e.g.
Actix-specific rules, Tokio-specific rules) but is unused in this phase.

### Rust-Specific Derived Rules

All 8 rules carry `source: RuleSource::Derived` so consumers can distinguish
them from embedded or repository-file rules.

| ID                                                    | Enforcement | Description                                      |
| ----------------------------------------------------- | ----------- | ------------------------------------------------ |
| `rust.error_handling.use_result`                      | Required    | Use `Result<T, E>` for all recoverable errors    |
| `rust.error_handling.no_unwrap_without_justification` | Required    | Never `unwrap()` without a justification comment |
| `rust.error_handling.propagate_with_question_mark`    | Required    | Use `?` for propagation; never `let _ =`         |
| `rust.error_handling.use_thiserror`                   | Recommended | Use `thiserror` for custom error types           |
| `rust.code_quality.pass_clippy_clean`                 | Required    | Must pass `cargo clippy -- -D warnings`          |
| `rust.code_quality.format_with_rustfmt`               | Required    | Must pass `cargo fmt --all`                      |
| `rust.documentation.doc_comments_on_public_items`     | Required    | All public items need `///` doc comments         |
| `rust.testing.test_public_functions`                  | Recommended | Test all public functions, target >80% coverage  |

## Language Detection

Language detection is performed by `detect_primary_language()` in
`governance/loader.rs`. It checks the current working directory for well-known
manifest files:

| Manifest checked              | Language returned |
| ----------------------------- | ----------------- |
| `Cargo.toml`                  | `"rust"`          |
| `package.json`                | `"javascript"`    |
| `pyproject.toml` / `setup.py` | `"python"`        |
| (none found)                  | `"unknown"`       |

When the language is `"unknown"` (or any unrecognised value),
`derive_from_context` returns an empty `Vec` and `apply_derived_enrichment`
returns the base rule set unchanged.

## Merge Strategy

`apply_derived_enrichment(base: RuleSet) -> RuleSet` appends only derived rules
whose IDs are not already present in `base`. This prevents any future overlap
with embedded defaults (which use the `governance.*` namespace, while derived
rules use the `rust.*` namespace — a convention that makes ID collisions
structurally impossible at present).

## Rule Source Traceability

The three `RuleSource` variants now map to distinct loader paths:

| `RuleSource`              | Loader path                                  |
| ------------------------- | -------------------------------------------- |
| `Embedded`                | Always — from `embedded_defaults()`          |
| `RepositoryFile { path }` | `load_from_agents_md` when AGENTS.md found   |
| `Derived`                 | `apply_derived_enrichment` when no AGENTS.md |

Consumers can inspect `rule.source` to distinguish rule origins for reporting or
filtering.

## Testing

Phase 2 adds 5 new tests to `governance/loader.rs` and 10 tests to
`governance/enrichment.rs`.

### loader.rs Phase 2 tests

| Test                                                                | Asserts                                                                |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| `test_load_for_config_with_agents_md_has_no_derived_rules`          | Present AGENTS.md blocks derived enrichment (`derived_count == 0`)     |
| `test_load_for_config_without_agents_md_has_derived_rules`          | Empty path triggers enrichment (`derived_count > 0`, `len > embedded`) |
| `test_load_for_config_without_agents_md_embedded_rules_all_present` | All 10 embedded default IDs survive enrichment                         |
| `test_load_for_config_nonexistent_path_has_derived_rules`           | Non-existent file path triggers enrichment                             |
| `test_load_for_config_with_empty_agents_md_has_no_derived_rules`    | Empty but present AGENTS.md blocks enrichment                          |

### enrichment.rs tests

| Test                                                                        | Asserts                                      |
| --------------------------------------------------------------------------- | -------------------------------------------- |
| `test_derive_from_context_with_rust_returns_nonempty_vec`                   | Rust language yields rules                   |
| `test_derive_from_context_with_rust_all_sources_are_derived`                | All sources are `RuleSource::Derived`        |
| `test_derive_from_context_with_rust_contains_required_error_handling_rules` | Required error-handling rules present        |
| `test_derive_from_context_with_rust_contains_recommended_rules`             | Recommended rules present                    |
| `test_derive_from_context_with_unknown_language_returns_empty`              | Unknown language yields empty vec            |
| `test_derive_from_context_with_empty_language_returns_empty`                | Empty string yields empty vec                |
| `test_derive_from_context_language_matching_is_case_insensitive`            | "RUST" and "Rust" produce rules              |
| `test_derive_from_context_frameworks_ignored_for_rust`                      | Non-empty frameworks still yields rust rules |
| `test_derive_from_context_rust_has_expected_rule_count`                     | Exactly 8 rules for Rust                     |
| `test_derive_from_context_rust_ids_are_unique`                              | No duplicate rule IDs                        |

## Files Changed

| File                           | Change type                                                                                                                |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| `src/governance/enrichment.rs` | New — `derive_from_context`, `rust_rules`, 10 tests                                                                        |
| `src/governance/mod.rs`        | Updated — `pub mod enrichment;` added, doc updated                                                                         |
| `src/governance/loader.rs`     | Updated — `detect_primary_language`, `apply_derived_enrichment`, `load_for_config` branching, 5 new tests, 2 updated tests |

## Extending to New Languages

To add a new language (e.g. Python), add a branch in `derive_from_context`:

```rust
"python" => python_rules(),
```

and implement `fn python_rules() -> Vec<GovernanceRule>` following the same
pattern as `rust_rules()`, using `"python.*"` as the ID namespace and
`RuleSource::Derived` as the source for all rules.

No changes to `GovernanceConfig`, `RuleSet`, or any validator are needed.
