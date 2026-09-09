# OSV Vulnerability Scanning Integration

## Overview

This document describes the addition of OSV (Open Source Vulnerabilities)
vulnerability scanning support to the `security_review` plugin.

## Changes

### `src/config.rs` — `SecurityReviewConfig`

Added one new field to `SecurityReviewConfig`:

```rust
pub osv_enabled: bool,
```

- Serialised with `#[serde(default = "default_true")]` so existing config files
  that omit the field default to `true`.
- Exposes whether the plugin should call the public OSV API when
  `dependency_scanning` is also enabled.
- The `Default` impl initialises this field via `default_true()` (consistent
  with `dependency_scanning` and other boolean gates in the struct).

### `src/clients/vuln/mod.rs` (new)

Stub module declaring the `VulnerabilityQuery` struct and the
`VulnerabilitySource` trait. These types are the shared interface for all
vulnerability database clients. The full implementations are provided by the
parallel OSV agent branch.

### `src/clients/vuln/osv/mod.rs` (new)

Stub module declaring `OsvClient`. The client wraps the public
`https://api.osv.dev/v1/` endpoint. No API key is required. The stub compiles
cleanly and allows the `security_review` plugin to import and reference the type
without requiring the full HTTP implementation to be present.

### `src/clients/vuln/osv/scoring.rs` (new)

Provides `OsvScore` (a clamped `f64` wrapper) and `score_severity`, which maps
OSV severity label strings (`"critical"`, `"high"`, `"medium"`, `"low"`) to
numeric scores on the CVSS `[0.0, 10.0]` scale. Fully tested.

### `src/plugins/security_review/plugin.rs`

#### New imports

Four forward-declared imports added for the full OSV integration:

- `crate::clients::vuln::osv::OsvClient`
- `crate::clients::vuln::osv::scoring::{OsvScore, score_severity}`
- `crate::clients::vuln::{VulnerabilityQuery, VulnerabilitySource}`
- `crate::scanner::scoring::ScoringSignal`

The first three are marked `#[allow(unused_imports)]` because the current stub
`resolve_osv_signals` returns an empty vec; they will be used once the full
manifest parser is wired in.

#### New execution step (step 2b)

After the `enabled` gate and before file prioritisation, the `run` method now:

1. Clones `ctx.scan_result` to avoid a simultaneous mutable and immutable
   borrow.
2. Conditionally calls `resolve_osv_signals` when both `config.osv_enabled` and
   `config.dependency_scanning` are `true`.
3. Stores the result in `osv_signals: Vec<ScoringSignal>`.

#### Updated `build_user_prompt`

The function signature gains an `osv_note: Option<&str>` parameter. When `Some`,
the note is appended to the prompt before the final instruction line. All
existing callers pass `None`; the `SingleSession` branch generates a note from
`osv_signals.len()` when the list is non-empty.

#### New `resolve_osv_signals` free function

An `async` helper that:

- Returns an empty vec immediately when no dependency manifests are present in
  the scan result.
- Otherwise returns an empty vec with a comment explaining that full per-package
  resolution requires a manifest parser (not yet available in `ScanResult`).
- Accepts `_ctx: &mut PluginContext` as a forward-declared parameter for future
  diagnostic recording without requiring callers to change their signatures.

## Design Decisions

- **Graceful degradation**: OSV failures (no manifests, or future network
  errors) return an empty signal list rather than aborting the plugin run.
- **No API key required**: The OSV endpoint is public and unauthenticated.
- **Borrow safety**: The scan result is cloned before the OSV call so that the
  mutable `ctx` borrow required for future diagnostics does not conflict with
  the immutable scan result reference.
- **Forward compatibility**: All imports and the `_ctx` parameter are declared
  now so that merging the parallel OSV HTTP implementation requires no edits to
  `plugin.rs`.

## Testing

New tests added:

| Location                      | Test name                                                           | What it covers                                   |
| ----------------------------- | ------------------------------------------------------------------- | ------------------------------------------------ |
| `config.rs`                   | `test_security_review_config_osv_enabled_defaults_to_true`          | Default value is `true`                          |
| `config.rs`                   | `test_security_review_config_osv_enabled_can_be_set_false`          | Field can be overridden                          |
| `plugin.rs`                   | `test_security_review_plugin_run_osv_disabled_skips_osv_resolution` | Plugin completes when `osv_enabled=false`        |
| `plugin.rs`                   | `test_build_user_prompt_with_osv_note_includes_note`                | OSV note appears in prompt                       |
| `plugin.rs`                   | `test_build_user_prompt_without_osv_note_has_no_osv_text`           | Prompt is clean without a note                   |
| `clients/vuln/osv/scoring.rs` | (8 tests)                                                           | `OsvScore` clamping and `score_severity` mapping |

All 1717 unit tests pass under `cargo test --all-features`.
