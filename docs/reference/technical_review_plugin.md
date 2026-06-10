# Technical Review Plugin Reference

## Overview

The technical review plugin performs AI-assisted code quality analysis against a
scanned repository. It reads a scan artifact produced by the repository scanner,
submits file content and metadata to the configured AI provider, and produces
structured findings across one or more review dimensions.

The plugin produces findings with severity levels, a RiskBand derived from the
highest finding severity, and reports in Markdown and JSON formats. SARIF output
is not supported for technical review; use the security review plugin for SARIF.

The plugin identifier is `technical-review`.

## Configuration Reference

The `technical_review` section in `config.yaml` controls plugin behavior.

| Field                | Type            | Default                | Description                                  |
| -------------------- | --------------- | ---------------------- | -------------------------------------------- |
| `max_findings`       | integer         | `25`                   | Maximum number of findings returned per run. |
| `severity_threshold` | string (enum)   | `"medium"`             | Minimum severity level to include in output. |
| `focus_areas`        | list of strings | all areas              | Review dimensions to evaluate.               |
| `report_formats`     | list of strings | `["markdown", "json"]` | Output report formats to produce.            |

### `severity_threshold` values

Findings below the threshold are suppressed from output. Valid values, from
lowest to highest:

- `"info"`
- `"low"`
- `"medium"`
- `"high"`
- `"critical"`

### `focus_areas` values

- `"architecture"`
- `"reliability"`
- `"maintainability"`
- `"performance"`
- `"testability"`
- `"security"`

When `focus_areas` is omitted, all dimensions are evaluated.

### `report_formats` values

- `"markdown"` - produces `technical_review.md`
- `"json"` - produces `technical_review.json`

SARIF is not a supported format for technical review.

## Review Dimensions

Each dimension targets a distinct aspect of code quality.

| Dimension         | Coverage                                                               |
| ----------------- | ---------------------------------------------------------------------- |
| `architecture`    | Structural design, module coupling, dependency management, layering    |
| `reliability`     | Error handling, fault tolerance, resource management, panics           |
| `maintainability` | Code clarity, naming conventions, complexity, inline documentation     |
| `performance`     | Algorithmic efficiency, hot-path bottlenecks, resource usage           |
| `testability`     | Test coverage, test structure, mockability, test isolation             |
| `security`        | Basic security checks; full security analysis requires security-review |

## Finding Structure

Each finding contains the following fields.

| Field         | Type    | Description                                              |
| ------------- | ------- | -------------------------------------------------------- |
| `id`          | string  | Unique finding identifier within the run.                |
| `severity`    | string  | One of: `critical`, `high`, `medium`, `low`, `info`.     |
| `dimension`   | string  | Review dimension that produced the finding.              |
| `title`       | string  | Short summary of the finding.                            |
| `message`     | string  | Detailed explanation and suggested remediation.          |
| `file_path`   | string  | Repository-relative path to the affected file, if known. |
| `line_number` | integer | Line number within the file, if known.                   |

### Severity Levels

| Severity   | Meaning                                                             |
| ---------- | ------------------------------------------------------------------- |
| `critical` | Defect likely to cause production failure or data corruption.       |
| `high`     | Significant design or reliability problem requiring prompt action.  |
| `medium`   | Moderate quality issue that should be addressed in normal workflow. |
| `low`      | Minor concern or style deviation with limited impact.               |
| `info`     | Informational observation; no action required.                      |

## Report Formats

### Markdown Report

Written to `<workspace>/reports/technical_review.md` and, when the global
`reports.output_dir` is configured, to `<reports_dir>/technical_review.md`.

The Markdown report includes:

- Run metadata (repository, provider, model, timestamp)
- RiskBand summary
- Findings table grouped by dimension
- Per-finding detail sections with file path and line number when available

### JSON Report

Written to `<workspace>/reports/technical_review.json`.

The JSON report mirrors the internal finding structure and includes:

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
  "risk_band": "Medium",
  "findings": [
    {
      "id": "TR-001",
      "severity": "medium",
      "dimension": "maintainability",
      "title": "Function exceeds recommended complexity threshold",
      "message": "The function `process_batch` has a cyclomatic complexity of 18. Consider splitting it into smaller, focused functions.",
      "file_path": "src/processor.rs",
      "line_number": 42
    }
  ],
  "plugin_scores": {
    "technical_review": 0.72
  },
  "generated_at": "2026-05-29T12:00:00Z"
}
```

## RiskBand

The RiskBand summarizes overall repository health as a single label derived from
the highest severity finding returned by the plugin.

| Highest Finding Severity | RiskBand   |
| ------------------------ | ---------- |
| `critical`               | `Critical` |
| `high`                   | `High`     |
| `medium`                 | `Medium`   |
| `low` or `info`          | `Low`      |
| No findings              | `Low`      |

The RiskBand appears at the top of the Markdown report and in the `risk_band`
field of the JSON report.

## Running a Technical Review

### Step 1: Create a scan artifact

```bash
xzardgz scan --repository . --output .xzardgz/scan/scan.json
```

### Step 2: Run the plugin

```bash
xzardgz plugin run technical-review --scan-artifact .xzardgz/scan/scan.json
```

### Step 3: Run via a workflow plan

```bash
xzardgz plan run --plan sample_plan.yaml
```

### Step 4: Inspect reports

Reports are written to the workspace reports directory and, when configured, to
the global reports output directory.

```bash
cat .xzardgz/workspaces/<workspace_id>/reports/technical_review.md
cat .xzardgz/workspaces/<workspace_id>/reports/technical_review.json
```

## Integration with Workflow Plans

Include the plugin as a `plugin` step in a workflow plan. The step should depend
on a `scan` step unless an explicit `scan_artifact` path is provided.

```yaml
steps:
  - id: "scan"
    type: "scan"
    description: "Scan repository"

  - id: "technical_review"
    type: "plugin"
    description: "Run technical review"
    plugin: "technical-review"
    depends_on:
      - "scan"
    config:
      max_findings: 25
      severity_threshold: "medium"
      focus_areas:
        - "architecture"
        - "reliability"
        - "maintainability"
        - "performance"
        - "testability"
      report_formats:
        - "markdown"
        - "json"
```

Step-level `config` fields override the global `technical_review` section in
`config.yaml` for that step only.

## Configuration Example

Full `technical_review` section in `config.yaml`:

```yaml
technical_review:
  max_findings: 25
  severity_threshold: "medium"
  focus_areas:
    - "architecture"
    - "reliability"
    - "maintainability"
    - "performance"
    - "testability"
  report_formats:
    - "markdown"
    - "json"
```

To restrict review to a single dimension and only return high-severity findings:

```yaml
technical_review:
  max_findings: 10
  severity_threshold: "high"
  focus_areas:
    - "reliability"
  report_formats:
    - "markdown"
```
