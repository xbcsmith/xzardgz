# Governance Reference

## Overview

The governance system enforces repository policy checks before plugin execution
begins. It validates that the repository conforms to structural and content
requirements defined by the project rules file (typically `AGENTS.md`).

Governance runs automatically at the start of every pipeline run when
`governance.enabled` is `true`. If validation fails and `fail_on_violation` is
`true`, the pipeline aborts before any scan or plugin step executes.

The governance system is independent of AI providers. It performs static checks
against repository structure and file contents.

## GovernanceConfig Reference

The `governance` section in `config.yaml` controls governance behavior.

| Field               | Type   | Default       | Description                                                         |
| ------------------- | ------ | ------------- | ------------------------------------------------------------------- |
| `enabled`           | bool   | `true`        | Enable or disable governance checks.                                |
| `rules_path`        | string | `"AGENTS.md"` | Path to the file that defines project governance rules.             |
| `fail_on_violation` | bool   | `true`        | Abort the pipeline when a violation at or above threshold is found. |

```yaml
governance:
  enabled: true
  rules_path: "AGENTS.md"
  fail_on_violation: true
```

To disable governance checks entirely:

```yaml
governance:
  enabled: false
```

## Validation Functions

The governance system applies the following checks.

### `check_required_files`

Verifies that a set of required files exist in the repository root. The default
required files are:

- `README.md`
- `AGENTS.md`

A violation is raised for each required file that is absent.

### `check_naming_conventions`

Verifies that documentation and configuration files follow naming conventions
defined in the rules file.

Conventions enforced by default:

- Markdown files use `lowercase_with_underscores.md` naming (`README.md` is the
  only permitted exception to the lowercase rule).
- YAML files use the `.yaml` extension, not `.yml`.
- No CamelCase, kebab-case, or uppercase characters in documentation file names.

A violation is raised for each file that does not conform.

### `check_forbidden_patterns`

Scans file contents for patterns that are disallowed by the project rules.
Patterns are defined in the rules file. Examples include:

- Emoji characters in source code, comments, or documentation
- Hardcoded literal credentials matching known patterns
- Use of deprecated configuration keys

A violation is raised for each occurrence of a forbidden pattern, including the
file path and the matched pattern.

### `check_license`

Verifies that a `LICENSE` file exists in the repository root. A violation is
raised if the file is absent.

## Governance Results and Violations

After running all validation functions, governance produces a `GovernanceResult`
containing a list of `GovernanceViolation` entries.

### `GovernanceViolation` structure

| Field       | Type              | Description                                                      |
| ----------- | ----------------- | ---------------------------------------------------------------- |
| `rule_id`   | string            | Stable identifier for the rule that was violated.                |
| `severity`  | string (enum)     | Violation severity: `critical`, `high`, `medium`, `low`, `info`. |
| `message`   | string            | Human-readable description of the violation.                     |
| `file_path` | string (optional) | Repository-relative path to the file involved, when applicable.  |

Example violations:

```json
[
  {
    "rule_id": "GOV-REQUIRED-FILE-001",
    "severity": "critical",
    "message": "Required file 'AGENTS.md' is missing from the repository root.",
    "file_path": null
  },
  {
    "rule_id": "GOV-NAMING-001",
    "severity": "high",
    "message": "Markdown file uses non-conforming name. Expected lowercase_with_underscores.md.",
    "file_path": "docs/MyGuide.md"
  },
  {
    "rule_id": "GOV-FORBIDDEN-001",
    "severity": "medium",
    "message": "Forbidden pattern detected: emoji character in documentation file.",
    "file_path": "docs/explanation/overview.md"
  }
]
```

## Fail Behavior

### `fail_on_violation`

When `fail_on_violation` is `true`, the pipeline aborts if any violation at or
above the `fail_threshold` is present in the governance result. No scan or
plugin steps are executed.

When `fail_on_violation` is `false`, violations are reported as diagnostics and
the pipeline continues.

### `fail_threshold`

Violations below the `fail_threshold` severity are reported but do not trigger
an abort even when `fail_on_violation` is `true`.

| `fail_threshold` | Violations that trigger abort               |
| ---------------- | ------------------------------------------- |
| `critical`       | Only `critical` violations                  |
| `high`           | `critical` and `high` violations            |
| `medium`         | `critical`, `high`, and `medium` violations |
| `low`            | All violations except `info`                |
| `info`           | All violations including `info`             |

The default `fail_threshold` is `medium`.

## Configuration Example

Standard governance configuration:

```yaml
governance:
  enabled: true
  rules_path: "AGENTS.md"
  fail_on_violation: true
```

Governance in audit mode (report violations without blocking the pipeline):

```yaml
governance:
  enabled: true
  rules_path: "AGENTS.md"
  fail_on_violation: false
```

Governance with a custom rules file:

```yaml
governance:
  enabled: true
  rules_path: "docs/POLICY.md"
  fail_on_violation: true
```

## Common Rules

The following governance rules are applied by default when `AGENTS.md` is
present.

| Rule ID                 | Check                      | Default Severity | Description                                              |
| ----------------------- | -------------------------- | ---------------- | -------------------------------------------------------- |
| `GOV-REQUIRED-FILE-001` | `check_required_files`     | `critical`       | `README.md` must exist in the repository root.           |
| `GOV-REQUIRED-FILE-002` | `check_required_files`     | `critical`       | `AGENTS.md` must exist in the repository root.           |
| `GOV-LICENSE-001`       | `check_license`            | `high`           | `LICENSE` file must exist in the repository root.        |
| `GOV-NAMING-001`        | `check_naming_conventions` | `high`           | Markdown files must use `lowercase_with_underscores.md`. |
| `GOV-NAMING-002`        | `check_naming_conventions` | `high`           | YAML files must use the `.yaml` extension, not `.yml`.   |
| `GOV-FORBIDDEN-001`     | `check_forbidden_patterns` | `medium`         | Emoji characters are not permitted in documentation.     |
| `GOV-FORBIDDEN-002`     | `check_forbidden_patterns` | `high`           | Hardcoded credential patterns are not permitted.         |

## Integration with Plugins

Governance runs before any scan or plugin step. The execution order within a
pipeline run is:

1. Governance checks (`check_required_files`, `check_naming_conventions`,
   `check_forbidden_patterns`, `check_license`)
2. If governance passes (or `fail_on_violation` is `false`): scan step
3. Plugin steps that depend on the scan

Governance violations are included in the `diagnostics` array of
`WorkspaceState` regardless of whether they cause a pipeline abort.

When governance causes an abort, the workspace `final_status` is set to `failed`
and no scan artifact is produced.

## Troubleshooting

### Pipeline aborts immediately with governance violations

Check the diagnostics in `state.json`:

```bash
cat .xzardgz/workspaces/<workspace_id>/state.json | python3 -m json.tool | grep -A 20 diagnostics
```

Each diagnostic includes `rule_id`, `severity`, `message`, and `file_path`.
Address the violations in the listed files, then re-run the pipeline.

### Disabling a specific check temporarily

Set `fail_on_violation: false` to run the pipeline while governance violations
exist. This is useful during onboarding when a repository does not yet conform
to all rules.

```yaml
governance:
  enabled: true
  rules_path: "AGENTS.md"
  fail_on_violation: false
```

### Missing required files

Create the required files in the repository root:

```bash
touch AGENTS.md
touch README.md
touch LICENSE
```

Populate each file with the appropriate content before committing.

### File naming violations

Rename non-conforming files to use `lowercase_with_underscores.md`:

```bash
git mv docs/MyGuide.md docs/my_guide.md
```

### YAML extension violations

Rename `.yml` files to `.yaml`:

```bash
git mv config.yml config.yaml
git mv .github/workflows/ci.yml .github/workflows/ci.yaml
```

Update any references to the renamed files before committing.

### Forbidden pattern violations

Open the file listed in the violation and remove or replace the forbidden
content. For emoji violations, replace emoji characters with plain text
descriptions. For credential violations, replace literal values with environment
variable references.
