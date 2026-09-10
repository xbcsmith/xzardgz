# Phase 0 SAST Implementation

## Overview

Phase 0 establishes the dependency foundation and test infrastructure for the
Static Application Security Testing (SAST) pipeline in xzardgz. No production
SAST logic is introduced here; the goal is to ensure all required crates are
available in `Cargo.toml` and that the test harness can gate corpus-dependent
tests without failing on machines that do not have the corpus checked out.

## Cargo Changes

### New Runtime Dependencies

| Crate               | Version                 | Purpose                                                  |
| ------------------- | ----------------------- | -------------------------------------------------------- |
| `ast-grep-core`     | 0.45                    | AST pattern matching engine for rule evaluation          |
| `ast-grep-language` | 0.45 (tree-sitter-rust) | Rust language support for ast-grep                       |
| `aho-corasick`      | 1                       | Multi-pattern string search for fast identifier scanning |
| `blake2`            | 0.10                    | BLAKE2 cryptographic hashing for file fingerprinting     |
| `rayon`             | 1                       | Data-parallel iterators for corpus-scale rule evaluation |
| `globset`           | 0.4                     | Glob pattern compilation used in rule path filters       |
| `dirs`              | 5                       | Platform-appropriate user directory resolution           |

### New Dev Dependencies

| Crate        | Version | Purpose                                             |
| ------------ | ------- | --------------------------------------------------- |
| `jsonschema` | 0.29    | JSON Schema validation used in Phase 0 schema tests |

### New Feature Flag

`sast-integration-tests` is an empty feature flag that gates corpus-scale
integration tests. Tests annotated with
`#[cfg(feature = "sast-integration-tests")]` are skipped in the default
`cargo test` run and must be explicitly opted into:

```bash
cargo test --features sast-integration-tests
```

The `rust-version = "1.88.0"` field was added to the `[package]` table to
declare the minimum supported Rust toolchain.

## Test Infrastructure

### `tests/helpers/sast_corpus.rs`

Provides `semgrep_rules_dir() -> Option<PathBuf>`, the single shared entry point
for corpus-gated tests. The function:

1. Reads `XZARDGZ_SEMGREP_RULES_DIR` from the environment.
2. Checks that the value, if present, resolves to an existing directory.
3. Prints an explicit skip message and returns `None` if either condition fails.
4. Returns `Some(path)` when the corpus is available.

Callers are expected to pattern-match on the return value and return early
(skipping, not failing) when `None` is returned. This design keeps CI green on
machines without the corpus while still enforcing correctness when the corpus is
present.

### `tests/sast_phase0.rs`

The Phase 0 integration test binary. It pulls in `sast_corpus` via `#[path]` and
exercises the following:

- `schema_tests::test_cyclonedx_schema_is_valid_json_schema` - parses
  `testdata/cyclonedx/bom-1.7.schema.json` with `serde_json` and compiles it
  with the `jsonschema` crate, asserting that the schema is well-formed.
- `schema_tests::test_sarif_schema_is_valid_json_schema` - the same check for
  `testdata/sarif/sarif-2.1.0.schema.json`.

## Vendored Schema Files

Both schemas are stored verbatim from their canonical upstream sources:

| File                                     | Source                             |
| ---------------------------------------- | ---------------------------------- |
| `testdata/cyclonedx/bom-1.7.schema.json` | CycloneDX specification repository |
| `testdata/sarif/sarif-2.1.0.schema.json` | Microsoft SARIF SDK repository     |

The files are included in the binary at compile time via `include_str!`, which
means a missing or malformed file causes a compile error rather than a silent
runtime failure.

## Environment Variables

| Variable                    | Required | Description                                                                                                                                    |
| --------------------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `XZARDGZ_SEMGREP_RULES_DIR` | No       | Path to a local checkout of the semgrep-rules corpus. When unset or pointing at a non-existent directory, corpus tests are skipped gracefully. |
