# Phase 4.5 Deliverable Gap Fixes

## Summary

This document describes the fixes applied to close deliverable gaps identified
against the Phase 4.5 milestone: a missing re-export, and three missing
`# Examples` blocks in public API documentation.

---

## Changes

### 1. Re-export of `osv_score_to_signal` (`src/clients/vuln/mod.rs`)

`osv_score_to_signal` was defined in `src/clients/vuln/osv/scoring.rs` but was
not re-exported from the `vuln` module facade. Callers outside the crate would
have had no stable import path for the function.

The re-export line was updated from:

```rust
pub use osv::scoring::{CvssBand, OsvScore, score_severity};
```

to:

```rust
pub use osv::scoring::{CvssBand, OsvScore, osv_score_to_signal, score_severity};
```

The function is now accessible as `xzardgz::clients::vuln::osv_score_to_signal`.

### 2. `# Examples` on `osv_score_to_signal` (`src/clients/vuln/osv/scoring.rs`)

The function had complete `///` documentation (description, band-to-weight
table, arguments, returns) but no runnable example. A compilable and executable
doc-test was added that constructs an `OsvScore` with a `High` band and asserts
that the returned signal is `Some`.

### 3. `# Examples` on `OsvClient::with_base_url` (`src/clients/vuln/osv/mod.rs`)

The constructor had argument, return, and intent documentation but no example. A
minimal doc-test was added showing how to construct a client pointed at a custom
base URL, which is the primary use-case for test-server redirection.

### 4. `# Examples` on `VulnerabilityRecord`, `VulnClientError`, and

`VulnerabilitySource` (`src/clients/vuln/mod.rs`)

Three public types in the `vuln` module facade were missing examples:

- `VulnerabilityRecord`: added a construction example asserting the `id` field.
- `VulnClientError`: added an example constructing the `Http` variant and
  verifying the `Display` output via `to_string()`.
- `VulnerabilitySource`: added a `no_run` example showing an async function that
  accepts `&dyn VulnerabilitySource` and calls `query`, demonstrating the
  trait's intended usage pattern without requiring a runtime in the doc-test
  harness.

---

## Quality Gates

All four gates passed after the changes:

| Gate                                                       | Result                |
| ---------------------------------------------------------- | --------------------- |
| `cargo fmt --all`                                          | clean                 |
| `cargo check --all-targets --all-features`                 | clean                 |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean                 |
| `cargo test --all-features`                                | 1741 passed, 0 failed |
