# Phase 8 Rulesets Data Model Implementation

This document explains the design decisions behind the five core data-model
files written for `src/clients/rulesets/` in Phase 8.

## Overview

Phase 8 introduces a structured acquisition layer for SAST rulesets. The five
files establish the data contracts that all future fetch, cache, and
licence-policy code will build on.

## Module boundary

`src/clients/rulesets/` must never import from `src/scanner/sast/`. The module
operates only on bytes, paths, `serde_yaml::Value`, and its own types. This
prevents circular dependencies between the data-fetching leaf and the scanner
core.

## File breakdown

### `error.rs` - `AcquisitionError`

Uses `thiserror` to define a flat enum. Each variant carries named fields rather
than a bare `String` so that callers can pattern-match on structured data
without parsing error messages.

Key variants:

| Variant          | Fields                            | Purpose                    |
| ---------------- | --------------------------------- | -------------------------- |
| `Git`            | `source_id`, `cause`              | git clone / fetch failures |
| `Http`           | `url`, `cause`                    | HTTPS download failures    |
| `Io`             | `path`, `cause`                   | filesystem access failures |
| `LicenceDenied`  | `source_id`, `class`, `reason`    | policy gate rejection      |
| `DigestMismatch` | `source_id`, `expected`, `actual` | content integrity check    |

### `source.rs` - `RulesetSource`

A four-variant enum covering all acquisition strategies. Only `Path` and `Git`
are fully implemented in Phase 8; `Url` and `Oci` are reserved.

The `source_kind()` method returns a static `&str` for logging and telemetry.
The `display_location()` method returns a human-readable location string
suitable for user-facing output.

The enum does not use `#[serde(tag = "type")]` because the `Path(PathBuf)`
newtype variant is not compatible with internally-tagged serialisation in serde
(serde requires the inner type to be a struct/map for internal tagging to work).
Standard serde enum serialisation is used instead, which roundtrips correctly.

### `sources.rs` - `SourcesManifest` and `RulesetSourceEntry`

Implements the semgrep-rules-manager v1 manifest format. Key design decisions:

- `SourcesManifest` is a `BTreeMap<String, RulesetSourceEntry>` type alias (not
  a struct). `BTreeMap` gives deterministic iteration order for tests and
  diff-friendly YAML serialisation.
- `#[serde(deny_unknown_fields)]` on `RulesetSourceEntry` rejects unknown YAML
  keys at parse time, surfacing misconfiguration early.
- `description` has `#[serde(default)]` so manifests that omit the field still
  parse successfully (the field defaults to an empty string).
- Hex-prefixed keys such as `0xdea` are preserved as the string `"0xdea"` by
  serde_yaml v0.9+ under YAML 1.2 semantics (integers in YAML 1.2 use decimal
  notation only). The test
  `test_parse_sources_manifest_string_key_0xdea_deserializes_as_string` pins
  this behaviour.

### `license.rs` - `LicenseClass`

A seven-variant enum covering the full spectrum from permissive to restricted.

`classify_license()` uses case-insensitive substring matching with a fixed
precedence order:

1. `Restricted` - "commons clause", "proprietary", "all rights reserved"
2. `NonCommercial` - "noncommercial", "non-commercial", "-nc-"
3. `NetworkCopyleft` - "agpl", "affero" (checked before GPL to avoid AGPL
   matching the GPL check)
4. `StrongCopyleft` - "gpl" but NOT "lgpl"
5. `WeakCopyleft` - "lgpl", "mpl", "mozilla", "epl", "eclipse", "cddl"
6. `Permissive` - "mit", "apache", "bsd", "isc", "zlib", "boost", "0bsd",
   "unlicense", "cc0"
7. `Unknown` (fallback)

The `is_allowed_by_default_policy()` method is an instance method on
`LicenseClass`. The free function `default_policy_allows()` is kept for backward
compatibility.

Default policy: allow `Permissive` and `WeakCopyleft`; deny everything else
including `Unknown` to prevent accidental use of unreviewed licences.

### `manifest.rs` - `BundleManifest`

A serialisable record written to cache after every successful fetch. The
`BundleManifest::new()` constructor encapsulates the licence classification
logic:

- `observed_license` takes precedence over `declared_license`.
- If both are `None`, `license_class` is `Unknown`.
- `allowed_by_policy` is derived directly from
  `license_class.is_allowed_by_default_policy()`.

`PartialEq` is derived to support roundtrip equality assertions in tests.

### `mod.rs` - module declarations and backward-compat wrappers

The minimal `mod.rs` declares the five sub-modules and re-exports
`SourcesManifest` and `RulesetSourceEntry` at the `crate::clients::rulesets`
level. The backward-compatible `load_manifest`, `sync_source`, and `SyncResult`
items are provided as thin wrappers over the new API so that
`crate::commands::ruleset` requires only field-accessor updates rather than a
full rewrite.

## Test counts

| File          | Unit tests | Doc tests | Total  |
| ------------- | ---------- | --------- | ------ |
| `error.rs`    | 7          | 0         | 7      |
| `source.rs`   | 10         | 3         | 13     |
| `sources.rs`  | 12         | 7         | 19     |
| `license.rs`  | 35         | 5         | 40     |
| `manifest.rs` | 9          | 2         | 11     |
| `mod.rs`      | 5          | 2         | 7      |
| **Total**     | **78**     | **19**    | **97** |

The combined `clients::rulesets` test count reported by `cargo test` is 108,
which includes doc-test compilation checks in addition to the runtime tests.

## Quality gate results

All four gates pass on the final implementation:

```text # clean
cargo check --all-targets --all-features                # 0 errors
cargo clippy --all-targets --all-features -- -D warnings # 0 warnings
cargo test --all-features                               # 2327 passed, 0 failed
```
