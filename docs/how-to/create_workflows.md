# How to Create Workflows

## Problem

You want to automate documentation generation and repository analysis tasks.

## Solution

Create workflow files in YAML or JSON format that define a sequence of actions.

## Steps

1. Create a workflow file (e.g., `my_workflow.yaml`).
2. Define the workflow name and description.
3. Add steps with actions and dependencies.
4. Run the workflow with `xzardgz run --plan my_workflow.yaml`.

## Example

```yaml
name: "Basic Documentation"
description: "Generate tutorial documentation"

steps:
  - id: "gen_tutorial"
    description: "Generate getting started tutorial"
    action:
      type: "GenerateDocumentation"
      category: "tutorial"
      topic: "Getting Started"
    dependencies: []
```

## Discussion

Workflows support dependency management between steps. Each step runs only after its dependencies complete successfully.
