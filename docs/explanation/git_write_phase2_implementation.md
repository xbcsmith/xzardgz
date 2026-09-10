# Git Write Operations Phase 2: GitHub PR Creation Implementation

## Overview

Phase 2 of the git write operations plan adds explicit-opt-in GitHub pull
request creation to the workflow executor. A new `ExecutionInput::CreatePr`
variant drives the feature, gated behind `config.pr.enabled` (default `false`).
No commit, push, or PR activity occurs unless the flag is explicitly set.

## Deliverables

| Deliverable                       | File                       | Status   |
| --------------------------------- | -------------------------- | -------- |
| `PrConfig` (enabled, draft)       | `src/config.rs`            | Complete |
| `WorkspaceStage::PrCreating`      | `src/workspace/stage.rs`   | Complete |
| `WorkspaceStage::PrComplete`      | `src/workspace/stage.rs`   | Complete |
| `GithubPrClient::create_pr`       | `src/clients/github/pr.rs` | Complete |
| `resolve_github_pat`              | `src/clients/github/pr.rs` | Complete |
| `ExecutionInput::CreatePr`        | `src/workflow/executor.rs` | Complete |
| `WorkflowExecutor::run_create_pr` | `src/workflow/executor.rs` | Complete |
| Executor no-opt-in test           | `src/workflow/executor.rs` | Complete |

## Configuration

### `PrConfig`

Added to `src/config.rs` as a top-level section on `Config`:

```rust
pub struct PrConfig {
    pub enabled: bool, // default false
    pub draft: bool,   // default false
}
```

Both fields default to `false`. PR creation is entirely inactive until
`pr.enabled = true` is set in the configuration file or programmatically.

### YAML configuration shape

```yaml
pr:
  enabled: true
  draft: false
```

## WorkspaceStage Changes

Two new variants were added to `WorkspaceStage` in `src/workspace/stage.rs`:

```rust
PrCreating { branch: String },
PrComplete {
    branch: String,
    pr_number: u64,
    pr_url: String,
},
```

`label()` returns `"pr_creating"` and `"pr_complete"` respectively. Both
variants are non-terminal (neither `is_complete()` nor `is_failed()` returns
`true` for them), consistent with the stage model for intermediate states.

## GitHub PR Client

### `GithubPrClient`

Located in `src/clients/github/pr.rs`. Posts to
`POST https://api.github.com/repos/{owner}/{repo}/pulls` via `reqwest`.

```rust
pub struct PrInput {
    pub owner: String,
    pub repo: String,
    pub head_branch: String,
    pub base_branch: String,
    pub title: String,
    pub body: Option<String>,
    pub draft: bool,
}

pub struct PrOutput {
    pub number: u64,
    pub html_url: String,
    pub state: String,
}
```

### `resolve_github_pat`

Credential resolution order:

1. `XZARDGZ_GITHUB_TOKEN` environment variable.
2. OS keyring service `xzardgz-github`, key `github_token`.

Returns `None` when neither source provides a token. The client accepts `None`
and will attempt the API call without a token, which GitHub will reject with a
`401`; this surfaces as `PrClientError::Http` to callers.

### Governance rules

`PrClientError::HeadEqualsBase` is returned when `head_branch == base_branch`,
preventing trivially invalid PRs before the network call is made. The executor
maps all `PrClientError` variants to `PipelineError::Git`.

## ExecutionInput::CreatePr

Added to the `ExecutionInput` enum in `src/workflow/executor.rs`:

```rust
CreatePr {
    repository: String,
    head_branch: String,
    base_branch: String,
    owner: String,
    repo: String,
    title: String,
    body: Option<String>,
    draft: bool,
    workspace: Option<String>,
},
```

`repository` and `workspace` are accepted but unused in this phase (prefixed
with `_` in `run_create_pr`). They are reserved for the future push-before-PR
integration described in the plan.

## Opt-In Gate

`WorkflowExecutor::run_create_pr` checks `self.config.pr.enabled` as its first
action. When `false`, it returns an immediate `ExecutionResult` with:

- `success: true`
- `errors: []`
- `stage_at_completion: WorkspaceStage::Complete`
- `is_dry_run: false`

No git operations, no network calls, and no workspace side effects occur.

## Test Strategy

### Unit tests

| Test                                                                        | Coverage                           |
| --------------------------------------------------------------------------- | ---------------------------------- |
| `test_pr_config_enabled_defaults_to_false`                                  | `PrConfig` default                 |
| `test_pr_config_draft_defaults_to_false`                                    | `PrConfig` default                 |
| `test_pr_creating_label_returns_expected_string`                            | `WorkspaceStage::PrCreating` label |
| `test_pr_complete_label_returns_expected_string`                            | `WorkspaceStage::PrComplete` label |
| `test_pr_creating_is_not_complete_or_failed`                                | Stage terminal predicates          |
| `test_pr_complete_is_not_terminal`                                          | Stage terminal predicates          |
| `test_pr_creating_serializes_and_deserializes_correctly`                    | Serde round-trip                   |
| `test_pr_complete_serializes_and_deserializes_correctly`                    | Serde round-trip                   |
| `test_execute_create_pr_without_opt_in_returns_success_with_no_pr_activity` | Executor opt-in gate               |

### Wiremock tests (in `src/clients/github/pr.rs`)

Eight mock HTTP tests cover:

- PR creation success (201 response)
- HTTP 422 Unprocessable Entity error
- HTTP 401 Unauthorized error
- Response parse failure
- `HeadEqualsBase` governance error
- Missing token path
- Draft PR flag forwarded correctly
- Custom base URL override

## Files Changed

| File                                                  | Change                                       |
| ----------------------------------------------------- | -------------------------------------------- |
| `src/config.rs`                                       | Added `PrConfig` and `Config::pr` field      |
| `src/workspace/stage.rs`                              | Added `PrCreating` and `PrComplete` variants |
| `src/clients/github/mod.rs`                           | New module re-exporting PR client types      |
| `src/clients/github/pr.rs`                            | New file: `GithubPrClient` implementation    |
| `src/clients/mod.rs`                                  | Added `pub mod github;`                      |
| `src/workflow/executor.rs`                            | Added `CreatePr` variant and `run_create_pr` |
| `docs/explanation/github_pr_client_implementation.md` | PR client design document                    |
| `docs/explanation/git_write_phase2_implementation.md` | This document                                |
