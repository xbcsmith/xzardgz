# Governance Loader: AGENTS.md-Based Rule Loading

## Summary

`src/governance/loader.rs` was rewritten to replace YAML-based governance file
loading with `AGENTS.md` Markdown-based loading. The YAML schema types
(`RuleOverride`, `CustomRule`, `RulesFile`) and the `load_from_path` function
were removed. Two new private helpers (`merge_with_defaults`,
`load_from_agents_md`) were added. The public `load_for_config` function was
updated to delegate to the new Markdown path.

## What Changed

### Removed

| Symbol                                                  | Reason                            |
| ------------------------------------------------------- | --------------------------------- |
| `use serde::{Deserialize, Serialize}`                   | No serializable types remain      |
| `RuleOverride` struct                                   | YAML schema no longer used        |
| `CustomRule` struct                                     | YAML schema no longer used        |
| `RulesFile` struct                                      | YAML schema no longer used        |
| `pub fn load_from_path`                                 | Replaced by `load_from_agents_md` |
| Six `load_from_path` tests                              | Covered functionality is gone     |
| `write_temp_yaml` test helper                           | No longer needed                  |
| `test_load_for_config_with_valid_file_merges_correctly` | Used YAML helper                  |

### Added

| Symbol                             | Purpose                                                                |
| ---------------------------------- | ---------------------------------------------------------------------- |
| `use super::parser`                | Access `parser::parse_agents_md`                                       |
| `fn merge_with_defaults`           | Merges parsed rules with embedded defaults; parsed IDs take precedence |
| `fn load_from_agents_md`           | Reads and parses an `AGENTS.md` file, falls back to defaults on error  |
| Updated `load_for_config`          | Delegates to `load_from_agents_md` when a file path is configured      |
| Four new loader tests              | Cover Markdown-based loading scenarios                                 |
| `write_temp_agents_md` test helper | Creates a temporary `AGENTS.md` for tests                              |

### Unchanged

- `RuleSet` struct and all impl methods (`new`, `get`, `by_enforcement`, `has`,
  `len`, `is_empty`) with their doc comments and doc tests.
- `embedded_defaults` function and all its tests.
- `test_load_for_config_with_empty_rules_path_returns_defaults`
- `test_load_for_config_with_nonexistent_file_falls_back_to_defaults`

## Design Decisions

### Silent Fallback

`load_from_agents_md` (and therefore `load_for_config`) never returns `Err`.
Read or parse failures emit a `tracing::warn!` and fall back to embedded
defaults. This keeps the pipeline running even when the repository's `AGENTS.md`
is absent or malformed.

### Merge Semantics

Parsed rules take precedence by ID. An embedded rule is omitted only when a
parsed rule with the same ID already exists in the result. This ensures
security-critical embedded rules cannot be silently dropped by the file contents
while still allowing repository-specific overrides via identical IDs.

### No New Dependencies

The implementation uses only existing project dependencies (`tracing`,
`tempfile` in tests) and the already-present `parser` sub-module.

## Validation

All quality gates passed:

```sh
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features governance::loader   # 22/22 passed
```
