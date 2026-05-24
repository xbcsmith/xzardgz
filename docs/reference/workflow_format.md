# Workflow Format Specification

## Overview

Workflow plans describe repository targets, workspace behavior, scan options,
plugin steps, provider overrides, and report output. Plans are plugin-first:
repository scanning produces an artifact, and plugins consume that artifact to
produce findings and reports.

Plans use `.yaml` files by convention.

## YAML Format

```yaml
version: "1"
name: "sample technical review"
description: "Scan a repository and run the technical review plugin"

repository:
  path: "."
  branch: "main"

workspace:
  root: ".xzardgz/workspaces"
  resume: true

provider:
  name: "openai"
  model: "gpt-4.1-mini"

scan:
  output: ".xzardgz/scan/sample_scan.json"
  ignore_patterns:
    - "target"
    - ".git"

steps:
  - id: "scan"
    type: "scan"
    description: "Create a scan artifact"

  - id: "technical_review"
    type: "plugin"
    description: "Run technical review"
    plugin: "technical-review"
    depends_on:
      - "scan"
    config:
      max_findings: 25
      severity_threshold: "medium"

reports:
  output_dir: ".xzardgz/reports"
  formats:
    - "markdown"
    - "json"
```

## JSON Format

```json
{
  "version": "1",
  "name": "sample security review",
  "description": "Scan a repository and run the security review plugin",
  "repository": {
    "path": ".",
    "branch": "main"
  },
  "workspace": {
    "root": ".xzardgz/workspaces",
    "resume": true
  },
  "provider": {
    "name": "openai",
    "model": "gpt-4.1-mini"
  },
  "scan": {
    "output": ".xzardgz/scan/sample_scan.json"
  },
  "steps": [
    {
      "id": "scan",
      "type": "scan",
      "description": "Create a scan artifact"
    },
    {
      "id": "security_review",
      "type": "plugin",
      "description": "Run security review",
      "plugin": "security-review",
      "depends_on": ["scan"],
      "config": {
        "include_sarif": true,
        "max_findings": 25
      }
    }
  ],
  "reports": {
    "output_dir": ".xzardgz/reports",
    "formats": ["markdown", "json", "sarif"]
  }
}
```

## Top-Level Fields

- `version`: Plan schema version.
- `name`: Human-readable workflow name.
- `description`: Short workflow summary.
- `repository`: Repository path, URL, and branch information.
- `workspace`: Workspace location and resume behavior.
- `provider`: Optional provider and model overrides.
- `scan`: Scanner options and scan artifact output path.
- `steps`: Ordered workflow steps with optional dependencies.
- `reports`: Report output settings.

## Repository Fields

- `path`: Local repository path.
- `url`: Repository clone URL. Use either `path` or `url`.
- `branch`: Target branch.
- `commit`: Optional commit SHA for pinned execution.

## Step Types

### `scan`

Creates or refreshes a scan artifact for the repository.

Common fields:

- `id`
- `type: "scan"`
- `description`
- `config`

### `plugin`

Runs a plugin against the current workspace or configured scan artifact.

Common fields:

- `id`
- `type: "plugin"`
- `plugin`: Plugin identifier.
- `depends_on`: Step IDs that must complete first.
- `config`: Plugin-specific configuration.

Built-in plugin identifiers:

- `technical-review`
- `security-review`

## Dependency Rules

- Step IDs must be unique.
- Dependencies must reference existing step IDs.
- Cyclic dependencies are invalid.
- Plugin steps usually depend on a scan step unless they provide an explicit
  `scan_artifact` path.

## Report Formats

Supported report formats are plugin-specific. Common formats include:

- `markdown`
- `json`
- `sarif` for security review

## Validation Rules

- Plans must not use legacy action names.
- Plugin identifiers must be known or configured.
- Plugin configuration must match the plugin schema.
- Repository targets must resolve to one repository.
- Report formats must be supported by the selected plugin.
