# SAST Taint Mode Implementation Plan

## Overview

This plan adds intraprocedural taint tracking (`mode: taint`) to the xzardgz
SAST engine defined in
[`sast_scanning_tool_plan.md`](sast_scanning_tool_plan.md) (referred to below as
"the base plan"). Taint mode lets a rule author declare `pattern-sources`,
`pattern-sinks`, optional `pattern-sanitizers`, and optional
`pattern-propagators`, each carrying an optional label, so the engine reports a
finding only when tainted data reaches a sink through the data flow of a single
function or method body -- not merely when a source and a sink both happen to
appear somewhere in the same file.

The base plan's Decision 3 deferred taint mode entirely, citing opengrep's
approximately 24,650-line OCaml implementation (a three-address-code IL, a CFG
builder, and an interprocedural fixpoint solver with function signatures). This
plan does not attempt that scope. It reuses everything the base plan already
builds -- `rule::ir::Formula`, `engine::formula::evaluate`, the pattern
compiler, metavariable conditions -- to match sources, sinks, sanitizers, and
propagators exactly as search-mode rules are matched today, and adds a single
new capability on top: an intraprocedural, may-analysis dataflow pass that
decides which sink matches are reachable from which source matches within one
function body. There is no custom IL, no interprocedural call graph, and no
function-signature inference. Calls to functions defined elsewhere are treated
as an opaque, configurable pass-through (Decision T1). This keeps the new code
closer to 2,000-3,000 lines of Rust than tens of thousands.

**Scope:** Intraprocedural taint only, for Python, JavaScript, TypeScript, Java,
and Go in the first rollout (Phases 0-4), with Rust, C, and C++ added in Phase 5
once the shared dataflow core is proven. Rules with `options: interfile: true`
and any rule requiring cross-file resolution are always gated (out of scope for
this plan -- see Decision T1).

**Depends on:** The base plan's Phases 0-4 (dependencies, AST layer, rule IR,
range-algebra formula engine, metavariable conditions) must ship first, because
this plan reuses `rule::ir::Formula` and `engine::formula::evaluate` unchanged
to match each source/sink/sanitizer/propagator formula. This plan does not
depend on the base plan's Phase 5 (parallel targeting), Phase 6 (builtin
ruleset/CLI), Phase 8 (ruleset acquisition), or Phase 9 (conformance harness),
but references their conventions where noted.

**Taint mode is explicitly out of scope for the base plan.** That plan's
`rule/compat.rs` currently gates `mode: taint` unconditionally via
`SkipReason::TaintMode`. Phase 0 of this plan replaces that blanket gate with
the narrower, targeted gates defined in Decision T1 and T4.

## Current State Analysis

### Existing Infrastructure

After the base plan's Phases 0-4 complete, the following components exist and
are directly reused by this plan:

| Component                   | Location                               | Relevance                                                                            |
| --------------------------- | -------------------------------------- | ------------------------------------------------------------------------------------ |
| `Formula` enum              | `src/scanner/sast/rule/ir.rs`          | Compiled rule IR; taint source/sink/sanitizer/propagator patterns compile to Formula |
| `engine::formula::evaluate` | `src/scanner/sast/engine/formula.rs`   | Evaluates a Formula to `Vec<RangeWithMetavars>`; reused unchanged (Decision T2)      |
| `engine::pattern`           | `src/scanner/sast/engine/pattern.rs`   | Pattern compilation and caching; taint formulas use the same compilation path        |
| `RangeWithMetavars`         | `src/scanner/sast/engine/range.rs`     | `{ start, end, bindings }`; the unit of evidence taint mode adds reachability to     |
| `SkipReason::TaintMode`     | `src/scanner/sast/rule/compat.rs`      | Unconditional skip; replaced by narrower gates in Phase 0.3 of this plan             |
| `SastMatch`                 | `src/scanner/sast/match_model.rs`      | Neutral engine output; extended with `trace` field in Phase 3                        |
| `fingerprint::hash`         | `src/scanner/sast/fingerprint.rs`      | Blake2b fingerprint; extended to fold source+sink range pair in Phase 3              |
| `output/sarif.rs`           | `src/scanner/sast/output/sarif.rs`     | SARIF 2.1.0; extended with `codeFlows`/`threadFlows` in Phase 3                      |
| `output/cyclonedx.rs`       | `src/scanner/sast/output/cyclonedx.rs` | CycloneDX 1.7; extended with trace narrative in Phase 3                              |
| `SastEngine`                | `src/scanner/sast/mod.rs`              | Public facade; gains `TaintPass` dispatch in Phase 4 without new public methods      |
| `SastEngineConfig`          | `src/scanner/sast/config.rs`           | Engine-wide configuration; gains `taint: TaintConfig` field in Phase 4               |

### Identified Issues

1. `ast-grep-core` provides tree matching only. There is no control-flow graph,
   no dataflow framework, and no notion of "this expression flows into that
   one". Everything in this plan's Phases 1-2 is new code.
2. `RangeWithMetavars` is a byte-range algebra over a single parse tree. It has
   no notion of program order, branching, or loops. The reachability question --
   "does the value bound at this sink range depend on the value bound at that
   source range?" -- cannot be answered by the range algebra alone, hence the
   need for a CFG and a fixpoint pass.
3. The base plan's `rule/compat.rs` gates `mode: taint` unconditionally as
   `SkipReason::TaintMode`. This is intentional scaffolding, not a permanent
   state. Phase 0.3 of this plan replaces it.
4. No per-language statement-classification infrastructure exists anywhere in
   xzardgz. Identifying function/method boundaries, assignments, branches,
   loops, and early returns requires per-grammar tree-sitter node kind tables,
   one per target language.

## Research Summary

Research against the checked-out reference plans and local copies of `opengrep`,
`semgrep-rules`, and `ast-grep` v0.45.3:

- **`Rule.ml`'s `taint_spec` reuses `formula` verbatim.** `taint_source`,
  `taint_sink`, `taint_sanitizer`, and `taint_propagator` each carry a `formula`
  field alongside taint-specific metadata (`label`,
  `source_requires`/`sink_requires`/`propagator_requires`, `by_side_effect`,
  `exact`, `from`/`to` metavariables for propagators). This confirms that the
  base plan's `Formula`/`engine::formula` machinery is the correct, unmodified
  substrate for taint's pattern matching -- only the wrapper metadata and the
  reachability question are new.
- **Labels are a small boolean-formula problem, not a taint-specific one.**
  `Rule.precondition` (`PLabel | PVariable | PBool | PAnd | POr | PNot`) is
  evaluated by `Taint.solve_precondition` against the set of labels reaching a
  program point. `PVariable` exists only to support opengrep's interprocedural
  case. Because this plan is intraprocedural, `PVariable` never arises and the
  evaluator is a plain DNF-over-labels solver.
- **`Dataflow_tainting.ml`'s own header comment** describes the analysis as
  "rudimentary in some ways ... a MAY analysis, it finds potential bugs" with
  "no alias analysis" and lval tracking "limited to `x.a.b.c` ... very coarse
  grained otherwise (e.g. `x[i] = tainted` taints the whole array)". This plan
  adopts the same posture deliberately (Decision T5) -- opengrep's own
  production analysis makes the same trade-offs.
- **Real corpus rules confirm the shape needed.**
  `go/lang/security/injection/tainted-url-host.yaml` uses two labels (`INPUT`,
  `CLEAN`) where `CLEAN` `requires: INPUT` and the sink
  `requires: INPUT and not CLEAN` -- a same-file-only DNF over concrete labels.
  `java/lang/security/audit/formatted-sql-string.yaml` uses
  `pattern-propagators` (`StringBuilder.append`) plus
  `options: {taint_assume_safe_numbers: true, taint_assume_safe_booleans: true}`
  -- confirming that these per-rule options exist and are used in practice to
  suppress false positives from non-string-typed values.
- **`options.taint_assume_safe_{functions,numbers,booleans,indexes,comparisons}`**
  are rule-level booleans controlling whether an opaque call, or a value of a
  given type, is assumed untainted by default. These map directly onto Decision
  T1's call-handling heuristic and are implemented as ordinary rule options, not
  engine-wide flags.
- **`options: interfile: true`** appears on rules needing cross-file resolution.
  This plan gates any rule declaring `interfile: true` unconditionally --
  cross-file resolution is out of scope.
- **No CFG or dataflow utilities exist in `ast-grep`.** Function/method
  boundaries and statement structure must be identified per-language directly
  from each grammar's node kinds; there is no shared "statement" abstraction to
  build on.
- **Corpus taint-rule frequency.** Of the 2,095 rule files measured in the base
  plan, 12.5% use `mode: taint`. Of those, approximately 60% target Java, Go,
  Python, or JavaScript/TypeScript -- confirming that Phase 1's language
  selection covers the majority of the actionable corpus.

## Decisions

### Decision T1: Intraprocedural only; calls are opaque with configurable defaults

No call graph, no cross-function taint signatures, no interfile resolution. A
call to any function -- whether defined elsewhere in the same file or in an
external package -- is treated as an opaque node whose result inherits the union
of its tainted arguments' labels by default, mirroring opengrep's own default.
Rule authors suppress this per-rule via
`options.taint_assume_safe_functions: true`, exactly as in upstream
Semgrep/opengrep. Rules with `options: interfile: true` are always gated by
`rule/compat.rs` via `SkipReason::TaintInterfile`, independent of language
support. Same-file cross-function signature inference (so that calling
`sanitize()` defined elsewhere in the same file is understood without a
`pattern-sanitizers` entry) is listed in the Deferred to Separate Plans section.

### Decision T2: New `taint/` submodule reuses `Formula` engine unchanged

`src/scanner/sast/taint/` adds the CFG and dataflow layer only. Source, sink,
sanitizer, and propagator patterns compile to the same `rule::ir::Formula` and
execute through the same `engine::formula::evaluate`, unmodified, producing the
same `Vec<RangeWithMetavars>` that search-mode rules already produce. Taint
mode's only new question is: given these ranges, which sink ranges are reachable
from which source ranges, subject to sanitizers, propagators, and label
preconditions? Nothing in `taint/` re-implements pattern matching.

### Decision T3: Hand-rolled CFG, no graph crate

Per-function CFGs in this analysis are small (one function body at a time) and
need only successor edges and a worklist fixpoint -- not dominator trees,
shortest paths, or other features a general graph crate provides. `taint/cfg.rs`
defines a plain `Cfg { nodes: Vec<CfgNode>, successors: Vec<Vec<NodeId>> }` and
`taint/dataflow.rs` runs a manual worklist (`VecDeque<NodeId>`) to fixpoint.
This avoids a new dependency for a data structure that is straightforward to
hand-roll at this scale, consistent with the base plan's principle of adding
only dependencies that are hard to reimplement.

### Decision T4: Phased language rollout, shared core

The CFG builder's traversal and dataflow engine are language-agnostic; only a
per-language "statement classification" table (which node kinds are assignments,
branches, loops, calls, returns, and function boundaries) differs. Phase 1 ships
Python, JavaScript, TypeScript, Java, and Go -- the languages that dominate the
`mode: taint` corpus. Phase 5 adds Rust, C, and C++ behind the same
`rule/compat.rs` gate used for search mode's per-language support
(`SkipReason::TaintLanguageUnsupported`), so a rule targeting an unsupported
language is skipped precisely, never partially evaluated.

### Decision T5: Coarse-grained, depth-bounded lvals; no alias analysis

Tracked l-values are `{ base: variable name, offset: Vec<Offset> }` with
`Offset::Field(String) | Offset::Index` (all subscripts collapse to one wildcard
offset; no precise array-index tracking) and a fixed maximum depth of 4
(configurable via `SastEngineConfig`). Deeper access chains are truncated to the
base variable. No pointer or alias analysis: assigning one variable to another
is treated as a copy of its taint, not an alias. This matches opengrep's own
documented trade-off and is a deliberate scope decision, revisited only if Phase
5's conformance results show it is the dominant source of missed detections.

### Decision T6: Trace capture via SARIF `codeFlows`; CycloneDX gets rendered text

A taint finding without an explanation of why the sink is tainted is far less
actionable than a search-mode finding. `SastMatch` gains an additive
`trace: Option<Vec<TaintStep>>` field (Phase 3). SARIF 2.1.0's
`result.codeFlows`/`threadFlows` (SARIF spec section 3.38) is a natural fit and
is populated when `trace` is present. CycloneDX 1.7's `vulnerabilities[]` schema
has no equivalent multi-step-flow field, so the trace is rendered as an ordered
plain-text narrative appended to `analysis.detail` -- evidence only, not a new
schema extension.

## Module Structure

```text
src/scanner/sast/
    taint/                     <- NEW (this plan)
        mod.rs                 <- TaintPass facade: run(spec, root, path) -> Vec<SastMatch>
        schema.rs              <- serde types for pattern-sources/-sinks/-sanitizers/-propagators;
                                  label, requires, by-side-effect, exact, from/to fields
        ir.rs                  <- TaintSpec, TaintSource, TaintSink, TaintSanitizer,
                                  TaintPropagator (each wraps a rule::ir::Formula)
        requires.rs            <- Precondition (Label|And|Or|Not|Bool); recursive-descent
                                  parser for DNF text; fn solve(labels, p) -> bool
        options.rs             <- TaintOptions: assume_safe_{functions,numbers,booleans,
                                  indexes,comparisons}; all default false
        lval.rs                <- Lval { base, offset: Vec<Offset> }; depth-bounded
        lang/
            mod.rs             <- StmtTable trait: classify(node) -> StmtKind
            python.rs          <- Phase 1
            javascript.rs      <- Phase 1 (also covers JSX)
            typescript.rs      <- Phase 1 (also covers TSX)
            java.rs            <- Phase 1
            go.rs              <- Phase 1
            rust.rs            <- Phase 5
            c.rs               <- Phase 5
            cpp.rs             <- Phase 5
        cfg.rs                 <- CfgNode, Cfg, CfgBuilder: one CFG per function/method body
        dataflow.rs            <- TaintEnv (Lval -> LabelSet), forward worklist fixpoint,
                                  transfer functions per StmtKind
        trace.rs               <- TaintStep, TaintTrace: ordered source -> ... -> sink path
        engine.rs              <- TaintPass orchestration: partition ranges by function,
                                  build Cfg per function, run fixpoint, emit SastMatch

    rule/
        schema.rs              <- EXTEND: mode: search | taint; taint-only YAML fields
        ir.rs                  <- EXTEND: RuleIr gains Taint(TaintSpec) variant
        compat.rs              <- EXTEND: replace unconditional TaintMode skip with
                                  TaintInterfile / TaintLanguageUnsupported /
                                  TaintInvalidPropagator gates
    match_model.rs             <- EXTEND: SastMatch.trace: Option<Vec<TaintStep>>
    fingerprint.rs             <- EXTEND: fold (source_range + sink_range) into hash
    output/
        sarif.rs               <- EXTEND: codeFlows/threadFlows when trace present
        cyclonedx.rs           <- EXTEND: rendered trace narrative in analysis.detail
    config.rs                  <- EXTEND: SastEngineConfig gains taint: TaintConfig
    rules/
        taint/*.yaml           <- NEW: first-party taint fixture rules (Apache-2.0)
```

### Naming Constraints

| Constraint                                       | Reason                                                                                                                                                     |
| ------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `TaintPass`, not `TaintEngine`                   | `SastEngine` remains the single public facade; `TaintPass` is an internal collaborator it calls per rule/file, mirroring how `engine::formula` is internal |
| CFG types in `taint::cfg`, not `taint::dataflow` | Keeps graph structure (language-derived) separate from the analysis that walks it (language-independent)                                                   |
| `TaintStep`/`TaintTrace`, not `CallTrace`        | Avoids collision with opengrep's own `call_trace` terminology while remaining self-descriptive                                                             |
| `TaintSpec` in `taint::ir`, not `rule::ir`       | Taint IR is distinct from search-mode `Formula`; `rule::ir::RuleIr` references `TaintSpec` via a new variant                                               |

## Implementation Phases

Quality gate (every phase, per AGENTS.md Rule 4):

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Markdown files added or updated in each phase must pass:

```bash
markdownlint --fix --config .markdownlint.json "${FILE}"
prettier --write --parser markdown --prose-wrap always "${FILE}"
```

---

### Phase 0: Taint Rule Schema, IR, Labels, and Requires

#### 0.1 Foundation Work

Build the schema and serde layer that all later phases depend on. No behavioral
code is written in this phase; everything here is data modelling.

**`taint/schema.rs`** -- serde types for all taint-specific YAML fields. Every
struct derives `Debug`, `Clone`, `serde::Deserialize`, and `serde::Serialize`.
All optional fields use `#[serde(default)]`.

- `TaintSourceSchema`: `formula: FormulaSchema` (reusing the base plan's
  existing deserialization type), `label: Option<String>`,
  `requires: Option<String>` (raw DNF text, parsed in 0.2),
  `exact: Option<bool>` (default `false`),
  `by_side_effect: Option<BySideEffect>` (default `No`).
- `TaintSinkSchema`: `formula: FormulaSchema`, `label: Option<String>`,
  `requires: Option<String>`, `exact: Option<bool>` (default `true`, matching
  upstream defaults).
- `TaintSanitizerSchema`: `formula: FormulaSchema`, `label: Option<String>`,
  `by_side_effect: Option<bool>` (default `false`),
  `not_conflicting: Option<bool>`.
- `TaintPropagatorSchema`: `formula: FormulaSchema`, `from: String`,
  `to: String`, `label: Option<String>`, `replace_labels: Option<Vec<String>>`.
- `BySideEffect`: `#[serde(rename_all = "snake_case")]` enum with variants
  `Only`, `Yes`, `No`.

**`rule/schema.rs` extension** -- add `mode: Option<RuleMode>` (default
`Search`) where `RuleMode` is
`#[serde(rename_all = "lowercase")] enum RuleMode { Search, Taint }`. Add
taint-only fields `pattern_sources`, `pattern_sinks`, `pattern_sanitizers`,
`pattern_propagators` as `Option<Vec<...>>` using the schema types above. A rule
with `mode: taint` and missing `pattern_sources` or `pattern_sinks` is a parse
error in Phase 0.2.

#### 0.2 Add Foundation Functionality

**`taint/ir.rs`** -- the compiled intermediate representation:

```rust
/// Compiled taint specification for a single rule.
///
/// Produced by compiling a rule whose `mode` field is `RuleMode::Taint`.
/// All pattern matching reuses `rule::ir::Formula` and `engine::formula::evaluate`
/// unchanged (Decision T2). The taint engine adds only reachability analysis.
pub struct TaintSpec {
    pub sources: Vec<TaintSource>,
    pub sanitizers: Vec<TaintSanitizer>,
    pub sinks: Vec<TaintSink>,
    pub propagators: Vec<TaintPropagator>,
    pub options: TaintOptions,
}

/// A compiled taint source pattern with its label and precondition.
pub struct TaintSource {
    pub formula: Formula,           // reused from rule::ir, unmodified
    pub label: String,              // default: "__SOURCE__"
    pub requires: Precondition,     // default: Precondition::Bool(true)
    pub exact: bool,
    pub by_side_effect: BySideEffect,
}
```

`TaintSink`, `TaintSanitizer`, and `TaintPropagator` follow the same shape. The
default `sink.requires` is `Precondition::Label("__SOURCE__".to_owned())` so
unlabeled rules -- the common case -- work with no label bookkeeping at all.

Extend `rule/ir.rs`: `RuleIr` gains a `Taint(TaintSpec)` variant alongside the
existing `Search(Formula)` variant. The rule compiler in `rule/parse.rs` routes
to this new variant when `mode: taint` is present and the compat gate passes.

**`taint/requires.rs`** -- label preconditions:

```rust
/// A boolean precondition over taint labels, evaluated at a program point.
///
/// Corresponds to opengrep's `Rule.precondition` type, restricted to the
/// intraprocedural (no `PVariable`) subset.
pub enum Precondition {
    /// Satisfied when the named label is present in the current label set.
    Label(String),
    /// Always satisfied (`true`) or never satisfied (`false`).
    Bool(bool),
    /// Satisfied when all children are satisfied.
    And(Vec<Precondition>),
    /// Satisfied when any child is satisfied.
    Or(Vec<Precondition>),
    /// Satisfied when the child is not satisfied.
    Not(Box<Precondition>),
}
```

A small recursive-descent parser handles the Python-subset boolean syntax used
in `requires:` fields (`and` / `or` / `not` / parentheses / label identifiers;
operator precedence: `not` > `and` > `or`).
`pub fn solve(labels: &BTreeSet<String>, p: &Precondition) -> bool` evaluates a
`Precondition` against a label set. The parser must be bounded in recursion
depth (maximum 32 levels; deeper input returns
`Err(RequiresError::MaxDepthExceeded)`).

**`taint/options.rs`** -- per-rule taint options:

```rust
/// Per-rule options controlling taint propagation heuristics.
///
/// All fields default to `false` (conservative: assume tainted), parsed from
/// the rule's existing `options:` map. These are rule-level knobs, not
/// engine-wide configuration.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct TaintOptions {
    pub assume_safe_functions: bool,
    pub assume_safe_numbers: bool,
    pub assume_safe_booleans: bool,
    pub assume_safe_indexes: bool,
    pub assume_safe_comparisons: bool,
}
```

Parse `TaintOptions` from the rule's existing `options:` map in
`rule/schema.rs`; field names are `taint_assume_safe_functions`, etc. (matching
upstream naming).

#### 0.3 Integrate Foundation Work

**`rule/compat.rs` narrowing** -- replace the unconditional
`SkipReason::TaintMode` variant with three targeted variants and the
corresponding detection logic:

- `SkipReason::TaintInterfile` -- emitted when `options.interfile == true`,
  regardless of language. Cross-file resolution is out of scope for this plan
  and likely for xzardgz generally (see Decision T1).
- `SkipReason::TaintLanguageUnsupported` -- emitted when the rule's language is
  not yet in the taint-supported language set. In Phase 0 the set is empty;
  Phase 1 adds Python, JavaScript, TypeScript, Java, and Go; Phase 5 adds Rust,
  C, and C++.
- `SkipReason::TaintInvalidPropagator` -- emitted when a `pattern-propagators`
  entry's `from` or `to` metavariable name is not bound by the propagator's own
  formula. Caught at load time, zero runtime cost.

A taint rule that passes all three gates is compiled via the schema and IR code
from 0.1-0.2 and stored as `RuleIr::Taint(TaintSpec)`. It is never partially
evaluated: all three gates apply before any compilation begins.

**First-party taint fixture rules** -- author 4-6 rules under
`src/scanner/sast/rules/taint/` following the Apache-2.0 fixture style
established by the base plan. At a minimum:

- One unlabeled source-to-sink rule (SQL string concatenation in Python or Go).
- One labeled rule with a `requires` DNF expression (mirrors the `INPUT`/`CLEAN`
  pattern from the corpus, intrafile only).
- One rule with a `pattern-sanitizers` entry.
- One rule with a `pattern-propagators` entry (mirrors the Java
  `StringBuilder.append` shape but for a supported Phase-1 language).

These rules are not copied from `semgrep-rules`; they are authored in-house per
base plan Decision 2. They provide real targets for Phases 1-3.

#### 0.4 Configuration Updates

No user-visible configuration changes in this phase. `TaintOptions` is parsed
from the rule's `options:` block, which already has a YAML representation.
`TaintConfig` (the engine-wide DoS bounds) is defined and wired into
`SastEngineConfig` in Phase 4.

#### 0.5 Testing Requirements

- Schema round-trip tests for every new field, including all `pattern-sources`,
  `pattern-sinks`, `pattern-sanitizers`, and `pattern-propagators` shapes found
  in the corpus examples cited in the Research Summary. Each test deserializes a
  YAML snippet and re-serializes it, asserting structural equality.
- `requires.rs` parser: one test per operator (`and`, `or`, `not`), operator
  precedence (`not` > `and` > `or`), parenthesization,
  unknown-label-is-valid-syntax (labels are identifiers; solving happens at
  analysis time), and parse-error cases including the max-depth guard.
- `requires.rs` solver: truth-table coverage for `And`, `Or`, `Not`, `Label`,
  and `Bool` combinations. Include the `INPUT and not CLEAN` shape from the
  corpus.
- `rule/compat.rs`: one test per new `SkipReason` variant (TaintInterfile,
  TaintLanguageUnsupported, TaintInvalidPropagator). A complementary test proves
  a well-formed intrafile, supported-language taint rule is not skipped after
  Phase 1's language set is registered.
- `TaintOptions` defaults test: deserializing an empty `options:` map produces
  all-false options.

Test names follow `test_<function>_<condition>_<expected>`.

#### 0.6 Deliverables

- `src/scanner/sast/taint/{schema,ir,requires,options}.rs` with `///` doc
  comments on every public item
- `src/scanner/sast/rule/schema.rs` extended with `mode` field and taint-only
  schema types
- `src/scanner/sast/rule/ir.rs` extended with `RuleIr::Taint(TaintSpec)` variant
- `src/scanner/sast/rule/compat.rs` narrowed: three targeted `SkipReason`
  variants replace the unconditional `TaintMode` skip
- 4-6 first-party taint fixture rules under `src/scanner/sast/rules/taint/`
  (`.yaml` extension)
- `docs/explanation/sast_taint_mode_implementation_plan.md` (this document)

#### 0.7 Success Criteria

| Criterion                                       | Verification                                                    |
| ----------------------------------------------- | --------------------------------------------------------------- |
| All corpus taint field shapes deserialize       | Schema round-trip tests pass for every cited corpus example     |
| Label solver matches hand-computed truth tables | Unit tests pass for all boolean combinations                    |
| No taint rule is silently partially evaluated   | `compat.rs` gate test suite passes for all three variants       |
| Unconditional skip is gone                      | `grep -n 'TaintMode' src/scanner/sast/rule/compat.rs` returns 0 |

---

### Phase 1: L-values, Function Boundaries, and Per-Function CFG

#### 1.1 Feature Work

**`taint/lval.rs`** -- coarse-grained, depth-bounded l-values (Decision T5):

```rust
/// An l-value tracked by the taint dataflow engine.
///
/// Lvals are coarse-grained: all array subscripts collapse to `Offset::Index`
/// (no precise index tracking), and chains deeper than `max_depth` are
/// truncated to the base variable. This matches opengrep's own production
/// trade-off (Decision T5).
pub struct Lval {
    pub base: String,
    pub offset: Vec<Offset>,
}

/// A single component of an l-value access chain.
pub enum Offset {
    /// A named field or attribute access, e.g. `obj.field`.
    Field(String),
    /// Any subscript access, e.g. `arr[i]`. Index is not tracked precisely.
    Index,
}
```

`pub fn from_node(node: &Node<'_>, max_depth: usize) -> Option<Lval>` recognizes
identifier, member/field-access, and subscript expressions per language,
returning `None` for call expressions, literals, and binary expressions. A
`None` result means the expression is treated as an anonymous temporary (still
checkable against sink formulas, just not assignable to a named lval).

**`taint/lang/mod.rs`** -- the `StmtTable` trait:

```rust
/// Per-language mapping from tree-sitter node kinds to statement classifications.
///
/// Implement this trait for each supported language. The CFG builder calls
/// `classify` once per top-level child of a function body.
pub trait StmtTable {
    fn classify<'n>(&self, node: Node<'n>) -> StmtKind<'n>;
}
```

**`taint/lang/` per-language modules** (`python.rs`, `javascript.rs`,
`typescript.rs`, `java.rs`, `go.rs`) -- one `StmtTable` implementation each,
mapping tree-sitter node kind strings (confirmed against each grammar's
node-kind list bundled in `ast-grep-language`) to `StmtKind`. The `StmtKind`
enum is:

```rust
pub enum StmtKind<'n> {
    Assign { lhs: Node<'n>, rhs: Node<'n> },
    ExprStmt(Node<'n>),
    If { cond: Node<'n>, then_branch: Node<'n>, else_branch: Option<Node<'n>> },
    Loop { cond: Option<Node<'n>>, body: Node<'n> },
    TryCatch { try_block: Node<'n>, catch_blocks: Vec<Node<'n>>, finally: Option<Node<'n>> },
    Return(Option<Node<'n>>),
    Break,
    Continue,
    Block(Vec<Node<'n>>),
    FunctionBoundary { params: Vec<Node<'n>>, body: Node<'n> },
    Other(Node<'n>),
}
```

`switch`/`match` statements classify as a `Loop`-free multi-way `If` (each case
is an alternative branch, joined at exit) to reuse the existing join logic.
`Other` is an opaque pass-through for unrecognized constructs.

**`taint/cfg.rs`** -- the CFG builder:

```rust
/// A node in the control-flow graph, corresponding to one classified statement.
pub struct CfgNode {
    pub kind: StmtKindOwned,
    pub range: Range,
}

/// An intraprocedural control-flow graph for one function or method body.
///
/// Built by `CfgBuilder::build` from a `FunctionBoundary` node. One CFG is
/// built per function/method boundary found in the file (Decision T1).
pub struct Cfg {
    pub nodes: Vec<CfgNode>,
    pub successors: Vec<Vec<usize>>,
    pub entry: usize,
    pub exits: Vec<usize>,
}
```

`pub fn build(function_node: &Node<'_>, table: &dyn StmtTable) -> Result<Cfg, CfgError>`:

- `if`/`try` produce a branch node with two or more successors that reconverge
  at a synthetic join node.
- Loops produce a back-edge to the loop header. `break`/`continue` produce edges
  to the loop successor/header respectively.
- `return` produces an edge to the function exit, skipping any remaining
  statements (dead-code- after-return is simply unreachable in the graph, not
  flagged).
- Nested functions each produce their own independent `Cfg`; they are not
  embedded in the enclosing function's CFG (Decision T1).
- A function body that exceeds `max_cfg_nodes_per_function` (Phase 4) returns
  `Err(CfgError::TooLarge)`.

#### 1.2 Integrate Feature

Wire the per-language `StmtTable` implementations into `taint/lang/mod.rs` via a
`fn table_for_language(lang: Language) -> Option<Box<dyn StmtTable>>` factory.
Return `None` for languages not yet in the taint-supported set (Rust, C, C++
until Phase 5); the `compat.rs` gate (Phase 0.3) already prevents any rule for
those languages from reaching this code, so `None` here is a defensive assertion
rather than a reachable path.

Confirm `src/scanner/sast/taint/` does not import anything from `src/clients/`,
`src/tools/`, or `src/plugins/`, consistent with the base plan's
component-boundary contract.

No new language-grammar Cargo features are needed in this phase. The Phase-1
languages (Python, JavaScript, TypeScript, Java, Go) must be added to
`ast-grep-language`'s feature list in `Cargo.toml`. Verify
`cargo tree -p ast-grep-language` lists exactly the expected grammar crates.

#### 1.3 Configuration Updates

No user-visible configuration changes in this phase. The `max_depth` lval
parameter defaults to 4 and is wired into `TaintConfig` in Phase 4. The
taint-supported-language set is implicit in `table_for_language`; it is exposed
in configuration in Phase 4 via `TaintConfig::enabled`.

#### 1.4 Testing Requirements

- `Lval::from_node`: success for identifier, field access, and subscript
  expressions per Phase-1 language; `None` for call expressions, literals, and
  binary expressions; depth-truncation at the configured maximum.
- `StmtTable` per language: one test per `StmtKind` variant with a minimal
  tree-sitter fixture, plus one test per language verifying an unrecognized
  construct falls back to `Other`.
- `cfg::build` golden-graph tests (node count, edge adjacency list) for each of:
  straight-line code, `if`/`else`, `if` with no `else`, `while`/`for`,
  `try`/`catch`/`finally`, early `return` inside a branch, and nested functions
  producing independent graphs.
- Property test: every node in a built `Cfg` is reachable from `entry` or is
  provably unreachable dead code; no `successors` entry references an index
  outside `nodes`.
- Fuzz-lite test: `cfg::build` on a tree containing `ERROR` nodes (as produced
  by `ast/diagnostics.rs`) must not panic. `Other` classification is the
  expected fallback.

#### 1.5 Deliverables

- `src/scanner/sast/taint/lval.rs` with `///` doc comments
- `src/scanner/sast/taint/lang/{mod,python,javascript,typescript,java,go}.rs`
- `src/scanner/sast/taint/cfg.rs` with `CfgNode`, `Cfg`, `CfgBuilder`,
  `CfgError`
- Updated `Cargo.toml` feature list for `ast-grep-language` Phase-1 grammars
- Golden-graph test fixtures for all five Phase-1 languages

#### 1.6 Success Criteria

| Criterion                                                      | Verification                                                 |
| -------------------------------------------------------------- | ------------------------------------------------------------ |
| Every Phase-1 language produces well-formed CFGs for 0.6 rules | Golden-graph tests pass for all five languages               |
| No panics on partial-parse / ERROR-node input                  | Fuzz-lite test passes on deliberately malformed source trees |
| Depth-bounded lvals never exceed configured maximum            | Property test on adversarial deep-chain input passes         |
| Component boundary maintained                                  | `grep -rn 'use crate::clients' src/scanner/sast/` returns 0  |

---

### Phase 2: Dataflow Fixpoint Engine

#### 2.1 Feature Work

**`taint/dataflow.rs`** -- the taint analysis core.

The lattice and environment:

```rust
/// The set of taint labels present at an l-value or expression at a given
/// program point. Join is per-key set union.
pub type LabelSet = BTreeSet<String>;

/// The taint environment at a single program point: a map from l-values to
/// the set of taint labels they carry.
///
/// The join of two environments is computed per-key by set union (may-analysis).
pub struct TaintEnv(HashMap<Lval, LabelSet>);
```

`TaintEnv` forms a finite join-semilattice bounded by the total label vocabulary
of the rule, guaranteeing fixpoint termination. A `max_fixpoint_iterations`
safety cap (Phase 4) guards against implementation bugs, not lattice
non-termination.

Transfer functions for each `StmtKind`:

- `Assign { lhs, rhs }`: `env[lval(lhs)] = labels_of(rhs, env)`. For simple
  assignments, this is a strong update (replace, not union), sound only because
  there is no alias analysis (Decision T5). For compound assignments (`+=`,
  string concatenation forms), a weak update (union) is used instead.
- `ExprStmt` / `Other`: no environment change, but the node's range is still
  checked against sink formulas (see Seeding/Sanitizing/Propagating/Sinking
  below).
- `If` / `Loop` / `TryCatch` branch nodes: no direct transfer; join semantics
  apply at reconverging nodes (see below).
- `Return`: no environment change (intraprocedural; the returned value's taint
  is available to the CFG's exit finding logic, not propagated to callers).
- Call expressions (any `CfgNode` whose range's root is a call): result labels
  equal the union of argument labels, unless `options.assume_safe_functions` is
  `true` (then empty) or the call's range matches a `pattern-sanitizers` or
  `pattern-propagators` formula (sanitizer/propagator handling takes precedence,
  see below).
- Literal/typed values: if `assume_safe_numbers` is `true` and the node is
  syntactically a numeric literal, or `assume_safe_booleans` is `true` and it is
  a boolean literal, labels are empty for that expression.

Join at reconverging nodes: at any `Cfg` node with more than one predecessor
(loop headers, post-`if`/`try` joins),
`env = union over predecessors of env_pred_k`, per-key set union. This is a
may-analysis union, matching opengrep's own "MAY analysis" framing. Loops
iterate the whole loop body against the worklist until no `env` in the loop
changes (standard monotone-fixpoint iteration), then proceed past the loop exit
with the converged state.

Seeding, sanitizing, propagating, and sinking (in order per node):

1. **Seed**: if the node's range intersects a `pattern-sources` match, union
   `{source.label}` into the environment at the matched expression's lval, gated
   by `requires::solve(env_labels, source.requires)`. This supports the
   `CLEAN requires INPUT` corpus pattern.
2. **Sanitize**: if the node's range intersects a `pattern-sanitizers` match,
   remove the sanitizer's label (or all labels, if no label is specified) from
   the covered lval/expression. When `not_conflicting` is `true`, skip the
   removal if the same range also matches a source or sink formula.
3. **Propagate**: if the node's range intersects a `pattern-propagators` match,
   read labels at the `from` metavariable's bound range and union them
   (optionally relabeled per `propagator.label`/`replace_labels`) onto the `to`
   metavariable's lval.
4. **Sink**: if the node's range intersects a `pattern-sinks` match, evaluate
   `requires::solve(labels_at_focused_range, sink.requires)` and, if satisfied,
   record a candidate finding. Findings are computed on the final converged
   state only (not on intermediate iterations) to avoid duplicate reports.

Worklist fixpoint driver:

```rust
/// Run the forward taint analysis on `cfg` for the given `spec` and `options`.
///
/// Returns a list of sink ranges where `sink.requires` is satisfied by the
/// taint labels reaching that range at fixpoint. The caller (Phase 3's
/// `TaintPass`) synthesizes `SastMatch` values from these results.
///
/// # Errors
///
/// Returns `DataflowError::IterationLimitExceeded` if fixpoint does not
/// converge within `max_iterations`. The function is then omitted from taint
/// analysis; search-mode rules on the same file are unaffected.
pub fn analyze(
    cfg: &Cfg,
    spec: &TaintSpec,
    options: &TaintOptions,
    max_iterations: usize,
) -> Result<Vec<SinkFinding>, DataflowError>
```

Standard forward worklist: seed `entry` with an empty `TaintEnv`, process nodes
in a `VecDeque`, re-enqueue successors whenever a node's outgoing `TaintEnv`
changes, stop when the queue empties.

#### 2.2 Integrate Feature

Wire `taint/dataflow.rs` with `taint/cfg.rs`: `analyze` accepts a `&Cfg`
produced by `cfg::build`. Neither module imports from the other's internals; the
`Cfg` struct is the interface. Confirm that `dataflow::analyze` makes no calls
into `engine::formula::evaluate` directly -- formula evaluation is the
responsibility of `TaintPass::run` (Phase 3), which evaluates all source/
sink/sanitizer/propagator formulas once per file and passes the resulting
`Vec<RangeWithMetavars>` into `analyze` as pre-computed range sets.

#### 2.3 Configuration Updates

No user-visible configuration changes in this phase. `max_fixpoint_iterations`
and `max_cfg_nodes_per_function` are defined in `TaintConfig` (Phase 4) and
threaded into `analyze` via the call in `TaintPass::run` (Phase 3).

#### 2.4 Testing Requirements

- Straight-line propagation: source -> assignment chain -> sink, single path;
  asserts exactly one finding.
- Branch union (may-analysis): taint introduced in only one branch of an `if`
  still reaches a post-join sink; asserts a finding. Taint sanitized in one
  branch but not the other still reaches the sink (unsound-by-design, matches
  upstream's documented may-analysis posture; test asserts this is the observed
  behavior with a comment explaining the trade-off).
- Loop fixpoint: taint introduced inside a loop body reaches a sink after the
  loop. Convergence occurs in a bounded number of iterations for a fixture with
  a self-referential loop-carried variable.
- Sanitizer breaks the path: identical fixture without the sanitizer call
  produces a finding; with it, produces none.
- Propagator relabeling: `from`-labeled taint reaches a sink gated on the
  propagator's output label, not the input label.
- `requires` gating at the source: the `CLEAN requires INPUT` shape -- asserts
  `CLEAN` is only seeded when `INPUT` is already present at that program point.
- `assume_safe_*` options: each of the five options independently verified to
  suppress taint through its corresponding construct when `true`, and to allow
  propagation when `false` (the default).
- Fixpoint termination: a pathological fixture with nested loops and a growing
  label set converges within `max_fixpoint_iterations` rather than hanging.

Test names: `test_analyze_straight_line_source_to_sink_produces_finding`,
`test_analyze_sanitizer_in_path_suppresses_finding`, etc.

#### 2.5 Deliverables

- `src/scanner/sast/taint/dataflow.rs` with `LabelSet`, `TaintEnv`, transfer
  functions, join logic, the seeding/sanitizing/propagating/sinking pipeline,
  and `analyze`
- `src/scanner/sast/taint/dataflow.rs` -- `DataflowError` via `thiserror`
- All Phase-1 language CFG fixtures produce correct findings for the Phase 0.6
  fixture rules

#### 2.6 Success Criteria

| Criterion                                                   | Verification                                                               |
| ----------------------------------------------------------- | -------------------------------------------------------------------------- |
| All 0.6 fixture rules produce exactly the expected findings | End-to-end fixture suite passes for all five Phase-1 languages             |
| May-analysis semantics hold at joins                        | Branch-union and sanitizer-in-one-branch tests match documented behavior   |
| Fixpoint always terminates                                  | Pathological-nesting and loop tests complete within the iteration cap      |
| `analyze` never calls `engine::formula::evaluate`           | `grep -n 'formula::evaluate' src/scanner/sast/taint/dataflow.rs` returns 0 |

---

### Phase 3: Finding Synthesis, Trace Capture, and Output Projections

#### 3.1 Feature Work

**`taint/trace.rs`**:

```rust
/// A single step in a taint propagation path, from source to sink.
///
/// Steps are recorded during fixpoint analysis. The ordered sequence
/// constitutes the trace reported in SARIF `codeFlows` and CycloneDX
/// `analysis.detail`.
pub struct TaintStep {
    pub path: PathBuf,
    pub position: Position,   // reused from match_model.rs
    pub description: String,  // e.g. "assigned to `query`", "passed to `append(...)`"
}

/// An ordered taint trace from source to sink.
///
/// The first element is the source; the last is the sink. Intermediate steps
/// are propagation points (assignments, calls, propagator matches). When
/// multiple paths converge on one sink, the shortest chain is recorded,
/// matching opengrep's own shortest-trace tie-break.
pub type TaintTrace = Vec<TaintStep>;
```

Trace recording is performed during `dataflow::analyze` Phase 2: for each label,
record the node at which it was introduced (source), each node where it was
propagated or reassigned onto a new lval, and the sink node. On the final
converged state, select the shortest such chain for each source-to-sink pair.

**`taint/engine.rs`** -- the `TaintPass` facade:

```rust
/// The taint analysis pass for a single file and rule.
///
/// `TaintPass` is an internal collaborator of `SastEngine`. It is not part of
/// the public API. `SastEngine::scan` constructs and calls `TaintPass::run`
/// for each rule whose `RuleIr` is `RuleIr::Taint(_)`.
pub struct TaintPass;

impl TaintPass {
    /// Run the taint analysis for `spec` against the parsed file at `path`.
    ///
    /// Returns one `SastMatch` per source-to-sink pair where the sink's
    /// `requires` precondition is satisfied at fixpoint.
    pub fn run(
        &self,
        rule: &RuleIr,
        root: &AstGrepRoot,
        path: &Path,
        config: &TaintConfig,
    ) -> Result<Vec<SastMatch>, SastError>
}
```

`TaintPass::run` orchestration:

1. Evaluate all source, sink, sanitizer, and propagator formulas once per file
   via `engine::formula::evaluate` (reused unmodified, Decision T2), producing
   four `Vec<RangeWithMetavars>`.
2. Discover all `FunctionBoundary` nodes in the file (via the per-language
   `StmtTable`) to produce a list of function bodies.
3. For each function body, call `cfg::build` (Phase 1). If `cfg::build` returns
   `CfgError::TooLarge`, skip taint analysis for that function and emit a
   `SastError::TaintCfgTruncated` diagnostic. Search-mode rules on the same file
   are unaffected.
4. Partition each source/sink/sanitizer/propagator range into whichever function
   body contains it (by byte-range containment). Ranges outside any function
   body (module-level or top-level statements) form one implicit "file-level"
   CFG.
5. Call `dataflow::analyze` (Phase 2) for each `Cfg` with its partitioned range
   sets and the rule's `TaintOptions`.
6. For each `SinkFinding` from `analyze`, construct a `SastMatch` with `trace`
   populated from the captured `TaintTrace`.

**`match_model.rs` extension**: add `pub trace: Option<Vec<TaintStep>>` to
`SastMatch`. This field is additive; all existing search-mode construction sites
pass `None`. The `SastMatch` fingerprint computation must be updated (see
below).

**`fingerprint.rs` extension**: fold the source range's `(start, end)` and the
sink range's `(start, end)` into the blake2b hash alongside the existing fields,
so that two distinct source-to-sink pairs converging on the same sink line
produce distinct fingerprints. The full intermediate trace is not hashed (it is
UI-only).

**`output/sarif.rs` extension**: when `SastMatch.trace` is `Some`, emit
`result.codeFlows[0].threadFlows[0].locations` with one `threadFlowLocation` per
`TaintStep`, per SARIF 2.1.0 section 3.38. Each location includes
`location.physicalLocation.region` (line/column) and `location.message.text`
from `TaintStep.description`.

**`output/cyclonedx.rs` extension**: when `SastMatch.trace` is `Some`, append a
numbered plain-text narrative to `analysis.detail`. Example: "1. Tainted value
originates at `request.form.get('q')` (line 8) -> 2. assigned to `query`
(line 9) -> 3. reaches sink `cursor.execute(query)` (line 10)."

#### 3.2 Integrate Feature

Wire `TaintPass` as a dependency of `SastEngine::scan` (fully implemented in
Phase 4). In this phase, add integration points: confirm `TaintPass::run` is
callable from a unit test that bypasses `SastEngine`, so the end-to-end path can
be tested in isolation before full CLI integration.

Confirm all existing search-mode `SastMatch` construction sites compile cleanly
after the additive `trace` field is added (the field is `Option`, so
`..Default::default()` or explicit `trace: None` suffices at each site).

#### 3.3 Configuration Updates

No new user-visible configuration keys in this phase. `TaintConfig` (defined in
Phase 4) is the vehicle for all taint-related configuration. The `trace` field
on `SastMatch` is always populated when a taint finding is produced; there is no
user-visible flag to suppress it.

#### 3.4 Testing Requirements

- Trace shortest-path test: two source-to-sink paths of different length to the
  same sink; asserts the shorter chain is recorded in `SastMatch.trace`.
- Fingerprint distinctness test: two distinct source/sink pairs converging on
  the same sink line produce different fingerprints. The same pair scanned twice
  produces the same fingerprint (stability, matching base plan Phase 6.6).
- SARIF golden-file test with a populated `codeFlows` array, validated against
  the vendored SARIF 2.1.0 schema (base plan Phase 0).
- CycloneDX golden-file test asserting the rendered narrative appears in
  `analysis.detail` and the BOM validates against the vendored CycloneDX 1.7
  schema.
- Empty-trace defense test: a `SastMatch` with `trace: Some(vec![])` (should not
  occur in practice, but defensively tested) serializes to valid SARIF/CycloneDX
  with `codeFlows` and detail text omitted, not a panic.
- Regression test: all existing search-mode SARIF and CycloneDX golden files
  still pass after the `trace` field is added to `SastMatch` (search-mode
  findings have `trace: None`, so output is unchanged).

#### 3.5 Deliverables

- `src/scanner/sast/taint/{trace,engine}.rs` with `///` doc comments on all
  public items
- `src/scanner/sast/match_model.rs` extended with `trace` field
- `src/scanner/sast/fingerprint.rs` extended to fold source+sink range pair
- `src/scanner/sast/output/sarif.rs` extended with `codeFlows`/`threadFlows`
  emission
- `src/scanner/sast/output/cyclonedx.rs` extended with trace narrative
- SARIF and CycloneDX golden-file tests with taint trace coverage

#### 3.6 Success Criteria

| Criterion                            | Verification                                             |
| ------------------------------------ | -------------------------------------------------------- |
| SARIF taint output is spec-valid     | Schema validation passes with `codeFlows` populated      |
| CycloneDX taint output is spec-valid | Schema validation passes with rendered `analysis.detail` |
| Traces are minimal and stable        | Shortest-path and fingerprint-stability tests pass       |
| Search-mode output is unchanged      | All pre-existing SARIF/CycloneDX golden files still pass |

---

### Phase 4: Engine Integration, CLI, and DoS Bounding

#### 4.1 Feature Work

**`config.rs` extension** -- add `TaintConfig` to `SastEngineConfig`:

```rust
/// Configuration for the taint analysis pass.
///
/// Fields are read from the `sast.taint:` YAML block or `XZARDGZ_SAST_TAINT_*`
/// environment variables. All fields have conservative defaults that prevent
/// runaway analysis on pathological inputs.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct TaintConfig {
    /// Whether taint analysis is enabled. Defaults to `true`.
    pub enabled: bool,
    /// Maximum number of CFG nodes per function before taint analysis is skipped
    /// for that function. Default 2,000.
    pub max_cfg_nodes_per_function: usize,
    /// Maximum fixpoint iterations before analysis aborts for a function.
    /// Default 10,000.
    pub max_fixpoint_iterations: usize,
    /// Maximum l-value offset depth. Chains deeper than this are truncated to
    /// the base variable. Default 4 (Decision T5).
    pub max_lval_depth: usize,
}
```

Both `max_cfg_nodes_per_function` and `max_fixpoint_iterations` produce a
recorded diagnostic (`SastError::TaintCfgTruncated` and
`SastError::TaintFixpointAborted` respectively) rather than a hard error. A
function that exceeds either cap is skipped for taint analysis only; search-mode
rules still evaluate normally against the same file.

`SastEngineConfig` gains `pub taint: TaintConfig` (default instance). Add
corresponding env var support: `XZARDGZ_SAST_TAINT_ENABLED`,
`XZARDGZ_SAST_TAINT_MAX_CFG_NODES_PER_FUNCTION`,
`XZARDGZ_SAST_TAINT_MAX_FIXPOINT_ITERATIONS`,
`XZARDGZ_SAST_TAINT_MAX_LVAL_DEPTH`, following the `XZARDGZ_SAST_*` prefix
convention.

**`SastEngine::scan` dispatch** -- rules compiled to `RuleIr::Taint(spec)` are
routed to `TaintPass::run` (Phase 3) instead of `engine::formula::evaluate`
directly. `SastEngine` itself gains no new public methods, preserving the base
plan's `new`/`with_rules`/`scan` facade.

**`xzardgz sast scan` CLI extension** -- add `--taint <auto|on|off>` (default
`auto`, which respects `TaintConfig::enabled`). No new subcommand; taint and
search-mode rules from the same loaded ruleset run in one `xzardgz sast scan`
invocation. This mirrors the base plan's existing flag conventions (`--output`,
`--pretty`, `--jsonpath`).

**Threat model addendum** (taint-specific threats, extending base plan's Threat
Model section):

| Threat                                                                   | Mitigation                                                                       | Owning section |
| ------------------------------------------------------------------------ | -------------------------------------------------------------------------------- | -------------- |
| Deeply nested branches/loops cause CFG explosion                         | `max_cfg_nodes_per_function`; function skipped, scan continues                   | 4.1            |
| Adversarial loop-carried label growth stalls fixpoint                    | `max_fixpoint_iterations`; analysis aborts for that function, diagnostic emitted | 4.1            |
| `requires` DNF with deeply nested boolean operators                      | Recursion depth cap in `requires.rs` parser (Phase 0.2; max 32 levels)           | 0.2            |
| Propagator `to`/`from` metavariable never binds in the formula           | `compat.rs` gate rejects at load time (Phase 0.3), zero runtime cost             | 0.3            |
| Rule explosion: a ruleset with thousands of taint rules scans every file | Per-rule-per-file timeout (`rule_timeout_ms`) from base plan applies equally     | base plan 2.1  |

#### 4.2 Integrate Feature

Wire `TaintConfig` into `SastEngineConfig` and confirm `config.example.yaml` and
`docs/reference/configuration.md` are updated with the new `sast.taint:` block
and its env vars. Confirm `SastEngine::scan` routes correctly: load a mixed
ruleset (search + taint rules) and assert that findings from both modes are
present in the output, each with the correct `rule_id` namespace.

#### 4.3 Configuration Updates

- `src/scanner/sast/config.rs`: `TaintConfig` struct and
  `SastEngineConfig::taint` field
- `config.example.yaml`: add documented `sast.taint:` block
- `docs/reference/configuration.md`: document `sast.taint` keys and
  `XZARDGZ_SAST_TAINT_*` env vars
- `docs/reference/cli.md`: document `--taint` flag on `xzardgz sast scan`

#### 4.4 Testing Requirements

- `TaintConfig` defaults test: deserializing an empty `sast.taint:` block
  produces the expected default values.
- CFG node-count cap test: a synthetically oversized function body is skipped
  for taint analysis only; search-mode rules on the same file still produce
  their expected findings.
- Fixpoint iteration cap test: a pathological loop-carried-label fixture aborts
  within the cap and emits `SastError::TaintFixpointAborted`; the scan does not
  hang.
- CLI flag test: `--taint off` produces zero taint findings even when taint
  rules are loaded and sources/sinks match. `--taint on` with a ruleset
  containing only search-mode rules is a no-op.
- End-to-end test: `xzardgz sast scan` against a small multi-language fixture
  directory containing both search-mode and taint-mode rules produces the
  correct combined output in both SARIF and CycloneDX formats.

#### 4.5 Deliverables

- `src/scanner/sast/config.rs` `TaintConfig` implemented and wired into
  `SastEngineConfig`
- `SastEngine::scan` dispatch for `RuleIr::Taint` implemented
- `--taint` CLI flag implemented in `xzardgz sast scan`
- `config.example.yaml` `sast.taint:` block
- `docs/reference/configuration.md` and `docs/reference/cli.md` updated

#### 4.6 Success Criteria

| Criterion                                                  | Verification                                                        |
| ---------------------------------------------------------- | ------------------------------------------------------------------- |
| Oversized functions degrade gracefully, scan continues     | Cap tests pass; unaffected rules on the same file still run         |
| `sast scan` runs search and taint rules in one pass        | End-to-end multi-mode fixture test produces combined output         |
| CLI flag controls taint execution independently of loading | `--taint off`/`on`/`auto` tests all pass                            |
| Config documented                                          | `config.example.yaml` and `docs/reference/configuration.md` updated |

---

### Phase 5: Conformance Testing and Language Expansion

#### 5.1 Feature Work

**Taint-scoped conformance harness** -- extend the base plan's Phase 9
conformance harness (gated on `sast-integration-tests` Cargo feature and
`XZARDGZ_SEMGREP_RULES_DIR` environment variable) with a taint-mode filter. The
taint harness:

- Selects only rules with `mode: taint` from the corpus.
- Excludes rules with `options: interfile: true` (out of scope per Decision T1).
- Excludes rules targeting languages not yet in the taint-supported set.
- Evaluates each remaining rule against the corpus's adjacent test fixture file
  (the `<rule>.yaml` + `<rule>-test.<ext>` convention used throughout
  `semgrep-rules`).
- Classifies each result as `Pass`, `Fail(ExpectedFinding)`,
  `Fail(UnexpectedFinding)`, or `Skip(reason)`.
- Reports a per-language and per-category breakdown, not just an aggregate pass
  rate.

Per Phase 5.3 below, no numeric exit bar is set for this phase; the pass rate is
recorded as a baseline.

**`taint/lang/{rust,c,cpp}.rs`** -- three new `StmtTable` implementations
following the Phase 1 pattern exactly. The same `StmtKind` enum, the same
`cfg::build`, and no dataflow-engine changes are needed (Phase 2 is fully
language-agnostic). Extend `table_for_language` in `taint/lang/mod.rs` and
extend `rule/compat.rs`'s taint-supported-language set to include Rust, C, and
C++.

Note: Rust grammar support requires no new Cargo feature addition --
`tree-sitter-rust` is already enabled by the base plan's Phase 0. C and C++
grammars require adding `tree-sitter-c` and `tree-sitter-cpp` features to
`ast-grep-language` in `Cargo.toml`. Verify `cargo tree` output shows exactly
the expected grammars after these additions.

#### 5.2 Integrate Feature

Wire the three new `StmtTable` implementations into `table_for_language`. Run
the full taint conformance harness and record the baseline pass rate and failure
breakdown in `docs/explanation/sast_taint_mode_implementation_plan.md` (in a new
"Conformance Baseline" subsection appended after this plan is otherwise
complete). Each failure must be categorized:

- `Interfile` -- rule requires cross-file analysis (Decision T1 deferred;
  expected failure).
- `CrossFunction` -- finding requires same-file cross-function signature
  inference (deferred).
- `UnsupportedConstruct` -- rule uses `metavariable-analysis`, deep expressions,
  or typed metavariables (deferred by base plan Decision 3).
- `AliasAnalysis` -- finding requires alias tracking beyond Decision T5's lval
  model.
- `Bug` -- a genuine analysis error; must be fixed before the baseline is
  recorded.

Failures in the `Bug` category block the phase from being considered complete.

#### 5.3 Configuration Updates

No new user-visible configuration changes in this phase. The
taint-supported-language set is updated implicitly by extending
`table_for_language`. The two new grammar crate features added to `Cargo.toml`
are documented in the Dependency Changes section.

#### 5.4 Testing Requirements

- Golden-graph tests for Rust, C, and C++ `StmtTable` implementations, matching
  Phase 1.4's bar: one test per `StmtKind` variant per language, plus one
  unrecognized-construct-to-`Other` fallback test.
- Fuzz-lite test on Rust/C/C++ CFG builder with `ERROR`-node trees, matching
  Phase 1.4's pattern.
- Taint conformance harness run against the full taint-mode, non-interfile
  corpus subset.
- Regression: all Phase 1 language golden-graph tests and fixture-rule
  end-to-end tests continue to pass (no regression from the language expansion).

#### 5.5 Deliverables

- `src/scanner/sast/taint/lang/{rust,c,cpp}.rs` implemented
- `Cargo.toml` feature additions for `tree-sitter-c` and `tree-sitter-cpp`
- Taint conformance harness implemented and run
- Baseline pass rate and categorized failure breakdown recorded in this document

#### 5.6 Success Criteria

| Criterion                                              | Verification                                                             |
| ------------------------------------------------------ | ------------------------------------------------------------------------ |
| Rust/C/C++ CFGs are well-formed                        | Golden-graph and fuzz-lite tests pass, matching Phase 1's bar            |
| Conformance baseline is recorded with categorized gaps | Subsection present in this document with a failure breakdown by category |
| No regression in Phase-1 language behavior             | All Phase 1 language fixture and golden-graph tests still pass           |
| No `Bug`-category failures outstanding                 | Every identified bug is either fixed or documented with a tracking issue |

---

## Worked Example

The following example exercises Phase 0 (schema and label preconditions), Phase
1 (CFG construction for Go), Phase 2 (dataflow with two labels and a `requires`
DNF at the sink), Phase 3 (`TaintTrace` construction and SARIF `codeFlows`
emission), and Phase 4 (CLI invocation). It is a Go rule detecting an HTTP
request parameter reaching a URL construction sink without sanitization --
representative of the `mode: taint` corpus's dominant pattern for Go.

```yaml
rules:
  - id: go-tainted-url-host
    message: >
      HTTP request parameter reaches url.Parse without sanitization. An
      attacker-controlled URL may cause SSRF.
    languages: [go]
    severity: ERROR
    mode: taint
    metadata:
      cwe: ["CWE-918: Server-Side Request Forgery (SSRF)"]
      owasp: ["A10:2021 - Server-Side Request Forgery (SSRF)"]
      confidence: HIGH
      category: security
      license: Apache-2.0
    pattern-sources:
      - patterns:
          - pattern: $REQ.URL.Query().Get(...)
          - pattern: $REQ.FormValue(...)
        label: INPUT
      - patterns:
          - pattern: $X + $Y
          - metavariable-pattern:
              metavariable: $X
              pattern: $Z
        label: CONCAT
        requires: INPUT
    pattern-sanitizers:
      - patterns:
          - pattern: url.PathEscape(...)
          - pattern: url.QueryEscape(...)
    pattern-sinks:
      - patterns:
          - pattern: url.Parse($U)
          - focus-metavariable: [$U]
        requires: INPUT and not CONCAT
    options:
      taint_assume_safe_numbers: true
      taint_assume_safe_booleans: true
```

Source input (`handler.go`, lines 14-19):

```go
func handleRedirect(w http.ResponseWriter, r *http.Request) {
    target := r.URL.Query().Get("redirect")
    dest, err := url.Parse(target)
    if err != nil { http.Error(w, "bad url", 400); return }
    http.Redirect(w, r, dest.String(), 302)
}
```

Engine path through this plan's phases:

1. **Phase 0 schema/compat**: rule deserializes cleanly.
   `requires: "INPUT and not CONCAT"` parses to
   `Precondition::And([Label("INPUT"), Not(Label("CONCAT"))])`. Language Go is
   in the taint-supported set (Phase 1). No `interfile: true` flag. Rule
   compiles to `RuleIr::Taint(TaintSpec { ... })`.
2. **Phase 3 formula evaluation** (`TaintPass::run` calls
   `engine::formula::evaluate` for each formula set): source formula matches
   `r.URL.Query().Get("redirect")` at line 15, binding label `INPUT`; sink
   formula matches `url.Parse(target)` at line 16, focusing on `target`.
3. **Phase 1 CFG**: `CfgBuilder::build` identifies the `handleRedirect` function
   body and builds a straight-line CFG: `entry` ->
   `Assign { lhs: target, rhs: ...Get(...) }` ->
   `Assign { lhs: dest, rhs: url.Parse(target) }` -> `ExprStmt(http.Redirect)`
   -> `exit`.
4. **Phase 2 dataflow**: entry environment is empty. At the `target` assignment
   node, the range intersects the `INPUT` source formula;
   `solve(env_labels={}, Bool(true))` is satisfied; seed
   `env[target] = {INPUT}`. At the `url.Parse(target)` node, the range
   intersects the sink formula;
   `solve({INPUT}, And([Label("INPUT"), Not(Label("CONCAT"))]))` evaluates
   `INPUT in labels` = true and `CONCAT in labels` = false; precondition
   satisfied; record `SinkFinding`. No sanitizer range intersects any node in
   this path.
5. **Phase 3 trace**:
   `TaintTrace = [TaintStep { description: "assigned to`target`", line: 15 }, TaintStep { description: "reaches sink`url.Parse(target)`", line: 16 }]`.
6. **Phase 3 SastMatch construction**:

```rust
SastMatch {
    rule_id:    "go-tainted-url-host".to_string(),
    message:    "HTTP request parameter reaches url.Parse without sanitization...".to_string(),
    severity:   Severity::Error,
    confidence: Confidence::High,
    path:       PathBuf::from("handler.go"),
    start:      Position { line: 16, col: 14, byte: 291 },
    end:        Position { line: 16, col: 20, byte: 297 },
    snippet:    MatchSnippet { text: "target".to_string(), context: "...".to_string() },
    fingerprint: "b7d3a2f1...".to_string(),
    trace:      Some(vec![
        TaintStep { path: "handler.go", position: (15, 12), description: "...".to_string() },
        TaintStep { path: "handler.go", position: (16, 14), description: "...".to_string() },
    ]),
    ..
}
```

1. **Phase 3 SARIF output**: `result.codeFlows[0].threadFlows[0].locations`
   contains two entries, one per `TaintStep`, with `physicalLocation.region` and
   `message.text` populated.
2. **Phase 3 CycloneDX output**: `analysis.detail` appends: "1. Tainted value
   originates at `r.URL.Query().Get(\"redirect\")` (line 15) -> 2. reaches sink
   `url.Parse(target)` (line 16)."

A handler that sanitizes the input passes `url.QueryEscape(target)` to
`url.Parse`, which intersects the `pattern-sanitizers` formula; the sanitizer
removes label `INPUT` from `target` before the sink is reached, so `solve`
returns `false` and no finding is produced.

---

## Risks

| Risk                                                                      | Likelihood | Impact | Mitigation                                                                                                         | Owning Phase |
| ------------------------------------------------------------------------- | ---------- | ------ | ------------------------------------------------------------------------------------------------------------------ | ------------ |
| Per-language `StmtTable`s miss constructs, causing false negatives        | High       | Medium | Phase 5 conformance baseline surfaces gaps empirically; each miss is categorized                                   | 1, 5         |
| Strong-update-without-aliasing produces false negatives on aliased vars   | Medium     | Medium | Documented trade-off (Decision T5), matches opengrep's own posture; categorized in conformance failures            | 2.2          |
| May-analysis at branch joins over-reports false positives                 | Medium     | Low    | Documented and accepted; matches upstream's "MAY analysis" framing; test asserts the behavior explicitly           | 2.3          |
| Opaque call-through heuristic misses same-file sanitizer functions        | High       | Medium | `pattern-sanitizers`/`pattern-propagators` cover the common cases explicitly; same-file inference is deferred      | T1, Deferred |
| CFG builder complexity across 8 languages accumulates subtle bugs         | Medium     | High   | Golden-graph tests per language per construct; conformance harness as integration backstop                         | 1.4, 5.4     |
| Phase 1 language grammar features bloat binary size                       | Low        | Low    | Only the five Phase-1 grammar crates are added; `--no-default-features` remains in force per base plan Phase 0     | 1.2          |
| Trace recording during fixpoint increases memory usage on large functions | Medium     | Low    | `max_cfg_nodes_per_function` bounds function size; trace is per-finding, not retained for every intermediate state | 4.1          |

---

## Dependency Changes

No new runtime crates are required beyond those already added by the base plan.
The CFG (Decision T3) and `requires` DNF parser (Phase 0.2) are hand-rolled
against existing dependencies (`ast-grep-core` already provides the tree-sitter
node API).

The Phase-1 language grammars require extending the `ast-grep-language` feature
list in `Cargo.toml`:

```toml
ast-grep-language = { version = "0.45", default-features = false, features = [
    "tree-sitter-rust",        # base plan Phase 0
    "tree-sitter-python",      # taint Phase 1
    "tree-sitter-javascript",  # taint Phase 1
    "tree-sitter-typescript",  # taint Phase 1
    "tree-sitter-java",        # taint Phase 1
    "tree-sitter-go",          # taint Phase 1
] }
```

Phase 5 adds `tree-sitter-c` and `tree-sitter-cpp` to this list.

| Crate/feature            | Version  | Purpose                        | First Used    |
| ------------------------ | -------- | ------------------------------ | ------------- |
| `tree-sitter-python`     | via 0.45 | Python CFG and pattern grammar | Taint Phase 1 |
| `tree-sitter-javascript` | via 0.45 | JS/JSX grammar                 | Taint Phase 1 |
| `tree-sitter-typescript` | via 0.45 | TS/TSX grammar                 | Taint Phase 1 |
| `tree-sitter-java`       | via 0.45 | Java grammar                   | Taint Phase 1 |
| `tree-sitter-go`         | via 0.45 | Go grammar                     | Taint Phase 1 |
| `tree-sitter-c`          | via 0.45 | C grammar                      | Taint Phase 5 |
| `tree-sitter-cpp`        | via 0.45 | C++ grammar                    | Taint Phase 5 |

`petgraph` and any other graph crate are deliberately not added (Decision T3).
If Phase 5 or later experience shows the hand-rolled worklist is a maintenance
burden, a follow-up could introduce `petgraph`, but this plan does not adopt it
speculatively.

---

## Documentation Deliverables

| Document                                                  | Change                                                                                                                  | Phase |
| --------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | ----- |
| `docs/explanation/sast_taint_mode_implementation_plan.md` | New: this document                                                                                                      | 0     |
| `docs/reference/configuration.md`                         | Document `sast.taint:` config keys and `XZARDGZ_SAST_TAINT_*` env vars                                                  | 4     |
| `docs/reference/cli.md`                                   | Document `--taint` flag on `xzardgz sast scan`                                                                          | 4     |
| `docs/how-to/write_taint_rules.md`                        | New: authoring `pattern-sources`/`-sinks`/`-sanitizers`/`-propagators`, labels, `requires`, and `assume_safe_*` options | 3     |
| `config.example.yaml`                                     | Add documented `sast.taint:` block                                                                                      | 4     |
| `README.md`                                               | Note taint-mode support alongside the existing `sast scan` entry                                                        | 4     |
| `docs/explanation/sast_taint_mode_implementation_plan.md` | Append conformance baseline subsection with per-category failure breakdown                                              | 5     |

All Markdown files pass `markdownlint --fix --config .markdownlint.json` and
`prettier --write --parser markdown --prose-wrap always` before any phase is
considered complete.

---

## Deferred to Separate Plans

| Deferred Item                                                                       | Reason                                                                                                                        | Trigger to Revisit                                                                                      |
| ----------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Interprocedural / cross-file (`interfile: true`) taint tracking                     | Requires a call graph, function-taint signatures, and a settled story for full-repository checkout (Decision T1)              | A product decision on repository checkout, plus this plan's intraprocedural core proving out in Phase 5 |
| Same-file cross-function signature inference                                        | Requires computing and caching per-function taint summaries; substantial complexity this plan explicitly avoids (Decision T1) | Phase 5 conformance baseline shows this is the dominant non-interfile failure category                  |
| Alias analysis and precise array-index tracking                                     | Decision T5's coarse-grained lval model is deliberate; refinement is a substantial, separable effort                          | Phase 5 conformance baseline shows aliasing is the dominant failure category                            |
| `metavariable-analysis`, deep expressions `<... ...>`, typed metavariables `(T $X)` | Already deferred by the base plan's Decision 3 for search mode; nothing in this plan changes that gate                        | Same trigger as the base plan's Deferred Items                                                          |
| Constant propagation (folding a known constant through a tainted path)              | Interacts with lval precision and alias analysis; better addressed in a combined follow-on                                    | After alias analysis and interprocedural work are scoped                                                |

---

Last updated: 2026-09-10
