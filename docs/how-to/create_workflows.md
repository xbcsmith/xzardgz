# How to Create Workflows

## Problem

You want to automate repository scanning, plugin execution, and report writing.

## Solution

Create a `.yaml` workflow plan that defines a repository target, scan settings,
plugin steps, provider overrides, and report formats. Run the plan with
`xzardgz run --plan <PLAN_FILE>`.

## Steps

1. Create a workflow file, such as `technical_review_plan.yaml`.
2. Define the workflow name and repository target.
3. Configure workspace and scan output paths.
4. Add a `scan` step.
5. Add one or more `plugin` steps that depend on the scan step.
6. Configure report output formats.
7. Run the workflow.

## Example

```yaml
version: "1"
name: "technical review"
description: "Scan the repository and run technical review"

repository:
  path: "."
  branch: "main"

workspace:
  root: ".xzardgz/workspaces"
  resume: true

steps:
  - id: "scan"
    type: "scan"
    description: "Create repository scan artifact"

  - id: "technical_review"
    type: "plugin"
    description: "Run the technical review plugin"
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

Run it with:

```bash
xzardgz run --plan technical_review_plan.yaml --config config.example.yaml
```

## Discussion

Workflows support dependency management between steps. A plugin step should run
after a scan step unless it explicitly references an existing scan artifact.
Keep plugin configuration close to the step when it is workflow-specific, and
place shared defaults in `config.yaml`.

Use the [Workflow Format Reference](../reference/workflow_format.md) for the
full plan structure.
