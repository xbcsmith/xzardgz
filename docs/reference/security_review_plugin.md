# Security Review Plugin Reference

## Overview

The security review plugin performs AI-assisted security analysis against a
scanned repository. It reads a scan artifact produced by the repository scanner,
submits file content and metadata to the configured AI provider, and produces
structured security findings across a set of security scope areas.

The plugin produces findings with severity levels, a RiskBand derived from the
highest finding severity, and reports in Markdown, JSON, and SARIF formats.
SARIF output is unique to this plugin and is suitable for upload to GitHub Code
Scanning.

The plugin identifier is `security-review`.

Secret values are never written to reports, transcripts, or watcher results.
Findings reference the location of a potential secret exposure but not the
secret value itself.

## Configuration Reference

The `security_review` section in `config.yaml` controls plugin behavior.

| Field                | Type            | Default                         | Description                                  |
| -------------------- | --------------- | ------------------------------- | -------------------------------------------- |
| `max_findings`       | integer         | `25`                            | Maximum number of findings returned per run. |
| `severity_threshold` | string (enum)   | `"medium"`                      | Minimum severity level to include in output. |
| `include_sarif`      | bool            | `true`                          | Produce a SARIF 2.1.0 output file.           |
| `report_formats`     | list of strings | `["markdown", "json", "sarif"]` | Output report formats to produce.            |

### `severity_threshold` values

Findings below the threshold are suppressed from output. Valid values, from
lowest to highest:

- `"info"`
- `"low"`
- `"medium"`
- `"high"`
- `"critical"`

### `report_formats` values

- `"markdown"` - produces `security_review.md`
- `"json"` - produces `security_review.json`
- `"sarif"` - produces `security_review.sarif.json`

When `include_sarif` is `false`, the `"sarif"` format is ignored even if listed
in `report_formats`.

## Security Scope Areas

The plugin evaluates the repository across the following scope areas.

| Area               | Description                                                                     |
| ------------------ | ------------------------------------------------------------------------------- |
| `injection`        | SQL injection, command injection, server-side template injection                |
| `authentication`   | Broken authentication, weak credentials, session management weaknesses          |
| `authorization`    | Privilege escalation, access control failures, insecure direct object reference |
| `cryptography`     | Weak algorithms, insecure key management, weak random number generation         |
| `secrets`          | Hardcoded credentials, API keys, tokens embedded in source or config files      |
| `dependencies`     | Known vulnerable packages identified from manifest files                        |
| `configuration`    | Insecure defaults, environment variable leakage, overly permissive settings     |
| `input_validation` | Cross-site scripting, path traversal, buffer overflow patterns                  |

## Finding Structure

Each finding contains the following fields.

| Field         | Type    | Description                                              |
| ------------- | ------- | -------------------------------------------------------- |
| `id`          | string  | Unique finding identifier within the run.                |
| `severity`    | string  | One of: `critical`, `high`, `medium`, `low`, `info`.     |
| `area`        | string  | Security scope area that produced the finding.           |
| `title`       | string  | Short summary of the finding.                            |
| `message`     | string  | Detailed explanation and suggested remediation.          |
| `file_path`   | string  | Repository-relative path to the affected file, if known. |
| `line_number` | integer | Line number within the file, if known.                   |
| `rule_id`     | string  | Stable rule identifier used in SARIF output.             |

### Severity Levels

| Severity   | Meaning                                                                |
| ---------- | ---------------------------------------------------------------------- |
| `critical` | Exploitable vulnerability with significant impact if left unaddressed. |
| `high`     | Serious security weakness requiring prompt remediation.                |
| `medium`   | Moderate risk that should be addressed in normal workflow.             |
| `low`      | Low-impact concern or defense-in-depth recommendation.                 |
| `info`     | Informational observation; no immediate action required.               |

## Secret Handling

The security review plugin identifies potential secret exposures by file path
and line number. It does not copy, store, or include the secret value in any
output.

- Report findings reference the location of a suspected exposure.
- Transcript capture redacts likely secret values when
  `trace_transcript.redact_secrets` is `true` (the default).
- Watcher result messages do not include secret values.
- JSON and SARIF reports contain `file_path` and `line_number` but not the
  exposed value.

## SARIF Output

### Format

The SARIF output conforms to SARIF 2.1.0. The file is written with the
`.sarif.json` extension.

Output path: `<workspace>/reports/security_review.sarif.json`

### Severity Mapping

SARIF `level` values are derived from finding severity as follows.

| Finding Severity | SARIF Level |
| ---------------- | ----------- |
| `critical`       | `error`     |
| `high`           | `error`     |
| `medium`         | `warning`   |
| `low`            | `note`      |
| `info`           | `note`      |

### Rule List

The SARIF `rules` array contains a deduplicated list of rule entries sorted by
`rule_id`. Each rule entry includes:

- `id`: stable rule identifier
- `name`: human-readable rule name
- `shortDescription.text`: brief description of the rule
- `defaultConfiguration.level`: default SARIF level for the rule

### GitHub Code Scanning Integration

Upload the SARIF file to GitHub Code Scanning to surface findings in the
Security tab of the repository.

```yaml
- name: Upload SARIF report
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: .xzardgz/workspaces/<workspace_id>/reports/security_review.sarif.json
```

The SARIF file produced by xzardgz is compatible with the GitHub Code Scanning
SARIF schema. Results appear under the `xzardgz / security-review` tool name in
the Code Scanning alerts view.

## Report Formats

### Markdown Report

Written to `<workspace>/reports/security_review.md`.

The Markdown report includes:

- Run metadata (repository, provider, model, timestamp)
- RiskBand summary
- Findings table grouped by security scope area
- Per-finding detail sections with file path and line number when available

### JSON Report

Written to `<workspace>/reports/security_review.json`.

The JSON report includes:

- `scan_id`
- `workspace_id`
- `risk_band`
- `findings`: array of finding objects
- `plugin_scores`: map of step identifiers to numeric scores
- `generated_at`

Example abbreviated JSON report:

```json
{
  "scan_id": "01HWXYZ1234567890ABCDEFGHI",
  "workspace_id": "01HWXYZ9876543210ABCDEFGHI",
  "risk_band": "High",
  "findings": [
    {
      "id": "SR-001",
      "severity": "high",
      "area": "secrets",
      "rule_id": "SEC-SECRETS-001",
      "title": "Hardcoded API key in source file",
      "message": "A string matching the pattern of an API key was found embedded in source code. Store credentials in environment variables and reference them indirectly.",
      "file_path": "src/client.rs",
      "line_number": 17
    }
  ],
  "plugin_scores": {
    "security_review": 0.41
  },
  "generated_at": "2026-05-29T12:00:00Z"
}
```

### SARIF Report

Written to `<workspace>/reports/security_review.sarif.json`.

Produced when `include_sarif` is `true` and `"sarif"` is listed in
`report_formats`.

## CI Behavior

### Exit Codes

By default, the plugin exits with a non-zero status code when one or more
findings at or above `severity_threshold` are present. This causes CI pipelines
to fail when security issues are detected.

| Condition                                  | Exit Code |
| ------------------------------------------ | --------- |
| No findings at or above threshold          | `0`       |
| One or more findings at or above threshold | non-zero  |

### `fail_on_findings`

Set `fail_on_findings: false` in the step config to suppress non-zero exit codes
even when findings are present. This is useful when running in audit mode.

```yaml
config:
  fail_on_findings: false
```

### `exit_code_on_findings`

Set `exit_code_on_findings` to a specific integer to control the exact exit code
used when findings cause failure.

```yaml
config:
  exit_code_on_findings: 1
```

## Running a Security Review

### Step 1: Create a scan artifact

```bash
xzardgz scan --repository . --output .xzardgz/scan/scan.json
```

### Step 2: Run the plugin

```bash
xzardgz plugin run security-review --scan-artifact .xzardgz/scan/scan.json
```

### Step 3: Run via a workflow plan

```bash
xzardgz plan run --plan sample_plan.yaml
```

### Step 4: Inspect reports

```bash
cat .xzardgz/workspaces/<workspace_id>/reports/security_review.md
cat .xzardgz/workspaces/<workspace_id>/reports/security_review.sarif.json
```

## Integration with Workflow Plans

Include the plugin as a `plugin` step in a workflow plan. The step should depend
on a `scan` step unless an explicit `scan_artifact` path is provided.

```yaml
steps:
  - id: "scan"
    type: "scan"
    description: "Scan repository"

  - id: "security_review"
    type: "plugin"
    description: "Run security review"
    plugin: "security-review"
    depends_on:
      - "scan"
    config:
      max_findings: 25
      severity_threshold: "medium"
      include_sarif: true
      report_formats:
        - "markdown"
        - "json"
        - "sarif"
```

Step-level `config` fields override the global `security_review` section in
`config.yaml` for that step only.

## Configuration Example

Full `security_review` section in `config.yaml`:

```yaml
security_review:
  max_findings: 25
  severity_threshold: "medium"
  include_sarif: true
  report_formats:
    - "markdown"
    - "json"
    - "sarif"
```

To run in audit mode without blocking CI on findings:

```yaml
security_review:
  max_findings: 50
  severity_threshold: "low"
  include_sarif: true
  fail_on_findings: false
  report_formats:
    - "markdown"
    - "json"
    - "sarif"
```
