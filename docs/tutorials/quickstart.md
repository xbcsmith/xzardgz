# Quickstart Guide

## Installation

```bash
cargo install --path .
```

## Basic Usage

1. Configure your provider (see [Configure Providers](../how_to/configure_providers.md)).
2. Generate documentation:

```bash
xzardgz generate --repository . --category tutorial --topic "Getting Started"
```

3. Run a workflow:

```bash
xzardgz run --plan examples/plans/analyze_repo.yaml
```
