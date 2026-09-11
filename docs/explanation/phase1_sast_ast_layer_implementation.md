# Phase 1 SAST AST Layer Implementation

## Overview

This document describes the Phase 1 implementation of the SAST scanning engine's
AST layer. The implementation covers the six files in the write scope plus the
necessary module wiring needed to make them compile.

## Files Written

### `src/scanner/sast/error.rs`

Defines the error hierarchy for the SAST engine. Three public enums are
declared:

- `SastError` - top-level error used by all public SAST functions. Variants
  cover rule file reading (`RuleRead`), rule parsing (`RuleParse`), source file
  reading (`FileRead`), oversized files (`FileTooLarge`), regex compilation
  (`RegexCompile`), scan timeouts (`ScanTimeout`), and internal errors
  (`Internal`). The `RuleParse` variant carries a `#[source]` chain to
  `RuleParseError`.

- `RuleParseError` - structured error for rule file parsing failures. Variants
  distinguish YAML deserialization errors (`Yaml`), schema violations
  (`Schema`), invalid rule identifiers (`InvalidId`), and structural invariant
  violations (`Invariant`).

- `SkipReason` - reason a rule was rejected by the compatibility gate before
  evaluation. Variants cover: `TaintMode`, `JoinMode`, `ExtractMode`,
  `StepMode`, `FixRegex`, `PatternPropagators`, `DeepExpression`,
  `TypedMetavariable`, `MetavariableAnalysis`, and
  `NoSupportedLanguage(Vec<String>)`. Rules that trigger any skip reason are
  excluded entirely rather than partially evaluated.

### `src/scanner/sast/config.rs`

Defines `SastEngineConfig`, the single source of truth for all SAST engine
tuning knobs:

| Field                  | Default | Purpose                                    |
| ---------------------- | ------- | ------------------------------------------ |
| `max_file_bytes`       | 5 MiB   | Files larger than this are skipped         |
| `rule_timeout_ms`      | 5000    | Per-rule timeout in milliseconds           |
| `max_matches_per_file` | 100     | Cap on matches per file per rule           |
| `jobs`                 | 0       | Worker threads (0 = available parallelism) |

The struct implements `Default`, provides a `new()` constructor, and is fully
serializable via `serde`. Private serde default helper functions populate fields
when they are absent from YAML input.

### `src/scanner/sast/ast/lang.rs`

Defines the `Language` enum with three variants: `Rust`, `Regex`, and `Generic`.
Only `Rust` requires AST parsing via tree-sitter.

Detection methods:

- `from_extension(ext)` - case-insensitive extension matching. Returns
  `Some(Rust)` for `"rs"`, `None` for unrecognised extensions.
- `from_path(path)` - tries extension first. For extensionless files, reads the
  first 64 bytes to detect shebangs (`#!/usr/bin/env rust` or
  `#!/usr/bin/rust`). Defaults to `Generic` on any mismatch or IO error.
- `to_ast_grep()` - maps `Rust` to `Some(SupportLang::Rust)`, others to `None`.
- `requires_ast()` - delegate to `to_ast_grep().is_some()`.
- `from_semgrep_name(name)` - case-insensitive parse of Semgrep `languages:`
  names.

### `src/scanner/sast/ast/diagnostics.rs`

Defines `ErrorNodeDensity`, which reports how many `ERROR` and `MISSING` nodes
tree-sitter produced in a parse tree. A clean parse has `error_nodes == 0`; a
pathologically corrupted input approaches `ratio() == 1.0`.

The `from_root` method uses an explicit stack-based traversal to avoid
call-stack overflow on deeply nested source files. Each node's `is_error()` and
`is_missing()` flags are checked; children are enqueued via
`stack.extend(node.children())`.

`is_degraded(threshold)` returns `true` when `ratio() > threshold`, letting
callers skip pattern matching for files whose parse trees are too corrupted to
be reliable.

### `src/scanner/sast/ast/parse.rs`

Defines `ParseCache`, a concurrency-safe parse cache keyed on
`(PathBuf, Language)`. Each cache entry is an `Arc<CachedRoot>`, which bundles
the `AstGrep<StrDoc<SupportLang>>` root together with its pre-computed
`ErrorNodeDensity`.

The `get_or_parse` method:

1. Returns early `Ok(None)` for non-AST languages (`Regex`, `Generic`).
2. Checks the cache under a `Mutex` lock; returns immediately on hit.
3. Checks file size; returns `Ok(None)` without caching if the file exceeds
   `max_file_bytes`.
4. Reads the file, calls `sg_lang.ast_grep(&src)` to parse, computes
   `ErrorNodeDensity::from_root`, wraps in `Arc<CachedRoot>`.
5. Re-acquires the lock, uses `HashMap::entry(...).or_insert_with(...)` to avoid
   double insertion in the face of racing threads, increments `parse_count` only
   on a fresh insertion.

`AstGrep<SgDoc>` does not implement `Debug`, so a manual `Debug` impl is
provided for `CachedRoot` that shows only the `density` field.

### `src/scanner/sast/ast/mod.rs`

Declares and re-exports the three submodules with public names:

```rust
pub use diagnostics::ErrorNodeDensity;
pub use lang::Language;
pub use parse::{CachedRoot, ParseCache};
```

## Module Wiring

The `src/scanner/sast/mod.rs` was updated to include `pub mod ast;` and
`pub mod config;` alongside the pre-existing `pub mod engine;`,
`pub mod error;`, and `pub mod rule;`. This allows the crate to compile the new
modules.

`src/scanner/mod.rs` already had `pub mod sast;` declared by a previous agent.

## Design Decisions

### Stack-based traversal in `ErrorNodeDensity::from_root`

Rust's default stack size can handle moderate AST depths, but adversarial or
auto-generated source files may produce trees hundreds of thousands of nodes
deep. The iterative stack-based approach is O(n) in time and space (where n is
the node count) and avoids any risk of stack overflow.

### `ParseCache` double-checked locking

The cache lock is released between the cache-miss check and the file IO to avoid
holding a `Mutex` during blocking disk reads. A second `HashMap::entry` call
under the lock prevents duplicate insertions from racing threads. Only the
thread that wins the `or_insert_with` race increments `parse_count`, keeping the
counter accurate.

### `Language::from_path` fallibility on IO

The shebang-detection path silently degrades to `Generic` on any IO error. This
preserves the function's infallibility contract while still attempting the
best-effort detection.

## Validation

All four AGENTS.md quality gates pass:

```text
cargo fmt --all                                      # ok
cargo check --all-targets --all-features             # ok
cargo clippy --all-targets --all-features -D warnings # ok
cargo test --all-features --lib                      # 1925 passed, 0 failed
```

## Test Coverage

| Module        | Tests                                                                                                            |
| ------------- | ---------------------------------------------------------------------------------------------------------------- |
| `error`       | Display strings for all variants; source chain accessibility                                                     |
| `config`      | All defaults; `new()` == `default()`; YAML round-trip; partial YAML                                              |
| `lang`        | Extension detection (lower/upper/unknown); path detection; shebang; to_ast_grep; requires_ast; from_semgrep_name |
| `diagnostics` | Clean parse; degraded parse; ratio edge cases; is_degraded boundaries                                            |
| `parse`       | Successful parse; single-parse invariant; oversized file; Generic/Regex early return; IO error                   |
