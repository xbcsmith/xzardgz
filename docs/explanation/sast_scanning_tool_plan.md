# SAST Scanning Tool Implementation Plan

## Overview

xzardgz's existing static analysis is line-based keyword, filename, and
dependency matching via `src/scanner/patterns.rs::PatternRegistry` and
`PatternSet`, augmented by a single AI completion call in
`src/plugins/security_review/`. Neither path can express a structural code
pattern: a rule that matches an actual parse tree node, captures metavariable
bindings, and combines positive and negative structural assertions. This plan
adds a first-party, from-scratch Rust static analysis engine to xzardgz, using
`ast-grep-core` as the tree-sitter parsing and pattern-matching substrate.

The engine evaluates a Semgrep-dialect YAML rule language against local source
trees. It is deliberately not a Semgrep wrapper, FFI binding, or full
rule-syntax-compatible reimplementation. Semgrep's complete DSL (taint mode,
deep expressions, typed metavariables, join mode) is a multi-year engineering
effort to replicate fully. This plan scopes a genuinely useful, constrained
subset first and treats broader coverage as explicit future work rather than
committing to unverified scope up front. Taint mode is deferred entirely to a
dedicated follow-on plan; see the Deferred to Separate Plans section.

The engine exposes two integration surfaces: a `SastTool` implementing
`ToolExecutor` for agent tool-calling from an `AgentSession`, and a direct
synchronous call path consumed by `src/plugins/security_review/plugin.rs` as a
deterministic pre-AI pass. A new `src/plugins/sast_scan/` plugin implements
`WorkflowPlugin` for standalone pipeline execution with a full `ScoringInput`
implementation and Markdown report. Ruleset acquisition over the network is
isolated in `src/clients/rulesets/`, respecting the existing clients
component-boundary contract: `src/clients/` must not call `src/scanner/`, and
`src/tools/` must not call `src/clients/` directly.

**v1 languages**: Rust (the project's own implementation language, giving the
engine real rules to exercise against xzardgz itself immediately), plus AST-free
`regex` and `generic` modes. The `regex`/`generic` modes cover roughly 12% of
the public semgrep-rules corpus with no parser overhead. Additional languages
are deferred to a follow-on expansion plan.

**Taint mode is explicitly out of scope for this plan.** It is deferred to
`docs/explanation/sast_taint_mode_implementation_plan.md`. See the Deferred to
Separate Plans section for details.

## Current State Analysis

### Existing Infrastructure

| Component                                           | Location                                                    | Relevance                                                                                                             |
| --------------------------------------------------- | ----------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `PatternRegistry` / `PatternSet`                    | `src/scanner/patterns.rs`                                   | Line-based keyword/filename detection; complementary signal, not replaced by this plan                                |
| `PluginContentScanner`                              | `src/scanner/preselect.rs`                                  | File-selection and parallel-scanning pass; SAST engine adds its own prefilter on top                                  |
| `ScanFinding`, `FindingSeverity`                    | `src/scanner/findings.rs`                                   | Existing finding types; SAST introduces `SastMatch` as a neutral engine output model                                  |
| `ScoringInput`, `ScoringSignal`, `ConfidenceScorer` | `src/scanner/scoring.rs`                                    | Baseline-fold AI-blend scoring; `SastScanResult` must implement `ScoringInput` per `src/plugins/AGENTS.md`            |
| `ToolExecutor` trait                                | `src/tools/mod.rs`                                          | `async fn execute(&self, params: Value) -> Result<ToolResult>` and `fn tool_definition()`; `SastTool` implements this |
| Tool registry                                       | `src/tools/registry.rs`                                     | Self-registers `SastTool` on startup                                                                                  |
| `WorkflowPlugin` trait                              | `src/plugins/trait_def.rs`                                  | `SastScanPlugin` implements this for standalone pipeline use                                                          |
| `SecurityReviewConfig`                              | `src/config.rs` and `src/plugins/security_review/config.rs` | Extended with SAST-enabling fields in Phase 7                                                                         |
| `Config` struct                                     | `src/config.rs`                                             | Gains `sast: SastEngineConfig` and `sast_scan: SastScanConfig` fields                                                 |
| Clients boundary contract                           | `src/clients/mod.rs`                                        | Must NOT call `scanner`; Must NOT be called from `tools/`; `src/clients/rulesets/` must respect this boundary         |
| `PipelineError`, `Result`                           | `src/error.rs`                                              | Unified flat error enum; `SastError` maps to `PipelineError::Scanner` via a `From` impl                               |

### Identified Issues

1. No AST parsing infrastructure exists in xzardgz. There is no tree-sitter
   dependency, no per-language parser abstraction, and no structural pattern
   matcher of any kind.
2. No rule-loading mechanism exists for any YAML rule DSL. Rules must be
   discovered, parsed, validated, and compiled into an IR before any scanning
   can begin.
3. `security_review`'s only non-pattern-registry finding source is a single AI
   completion call. There is no deterministic, rule-driven static analysis
   source, which means findings have no ground-truth baseline independent of the
   AI provider.
4. The `ToolExecutor` registry has no SAST-capable tool, so an `AgentSession`
   cannot request structural code scanning on demand.
5. No ruleset supply mechanism exists. There is no search path, no cache, and no
   licence policy. The engine must ship usable first-party rules and treat any
   external ruleset as untrusted, optional input.

## Research Summary

The reference plans (`doberman_sast_scanning_implementation_plan.md` and
`reposcan_sast_scanning_client_plan.md`) carried out substantial research
against local checkouts of `opengrep`, `ast-grep` (v0.45.3), and the
`semgrep-rules` corpus (2,095 rule files). The headline findings that drive
architectural decisions in this plan:

- **`patterns:` / `pattern-either:` / `pattern-not:` are range-set algebra**,
  not node matching. `And` is range intersection with metavariable unification;
  `Or` is union; `Not` subtracts from the enclosing `And`'s positive conjuncts;
  `focus-metavariable` narrows surviving ranges after the whole formula
  resolves. This is measured from `Match_search_mode.ml` and
  `Range_with_metavars.ml` in the opengrep source.
- **Prefiltering is mandatory at this rule count.** Semgrep's `Analyze_rule.ml`
  derives a `Pred(Idents | Regexp) | And | Or` formula per rule (deliberately no
  `Not`) and evaluates it against raw file bytes before parsing, so rejected
  files are never parsed. Skipping prefiltering causes catastrophic performance
  degradation on any non-trivial repository.
- **`ast-grep-core`'s `Matcher` trait is public and user-implementable.** This
  is what makes building the Semgrep formula algebra on top of ast-grep
  primitives (rather than translating to `ast-grep-config` YAML) both correct
  and necessary. Translating `patterns:` / `...` into ast-grep's `all:` / `$$$`
  is lossy because the two AND and ellipsis semantics genuinely differ in ways
  that produce silently wrong findings.
- **Construct frequency** (of 2,095 files): `pattern:` 84.3%, `patterns:` 80.6%,
  `...` ellipsis 78.3%, `pattern-either:` 45.6%, `pattern-inside:` 44.6%,
  `pattern-not:` 16.4%, `metavariable-regex:` 16.2%, `mode: taint` 12.5%,
  `focus-metavariable:` 11.6%, `metavariable-pattern:` 6.2%,
  `metavariable-comparison:` 1.9%. The `regex`/`generic` language entries are
  approximately 12% of the corpus and require no AST at all.
- **65 rule files and 119 CWE-326/327/328 references** are crypto-relevant and
  the most important set for a future CBOM/PQC consumer.
- **ast-grep 0.45.3** requires `rust-version = "1.88.0"` and
  `tree-sitter = "0.27.0"`. Cargo features on `ast-grep-language` are
  crate-named (`tree-sitter-rust`, etc.) and `default = ["builtin-parser"]`
  enables all 27 grammars, so `--no-default-features` plus explicit feature
  selection is mandatory to avoid bloat.
- **The `semgrep_rules_sources.yaml` manifest** lists 14 third-party rule
  sources. Licences span MIT through AGPL-3.0, GPL-3.0, and CC BY-NC-SA 4.0,
  with two sources declaring no licence at all. `semgrep-rules` itself carries a
  Commons Clause restriction that is absent from its own manifest entry.
  Declared licence is not ground truth; verification against the fetched
  repository is required.

## Decisions

### Decision 1: Engine placement respects the scanner/clients boundary

xzardgz enforces a component-boundary contract via `src/clients/mod.rs`: clients
must not call the scanner, and tools must not call clients directly. This plan
respects that boundary with a strict two-module split:

- `src/scanner/sast/` is the **engine**: network-free, offline-only, parses
  local files and evaluates rules already resolved from disk. It must never
  import `reqwest`, `git2`, or any other network-capable crate.
- `src/clients/rulesets/` handles ruleset **acquisition only** (git/path/HTTPS
  fetch, licence verification, content-addressed caching). It must never import
  any type from `src/scanner/sast/`. A CI grep enforces this boundary.

`src/tools/sast_tool.rs` and `src/plugins/sast_scan/` import from
`src/scanner/sast/` but not from `src/clients/rulesets/`. Ruleset resolution
(reading rules already on disk) is the engine's responsibility; fetching and
caching new rulesets is `src/clients/rulesets/`'s responsibility.

### Decision 2: Ruleset licensing and acquisition

`semgrep-rules` carries a Commons Clause restriction not reflected in its own
declared licence metadata. Accordingly:

1. `semgrep-rules` is **never vendored** into this repository. No
   `include_str!`, no submodule, no copy.
2. xzardgz ships a small **first-party ruleset** under the repository's existing
   licence, covering Rust-specific crypto detections. The semgrep-rules corpus
   is a reference for what to detect, never a source to copy.
3. The engine discovers rulesets from a fixed, offline, ordered search path.
   Nothing is fetched implicitly during a scan.
4. Licence is verified against the fetched repository, not taken from a
   manifest; both declared and observed licences are recorded. A deny-by-default
   policy gates which external rulesets may load.

### Decision 3: Engine scope is search mode only; taint mode is a separate plan

Taint mode (source/sink/sanitizer dataflow, even scoped to a single function)
requires a CFG-like structure, a propagation algorithm, and a new rule-schema
surface, each with its own soundness and performance considerations. The
reference plan measured the equivalent implementation at approximately 24,650
lines of OCaml in opengrep, spanning an IL, a CFG builder, and a fixpoint
solver. This plan ships **search mode only**. Taint mode is deferred to
`docs/explanation/sast_taint_mode_implementation_plan.md`, which can build on
the shared AST layer this plan produces.

All other deferred constructs from the reference plan also stand: deep
expressions `<... ...>`, typed metavariables `(T $X)`, `metavariable-analysis`,
constant propagation. These affect a combined approximately 5% of the corpus.

### Decision 4: Native formula engine, not YAML translation

Translating `patterns:` / `...` into `ast-grep-config`'s `all:` / `$$$` is lossy
in ways that produce silently wrong findings. xzardgz implements the
range-with-metavariables algebra directly on `ast-grep-core` primitives via the
`Matcher` trait and does **not** depend on `ast-grep-config`. The `Formula` IR
(`And`, `Or`, `Not`, `Inside`, `Leaf`) is compiled from the parsed rule schema
and evaluated natively against each file's parsed root.

### Decision 5: Dual integration -- ToolExecutor and direct call path

The engine is consumed in two ways that must not be conflated:

- `SastTool: ToolExecutor` registers in `src/tools/registry.rs` and is callable
  from any `AgentSession` via the tool-calling protocol. It accepts JSON
  parameters and returns a `ToolResult`.
- A direct synchronous call path in `src/plugins/security_review/plugin.rs` runs
  the engine as a deterministic pre-AI pass, adding `SastMatch` findings to
  `security_review`'s scoring pipeline before the AI completion call. This path
  requires no tool-calling infrastructure and is always available regardless of
  whether an AI provider is configured.

The `SastScanPlugin: WorkflowPlugin` is a third, standalone consumer for use
cases where a dedicated SAST pipeline run is desired outside the context of
`security_review`.

## Module Structure

```text
src/scanner/sast/                  <- ENGINE: network-free, no reqwest, no git2
    mod.rs                         <- SastEngine facade: new / with_rules / scan
    error.rs                       <- SastError, RuleParseError, SkipReason (thiserror)
    config.rs                      <- SastEngineConfig: timeouts, limits, jobs
    match_model.rs                 <- SastMatch: neutral engine output model
    fingerprint.rs                 <- stable blake2b match fingerprint
    embedded.rs                    <- embedded_rules() BTreeMap (include_str! pattern)
    rule/
        mod.rs
        schema.rs                  <- serde types mirroring the Semgrep YAML surface
        ir.rs                      <- RuleIr, Formula enum (Leaf/And/Or/Not/Inside)
        parse.rs                   <- schema -> ir with structural invariant checks
        compat.rs                  <- unsupported-construct gate; produces SkipReason
        metadata.rs                <- typed metadata: cwe, owasp, confidence, licence
        resolve.rs                 <- ruleset search-path resolution and precedence
    ast/
        mod.rs
        lang.rs                    <- Language enum, extension detection, ast-grep bridge
        parse.rs                   <- parse cache: path -> Root, one parse per file
        diagnostics.rs             <- ERROR-node density accounting
    engine/
        mod.rs
        pattern.rs                 <- ast-grep Pattern compilation and caching
        range.rs                   <- RangeWithMetavars; intersect / union / subtract
        formula.rs                 <- AND/OR/NOT/INSIDE evaluation over range sets
        conditions.rs              <- metavariable-regex/pattern/comparison, focus
        compare.rs                 <- closed-grammar comparison evaluator
        regex_mode.rs              <- languages: [regex, generic] path, no AST
    target/
        mod.rs
        discover.rs                <- gitignore-aware walk, binary-skip, size limit
        prefilter.rs               <- aho-corasick + RegexSet formula over raw bytes
    output/
        mod.rs
        sarif.rs                   <- SARIF 2.1.0 projection
        cyclonedx.rs               <- CycloneDX 1.7 vulnerability projection
    rules/                         <- bundled first-party rules (Apache-2.0)
        security/*.yaml
        crypto/*.yaml

src/plugins/sast_scan/             <- PLUGIN (WorkflowPlugin)
    mod.rs
    plugin.rs                      <- SastScanPlugin: impl WorkflowPlugin
    config.rs                      <- SastScanConfig (ai_confidence_weight, ai_analysis_enabled)
    report.rs                      <- Markdown report generator
    scoring.rs                     <- impl ScoringInput for SastScanResult

src/tools/sast_tool.rs             <- SastTool: impl ToolExecutor

src/clients/rulesets/              <- PHASE 8: acquisition, network. MUST NOT import
                                      src::scanner::sast types.
    mod.rs
    sources.rs                     <- semgrep_rules_sources.yaml manifest format
    source.rs                      <- RulesetSource: Path | Git | Url | Oci
    fetch.rs                       <- git clone / HTTPS retrieval
    license.rs                     <- licence detection, classification, policy gate
    cache.rs                       <- content-addressed cache
    manifest.rs                    <- BundleManifest: source, commit, licences, digest
```

### Naming Constraints

| Constraint                                                | Reason                                                                               |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| Engine match type is `SastMatch`                          | No naming conflict exists in xzardgz; matches opengrep terminology for engine output |
| Engine config is `SastEngineConfig`                       | Distinguishes engine knobs from plugin-level `SastScanConfig`                        |
| Plugin config key in `Config` is `sast_scan`              | Matches existing `technical_review` / `security_review` pattern                      |
| Engine config key in `Config` is `sast`                   | Engine-wide, shared across both consumers                                            |
| Env prefix is `XZARDGZ_SAST_*`                            | Matches existing env var conventions in `src/config.rs`                              |
| `SastError` maps to `PipelineError::Scanner`              | Follows the existing `From<X> for PipelineError` pattern in `src/error.rs`           |
| `src/clients/rulesets/` never imports `src/scanner/sast/` | Enforces the clients boundary contract in `src/clients/mod.rs`                       |

## Implementation Phases

Quality gate (every phase, per AGENTS.md Rule 4):

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Markdown files updated or added in each phase must pass:

```bash
markdownlint --fix --config .markdownlint.json "${FILE}"
prettier --write --parser markdown --prose-wrap always "${FILE}"
```

### Phase 0: Dependencies and MSRV Gate

#### 0.1 Foundation Work

Add crate dependencies required by the engine. `ast-grep-language`'s Cargo
features are crate-named; `--no-default-features` is mandatory because the
default feature enables all 27 grammars. v1 requires only `tree-sitter-rust`:

```bash
cargo add ast-grep-core@0.45
cargo add ast-grep-language@0.45 --no-default-features --features tree-sitter-rust
cargo add aho-corasick
cargo add blake2
cargo add rayon
cargo add ignore
cargo add globset
cargo add dirs
cargo add --dev jsonschema
```

`ast-grep-config` is deliberately **not** added (Decision 4). `regex`, `serde`,
`serde_json`, `thiserror`, `serde_yaml`, and `tempfile` are already dependencies
and must not be re-added. `git2` is deferred to Phase 8.

Set the MSRV in `Cargo.toml` `[package]`:

```toml
rust-version = "1.88.0"
```

Vendor the CycloneDX 1.7 JSON Schema (Apache-2.0) to
`testdata/cyclonedx/bom-1.7.schema.json` and the SARIF 2.1.0 schema to
`testdata/sarif/sarif-2.1.0.schema.json`, each with a `README.md` recording the
upstream URL and retrieval date.

#### 0.2 Add Foundation Functionality

Establish the external-corpus test convention. Tests requiring the
`semgrep-rules` corpus (Phase 9) are gated on a `sast-integration-tests` Cargo
feature and a `XZARDGZ_SEMGREP_RULES_DIR` environment variable pointing at a
local checkout. When the feature flag is set and the variable is unset or points
at a non-existent directory, the test must skip with an explicit printed reason
and must never fail or silently pass. Add one shared helper in
`tests/helpers/sast_corpus.rs`.

#### 0.3 Integrate Foundation Work

Verify that `cargo check --all-targets --all-features` is clean after adding the
dependencies. Confirm `cargo tree -p ast-grep-language` lists exactly one
`tree-sitter-*` crate (`tree-sitter-rust`). Confirm
`grep -n 'rust-version' Cargo.toml` prints `1.88.0`.

#### 0.4 Configuration Updates

No user-visible configuration changes in this phase. The `sast` and `sast_scan`
config keys are added to `src/config.rs` in Phase 6 once the engine is complete
enough to configure meaningfully.

#### 0.5 Testing Requirements

- `cargo check --all-targets --all-features` exits 0 after dependency additions.
- A unit test asserts both vendored schemas parse as valid JSON Schema via the
  `jsonschema` crate.
- `tests/helpers/sast_corpus.rs` has tests for both the present and absent cases
  of `XZARDGZ_SEMGREP_RULES_DIR`.
- `cargo tree -p ast-grep-language` output is asserted (in CI or a dedicated
  check) to list exactly one grammar crate.

#### 0.6 Deliverables

- `ast-grep-core`, `ast-grep-language` (rust feature only), `aho-corasick`,
  `blake2`, `rayon`, `ignore`, `globset`, `dirs` added to `Cargo.toml`;
  `jsonschema` added as a dev-dependency
- `rust-version = "1.88.0"` set in `Cargo.toml`
- `testdata/cyclonedx/bom-1.7.schema.json` and
  `testdata/sarif/sarif-2.1.0.schema.json` with provenance `README.md` files
- `tests/helpers/sast_corpus.rs` corpus-gating helper
- `docs/explanation/sast_scanning_tool_plan.md` (this document)

#### 0.7 Success Criteria

| Criterion                   | Verification                                                             |
| --------------------------- | ------------------------------------------------------------------------ |
| Dependencies resolve        | `cargo check --all-targets --all-features` exits 0                       |
| MSRV declared               | `grep -n 'rust-version' Cargo.toml` prints `1.88.0`                      |
| Only one grammar linked     | `cargo tree -p ast-grep-language` shows only `tree-sitter-rust`          |
| Corpus helper skips cleanly | `cargo test --features sast-integration-tests` passes with env var unset |

---

### Phase 1: Shared AST Layer, Rule Model, Compatibility Gate, and Regex Mode

#### 1.1 Feature Work

Build the foundational components that all later phases depend on. This is the
most conceptually dense phase because it establishes correctness invariants that
subsequent phases must not violate.

**`src/scanner/sast/ast/lang.rs`**: `Language` enum with one `Rust` AST variant
plus `Regex` and `Generic`. Extension-based detection (`rs` -> `Rust`; no
extension defaults to `Generic` with optional shebang detection), and
`to_ast_grep(&self) -> Option<ast_grep_language::SupportLang>` returning `None`
for `Regex`/`Generic`.

**`src/scanner/sast/ast/parse.rs`**: One `ast_grep_core::AstGrep` root per
`(path, language)` pair per scan, behind a concurrency-safe cache. Enforce
`SastEngineConfig::max_file_bytes` before reading. A file parsed with errors is
retained with a degraded density flag rather than silently skipped.

**`src/scanner/sast/ast/diagnostics.rs`**: Count `ERROR` and `MISSING` nodes per
file; expose `ErrorNodeDensity` so tree-sitter's silent partial parses are
surfaced rather than hidden.

**`src/scanner/sast/rule/schema.rs`** and
**`src/scanner/sast/rule/metadata.rs`**: Serde structs modelling the full
Semgrep YAML rule surface, including constructs the engine will not execute, so
the compatibility gate can report them precisely. Closed metadata enums
(`Confidence`, `Likelihood`, `Impact`, `Severity`, `Category`) with
`#[serde(other)] Unknown` arms. Accept `String | Vec<String>` for `cwe`,
`owasp`, `references`, and `technology`.

**`src/scanner/sast/rule/ir.rs`**: The compiled intermediate representation:

```rust
pub enum Formula {
    Leaf(Leaf),
    And {
        conjuncts: Vec<Formula>,
        negations: Vec<Formula>,
        conditions: Vec<Condition>,
        focus: Vec<MetavarId>,
    },
    Or(Vec<Formula>),
    Inside(Box<Formula>),
}
```

`And` carries negations, conditions, and focus as siblings rather than nested
nodes: `pattern-not` subtracts from the enclosing `And`'s positive conjuncts
after the full formula resolves; `focus-metavariable` applies last.

**`src/scanner/sast/rule/parse.rs`**: Enforce structural invariants: exactly one
of `pattern` / `patterns` / `pattern-either` / `pattern-regex` at the formula
root; `pattern-not*` only inside `patterns:`; `patterns:` has at least one
positive term; `metavariable-*` and `focus-metavariable` only under `patterns:`;
rule id matches `^[a-zA-Z0-9._-]+$`.

**`src/scanner/sast/rule/compat.rs`**: Return `Vec<SkipReason>` for:
`mode: taint` (deferred; see Decision 3), `mode: join` / `extract` / `step`,
deep expressions `<... ...>`, typed metavariables `(T $X)`,
`metavariable-analysis`, `fix-regex`, `pattern-propagators`. Never partially
evaluate a gated rule; skip the entire rule and log the reason.

**`src/scanner/sast/engine/regex_mode.rs`**: For `languages: [regex]` or
`[generic]`, apply `pattern-regex` over file bytes, binding named capture groups
as `$NAME` metavariables, with no AST parsing involved. Compile all
rule-supplied regexes through a single builder with
`RegexBuilder::size_limit(10_485_760)` and `dfa_size_limit(10_485_760)` (10 MiB
each) to bound ReDoS risk (Threat Model).

**`src/scanner/sast/error.rs`**: `SastError`, `RuleParseError`, `SkipReason`
using `thiserror`. Add `PipelineError::Sast(String)` variant to `src/error.rs`
(or reuse `PipelineError::Scanner`) and implement
`From<SastError> for PipelineError` mapping to that variant, following the
`From<ConfigError> for PipelineError` pattern already in `src/error.rs`.

**`src/scanner/sast/config.rs`**: `SastEngineConfig` with fields:
`max_file_bytes: u64` (default 5 MiB), `rule_timeout_ms: u64` (default 5,000),
`max_matches_per_file: usize` (default 100), `jobs: usize` (default 0 =
`available_parallelism`).

**First-party test fixture rules**: Author 5-8 first-party Apache-2.0 rules
under `src/scanner/sast/rules/` covering Rust-specific crypto detections (weak
RSA key size via the `rsa` crate, MD5/SHA-1 usage via the `md5`/`sha1` crates,
use of the `DES` cipher) plus Rust security patterns (hardcoded credential
patterns in struct literals, use of `unsafe` in a specific call pattern). These
fixture rules are not copied from semgrep-rules; they are authored in-house.
They give Phases 2 through 6 real targets to compile, match, and output.

#### 1.2 Integrate Feature

Wire `SastError` into the pipeline error type. Confirm `src/scanner/sast/` does
not import anything from `src/clients/`, `src/tools/`, or `src/plugins/`.
Confirm `src/error.rs` has the `From<SastError>` impl following the existing
legacy sub-error pattern.

#### 1.3 Configuration Updates

No user-visible config keys are added in this phase. `SastEngineConfig` is
defined here but wired into `Config` in Phase 6.

#### 1.4 Testing Requirements

- Unit tests for `Language` detection: success (each known extension), failure
  (unknown extension), edge cases (extensionless with shebang, uppercase
  extension).
- Unit tests for every parse invariant in `rule/parse.rs` (one test per
  invariant, verifying both the valid and invalid cases).
- Unit tests for every `SkipReason` variant in `rule/compat.rs` (assert the rule
  is skipped, not partially evaluated).
- `regex_mode` tests: named-capture group binding, no-match, invalid regex
  rejected, oversized regex rejected.
- Parse-cache test: a file is parsed exactly once across two lookups of the same
  path.
- `ErrorNodeDensity` test: a deliberately malformed Rust source file produces a
  non-zero density.
- Corpus regression test (gated on `sast-integration-tests` feature): parse all
  rule files in `XZARDGZ_SEMGREP_RULES_DIR`; assert zero unexpected parse
  failures (rules that fail for reasons other than a recognized `SkipReason`).

#### 1.5 Deliverables

- `src/scanner/sast/ast/{mod,lang,parse,diagnostics}.rs`
- `src/scanner/sast/rule/{mod,schema,ir,parse,compat,metadata}.rs`
- `src/scanner/sast/engine/regex_mode.rs` with regex size limits
- `src/scanner/sast/error.rs` with `SastError`, `RuleParseError`, `SkipReason`
- `From<SastError> for PipelineError` in `src/error.rs`
- `src/scanner/sast/config.rs` with `SastEngineConfig`
- 5-8 first-party rules under `src/scanner/sast/rules/`
- `///` doc comments on every public item

#### 1.6 Success Criteria

| Criterion                            | Verification                                                        |
| ------------------------------------ | ------------------------------------------------------------------- |
| Every corpus rule parses or is gated | Corpus regression test reports zero unexpected failures             |
| Regex mode works with no AST         | `regex_mode` tests pass with `ast/parse.rs` exercised zero times    |
| Parse cache is single-parse          | Cache test asserts the parse counter equals 1 after two lookups     |
| Boundary respected                   | `grep -rn 'use crate::clients' src/scanner/sast/` returns zero hits |

---

### Phase 2: Core Matching Engine

#### 2.1 Feature Work

Build the three engine modules that translate a compiled `Formula` into a set of
`RangeWithMetavars` over a parsed source file.

**`src/scanner/sast/engine/pattern.rs`**: Compile a Semgrep pattern string to an
`ast_grep_core::Pattern`. The `...` to `$$$` rewrite is context-aware: argument
lists, statement sequences, parameter lists, and array/slice literals map to
different structural wildcards. Cache compiled patterns keyed by
`(pattern_text, language)`. Use `MatchStrictness::Relaxed` by default; honour
`options.strictness` when present in the rule.

**`src/scanner/sast/engine/range.rs`**: The core algebraic type:

```rust
pub struct RangeWithMetavars {
    pub start: usize,
    pub end: usize,
    pub bindings: MetavarBindings,
}
```

Operations:

- `intersect(a, b)`: overlap check plus binding unification; incompatible
  bindings (two ranges binding `$X` to different text) kill the pair.
- `union(sets)`: deduplicate by `(start, end, bindings)`.
- `subtract(pos, neg)`: drop positive ranges fully contained within a negative
  range.

Keep ranges sorted by `(start, end)` for deterministic output.

**`src/scanner/sast/engine/formula.rs`**: Evaluate a `Formula` against a parsed
root, producing `Vec<RangeWithMetavars>`. Enforce a per-rule-per-file timeout
(`SastEngineConfig::rule_timeout_ms`, default 5,000 ms) and
`max_matches_per_file` (default 100), recording a `TruncationReason` when the
cap is hit rather than silently dropping findings.

#### 2.2 Integrate Feature

Wire the three engine modules so a `RuleIr` plus a parsed
`ast_grep_core::AstGrep` root can be driven to a `Vec<RangeWithMetavars>` in a
single call. Confirm the fixture rules from Phase 1 compile and produce matches
on hand-written Rust source fixtures under `testdata/sast/fixtures/`.

#### 2.3 Configuration Updates

`SastEngineConfig` fields `rule_timeout_ms` and `max_matches_per_file` are
defined in Phase 1 and first exercised in this phase's timeout and truncation
tests.

#### 2.4 Testing Requirements

- `intersect` / `union` / `subtract`: success, empty-set, identical-range,
  adjacent-but-disjoint, and fully-contained cases.
- Binding-unification failure: two ranges binding `$X` to different text must
  not intersect.
- Property test: `subtract(a, b).len() <= a.len()` holds for arbitrary `a` and
  `b` inputs (at least 10,000 generated cases).
- `...` rewrite tests: one positive and one near-miss fixture per syntactic
  context (argument list, statement sequence, slice literal), exercising the
  context-aware rewrite.
- Timeout test: a pathological pattern completes within 2x `rule_timeout_ms` and
  records a `TruncationReason`.
- Truncation test: a rule that would produce 200 matches on a fixture is capped
  at 100 and records the reason.
- End-to-end fixture test: the Phase 1 first-party rules match their positive
  fixtures and do not fire on their negative fixtures.

#### 2.5 Deliverables

- `src/scanner/sast/engine/{mod,pattern,range,formula}.rs`
- Property tests for `subtract` and `union` in `engine/range.rs`
- Positive and negative fixture files for each Phase 1 rule under
  `testdata/sast/fixtures/`
- End-to-end fixture suite passing

#### 2.6 Success Criteria

| Criterion                               | Verification                                                    |
| --------------------------------------- | --------------------------------------------------------------- |
| Fixture rules all match correctly       | End-to-end fixture suite passes through `formula.rs`            |
| Subtraction is sound                    | Property test passes over 10,000 generated cases                |
| Timeouts are enforced per rule per file | Pathological-pattern test completes within 2x `rule_timeout_ms` |
| Context-aware `...` rewrite is correct  | One passing and one non-matching fixture per syntactic context  |

---

### Phase 3: Metavariable Conditions and Focus

#### 3.1 Feature Work

Extend the engine with the condition evaluators that filter or narrow the range
set produced by Phase 2's formula evaluator.

**`metavariable-regex`** (`src/scanner/sast/engine/conditions.rs`): Anchored
match against the bound text of a named metavariable, compiled through the same
size-limited builder from Phase 1's `regex_mode.rs`. An unanchored substring
match must not satisfy the condition.

**`metavariable-pattern`** (`src/scanner/sast/engine/conditions.rs`): Recursive
`Formula` evaluation scoped to the bound node of a named metavariable, with an
optional `language:` switch for cross-language metavariable patterns. Enforce a
recursion depth limit of 10 to prevent unbounded nesting.

**`metavariable-comparison`** (`src/scanner/sast/engine/compare.rs`): A
deliberately small closed-grammar expression evaluator. Supported: integer and
float literals, metavariable references, the operators `< <= > >= == !=`, and
the logical connectives `and` / `or` / `not`. Optional `strip` (strip
non-numeric suffix) and `base` (numeric base for the bound text) options. Any
unsupported construct produces `SkipReason::UnsupportedComparison` and skips the
rule entirely, never attempting partial evaluation. This evaluator must not call
`eval` or any general-purpose expression evaluator.

**`focus-metavariable`** (`src/scanner/sast/engine/conditions.rs`): Narrow each
surviving range to the focused metavariable's range after the full formula
resolves. Multiple focus variables intersect. A range where the focused variable
has no binding yields no surviving range.

#### 3.2 Integrate Feature

Integrate the condition evaluators into `engine/formula.rs`'s `And` evaluation
path. Conditions are applied after the positive conjuncts intersect and after
`Not` subtraction, but before `focus-metavariable` narrowing. Confirm the worked
example (see Worked Example section) matches a weak RSA key call and does not
match a compliant one.

#### 3.3 Configuration Updates

`metavariable-regex` compilations use the same size-limited `RegexBuilder`
established in Phase

1. No new `SastEngineConfig` fields are required in this phase.

#### 3.4 Testing Requirements

- `metavariable-regex`: anchored match succeeds; an unanchored substring does
  not match; an invalid regex is rejected at rule compile time; an oversized
  regex is rejected.
- `metavariable-pattern`: nested match succeeds; language switch works;
  depth-limit exceeded returns `SkipReason::RecursionLimitExceeded`.
- `compare.rs`: one test per operator, one test for precedence, one test each
  for `strip` and `base`, and tests for division by zero, type mismatch, and an
  unsupported construct producing `SkipReason::UnsupportedComparison`.
- `focus-metavariable`: single focus; multiple intersecting focuses; a
  non-containing focus variable yields no surviving range.
- Worked-example integration test: the Rust weak-RSA-key rule from the Worked
  Example section matches a `RsaPrivateKey::new(&mut rng, 1024)` call site and
  does not match a `2048` call site.

#### 3.5 Deliverables

- `src/scanner/sast/engine/conditions.rs` (metavariable-regex,
  metavariable-pattern, focus)
- `src/scanner/sast/engine/compare.rs` (metavariable-comparison closed-grammar
  evaluator)
- `SkipReason::UnsupportedComparison` and `SkipReason::RecursionLimitExceeded`
  wired into `rule/compat.rs`
- Worked-example integration test passing
- `///` doc comments on all public items

#### 3.6 Success Criteria

| Criterion                                         | Verification                                                        |
| ------------------------------------------------- | ------------------------------------------------------------------- |
| Comparison rejects all out-of-grammar expressions | Test over 1,000 random expressions: no panic, no partial evaluation |
| Regex conditions are anchored                     | Unanchored-substring test asserts no match                          |
| Focus narrows the reported range                  | Worked-example test asserts the match range covers only `$BITS`     |
| Recursion depth is bounded                        | Depth-limit test does not overflow the stack                        |

---

### Phase 4: Targeting, Prefiltering, and Parallel Execution

#### 4.1 Feature Work

Build the file-selection infrastructure and the `SastEngine` facade that ties
everything together.

**`src/scanner/sast/target/discover.rs`**: Gitignore-aware directory walk via
the `ignore` crate. `paths.include` and `paths.exclude` via `globset`. Skip
binary files (null-byte heuristic) and enforce
`SastEngineConfig::max_file_bytes` before reading. Expose the walk result as a
sorted, deterministic list of paths.

**`src/scanner/sast/target/prefilter.rs`**: For each rule, derive a
`Pred(Idents | Regexp) | And | Or` predicate formula from the `Formula` IR
(deliberately no `Not`, because negation cannot soundly exclude a file). Compile
the union of all literal identifiers from all rules into one
`aho_corasick::AhoCorasick` automaton and regex predicates into one
`regex::RegexSet`. Bail out to `None` (meaning "must analyse") for over-general
formulas such as bare `pattern: $X`. Only advance to AST parsing when at least
one rule's predicate survives against the raw file bytes.

**`src/scanner/sast/mod.rs`** -- `SastEngine` facade:

```rust
impl SastEngine {
    pub fn new(config: SastEngineConfig) -> Result<Self, SastError>;
    pub fn with_rules(&mut self, rules: Vec<RuleIr>) -> Result<(), SastError>;
    pub fn scan(&self, root: &Path) -> Result<SastScanReport, SastError>;
}
```

`SastScanReport` carries: `matches: Vec<SastMatch>`,
`skipped_rules: Vec<SkippedRule>`, `scanned_file_count: usize`,
`skipped_file_count: usize`, `parse_error_count: usize`,
`truncated_rule_file_pairs: usize`, and `duration_ms: u64`. Parallelise over
target files with `rayon`, honouring `SastEngineConfig::jobs` (0 =
`available_parallelism`). Convert per-target panics and errors into per-target
error records rather than aborting the scan; one bad file must not lose results
from the rest.

The prefilter step is layered on top of `PluginContentScanner`'s file-selection
pass when the engine is invoked from within a plugin: the plugin already has a
filtered file list, and the engine's own prefilter applies its
aho-corasick/regex pass on top of that to avoid re-walking. When called from
`SastTool` or the CLI, the engine drives its own walk via `discover.rs`.

#### 4.2 Integrate Feature

Confirm the engine produces identical finding sets when invoked via the facade
against a fixture repository with and without prefiltering enabled. This
differential test is the single most important correctness assertion in this
phase.

#### 4.3 Configuration Updates

`SastEngineConfig::jobs` is defined in Phase 1 and first exercised here. No new
config fields are required.

#### 4.4 Testing Requirements

- **Prefilter soundness differential test**: run a sample of rules over a
  fixture corpus with prefiltering on and off; assert byte-identical finding
  sets. This is the most important test in this phase.
- Prefilter bail-out test: a rule whose formula reduces to bare `$X` produces a
  `None` predicate and always passes the filter.
- No-`Not` in prefilter: assert `SkipReason::NegationInPrefilter` is never
  produced; negation always degrades to `None`.
- Discovery tests: `.gitignore` entries are respected; binary files are skipped;
  `paths.include` and `paths.exclude` are honoured; `max_file_bytes` is
  enforced.
- Panic-isolation test: a synthetic target that panics during rule evaluation
  does not cause the scan to lose results from other files.
- Determinism test: two runs over the same fixture tree produce identically
  ordered results (sorted by `(path, start_byte, rule_id)`).

#### 4.5 Deliverables

- `src/scanner/sast/target/{mod,discover,prefilter}.rs`
- `src/scanner/sast/mod.rs` with `SastEngine`, `SastScanReport`
- Rayon parallelism with per-target error isolation
- Prefilter soundness differential test

#### 4.6 Success Criteria

| Criterion                          | Verification                                                            |
| ---------------------------------- | ----------------------------------------------------------------------- |
| Prefilter is sound                 | Differential test: identical finding sets with and without prefiltering |
| Prefiltered files are never parsed | Parse-counter assertion in the differential test                        |
| One bad file cannot lose a run     | Panic-isolation test passes                                             |
| Results are deterministic          | Two-run determinism test produces identical output                      |

---

### Phase 5: Match Model and Output Projections

#### 5.1 Feature Work

Introduce the neutral `SastMatch` type that all consumers receive, stable
fingerprinting for deduplication and change tracking, and the two output
projections.

**`src/scanner/sast/match_model.rs`**:

```rust
pub struct SastMatch {
    pub rule_id:        String,              // namespaced: "<ruleset_id>::<rule_id>"
    pub ruleset_id:     String,
    pub message:        String,
    pub severity:       Severity,
    pub confidence:     Confidence,
    pub path:           PathBuf,             // repo-relative, always
    pub start:          Position,            // line 1-based, col 0-based, byte offset
    pub end:            Position,
    pub snippet:        MatchSnippet,
    pub metavariables:  BTreeMap<String, MetavarBinding>,
    pub metadata:       RuleMetadata,        // cwe, owasp, references, licence, url
    pub fingerprint:    String,
    pub fix:            Option<String>,      // recorded only, never auto-applied
}
```

`Position` carries `line: u32`, `col: u32`, and `byte: usize`. `MatchSnippet`
carries the matched source text plus the surrounding context lines for
reporting.

**`src/scanner/sast/fingerprint.rs`**: blake2b over
`(canonical formula string with bindings substituted, repo-relative path, rule_id)`
plus an index suffix for multiple matches of the same rule in the same file. The
fingerprint must be stable across runs and across absolute scan-root paths; no
absolute path may appear in the hash input.

**`src/scanner/sast/output/sarif.rs`**: SARIF 2.1.0 projection. Severity
mapping: `Info` / `Low` maps to `note`; `Warning` / `Medium` maps to `warning`;
`Error` / `Critical` / `High` maps to `error`. The `rules[].properties.tags`
array carries each `cwe` and `owasp` value plus a literal `"security"` tag when
any CWE is present.

**`src/scanner/sast/output/cyclonedx.rs`**: Project `SastMatch` items into a
CycloneDX 1.7 `vulnerabilities` array. Each entry carries `ratings`, `cwes`,
`affects`, `analysis`, and `source` (the rule's source URL and the ruleset's
licence). This module does not define new CycloneDX types; it uses the
`CycloneDxVulnerability` struct from wherever that is defined in the codebase
(or defines it here if it does not yet exist).

#### 5.2 Integrate Feature

Confirm the `SastEngine::scan` facade now returns `SastScanReport` containing
fully populated `SastMatch` items with fingerprints, and that the two
projections can serialize the report to valid SARIF and CycloneDX 1.7 JSON.

#### 5.3 Configuration Updates

No new config fields. The `SastEngineConfig` fields established in Phase 1 are
sufficient.

#### 5.4 Testing Requirements

- Golden-file tests for both projections against fixtures under
  `testdata/sast/golden/`.
- CycloneDX golden output validated against
  `testdata/cyclonedx/bom-1.7.schema.json` via the `jsonschema` dev-dependency.
- SARIF golden output validated against
  `testdata/sarif/sarif-2.1.0.schema.json`.
- Fingerprint stability test: the same scan from two different absolute scan
  roots produces identical fingerprints.
- Fingerprint uniqueness test: two matches of the same rule in the same file get
  different fingerprints via the index suffix.
- Empty-result test: zero matches produces a schema-conformant SARIF run and an
  empty `vulnerabilities` CycloneDX BOM, not an error.
- `fix` field test: a rule with `fix:` has the fix recorded in `SastMatch.fix`
  and does not appear in any modified file.

#### 5.5 Deliverables

- `src/scanner/sast/match_model.rs` with `SastMatch`, `Position`,
  `MatchSnippet`, `MetavarBinding`
- `src/scanner/sast/fingerprint.rs`
- `src/scanner/sast/output/{mod,sarif,cyclonedx}.rs`
- Golden-file tests for both projections
- Schema validation tests

#### 5.6 Success Criteria

| Criterion                                 | Verification                                                     |
| ----------------------------------------- | ---------------------------------------------------------------- |
| SARIF output is spec-valid                | `jsonschema` validation against the vendored 2.1.0 schema passes |
| CycloneDX output is spec-valid            | `jsonschema` validation against `bom-1.7.schema.json` passes     |
| Fingerprints survive scan-root relocation | Two-root stability test asserts identical hashes                 |
| `fix` is never applied                    | No source file is modified during any test run                   |

---

### Phase 6: Builtin Ruleset, Resolution, and CLI Subcommand

#### 6.1 Feature Work

Complete the first-party ruleset, the embedded rule assets, the ruleset search
path, and the `xzardgz sast scan` CLI subcommand.

**Complete the first-party ruleset**: Extend the Phase 1 fixture rules to the
full v1 builtin set:

| Ruleset id         | Directory                          | Target coverage                                                                                                                                      |
| ------------------ | ---------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `builtin:crypto`   | `src/scanner/sast/rules/crypto/`   | Weak RSA key size (rsa crate), MD5/SHA-1 (md5/sha1 crates), DES/RC4/3DES cipher usage, hardcoded IV/salt, weak RNG. Minimum 15 rules for Rust.       |
| `builtin:security` | `src/scanner/sast/rules/security/` | Hardcoded credentials in struct literals (AST assignment patterns, not substrings), insecure TLS config, debug flags left enabled. Minimum 10 rules. |

Each rule carries `metadata.license` (this repository's licence) and
`metadata.source-rule-url`.

**`src/scanner/sast/embedded.rs`**:
`pub fn embedded_rules() -> BTreeMap<&'static str, &'static str>` built with
`include_str!` macros. Add a corresponding `include` entry in `Cargo.toml`
`[package]` so rule files ship in a packaged crate.

**`src/scanner/sast/rule/resolve.rs`**: Ordered ruleset search path:

| Order | Location                             | Purpose                      |
| ----: | ------------------------------------ | ---------------------------- |
|     1 | `--ruleset <id\|path>` CLI flag      | Per-invocation override      |
|     2 | `XZARDGZ_SAST_RULESET_PATHS` env var | Deployment-supplied override |
|     3 | `~/.config/xzardgz/rulesets/`        | User-supplied, per-user      |
|     4 | `builtin:` embedded rules            | Always-available fallback    |

Discovery is offline; a missing external ruleset degrades to the builtin set
with a logged warning, never failing the run. Rule ids are namespaced by ruleset
id (`builtin:crypto::rust-weak-rsa-key`) so two rulesets defining the same base
id cannot collide.

**`xzardgz sast scan` CLI subcommand**: Following the existing CLI subcommand
patterns in the project:

| Flag                                | Effect                                                                 |
| ----------------------------------- | ---------------------------------------------------------------------- |
| `<path>` (positional)               | Directory to scan (required)                                           |
| `--ruleset <ID\|PATH>`              | Repeatable; overrides the default `[builtin:crypto, builtin:security]` |
| `--ruleset-path <DIR>`              | Repeatable; prepends to the search path                                |
| `--format <sarif\|cyclonedx\|json>` | Output projection; default `json` (raw `SastMatch` list)               |
| `--output <FILE>`                   | Write to file instead of stdout                                        |
| `--jobs <N>`                        | Parallelism override (0 = automatic)                                   |
| `--severity <LEVEL>`                | Minimum severity threshold (info/low/medium/high/critical)             |

#### 6.2 Integrate Feature

Wire `SastEngine` into the CLI subcommand. Confirm a scan of the xzardgz
repository itself against the builtin ruleset completes without error and
produces valid SARIF output.

#### 6.3 Configuration Updates

Add two new top-level fields to `src/config.rs::Config`:

```rust
#[serde(default)]
pub sast: SastEngineConfig,

#[serde(default)]
pub sast_scan: SastScanConfig,
```

`SastEngineConfig` is defined in Phase 1. `SastScanConfig` is defined in Phase 7
(`src/plugins/sast_scan/config.rs`) and imported here. The YAML config file key
`sast` maps to `SastEngineConfig`; the key `sast_scan` maps to `SastScanConfig`.
Environment variable overrides follow the `XZARDGZ_SAST_*` prefix convention.

Update `config.example.yaml` with documented `sast:` and `sast_scan:` blocks.

#### 6.4 Testing Requirements

- Every builtin rule parses, compiles, and gates cleanly (zero unexpected
  `SkipReason` values).
- Every builtin rule has a positive fixture (confirming it matches) and a
  negative fixture (confirming it does not fire on compliant code) under
  `testdata/sast/builtin/`.
- Search-path precedence test: a rule id present at multiple levels resolves to
  the highest.
- Missing-ruleset test: a configured path that does not exist produces a warning
  and falls back to the builtin set; the scan still succeeds.
- Namespacing test: two rulesets defining the same rule id produce distinct
  namespaced ids.
- CLI tests: default format output, each `--format` value, `--output` to a file,
  missing path argument fails with a usage error.

#### 6.5 Deliverables

- `builtin:crypto` (minimum 15 rules) and `builtin:security` (minimum 10 rules)
- Positive and negative fixtures for every builtin rule
- `src/scanner/sast/embedded.rs` with `embedded_rules()`
- `Cargo.toml` `include` entry for `src/scanner/sast/rules/`
- `src/scanner/sast/rule/resolve.rs` with the four-level search path and rule id
  namespacing
- `xzardgz sast scan` CLI subcommand
- `sast: SastEngineConfig` and `sast_scan: SastScanConfig` fields added to
  `Config`
- `config.example.yaml` updated

#### 6.6 Success Criteria

| Criterion                                 | Verification                                                                             |
| ----------------------------------------- | ---------------------------------------------------------------------------------------- |
| Builtin rules all work                    | 100% of builtin fixture pairs pass                                                       |
| Engine is useful with zero external rules | Scan with an empty ruleset search path still produces crypto findings on fixture code    |
| Missing rulesets never fail a run         | Missing-path test asserts warning and success                                            |
| Config round-trips                        | A `Config` with `sast:` and `sast_scan:` blocks serializes and deserializes without loss |

---

### Phase 7: Plugin, ToolExecutor, and security_review Wiring

#### 7.1 Feature Work

Implement the three integration surfaces (standalone plugin, tool executor, and
deterministic pre-AI pass) that give the rest of the pipeline access to the
engine.

**`src/plugins/sast_scan/config.rs`** -- `SastScanConfig`:

```rust
pub struct SastScanConfig {
    pub enabled: bool,
    pub rulesets: Vec<String>,
    pub severity_threshold: String,
    pub max_findings: usize,
    pub ai_confidence_weight: f64,      // required by src/plugins/AGENTS.md
    pub ai_analysis_enabled: bool,      // required by src/plugins/AGENTS.md
    pub report_formats: Vec<String>,
}
```

`ai_confidence_weight` and `ai_analysis_enabled` are required from the first
version of this struct per `src/plugins/AGENTS.md`. `review_violations` is set
to `false` and documented in the signal catalog (see Documentation
Deliverables).

**`src/plugins/sast_scan/scoring.rs`**: `SastScanResult` implementing
`ScoringInput`:

- `signals()`: emit `ScoringSignal::Positive` for each `SastMatch` with
  `Confidence::High`, `ScoringSignal::Negative` for matches with
  `Confidence::Low`, and `ScoringSignal::AbsoluteViolation` for matches with
  CWE-326/327/328 at severity `Error` or higher.
- `context_for_ai()`: emit a compact summary of the match set suitable for an AI
  leg.
- `plugin_name()`: return `"sast_scan"`.

**`src/plugins/sast_scan/plugin.rs`** -- `SastScanPlugin: impl WorkflowPlugin`:

- `name()` returns `"sast_scan"`.
- `supported_formats()` returns `["markdown", "json", "sarif"]`.
- `required_tool_access()` returns `ToolAccessLevel::ReadOnly`.
- `run(ctx)` constructs a `SastEngine` from `ctx.config.sast`, loads the
  rulesets configured in `ctx.config.sast_scan`, scans the workspace root, runs
  `ConfidenceScorer` on the result, and returns a `PluginOutput` containing the
  Markdown report.

**`src/plugins/sast_scan/report.rs`**: Markdown report structured as: summary
table (file, rule id, severity, confidence, message), grouped by severity, with
a section per finding showing the snippet and metavariable bindings.

**`src/tools/sast_tool.rs`** -- `SastTool: impl ToolExecutor`:

- `tool_definition()` returns a `Tool` with a JSON Schema accepting: `path`
  (required), `rulesets` (optional array of strings), `format` (optional:
  `"json"` / `"sarif"` / `"cyclonedx"`), `severity_threshold` (optional string),
  and `jobs` (optional integer).
- `execute(params)` deserializes the params, constructs a `SastEngine`, scans
  the requested path, and returns a `ToolResult` whose `output` is the
  serialized findings.

**`src/plugins/security_review/plugin.rs` extension**: Add a synchronous SAST
pre-pass that runs the engine against the workspace root before the AI
completion call. The pre-pass uses `ctx.config.sast` and the `sast_enabled` /
`sast_rulesets` / `sast_severity_threshold` fields added to
`SecurityReviewConfig`. The resulting `Vec<SastMatch>` is folded into the
plugin's signal list as additional `ScoringSignal` contributions before the AI
leg runs.

Extend `SecurityReviewConfig` in `src/config.rs`:

```rust
pub sast_enabled: bool,                  // default false until Phase 7 is complete
pub sast_rulesets: Vec<String>,          // default: ["builtin:crypto", "builtin:security"]
pub sast_severity_threshold: String,     // default "warning"
```

#### 7.2 Integrate Feature

Register `SastTool` in `src/tools/registry.rs`. Register `SastScanPlugin` in the
plugin registry. Confirm `SastScanPlugin` and `SastTool` both import from
`src/scanner/sast/` but not from `src/clients/`. Confirm
`src/tools/sast_tool.rs` does not import from `src/clients/` (tools must not
call clients per the boundary contract).

#### 7.3 Configuration Updates

`SecurityReviewConfig` gains `sast_enabled`, `sast_rulesets`, and
`sast_severity_threshold`. `SastScanConfig` is wired as `Config::sast_scan`. The
`config.example.yaml` gains documented `sast_scan:` and `security_review.sast_*`
fields. Validate `sast_severity_threshold` in
`src/plugins/security_review/config.rs::validate_security_review_config`
following the existing `VALID_SEVERITY_THRESHOLDS` pattern.

#### 7.4 Testing Requirements

Three tests required per `src/plugins/AGENTS.md` for the `ScoringInput`
implementation:

1. `test_absolute_violation_passes_filter_regardless_of_threshold`: a
   `SastScanResult` with a CWE-326 `Error`-severity match is retained even with
   `confidence_threshold = 1.0`.
2. `test_ai_confidence_weight_shifts_blended_score`: run scoring twice with
   different `ai_confidence_weight` values; assert blended scores differ.
3. `test_ai_analysis_disabled_yields_none_ai_score_and_blended_equals_static`:
   assert `ai_score` is `None` and `blended_score == static_score` when
   `ai_analysis_enabled = false`.

Additional tests:

- `SastScanPlugin::run` on a fixture workspace containing a known vulnerable
  pattern produces a `PluginOutput` whose Markdown report contains the finding's
  rule id and message.
- `SastTool::execute` with valid JSON params produces a `ToolResult` with
  non-empty `output` and `error = None`.
- `SastTool::execute` with an invalid path produces a `ToolResult` with
  `error = Some(...)`.
- `security_review` end-to-end test: the `sast_enabled = true` path produces
  SAST findings in the report alongside pattern-registry findings and without
  requiring an AI provider call.
- `SecurityReviewConfig` validation: invalid `sast_severity_threshold` returns a
  `Plugin` error.

#### 7.5 Deliverables

- `src/plugins/sast_scan/{mod,plugin,config,report,scoring}.rs`
- `src/tools/sast_tool.rs`
- `SastTool` registered in `src/tools/registry.rs`
- `SastScanPlugin` registered in the plugin registry
- `SecurityReviewConfig` extended with three `sast_*` fields
- `Config::sast_scan` field wired
- `docs/reference/confidence_scoring.md` updated with the `sast_scan` signal
  catalog
- `config.example.yaml` updated

#### 7.6 Success Criteria

| Criterion                             | Verification                                                             |
| ------------------------------------- | ------------------------------------------------------------------------ |
| All three scoring tests pass          | `src/plugins/AGENTS.md` required tests all pass                          |
| `SastScanPlugin` runs standalone      | Plugin fixture test produces a Markdown report with the expected finding |
| `SastTool` is callable from a session | Tool execute test returns a non-empty `ToolResult`                       |
| `security_review` SAST pre-pass works | End-to-end test produces SAST findings with no AI provider               |
| Boundaries are not violated           | `grep -rn 'use crate::clients' src/tools/sast_tool.rs` returns zero hits |

---

### Phase 8: Ruleset Acquisition and Licence Policy

#### 8.1 Feature Work

Build the acquisition layer in `src/clients/rulesets/`. This module handles
git/path/HTTPS fetch, licence verification, and content-addressed caching. It
must **not** import any type from `src/scanner/sast/`; it deals only in bytes,
paths, `serde_yaml::Value`, and its own `BundleManifest` type. A CI grep
enforces this.

**`src/clients/rulesets/sources.rs`**: Adopt the `semgrep-rules-manager`
manifest format for v1 parsing: source id -> `description`, `repository_url`,
`repository_branch`, `ignored`, `author`, `license`, `preprocessors`, plus
additive optional fields `enabled` (default `false`), `repository_commit`, and
`subdir`. Use `deny_unknown_fields` on the per-source struct; source ids
deserialize as `String` (a hex-like key such as `0xdea` must not silently coerce
to an integer).

**`src/clients/rulesets/source.rs`**: `RulesetSource` acquisition modes:

```rust
pub enum RulesetSource {
    Path(PathBuf),
    Git { url: Url, branch: String, commit: Option<String> },
    Url { url: Url, digest: Sha256Digest },
    Oci { reference: String, digest: Sha256Digest },
}
```

`Git` is the primary path (add `git2` to `Cargo.toml` in this phase). Clone
shallow at the pinned branch; resolve and record the commit SHA on every fetch.
Disable symlinks and set `--recurse-submodules` off.

**`src/clients/rulesets/fetch.rs`** and **`src/clients/rulesets/cache.rs`**:
Fetching is an explicit operation triggered only by `xzardgz ruleset sync`,
never a side effect of scanning. Content-addressed cache under
`dirs::cache_dir()/xzardgz/rulesets/<source_id>/<resolved_commit>/`.

**`src/clients/rulesets/license.rs`**: Read the fetched repository's actual
`LICENSE` or `COPYING` file plus a sample of rule `metadata.license` values.
Record both declared and observed licence. Classify into: `Permissive`,
`WeakCopyleft`, `StrongCopyleft`, `NetworkCopyleft`, `NonCommercial`,
`Restricted`, or `Unknown`. **Default policy**: allow `Permissive` and
`WeakCopyleft`; deny everything else, including `Unknown`. A denied ruleset is
skipped with a typed reason, never contributing findings silently.

**`src/clients/rulesets/manifest.rs`**: `BundleManifest` carrying source id,
resolved commit, declared and observed licence, content digest, and rule count.

**`xzardgz ruleset` CLI subcommand**:

| Command                           | Behaviour                                                                         |
| --------------------------------- | --------------------------------------------------------------------------------- |
| `xzardgz ruleset list`            | List resolved rulesets: id, rule count, licence class, source                     |
| `xzardgz ruleset sync [<source>]` | Fetch enabled sources, pin commits, verify licences, write `BundleManifest`       |
| `xzardgz ruleset doctor`          | Validate without scanning: unreachable URLs, missing branches, licence divergence |
| `xzardgz ruleset assess <source>` | Fetch, run the Phase 1 parser, report supported/gated split                       |

#### 8.2 Integrate Feature

Confirm the `src/clients/rulesets/` module compiles cleanly and that a CI-grep
over `use crate::scanner` within `src/clients/rulesets/` returns zero hits.
Confirm the search path in `src/scanner/sast/rule/resolve.rs` can load a ruleset
installed by `ruleset sync`.

#### 8.3 Configuration Updates

Add a `ruleset_sources_file` field to `SastEngineConfig` (optional `PathBuf`)
pointing at a user-maintained `sources.yaml` manifest. Add a
`assets/sources.example.yaml` under `src/clients/rulesets/` listing the 14
public sources (all with `enabled: false`) as a starting template.

#### 8.4 Testing Requirements

- Manifest round-trip test against `testdata/sast/semgrep_rules_sources.yaml` (a
  copy of the reference manifest checked in for test use).
- Non-string-key test: an unquoted `0xdea` key produces a typed parse error.
- Licence policy test: the default policy denies `NonCommercial` and
  `StrongCopyleft` sources and allows `Permissive` ones.
- Commons Clause test: the observed `Commons Clause` string classifies as
  `Restricted`, not `WeakCopyleft`.
- Staleness tests: a 404, a redirect, and a missing branch each warn and skip;
  none fail the command.
- Cache test: two syncs of the same commit perform only one fetch.
- Charter test: `grep -rn 'scanner::sast' src/clients/rulesets/` returns zero
  hits.

#### 8.5 Deliverables

- `src/clients/rulesets/{mod,sources,source,fetch,license,cache,manifest}.rs`
- `src/clients/rulesets/assets/sources.example.yaml`
- `git2` added to `Cargo.toml`
- Licence classification with deny-by-default policy
- `BundleManifest` with declared and observed licences and pinned commit SHA
- `xzardgz ruleset` subcommand with four sub-subcommands
- `docs/how-to/supply_sast_rulesets.md` (new document)

#### 8.6 Success Criteria

| Criterion                                   | Verification                                                |
| ------------------------------------------- | ----------------------------------------------------------- |
| Manifests parse unchanged                   | Manifest round-trip test passes                             |
| Engine/acquisition boundary not violated    | Charter grep exits non-zero                                 |
| Non-permissive rules cannot load by default | Default-policy test denies the expected sources             |
| Scans are reproducible                      | Commit-pinning test asserts a SHA in every `BundleManifest` |
| A stale manifest never fails a run          | Staleness tests all warn and skip                           |

---

### Phase 9: Conformance Testing

#### 9.1 Feature Work

Build a conformance harness that measures the engine's accuracy against the
semgrep-rules corpus's own test annotations. This phase does not add new engine
features; it measures correctness of everything built in Phases 1 through 8.

**`tests/sast_conformance.rs`**: For each rule in a developer-local
`semgrep-rules` checkout whose language is in the v1 set (Rust, `regex`,
`generic`), run the engine against the sibling fixture file containing
`# ruleid:` and `# ok:` annotation comments. Assert that findings land on the
annotated `# ruleid:` lines and that no finding fires on `# ok:` lines. Gate
entirely on the `sast-integration-tests` feature and the
`XZARDGZ_SEMGREP_RULES_DIR` environment variable; CI without the checkout passes
with an explicit skip message.

**`testdata/sast/conformance_baseline.json`**: A checked-in snapshot of the
harness result: one entry per rule, recording `pass`, `fail`, or
`skipped(<reason>)`. Regressions from `pass` to `fail` are treated as build
failures when the harness runs.

**Exit bar**: 90% or more of non-gated search-mode rules passing for the v1
language set. If the bar is not met initially, the failure distribution (most
likely from `...` to `$$$` rewrite edge cases) drives targeted Phase 2 fixes.

#### 9.2 Integrate Feature

Integrate the harness into the existing test suite. Confirm that running the
harness against the Phase 1 first-party rules (which are authored to pass the
engine) scores 100% before measuring against the external corpus.

#### 9.3 Configuration Updates

No config changes. The `XZARDGZ_SEMGREP_RULES_DIR` environment variable is the
only input.

#### 9.4 Testing Requirements

- Harness unit tests over synthetic fixtures: a matching rule that passes, a
  non-matching rule that fails, and a `# todoruleid:` annotation that neither
  passes nor fails the test.
- Baseline drift test: artificially regressing one rule from `pass` to `fail` in
  the baseline causes the test to fail.
- Skip-message test: with `XZARDGZ_SEMGREP_RULES_DIR` unset, the harness prints
  an explicit skip reason and the test still exits 0.

#### 9.5 Deliverables

- `tests/sast_conformance.rs` harness
- `testdata/sast/conformance_baseline.json` checked in
- Baseline drift detection wired as a test failure
- Measured pass rate per language recorded in this document (fill in after first
  run)
- `docs/explanation/sast_scanning_tool_plan.md` updated with measured pass rates

#### 9.6 Success Criteria

| Criterion                          | Verification                                          |
| ---------------------------------- | ----------------------------------------------------- |
| Conformance bar met                | 90% or more of non-gated v1-language rules pass       |
| Baseline is enforced               | An artificially regressed rule causes a test failure  |
| CI is unaffected by missing corpus | `cargo test --all-features` passes with env var unset |

---

## Worked Example

The following example exercises Phase 2 (formula evaluation), Phase 3
(metavariable-comparison and focus-metavariable), Phase 5 (SastMatch output),
and Phase 6 (builtin ruleset). It detects an RSA key generated with fewer than
2048 bits using the Rust `rsa` crate and is representative of the
`builtin:crypto` rules.

```yaml
rules:
  - id: rust-weak-rsa-key
    message:
      RSA keys must be at least 2048 bits. Keys shorter than 2048 bits are
      vulnerable to factoring attacks.
    languages: [rust]
    severity: WARNING
    metadata:
      cwe: ["CWE-326: Inadequate Encryption Strength"]
      owasp: ["A02:2021 - Cryptographic Failures"]
      confidence: HIGH
      category: security
      license: Apache-2.0
    patterns:
      - pattern: RsaPrivateKey::new(&mut $RNG, $BITS)
      - metavariable-comparison:
          metavariable: $BITS
          comparison: $BITS < 2048
      - focus-metavariable: [$BITS]
    fix: "2048"
```

Source input (`src/crypto/key_gen.rs` line 12):

```rust
let key = RsaPrivateKey::new(&mut rng, 1024)?;
```

Engine path:

1. **Phase 4 prefilter**: `RsaPrivateKey::new` is an aho-corasick literal; files
   not containing this string are never parsed.
2. **Phase 2 pattern compilation**: `$RNG` and `$BITS` are Semgrep
   metavariables; `...` is absent so no context-aware rewrite is needed.
3. **Phase 2 formula evaluation**: the single `Leaf` pattern matches the call
   site, binding `$RNG` to `rng` and `$BITS` to `1024`.
4. **Phase 3 metavariable-comparison**: `1024 < 2048` evaluates to `true`; the
   range survives.
5. **Phase 3 focus-metavariable**: the surviving range is narrowed to the byte
   range of `1024`.
6. **Phase 5 SastMatch construction**:

```rust
SastMatch {
    rule_id:       "builtin:crypto::rust-weak-rsa-key".to_string(),
    ruleset_id:    "builtin:crypto".to_string(),
    message:       "RSA keys must be at least 2048 bits. Keys shorter...".to_string(),
    severity:      Severity::Warning,
    confidence:    Confidence::High,
    path:          PathBuf::from("src/crypto/key_gen.rs"),
    start:         Position { line: 12, col: 38, byte: 382 },
    end:           Position { line: 12, col: 42, byte: 386 },
    snippet:       MatchSnippet { text: "1024".to_string(), context: "...".to_string() },
    metavariables: BTreeMap::from([
        ("$BITS".to_string(), MetavarBinding { text: "1024".to_string(), .. }),
        ("$RNG".to_string(),  MetavarBinding { text: "rng".to_string(),  .. }),
    ]),
    metadata:      RuleMetadata { cwe: vec!["CWE-326: ...".to_string()], .. },
    fingerprint:   "a3f2b1c8...".to_string(),
    fix:           Some("2048".to_string()),
}
```

1. **`fix` field**: recorded in `SastMatch.fix` and reported in the Markdown
   output. Never applied to the source file automatically.
2. **Phase 5 output**: SARIF `warning` result referencing CWE-326 in
   `properties.tags`; CycloneDX 1.7 vulnerability entry with CWE `326` in the
   `cwes` array.

A compliant call `RsaPrivateKey::new(&mut rng, 2048)` does not match because
`2048 < 2048` evaluates to `false`.

---

## Threat Model: Untrusted Rulesets

From Phase 8 onward, rule content is attacker-influenceable input once external
rulesets are enabled. The following table records each identified threat, its
mitigation, and the phase where the mitigation is implemented.

| Threat                                                   | Mitigation                                                                                                                           | Phase      |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ---------- |
| ReDoS via a rule-supplied regex                          | All rule regexes compiled through one builder with `size_limit` and `dfa_size_limit` at 10 MiB                                       | 1.1        |
| Pathological pattern causes unbounded scan time          | Per-rule-per-file timeout (`rule_timeout_ms`) and `max_matches_per_file` cap                                                         | 2.1        |
| A malicious `fix:` rewrites source code                  | `fix` is recorded in `SastMatch` and reported, never applied to any file                                                             | 5.1        |
| Path traversal via `subdir:` in a manifest               | `subdir` values are canonicalized and asserted to remain within the bundle directory; `..` components are rejected                   | 8.1        |
| Symlink escape from a cloned repository                  | Clone with symlinks disabled; symlinks pointing outside the bundle directory are rejected                                            | 8.1        |
| Git submodule fetching unexpected content                | `--recurse-submodules` disabled on all clone operations                                                                              | 8.1        |
| Branch force-push making scans irreproducible            | Commit SHA pinned and recorded per fetch; a changed SHA on re-sync is logged and reported                                            | 8.1        |
| Licence laundering (declared differs from actual)        | Licence verified against the fetched repository file, not the manifest; both values recorded; divergence is warned                   | 8.4        |
| Ruleset id collision hijacking another source's findings | Every rule id is namespaced by ruleset id                                                                                            | 6.1        |
| Rule content exfiltrating data                           | The engine (`src/scanner/sast/`) makes no network calls; `src/clients/rulesets/` touches the network only on explicit `ruleset sync` | Decision 1 |
| Enormous rule file exhausting memory                     | `SastEngineConfig::max_file_bytes` applies to rule files as well as scan targets                                                     | 1.1        |
| Metavariable-comparison used as a general `eval`         | `compare.rs` is a closed-grammar evaluator; anything outside the grammar produces `SkipReason::UnsupportedComparison`                | 3.1        |
| Recursive `metavariable-pattern` overflowing the stack   | Recursion depth limited to 10                                                                                                        | 3.1        |

---

## Risks

| Risk                                                                  | Likelihood | Impact   | Mitigation                                                                                                                            | Owning Phase |
| --------------------------------------------------------------------- | ---------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------- | ------------ |
| `...` to `$$$` rewrite is subtly wrong in some Rust syntactic context | High       | High     | Context-aware rewrite with one positive and one near-miss fixture per context; Phase 9 conformance baseline is the detector           | 2.1, 9       |
| Prefilter soundness failure causes silent false negatives             | Medium     | Critical | With/without differential test; no `Not` in the prefilter formula                                                                     | 4.1, 4.4     |
| Non-commercial or copyleft rules pulled in by default                 | High       | Critical | Deny-by-default licence policy including `Unknown`; default policy denies anything not `Permissive` or `WeakCopyleft`                 | 8.4          |
| Declared licence diverges from actual repository content              | High       | High     | Verify against the fetched repository file; record both; warn on divergence                                                           | 8.4          |
| Branch-tracked rulesets make scans irreproducible                     | High       | Medium   | Resolve and pin a commit SHA on every fetch                                                                                           | 8.1          |
| First-party ruleset coverage is insufficient for v1 crypto use cases  | Medium     | Medium   | Minimum rule counts enforced (15 crypto, 10 security); positive/negative fixtures required for each                                   | 6.1          |
| Single Rust grammar linked may miss xzardgz's own code patterns       | Low        | Low      | v1 is explicitly scoped to Rust + regex/generic; language expansion is a separate follow-on plan                                      | 0.1          |
| `ai_confidence_weight` misconfiguration silences valid findings       | Medium     | Medium   | Default `ai_confidence_weight = 0.3` with explicit documentation; required test asserts AI leg disabled yields static score unchanged | 7.1          |

---

## Dependency Changes

### Added

| Crate               | Version | Purpose                                                 | First Used |
| ------------------- | ------- | ------------------------------------------------------- | ---------- |
| `ast-grep-core`     | 0.45    | Tree-sitter matching, patterns, metavar bindings        | Phase 1    |
| `ast-grep-language` | 0.45    | Grammar bundle; only `tree-sitter-rust` feature enabled | Phase 1    |
| `aho-corasick`      | 1.x     | Multi-literal prefilter automaton                       | Phase 4    |
| `blake2`            | 0.10    | Stable match fingerprints                               | Phase 5    |
| `rayon`             | 1.x     | Parallel per-file scanning                              | Phase 4    |
| `ignore`            | 0.4     | Gitignore-aware directory walking                       | Phase 4    |
| `globset`           | 0.4     | `paths.include` / `paths.exclude` glob matching         | Phase 4    |
| `dirs`              | 5.x     | User cache/config directory resolution                  | Phase 6, 8 |
| `git2`              | 0.x     | Ruleset git clone and fetch                             | Phase 8    |
| `jsonschema` (dev)  | 0.x     | CycloneDX and SARIF golden-file schema validation       | Phase 0, 5 |

`ast-grep-config` is deliberately **not** added (Decision 4). `regex`, `serde`,
`serde_json`, `serde_yaml`, `thiserror`, `url`, and `tempfile` are already
dependencies and must not be re-added.

---

## Documentation Deliverables

| Document                                      | Change                                                                             | Phase |
| --------------------------------------------- | ---------------------------------------------------------------------------------- | ----- |
| `testdata/cyclonedx/README.md`                | New: CycloneDX 1.7 schema provenance, upstream URL, retrieval date                 | 0     |
| `testdata/sarif/README.md`                    | New: SARIF 2.1.0 schema provenance, upstream URL, retrieval date                   | 0     |
| `docs/reference/confidence_scoring.md`        | Add `sast_scan` signal catalog subsection per `src/plugins/AGENTS.md` requirements | 7     |
| `docs/reference/cli.md`                       | Document `xzardgz sast scan` and its flags                                         | 6     |
| `docs/reference/cli.md`                       | Document `xzardgz ruleset` subcommand                                              | 8     |
| `docs/reference/configuration.md`             | Document `sast:` and `sast_scan:` config keys with env vars                        | 6     |
| `docs/how-to/supply_sast_rulesets.md`         | New: search path, precedence, builtin fallback                                     | 6     |
| `docs/how-to/supply_sast_rulesets.md`         | Extend with acquisition, licence policy, `ruleset` subcommands                     | 8     |
| `README.md`                                   | Add SAST scanning to the feature list                                              | 6, 8  |
| `config.example.yaml`                         | Add documented `sast:` and `sast_scan:` blocks                                     | 6     |
| `docs/explanation/sast_scanning_tool_plan.md` | Fill in measured Phase 9 conformance pass rates                                    | 9     |

All Markdown files pass `markdownlint --fix --config .markdownlint.json` and
`prettier --write --parser markdown --prose-wrap always` before any phase is
considered complete.

---

## Deferred to Separate Plans

| Deferred Item                                                                           | Reason                                                                                                                                                                                           | Trigger to Revisit                                                                                                                                                                        |
| --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Taint mode** (`mode: taint`; sources, sinks, sanitizers, intraprocedural dataflow)    | Substantial enough to warrant a dedicated plan and review cycle. Full opengrep parity is a whole-program IL and fixpoint solver, approximately 24,650 lines of OCaml equivalent. See Decision 3. | This plan's search-mode engine (Phases 0 through 8) ships and proves out the shared AST layer a taint plan would build on. See `docs/explanation/sast_taint_mode_implementation_plan.md`. |
| **Deep expressions `<... ...>`, typed metavariables `(T $X)`, `metavariable-analysis`** | Combined approximately 5% of the corpus; deferred to keep this plan achievable.                                                                                                                  | Phase 9 conformance baseline reveals whether these constructs are the dominant source of remaining failures.                                                                              |
| **Language expansion** (Python, JavaScript/TypeScript, Go, Java, C, C++)                | v1 is scoped to Rust + regex/generic by Decision 3 to keep build time, binary size, and grammar maintenance tractable.                                                                           | After Phase 9 demonstrates search-mode correctness for Rust, language expansion is a natural follow-on.                                                                                   |
| **CBOM plugin**                                                                         | Needs its own crypto domain model plan; depends on `SastMatch` output from this plan.                                                                                                            | Phase 6 and 7 complete and first-party `builtin:crypto` rules proven out.                                                                                                                 |
| **PQC plugin**                                                                          | Depends on this plan's `SastMatch` output for crypto detection evidence.                                                                                                                         | Same trigger as CBOM.                                                                                                                                                                     |
| **OCI ruleset registry source** (`Oci` variant in `RulesetSource`)                      | `RulesetSource` can add this variant without redesign of the acquisition layer.                                                                                                                  | When a deployment requires pulling rulesets from an OCI registry.                                                                                                                         |
| **`pattern-inside` support**                                                            | 44.6% construct frequency; not trivially layered on top of range algebra. Deserves targeted design work.                                                                                         | After Phase 9 baseline shows what fraction of failures it explains.                                                                                                                       |

---

Last updated: 2026-09-10
