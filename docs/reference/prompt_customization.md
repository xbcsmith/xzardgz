# Prompt Customization Reference

## Overview

XZardgz uses the [Tera](https://keats.github.io/tera/) template engine to render
the prompts sent to AI providers. Every built-in prompt is compiled into the
binary as an embedded default, but operators and developers can override
individual templates by placing edited `.tera` files in a local directory. No
restart or recompilation is required when templates change.

This document covers the resolution model, the quick-start workflow for creating
overrides, the full CLI reference, the configuration schema, Tera template
syntax, and fallback behavior.

---

## Resolution Order

When a plugin requests a template, the runtime evaluates three levels in order
and uses the first match found:

1. **In-memory overrides** -- Registered programmatically at runtime. This level
   is intended for testing and is not user-facing.
2. **File-based overrides** -- Files found in the directories listed in
   `prompts.directories` (default: `.xzardgz/prompts`). This level is active
   only when `prompts.allow_overrides` is `true`.
3. **Compiled-in embedded defaults** -- Templates bundled with the binary at
   build time. This level is always available and cannot be removed.

The search stops at the first level that contains a matching template. Level 3
is always reached if levels 1 and 2 produce no match.

To inspect the resolution order for the current configuration:

```bash
xzardgz prompts show-order
```

---

## Template Directory Layout

File-based overrides follow a two-level directory structure:

```text
{directory}/{plugin}/{key}.tera
```

The default override directory is `.xzardgz/prompts`. The full tree for both
built-in plugins looks like:

```text
.xzardgz/prompts/
  security_review/
    system.tera
  technical_review/
    system.tera
```

### Known Plugins and Template Keys

| Plugin             | Key      | Description                                     |
| ------------------ | -------- | ----------------------------------------------- |
| `security_review`  | `system` | System prompt for the AI security review agent  |
| `technical_review` | `system` | System prompt for the AI technical review agent |

Plugin names use underscores (`security_review`). CLI commands also accept
kebab-case (`security-review`), which is normalized automatically.

---

## Quick-Start Guide

### Step 1: Export Embedded Defaults

Export all compiled-in templates to disk to use as a starting point:

```bash
xzardgz prompts export --output-dir .xzardgz/prompts
```

This writes one `.tera` file per template into the plugin subdirectories.
Existing files are not overwritten.

### Step 2: Edit the Template

Open the file for the template you want to customize. For example, to change the
security review system prompt:

```text
.xzardgz/prompts/security_review/system.tera
```

Edit the Tera content. See [Template Syntax](#template-syntax) for variable and
control-flow syntax.

### Step 3: Verify Configuration

Ensure `config.yaml` has `allow_overrides: true` and the override directory is
listed in `prompts.directories`:

```yaml
prompts:
  directories:
    - .xzardgz/prompts
  allow_overrides: true
```

### Step 4: Validate Override Directories

Check that all configured override directories are reachable:

```bash
xzardgz prompts validate
```

### Step 5: Preview the Rendered Output

Render the template to stdout to confirm substitution before running a full
pipeline:

```bash
xzardgz prompts render security_review system --context '{"repo_name": "myrepo"}'
```

The override is picked up automatically at runtime with no further action
required.

---

## CLI Reference

All prompt subcommands are grouped under `xzardgz prompts`.

### export

Export all embedded default templates to disk for inspection and editing.

```bash
xzardgz prompts export [--output-dir <path>]
```

| Flag           | Default            | Description                                |
| -------------- | ------------------ | ------------------------------------------ |
| `--output-dir` | `.xzardgz/prompts` | Directory to write exported template files |

Existing files are not overwritten. Running export again is safe.

### show-order

Display the three-level resolution order for the current configuration.

```bash
xzardgz prompts show-order
```

The output lists each level and indicates whether file-based overrides are
active.

### list-templates

List all template keys available for a given plugin.

```bash
xzardgz prompts list-templates <plugin>
```

Examples:

```bash
xzardgz prompts list-templates security_review
xzardgz prompts list-templates technical_review
```

### render

Render a template to stdout with an optional JSON context object.

```bash
xzardgz prompts render <plugin> <key> [--context <json>]
```

| Argument    | Description                                                                  |
| ----------- | ---------------------------------------------------------------------------- |
| `<plugin>`  | Plugin name: `security_review`, `technical_review`, or kebab-case equivalent |
| `<key>`     | Template key (`system`)                                                      |
| `--context` | JSON object whose keys become Tera variables                                 |

Examples:

```bash
xzardgz prompts render security_review system
xzardgz prompts render security_review system --context '{"repo_name": "myrepo"}'
xzardgz prompts render technical_review system --context '{}'
```

### validate

Validate all configured override directories for accessibility.

```bash
xzardgz prompts validate
```

Exits nonzero if any configured directory is inaccessible.

---

## Configuration Reference

The `prompts` section in `config.yaml` controls override behavior.

```yaml
prompts:
  directories:
    - .xzardgz/prompts
  allow_overrides: true
```

| Field             | Type            | Default                | Description                                   |
| ----------------- | --------------- | ---------------------- | --------------------------------------------- |
| `directories`     | List of strings | `[".xzardgz/prompts"]` | Directories searched for file-based overrides |
| `allow_overrides` | Boolean         | `true`                 | Enable or disable file-based override lookup  |

When `allow_overrides` is `false`, the runtime skips file-based overrides
entirely and resolves all templates from embedded defaults. Use this in
environments where prompt integrity must be guaranteed.

---

## Template Syntax

Templates use [Tera](https://keats.github.io/tera/) syntax. Tera is a
Jinja2-inspired engine with strong typing and clear error messages.

### Variable Substitution

Reference a variable with double-brace syntax:

```text
{{ variable_name }}
```

The `--context` flag accepts a JSON object. Each key in the JSON object becomes
a Tera variable in the template:

```bash
xzardgz prompts render security_review system --context '{"repo_name": "myrepo"}'
```

Template:

```text
Reviewing repository: {{ repo_name }}
```

### Conditional Blocks

```text
{% if include_tests %}
Include test files in the analysis scope.
{% endif %}
```

### Iteration

```text
{% for item in items %}
- {{ item }}
{% endfor %}
```

### Accessing Object Fields

Use dot notation to access fields on objects:

```text
{{ repository.name }}
{{ finding.severity }}
```

---

## Fallback Behavior

The runtime degrades gracefully. No missing file or configuration problem causes
a crash.

| Condition                                  | Behavior                                                 |
| ------------------------------------------ | -------------------------------------------------------- |
| Override file is missing                   | Embedded default used; no error                          |
| Override file contains invalid Tera syntax | Warning logged; embedded default used; no crash          |
| Override directory does not exist          | Embedded default used; no error                          |
| `allow_overrides` is `false`               | Embedded default used; override directories not searched |

The embedded defaults are always available as a last resort. A broken override
does not prevent the pipeline from running.
