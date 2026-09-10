# OSV Vulnerability Scanning Integration

## Overview

This document describes the addition of OSV (Open Source Vulnerabilities)
vulnerability scanning support to the `security_review` plugin. It covers the
`src/clients/vuln/` module, the `OsvClient` HTTP client, the CVSS scoring
calculator, the `security_review` plugin wiring, and all configuration changes
introduced by Phase 3 of the
[external_data_clients_plan.md](external_data_clients_plan.md).

## Changes

### `testdata/osv.dev.results.json`

The test fixture was renamed from `testdata/osv.dev.resutls.json` (which had a
typo in "resutls") to the correct `testdata/osv.dev.results.json`. This file is
the canonical fixture for OSV deserialization and wiremock tests. It contains
real OSV response shapes including entries with both CVSS_V3 and CVSS_V4
severity, entries with only CVSS_V4, and `PYSEC-*` entries with no `severity`
key at all.

### `src/clients/vuln/mod.rs` (new)

Defines the shared vulnerability-client interface:

- `VulnerabilityQuery` -- query inputs carrying `name`, `version`, `ecosystem`,
  `purl`, and `commit` fields.
- `VulnerabilityRecord` -- a single OSV vulnerability record deserialised from
  the `vulns[]` response array.
- `OsvReference`, `OsvSeverityEntry`, `OsvAffected`, `OsvPackage` -- nested OSV
  schema types.
- `VulnClientError` -- thiserror enum with `Http`, `Parse`, and `NoResults`
  variants.
- `VulnerabilitySource` -- async trait
  (`query(&VulnerabilityQuery) -> Result<Vec<VulnerabilityRecord>, VulnClientError>`)
  implemented by all vulnerability database clients.

All struct fields use `#[serde(default)]` where the field may be absent in OSV
responses (e.g. the `severity`, `aliases`, `references`, and `affected` arrays).

### `src/clients/vuln/osv/mod.rs` (new)

Implements `OsvClient`, which wraps the public
`POST https://api.osv.dev/v1/query` endpoint. No API key is required.

`OsvClient` implements `VulnerabilitySource` with a three-level fallback
algorithm:

1. **PURL query** -- when `dep.purl` is `Some`, send a PURL-based request (with
   `dep.version` as a sibling field when present). Return immediately on
   non-empty results.
2. **Name and ecosystem query** -- when step 1 returns empty or `dep.purl` is
   absent, and both `dep.name` (non-empty) and `dep.ecosystem` are available,
   send a name-plus-ecosystem query (with optional `version`). Return on
   non-empty results.
3. **Commit hash query** -- when `dep.commit` is `Some` and prior steps found
   nothing, send a commit-hash-based query.

An empty `vulns` array is a valid non-error result. An HTTP failure at any step
is returned immediately as `VulnClientError::Http`; empty results advance to the
next fallback step.

`OsvClient::with_base_url(url)` is provided for testing: it directs all requests
to a wiremock server rather than the production endpoint.

### `src/clients/vuln/osv/scoring.rs` (new)

Provides CVSS scoring for OSV `severity[]` arrays:

- `CvssBand` -- qualitative severity band: `Low`, `Medium`, `High`, `Critical`.
- `OsvScore` -- scored result carrying `cvss_v3_score: Option<f64>`,
  `cvss_v4_score: Option<f64>`, `primary_score: f64`, and
  `band: Option<CvssBand>`.
- `score_severity(severity: &[OsvSeverityEntry]) -> OsvScore` -- converts the
  raw severity array into an `OsvScore`.
- `osv_score_to_signal(score: &OsvScore) -> Option<ScoringSignal>` -- maps the
  qualitative band to a `ScoringSignal::Negative` weight consistent with other
  deterministic `security_review` signals.

**Scoring rules:**

| Condition                        | `primary_score` | `band`                 |
| -------------------------------- | --------------- | ---------------------- |
| `severity` is empty or absent    | `0.0`           | `None`                 |
| Both CVSS_V3 and CVSS_V4 present | CVSS v3 score   | from CVSS v3           |
| Only CVSS_V4 present             | CVSS v4 score   | from CVSS v4           |
| Only CVSS_V3 present             | CVSS v3 score   | from CVSS v3           |
| Malformed or unparseable vector  | `0.0` per entry | does not fail the call |

CVSS v3 is always preferred as `primary_score` when both versions are present.
The v4 score is retained in `cvss_v4_score` for audit purposes only.

CVSS v3.x uses the standard base score formula. The implementation branches on
the vector string prefix (`CVSS:3.0/` vs `CVSS:3.1/`) because OSV's
`type: "CVSS_V3"` covers both v3.0 and v3.1 vectors. CVSS v4.0 uses the EQ-level
approach operating on exploit and impact sub-dimensions.

### `src/config.rs` -- `SecurityReviewConfig`

Added one new field:

```rust
pub osv_enabled: bool,
```

Serialised with `#[serde(default = "default_true")]` so existing configuration
files that omit the field default to `true`. The field controls whether the
plugin queries the OSV API when `dependency_scanning` is also enabled. The
`Default` impl initialises it via `default_true()`, consistent with the other
boolean gate fields in the struct.

### `src/plugins/security_review/plugin.rs`

The `run` method gains a new step (step 2b) between the `enabled` gate and file
prioritisation:

1. Clones `ctx.scan_result` to avoid a simultaneous mutable and immutable
   borrow.
2. Calls `resolve_osv_signals` when both `config.osv_enabled` and
   `config.dependency_scanning` are `true`.
3. Stores the result in `osv_signals: Vec<ScoringSignal>`.

`build_user_prompt` gains an `osv_note: Option<&str>` parameter. When `Some`,
the note is appended to the prompt before the final instruction line. The
`SingleSession` branch generates a note from `osv_signals.len()` when signals
are present; batch sessions and all other callers pass `None`.

`resolve_osv_signals` is an async free function that returns an empty signal
list immediately when no dependency manifests are present in the scan result.
Full per-package OSV scanning requires a parsed dependency manifest;
`ScanResult` currently provides manifest file paths only. The `OsvClient` is
complete and tested in isolation; per-package resolution is deferred until a
manifest parser is wired into the scanner.

## Design Decisions

- **Graceful degradation**: OSV failures (no manifests, future network errors)
  return an empty signal list rather than aborting the plugin run.
- **No API key required**: The OSV endpoint is public and unauthenticated;
  `osv_enabled` defaults to `true`.
- **CVSS v3 preferred**: When both CVSS_V3 and CVSS_V4 entries are present, the
  v3 base score is used as `primary_score`. v4 is retained separately for audit
  but never blended.
- **Absent severity is not an error**: `PYSEC-*` entries in the fixture have no
  `severity` key. The `#[serde(default)]` attribute on
  `VulnerabilityRecord.severity` deserialises absence as an empty vec, and
  `score_severity(&[])` returns `primary_score = 0.0` with `band = None`.

## Testing

| Location                      | Test                                                                 | Coverage                                        |
| ----------------------------- | -------------------------------------------------------------------- | ----------------------------------------------- |
| `config.rs`                   | `test_security_review_config_osv_enabled_defaults_to_true`           | Default value is `true`                         |
| `config.rs`                   | `test_security_review_config_osv_enabled_can_be_set_false`           | Field can be overridden                         |
| `plugin.rs`                   | `test_security_review_plugin_run_osv_disabled_skips_osv_resolution`  | Plugin completes when `osv_enabled=false`       |
| `plugin.rs`                   | `test_build_user_prompt_with_osv_note_includes_note`                 | OSV note appears in prompt                      |
| `plugin.rs`                   | `test_build_user_prompt_without_osv_note_has_no_osv_text`            | Prompt is clean without a note                  |
| `clients/vuln/osv/mod.rs`     | `test_osv_client_query_with_purl_returns_vulns`                      | PURL query succeeds and returns fixture records |
| `clients/vuln/osv/mod.rs`     | `test_osv_client_query_with_purl_empty_falls_back_to_name_ecosystem` | PURL empty triggers name+ecosystem fallback     |
| `clients/vuln/osv/mod.rs`     | `test_osv_client_query_all_empty_returns_ok_empty`                   | All strategies empty returns `Ok(vec![])`       |
| `clients/vuln/osv/mod.rs`     | `test_osv_client_query_http_error_returns_err`                       | HTTP 500 surfaces as `VulnClientError::Http`    |
| `clients/vuln/osv/scoring.rs` | 27 unit tests                                                        | All CVSS shapes, band boundaries, signal map    |
