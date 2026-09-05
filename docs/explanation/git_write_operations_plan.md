# Git Write Operations Implementation Plan

## Overview

`src/git/ops.rs::GitRepository` today only opens, clones, and checks out
existing branches — there is no branch creation, commit, push, or PR
creation anywhere in the codebase, and no GitHub API client at all beyond
Copilot's OAuth flow (which only yields a Copilot token, not a
repo-scoped PAT). This plan adds a write path — branch, commit, push, and
GitHub-only pull request creation — gated behind an explicit opt-in flag, so
a workflow can propose remediation output back to the source repository
instead of stopping at scan-and-report.

## Current State Analysis

### Existing Infrastructure

- `GitRepository::open`, `clone_repo` (with remote-URL validation via
  `git::governance::validate_remote_url`), `current_branch`, `head_commit`,
  `is_dirty`, and `checkout_branch` (existing local branches only, via
  `CheckoutBuilder::safe()`) are implemented and tested in `src/git/ops.rs`.
- `src/auth/store.rs::SecretStore` (with `KeyringStore` and `EnvVarStore`
  backends) already provides the credential-resolution pattern this plan
  will extend to a GitHub PAT.
- `src/governance/validator.rs`'s `GovernanceChecker::check_branch` already
  exists and can be reused for branch-naming enforcement.
- `src/workspace/stage.rs::WorkspaceStage` already models an explicit stage
  state machine that a new PR-creation stage can extend.

### Identified Issues

- No branch creation, commit, or push capability exists at all.
- No GitHub REST API client exists in the codebase; `reqwest` is currently
  used only for AI provider calls.
- Without this capability, xzardgz is scan-and-report only and cannot close
  the loop of proposing fixes back to a repository.

## Implementation Phases

### Phase 1: Branch, Commit, and Push Primitives

#### 1.1 Foundation Work

Add `create_branch(name)` to `GitRepository`: creates a new local branch from
the current `HEAD`. Naming is configurable with a timestamped, ULID-suffixed
default (per this project's general ULID-over-UUID preference), kept
separate from the read-only surface used during scanning.

#### 1.2 Add Foundation Functionality

Add `commit_paths(paths, message)` (stage the given output files and commit
with a governance-compliant message) and `push_branch(name)` (push via
HTTPS token or SSH key, resolved through `auth/store.rs`).

#### 1.3 Integrate Foundation Work

House these in a new `git::write` sub-module, distinct from the read-only
`git::ops` surface, so plugins never gain write access to a source checkout
through the same path used for scanning — preserving the existing sandbox
boundary.

#### 1.4 Testing Requirements

Unit tests against a temporary git repository fixture: branch creation
produces the expected `HEAD`, commit produces the expected tree, and push
against a local bare-repository remote succeeds and is verifiable via a
second clone.

#### 1.5 Deliverables

`git::write::{create_branch, commit_paths, push_branch}`.

#### 1.6 Success Criteria

A round-trip test — create branch, commit a file, push to a local bare
remote, verify content via a second clone — passes.

### Phase 2: GitHub PR Creation, Explicit Opt-In

#### 2.1 Feature Work

Add a `github` client module (REST API via `reqwest`) implementing PR
creation only — no broader GitHub API surface. GitHub is the only supported
host for this plan; GitLab is explicitly out of scope.

#### 2.2 Integrate Feature

Add an explicit opt-in flag (e.g. `--create-pr` / `pr.enabled`); with it
unset, no commit, push, or PR activity occurs even if output files were
written to the workspace. Add a corresponding stage to
`workspace/stage.rs`'s stage enum and a matching `ExecutionInput`/executor
entry point in `WorkflowExecutor`, so PR creation is dispatched the same way
every other stage is — no command handler calls the GitHub client directly.

#### 2.3 Configuration Updates

Extend `auth/store.rs`'s `SecretStore` pattern (env var, then keychain) to a
new GitHub PAT credential type. Add governance rules for branch naming and
for rejecting a PR targeting the default branch automatically, reusing
`GovernanceChecker::check_branch`.

#### 2.4 Testing Requirements

Mocked GitHub API tests (via the existing `wiremock` dependency) for PR
creation success and failure paths. An executor-level test asserting that
omitting the opt-in flag results in zero commit/push/PR activity regardless
of what output files exist.

#### 2.5 Deliverables

A `pr` execution stage that produces a real PR against a target GitHub
repository, verified via mocked tests in CI and manually via the
corresponding demo directory.

#### 2.6 Success Criteria

PR creation never occurs without explicit opt-in, and never targets a
repository's default branch automatically.
