# Phase 8: Ruleset Acquisition and Licence Policy Implementation

## Overview

Phase 8 adds the acquisition layer for external SAST rulesets. It delivers four
interconnected components:

- `src/clients/rulesets/`: A pure I/O module that describes ruleset origins,
  classifies licences, enforces the default policy, and persists fetch results
  as `BundleManifest` records.
- `src/commands/ruleset.rs`: The `xzardgz ruleset` CLI group with `list`,
  `sync`, `doctor`, and `assess` subcommands.
- `testdata/sast/semgrep_rules_sources.yaml`: A reference manifest with five
  entries spanning all licence classes, used in round-trip tests.
- `SastEngineConfig.ruleset_sources_file`: An optional `PathBuf` that locates
  the sources manifest.

The acquisition layer is deliberately isolated from the scanner. The
`src/scanner/sast/` modules never depend on `src/clients/rulesets/`; the
dependency arrow runs in one direction only.

## Module Structure

All new files live under `src/clients/rulesets/`:

| File                          | Purpose                                                                                    |
| ----------------------------- | ------------------------------------------------------------------------------------------ |
| `error.rs`                    | `AcquisitionError` — typed error variants for all acquisition failures                     |
| `license.rs`                  | `LicenseClass` enum, `classify_license`, `classify_license_file`, `default_policy_allows`  |
| `manifest.rs`                 | `BundleManifest` — the persistent record written after every successful fetch              |
| `mod.rs`                      | Module re-exports; also hosts the legacy `RulesetSource` struct and `load_manifest`        |
| `source.rs`                   | `RulesetSource` enum — `Path`, `Git`, `Url`, `Oci` variants                                |
| `sources.rs`                  | `RulesetSourceEntry`, `SourcesManifest`, `parse_sources_manifest`, `load_sources_manifest` |
| `assets/sources.example.yaml` | Curated list of 14 public repositories, all `enabled: false` by default                    |

## Boundary Contract

`src/clients/rulesets/` is a pure I/O module. It may import from `src/error.rs`
and standard library crates, but it must never import from `src/scanner/sast/`.

```text
src/commands/ruleset.rs
        |
        v
src/clients/rulesets/      <-- acquisition layer (I/O only)
        |
        v
src/scanner/sast/          <-- rule execution layer
```

Inverting this dependency would couple the acquisition layer to AST matchers,
pattern compilers, and rule IR types — none of which belong in an I/O module.
The boundary is enforced by a grep-based test that asserts no file under
`src/clients/rulesets/` contains a `use crate::scanner` import.

## sources.yaml Format

The sources manifest follows the semgrep-rules-manager v1 format: a flat YAML
mapping whose top-level keys are source identifiers and whose values are source
entry objects.

```yaml
"0xdea":
  description: "Security-focused Semgrep rules"
  repository_url: "https://github.com/0xdea/semgrep-rules"
  repository_branch: "main"
  author: "0xdea"
  license: "MIT"
  enabled: false
```

Key design decisions:

- Source IDs are strings, not integers. YAML parses bare `0xdea` as a hex
  integer unless the key is quoted. All IDs that could be misread as a number or
  contain shell-special characters must be quoted.
- `enabled` defaults to `false` via `#[serde(default)]`. An entry without an
  explicit `enabled:` field is treated as disabled. This prevents accidental
  activation of unlicensed sources.
- `license` is typed as `Option<String>` in `RulesetSourceEntry`. A missing
  `license:` key yields `LicenseClass::Unknown`, which is denied by the default
  policy.

## RulesetSource Variants

`RulesetSource` in `source.rs` is an enum that maps to distinct fetch
strategies. Callers that resolve a sources manifest entry into a `RulesetSource`
choose the variant based on the scheme of `repository_url`.

| Variant | Key fields                                | Phase 8 status      |
| ------- | ----------------------------------------- | ------------------- |
| `Path`  | `PathBuf` local directory                 | Implemented         |
| `Git`   | `url`, `branch`, `commit: Option<String>` | Implemented         |
| `Url`   | `url`, `digest` (SHA-256 hex)             | Reserved (Phase 9+) |
| `Oci`   | `reference`, `digest`                     | Reserved (Phase 9+) |

`Path` is used for local development fixtures. `Git` is the primary production
variant: when `commit` is present it pins to a specific SHA; otherwise the tip
of `branch` is fetched.

## LicenseClass and Default Policy

`LicenseClass` is a seven-variant enum defined in `license.rs`. Two
classification functions are provided:

- `classify_license(spdx_id: &str)` — maps an SPDX identifier string
  case-insensitively to a `LicenseClass`.
- `classify_license_file(content: &str)` — uses keyword heuristics on the raw
  text of a `LICENSE` or `COPYING` file when no SPDX ID is available.

| Class             | SPDX examples                             | Policy  |
| ----------------- | ----------------------------------------- | ------- |
| `Permissive`      | MIT, Apache-2.0, BSD-2-Clause, ISC, 0BSD  | Allowed |
| `WeakCopyleft`    | LGPL-2.1, LGPL-3.0, MPL-2.0, EUPL-1.2     | Allowed |
| `StrongCopyleft`  | GPL-2.0, GPL-3.0                          | Denied  |
| `NetworkCopyleft` | AGPL-3.0, SSPL-1.0                        | Denied  |
| `NonCommercial`   | CC-BY-NC-4.0, CC-BY-NC-SA-4.0             | Denied  |
| `Restricted`      | Proprietary, "Commons Clause", UNLICENSED | Denied  |
| `Unknown`         | Absent or unrecognised identifier         | Denied  |

Two important subtleties:

- A "Commons Clause" addendum converts an otherwise permissive licence into
  `Restricted`, not `WeakCopyleft`. The classifier catches the phrase "COMMONS
  CLAUSE" in file text via `classify_license_file`.
- `Unknown` is denied. This is the safe default: an unreviewed or absent licence
  must not silently contribute findings.

`default_policy_allows` returns `true` for `Permissive` and `WeakCopyleft` only.
Policy override is not supported in Phase 8.

## BundleManifest

One `BundleManifest` is written per `(source_id, resolved_commit)` pair after a
successful sync. It is the authoritative record that a source was fetched,
licence-checked, and cached.

| Field               | Type             | Purpose                                         |
| ------------------- | ---------------- | ----------------------------------------------- |
| `source_id`         | `String`         | Identifier key from the sources manifest        |
| `resolved_commit`   | `Option<String>` | Git SHA at fetch time; `None` for Path/Url/Oci  |
| `declared_license`  | `Option<String>` | SPDX ID copied from the manifest entry          |
| `observed_license`  | `Option<String>` | SPDX ID inferred from the fetched LICENSE file  |
| `license_class`     | `LicenseClass`   | Broad class determined at fetch time            |
| `content_digest`    | `String`         | Hex SHA-256 of directory paths and sizes        |
| `rule_count`        | `usize`          | Number of `.yaml` rule files in the bundle      |
| `fetched_at`        | `String`         | RFC 3339 timestamp                              |
| `allowed_by_policy` | `bool`           | Whether the active policy permitted this source |

`observed_license` takes precedence over `declared_license` when both are
present and they differ. In Phase 8, `observed_license` is always `None`; the
SPDX extraction pass that reads the fetched `LICENSE` file is deferred to a
later phase.

## Cache Design

Fetched rulesets are stored in a content-addressed directory tree:

```text
<cache_dir>/xzardgz/rulesets/<source_id>/<commit>/
```

On Linux the cache root is typically `~/.cache/`; on macOS it is
`~/Library/Caches/`. Re-syncing a source at the same commit SHA is a no-op: the
directory already exists, so the fetch, licence check, and manifest write are
all skipped. This makes repeated `ruleset sync` calls idempotent and fast in CI
environments where the cache is preserved between runs.

## CLI Subcommands

| Subcommand            | Description                                                                 |
| --------------------- | --------------------------------------------------------------------------- |
| `ruleset list`        | Prints a table of all sources: ID, enabled flag, licence, and URL           |
| `ruleset sync [id]`   | Fetches enabled sources; with an ID, syncs only that source                 |
| `ruleset doctor`      | Validates URLs and branches; reports issues without writing any files       |
| `ruleset assess <id>` | Reports rule count and detected licence class; does not modify the manifest |

All subcommands degrade gracefully when the sources file does not exist. They
emit a `tracing::warn!` and continue with an empty manifest rather than
returning an error. `assess` and `sync <id>` are the only subcommands that
return `Err` for a missing source ID, because the caller named a specific source
and an absent entry is an unambiguous mistake.

## Testing

| Test                  | What it verifies                                                                                |
| --------------------- | ----------------------------------------------------------------------------------------------- |
| Manifest round-trip   | `BundleManifest` survives a serde YAML serialise/deserialise cycle                              |
| `0xdea` string key    | `testdata/sast/semgrep_rules_sources.yaml` loads and the `"0xdea"` key is preserved as a string |
| Licence policy        | `default_policy_allows` returns correct verdicts for all seven classes                          |
| Commons Clause        | `classify_license_file` returns `Restricted` for text containing "COMMONS CLAUSE"               |
| Staleness             | Syncing at a new commit SHA writes a new cache entry alongside the old one                      |
| Cache idempotency     | Syncing twice at the same commit SHA does not re-fetch                                          |
| Boundary charter grep | No `use crate::scanner` appears in any file under `src/clients/rulesets/`                       |

## Config Changes

`SastEngineConfig` in `src/scanner/sast/config.rs` gains one new field:

```rust
/// Optional path to a sources.yaml manifest for external ruleset management.
///
/// When `None`, the command layer falls back to `~/.config/xzardgz/sources.yaml`.
#[serde(default)]
pub ruleset_sources_file: Option<std::path::PathBuf>,
```

The field is serialised with `#[serde(default)]` so it is omitted from YAML when
`None`. It has no effect on the SAST engine itself; the `ruleset` command group
reads it to locate the manifest file.
