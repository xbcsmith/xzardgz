# Vulnerability Client Implementation

## Overview

This document describes the `src/clients/vuln/` module, which provides OSV
vulnerability lookup and CVSS scoring for the external data clients layer.

The module is split into three files:

| File                              | Role                                      |
| --------------------------------- | ----------------------------------------- |
| `src/clients/vuln/mod.rs`         | Shared types, error enum, async trait     |
| `src/clients/vuln/osv/mod.rs`     | OSV HTTP client with three-level fallback |
| `src/clients/vuln/osv/scoring.rs` | CVSS v3.x and v4.0 base-score computation |

## Component Boundary

The `clients::vuln` layer is a pure leaf in the dependency graph:

| Rule                    | Detail                          |
| ----------------------- | ------------------------------- |
| May depend on           | `auth`, `config`                |
| Must NOT call           | `scanner`, `providers`, `agent` |
| Must NOT be called from | `tools/`                        |

## Data Model

### VulnerabilityQuery

Callers populate at least one of the following lookup strategies:

- `purl` (PURL string, e.g. `pkg:pypi/jinja2@2.9.6`)
- `name` + `ecosystem` (e.g. `"jinja2"` / `"PyPI"`)
- `commit` (Git commit hash)

An optional `version` refines purl and name+ecosystem queries.

### VulnerabilityRecord

Mirrors the OSV schema. Key fields:

- `id` - advisory identifier (e.g. `GHSA-462w-v97r-4m45`)
- `severity` - `Vec<OsvSeverityEntry>` with CVSS vector strings
- `affected` - package and version-range data
- `database_specific` - opaque JSON for source-specific metadata

Unknown JSON fields are silently ignored during deserialization.

## OsvClient: Three-Level Fallback

`OsvClient` sends `POST https://api.osv.dev/v1/query` in up to three passes:

```text
Step 1 - PURL query
  If dep.purl is Some:
    POST {"package": {"purl": "<purl>"}, "version": "<v>"}
    On non-empty results: return immediately

Step 2 - Name + ecosystem query
  If Step 1 empty OR dep.purl is None:
    AND dep.name is non-empty AND dep.ecosystem is Some:
    POST {"package": {"name": "<name>", "ecosystem": "<eco>"}, "version": "<v>"}
    On non-empty results: return immediately

Step 3 - Commit query
  If Step 2 empty AND dep.commit is Some:
    POST {"commit": "<hash>"}
    On non-empty results: return immediately

Return Ok(vec![]) if all steps yield empty results.
```

An HTTP error at any step is returned immediately as `VulnClientError::Http`.
Empty results (absent or empty `vulns` key) advance to the next fallback.

### Constructor variants

| Constructor                     | Use case                                  |
| ------------------------------- | ----------------------------------------- |
| `OsvClient::new()`              | Production; targets `https://api.osv.dev` |
| `OsvClient::with_base_url(url)` | Tests; targets a wiremock server          |

## CVSS Scoring

### score_severity

`score_severity(&[OsvSeverityEntry]) -> OsvScore`

Iterates over severity entries, branches on `entry.r#type`:

- `"CVSS_V3"`: validates prefix (`CVSS:3.0/` or `CVSS:3.1/`), calls the v3
  scorer, and accumulates the maximum valid score as `cvss_v3_score`.
- `"CVSS_V4"`: validates prefix (`CVSS:4.0/`), calls the v4 scorer, and
  accumulates as `cvss_v4_score`.
- Other types: silently ignored.

A malformed or unrecognised vector degrades to `0.0` for that entry without
failing the call.

Primary-score selection:

1. v3 score when any valid `CVSS_V3` entry is present
2. v4 score when no valid v3 entry exists
3. `0.0` when neither is present or valid

### CVSS v3.x Formula (3.0 and 3.1 identical)

Vector format: `CVSS:3.1/AV:X/AC:X/PR:X/UI:X/S:X/C:X/I:X/A:X`

```text
ISC = 1.0 - (1.0 - C) * (1.0 - I) * (1.0 - A)

ISS (Scope Unchanged):  6.42 * ISC
ISS (Scope Changed):    7.52 * (ISC - 0.029) - 3.25 * (ISC - 0.02)^15

ESS = 8.22 * AV * AC * PR * UI

if ISS <= 0.0: base_score = 0.0
elif Scope Unchanged: base_score = roundup(min(ISS + ESS, 10.0))
else:                 base_score = roundup(min(1.08 * (ISS + ESS), 10.0))

roundup(x) = ceil(x * 10.0) / 10.0
```

Known test values:

| Vector                                         | Score      |
| ---------------------------------------------- | ---------- |
| `CVSS:3.0/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N` | 8.6 (HIGH) |
| `CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N` | 8.6 (HIGH) |

### CVSS v4.0 Formula (EQ-level lookup)

Vector format: `CVSS:4.0/AV:X/AC:X/AT:X/PR:X/UI:X/VC:X/VI:X/VA:X/SC:X/SI:X/SA:X`

EQ levels:

| EQ  | Metric   | Values                               |
| --- | -------- | ------------------------------------ |
| EQ1 | AV       | N=0, A=1, L=2, P=3                   |
| EQ2 | AC+AT    | (AC=L AND AT=N)=0, else=1            |
| EQ3 | PR+UI    | (both N)=0, (one N)=1, else=2        |
| EQ4 | VC/VI/VA | (any H)=0, (any L no H)=1, (all N)=2 |
| EQ5 | SC/SI/SA | (any H)=0, (any L no H)=1, (all N)=2 |

Score formula:

```text
base   = 10.0 - EQ1_W[eq1] - EQ2_W[eq2] - EQ3_W[eq3]
impact = 10.0 - min(EQ4_W[eq4] + EQ5_W[eq5], 8.0)
raw    = (base + impact) / 2.0
score  = roundup(clamp(raw, 0.0, 10.0))
```

Known test values:

| Vector                                                            | Score        |
| ----------------------------------------------------------------- | ------------ |
| `CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:N/VI:N/VA:N/SC:H/SI:N/SA:N` | 8.0 (HIGH)   |
| `CVSS:4.0/AV:L/AC:L/AT:P/PR:L/UI:P/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N` | 5.8 (MEDIUM) |

### CvssBand

| Band     | Range      |
| -------- | ---------- |
| Low      | 0.1 - 3.9  |
| Medium   | 4.0 - 6.9  |
| High     | 7.0 - 8.9  |
| Critical | 9.0 - 10.0 |

A score of `0.0` or below returns `None`.

### osv_score_to_signal

Maps an `OsvScore` to a `ScoringSignal::Negative` for use in the
`ConfidenceScorer` pipeline:

| Band     | Label                        | Weight |
| -------- | ---------------------------- | ------ |
| Low      | `osv-vulnerability-low`      | 0.10   |
| Medium   | `osv-vulnerability-medium`   | 0.25   |
| High     | `osv-vulnerability-high`     | 0.45   |
| Critical | `osv-vulnerability-critical` | 0.65   |

Returns `None` when `primary_score` is `0.0`.

## Tests

### Unit tests (scoring.rs) - 27 tests

Covers `CvssBand::from_score` boundaries, display, `score_severity` with empty
input, v3-only, v4-only, both versions, malformed vectors, and
`osv_score_to_signal` for each band.

Hand-computed CVSS vector values are verified to within 0.05 tolerance.

### Integration tests (osv/mod.rs) - 4 wiremock tests

| Test                                                                 | Description                                          |
| -------------------------------------------------------------------- | ---------------------------------------------------- |
| `test_osv_client_query_with_purl_returns_vulns`                      | PURL query returns fixture; first record ID verified |
| `test_osv_client_query_with_purl_empty_falls_back_to_name_ecosystem` | Empty purl response triggers name+ecosystem fallback |
| `test_osv_client_query_all_empty_returns_ok_empty`                   | All strategies empty returns `Ok(vec![])`            |
| `test_osv_client_query_http_error_returns_err`                       | HTTP 500 returns `VulnClientError::Http`             |

The fallback test uses wiremock mock priority (most-recently-mounted mock is
matched first) with `up_to_n_times(1)` to simulate the first call returning
empty and the second returning the fixture.

Fixture file used: `testdata/osv.dev.results.json`.

## Quality Gate Results

All four quality-gate commands pass with no warnings or failures:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

31 new tests added, 0 failures.
