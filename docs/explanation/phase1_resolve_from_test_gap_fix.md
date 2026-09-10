# Phase 1.4 Test Gap Fix: `resolve_scorecard_from` and `resolve_repodata_from`

## Problem

Phase 1.4 required wiremock tests that exercise the remote fallback path through
the public `resolve_scorecard` and `resolve_repodata` functions when no local
cache file is present. The existing tests
`test_resolve_scorecard_fetches_remote_when_no_local_file` and
`test_resolve_repodata_fetches_remote_when_no_local_file` bypassed the resolve
functions entirely, calling the lower-level `fetch_scorecard_from` and
`fetch_repodata_from` helpers directly. That left the local-miss-to-remote-fetch
branching logic inside `resolve_scorecard` and `resolve_repodata` untested via
wiremock.

The root cause was a testability gap: both public resolve functions called their
respective fetch helpers with a hardcoded production URL constant
(`SCORECARD_API_BASE`, `GITHUB_API_BASE`), so there was no seam to inject a mock
server URL without a network call to the real API.

## Solution

A `_from` variant was added for each resolver, following the same pattern
already used by `fetch_scorecard_from` and `fetch_repodata_from`:

- `pub(crate) async fn resolve_scorecard_from(repo, workspace_root, base_url)`
  -- contains the full two-level fallback logic (local file check, then remote
  fetch via `fetch_scorecard_from(repo, base_url)`).
- `pub(crate) async fn resolve_repodata_from(repo, workspace_root, base_url)` --
  mirrors the same pattern for GitHub repo metadata.

The public functions now delegate to these internal variants:

```rust
pub async fn resolve_scorecard(repo, workspace_root) {
    resolve_scorecard_from(repo, workspace_root, SCORECARD_API_BASE).await
}

pub async fn resolve_repodata(repo, workspace_root) {
    resolve_repodata_from(repo, workspace_root, GITHUB_API_BASE).await
}
```

This preserves the unchanged public API while enabling the test code to inject a
wiremock server URI as `base_url`.

## New Tests

### `src/clients/scorecard.rs`

`test_resolve_scorecard_with_no_local_file_falls_back_to_remote`

- Creates an empty `tempdir` (no `scorecard.json`).
- Starts a `wiremock::MockServer` and mounts a
  `GET /projects/github.com/ossf/scorecard` handler that returns the fixture
  JSON.
- Calls
  `resolve_scorecard_from("ossf/scorecard", workspace, &mock_server.uri())`.
- Asserts `Ok`, checks `score == 7.5`, `repo.name`, and `checks.len() == 1`.

### `src/clients/repodata.rs`

`test_resolve_repodata_with_no_local_file_falls_back_to_remote`

- Creates an empty `tempdir` (no `repodata.json`).
- Starts a `wiremock::MockServer` and mounts a `GET /repos/ossf/scorecard`
  handler that returns the fixture JSON.
- Calls
  `resolve_repodata_from("ossf/scorecard", workspace, &mock_server.uri())`.
- Asserts `Ok`, checks `full_name`, `stargazers_count`, and `language`.

## Quality Gate Results

All four gates passed with no warnings:

```text
cargo fmt --all                                              -- ok
cargo check --all-targets --all-features                    -- ok (0.12s)
cargo clippy --all-targets --all-features -- -D warnings    -- ok (3.20s)
cargo test --all-features                                   -- 1743 tests, 0 failures
```
