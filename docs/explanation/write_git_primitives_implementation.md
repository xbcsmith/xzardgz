#Write Git Primitives: Phase 1 Implementation

## Overview

This document describes the design and implementation of Phase 1 of the write
git surface: branch creation, file staging and committing, and branch pushing to
a remote. The implementation lives in `src/git/write.rs` and is exported from
`src/git/mod.rs` as `pub mod write`.

## Design Goals

### Sandbox separation

The pre-existing `GitRepository` in `src/git/ops.rs` is deliberately read-only.
Plugins and scanners use that surface and are sandboxed away from mutating
state. The new `GitWriteRepository` in `src/git/write.rs` is a separate struct
with no shared type hierarchy, so the type system statically prevents plugins
from accidentally obtaining a write-capable handle.

### Error taxonomy

All errors map to existing `PipelineError` variants:

- `PipelineError::Governance` - branch name fails `validate_branch_name`
- `PipelineError::Git` - all libgit2 / push failures

No new error variants were introduced.

## Public API

### `GitWriteRepository`

```rust
pub struct GitWriteRepository {
    repo: git2::Repository,
    author_name: String,
    author_email: String,
}
```

The struct is opened with
`GitWriteRepository::open(path, author_name, author_email)`. The author identity
is stored at construction time and applied to every commit, avoiding the need to
pass it on each call.

### `default_branch_name`

Returns `"xzardgz/<ulid_lowercase>"`. The ULID encodes a millisecond timestamp
plus 80 bits of randomness, guaranteeing uniqueness across parallel workflow
executions without coordination.

### `create_branch`

1. Validates the name via `validate_branch_name` (returns `Governance` error on
   failure).
2. Resolves the current HEAD commit OID.
3. Creates the branch with `force = false` so duplicate names return an error
   rather than silently replacing an existing branch.
4. Calls `repo.set_head` to switch HEAD to the new branch.

### `commit_paths`

The implementation follows the parent-commit lifetime pattern recommended by the
git2 crate to avoid borrow-checker conflicts:

```rust
let parent_oid = self.repo.head().ok().and_then(|h| h.target());
let parent_commit = if let Some(oid) = parent_oid {
    Some(self.repo.find_commit(oid)?)
} else {
    None
};
let parents: &[&git2::Commit] = match parent_commit.as_ref() {
    Some(c) => &[c],
    None => &[],
};
```

Absolute paths are stripped of the workdir prefix before being passed to the
index, matching the behaviour of `git add <absolute-path>`. Rust's
`Path::strip_prefix` is component-aware, so trailing slashes in the workdir path
returned by libgit2 are handled correctly.

### `push_branch`

Credential resolution order:

1. `XZARDGZ_GITHUB_TOKEN` environment variable (`USER_PASS_PLAINTEXT` with
   username `x-access-token`, compatible with GitHub PATs and GitHub Actions
   `GITHUB_TOKEN`).
2. OS keyring `github_token` in service `xzardgz-github`.
3. SSH agent (`SSH_KEY` credential type).
4. libgit2 default credential (Kerberos/NTLM fallback).

For `file://` remotes (used in all unit tests) libgit2 does not invoke the
credential callback at all, so authentication is a no-op in tests.

The token is captured by move into the `FnMut` closure passed to
`RemoteCallbacks::credentials`, ensuring the closure is `'static`-compatible
without reference lifetimes.

### `current_branch`

Mirrors `GitRepository::current_branch` exactly. Returns `None` for detached
HEAD and for the unborn-branch state (empty repo). Returns
`Err(PipelineError::Git)` for all other HEAD read failures.

## Credential Resolution Helper

`resolve_github_token` (private free function) first queries `EnvVarStore` for
`XZARDGZ_GITHUB_TOKEN`, then falls back to `KeyringStore` for the same key in
the `xzardgz-github` service. Both stores implement the `SecretStore` trait, so
the lookup pattern is uniform.

## Test Strategy

The test suite in `write.rs` uses two helpers:

- `make_local_repo` - creates a tempdir with an initialised git repo and one
  initial commit containing `README.md`. Returns `(TempDir, git2::Repository)`.
- `make_write_repo_with_remote` - additionally creates a bare repo as `origin`,
  pushes the initial commit, and aligns the bare repo's HEAD so that downstream
  `git2::Repository::clone` calls succeed regardless of the system default
  branch setting. Returns `(bare_td, local_td, GitWriteRepository)`.

### Test coverage

| Test                                                            | What it verifies                                |
| --------------------------------------------------------------- | ----------------------------------------------- |
| `test_create_branch_produces_expected_head`                     | `current_branch()` reflects new branch          |
| `test_create_branch_with_invalid_name_returns_governance_error` | Governance guard fires before git               |
| `test_create_branch_with_duplicate_name_returns_error`          | `force=false` enforced                          |
| `test_default_branch_name_starts_with_xzardgz_prefix`           | Prefix contract                                 |
| `test_default_branch_name_has_unique_values`                    | ULID uniqueness                                 |
| `test_commit_paths_returns_40_char_sha`                         | Return value length                             |
| `test_commit_paths_file_appears_in_committed_tree`              | Tree content                                    |
| `test_commit_paths_with_absolute_path_succeeds`                 | Absolute path stripping                         |
| `test_push_branch_to_local_bare_remote_succeeds`                | Happy-path push                                 |
| `test_push_branch_with_invalid_name_returns_error`              | Governance guard                                |
| `test_push_branch_nonexistent_branch_returns_error`             | Push of missing ref                             |
| `test_round_trip_create_commit_push_verify_content`             | End-to-end: OID and file content in cloned repo |

The round-trip test clones the bare repo into a separate tempdir and verifies
both the commit OID and the blob content of `analysis.txt` via the remote
tracking branch `origin/feature/round-trip`.

## Files Changed

| File                                                      | Change                                            |
| --------------------------------------------------------- | ------------------------------------------------- |
| `src/git/write.rs`                                        | New file: `GitWriteRepository` and its test suite |
| `src/git/mod.rs`                                          | Added `pub mod write;`                            |
| `docs/explanation/write_git_primitives_implementation.md` | This document                                     |
