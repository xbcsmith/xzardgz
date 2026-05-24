# xzardgz

**Generic AI workflow harness for repository scanning, plugin execution, and
reporting.**

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Overview

xzardgz is a Rust-based command-line workflow harness. It coordinates repository
access, structured scanning, provider-backed agent work, plugin execution,
workspace persistence, report writing, and watcher processing.

The first-release surface is centered on generic workflows rather than a
single-purpose content task. Built-in plugin identifiers are `technical-review`
and `security-review`.

## Features

- **Workflow harness**: Run local plans or direct plugin invocations.
- **Repository scanning**: Build structured scan artifacts for plugins and CI.
- **Plugin runtime**: Execute review plugins against workspaces or scan output.
- **Watcher mode**: Consume task messages and publish result messages.
- **Provider abstraction**: Configure OpenAI, Anthropic, Ollama, or Copilot.
- **Authentication management**: Store, validate, and remove provider
  credentials.
- **Prompt management**: Export, validate, inspect, and render prompt templates.
- **MCP integration**: Validate MCP servers and inspect exposed tools.

## Installation

### From Source

```bash
git clone https://github.com/xbcsmith/xzardgz.git
cd xzardgz
cargo install --path .
```

### Prerequisites

- Rust 1.70+ with the 2024 edition toolchain.
- Credentials or local access for at least one supported provider.
- Kafka access only when using watcher mode.

## Quick Start

1. Create a configuration file:

```yaml
provider:
  default: "openai"

openai:
  api_key_env: "OPENAI_API_KEY"
  model: "gpt-4.1-mini"

workspace:
  root: ".xzardgz/workspaces"

plugins:
  enabled:
    - "technical-review"
    - "security-review"
```

1. Run a technical review workflow:

```bash
xzardgz run \
  --repository . \
  --plugin technical-review \
  --config config.example.yaml \
  --output .xzardgz/reports
```

1. Scan a repository without running a plugin:

```bash
xzardgz scan --repository . --output .xzardgz/scan/scan.json
```

1. Inspect available plugins:

```bash
xzardgz plugin list
```

## Commands

### `run`

Runs a local workflow plan or direct plugin invocation. It prepares a workspace,
opens or clones the repository, scans it, runs the selected plugin, writes
reports, and prints a result summary.

### `scan`

Runs repository scanning only and writes a structured scan artifact.

### `plugin`

Lists plugins, shows plugin schemas, validates plugin configuration, and runs a
plugin against a workspace or scan artifact.

### `watch`

Starts watcher mode for Kafka-backed task processing. It validates matcher
rules, routes accepted tasks through the workflow harness, persists workspace
state, writes reports, and publishes result messages.

### `auth`

Manages credentials for OpenAI, Anthropic, Copilot, and Ollama.

### `prompts`

Manages prompt templates, including export, validation, resolution inspection,
and test rendering.

### `mcp`

Validates MCP server configuration, lists configured servers, discovers tools,
and tests safe tool invocation.

## Configuration

See [Configuration Reference](docs/reference/configuration.md) for supported
configuration sections. The example file is
[config.example.yaml](config.example.yaml).

## Documentation

- [Documentation Index](docs/README.md)
- [Quickstart Guide](docs/tutorials/quickstart.md)
- [Configure Providers](docs/how-to/configure_providers.md)
- [Create Workflows](docs/how-to/create_workflows.md)
- [Architecture](docs/explanation/architecture.md)
- [CLI Reference](docs/reference/cli.md)
- [Configuration Reference](docs/reference/configuration.md)
- [Workflow Format](docs/reference/workflow_format.md)

## Development

### Building

```bash
cargo build --release
```

### Testing

```bash
cargo test
```

### Code Quality

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for
details.
