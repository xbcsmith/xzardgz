# Phase 9: Conformance Testing Implementation

## Overview

Conformance testing is the mechanism by which the SAST engine verifies that
every rule produces the results its author intended. A conformance fixture is a
source file annotated with inline comments declaring which lines a rule should
match and which it should not. The harness parses those annotations, runs the
rule against the fixture, and compares the scanner output against the declared
expectations.

This approach gives each rule a standing executable specification: when the
engine changes, the fixture either still passes (the change is safe) or
regresses (the change must be justified). A checked-in baseline records the
last-known good state so that regressions are detected automatically in CI.

Phase 9 delivers:

- `src/scanner/sast/conformance.rs` -- the full harness implementation.
- `testdata/sast/conformance_baseline.json` -- the initial golden baseline.
- `testdata/sast/conformance/` -- fixture files for every built-in rule.
- `testdata/sast/conformance/synthetic_*.yaml` and `synthetic_*.rs` -- isolated
  synthetic fixtures for unit-testing the harness itself.
- `tests/sast_conformance.rs` -- the integration test file.

## Architecture

### Module layout

```text
src/scanner/sast/conformance.rs
  parse_annotations()           -- extract annotation metadata from fixture source
  scan_in_memory()              -- run a rule against an in-memory string
    scan_in_memory_regex()      -- regex-mode path (RegexModeScanner)
    scan_in_memory_ast()        -- AST path (SupportLang::Rust + scan_rule)
  check_conformance()           -- compare scanner output against annotations
  run_rule_conformance()        -- orchestrate the three steps above
  ConformanceBaseline           -- JSON baseline loader and regression detector
  RuleConformanceResult         -- Pass | Fail { missed, fp } | Skipped
  Annotation                   -- single annotation with line numbers and kind
  AnnotationKind                -- RuleId | Ok | TodoRuleId
  ConformanceError              -- Io | Json | Scan | UnsupportedLanguage
```

### Data flow

```text
fixture_src
    |
    v
parse_annotations()         --> Vec<Annotation>
                                    |
fixture_src                         |
    |                               |
    v                               v
scan_in_memory()            --> Vec<u32>  (matched line numbers, 1-based)
    |
    +-- applies_to_regex_mode? --> RegexModeScanner::scan_bytes()
    +-- applies_to_rust?       --> SupportLang::Rust.ast_grep() + scan_rule()
                                    |
                                    v
                           check_conformance()
                                    |
                                    v
                           RuleConformanceResult
```

### Supported rule languages

| Language  | Scanner path           | Notes                                   |
| --------- | ---------------------- | --------------------------------------- |
| `regex`   | `scan_in_memory_regex` | Direct regex over raw bytes             |
| `generic` | `scan_in_memory_regex` | Same path as regex                      |
| `rust`    | `scan_in_memory_ast`   | tree-sitter parse + formula engine      |
| Others    | Error                  | `ConformanceError::UnsupportedLanguage` |

## Annotation Format

Annotations are line comments placed directly above the target line or inline on
the same line as the target code.

### Above-style (most common)

```rust
// ruleid: rust-md5-usage
let _ = md5::compute(b"hello");
```

The comment is on its own line. `target_line` = `annotation_line + 1`. The
scanner must match line 2.

### Inline-style

```rust
let _ = md5::compute(data); // ruleid: rust-md5-usage
```

Code precedes the comment on the same line. `target_line` = `annotation_line`.
The scanner must match the same line.

### Annotation kinds

| Annotation         | Meaning                                                    |
| ------------------ | ---------------------------------------------------------- |
| `ruleid: <id>`     | The named rule must match the target line.                 |
| `ok: <reason>`     | No rule must match the target line (global, not per-rule). |
| `todoruleid: <id>` | Expected match, not yet enforced. Treated as absent.       |

### Detection rules

1. If a fixture has no `ruleid:` or `ok:` annotations for the rule being
   checked, the result is `Skipped`. The fixture is not testing this rule.
2. `todoruleid:` annotations are ignored; they never cause `Fail`.
3. A `ruleid:` target line missing from the scanner output is a `missed_line`.
4. An `ok:` target line present in the scanner output is a
   `false_positive_line`.
5. If `missed_lines` and `false_positive_lines` are both empty the result is
   `Pass`; otherwise it is `Fail`.

## Synthetic Rules

Three synthetic rules exist exclusively to test the harness logic itself. They
are defined in `testdata/sast/conformance/` alongside their fixtures.

| Rule                  | Expected result                                                     |
| --------------------- | ------------------------------------------------------------------- |
| `synthetic-pass-rule` | `Pass` -- pattern matches the expected line, nothing on the ok line |
| `synthetic-fail-rule` | `Fail` -- pattern misses the expected line and fires on the ok line |
| `synthetic-todo-rule` | `Skipped` -- only `todoruleid:` annotation present                  |

The synthetic rules use `languages: [regex]` so they exercise only the regex
path and compile deterministically without AST dependencies.

## First-Party Rule Results

The table below records the state captured in
`testdata/sast/conformance_baseline.json`.

| Rule                                | Baseline status      |
| ----------------------------------- | -------------------- |
| `regex-hardcoded-const-secret`      | pass                 |
| `regex-hardcoded-db-url`            | pass                 |
| `regex-hardcoded-secret-assignment` | pass                 |
| `rust-aes-ecb-mode`                 | pass                 |
| `rust-blowfish-cipher`              | pass                 |
| `rust-crc32-for-security`           | pass                 |
| `rust-des-cipher`                   | pass                 |
| `rust-hardcoded-iv`                 | pass                 |
| `rust-hardcoded-password-field`     | skipped (no fixture) |
| `rust-hardcoded-salt`               | pass                 |
| `rust-md5-usage`                    | pass                 |
| `rust-openssl-no-verify`            | pass                 |
| `rust-pbkdf2-low-rounds`            | pass                 |
| `rust-pkcs1v15-encrypt`             | pass                 |
| `rust-rc4-cipher`                   | pass                 |
| `rust-reqwest-invalid-certs`        | pass                 |
| `rust-reqwest-invalid-hostnames`    | pass                 |
| `rust-sha1-hmac`                    | pass                 |
| `rust-sha1-usage`                   | pass                 |
| `rust-shell-command-injection`      | pass                 |
| `rust-triple-des-cipher`            | pass                 |
| `rust-unsafe-ffi-call`              | skipped (no fixture) |
| `rust-unsafe-transmute`             | pass                 |
| `rust-weak-prng`                    | pass                 |
| `rust-weak-rsa-key`                 | pass                 |

## Baseline Management

The checked-in file `testdata/sast/conformance_baseline.json` is the reference
point for regression detection. The
`test_first_party_baseline_has_no_regressions` test loads this file and fails if
any rule that was `"pass"` in the baseline is now `Fail`.

### Updating the baseline after intentional changes

If a rule is modified and its fixture is updated to match the new behaviour,
regenerate the baseline by running all first-party conformance tests and
recording the results:

```bash
cargo test --test sast_conformance first_party 2>&1 \
    | grep "rule '" \
    | ...
```

Alternatively, write a small binary or build script that calls
`run_rule_conformance` for every embedded rule and serialises the results to
`conformance_baseline.json` with the current date in `generated_at`.

A rule that moves from `pass` to `Skipped` (fixture removed) is not a
regression. A rule that moves from `pass` to `Fail` is always a regression and
must be explicitly resolved before merging.

## Running the Conformance Harness

### Unit and synthetic tests (always run)

```bash
cargo test --test sast_conformance
```

### First-party rule tests (always run, no extra dependencies)

```bash
cargo test --test sast_conformance first_party
```

### External corpus tests (requires feature flag and env var)

```bash
export XZARDGZ_SEMGREP_RULES_DIR=/path/to/semgrep-rules
cargo test --test sast_conformance --features sast-integration-tests corpus
```

The corpus tests walk the directory tree at `XZARDGZ_SEMGREP_RULES_DIR`, compile
every YAML file, and run conformance against sibling fixture files. When the env
var is unset the test module is compiled but exits immediately with a skip
message rather than failing.

## Measured Pass Rates

The table below will be filled in after the first full corpus run.

| Corpus                   | Total rules | Passed | Skipped | Failed | Pass rate |
| ------------------------ | ----------- | ------ | ------- | ------ | --------- |
| First-party (embedded)   | 25          | 23     | 2       | 0      | 100%      |
| External (semgrep-rules) | TBD         | TBD    | TBD     | TBD    | TBD       |

## Exit Bar

The external corpus gate
(`test_external_corpus_at_least_90_percent_of_v1_rules_pass`) requires at least
90% of v1-language corpus rules with annotated fixtures to produce `Pass`. Rules
that produce `Skipped` (no applicable annotations for the rule id being tested,
or unsupported language) are excluded from the denominator.

The 90% bar is a minimum quality threshold. It is expected to rise toward 100%
as the formula engine gains broader construct coverage in later phases.
