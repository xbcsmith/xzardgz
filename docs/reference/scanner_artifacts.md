# Scanner Artifacts Reference

## Overview

The scan artifact is a JSON file produced by the repository scanner. It captures
a point-in-time inventory of all files in a repository that match the scanner
configuration. Plugins consume the scan artifact rather than reading the
repository directly, which decouples file discovery from AI analysis and makes
plugin runs reproducible and resumable.

A scan artifact can be produced as a standalone step or as part of a workflow
plan. Once produced, it can be reused across multiple plugin runs without
re-scanning the repository.

## ScanResult Schema

The top-level object in a scan artifact conforms to the `ScanResult` schema.

| Field              | Type             | Description                                                   |
| ------------------ | ---------------- | ------------------------------------------------------------- |
| `scan_id`          | string (ULID)    | Unique identifier for this scan.                              |
| `repository_path`  | string           | Absolute filesystem path to the repository root at scan time. |
| `scanned_at`       | string           | ISO 8601 timestamp when the scan completed.                   |
| `file_count`       | integer          | Number of files included in the artifact.                     |
| `total_bytes`      | integer          | Combined size in bytes of all included files.                 |
| `files`            | array            | List of `ScannedFile` entries.                                |
| `ignored_patterns` | array of strings | Ignore patterns applied during this scan.                     |
| `scan_config`      | object           | Snapshot of the `ScannerConfig` used for this scan.           |

## ScannedFile Schema

Each entry in the `files` array conforms to the `ScannedFile` schema.

| Field        | Type              | Description                                                   |
| ------------ | ----------------- | ------------------------------------------------------------- |
| `path`       | string            | Repository-relative path to the file.                         |
| `size_bytes` | integer           | File size in bytes.                                           |
| `language`   | string (optional) | Detected programming language, e.g. `Rust`, `Python`, `YAML`. |
| `category`   | string (enum)     | File category. See categories below.                          |
| `sha256`     | string (optional) | Hex-encoded SHA-256 digest of the file contents.              |

### `language` detection

Language detection is heuristic and based on file extension and content
sampling. When the language cannot be determined, the field is absent or `null`.

## File Categories

The `category` field classifies each file into one of the following groups.

| Category        | Description                                                           |
| --------------- | --------------------------------------------------------------------- |
| `source`        | Source code files: `.rs`, `.py`, `.go`, `.ts`, `.js`, etc.            |
| `config`        | Configuration files: `.yaml`, `.toml`, `.json`, `.env`, etc.          |
| `data`          | Data files: `.csv`, `.parquet`, `.sql`, database dumps, etc.          |
| `documentation` | Documentation files: `.md`, `.rst`, `.txt`, etc.                      |
| `test`          | Test files identified by path convention or naming pattern.           |
| `build`         | Build system files: `Makefile`, `Dockerfile`, CI workflow files, etc. |
| `other`         | Files that do not match any of the above categories.                  |

## Ignore Rules

The scanner applies ignore rules in the following order.

### `ignore_patterns`

Path segment matching. A file or directory is excluded when any segment of its
path matches a configured pattern. Patterns are matched case-sensitively.

Common default patterns:

- `target`
- `.git`
- `node_modules`
- `__pycache__`
- `dist`
- `build`
- `vendor`

### `.gitignore` rules

The scanner respects `.gitignore` files in the repository using the `ignore`
crate. Rules in `.gitignore` files at any depth in the repository are honored.

### `include_hidden`

When `include_hidden` is `false` (the default), hidden files and directories
(those with names beginning with `.`) are excluded, unless they are reached
through an explicit path that is not hidden.

### `follow_symlinks`

When `follow_symlinks` is `false` (the default), symbolic links are not
followed. Setting this to `true` can cause infinite loops on repositories with
circular symlinks.

### `max_file_size_bytes`

Files with a size exceeding `max_file_size_bytes` are skipped and not included
in the artifact. The default is 1 MiB (1,048,576 bytes). The skipped file count
is not reflected in `file_count`.

## Scanner Configuration Reference

The `scanner` section in `config.yaml` controls scanner behavior. A snapshot of
the resolved configuration is stored in `scan_config` inside the artifact.

| Field                 | Type            | Default        | Description                                       |
| --------------------- | --------------- | -------------- | ------------------------------------------------- |
| `include_hidden`      | bool            | `false`        | Include hidden files and directories.             |
| `follow_symlinks`     | bool            | `false`        | Follow symbolic links during directory traversal. |
| `max_file_size_bytes` | integer         | `1048576`      | Skip files larger than this byte count.           |
| `ignore_patterns`     | list of strings | (see defaults) | Path segments that exclude a file or directory.   |

```yaml
scanner:
  include_hidden: false
  follow_symlinks: false
  max_file_size_bytes: 1048576
  ignore_patterns:
    - "target"
    - ".git"
    - "node_modules"
    - "__pycache__"
    - "dist"
    - "build"
    - "vendor"
```

## Running a Standalone Scan

Use the `scan` subcommand to produce an artifact without running a plugin.

```bash
xzardgz scan --repository . --output .xzardgz/scan/scan.json
```

Options:

- `--repository <PATH>`: Path to the repository root to scan. Defaults to the
  current directory.
- `--output <ARTIFACT_PATH>`: Path where the scan artifact JSON file is written.
- `--config <CONFIG_PATH>`: Optional path to a `config.yaml` file.

The output directory is created if it does not already exist.

## Reusing Scan Artifacts

Pass `--scan-artifact` to a plugin run to reuse an existing artifact and skip
re-scanning:

```bash
xzardgz plugin run technical-review --scan-artifact .xzardgz/scan/scan.json
xzardgz plugin run security-review --scan-artifact .xzardgz/scan/scan.json
```

Within a workspace, the pipeline automatically reuses the scan artifact when
`scan_artifact_path` is set in `state.json` and the file is present on disk. See
the [Workspace Model Reference](workspace_model.md) for idempotency rules.

## Use Cases

### CI preflight

Run the scanner at the start of a CI job to produce an artifact that multiple
downstream plugin steps consume. This avoids re-scanning for each plugin.

```bash
xzardgz scan --repository . --output .xzardgz/scan/scan.json
xzardgz plugin run technical-review --scan-artifact .xzardgz/scan/scan.json
xzardgz plugin run security-review --scan-artifact .xzardgz/scan/scan.json
```

### Plugin development

When developing or testing a plugin, produce a scan artifact once and re-run the
plugin against it repeatedly without re-scanning.

### Debugging

Inspect the scan artifact to verify that the expected files are included and
that ignore patterns are working correctly.

```bash
cat .xzardgz/scan/scan.json | python3 -m json.tool | head -80
```

## Example Scan Artifact

Abbreviated JSON showing the structure of a scan artifact.

```json
{
  "scan_id": "01HWXYZ1234567890ABCDEFGHI",
  "repository_path": "/home/user/projects/myapp",
  "scanned_at": "2026-05-29T12:00:00Z",
  "file_count": 3,
  "total_bytes": 8192,
  "ignored_patterns": ["target", ".git", "node_modules"],
  "scan_config": {
    "include_hidden": false,
    "follow_symlinks": false,
    "max_file_size_bytes": 1048576,
    "ignore_patterns": ["target", ".git", "node_modules"]
  },
  "files": [
    {
      "path": "src/main.rs",
      "size_bytes": 1024,
      "language": "Rust",
      "category": "source",
      "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    },
    {
      "path": "Cargo.toml",
      "size_bytes": 512,
      "language": null,
      "category": "config",
      "sha256": "a948904f2f0f479b8f936dba6212ef2f16c2b72d61e4a9e57c3dca53e63f8b5c"
    },
    {
      "path": "README.md",
      "size_bytes": 6656,
      "language": null,
      "category": "documentation",
      "sha256": "c4ca4238a0b923820dcc509a6f75849bc81e728d9d4c2f636f067f89cc14862c"
    }
  ]
}
```
