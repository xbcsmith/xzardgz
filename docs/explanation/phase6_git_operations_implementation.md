# Phase 6: Git Operations Implementation

## Overview

Phase 6 introduces a comprehensive `git` module (`src/git/`) that replaces the
minimal `src/repository/git.rs` stub. The new module provides safe,
non-destructive git operations used throughout the XZardgz pipeline, including
local repository introspection, remote cloning, branch management, dirty-state
detection, and governance-layer validation of branch names, paths, and remote
URLs.

## Module Layout

```text
src/git/
  mod.rs         Re-exports all three submodules
  governance.rs  Validation and URL normalization functions
  metadata.rs    GitMetadata struct (feeds workspace state and scan artifacts)
  ops.rs         GitRepository struct (wraps git2::Repository)
```

The previous `src/repository/git.rs` file is now a thin re-export shim that
preserves backward compatibility for any future code that imports from the old
path:

```text
src/repository/git.rs   pub use crate::git::ops::GitRepository;
```

## Components

### `src/git/governance.rs`

Provides five public functions used to validate inputs and normalize values
before any git operation is performed.

| Function                   | Purpose                                                                               |
| -------------------------- | ------------------------------------------------------------------------------------- |
| `validate_branch_name`     | Enforces git-check-ref-format rules; returns `PipelineError::Governance` on violation |
| `validate_repository_path` | Checks that a path exists and is a directory                                          |
| `validate_remote_url`      | Rejects empty strings and unsupported URL schemes                                     |
| `normalize_url`            | Strips trailing slashes, `.git` suffix, lowercases; never fails                       |
| `hash_url`                 | SHA-256 of the normalized URL as a 64-char lowercase hex string                       |

Branch name validation enforces nine rules drawn from `git-check-ref-format`:

1. Must not be empty.
2. Must not exceed 250 characters.
3. Must not contain space, tilde, caret, colon, question mark, asterisk,
   backslash, or open bracket.
4. Must not contain `..` (consecutive dots).
5. Must not start with `/`.
6. Must not end with `/`.
7. Must not end with `.` (single trailing dot).
8. Must not end with `.lock`.
9. Must not contain ASCII control characters (code points below 32 or equal to
   127).

### `src/git/metadata.rs`

Defines `GitMetadata`, the serializable snapshot of all git-related state for a
repository checkout.

```text
GitMetadata {
    repository_url:          Option<String>  remote origin URL
    repository_hash:         Option<String>  SHA-256 of normalized URL
    local_repository_path:   String          absolute path to working directory
    branch_name:             Option<String>  current branch (None = detached HEAD)
    target_branch:           Option<String>  branch requested by workflow or CLI
    head_commit:             Option<String>  40-char SHA-1 of HEAD (None = no commits)
    is_dirty:                bool            staged or unstaged changes to tracked files
}
```

`GitMetadata` derives `Debug`, `Clone`, `Default`, `Serialize`, and
`Deserialize`. The `Default` implementation sets `is_dirty` to `false` and all
`Option` fields to `None`, which is safe to use as a zero-value.

### `src/git/ops.rs`

Defines `GitRepository`, a safe wrapper around `git2::Repository`.

| Method            | Description                                                       |
| ----------------- | ----------------------------------------------------------------- |
| `open`            | Opens an existing local repository                                |
| `is_repository`   | Returns `true` if a path contains a git repository                |
| `clone_repo`      | Validates the URL then clones from a remote                       |
| `current_branch`  | Returns the branch name or `None` for detached HEAD               |
| `head_commit`     | Returns the 40-char hex SHA-1 of HEAD                             |
| `is_dirty`        | Returns `true` when tracked files have staged or unstaged changes |
| `checkout_branch` | Switches to an existing local branch (safe checkout only)         |
| `path`            | Returns the repository working directory path                     |
| `metadata`        | Collects a complete `GitMetadata` snapshot                        |

All operations are non-destructive by design:

- `clone_repo` uses the standard (non-force) protocol.
- `checkout_branch` uses `git2::build::CheckoutBuilder::safe()`, which aborts
  rather than overwriting local modifications.
- No force-push, hard-reset, or branch-delete operations are exposed.

The `is_dirty` check excludes untracked files; only staged or unstaged changes
to already-tracked files set the flag. This matches the semantics most useful
for governance checks: a repository containing only untracked files should not
block a scan run.

## Workspace Integration

Two new methods propagate `GitMetadata` into the workspace state layer:

### `WorkspaceState::apply_git_metadata`

Updates the workspace state fields from a `GitMetadata` snapshot. Fields are
only overwritten when the corresponding metadata field is `Some(_)`, preserving
previously recorded values when metadata is partial.

Fields updated:

- `repository_url` (only when metadata has a remote URL)
- `repository_hash` (only when metadata has a hash)
- `local_repository_path` (always)
- `branch_name` (always)
- `target_branch` (only when metadata has a target)
- `scan_artifact_head_commit` (always)
- `updated_at` (always refreshed to current UTC time)

### `WorkspaceManager::apply_git_metadata`

Calls `WorkspaceState::apply_git_metadata` then immediately saves the updated
state to disk. Returns `PipelineError::Workspace` if the file write fails.

## Governance Integration

All git operations follow the governance layer:

- `clone_repo` calls `validate_remote_url` before any network activity.
- `checkout_branch` finds branches by name only (no creation, no force).
- URL normalization is performed before hashing so that
  `https://github.com/example/repo` and `https://github.com/example/repo.git`
  produce the same hash.

## Testing

Phase 6 adds tests across five locations:

| File                     | New tests |
| ------------------------ | --------- |
| `src/git/governance.rs`  | 22        |
| `src/git/metadata.rs`    | 3         |
| `src/git/ops.rs`         | 13        |
| `src/workspace/state.rs` | 7         |
| `src/workspace/mod.rs`   | 2         |

Total: 47 new tests.

All git operation tests use `tempfile::tempdir()` and `git2::Repository::init()`
to create isolated repositories with no external dependencies. Tests cover:

- Local repository opening and detection.
- Non-repository path errors.
- Branch name validation (valid and each forbidden pattern).
- HEAD commit detection (with and without commits).
- Dirty status detection (staged changes, unstaged changes, untracked-only).
- Safe checkout of existing branches.
- Repository URL hashing (determinism, normalization).
- `apply_git_metadata` propagation and disk persistence.

## Success Criteria

Phase 6 is complete when:

- `cargo fmt --all` produces no changes.
- `cargo check --all-targets --all-features` emits no errors.
- `cargo clippy --all-targets --all-features -- -D warnings` emits no warnings.
- `cargo test --all-features` passes all tests.
- Workflows can operate on local and remote repositories.
- Scan artifacts include meaningful git metadata (branch, HEAD commit, dirty
  flag, remote URL hash).
