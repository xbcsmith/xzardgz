# SAST Scanning Tool Implementation Plan

## Overview

xzardgz's only structural-security scanning today is line-based
keyword/dependency/filename matching (`src/scanner/patterns.rs`) plus a
single AI completion call in `security_review`. Neither can express a real
static-analysis rule (a structural code pattern with metavariable capture,
matched against an actual parse tree). This plan adds a first-party,
from-scratch Rust static-analysis engine, inspired by Semgrep's rule model
and bundling a curated subset of the public `semgrep-rules` ruleset as its
default, exposed as a pluggable tool that `security_review` (and future
plugins) can call deterministically, with no AI provider required to
produce a finding.

This is explicitly not a Semgrep wrapper, FFI binding, or full
rule-syntax-compatible reimplementation — Semgrep's complete rule DSL
(taint mode, `pattern-inside`, `metavariable-regex`, join mode, and its
per-language matching semantics) is a multi-year engineering effort to
fully replicate. This plan scopes a genuinely useful, constrained subset
first, and treats broader coverage as explicit future work rather than
committing to unverified scope up front.

## Current State Analysis

### Existing Infrastructure

- `src/scanner/patterns.rs::PatternRegistry`/`PatternSet` provide
  keyword/dependency/filename detection — line-based, with no structural or
  AST awareness, but a useful complementary signal this plan does not
  replace.
- `src/scanner/preselect.rs::PluginContentScanner` already provides a
  file-selection and parallel-scanning pass that a new SAST engine can reuse
  rather than re-walking the repository tree independently.
- `src/tools/tool.rs`'s `ToolExecutor` trait and `src/tools/registry.rs`
  already model pluggable, sandboxed tools callable from an `AgentSession`
  (per the agent tool-calling integration plan), giving this engine a
  natural integration point as an on-demand tool as well as a deterministic
  pre-AI pass.

### Identified Issues

- No AST parsing infrastructure exists in xzardgz — no `tree-sitter`
  dependency, no per-language parser abstraction.
- No rule-loading mechanism for any YAML rule DSL exists.
- `security_review` has no deterministic, rule-driven static-analysis
  source; its only non-pattern-registry finding source is a single AI
  completion call.

## Implementation Phases

### Phase 1: Rule Model and YAML Loader (Constrained Subset)

#### 1.1 Foundation Work

Define the constrained subset of the Semgrep YAML rule schema this engine
supports initially: `id`, `languages`, `message`, `severity`, `pattern` /
`pattern-either` / `pattern-not`, and `$X`-style metavariable capture. Full
Semgrep parity — taint mode, `pattern-inside`, `metavariable-regex`, join
mode — is out of scope for this phase and tracked as a known limitation,
not a silent gap.

#### 1.2 Add Foundation Functionality

Implement `sast::rules::{Rule, RuleSet, load_rules_dir}`, parsing YAML rule
files into the constrained model above. A rule using an unsupported
construct is skipped with a logged warning rather than failing the entire
load, so a ruleset with some unsupported rules still loads everything it
can.

#### 1.3 Integrate Foundation Work

Vendor or fetch a curated starting subset of the public `semgrep-rules`
repository — scoped to the language(s) targeted in Phase 2, not the full
multi-language corpus — as this tool's default bundled ruleset.

#### 1.4 Testing Requirements

Unit tests loading a small hand-written rule set and asserting correct
parsing, plus a test asserting an unsupported construct is skipped with a
warning rather than aborting the load.

#### 1.5 Deliverables

A working YAML rule loader for the constrained rule subset, with a curated
default ruleset bundled or fetched at build/setup time.

#### 1.6 Success Criteria

Loading the full curated default ruleset produces zero hard failures; every
skipped or unsupported rule is logged and enumerable.

### Phase 2: AST-Based Structural Matching (Single Language)

#### 2.1 Feature Work

Add `tree-sitter` plus one language grammar — Rust, matching xzardgz's own
implementation language, as the first target — and implement a structural
pattern matcher: compile a rule's `pattern` into an AST-walk matcher
(directly, or via a tree-sitter query) that captures metavariable bindings.

#### 2.2 Integrate Feature

Wire the matcher into `sast::engine::scan_file(path, ruleset) -> Vec<SastFinding>`,
reusing `PluginContentScanner`'s existing file-selection and
parallel-scanning infrastructure rather than re-walking the repository tree
independently.

#### 2.3 Configuration Updates

Add a `sast` configuration block (enabled languages, ruleset path or
override, severity threshold) following the same configuration conventions
already used by `security_review`/`technical_review`.

#### 2.4 Testing Requirements

Golden-file tests: known-vulnerable code snippets matched against specific
rules from the curated ruleset, asserting exact match locations and
metavariable bindings. `semgrep-rules` ships positive/negative test
snippets alongside each rule; reuse these directly as fixtures rather than
authoring new ones.

#### 2.5 Deliverables

A working single-language structural SAST engine producing `SastFinding`s
with file, line, rule id, and severity.

#### 2.6 Success Criteria

The engine matches every positive fixture and does not fire on any negative
fixture in the curated ruleset's own bundled test snippets.

### Phase 3: Expose as a Pluggable Tool

#### 3.1 Feature Work

Implement `SastTool: ToolExecutor` so any plugin's tool-calling
`AgentSession` can invoke a SAST scan on demand, and add a direct,
non-agentic call path so `security_review` can run it deterministically as
part of its static-analysis pass — not gated behind AI tool-calling at all.

#### 3.2 Integrate Feature

Wire `SastFinding`s into `security_review`'s finding and scoring pipeline
(per the confidence scoring integration plan) as an additional deterministic
signal source, alongside pattern-registry hits and the OSV/SCA client (per
the external data clients plan).

#### 3.3 Configuration Updates

Expose `sast.enabled`, `sast.rulesets` (default plus operator-supplied
overrides), and a severity threshold through `SecurityReviewConfig`.

#### 3.4 Testing Requirements

An end-to-end `security_review` test asserting SAST findings appear in the
plugin's JSON, Markdown, and SARIF output alongside pattern-registry and
AI-sourced findings.

#### 3.5 Deliverables

A SAST scan step fully integrated into `security_review`'s report output.

#### 3.6 Success Criteria

Running `security_review` against a fixture repository containing a known
vulnerable pattern from the bundled ruleset produces the corresponding
finding with no AI provider call required.

### Phase 4: Broaden Language and Rule Coverage (Future Iteration)

#### 4.1 Feature Work (sketch only)

Add further tree-sitter grammars (Python, JavaScript/TypeScript, Go) and
expand the curated ruleset subset per language. Revisit which Phase-1
deferred Semgrep constructs (`pattern-inside`, `metavariable-regex`, taint
mode) are worth implementing, based on real false-negative/false-positive
experience gathered from Phases 1-3 in production use.

#### 4.2 Add Functionality (sketch only)

Not yet scoped.

#### 4.3 Integrate (sketch only)

Not yet scoped.

#### 4.4 Testing Requirements (sketch only)

Not yet scoped.

#### 4.5 Deliverables

None yet — this phase is a placeholder, to be fleshed out in a follow-up
planning pass once Phases 1-3 have real production usage to learn from.

#### 4.6 Success Criteria

Not applicable until this phase is re-scoped.
