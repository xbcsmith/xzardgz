# Security Review Demo

This demo walks through running the XZardgz security review plugin against two
different targets: the bundled `fixture-repo/` Python project (offline dry run,
no API key required) and the public
[pallets/jinja](https://github.com/pallets/jinja) repository on GitHub (live
run, requires an OpenAI API key).

Run all commands from the **repository root**.

## What This Demo Shows

- How to run a dry-run security review with no API key (`--dry-run`).
- How to run a full security review against a local repository fixture.
- How to run a full security review against a real public GitHub repository,
  satisfying the Phase 2 requirement for at least one demo targeting realistic
  scale and content.
- How to read the produced Markdown, JSON, and SARIF reports.

## Prerequisites

- `xzardgz` installed and on your `PATH`:

  ```bash
  cargo install --path .
  ```

- For Scenarios B and C only: an OpenAI API key:

  ```bash
  export OPENAI_API_KEY="sk-..."
  ```

## Fixture Repository

The `fixture-repo/` subdirectory is a minimal Python authentication service:

```text
fixture-repo/
  README.md     Project description
  app.py        HTTP handler entry point: login dispatch
  auth.py       Token verification and session helpers
  db.py         In-memory database query helpers
  requirements.txt   Dependency list (empty for this demo)
```

---

## Scenario A: Offline Dry Run (no API key required)

Validate the plugin configuration and scan the fixture repository without making
any AI calls or writing any report files.

```bash
xzardgz run \
  --plugin security-review \
  --repository demo/security-review/fixture-repo \
  --config demo/security-review/config.yaml \
  --dry-run
```

Expected output:

```text
Workspace: <workspace-id>
Correlation ID: <ulid>
Dry run: validation only, no side effects performed.
Success: true
```

The workspace ID and correlation ID change on every run. No files are written to
disk in dry-run mode.

---

## Scenario B: Live Local Run Against the Fixture Repository

Run a full security review against the bundled fixture repository. This makes
real AI provider calls and writes Markdown, JSON, and SARIF reports.

Requires `OPENAI_API_KEY` to be set.

```bash
xzardgz run \
  --plugin security-review \
  --repository demo/security-review/fixture-repo \
  --config demo/security-review/config.yaml
```

Expected output (exact finding count and report paths will vary):

```text
Workspace: <workspace-id>
Correlation ID: <ulid>
Report (security_review): .xzardgz/reports/<step-id>/report.md
Report (security_review): .xzardgz/reports/<step-id>/report.json
Report (security_review): .xzardgz/reports/<step-id>/report.sarif
Success: true
```

### Reading the Reports

Open the Markdown report for a human-readable summary:

```bash
cat .xzardgz/reports/*/report.md
```

Inspect the SARIF report for tool-compatible structured output:

```bash
cat .xzardgz/reports/*/report.sarif
```

The SARIF report can be uploaded to GitHub Advanced Security via the
[upload-sarif](https://github.com/github/codeql-action) action.

---

## Scenario C: Live Run Against a Real Public GitHub Repository

This scenario targets [pallets/jinja](https://github.com/pallets/jinja), the
widely used Python template engine. It demonstrates XZardgz operating at
realistic scale and content — a production-grade open-source project with
hundreds of source files, active history, and known security considerations.

Requires `OPENAI_API_KEY` to be set.

```bash
xzardgz run \
  --plugin security-review \
  --repository https://github.com/pallets/jinja \
  --branch main \
  --config demo/security-review/config.yaml
```

XZardgz will:

1. Clone the repository to a temporary workspace directory.
2. Scan the working tree: file inventory, language detection, dependency
   manifests.
3. Run the security review plugin against the scan artifact.
4. Write Markdown, JSON, and SARIF reports to `.xzardgz/reports/`.

Expected output (finding count and report paths will vary):

```text
Workspace: <workspace-id>
Correlation ID: <ulid>
Report (security_review): .xzardgz/reports/<step-id>/report.md
Report (security_review): .xzardgz/reports/<step-id>/report.json
Report (security_review): .xzardgz/reports/<step-id>/report.sarif
Success: true
```

### Why pallets/jinja

- It is a public, well-maintained Python project with a clear security surface
  (sandboxed template rendering, known CVE history).
- It is large enough to produce meaningful findings but small enough to complete
  within a single model context window.
- It has no external service dependencies, so the demo is fully reproducible
  with just a GitHub clone.

### Adding a Correlation ID for Tracing

When integrating with CI, pass a stable identifier so you can correlate the
report to the triggering run:

```bash
xzardgz run \
  --plugin security-review \
  --repository https://github.com/pallets/jinja \
  --branch main \
  --config demo/security-review/config.yaml \
  --correlation-id "ci-security-jinja-$(date +%Y%m%d)"
```

---

## Reusing a Scan Artifact

After the first run, pass the cached scan artifact directly to skip the scan
phase on subsequent runs:

```bash
xzardgz run \
  --plugin security-review \
  --scan-artifact .xzardgz/scan/security_scan.json \
  --config demo/security-review/config.yaml
```

This is useful when iterating on plugin configuration without re-cloning or
re-scanning the repository.

---

## Further Reading

- [Security Review Plugin Reference](../../docs/reference/security_review_plugin.md)
- [CLI Reference: run](../../docs/reference/cli.md)
- [SARIF Output and GitHub Code Scanning](../../docs/reference/security_review_plugin.md)
- [Configuration Reference](../../docs/reference/configuration.md)
