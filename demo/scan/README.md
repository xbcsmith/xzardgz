# Scan Demo: Repository Scanner

This demo walks through scanning a local repository with `xzardgz scan`. The
scan command requires no AI provider and no network access -- it runs entirely
offline against the bundled `fixture-repo/` Python project.

## What This Demo Shows

- How to run `xzardgz scan` against a local directory.
- What a scan artifact contains (file inventory, language detection, dependency
  manifests).
- How `--dry-run` validates configuration without writing any files.

## Prerequisites

- `xzardgz` installed and on your `PATH`:

  ```bash
  cargo install --path .
  ```

No API key, no network access, and no other dependencies are required.

Run all commands from the **repository root**.

## Fixture Repository

The `fixture-repo/` subdirectory is a minimal Python data-processing library:

```text
fixture-repo/
  README.md          Project description
  pipeline.py        Main pipeline runner (reads JSON, filters, writes JSON)
  models.py          Typed data model classes: Record, TransformResult
  requirements.txt   Dependency list (empty for this demo)
```

## Step 1: Validate Configuration (dry run)

Check that the scanner configuration is valid without writing any output:

```bash
xzardgz scan \
  --repository demo/scan/fixture-repo \
  --output /tmp/xzardgz-demo-scan.json \
  --config demo/scan/config.yaml \
  --dry-run
```

Expected output:

```text
Workspace: <workspace-id>
Correlation ID: <ulid>
Dry run: validation only, no side effects performed.
Success: true
```

## Step 2: Scan the Fixture Repository

Run the scanner and produce a scan artifact:

```bash
xzardgz scan \
  --repository demo/scan/fixture-repo \
  --output /tmp/xzardgz-demo-scan.json \
  --config demo/scan/config.yaml
```

Expected output:

```text
Workspace: <workspace-id>
Correlation ID: <ulid>
Scan artifact: /tmp/xzardgz-demo-scan.json
Success: true
```

The workspace ID and correlation ID are unique to each run. The correlation ID
is a ULID generated automatically unless you supply `--correlation-id`.

## Step 3: Inspect the Scan Artifact

The scan artifact is a JSON file containing a structured inventory of the
repository:

```bash
cat /tmp/xzardgz-demo-scan.json
```

The artifact includes:

- **File inventory**: every file discovered, grouped by language and directory.
- **Language summary**: detected programming languages and file counts.
- **Dependency manifests**: paths to files like `requirements.txt`,
  `package.json`, or `Cargo.toml` that list external dependencies.
- **Plugin preselection**: signals used to decide which review plugin is most
  relevant for this repository.

## Step 4: Scan With a Custom Correlation ID

Pass `--correlation-id` to supply your own tracing identifier, for example when
integrating scan results into a CI pipeline:

```bash
xzardgz scan \
  --repository demo/scan/fixture-repo \
  --output /tmp/xzardgz-demo-scan.json \
  --config demo/scan/config.yaml \
  --correlation-id "ci-run-20240115-001"
```

Expected output:

```text
Workspace: <workspace-id>
Correlation ID: ci-run-20240115-001
Scan artifact: /tmp/xzardgz-demo-scan.json
Success: true
```

## What to Try Next

- Run the scan against your own project by replacing `demo/scan/fixture-repo`
  with the path to your repository.
- Pass the scan artifact to a plugin with
  `xzardgz run --plugin security-review --scan-artifact /tmp/xzardgz-demo-scan.json`
  to skip the scan step on the next run.
- See the `demo/security-review/` demo for a full scan-plus-plugin walkthrough.

## Further Reading

- [CLI Reference: scan](../../docs/reference/cli.md)
- [Scanner Artifacts Reference](../../docs/reference/scanner_artifacts.md)
- [Configuration Reference](../../docs/reference/configuration.md)
