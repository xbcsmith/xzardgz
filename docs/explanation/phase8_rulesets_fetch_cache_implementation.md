# Phase 8 Rulesets Fetch and Cache Implementation

## Overview

Phase 8 adds two operational modules to `src/clients/rulesets/`:

- `fetch.rs` - low-level acquisition functions (local path, Git, file counting,
  licence detection)
- `cache.rs` - content-addressed on-disk manifest cache

It also wires both modules into the module tree via `mod.rs` and extends the
SAST rule resolver (`src/scanner/sast/rule/resolve.rs`) with a function that
loads rules from the cache.

## Module Layout

```text
src/clients/rulesets/
  cache.rs     <- new: content-addressed cache
  error.rs     <- struct-based error variants (Git/Io/Http/...)
  fetch.rs     <- new: path/git/file-counting helpers
  license.rs   <- LicenseClass enum + classifier
  manifest.rs  <- BundleManifest serde type
  mod.rs       <- module declarations + backward-compat wrappers
  source.rs    <- RulesetSource enum (Path/Git/Url/Oci)
  sources.rs   <- SourcesManifest type alias + RulesetSourceEntry
```

## Boundary Contract

`src/clients/rulesets/` must never import from `src/scanner/sast/`. This is
verified in CI by:

```shell
grep -rn 'scanner::sast' src/clients/rulesets/
```

The only hit is a documentation comment in `mod.rs`, not an import.

## `fetch.rs` Design

### `fetch_path_source`

Validates that the supplied path exists and is a directory, then returns a
`FetchResult` with no resolved commit. Used for the `RulesetSource::Path`
variant.

### `fetch_git_source`

Uses `git2::build::RepoBuilder` with `FetchOptions::depth(1)` for a shallow
clone. The function is defensive:

1. If `target_dir` exists and already contains a repository at the requested
   commit (prefix match), it returns immediately without a network call.
2. If the directory exists but does not match, it is removed via
   `std::fs::remove_dir_all` before re-cloning.
3. After cloning, `HEAD` is peeled to a `Commit` and the 40-char hex SHA is
   extracted. If a pinned commit was requested, a prefix match is verified.

### `count_yaml_files`

Walks a directory recursively with `walkdir` and counts files with `.yaml` or
`.yml` extensions. Used to populate `BundleManifest::rule_count`.

### `read_license_file`

Checks for the standard licence file names in order: `LICENSE`, `LICENSE.txt`,
`LICENSE.md`, `COPYING`, `COPYING.txt`. Case-sensitive on Unix. Returns the raw
text of the first file found.

## `cache.rs` Design

### Cache Location

`rulesets_cache_root()` returns `<dirs::cache_dir>/xzardgz/rulesets/`. On macOS
this resolves to `~/Library/Caches/xzardgz/rulesets/`; on Linux to
`~/.cache/xzardgz/rulesets/`. Falls back to `$HOME/.cache` or `./.cache` if
`dirs::cache_dir()` returns `None`.

### Cache Path Schema

Each `(source_id, commit)` pair maps to a directory:

```text
<cache_root>/<source_id>/<commit>/bundle_manifest.yaml
```

A cache entry is considered valid when this file exists (`is_cached`).

### Digest Algorithm

`compute_directory_digest` walks the directory, sorts all file paths by their
relative path for determinism, then feeds each file as:

```text
<relative_path_bytes> NUL <file_content_bytes>
```

into a SHA-256 hasher. The result is a 64-character lowercase hex string.

### Internal Helpers

`write_manifest_to_dir` and `read_manifest_from_dir` take an explicit directory
path rather than deriving it from `source_id + commit`. This allows tests to
write to a `tempdir`-rooted path (`cache_path_for_with_root`) instead of
touching the real OS cache directory.

## `resolve.rs` Extension: `load_cached_ruleset`

`load_cached_ruleset(source_id, commit)` integrates the cache with the SAST
rule-loading pipeline:

1. Calls `is_cached` to guard against missing entries.
2. Reads the `BundleManifest` and checks `allowed_by_policy`.
3. If permitted, calls the existing `load_rules_from_dir` on the cache
   directory.
4. All early-exit paths emit a `tracing::warn!` and return `Ok(Vec::new())`.

## `mod.rs` Changes

`pub mod cache;` and `pub mod fetch;` were added alongside the existing module
declarations. `pub mod error;`, `pub mod license;`, `pub mod manifest;`,
`pub mod source;`, and `pub mod sources;` were already present.

Backward-compatible items (`load_manifest`, `SyncResult`, `sync_source`) are
retained in `mod.rs` and delegate to `sources::load_sources_manifest` and
validate `RulesetSourceEntry` fields.

## Test Coverage

| Module                         | Tests Added                                              |
| ------------------------------ | -------------------------------------------------------- |
| `clients::rulesets::fetch`     | 6 (path source ok/err, yaml count, license file)         |
| `clients::rulesets::cache`     | 4 (is_cached false, write/read roundtrip, digest eq/neq) |
| `scanner::sast::rule::resolve` | 1 (load_cached_ruleset with no cache returns empty)      |

All 2338 unit tests and all doctests pass with zero warnings.
