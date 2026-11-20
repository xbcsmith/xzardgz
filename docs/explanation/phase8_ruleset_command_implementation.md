# Phase 8: Ruleset Command Implementation

## Overview

Phase 8 adds the `ruleset` command group to the xzardgz CLI. It provides four
subcommands for managing external SAST ruleset sources: `list`, `sync`,
`doctor`, and `assess`. All subcommands are offline-safe; a missing or
unreachable manifest produces a warning rather than a hard failure.

## Components Added

### `src/cli.rs`

Added `Commands::Ruleset` variant with a `RulesetCommands` subcommand enum.

New argument structs:

| Struct              | Purpose                                         |
| ------------------- | ----------------------------------------------- |
| `RulesetListArgs`   | `--sources-file` optional path override         |
| `RulesetSyncArgs`   | optional source ID positional, `--sources-file` |
| `RulesetDoctorArgs` | `--sources-file` optional path override         |
| `RulesetAssessArgs` | required source ID positional, `--sources-file` |

Six CLI parse tests were added following the
`test_<function>_<condition>_<expected>` naming convention.

### `src/main.rs`

Added
`Commands::Ruleset { command } => commands::ruleset::execute(command).await`
dispatch arm after the existing `Sast` arm.

### `src/commands/ruleset.rs`

Implements the command handler with:

- `execute` - top-level dispatcher that loads `Config` and delegates.
- `execute_list` - prints a table of `SOURCE_ID | ENABLED | LICENCE | URL`.
- `execute_sync` - syncs one or all enabled sources via `sync_source`.
- `execute_doctor` - validates URLs are HTTPS and branches are non-empty.
- `execute_assess` - looks up a source and reports validation output.

Two private helpers keep manifest resolution DRY:

- `resolve_sources_path` - resolves CLI flag > config field >
  `~/.config/xzardgz/sources.yaml`.
- `load_manifest_or_empty` - loads a manifest or returns an empty map with a
  `tracing::warn!`.

### `src/clients/rulesets/mod.rs`

New module with the acquisition layer types:

| Item              | Kind       | Purpose                                                 |
| ----------------- | ---------- | ------------------------------------------------------- |
| `RulesetSource`   | struct     | Single source definition from `sources.yaml`            |
| `SourcesManifest` | type alias | `HashMap<String, RulesetSource>`                        |
| `load_manifest`   | fn         | Deserialise a `sources.yaml` file                       |
| `SyncResult`      | struct     | Outcome of a single sync operation                      |
| `sync_source`     | fn         | Validate a source URL and branch (stub; no network I/O) |

### `src/clients/mod.rs`

Added `pub mod rulesets;` declaration.

### `src/commands/mod.rs`

Added `pub mod ruleset;` declaration.

### `src/scanner/sast/config.rs`

Added `ruleset_sources_file: Option<std::path::PathBuf>` to `SastEngineConfig`.

- Default value: `None`
- Serde attribute: `#[serde(default)]` (omits from YAML when `None`)
- Updated doctest, `Default` impl, and all serialization roundtrip tests.

### `src/clients/rulesets/assets/sources.example.yaml`

Fourteen curated public Semgrep-compatible SAST rule repositories. All sources
default to `enabled: false`. Licence classes range from `MIT` (permissive) to
`AGPL-3.0` (network copyleft); users should review the licence before enabling.

### `config.example.yaml`

Added a commented-out `ruleset_sources_file` entry in the `sast:` block to
document the new field without changing runtime defaults.

## Design Decisions

### Offline-safe by design

Every manifest-loading path degrades to an empty manifest with a
`tracing::warn!`. This keeps `ruleset list` and `ruleset doctor` usable even
when the sources file has not yet been created (e.g. fresh checkout).

### `assess` returns `Err` on missing source ID

Unlike `list`/`doctor`/`sync-all`, `assess` and `sync <id>` require the named
source to exist. Returning `Err` allows callers (CI scripts, tests) to detect
bad source IDs unambiguously.

### Stub network layer

`sync_source` validates URL scheme and branch presence but performs no actual
network I/O. The function signature and `SyncResult` type are designed to be
drop-in replaceable with a real implementation in a later phase without changing
any command handler code.

## Testing

All new public functions have tests. Naming follows
`test_<function>_<condition>_<expected>`. Key coverage:

| Test                                                                          | Verifies                |
| ----------------------------------------------------------------------------- | ----------------------- |
| `test_execute_list_with_missing_sources_file_returns_ok`                      | Graceful degradation    |
| `test_execute_doctor_with_missing_sources_file_returns_ok`                    | Graceful degradation    |
| `test_execute_sync_with_valid_sources_file_and_no_enabled_sources_returns_ok` | No-op when all disabled |
| `test_execute_assess_with_nonexistent_source_id_returns_err`                  | Error on bad ID         |
| `test_sync_source_with_http_url_returns_failure`                              | URL validation          |
| `test_load_manifest_with_invalid_yaml_returns_sast_error`                     | Parse error handling    |
