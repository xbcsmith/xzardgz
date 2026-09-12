# Phase 4: SastEngine Facade Implementation

## Overview

This document describes the Phase 4 addition of the `SastEngine` facade to
`src/scanner/sast/mod.rs`. The facade ties together the file-discovery,
prefiltering, AST-formula, and regex-mode subsystems into a single parallelised
scan operation exposed through a clean public API.

## New Public Types

### `SastMatch`

Represents one match produced by the engine. Fields:

| Field       | Type                       | Description                            |
| ----------- | -------------------------- | -------------------------------------- |
| `path`      | `PathBuf`                  | Filesystem path of the matched file    |
| `start`     | `usize`                    | Byte offset of match start (inclusive) |
| `end`       | `usize`                    | Byte offset of match end (exclusive)   |
| `rule_id`   | `String`                   | Identifier of the matching rule        |
| `bindings`  | `BTreeMap<String, String>` | Metavariable bindings (text only)      |
| `truncated` | `bool`                     | Whether the per-file match cap was hit |

Phase 5 will extend this with `message`, `severity`, `snippet`, `fingerprint`,
and `fix`.

### `SkippedRule`

Carries the `rule_id` and human-readable `reason` for any rule that could not be
evaluated (e.g. regex compile failure).

### `SastScanReport`

The value returned by `SastEngine::scan`. Contains the full match list, skipped
rules, file counts, truncation count, and wall-clock duration.

## `SastEngine` API

```text
SastEngine::new(config) -> Result<Self, SastError>
engine.with_rules(rules) -> Result<(), SastError>
engine.with_discovery(discovery) -> &mut Self
engine.scan(root) -> Result<SastScanReport, SastError>
```

`new` and `with_rules` return `Result` for forward-compatibility with future
validation, but are currently infallible.

## Scan Pipeline

```text
scan(root)
  1. Prefilter::from_rules        -- build AC + regex prefilter from rule set
  2. Classify rules               -- regex-mode rules -> RegexModeScanner
                                     AST rules -> Vec<RuleIr>
  3. discover_files               -- gitignore-aware, size-bounded file walk
  4. rayon::ThreadPool::install   -- parallel per-file processing
       process_file(path, ...)
         a. read bytes
         b. size guard (TOCTOU defence)
         c. binary check (NUL bytes in first 8 KiB)
         d. prefilter (AC / regex quick-check)
         e. AST scan   (Rust files only)
         f. regex scan (all files)
  5. Aggregate + sort             -- deterministic (path, start, end, rule_id)
```

Binary files and files that fail the prefilter are excluded by `discover_files`
before they enter the parallel stage. The TOCTOU guards in `process_file` defend
against file changes between discovery and read.

## Panic Isolation

Each per-file closure is wrapped with `std::panic::catch_unwind` and
`AssertUnwindSafe`. A panic in one file produces a `FileResult` with
`parse_error: true` and does not interrupt processing of other files.

## Thread Pool

The Rayon thread pool is built fresh per `scan` call using
`SastEngineConfig::jobs`. `jobs == 0` lets Rayon use available parallelism
(default behaviour). This avoids interference with the global Rayon pool used
elsewhere in the process.

## Tests Added

Four new tests inside `mod tests` in `src/scanner/sast/mod.rs`:

| Test                                                             | What it verifies                                                                |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| `test_sast_engine_prefilter_soundness_differential`              | MD5 rule match counts are identical with and without `always_analyze` set       |
| `test_sast_engine_two_runs_produce_identical_sorted_output`      | Two sequential scans over the same fixtures produce the same sorted match list  |
| `test_sast_engine_panic_in_one_file_does_not_lose_other_results` | A binary file does not prevent matches from other files                         |
| `test_sast_engine_scan_counts_files_correctly`                   | `scanned_file_count` equals the number of non-binary, non-excluded source files |

## Files Changed

- `src/scanner/sast/mod.rs` -- added imports, `SastMatch`, `SkippedRule`,
  `SastScanReport`, `SastEngine`, `FileResult`, `process_file`,
  `skipped_result`, and four integration tests
- `docs/explanation/phase4_sast_engine_facade_implementation.md` -- this file
