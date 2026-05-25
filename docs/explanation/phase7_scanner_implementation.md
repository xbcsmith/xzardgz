# Phase 7: Repository Scanner and Scanner Common Infrastructure Implementation

## Overview

Phase 7 introduces `src/scanner/`, a self-contained, AI-free repository scanner
module. The scanner walks a directory tree, respects `.gitignore` rules and
configured exclusion patterns, detects file languages, identifies binary files,
runs cross-cutting hooks for pre-AI findings, categorises every file by semantic
role, and produces a deterministic, versioned `ScanResult` artifact that
downstream plugins can consume without re-scanning the repository.

## Module Layout

```text
src/scanner/
  mod.rs        Scanner struct, scan() method, categorisation helpers, re-exports
  config.rs     ScannerConfig — traversal settings and constants
  language.rs   detect_language(), is_binary_content()
  findings.rs   FindingSeverity enum, ScanFinding struct
  result.rs     FileEntry, LanguageStats, PluginPreselection, ScanResult
  patterns.rs   PatternSet, PatternRegistry with built-in sets
  scoring.rs    ScoringSignal, ScoringInput, ConfidenceScorer
  hooks.rs      CrossCutHook trait, SecretsHook, UnsafeRustHook, CommandExecutionHook
  preselect.rs  PluginContentScanner
```

## Components

### `config.rs` — ScannerConfig

Controls all aspects of the file traversal:

| Field                 | Default | Purpose                                          |
| --------------------- | ------- | ------------------------------------------------ |
| `exclude_patterns`    | `[]`    | Glob patterns for paths to skip                  |
| `max_file_size_bytes` | 1 MiB   | Files larger than this are omitted               |
| `include_hidden`      | `false` | Whether to traverse hidden files and directories |
| `respect_gitignore`   | `true`  | Whether to honour `.gitignore` rules             |
| `max_concurrency`     | `4`     | Concurrency limit (reserved for future use)      |

Builder methods (`with_exclude_patterns`, `with_max_file_size`,
`with_include_hidden`, `with_respect_gitignore`, `with_max_concurrency`)
implement a fluent builder pattern.

### `language.rs` — Language and Binary Detection

`detect_language(path)` maps file extensions to human-readable language names.
Supported languages: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++,
Ruby, PHP, Swift, Kotlin, C#, Shell, YAML, JSON, TOML, Markdown, HTML, CSS, SQL,
Zig, Elixir, XML, Text, Dockerfile.

`is_binary_content(bytes)` scans the first 8 192 bytes for null bytes, the
standard heuristic for binary detection used by `git diff` and similar tools.

### `findings.rs` — Pre-AI Scan Findings

`FindingSeverity` enum: `Info < Low < Medium < High < Critical` (derived `Ord`).

`ScanFinding` carries: `kind` (rule name), `file` (repo-relative path), `line`
(1-based, optional), `evidence` (matched text), `severity`.

### `result.rs` — Versioned Scan Artifact

`ScanResult` is the top-level output of a scan. All fields are serialisable via
`serde_yaml`. Key fields from Phase 7 requirements:

| Field                     | Description                                      |
| ------------------------- | ------------------------------------------------ |
| `version`                 | Schema version (`"1"`)                           |
| `repository_url`          | From git metadata                                |
| `repository_name`         | Last path component of the root directory        |
| `head_commit`             | From git metadata                                |
| `scan_timestamp`          | UTC timestamp                                    |
| `repository_structure`    | All scanned `FileEntry` values, sorted           |
| `language_statistics`     | Per-language `LanguageStats`                     |
| `primary_language`        | Language with the highest file count             |
| `frameworks`              | Detected toolchains (Rust, Docker, Node.js, ...) |
| `documentation_inventory` | README, docs/, \*.md files                       |
| `governance_rules`        | CODEOWNERS, LICENSE, SECURITY.md, .github/       |
| `cli_commands`            | cli.rs, commands/, cmd/ files                    |
| `public_apis`             | Non-test .rs, .go, index.js/ts files             |
| `entrypoints`             | main.rs, lib.rs, index.js, main.py, etc.         |
| `config_surface`          | config.yaml, settings._,_.env, \*rc files        |
| `key_files`               | README, LICENSE, CONTRIBUTING, CHANGELOG         |
| `dependency_manifests`    | Cargo.toml, package.json, go.mod, etc.           |
| `test_files`              | **test.rs, test**.py, tests/, spec/ etc.         |
| `build_files`             | Makefile, build.rs, Dockerfile, CI configs       |
| `security_relevant_files` | .env, files named secret/credential/password     |
| `findings`                | Pre-AI findings from hooks                       |
| `plugin_preselection`     | `PluginPreselection` summary for plugins         |

`PluginPreselection` groups files by concern for plugin consumption:
`entrypoints`, `public_apis`, `config_surfaces`, `dependency_manifests`,
`risky_pattern_files`, `secrets_like_files`, `unsafe_rust_files`,
`command_execution_files`, `network_client_files`, `auth_files`, `test_files`,
`missing_test_signals`.

### `patterns.rs` — Pattern Sets and Registry

`PatternSet` groups `keywords` (content substrings), `dependencies` (package
names), and `file_names` (path patterns) for a single concern.

`PatternRegistry::default_registry()` provides six built-in sets: `"secrets"`,
`"unsafe_rust"`, `"command_execution"`, `"network_clients"`, `"auth"`,
`"risky"`.

### `scoring.rs` — Confidence Scoring

`ConfidenceScorer::score(input)` computes a weighted average of `ScoringSignal`
values, clamped to `[0.0, 1.0]`. Used by downstream analysis layers.

### `hooks.rs` — Cross-Cutting Hooks

`CrossCutHook: Send + Sync` trait with `name()` and `scan(path, content)`.

Built-in hooks:

| Hook                   | Severity | Patterns                                      |
| ---------------------- | -------- | --------------------------------------------- |
| `SecretsHook`          | High     | password=, api_key=, -----BEGIN, SECRET_KEY   |
| `UnsafeRustHook`       | Medium   | unsafe {, unsafe fn, unsafe impl (`.rs` only) |
| `CommandExecutionHook` | Medium   | std::process::Command, os.system(, popen(     |

`default_hooks()` returns all three.

### `preselect.rs` — Plugin Content Scanner

`PluginContentScanner` wraps a `PatternSet` and scans file content for plugin-
specific signals. Each match produces a `ScanFinding` with kind
`"pattern_match:{set_name}"` and severity `Low`. Results are sorted by line
number for deterministic output.

### `mod.rs` — Scanner

`Scanner::new(config)` creates a scanner with the default registry and no hooks.
`Scanner::with_hook(hook)` adds hooks. `Scanner::scan(root, git_metadata)`
performs the full scan synchronously:

1. Walk the directory tree (respecting `.gitignore` and exclusion patterns).
2. Sort all paths alphabetically for deterministic ordering.
3. For each file: check size limit, read bytes, detect binary, detect language.
4. For non-binary files: run all hooks and apply content-flag detection.
5. Categorise each file by path name into semantic lists.
6. Compute language statistics and detect frameworks.
7. Compute missing-test signals (source files without apparent test coverage).
8. Assemble and return a versioned `ScanResult`.

File categorisation uses path-name heuristics only (no content needed). Content
analysis for preselection uses simple substring matching against seven pattern
arrays: `SECRETS_CONTENT_PATTERNS`, `CMD_EXEC_PATTERNS`, `NETWORK_PATTERNS`,
`AUTH_PATTERNS`, `RISKY_PATTERNS`, plus `unsafe` keyword detection for `.rs`.

Framework detection checks for the presence of manifest files: `Cargo.toml`
(Rust), `package.json` (Node.js), `requirements.txt`/`pyproject.toml` (Python),
`go.mod` (Go), `pom.xml` (Maven), `build.gradle` (Gradle), `Dockerfile`
(Docker), `.github/workflows/` (GitHub Actions), `manage.py` (Django),
`CMakeLists.txt` (CMake).

## Integration

- `pub mod scanner;` added to `src/lib.rs`.
- `src/repository/scanner.rs` continues to exist for backward compatibility but
  `src/scanner/` is the canonical scanner implementation.
- `ScanResult::to_yaml()` and `ScanResult::load_from_str()` enable persisting
  the scan artifact to disk (the workspace's `scan_artifact_path`).

## Testing

Phase 7 adds tests across the scanner module:

| File                   | New tests |
| ---------------------- | --------- |
| `scanner/config.rs`    | 8         |
| `scanner/language.rs`  | 13        |
| `scanner/findings.rs`  | 5         |
| `scanner/result.rs`    | 7         |
| `scanner/patterns.rs`  | 8         |
| `scanner/scoring.rs`   | 9         |
| `scanner/hooks.rs`     | 10        |
| `scanner/preselect.rs` | 8         |
| `scanner/mod.rs`       | 20        |

Total: 88 new tests. All 7.5 testing requirements are covered.

## Success Criteria

- Scanner output is deterministic: sorted alphabetically, reproducible across
  runs on the same repository.
- Versioned scan artifact: `ScanResult::version = "1"`, round-trips through YAML
  serialisation.
- Scanner has no AI provider dependency.
- Plugins can consume `ScanResult` without re-scanning when appropriate.
- All four quality gates pass: `cargo fmt`, `cargo check`,
  `cargo clippy -D warnings`, `cargo test`.
