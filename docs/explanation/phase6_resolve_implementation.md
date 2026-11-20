# Phase 6 Ruleset Resolve Implementation

## Overview

`src/scanner/sast/rule/resolve.rs` implements the four-level ruleset search path
and rule-id namespacing for the SAST scanning pipeline.

## Public API

### Types

**`NamespacedRule`** — A compiled rule with its ruleset namespace applied.
Contains:

- `ruleset_id: String` — identifier for the owning ruleset (e.g.
  `"builtin:crypto"`)
- `namespaced_id: String` — fully-qualified id `"<ruleset_id>::<base_id>"`
- `rule: RuleIr` — the compiled rule; `rule.id` holds the base id unchanged

**`ResolveConfig`** — Configuration built from CLI flags before calling
`resolve_rules`. Contains:

- `cli_rulesets: Vec<String>` — ruleset ids or filesystem paths from `--ruleset`
- `cli_ruleset_paths: Vec<PathBuf>` — extra search directories from
  `--ruleset-path`

### Functions

| Function                               | Description                             |
| -------------------------------------- | --------------------------------------- |
| `namespace_rule(ruleset_id, rule)`     | Wraps a `RuleIr` in a `NamespacedRule`  |
| `load_rules_from_dir(dir, ruleset_id)` | Walks a directory, parses `.yaml` files |
| `load_builtin_ruleset(ruleset_id)`     | Loads from embedded catalogue by id     |
| `resolve_rules(config)`                | Orchestrates the four-level search      |

## Four-Level Search Path

Rules are resolved in this priority order:

1. **CLI rulesets** (`config.cli_rulesets`): entries starting with `"builtin:"`
   are loaded from the embedded catalogue; all others are treated as filesystem
   paths.
2. **CLI ruleset paths** (`config.cli_ruleset_paths`): directories walked for
   `.yaml` files with the directory path used as the ruleset id.
3. **Environment variable** (`XZARDGZ_SAST_RULESET_PATHS`): colon-separated
   (Unix) or semicolon-separated (Windows) list of directories.
4. **User config directory** (`~/.config/xzardgz/rulesets/`): optional user
   rulesets directory.
5. **Embedded defaults** (`"builtin:crypto"` and `"builtin:security"`): loaded
   only when `cli_rulesets` is empty and levels 3-4 found no YAML files.

## Namespacing

Every rule id is namespaced as `"<ruleset_id>::<base_id>"`. For example:

- Base id: `"rust-md5-usage"`
- Ruleset id: `"builtin:crypto"`
- Namespaced id: `"builtin:crypto::rust-md5-usage"`

The `rule.id` field on the `RuleIr` within a `NamespacedRule` always holds the
original base id, unchanged.

## Error Handling

- Missing directories: logged with `tracing::warn!`, return `Ok(vec![])`.
- File read errors: propagated as `SastError::RuleRead`.
- Rule parse errors: propagated as `SastError::RuleParse`.
- Skipped rules (unsupported constructs): logged with `tracing::warn!`, omitted
  from results.

## Deduplication

No deduplication is performed. If two rulesets define a rule with the same base
id, they produce two entries with distinct `namespaced_id` values.

## Related Modules

- `crate::scanner::sast::embedded` — provides `embedded_rules()` and
  `ruleset_id_from_key()`
- `crate::scanner::sast::rule::parse` — provides `parse_and_compile()`
- `crate::scanner::sast::error` — `SastError` variants used in return types

## Supporting Changes

Two pre-existing Phase 6 stubs were completed to allow compilation:

- `src/commands/sast.rs` — created with a minimal `execute(SastCommands)`
  dispatcher (the `SastCommands::Scan` arm returns `Ok(())` as a placeholder).
- `src/config.rs` — removed duplicate `sast` and `sast_scan` field declarations
  that were causing a compile error.

A `.cargo/config.toml` was added with a macOS-specific linker flag
(`-undefined dynamic_lookup`) to work around an `rdkafka-sys` 4.9.0 / OpenSSL
3.6+ symbol incompatibility (`_BIO_eof`, `_OPENSSL_sk_set_cmp_thunks`,
`_OPENSSL_sk_set_copy_thunks`). This flag should be removed once `rdkafka-sys`
is updated to support OpenSSL 4.x.

## Test Coverage

Ten unit tests cover all public functions and corner cases:

| Test                                                                   | What it verifies                  |
| ---------------------------------------------------------------------- | --------------------------------- |
| `test_namespace_rule_produces_correct_namespaced_id`                   | Correct `namespaced_id` formation |
| `test_load_builtin_ruleset_crypto_returns_at_least_15_rules`           | Crypto ruleset count              |
| `test_load_builtin_ruleset_security_returns_at_least_10_rules`         | Security ruleset count            |
| `test_load_builtin_ruleset_unknown_id_returns_empty_vec`               | Graceful unknown id handling      |
| `test_load_rules_from_dir_missing_directory_returns_empty`             | Missing dir returns `Ok(vec![])`  |
| `test_load_rules_from_dir_with_valid_yaml_returns_rules`               | End-to-end directory loading      |
| `test_resolve_rules_with_empty_config_loads_default_builtins`          | Default fallback (>= 25 rules)    |
| `test_resolve_rules_namespacing_produces_distinct_ids_across_rulesets` | No id collisions                  |
| `test_search_path_precedence_cli_ruleset_path_takes_priority`          | Custom path included in results   |
| `test_missing_ruleset_path_does_not_fail_run`                          | Non-fatal missing path handling   |
