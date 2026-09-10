# Plan Examples

Workflow plan files describe a repository target, scan settings, plugin steps,
provider settings, and report output. Run any plan with:

```bash
xzardgz run --plan <PLAN_FILE> --config config.yaml
```

## Plans in This Directory

### `scan_only.yaml`

Scans the repository and writes a scan artifact without running any plugins. Use
this to inspect what the scanner discovers before committing to a full review,
or to pre-cache a scan artifact for later use.

```bash
xzardgz run --plan examples/plans/scan_only.yaml --config config.yaml
```

Output: `.xzardgz/scan/scan.json`

### `analyze_repo.yaml`

Scans the repository and runs the technical review plugin against the scan
artifact. Focuses on architecture, reliability, and maintainability. Returns up
to 25 findings at medium severity or above.

```bash
xzardgz run --plan examples/plans/analyze_repo.yaml --config config.yaml
```

Output: `.xzardgz/reports/` (Markdown and JSON)

### `technical_review_local.yaml`

A more thorough technical review that covers all five focus areas: architecture,
reliability, maintainability, performance, and testability. Uses a lower
severity threshold (`low`) to surface a broader set of observations.

```bash
xzardgz run --plan examples/plans/technical_review_local.yaml --config config.yaml
```

Output: `.xzardgz/reports/` (Markdown and JSON)

### `simple_security_review.yaml`

Scans the repository and runs the security review plugin. Produces Markdown,
JSON, and SARIF output. Returns up to 25 findings at medium severity or above.

```bash
xzardgz run --plan examples/plans/simple_security_review.yaml --config config.yaml
```

Output: `.xzardgz/reports/` (Markdown, JSON, and SARIF)

### `security_review_local.yaml`

A thorough security review with a low severity threshold (`low`) that surfaces
the broadest possible set of findings, including informational items. Returns up
to 50 findings. Produces Markdown, JSON, and SARIF output.

Suitable for an initial security assessment or for uploading a comprehensive
SARIF report to GitHub Code Scanning.

```bash
xzardgz run --plan examples/plans/security_review_local.yaml --config config.yaml
```

Output: `.xzardgz/reports/` (Markdown, JSON, and SARIF)

### `create_pr.yaml`

A configuration snippet showing how to add GitHub pull request creation to your
`config.yaml`. Copy the `pr:` block into your configuration file and set the
`XZARDGZ_GITHUB_TOKEN` environment variable before running.

Requirements:

- A GitHub Personal Access Token with the `repo` scope, set as
  `XZARDGZ_GITHUB_TOKEN`.
- A pre-existing local branch (`head_branch`) to use as the PR source.
- A target branch (`base_branch`) that differs from `head_branch`.

## Customising a Plan

Common adjustments:

- Change `repository.path` to point at a different local checkout.
- Change `repository.url` to clone a remote repository before scanning.
- Change `provider.name` and `provider.model` to use a different AI provider.
- Adjust `config.max_findings` and `config.severity_threshold` in the plugin
  step.
- Add or remove entries in `scan.ignore_patterns` to control what the scanner
  includes.

## Reusing a Scan Artifact

If you already have a scan artifact, pass it directly instead of running a new
scan:

```bash
xzardgz plugin run technical-review \
  --scan-artifact .xzardgz/scan/scan.json \
  --config config.yaml
```

## Dry Run

Validate a plan without making any provider calls or writing reports:

```bash
xzardgz run --plan examples/plans/analyze_repo.yaml --config config.yaml --dry-run
```

## Further Reading

- [Workflow Format Reference](../../docs/reference/workflow_format.md)
- [Technical Review Plugin Reference](../../docs/reference/technical_review_plugin.md)
- [Security Review Plugin Reference](../../docs/reference/security_review_plugin.md)
- [Create Workflows How-To](../../docs/how-to/create_workflows.md)
