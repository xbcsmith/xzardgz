# Phase 7: Repository Scanner Implementation

## Overview

Phase 7 introduces the `scanner` module, which provides the data structures,
configuration, and utility functions needed to analyse a repository before any
AI processing takes place. The module sits between the existing `git` layer
(which fetches repository contents) and the plugin / agent layer (which
consumes the structured output).

The four source files created in this phase are:

| File | Purpose |
| ---- | ------- |
| `src/scanner/config.rs` | Traversal configuration via `ScannerConfig` |
| `src/scanner/language.rs` | File-type detection and binary heuristic |
| `src/scanner/findings.rs` | Pre-AI finding severity and finding type |
| `src/scanner/result.rs` | Top-level versioned scan artifact |

## Design Decisions

### ScannerConfig Builder Pattern

`ScannerConfig` uses a consuming builder pattern rather than a mutable-reference
pattern. Each `with_*` method takes ownership of `self` and returns `Self`,
enabling fluent chaining without requiring the caller to declare the variable
`mut`. The `Default` implementation provides sensible production values:
hidden files excluded, gitignore respected, 1 MiB file-size cap, and
concurrency limited to four workers.

### Language Detection

`detect_language` uses a single `match` on the lowercased file extension.
Lowercasing before matching makes the function case-insensitive without
requiring a case-insensitive map. The only extensionless file that is
recognised is `"Dockerfile"`, which is detected by an exact filename comparison
executed before the extension branch.

The `is_binary_content` heuristic inspects only the first 8 192 bytes for
null bytes. This window is large enough to catch binary file headers while
remaining fast for large text files.

### FindingSeverity Ordering

`FindingSeverity` variants are declared in ascending severity order
(`Info, Low, Medium, High, Critical`). The derived `Ord` implementation
therefore gives `Info < Low < Medium < High < Critical`, meaning a `Critical`
finding compares greater than an `Info` finding. Callers can sort findings by
severity or filter by threshold using standard comparison operators without
writing custom comparators.

### ScanResult as a Versioned Artifact

`ScanResult` carries a `version` field (currently `"1"`) so that consuming
tools can detect incompatible schema changes and either reject or migrate older
artifacts. The `to_yaml` / `load_from_str` pair provides a round-trip
serialization boundary: `to_yaml` maps `serde_yaml` errors to
`PipelineError::Scanner` and `load_from_str` does the same for parse failures,
keeping all error handling consistent with the rest of the pipeline.

`PluginPreselection` derives `Default` independently of `ScanResult` because
it is a pure categorisation overlay that can be constructed from an in-progress
scan before the top-level result is finalised.

### Error Handling

All public functions that can fail return `crate::error::Result<T>`, which is
`std::result::Result<T, PipelineError>`. Serialization and deserialization
failures are mapped to `PipelineError::Scanner(String)` so callers receive a
uniform error type and can pattern-match without importing `serde_yaml`
directly.

## Module Interaction

```
git::metadata  -->  scanner::config       (configure traversal)
                -->  scanner::language     (classify files)
                -->  scanner::findings     (record pre-AI signals)
                -->  scanner::result       (assemble ScanResult)
                                |
                                v
                    plugins / agents consume ScanResult
```

`scanner::result` depends on `scanner::findings` for the `ScanFinding` type
embedded inside `ScanResult::findings`. All other scanner sub-modules are
independent.

## Test Coverage

Each module contains a `#[cfg(test)] mod tests` block with unit tests covering
success paths, failure paths, and edge cases:

- `config.rs`: seven tests covering default values, each builder method, and a
  full builder chain.
- `language.rs`: twelve tests covering known extensions, the `Dockerfile`
  special case, case-insensitivity, unknown extensions, binary detection with
  and without null bytes, and the 8 192-byte inspection window boundary.
- `findings.rs`: five tests covering severity ordering, `as_str`, `label`, the
  `ScanFinding::new` constructor, and YAML serialization.
- `result.rs`: seven tests covering the version constant, `FileEntry`
  serialization, `LanguageStats` field access, `PluginPreselection::default`,
  YAML output content, round-trip fidelity, and rejection of malformed YAML.

## Files Created

- `src/scanner/config.rs`
- `src/scanner/language.rs`
- `src/scanner/findings.rs`
- `src/scanner/result.rs`
- `docs/explanation/phase7_repository_scanner_implementation.md`
