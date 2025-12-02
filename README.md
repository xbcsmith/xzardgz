# xzardgz

**Autonomous AI agent for documentation generation using the Diataxis framework.**

[![CI](https://github.com/xbcsmith/xzardgz/workflows/CI/badge.svg)](https://github.com/xbcsmith/xzardgz/actions)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Overview

xzardgz is a Rust-based autonomous AI agent that helps you generate high-quality documentation following the [Diataxis](https://diataxis.fr/) framework. It analyzes your codebase and generates documentation across four categories:

- **Tutorials**: Learning-oriented guides
- **How-to guides**: Problem-solving oriented instructions
- **Explanation**: Understanding-oriented discussions
- **Reference**: Information-oriented technical descriptions

## Features

- 🤖 **AI-Powered**: Leverages AI providers (Ollama, GitHub Copilot) for content generation
- 📚 **Diataxis Framework**: Follows best practices for documentation structure
- 🔄 **Workflow Engine**: Execute multi-step documentation generation workflows
- 🔍 **Repository Analysis**: Scans and analyzes code structure
- 🛠️ **Tool System**: Extensible tool framework for file operations and more
- 🎯 **CLI Interface**: Simple command-line interface

## Installation

### From Source

```bash
git clone https://github.com/xbcsmith/xzardgz.git
cd xzardgz
cargo install --path .
```

### Prerequisites

- Rust 1.70+ (2024 edition)
- Ollama (for local AI) or GitHub Copilot access

## Quick Start

1. **Configure your provider**:

```yaml
# config.yaml
provider:
  provider_type: "ollama"
  model: "qwen2.5-coder"
```

2. **Generate documentation**:

```bash
xzardgz generate \
  --repository . \
  --category tutorial \
  --topic "Getting Started"
```

3. **Run a workflow**:

```bash
xzardgz run --plan examples/plans/analyze_repo.yaml
```

## Commands

### `generate`

Generate documentation for a specific topic:

```bash
xzardgz generate \
  --repository <PATH> \
  --category <tutorial|how-to|explanation|reference> \
  --topic "<TOPIC>" \
  --output <OUTPUT_DIR> \
  --overwrite
```

### `run`

Execute a workflow plan:

```bash
xzardgz run --plan <PLAN_FILE>
```

### `auth`

Authenticate with providers:

```bash
xzardgz auth login
```

### `chat`

Interactive chat session:

```bash
xzardgz chat
```

## Configuration

See [Configuration Reference](docs/reference/configuration.md) for detailed configuration options.

## Documentation

- [Quickstart Guide](docs/tutorials/quickstart.md)
- [Configure Providers](docs/how_to/configure_providers.md)
- [Create Workflows](docs/how_to/create_workflows.md)
- [Architecture](docs/explanation/architecture.md)
- [CLI Reference](docs/reference/cli.md)
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
cargo clippy
cargo fmt
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Inspired by the [Diataxis](https://diataxis.fr/) documentation framework
- Built with Rust and async/await
