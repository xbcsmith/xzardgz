# CVSS-RS Severity Band Mapping: Phase 4 Implementation

## Overview

Phase 4 of the external data clients plan integrates the `cvss-rs` crate
(`cvss-rs = "0.5.0"`) for severity-band mapping in the OSV vulnerability scoring
module. The `CvssBand::from_score` function now delegates band classification to
`cvss_rs::score_to_severity` rather than maintaining its own threshold
comparisons inline.

## What Changed

### `Cargo.toml`

Added the dependency:

```toml
cvss-rs = "0.5.0"
```

### `src/clients/vuln/osv/scoring.rs`

Two targeted changes were made:

1. **Module doc comment** -- extended to note that `cvss_rs::score_to_severity`
   provides the band mapping, and to explain why the custom vector-string
   scorers (`score_cvss3`, `score_cvss4`) remain local.

2. **`CvssBand::from_score`** -- replaced manual `if/else` threshold chains with
   a `match` on the `Option<cvss_rs::Severity>` returned by
   `cvss_rs::score_to_severity`.

## Design Rationale

### Why cvss-rs for band mapping only

`cvss-rs` is a JSON/serde deserialiser for CVSS objects; it does not expose a
function that evaluates a CVSS vector string to produce a numeric base score.
The two custom functions `score_cvss3` and `score_cvss4` therefore remain
unchanged: they implement the NVD base-score formula and the CVSS v4 EQ-level
lookup table respectively, and have no equivalent in the library.

The only thing `cvss-rs` contributes here is `score_to_severity`, which converts
a floating-point score into one of five named bands. This is a well-defined,
tested function that exactly mirrors the NVD / FIRST boundaries used throughout
the project:

| Score range | `cvss_rs::Severity` | `CvssBand`           |
| ----------- | ------------------- | -------------------- |
| < 0.1       | `None`              | `Option::None`       |
| 0.1 - 3.9   | `Low`               | `CvssBand::Low`      |
| 4.0 - 6.9   | `Medium`            | `CvssBand::Medium`   |
| 7.0 - 8.9   | `High`              | `CvssBand::High`     |
| 9.0 - 10.0  | `Critical`          | `CvssBand::Critical` |

### Match exhaustiveness

The `match` in `from_score` handles both the `None` branch and the
`Some(cvss_rs::Severity::None)` branch together. This is intentional:
`score_to_severity` may return `None` for out-of-range input, while
`Severity::None` represents a valid in-range score below `0.1`. Both cases map
to `Option::None` for `CvssBand`.

## Quality Gate Results

All four quality gates passed against the final state of the code:

```text
cargo fmt --all                              -- ok
cargo check --all-targets --all-features     -- ok
cargo clippy --all-targets --all-features \
  -- -D warnings                             -- ok
cargo test --all-features                    -- 446 tests passed, 0 failed
```

All pre-existing `CvssBand` boundary tests (zero, negative, each band, and each
band boundary) continue to pass without modification.
