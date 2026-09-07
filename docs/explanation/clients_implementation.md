# Phase 1 Clients Module Implementation

## Overview

This document explains the design and implementation of the `src/clients/`
module introduced in Phase 1 of the external data clients plan. The module
provides a thin, testable HTTP and file-system layer for fetching supply-chain
signals from two external sources: the OpenSSF Scorecard REST API and the GitHub
Repository REST API.

## Module Structure

```text
src/clients/
    mod.rs        - boundary contract, parse_github_slug, ExternalSignals
    scorecard.rs  - ScorecardResult types, fetch/resolve functions, tests
    repodata.rs   - RepoMetadata types, fetch/resolve functions, tests
```

## Component-Boundary Contract

The `clients` module sits at the boundary between the pipeline core and the
external world. It is permitted to depend on `auth` (for credential lookup) and
`config` (for future configuration hooks). It must never be called from `tools/`
and must never call into `scanner`, `providers`, or `agent`. This prevents
circular imports and keeps the data-fetching layer as a pure leaf in the
dependency graph.

## Slug Parsing

`parse_github_slug` accepts five input formats:

- `owner/repo`
- `github.com/owner/repo`
- `https://github.com/owner/repo`
- `https://github.com/owner/repo.git`
- `git@github.com:owner/repo.git`

The function rejects non-GitHub hosts by detecting a dot in the first path
segment (e.g., `gitlab.com`) and rejects non-GitHub SSH hosts by matching the
`git@` prefix and requiring exactly `git@github.com:`. This avoids the need for
a URL parser dependency and keeps the logic explicit.

## Resolution Strategy

Both `scorecard.rs` and `repodata.rs` implement the same two-level fallback
pattern:

1. Check for a pre-fetched JSON file (`scorecard.json` / `repodata.json`) in the
   workspace root. If found, parse and return it immediately. This path makes no
   network requests and enables offline and cached use cases.
2. Fall back to the corresponding REST API. On failure, log the error at `DEBUG`
   level via `tracing` and return the `AllSourcesExhausted` error variant.

Local file read or parse errors are surfaced immediately (they do not fall
through to the remote path) so that a corrupted local file does not silently
trigger a network request.

## Authentication

Repository metadata requests to the GitHub API are optionally authenticated.
`resolve_repodata` reads `GITHUB_TOKEN` from the environment via `EnvVarStore`
from the `auth` module. When the variable is absent or `EnvVarStore::get_secret`
returns an error, an unauthenticated request is made. This approach respects the
`SecretStore` abstraction and avoids any direct `std::env::var` calls outside
the `auth` module.

## Error Design

Each sub-module defines its own `thiserror`-based error enum
(`ScorecardResolveError`, `RepoDataResolveError`) with named-field variants for
structured errors (`Http`, `LocalFile`) and tuple variants for simpler cases
(`Parse`, `AllSourcesExhausted`, `InvalidRepo`). These errors are intentionally
separate from `PipelineError` so that callers can pattern-match on specific
failure modes without stringly-typed comparisons.

## Testability

The internal `fetch_scorecard_from` and `fetch_repodata_from` functions are
`pub(crate)` and accept a `base_url` parameter. This allows unit tests to inject
a `wiremock` mock server URL in place of the production base URL, exercising
HTTP success and error paths without network access. The `resolve_*` functions
are tested against the local-file path using `tempfile::tempdir` and against the
invalid-repo path using a known-bad identifier.

## ExternalSignals

`ExternalSignals` is a plain aggregate struct that holds optional results from
both resolvers. Its `is_empty` method returns `true` when both fields are
`None`, providing a single check that callers can use to determine whether any
external signal data was retrieved.
