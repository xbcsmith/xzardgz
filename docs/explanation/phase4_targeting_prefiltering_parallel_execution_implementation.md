# Phase 4: Targeting, Prefiltering, and Parallel Execution Implementation

## Overview

Phase 4 adds the file-discovery infrastructure, an Aho-Corasick and regex-based
prefilter, and the `SastEngine` facade that parallelises scanning across
discovered files using Rayon. Together these three additions turn the formula
evaluator and condition evaluators from Phases 2 and 3 into a complete,
production-capable scan pipeline that can be aimed at a real source tree.

The phase depends directly on the Phase 2 `eval_formula` and `scan_rule` entry
points in `src/scanner/sast/engine/formula.rs`, on the Phase 3 condition
evaluators in `src/scanner/sast/engine/conditions.rs`, and on the `MetavarValue`
binding type introduced in Phase 3. No changes are made to any of those modules;
Phase 4 is purely additive.

## Architecture

Phase 4 introduces three new modules and wires them together in the existing
`SastEngine` facade.

- `src/scanner/sast/target/discover.rs` performs a gitignore-aware recursive
  directory walk, applies glob include and exclude filters, detects binary
  files, and enforces the maximum file size limit. It returns a sorted,
  deterministic `Vec<PathBuf>`.
- `src/scanner/sast/target/prefilter.rs` extracts literal strings and regex
  patterns from the compiled rule formulas and builds a single Aho-Corasick
  automaton and a `RegexSet` that can screen raw file bytes before any AST
  parser is invoked.
- `src/scanner/sast/mod.rs` gains the `SastEngine` struct, `SastScanReport`,
  `SastMatch`, and `SkippedRule` types. `SastEngine::scan` drives the entire
  pipeline and distributes work across a Rayon thread pool.

The full scan pipeline for a single invocation is:

```text
SastEngine::scan(root)
    |
    +-- discover_files()  -->  sorted Vec<PathBuf>
    |
    +-- Prefilter::from_rules()  -->  AhoCorasick + RegexSet
    |
    +-- rayon::par_iter() over files
            |
            +-- read file bytes
            +-- prefilter.file_may_match()  -->  skip if false
            +-- parse AST (Rust files only)
            +-- eval_formula() for each rule
            +-- RegexModeScanner for regex/generic rules
            |
            +-- collect SastMatch entries
    |
    +-- sort by (path, start, end, rule_id)
    +-- return SastScanReport
```

The prefilter sits between file discovery and AST parsing. It uses only raw
bytes, so it never pays the cost of tree-sitter parsing for files that cannot
possibly match any rule.

## File Discovery (`target/discover.rs`)

### DiscoveryConfig

`DiscoveryConfig` is a plain data struct that carries the parameters for a
single walk:

```rust
pub struct DiscoveryConfig {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub max_file_bytes: u64,
}
```

`include` and `exclude` hold glob patterns. An empty `include` list means
"accept every path that is not excluded". An empty `exclude` list means "do not
exclude anything extra beyond what gitignore specifies".

### Gitignore-Aware Walking

File discovery delegates to `ignore::WalkBuilder` from the `ignore` crate.
`WalkBuilder` automatically consults `.gitignore` files at every directory
level, the global gitignore file referenced by `core.excludesFile` in the user's
Git configuration, and the per-repository `.git/info/exclude` file. Hidden files
and the `.git` directory itself are skipped by default.

This means rules that exist in a developer's global `~/.gitignore` are respected
without any configuration, and generated directories such as `target/` or
`node_modules/` that appear in a project-level `.gitignore` are never offered to
the scanner.

### Binary File Detection

Before a file is added to the candidate list, its first 8,192 bytes are read and
scanned for a null byte (`0x00`). Any file that contains a null byte in its
opening window is classified as binary and silently omitted. This heuristic
matches the behaviour used by `git diff` and
`grep --binary-files=without-match`. The 8,192-byte probe is small enough to be
fast even for large files and is sufficient to detect virtually all binary
formats in practice.

Binary detection prevents tree-sitter and the regex engine from receiving inputs
they were not designed to parse. Both parsers accept arbitrary bytes, but binary
content produces meaningless AST structures and inflates match counts with false
positives.

### Maximum File Size Enforcement

File size is checked via the `DirEntry` metadata returned by
`ignore::WalkBuilder`. The metadata call is O(1) and does not require reading
the file. Any entry whose reported size exceeds
`SastEngineConfig::max_file_bytes` is skipped and counted in
`SastScanReport::skipped_file_count`. The default limit is 5 MiB. This prevents
a single unusually large generated file from consuming a disproportionate share
of memory or scan time.

### Glob Include and Exclude Matching

`DiscoveryConfig::include` and `DiscoveryConfig::exclude` patterns are compiled
into a `globset::GlobSet` during `SastEngine::new`. At walk time, each
`DirEntry` path is made relative to the scan root before matching, so patterns
such as `src/**/*.rs` work regardless of the absolute path of the scan root.

Matching order is: if a non-empty `include` list is present and the relative
path does not match any include pattern, the file is skipped. If the relative
path matches any exclude pattern, the file is skipped. A file must survive both
checks before it is added to the candidate list.

### Sorted Output

`discover_files` collects all surviving paths into a `Vec<PathBuf>` and calls
`sort` before returning. Sorting guarantees that two invocations over the same
directory tree produce files in the same order regardless of the order in which
the operating system returns directory entries. Deterministic ordering is a
prerequisite for the determinism guarantee on the final `SastScanReport`.

## Prefilter (`target/prefilter.rs`)

### Soundness Invariant

The prefilter upholds a strict soundness invariant: if
`Prefilter::file_may_match` returns `false` for a given file, then no rule in
the engine can possibly produce a match for that file. The converse is not
guaranteed. The prefilter may return `true` for files that ultimately produce no
matches; this is a false positive and is acceptable because it means the file
will be parsed and evaluated normally. The only thing the prefilter must never
do is return `false` for a file that would have produced a match. That would be
a silent miss.

### Literal Extraction

To build the Aho-Corasick automaton, the prefilter walks each compiled `Formula`
tree and extracts literal string tokens from `Leaf::Pattern` nodes. A pattern
string is tokenised by splitting on metavariable syntax (`$NAME`, `$$$NAME`,
`...`). The non-metavariable segments that remain after splitting are the
literals. For example, the pattern `RsaPrivateKey::new(&mut $RNG, $BITS)` yields
the literal `RsaPrivateKey::new(&mut` and `, )` after splitting on `$RNG` and
`$BITS`.

Only segments of three or more characters are retained to avoid inserting
single-character or two-character fragments that would match almost every file.

### No Negations in the Prefilter

Negation clauses in a `Formula::And` node are never used to derive prefilter
predicates. Including a negation would require that a file be excluded when it
contains the negated literal. That logic is unsound: a file could contain both
the negated literal and a positive match for the main pattern. The negation
would then cause the prefilter to skip the file, producing a silent miss.
Instead, negations are silently ignored during predicate extraction, which means
files that could only be excluded by the negation clause are still parsed and
evaluated normally.

### The `always_analyze` Flag

When a rule's formula contains no extractable literals at all, for example
because the entire pattern is a bare metavariable `$X`, the prefilter cannot
make a soundness claim about that rule. In this case the `always_analyze` flag
on the `Prefilter` struct is set to `true`. When `always_analyze` is true,
`file_may_match` returns `true` unconditionally for every file, effectively
disabling prefiltering for the entire engine. This is safe and correct, though
it means scans that include very general rules cannot benefit from prefiltering.

Rules that produce no literals are expected to be rare in practice. The common
case is that patterns contain at least one function name or type name literal
that narrows the candidate set significantly.

### Aho-Corasick and RegexSet

Literals from all rules are merged into a single set and compiled into one
`aho_corasick::AhoCorasick` automaton using the `aho-corasick = "1"` crate.
Searching for all literals simultaneously in a single pass over the file bytes
is O(n) in the file size, regardless of the number of literals, making this far
cheaper than running a separate `contains` check for each literal.

For rules that carry `Leaf::Regex` leaves, the pattern strings are collected and
compiled into a `regex::RegexSet`. A `RegexSet` determines whether any of its
constituent patterns match a string in a single pass and is substantially faster
than testing each pattern individually.

`file_may_match` runs the Aho-Corasick search first. If any literal matches, the
function returns `true` immediately without running the `RegexSet`. If no
literal matches, the `RegexSet` is run. If any regex pattern matches, the
function returns `true`. Only if neither search finds a match does the function
return `false`.

### Differential Test as Primary Correctness Guarantee

Because the soundness invariant cannot be verified by inspection alone, the
primary correctness guarantee comes from a differential test that runs the full
engine against a fixture corpus twice: once with prefiltering enabled and once
with prefiltering disabled. The test asserts that both runs produce
byte-identical finding sets. If the prefilter ever silently drops a file that
would have produced a match, the differential test will catch it.

## SastEngine Facade (`scanner/sast/mod.rs`)

### SastEngine Struct

`SastEngine` is the public entry point for all scanning. It holds:

- A `SastEngineConfig` for runtime limits and thread-pool sizing.
- A `Vec<RuleIr>` of compiled rules loaded via `with_rules`.
- Compiled `GlobSet` values for include and exclude patterns derived from the
  config.
- A `PatternCompiler` shared across all worker threads.

`SastEngine` is `Send + Sync` and is safe to share across threads. The
underlying `PatternCompiler` uses interior mutability with a lock-free cache.

### Public Methods

`SastEngine::new(config: SastEngineConfig) -> Result<Self, SastError>`
constructs the engine, compiles the include and exclude glob sets, and validates
the configuration. It returns an error if any glob pattern is syntactically
invalid.

`SastEngine::with_rules(rules: Vec<RuleIr>) -> Result<(), SastError>` installs a
rule set. Rules that fail to build their `RegexModeScanner` or whose formulas
are structurally unsupported are moved to the `skipped_rules` list inside the
engine rather than causing a hard error. This matches the Phase 1 and Phase 2
convention that individual rule failures should not prevent scanning with the
remaining rules.

`SastEngine::scan(root: &Path) -> Result<SastScanReport, SastError>` drives the
full pipeline: file discovery, prefilter construction, parallel evaluation,
result collection, and sorting.

### SastScanReport

`SastScanReport` is the return value of a completed scan:

```rust
pub struct SastScanReport {
    pub matches: Vec<SastMatch>,
    pub skipped_rules: Vec<SkippedRule>,
    pub scanned_file_count: usize,
    pub skipped_file_count: usize,
    pub parse_error_count: usize,
    pub truncated_rule_file_pairs: usize,
    pub duration_ms: u64,
}
```

`matches` contains every finding produced by any rule across all scanned files.
`skipped_rules` lists rules that could not be initialised, together with the
reason for skipping. `scanned_file_count` counts files that passed the prefilter
and were fully evaluated. `skipped_file_count` counts files that were excluded
by size, binary detection, glob filters, or the prefilter. `parse_error_count`
counts files where the AST parser returned a high error-node density and the
file was not evaluated against AST-mode rules. `truncated_rule_file_pairs`
counts `(file, rule)` pairs where `eval_formula` returned a `TruncationReason`.

### SastMatch

The `SastMatch` type in Phase 4 is intentionally minimal:

```rust
pub struct SastMatch {
    pub rule_id: String,
    pub path: PathBuf,
    pub start: usize,
    pub end: usize,
}
```

`rule_id` identifies which rule produced the match. `path` is the absolute path
of the file. `start` and `end` are byte offsets in that file (half-open interval
`[start, end)`).

Phase 5 will enrich this type with `message`, `severity`, `confidence`,
`snippet`, `metavariables`, `metadata`, `fingerprint`, and `fix` fields. The
Phase 4 representation is sufficient for testing correctness of discovery,
prefiltering, and parallel evaluation.

### SkippedRule

```rust
pub struct SkippedRule {
    pub rule_id: String,
    pub reason: String,
}
```

A `SkippedRule` record is created whenever a rule fails initialisation during
`with_rules`. The `reason` string contains the human-readable error message from
the failing initialisation step. Callers can inspect the `skipped_rules` field
of `SastScanReport` to audit which rules were not applied and why.

### Rayon Parallelism

`SastEngine::scan` builds a Rayon thread pool before beginning the scan. The
thread count is controlled by `SastEngineConfig::jobs`:

- `jobs = 0`: the engine calls `std::thread::available_parallelism` and uses the
  reported logical CPU count as the thread count. This is the default and is
  suitable for most deployments.
- `jobs > 0`: the engine creates a pool with exactly that many threads. This is
  useful for constraining resource usage in environments where parallelism must
  be bounded explicitly, such as shared CI runners.

The thread pool is created with
`rayon::ThreadPoolBuilder::new().num_threads(n).build()` and is scoped to the
single `scan` call. It does not affect the global Rayon thread pool used by
other parts of the binary.

Files are processed via `par_iter` over the discovered `Vec<PathBuf>`. Each file
is processed independently, producing a `Vec<SastMatch>` that is collected into
the final result via a Rayon `flat_map`.

### Per-File Panic Isolation

The body of each per-file closure is wrapped in `std::panic::catch_unwind` with
`AssertUnwindSafe`. If a file causes a panic inside tree-sitter, the formula
evaluator, or any other component, the panic is caught and converted into a
per-file error record. The file is counted in `parse_error_count` and scanning
continues on all remaining files. One malformed or adversarial file cannot abort
the scan and cause findings from other files to be lost.

`AssertUnwindSafe` is used because all data accessed inside the closure is
either owned by the closure or is shared read-only through `Arc`. No shared
mutable state crosses the unwind boundary.

### Result Ordering

After all files are processed, `matches` is sorted by
`(path, start, end, rule_id)`. Rayon does not preserve insertion order across
parallel iterators, so the sort is mandatory for deterministic output. Sorting
by path first groups all findings for the same file together. Sorting by `start`
and `end` within a file orders findings by source position. Sorting by `rule_id`
last provides a stable tie-breaker when two rules produce a finding at the same
byte range.

## Testing

### Discovery Tests

Discovery tests use `tempfile::TempDir` to construct isolated directory trees
without touching the real filesystem. Each test creates a known set of files and
directories, invokes `discover_files`, and asserts that the returned path list
matches the expected set.

Specific discovery scenarios covered:

- A `.gitignore` entry that excludes `target/` causes files inside `target/` to
  be omitted.
- A file containing a null byte in its first 8,192 bytes is classified as binary
  and omitted.
- A file larger than `max_file_bytes` is omitted and the skipped count is
  incremented.
- A `paths.include` pattern of `src/**/*.rs` causes non-Rust and non-`src/`
  files to be omitted.
- A `paths.exclude` pattern of `**/generated.rs` causes matching files to be
  omitted.
- Results are returned in sorted order regardless of the walk order.

### Prefilter Soundness Differential Test

The differential test is the most important correctness test in Phase 4. It
takes a fixture corpus and a set of rules and runs the full `SastEngine::scan`
pipeline twice: once with prefiltering enabled and once with a modified engine
that always returns `true` from `file_may_match`. The test then asserts:

- Both runs produce identical `Vec<SastMatch>` values after sorting.
- The prefiltered run reports a lower or equal `scanned_file_count` than the
  unfiltered run, confirming that the prefilter is actually excluding files.
- The unfiltered run's `scanned_file_count` equals the total number of
  non-binary, non-oversized files in the fixture corpus.

### Panic-Isolation Test

The panic-isolation test creates a temporary directory containing several valid
Rust fixture files alongside one synthetic file whose content is designed to
trigger a panic inside the evaluation closure. The test runs `SastEngine::scan`
over that directory and asserts:

- The scan completes without propagating a panic to the caller.
- Findings from the valid files are present in `SastScanReport::matches`.
- The synthetic file's failure is reflected in `parse_error_count` rather than
  an `Err` return.

### Determinism Test

The determinism test runs `SastEngine::scan` twice over the same fixture
directory with the same configuration and asserts that both
`SastScanReport::matches` vectors are identical. This confirms that Rayon's
non-deterministic scheduling is fully neutralised by the post-scan sort.

### File Count Tracking

The test suite includes assertions on all three file count fields of
`SastScanReport`:

- `scanned_file_count` must equal the number of files that passed all filters
  and were evaluated.
- `skipped_file_count` must equal the number of files excluded by binary
  detection, size limits, glob filters, or the prefilter.
- `parse_error_count` must equal the number of files where AST parsing failed at
  the error-node density threshold.

## Security Notes

- Binary file detection prevents tree-sitter and the regex engine from receiving
  binary blobs. Feeding binary content to tree-sitter can produce deeply nested
  error nodes that exhaust stack space or produce meaningless but
  expensive-to-evaluate AST structures.
- `max_file_bytes` is enforced before the file is read into memory. A repository
  that contains a large binary that was not caught by gitignore, or a generated
  file that was accidentally committed, cannot cause the scanner to allocate an
  unbounded amount of memory.
- The prefilter soundness invariant is conservative: it can only skip files,
  never suppress findings from files that are evaluated. A bug in the prefilter
  can cause false negatives in file selection, but the differential test is
  designed to catch exactly that class of bug.
- The Rayon thread pool is bounded by `SastEngineConfig::jobs`. On shared
  infrastructure, setting `jobs` to a small positive value prevents the scanner
  from consuming all available CPU cores and starving other processes.
- Panic isolation via `catch_unwind` prevents a single malformed file from
  crashing the binary and losing all findings collected up to that point.

## Related Phases

Phase 2 provided `eval_formula` and `scan_rule`, which are the core evaluation
primitives consumed by `SastEngine::scan` for each rule applied to each file.
That implementation is described in
`docs/explanation/phase2_sast_formula_engine_implementation.md`.

Phase 3 provided the metavariable condition evaluators and the `MetavarValue`
binding type. The `focus-metavariable` narrowing that Phase 3 implements is what
makes per-metavar byte spans available in the `RangeWithMetavars` values that
`SastEngine` collects. That implementation is described in
`docs/explanation/phase3_metavariable_conditions_focus_implementation.md`.

Phase 5 will enrich `SastMatch` with `message`, `severity`, `confidence`,
`snippet`, `metavariables`, `metadata`, `fingerprint`, and `fix` fields. The
byte offsets in `SastMatch` that Phase 4 establishes are the foundation for the
snippet extraction and fingerprinting logic that Phase 5 will add.
