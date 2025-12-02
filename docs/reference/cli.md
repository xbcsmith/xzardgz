# CLI Reference

## Commands

### `run`

Run a workflow plan.

```bash
xzardgz run --plan <PATH>
```

### `generate`

Generate documentation.

```bash
xzardgz generate --repository <PATH> --category <CATEGORY> --topic <TOPIC>
```

Options:
- `--repository`: Path to the repository (default: ".")
- `--category`: Documentation category (tutorial, how-to, explanation, reference)
- `--topic`: Topic to generate documentation for
- `--output`: Output directory (default: ".")
- `--overwrite`: Overwrite existing files

### `auth`

Authenticate with providers.

```bash
xzardgz auth login
```
