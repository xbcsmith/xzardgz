# Prompt Customization Reference

## Overview

XZardgz externalizes its AI prompt templates so operators and developers can
tune the instructions sent to the AI provider without modifying compiled code.
Prompt customization controls how each plugin frames its analysis questions,
what context it emphasizes, and what response format it requests.

Every built-in template can be exported, edited, and placed in a local directory
that takes precedence over the built-in copy. The pipeline resolves the correct
template at runtime using a defined search order, so no restart or recompilation
is needed when templates change.

---

## Prompt Resolution Order

When a plugin requests a named template, the runtime searches directories in
this order and uses the first match:

1. CLI-specified directory (`--prompt-dir <PATH>` flag or `prompt_dir` in plugin
   config)
2. Project prompt directory (`.xzardgz/prompts` by default, or the first entry
   in `prompts.directories`)
3. Additional directories listed in `prompts.directories` (in order)
4. Built-in templates compiled into the binary

The search stops at the first directory that contains a file matching the
template name. This means a template at step 1 shadows the same template at
steps 2, 3, and 4.

### Precedence Example

Given this configuration:

```yaml
prompts:
  directories:
    - ".xzardgz/prompts"
    - "/shared/team-prompts"
  allow_overrides: true
```

And the CLI invocation:

```bash
xzardgz run --plugin technical-review --prompt-dir ./custom-prompts
```

For the template `technical-review/summary`, the runtime checks:

1. `./custom-prompts/technical-review/summary.md`
2. `.xzardgz/prompts/technical-review/summary.md`
3. `/shared/team-prompts/technical-review/summary.md`
4. Built-in template

---

## Template Directory Layout

Prompt directories use a two-level layout: the top level contains one
subdirectory per plugin, and each subdirectory contains the templates for that
plugin.

```text
prompts/
  technical-review/
    summary.md
    finding.md
    dimension_prompt.md
  security-review/
    summary.md
    finding.md
    cve_lookup_prompt.md
```

The directory name must exactly match the plugin identifier used in
`plugins.enabled`. Templates not recognized by a plugin are silently ignored.

---

## Template Format

Templates use [Handlebars](https://handlebarsjs.com/) syntax for variable
substitution. A template is a plain text file, typically with a `.md` or `.txt`
extension.

### Variable Substitution

Variables are referenced with double-brace syntax:

```text
{{variable_name}}
```

Nested fields use dot notation:

```text
{{repository.name}}
{{finding.severity}}
```

Iteration over a list:

```text
{{#each findings}}
- {{this.title}}: {{this.description}}
{{/each}}
```

Conditional blocks:

```text
{{#if include_tests}}
Include test files in the analysis scope.
{{/if}}
```

### File Extensions

Both `.md` and `.txt` extensions are recognized. When both exist in the same
directory for the same base name, `.md` takes precedence.

---

## Available Variables by Plugin

### technical-review Variables

Variables available in all `technical-review` templates:

| Variable                    | Type            | Description                                       |
| --------------------------- | --------------- | ------------------------------------------------- |
| `repository.name`           | String          | Repository name extracted from the scan artifact  |
| `repository.url`            | String          | Repository URL when cloned remotely               |
| `dimensions`                | List of strings | Active review dimension category keys             |
| `files`                     | List of objects | Files selected for this analysis batch            |
| `files[].path`              | String          | Repository-relative file path                     |
| `files[].language`          | String          | Detected programming language                     |
| `files[].content`           | String          | File content included for analysis                |
| `max_findings`              | Integer         | Maximum findings to report                        |
| `severity_threshold`        | String          | Minimum severity to include                       |
| `confidence_threshold`      | Float           | Minimum AI confidence to retain a finding         |
| `focus_areas`               | List of strings | Active subset of dimensions, or empty for all     |
| `findings`                  | List of objects | Findings accumulated so far (in verify templates) |
| `findings[].category`       | String          | Dimension category key                            |
| `findings[].severity`       | String          | Severity level string                             |
| `findings[].evidence`       | String          | Observed evidence text                            |
| `findings[].recommendation` | String          | Remediation recommendation                        |

### security-review Variables

Variables available in all `security-review` templates:

| Variable                 | Type            | Description                                       |
| ------------------------ | --------------- | ------------------------------------------------- |
| `repository.name`        | String          | Repository name                                   |
| `repository.url`         | String          | Repository URL                                    |
| `categories`             | List of strings | Active security category names                    |
| `files`                  | List of objects | Files selected for this analysis batch            |
| `files[].path`           | String          | Repository-relative file path                     |
| `files[].language`       | String          | Detected programming language                     |
| `files[].content`        | String          | File content included for analysis                |
| `max_findings`           | Integer         | Maximum findings to report                        |
| `severity_threshold`     | String          | Minimum severity to include                       |
| `confidence_threshold`   | Float           | Minimum AI confidence to retain a finding         |
| `secret_scanning`        | Boolean         | Whether secret scanning is active                 |
| `dependency_scanning`    | Boolean         | Whether dependency scanning is active             |
| `findings`               | List of objects | Findings accumulated so far (in verify templates) |
| `findings[].category`    | String          | Security category name                            |
| `findings[].severity`    | String          | Severity level string                             |
| `findings[].cwe`         | String          | CWE identifier                                    |
| `findings[].evidence`    | String          | Evidence text                                     |
| `findings[].remediation` | String          | Remediation recommendation                        |

---

## Built-in Templates

### technical-review Templates

| Template Name      | File                                   | Purpose                                             |
| ------------------ | -------------------------------------- | --------------------------------------------------- |
| `summary`          | `technical-review/summary.md`          | System prompt instructing the model on review scope |
| `finding`          | `technical-review/finding.md`          | Per-finding output format specification             |
| `dimension_prompt` | `technical-review/dimension_prompt.md` | Prompt for a single review dimension pass           |

### security-review Templates

| Template Name       | File                                   | Purpose                                               |
| ------------------- | -------------------------------------- | ----------------------------------------------------- |
| `summary`           | `security-review/summary.md`           | System prompt instructing the model on security scope |
| `finding`           | `security-review/finding.md`           | Per-finding output format specification               |
| `cve_lookup_prompt` | `security-review/cve_lookup_prompt.md` | Prompt used when querying CVE context via MCP         |

Export all built-in templates with the `prompts export` command:

```bash
xzardgz prompts export --output ./prompts
```

---

## Configuration Reference

### `prompts` section

```yaml
prompts:
  directories:
    - ".xzardgz/prompts"
  allow_overrides: true
```

| Field             | Type          | Default                | Description                                         |
| ----------------- | ------------- | ---------------------- | --------------------------------------------------- |
| `directories`     | List of paths | `[".xzardgz/prompts"]` | Ordered list of directories searched for templates  |
| `allow_overrides` | Boolean       | `true`                 | Permit local templates to shadow built-in templates |

When `allow_overrides` is `false`, only built-in templates are used regardless
of what files exist in the configured directories. Set this to `false` in
environments where prompt integrity must be guaranteed.

### Plugin-level `prompt_dir`

Individual plugins accept a `prompt_dir` field that is checked before the global
`prompts.directories` list:

```yaml
technical_review:
  prompt_dir: "./custom-prompts/technical-review"

security_review:
  prompt_dir: "./custom-prompts/security-review"
```

The `prompt_dir` value may be absolute or relative to the working directory.

---

## Using the `prompts` Command

### Export built-in templates

Write all built-in templates to a local directory for inspection and
customization:

```bash
xzardgz prompts export --output ./prompts
```

This creates the full directory tree including plugin subdirectories. Existing
files are not overwritten unless `--overwrite` is provided.

### Validate a prompt directory

Check that all templates in a directory are syntactically valid Handlebars and
that required variables are present:

```bash
xzardgz prompts validate --dir ./prompts
```

Validation reports missing required variables, unrecognized template names, and
Handlebars syntax errors. The command exits nonzero on any validation failure.

### Show resolution order

Print the directories that will be searched and in what order for the current
configuration:

```bash
xzardgz prompts resolution-order
```

The output lists each directory with a label indicating whether it is a CLI
override, a configured project directory, or the built-in template store.

### List templates for a plugin

Show all templates available for a given plugin, indicating which directory each
resolves from:

```bash
xzardgz prompts list --plugin technical-review
xzardgz prompts list --plugin security-review
```

The output marks each template as `built-in`, `project`, or `cli-override`.

### Render a prompt with test context

Render a named template with a test context file and print the result to stdout:

```bash
xzardgz prompts render \
  --template technical-review/summary \
  --context context.json
```

The `context.json` file provides variable values for the render. This command is
useful for verifying template changes before running a full plugin execution.

Example `context.json`:

```json
{
  "repository": {
    "name": "my-project",
    "url": "https://github.com/example/my-project"
  },
  "dimensions": ["architecture", "maintainability"],
  "max_findings": 10,
  "severity_threshold": "medium",
  "confidence_threshold": 0.7,
  "focus_areas": [],
  "files": [
    {
      "path": "src/main.rs",
      "language": "rust",
      "content": "fn main() {}"
    }
  ]
}
```

---

## How to Override a Built-in Template

This section walks through overriding the `technical-review/summary` template.

**Step 1.** Export the built-in templates:

```bash
xzardgz prompts export --output .xzardgz/prompts
```

**Step 2.** Open the exported template:

```bash
# The exported file is at:
.xzardgz/prompts/technical-review/summary.md
```

**Step 3.** Edit the template. For example, to restrict analysis to a specific
architectural style, add instructions to the system prompt section.

**Step 4.** Validate the edited template:

```bash
xzardgz prompts validate --dir .xzardgz/prompts
```

Fix any reported errors before proceeding.

**Step 5.** Render the template with test context to confirm the output looks
correct:

```bash
xzardgz prompts render \
  --template technical-review/summary \
  --context .xzardgz/test-context.json
```

**Step 6.** Verify that the configuration file has `allow_overrides: true` and
that `.xzardgz/prompts` appears in `prompts.directories`:

```yaml
prompts:
  directories:
    - ".xzardgz/prompts"
  allow_overrides: true
```

**Step 7.** Run the plugin and confirm the override takes effect:

```bash
xzardgz run --plugin technical-review --repository . --dry-run
```

---

## Best Practices

- Keep overrides minimal. Copy only the specific template you need to change and
  leave the others to resolve from built-ins. Fewer overrides reduce maintenance
  burden as built-in templates evolve.
- Always validate with `prompts validate` before running. Handlebars syntax
  errors produce unhelpful AI results and may cause the plugin to abort.
- Always render with `prompts render` before running. Confirm that the rendered
  output is coherent and that all variable references resolve.
- Store custom prompts in version control alongside your configuration. Treat
  prompt changes with the same review discipline as code changes.
- Use `prompts list` after each configuration change to confirm that templates
  resolve from the expected directories.
- Do not modify exported built-in templates in place if you intend to track them
  separately. Copy them to a project directory first.
- When testing a new template variant, use `--prompt-dir` at the CLI level so
  that the change is isolated to a single run and does not affect the project
  prompt directory.
