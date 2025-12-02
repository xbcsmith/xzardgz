# Workflow Format Specification

## YAML Format

```yaml
name: "Repository Analysis"
description: "Analyze repository and generate documentation"
repository: "."

steps:
  - id: "scan"
    description: "Scan repository"
    action:
      type: "ScanRepository"
    dependencies: []

  - id: "generate"
    description: "Generate documentation"
    action:
      type: "GenerateDocumentation"
      category: "explanation"
    dependencies: ["scan"]

deliverables:
  - type: "Documentation"
    path: "docs/explanation/analysis.md"
```

## JSON Format

```json
{
  "name": "Repository Analysis",
  "description": "Analyze repository and generate documentation",
  "repository": ".",
  "steps": [
    {
      "id": "scan",
      "description": "Scan repository",
      "action": {
        "type": "ScanRepository"
      },
      "dependencies": []
    }
  ],
  "deliverables": []
}
```

## Action Types

- `ScanRepository`: Scan repository files
- `AnalyzeCode`: Analyze code structure
- `GenerateDocumentation`: Generate docs with specified category
- `ExecuteCommand`: Execute a shell command
