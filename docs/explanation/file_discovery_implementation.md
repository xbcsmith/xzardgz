# File Discovery Module Implementation

## Overview

The file-discovery module (`scanner::sast::target::discover`) provides
gitignore-aware, pattern-filtered, size-bounded, and binary-safe file
enumeration for the SAST engine. Its primary entry point is `discover_files`,
which walks a directory tree and returns a sorted, deterministic list of paths
that are safe to pass to the scanning pipeline.

## Module Layout

```text
src/scanner/sast/target/
    mod.rs        -- submodule declarations
    discover.rs   -- discover_files, DiscoveryConfig, is_binary
    prefilter.rs  -- reserved for future content pre-filtering
```

## Design Decisions

### Gitignore Awareness via the `ignore` Crate

The walker is built with `ignore::WalkBuilder` configured to respect:

- `.gitignore` files anywhere in the directory tree (`git_ignore(true)`)
- The global gitignore configured by the user (`git_global(true)`)
- The per-repository exclude file at `.git/info/exclude` (`git_exclude(true)`)
- Hidden files are walked (`hidden(false)`); `.gitignore` decides what to skip

This matches the behaviour a developer sees at the command line: files that git
ignores are also ignored by the SAST scanner. The `ignore` crate honours
`.gitignore` semantics even when the scanned directory is not inside a git
repository.

### Filter Order

Filters are applied in a strict order to keep the logic predictable:

1. Gitignore / ignore-crate rules (enforced by the walker before yielding)
2. Exclude glob patterns (applied to path relative to root)
3. Include glob patterns (applied to path relative to root)
4. Maximum file size (`max_file_bytes`)
5. Binary detection (NUL-byte heuristic on first 8,192 bytes)

Exclusion is evaluated before inclusion so that an exclude pattern cannot be
overridden by an include pattern. This follows the principle of least surprise:
if a file is excluded, it stays excluded regardless of whether it would match an
include glob.

### Relative Path Matching for Globs

Glob patterns are matched against the path _relative to the scan root_, not the
absolute path. This makes patterns portable: `*.rs` matches any Rust file
directly in the root, and `**/*.rs` matches Rust files anywhere in the tree. The
relative path is obtained via `Path::strip_prefix(root)`, falling back to the
absolute path if stripping fails.

### Binary Detection Heuristic

The `is_binary` function reads up to 8,192 bytes and checks for the presence of
a NUL byte (`\0`). This is the same heuristic used by git itself. It is fast
(one syscall, bounded read) and reliable in practice: text files almost never
contain NUL bytes, while binary formats (ELF, PE, compressed archives, images)
nearly always do.

On read failure, `is_binary` returns `false` (treat as text), erring on the side
of inclusion. A file that cannot be opened will fail later in the pipeline with
a proper `SastError::FileRead` rather than being silently dropped.

### Determinism via Sorting

Directory-walk order is undefined across platforms and file systems. All results
are sorted lexicographically before being returned, ensuring that scan output is
stable across runs and platforms. This matters for result diffing and
reproducible CI pipelines.

### Error Propagation

Walk errors and glob-compilation errors are surfaced as `SastError::FileRead`
with the scan root as the path and a human-readable cause. Per-file errors (size
check, binary detection) are silent skips rather than hard errors, because a
locked or unreadable file in a large repo should not abort the entire scan.

## Public API

### `DiscoveryConfig`

```rust
pub struct DiscoveryConfig {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}
```

Both fields default to empty vecs. An empty `include` slice means "no include
filter" (all non-excluded files pass).

### `discover_files`

```rust
pub fn discover_files(
    root: &Path,
    config: &DiscoveryConfig,
    max_file_bytes: u64,
) -> Result<Vec<PathBuf>, SastError>
```

Returns a sorted `Vec<PathBuf>` of absolute paths. Returns `SastError::FileRead`
on walk I/O errors or invalid glob patterns.

### `is_binary` (crate-internal)

```rust
pub(crate) fn is_binary(path: &Path) -> bool
```

Exposed at crate visibility to allow direct unit testing. Not part of the public
API.

## Test Coverage

Ten unit tests cover the full behaviour matrix:

| Test                                                         | Assertion                                         |
| ------------------------------------------------------------ | ------------------------------------------------- |
| `test_discover_files_returns_sorted_paths`                   | Output is lexicographically sorted                |
| `test_discover_files_respects_gitignore`                     | `.gitignore`-listed files are absent              |
| `test_discover_files_skips_binary_files`                     | NUL-byte files are silently skipped               |
| `test_discover_files_enforces_max_file_bytes`                | Oversized files are silently skipped              |
| `test_discover_files_include_pattern_filters`                | Only matching files pass a non-empty include list |
| `test_discover_files_exclude_pattern_filters`                | Matching files are absent from results            |
| `test_discover_files_empty_include_returns_all_non_excluded` | Empty include = no include filter                 |
| `test_discover_files_skips_directories`                      | No directory paths appear in the output           |
| `test_is_binary_with_null_byte_returns_true`                 | NUL byte detected correctly                       |
| `test_is_binary_without_null_byte_returns_false`             | Clean text file not misidentified                 |

All tests use `tempfile::TempDir` for isolated, portable file system operations.
