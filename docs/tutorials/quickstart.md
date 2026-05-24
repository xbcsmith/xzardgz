# Quickstart Guide

## Installation

Install from the repository root:

```bash
cargo install --path .
```

## Basic Usage

1. Configure your provider. See
   [Configure Providers](../how-to/configure_providers.md).
1. Create or reuse `config.example.yaml`.
1. Scan the current repository:

```bash
xzardgz scan --repository . --output .xzardgz/scan/quickstart_scan.json
```

1. Run a plugin workflow:

```bash
xzardgz run \
  --repository . \
  --plugin technical-review \
  --config config.example.yaml \
  --output .xzardgz/reports
```

1. Inspect available plugins:

```bash
xzardgz plugin list
```

## Next Steps

- Build a plan file with [Create Workflows](../how-to/create_workflows.md).
- Review the [CLI Reference](../reference/cli.md).
- Review the [Configuration Reference](../reference/configuration.md).
