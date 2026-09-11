# Phase 1: Shared AST Layer, Rule Model, Compatibility Gate, and Regex Mode

## Overview

Phase 1 of the SAST scanning tool establishes the foundational components that
all subsequent phases build upon. It delivers a complete rule model, a
compatibility gate, an AST parsing layer backed by tree-sitter, a parse cache,
and a working regex-mode scanner that can execute rules against file contents
with no AST parsing overhead.

This document describes the architecture, key design decisions, and the test
invariants enforced by the Phase 1 implementation.

## Deliverables

| Artifact          | Path                                    | Purpose                                      |
| ----------------- | --------------------------------------- | -------------------------------------------- |
| Error types       | `src/scanner/sast/error.rs`             | `SastError`, `RuleParseError`, `SkipReason`  |
| Engine config     | `src/scanner/sast/config.rs`            | `SastEngineConfig` with defaults             |
| Language enum     | `src/scanner/sast/ast/lang.rs`          | `Language`: Rust, Regex, Generic             |
| Parse cache       | `src/scanner/sast/ast/parse.rs`         | Single-parse-per-path concurrency-safe cache |
| Error density     | `src/scanner/sast/ast/diagnostics.rs`   | `ErrorNodeDensity` accounting                |
| Rule metadata     | `src/scanner/sast/rule/metadata.rs`     | Typed metadata enums and `RuleMetadata`      |
| Rule schema       | `src/scanner/sast/rule/schema.rs`       | Serde types mirroring Semgrep YAML           |
| Rule IR           | `src/scanner/sast/rule/ir.rs`           | `Formula`, `RuleIr`, `CompileOutcome`        |
| Rule parser       | `src/scanner/sast/rule/parse.rs`        | Schema to IR compiler with invariant checks  |
| Compat gate       | `src/scanner/sast/rule/compat.rs`       | Unsupported-construct gating                 |
| Regex engine      | `src/scanner/sast/engine/regex_mode.rs` | AST-free regex scanning                      |
| First-party rules | `src/scanner/sast/rules/`               | 8 Apache-2.0 rules                           |
| Error wiring      | `src/error.rs`                          | `From<SastError> for PipelineError`          |

## Module Architecture

````text
```text
src/scanner/sast/
    mod.rs              -- top-level re-exports
    error.rs            -- SastError, RuleParseError, SkipReason
    config.rs           -- SastEngineConfig
    ast/
        lang.rs         -- Language enum and detection
        parse.rs        -- ParseCache: concurrency-safe, single-parse
        diagnostics.rs  -- ErrorNodeDensity
    rule/
        schema.rs       -- Serde types (full Semgrep YAML surface)
        metadata.rs     -- Typed metadata with StringOrVec deserializer
        ir.rs           -- Formula, RuleIr, CompileOutcome
        parse.rs        -- compile_rule: schema -> CompileOutcome
        compat.rs       -- check_compat: SkipReason gating
    engine/
        regex_mode.rs   -- RegexModeScanner, RegexMatch
    rules/
        crypto/         -- Crypto weakness detection rules (Apache-2.0)
        security/       -- General security rules (Apache-2.0)
````

The engine is entirely network-free. It imports nothing from `src/clients/`,
`src/tools/`, or `src/plugins/`. This invariant is enforced by a grep gate in
CI:

```bash
grep -rn 'use crate::clients' src/scanner/sast/
```

## Language Support

Phase 1 supports three language modes:

| Mode    | Enum variant        | AST                    | Description                          |
| ------- | ------------------- | ---------------------- | ------------------------------------ |
| Rust    | `Language::Rust`    | Yes (tree-sitter-rust) | Full structural matching in Phase 2+ |
| Regex   | `Language::Regex`   | No                     | `pattern-regex` over raw bytes       |
| Generic | `Language::Generic` | No                     | Same as regex, language-agnostic     |

Language detection follows this priority order:

1. File extension (`rs` -> Rust)
2. Shebang line for extensionless files (limited; Generic default)
3. Default: Generic

The mapping between Semgrep `languages:` list values and the `Language` enum is
handled by `Language::from_semgrep_name`, which is case-insensitive.

## Rule Model

### Schema Layer (`rule/schema.rs`)

The schema layer models the complete Semgrep YAML rule surface using serde
types. It includes ALL constructs, even those the engine cannot yet execute
(taint mode fields, deep expressions, etc.), so the compatibility gate can
inspect them with precision. Unknown top-level fields in the YAML `metadata`
block are preserved in `RuleMetadata::extra` using `#[serde(flatten)]`.

### Intermediate Representation (`rule/ir.rs`)

The IR is the compiled, validated form of a rule. It is only created for rules
that pass both the compatibility gate and all structural invariant checks. The
core type is `Formula`:

```rust
Formula::Leaf(Leaf)
Formula::And { conjuncts, negations, conditions, focus }
Formula::Or(Vec<Formula>)
Formula::Inside(Box<Formula>)
```

This matches the range-set algebra described in the implementation plan:

- `And` intersects positive conjuncts, then subtracts negations, then applies
  conditions and focus.
- `Or` computes the union of its children.
- `Inside` restricts matches to nodes contained within the sub-formula's match.
- `Leaf` is a primitive: either a structural pattern string or a regex pattern.

### Compatibility Gate (`rule/compat.rs`)

The gate inspects a `RuleSchema` and returns `Vec<SkipReason>`. A non-empty list
causes the entire rule to be skipped -- never partially evaluated. Gated
constructs:

| Construct                             | `SkipReason`           |
| ------------------------------------- | ---------------------- |
| `mode: taint`                         | `TaintMode`            |
| `pattern-propagators`                 | `PatternPropagators`   |
| `metavariable-analysis`               | `MetavariableAnalysis` |
| No supported language in `languages:` | `NoSupportedLanguage`  |

Additional constructs (`JoinMode`, `ExtractMode`, `StepMode`, `DeepExpression`,
`TypedMetavariable`, `FixRegex`) are defined in `SkipReason` for use when the
corresponding detection logic is added. The gate currently passes through rules
containing those constructs unless another gating condition triggers.

### Rule Parser (`rule/parse.rs`)

The parser enforces these structural invariants before compiling to IR:

1. Rule `id` matches `^[a-zA-Z0-9._/-]+$`.
2. Exactly one formula root (`pattern`, `patterns`, `pattern-either`, or
   `pattern-regex`) is present at the rule level.
3. A `patterns:` list must have at least one positive term (`pattern`,
   `pattern-inside`, `pattern-either`, or `pattern-regex` in any term).
4. Each `PatternTerm` is dispatched to the correct bucket:
   - Positive conjuncts (`pattern`, `pattern-inside`, `pattern-either`,
     `pattern-regex`)
   - Negative conjuncts (`pattern-not`, `pattern-not-inside`)
   - Conditions (`metavariable-regex`, `metavariable-pattern`,
     `metavariable-comparison`)
   - Focus (`focus-metavariable`)

Violations produce `Err(RuleParseError::Invariant { ... })`. Unsupported
constructs produce `Ok(CompileOutcome::Skipped { ... })`.

## Parse Cache

`ParseCache` provides a single-parse guarantee: each `(PathBuf, Language)` pair
is parsed at most once per scan session, regardless of how many rules reference
it. The cache is backed by a `Mutex<HashMap>` so it is safe to use from multiple
rayon worker threads (added in Phase 4).

Files larger than `SastEngineConfig::max_file_bytes` (default 5 MiB) are skipped
without reading; `get_or_parse` returns `Ok(None)` for them. The `parse_count()`
method enables testing the single-parse invariant.

## Error Node Density

Tree-sitter silently produces partial parse trees for syntactically invalid
source files by inserting `ERROR` and `MISSING` nodes. `ErrorNodeDensity` counts
these nodes via a stack-based traversal (not recursion, to avoid stack overflow
on deeply nested trees) and exposes the ratio and a threshold-based
`is_degraded` predicate. A file with non-zero density is retained rather than
discarded; later phases may decide to lower the confidence of matches from
degraded files.

## Regex Mode

`RegexModeScanner` implements pattern-regex scanning for `languages: [regex]`
and `languages: [generic]` rules. No AST parsing is involved.

Key properties:

- All regexes compiled with `RegexBuilder::size_limit(10_485_760)` and
  `dfa_size_limit(10_485_760)` (10 MiB each) to bound ReDoS risk.
- Named capture groups become `$NAME` metavariables in
  `RegexMatch::metavariables`.
- Line numbers are computed via binary search over a precomputed line-start
  offset table (O(log n) per match).
- Invalid UTF-8 content is handled lossily; matches within replacement
  characters are excluded.
- The scanner stops after `max_matches` results to bound memory usage.

## First-Party Rules

Eight first-party rules are bundled under `src/scanner/sast/rules/` (all
Apache-2.0 licensed, authored in-house, NOT copied from semgrep-rules):

| Rule ID                             | Path                                    | Mode  | CWE              |
| ----------------------------------- | --------------------------------------- | ----- | ---------------- |
| `rust-weak-rsa-key`                 | `crypto/rust_weak_rsa_key.yaml`         | rust  | CWE-326          |
| `rust-md5-usage`                    | `crypto/rust_md5_usage.yaml`            | rust  | CWE-327, CWE-328 |
| `rust-sha1-usage`                   | `crypto/rust_sha1_usage.yaml`           | rust  | CWE-327, CWE-328 |
| `rust-des-cipher`                   | `crypto/rust_des_cipher.yaml`           | rust  | CWE-327          |
| `rust-rc4-cipher`                   | `crypto/rust_rc4_cipher.yaml`           | rust  | CWE-327          |
| `rust-hardcoded-password-field`     | `security/rust_hardcoded_password.yaml` | rust  | CWE-798          |
| `rust-unsafe-ffi-call`              | `security/rust_unsafe_ffi.yaml`         | rust  | CWE-676          |
| `regex-hardcoded-secret-assignment` | `security/regex_hardcoded_secret.yaml`  | regex | CWE-798          |

These rules serve as Phase 2+ targets for structural matching evaluation and are
the first inputs to the pattern compiler added in Phase 2.

## Error Handling and Pipeline Integration

`SastError` maps to `PipelineError::Scanner` via
`From<SastError> for PipelineError` in `src/error.rs`, following the established
pattern for all legacy sub-error types.

The engine never panics on rule or file errors: all failures are propagated as
`Result<_, SastError>`. The compatibility gate returns skipped rules rather than
errors, and skipped rules are logged without aborting the scan.

## Quality Gates

All four mandatory gates pass:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## Boundary Invariant

The `src/scanner/sast/` module must never import from `src/clients/`,
`src/tools/`, or `src/plugins/`. This is the network-free / offline-only
contract stated in Decision 1 of the implementation plan. The engine only reads
files already on disk; ruleset fetching is deferred to `src/clients/rulesets/`
(Phase 8).

## Relation to Other Phases

| Phase   | What builds on Phase 1                                                      |
| ------- | --------------------------------------------------------------------------- |
| Phase 2 | Core matching engine: pattern compiler, range algebra, formula evaluator    |
| Phase 3 | Metavariable conditions and focus: evaluates `Condition::MetavarRegex` etc. |
| Phase 4 | Parallel execution: rayon worker pool, prefiltering, file targeting         |
| Phase 5 | `SastMatch` output model, SARIF and CycloneDX projections                   |
| Phase 6 | Builtin ruleset resolution, CLI subcommand                                  |
| Phase 8 | Ruleset acquisition from git/HTTPS/OCI                                      |
