# Phase 4: CVSS Scoring Calculator Implementation

## Overview

Phase 4 of the [external data clients plan](external_data_clients_plan.md) adds
CVSS base-score computation to the OSV vulnerability client introduced in
Phase 3. Raw CVSS vector strings embedded in OSV `severity[]` entries are parsed
and converted into numeric scores and a qualitative severity band. The resulting
`OsvScore` feeds directly into the `ConfidenceScorer` pipeline via
`osv_score_to_signal`.

## Deliverables

| Item                           | Location                          |
| ------------------------------ | --------------------------------- |
| `score_severity` function      | `src/clients/vuln/osv/scoring.rs` |
| `OsvScore` struct              | `src/clients/vuln/osv/scoring.rs` |
| `CvssBand` enum                | `src/clients/vuln/osv/scoring.rs` |
| `osv_score_to_signal` function | `src/clients/vuln/osv/scoring.rs` |
| `cvss-rs` dependency           | `Cargo.toml`                      |
| Public re-exports              | `src/clients/vuln/mod.rs`         |

## Changes

### `Cargo.toml`

Added `cvss-rs = "0.5.0"` to `[dependencies]`. The `cvss-rs` crate
(<https://github.com/scm-rs/cvss-rs>) provides `score_to_severity`, a function
that maps a floating-point CVSS score to a named severity band following the
NVD/FIRST banding table. This is used to back `CvssBand::from_score`.

### `src/clients/vuln/osv/scoring.rs` (new)

#### `CvssBand`

An enum representing the four CVSS severity bands:

| Variant    | Score range |
| ---------- | ----------- |
| `Low`      | 0.1 - 3.9   |
| `Medium`   | 4.0 - 6.9   |
| `High`     | 7.0 - 8.9   |
| `Critical` | 9.0 - 10.0  |

`CvssBand::from_score(score)` delegates to `cvss_rs::score_to_severity` and maps
the returned `cvss_rs::Severity` variant to the corresponding `CvssBand`. Both
`Option::None` and `Some(Severity::None)` (scores below 0.1 or out-of-range)
collapse to `Option::None`.

#### `OsvScore`

A scored result struct carrying:

- `cvss_v3_score: Option<f64>` - highest valid CVSS v3.x score found.
- `cvss_v4_score: Option<f64>` - highest valid CVSS v4.0 score found, retained
  for audit purposes only.
- `primary_score: f64` - the authoritative score: v3 when available, v4 when v3
  is absent, `0.0` when neither is valid.
- `band: Option<CvssBand>` - qualitative band derived from `primary_score`.

#### `score_severity`

`score_severity(severity: &[OsvSeverityEntry]) -> OsvScore`

Iterates over severity entries and branches on `entry.r#type`:

- `"CVSS_V3"`: validates prefix (`CVSS:3.0/` or `CVSS:3.1/`). OSV's
  `severity[].type == "CVSS_V3"` covers both versions under the same tag, so the
  parser branches on the vector string prefix rather than trusting `type` alone.
  Calls the custom v3 scorer and accumulates the highest valid score.
- `"CVSS_V4"`: validates prefix (`CVSS:4.0/`), calls the custom v4 scorer, and
  accumulates as `cvss_v4_score`.
- Unknown types: silently ignored.

A malformed or unrecognised vector string degrades to `0.0` for that entry
without failing the whole call.

Primary-score selection:

1. CVSS v3 score when any valid `CVSS_V3` entry is present.
2. CVSS v4 score when no valid v3 entry exists.
3. `0.0` when neither is present or valid.

#### CVSS v3.x Vector Scoring

Implements the NVD base-score formula for both 3.0 and 3.1 (the formulas are
identical):

```text
ISC = 1.0 - (1.0 - C) * (1.0 - I) * (1.0 - A)

ISS (Scope Unchanged): 6.42 * ISC
ISS (Scope Changed):   7.52 * (ISC - 0.029) - 3.25 * (ISC - 0.02)^15

ESS = 8.22 * AV * AC * PR * UI

base (Scope Unchanged): roundup(min(ISS + ESS, 10.0))
base (Scope Changed):   roundup(min(1.08 * (ISS + ESS), 10.0))
```

`roundup(x) = ceil(x * 10.0) / 10.0`

Known hand-computed values:

| Vector                                         | Score      |
| ---------------------------------------------- | ---------- |
| `CVSS:3.0/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N` | 8.6 (HIGH) |
| `CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N` | 8.6 (HIGH) |

#### CVSS v4.0 Vector Scoring

Implements the EQ-level lookup table approach operating on five equivalence
classes:

| EQ  | Metric(s) | Levels                               |
| --- | --------- | ------------------------------------ |
| EQ1 | AV        | N=0, A=1, L=2, P=3                   |
| EQ2 | AC + AT   | (AC=L AND AT=N)=0, else=1            |
| EQ3 | PR + UI   | (both N)=0, (one N)=1, else=2        |
| EQ4 | VC/VI/VA  | (any H)=0, (any L no H)=1, (all N)=2 |
| EQ5 | SC/SI/SA  | (any H)=0, (any L no H)=1, (all N)=2 |

Score formula:

```text
base   = 10.0 - EQ1_W[eq1] - EQ2_W[eq2] - EQ3_W[eq3]
impact = 10.0 - min(EQ4_W[eq4] + EQ5_W[eq5], 8.0)
raw    = (base + impact) / 2.0
score  = roundup(clamp(raw, 0.0, 10.0))
```

Known hand-computed values:

| Vector                                                            | Score        |
| ----------------------------------------------------------------- | ------------ |
| `CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:N/VI:N/VA:N/SC:H/SI:N/SA:N` | 8.0 (HIGH)   |
| `CVSS:4.0/AV:L/AC:L/AT:P/PR:L/UI:P/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N` | 5.8 (MEDIUM) |

#### `osv_score_to_signal`

Maps an `OsvScore` to a `ScoringSignal::Negative` for the `ConfidenceScorer`
pipeline:

| Band     | Label                        | Weight |
| -------- | ---------------------------- | ------ |
| Low      | `osv-vulnerability-low`      | 0.10   |
| Medium   | `osv-vulnerability-medium`   | 0.25   |
| High     | `osv-vulnerability-high`     | 0.45   |
| Critical | `osv-vulnerability-critical` | 0.65   |

Returns `None` when `primary_score` is `0.0` (no band assigned).

### `src/clients/vuln/mod.rs`

Adds public re-exports for `CvssBand`, `OsvScore`, and `score_severity` so
callers use the clean `clients::vuln::` path:

```rust
pub use osv::scoring::{CvssBand, OsvScore, score_severity};
```

## Design Decisions

### Custom vector scoring retained

`cvss-rs` is a JSON deserialisation library: it parses pre-computed CVSS JSON
objects (which carry the `baseScore` field alongside the vector string). It does
not offer a function that accepts a compact vector string and returns a numeric
score. OSV's `severity[].score` is a compact vector string only, so all
base-score computation must be performed locally. The two private functions
`score_cvss3` and `score_cvss4` implement the NVD formulas directly.

### CVSS v3 preferred over v4 as primary score

When a record carries both a `CVSS_V3` and a `CVSS_V4` entry, the v3 base score
is always used as `primary_score`. The v4 score is retained in `cvss_v4_score`
for audit purposes only and is never blended with the primary score. This aligns
with established industry practice of treating the v3 score as the canonical
signal while the v4 ecosystem matures.

### Absent severity is not an error

`PYSEC-*` entries in the OSV fixture have no `severity` key at all. The
`#[serde(default)]` attribute on `VulnerabilityRecord.severity` deserialises
absence as an empty `Vec`. `score_severity(&[])` returns `primary_score = 0.0`
with `band = None` and no error.

### Malformed vectors degrade gracefully

An unrecognised prefix or invalid metric value causes `score_cvss3` /
`score_cvss4` to return `0.0` for that one entry. The rest of the severity array
continues to be processed normally.

## Testing

Unit tests in `src/clients/vuln/osv/scoring.rs` cover all severity shapes
observed in real OSV data:

| Shape                        | Fixture representative            | Test                                                                    |
| ---------------------------- | --------------------------------- | ----------------------------------------------------------------------- |
| Empty `severity[]`           | All PYSEC entries                 | `test_score_severity_with_empty_slice_returns_zero`                     |
| No `severity` key            | PYSEC-2014-8, PYSEC-2014-82, etc. | `test_score_pysec_entry_with_no_severity_returns_zero`                  |
| `CVSS_V3` only               | GHSA-h5c8-rqwp-cp95 (3.1)         | `test_score_severity_with_only_cvss_v3_uses_v3`                         |
| `CVSS_V4` only               | GHSA-cpwx-vrp4-4pq7               | `test_score_severity_with_only_cvss_v4_uses_v4`                         |
| Both `CVSS_V3` and `CVSS_V4` | GHSA-462w-v97r-4m45               | `test_score_severity_with_both_cvss_v3_and_v4_prefers_v3`               |
| Malformed vector             | Synthetic                         | `test_score_severity_with_malformed_vector_returns_zero_for_that_entry` |

Hand-computed expected values:

| Vector                                                            | Expected score | Band   |
| ----------------------------------------------------------------- | -------------- | ------ |
| `CVSS:3.0/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N`                    | 8.6            | High   |
| `CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N`                    | 8.6            | High   |
| `CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:N/VI:N/VA:N/SC:H/SI:N/SA:N` | 8.0            | High   |
| `CVSS:4.0/AV:L/AC:L/AT:P/PR:L/UI:P/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N` | 5.8            | Medium |

## Success Criteria Verification

From the plan (section 4.6):

- **Empty or missing severity scores 0.0**: `score_severity(&[])` and
  `score_severity` over a record with no `severity` key both return
  `primary_score = 0.0`. Verified by
  `test_score_severity_with_empty_slice_returns_zero`.
- **Both CVSS versions: v3 is primary, v4 retained**: When both `CVSS_V3` and
  `CVSS_V4` entries are present, `primary_score` equals `cvss_v3_score` and
  `cvss_v4_score` is also populated. Verified by
  `test_both_cvss_versions_primary_is_v3_v4_retained`.

## Quality Gate Results

All four quality gates pass with zero warnings or failures:

```text
cargo fmt --all                                          ok
cargo check --all-targets --all-features                 ok
cargo clippy --all-targets --all-features -- -D warnings ok
cargo test --all-features                                446 passed, 0 failed
```
