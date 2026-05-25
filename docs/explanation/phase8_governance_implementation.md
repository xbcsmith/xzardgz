# Phase 8 Governance System Implementation

## Overview

Phase 8 adds a three-layer governance system to the XZardgz pipeline. The system
validates branch names, file paths, plugin identifiers, event types, provider
endpoints, and content against a set of configurable rules before any
destructive or outbound operation is performed.

## Architecture

The implementation follows a three-layer design that maps directly to the three
source modules inside `src/governance/`.

### Layer 1: Rules (`src/governance/rules.rs`)

The rules layer defines the core data model:

- `EnforcementLevel` - `Required`, `Recommended`, or `Optional`. Determines
  whether a violation stops execution, produces a warning, or produces an
  informational entry.
- `RuleSource` - `Embedded`, `RepositoryFile { path }`, or `Derived`. Records
  where each rule came from.
- `GovernanceRule` - A single policy identified by a reverse-DNS ID string.
- `GovernanceViolation` - A concrete instance of a rule being violated, carrying
  the offending value and a human-readable message.
- `GovernanceResult` - The aggregate of zero or more violations from one or more
  checks. Provides helpers such as `has_blocking_violations()`, `merge()`, and
  `to_diagnostics()`.

`GovernanceResult::to_diagnostics()` converts results to the pipeline's
`Diagnostics` type:

- `Required` and `Recommended` violations become `DiagnosticLevel::Warning`.
- `Optional` violations become `DiagnosticLevel::Info`.
- All entries use `DiagnosticCategory::Governance` and include the rule ID in
  the message prefix.

### Layer 2: Loader (`src/governance/loader.rs`)

The loader layer resolves the active `RuleSet` used by the validator.

`embedded_defaults()` returns ten hardcoded rules compiled into the binary:

| Rule ID                                 | Enforcement |
| --------------------------------------- | ----------- |
| `governance.branch.safe_pattern`        | Recommended |
| `governance.path.no_traversal`          | Required    |
| `governance.path.no_null`               | Required    |
| `governance.plugin.valid_name`          | Required    |
| `governance.event.known_type`           | Required    |
| `governance.endpoint.require_https`     | Required    |
| `governance.content.no_secrets_pattern` | Recommended |
| `governance.workspace.no_traversal`     | Required    |
| `governance.output.no_traversal`        | Required    |
| `governance.report.no_traversal`        | Required    |

`load_from_path(path)` reads a YAML file and merges it with the embedded
defaults. The file schema (`RulesFile`) supports:

- `overrides` - A list of `RuleOverride` entries that can disable rules by ID
  (`disabled: true`) or change their enforcement level.
- `additional_rules` - A list of `CustomRule` entries that are appended with
  `RuleSource::Derived`.

`load_for_config(config)` implements the resolution policy used by
`GovernanceChecker`: if `rules_path` is non-empty and the file exists it is
loaded and merged; otherwise embedded defaults are returned silently. This means
repositories that do not have a governance file work without any configuration
change.

### Layer 3: Validator and Checker

`GovernanceValidator` (`src/governance/validator.rs`) applies the `RuleSet` to
individual inputs. Each `validate_*` method returns a `GovernanceResult`. All
pattern matching is implemented with plain Rust string operations — no external
regex crate is used.

Validation rules:

- **Branch names** - Must be `main`, `master`, or `develop`, or start with an
  approved prefix (`feature/`, `fix/`, `hotfix/`, `release/`, `chore/`,
  `refactor/`, `test/`, `ci/`, `docs/`) followed by a non-empty suffix.
- **File / output / report / workspace paths** - Each path is checked for `..`
  components after normalising backslashes to forward slashes. File paths are
  additionally checked for null bytes.
- **Plugin names** - Must begin with a lowercase ASCII letter and contain only
  lowercase ASCII letters, ASCII digits, and underscores.
- **Event types** - Must be one of: `push`, `pull_request`, `issue`, `release`,
  `schedule`, `workflow_dispatch`, `tag`, `commit`, `merge`.
- **Provider endpoints** - Must begin with `https://` (case-insensitive). Empty
  strings are skipped.
- **Content safety** - Checked for assignment patterns (`password=`, `api_key=`,
  `secret=`, etc., case-insensitively) and exact-case PEM private key headers.
  Offending content is replaced with `"(content redacted)"` in the violation
  value to prevent secret material from appearing in logs.

`GovernanceChecker` (`src/governance/mod.rs`) wraps `GovernanceValidator` with a
`GovernanceConfig` and enforces two flags:

- `enabled` - When `false` every `check_*` call returns empty diagnostics
  immediately without touching the validator.
- `fail_on_violation` - When `true` and a `Required` violation is present,
  `handle_result` collects all blocking violation messages, joins them with
  `"; "`, and returns `Err(PipelineError::Governance(...))`. Non-blocking
  violations (`Recommended` / `Optional`) are always converted to diagnostics
  regardless of this flag.

`check_workflow_inputs` merges results from all input categories before the
single `handle_result` call so the caller receives the complete picture in one
error or diagnostic set.

## Files Created or Modified

| Path                          | Change                                                        |
| ----------------------------- | ------------------------------------------------------------- |
| `src/governance/rules.rs`     | New — data model                                              |
| `src/governance/loader.rs`    | New — rule loading                                            |
| `src/governance/validator.rs` | New — validation logic                                        |
| `src/governance/mod.rs`       | New — GovernanceChecker and re-exports                        |
| `src/governance.rs`           | Deleted — stub replaced by the directory module               |
| `src/diagnostics.rs`          | Modified — added `Governance` variant to `DiagnosticCategory` |

## Test Coverage

Each module has a `#[cfg(test)]` section with tests named following the
`test_<function>_<condition>_<expected>` convention:

- `rules.rs` - 30 tests covering all methods and edge cases
- `loader.rs` - 18 tests including tempfile-based round-trip tests for
  `load_from_path`
- `validator.rs` - 55 tests covering all validators and private helpers
- `mod.rs` - 30 tests covering `GovernanceChecker` including all scenarios
  specified in the task

All 509 unit tests and 132 doc tests pass. `cargo fmt`, `cargo check`, and
`cargo clippy -D warnings` all exit with no errors or warnings.

## Design Decisions

### No regex crate

All pattern matching uses `str::contains`, `str::starts_with`,
`str::strip_prefix`, and iterator combinators on split slices. This satisfies
the project constraint and avoids compilation overhead.

### Rule presence is optional in validators

Every `validate_*` method uses `rules.get(id)` and skips the check when the rule
is absent. This means disabling a rule in the repository governance file
completely removes the associated check without touching the validator code.

### Recommended violations never block

`handle_result` only gates on `Required` violations. The branch and content
rules are `Recommended` by default, so they produce warning diagnostics even
when `fail_on_violation` is `true`. Repository operators can promote either rule
to `Required` via an enforcement override in the governance file if stricter
behaviour is needed.

### load_for_config falls back silently

A missing or empty `rules_path` does not return an error. This ensures that
repositories without a governance file continue to work, while repositories that
do provide one benefit from customisation.
