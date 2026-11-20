# Phase 6: SAST Scan Command Implementation

## Overview

This document describes the implementation of `src/commands/sast.rs` and its
supporting changes delivered in Phase 6 of the SAST scanning tool. The primary
deliverable is the `xzardgz sast scan` CLI subcommand, which wires together the
rule resolver, the SAST engine, and the output projection layer into a single
end-to-end scan pipeline.

## Deliverables

| Deliverable                             | Location                                                      |
| --------------------------------------- | ------------------------------------------------------------- |
| `Sast` variant in `Commands` enum       | `src/cli.rs`                                                  |
| `SastCommands` and `SastScanArgs` types | `src/cli.rs`                                                  |
| Dispatch in `main`                      | `src/main.rs`                                                 |
| `pub mod sast` registration             | `src/commands/mod.rs`                                         |
| `PipelineError::Sast` variant           | `src/error.rs`                                                |
| `sast` field on `Config`                | `src/config.rs`                                               |
| `sast_scan` field on `Config`           | `src/config.rs`                                               |
| Command handler                         | `src/commands/sast.rs`                                        |
| This document                           | `docs/explanation/phase6_sast_scan_command_implementation.md` |

## Architecture

```text
xzardgz sast scan [OPTIONS] PATH
        |
        v
commands::sast::execute(SastCommands)
        |
        v
Config::load()      ->  Config { sast: SastEngineConfig, sast_scan: SastScanConfig }
        |
        v
execute_with(SastScanArgs, Config)
        |
        +-- resolve_rules(ResolveConfig)
        |       |
        |       +-- load_builtin_ruleset("builtin:crypto")
        |       +-- load_builtin_ruleset("builtin:security")
        |       +-- load_rules_from_dir(external_path, ruleset_id)
        |       `-- [returns Vec<NamespacedRule>]
        |
        +-- SastEngine::new(SastEngineConfig)
        |
        +-- engine.with_rules(Vec<RuleIr>)
        |
        +-- engine.scan(&path)  ->  SastScanReport
        |
        +-- post-process: stamp ruleset_id and namespaced rule_id on each match
        |
        +-- retain matches >= severity threshold
        |
        +-- truncate to max_findings
        |
        `-- write_output(report, format, output_path)
                |
                +-- "sarif"     -> render_sarif(report) -> JSON
                +-- "cyclonedx" -> render_cyclonedx(report) -> JSON
                `-- "json"      -> report.matches -> JSON array
```

## Configuration Merge Strategy

CLI flags always take precedence over configuration file values:

| Concern         | CLI flag         | Config key               | Default          |
| --------------- | ---------------- | ------------------------ | ---------------- |
| Rulesets        | `--ruleset`      | `sast_scan.rulesets`     | builtin defaults |
| Search dirs     | `--ruleset-path` | (CLI only)               | none             |
| Output format   | `--format`       | (CLI only)               | `"json"`         |
| Output file     | `--output`       | (CLI only)               | stdout           |
| Worker threads  | `--jobs`         | `sast.jobs`              | `0` (auto)       |
| Severity filter | `--severity`     | (CLI only)               | `"warning"`      |
| Max findings    | (CLI only)       | `sast_scan.max_findings` | `500`            |

When `--ruleset` is not supplied, `sast_scan.rulesets` from the configuration
file is used. If that is also empty, `resolve_rules` falls back to the embedded
defaults (`builtin:crypto` and `builtin:security`).

## Rule Namespacing Post-Processing

The SAST engine sets `rule_id = rule.id` and `ruleset_id = ""` on every match.
After `engine.scan()` returns, the command handler updates each match with the
fully-qualified identifiers from the pre-built lookup table:

```text
base_rule_id -> (namespaced_id, ruleset_id)
```

For example, a match from the `rust-md5-usage` rule loaded via the
`builtin:crypto` ruleset is updated as:

```text
match.rule_id    = "builtin:crypto::rust-md5-usage"
match.ruleset_id = "builtin:crypto"
```

This design keeps the engine free of namespace concerns while ensuring every
output record carries the full provenance of its source rule.

## Severity Filter

The `severity_passes_threshold` function implements a simple three-level
comparison:

| Severity | Numeric level |
| -------- | ------------- |
| Info     | 0             |
| Warning  | 1             |
| Error    | 2             |

A match passes when `match_level >= threshold_level`. An unrecognised threshold
string defaults to `"warning"` (level 1) to fail safe.

## Output Formats

| Format           | Description                                      |
| ---------------- | ------------------------------------------------ |
| `json` (default) | JSON array of `SastMatch` structs                |
| `sarif`          | SARIF 2.1.0 log produced by `render_sarif`       |
| `cyclonedx`      | CycloneDX 1.7 BOM produced by `render_cyclonedx` |

Output is written to `--output FILE` when specified, otherwise to stdout.

## Error Handling

| Error condition                | `PipelineError` variant           |
| ------------------------------ | --------------------------------- |
| Config file parse error        | `Config`                          |
| Rule resolution / engine error | `Scanner` (via `From<SastError>`) |
| Output serialisation failure   | `Sast`                            |
| File I/O failure               | `Io` (via `From<std::io::Error>`) |

## Testing

Five unit tests are included in `src/commands/sast.rs`:

| Test                                                                   | Coverage                                     |
| ---------------------------------------------------------------------- | -------------------------------------------- |
| `test_severity_passes_threshold_info_passes_all_levels`                | `severity_passes_threshold` with `"info"`    |
| `test_severity_passes_threshold_warning_blocks_info`                   | `severity_passes_threshold` with `"warning"` |
| `test_severity_passes_threshold_error_blocks_warning`                  | `severity_passes_threshold` with `"error"`   |
| `test_severity_passes_threshold_unknown_threshold_defaults_to_warning` | Unknown threshold handling                   |
| `test_execute_sast_scan_on_empty_directory_returns_ok`                 | Full pipeline integration on empty directory |
