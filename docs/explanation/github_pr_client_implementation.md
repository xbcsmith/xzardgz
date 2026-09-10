# GitHub PR Client Implementation

This document explains the design and implementation of the GitHub pull request
creation client added in `src/clients/github/`.

## Overview

The GitHub PR client is a thin async wrapper around the GitHub REST API endpoint
`POST /repos/{owner}/{repo}/pulls`. It is scoped strictly to PR creation; no
other GitHub API operations are supported.

## Module Structure

```text
src/clients/github/
    mod.rs   - module declaration and public re-exports
    pr.rs    - GithubPrClient, PrInput, PrOutput, PrClientError, resolve_github_pat
```

The module is registered under `src/clients/mod.rs` alongside the existing
`scorecard`, `repodata`, and `vuln` clients.

## Component Boundary

The github sub-module follows the same boundary rules as the rest of
`src/clients/`:

| Rule          | Detail                          |
| ------------- | ------------------------------- |
| May depend on | `auth`, `config`                |
| Must NOT call | `scanner`, `providers`, `agent` |
| Must NOT be   | called from `tools/`            |

This keeps the data-fetching layer a pure leaf in the dependency graph.

## Key Types

### GithubPrClient

The client struct holds a `reqwest::Client`, the API base URL, and an optional
PAT. All request headers (Authorization, Accept, User-Agent,
X-GitHub-Api-Version, Content-Type) are set per request inside `create_pr`.

Using `with_base_url` allows tests to target a local wiremock server without any
real network traffic.

### PrInput / PrOutput

Plain data structs. `PrInput` carries the owner, repo, branch names, title,
optional body, and draft flag. `PrOutput` carries the PR number, HTML URL, and
state string.

### PrClientError

A dedicated `thiserror`-derived enum with four variants:

- `Http { status, message }` - non-2xx response from GitHub.
- `Parse(String)` - response body could not be deserialized.
- `HeadEqualsBase(String)` - validated before the HTTP call.
- `MissingToken` - validated before the HTTP call.

The error type is not mapped to `PipelineError` inside the client. Callers
(executors) handle that mapping so the client stays free of upper-layer
dependencies.

## Token Resolution

`resolve_github_pat` checks two sources in order:

1. Environment variable `XZARDGZ_GITHUB_TOKEN` via `EnvVarStore`.
2. OS keyring key `github_token` in service `xzardgz-github` via `KeyringStore`.

This mirrors the pattern used by the OpenAI and Anthropic auth modules.

## Serialization

Internal serde structs (`CreatePrRequest`, `CreatePrResponse`) are private to
the module. `CreatePrRequest` uses
`#[serde(skip_serializing_if = "Option::is_none")]` on the `body` field to omit
the key entirely when no body is provided, which matches GitHub API
expectations.

## Tests

Eight unit tests are included in a `#[cfg(test)]` block at the bottom of
`pr.rs`. All async tests use `#[tokio::test]`. A wiremock `MockServer` is
started per test so no real network calls are made.

| Test name                                                  | Scenario                                |
| ---------------------------------------------------------- | --------------------------------------- |
| `test_create_pr_with_valid_input_returns_pr_output`        | Happy path; 201 response                |
| `test_create_pr_http_error_returns_pr_client_error`        | 422 response maps to `Http` variant     |
| `test_create_pr_without_token_returns_missing_token_error` | `None` token returns `MissingToken`     |
| `test_create_pr_with_same_head_and_base_returns_error`     | Equal branches return `HeadEqualsBase`  |
| `test_create_pr_sends_authorization_header`                | Header matcher verifies Bearer token    |
| `test_create_pr_with_draft_true_sends_draft_flag`          | draft=true passes through               |
| `test_resolve_github_pat_returns_none_when_not_set`        | Env var absent; function does not panic |
| `test_resolve_github_pat_returns_token_from_env_var`       | Env var set; token returned             |
