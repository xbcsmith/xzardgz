# Prompt Template Overrides

This directory contains example overrides for the built-in prompt templates used
by XZardgz plugins. Copying and customising these files lets you tailor the
language model instructions without modifying the binary.

## Directory Structure

Templates are organised by plugin name:

```text
examples/prompts/
  technical_review/
    summary.md        # Override for the technical-review summary prompt
  security_review/
    summary.md        # Override for the security-review summary prompt
```

When you provide a prompts directory in your configuration, XZardgz resolves
templates by walking each directory in order and using the first match. Built-in
templates serve as the fallback when no override is found.

## Variable Substitution

Templates use [Handlebars](https://handlebarsjs.com/) syntax for variable
substitution. Available variables depend on the plugin and template slot; each
template file documents the variables it expects in a header comment.

Common variables:

| Variable              | Description                                                   |
| --------------------- | ------------------------------------------------------------- |
| `{{repository_name}}` | Short name of the repository under review                     |
| `{{finding_count}}`   | Total number of findings produced                             |
| `{{risk_band}}`       | Aggregated risk level (low, medium, high, critical)           |
| `{{sarif_path}}`      | Absolute path to the SARIF output file (security-review only) |

## Configuration

Reference this directory (or your own copy) in `config.yaml`:

```yaml
prompts:
  directories:
    - "examples/prompts"
```

Multiple directories are supported; they are searched in the order listed.

## Template Example

A minimal override that changes the framing of a summary prompt:

```text
You are reviewing {{repository_name}}.
Found {{finding_count}} findings at {{risk_band}} risk.
Summarise the most important actions the team should take.
```

The files in this directory are working examples you can copy as a starting
point. Remove or comment out sections you do not want to override.
