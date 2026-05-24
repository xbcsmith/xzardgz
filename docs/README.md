# XZardgz Documentation

Welcome to the XZardgz documentation. Use this index to find workflow harness
concepts, setup guides, command references, and implementation notes.

## Quick Start

New to XZardgz? Start with the [Quickstart Tutorial](tutorials/quickstart.md).

## Documentation by Type

### Tutorials

Step-by-step learning material.

- [Quickstart](tutorials/quickstart.md) - Install the CLI and run a first plugin
  workflow.

### How-To Guides

Task-focused guides for specific goals.

- [Configure Providers](how-to/configure_providers.md) - Configure OpenAI,
  Anthropic, Ollama, or Copilot.
- [Create Workflows](how-to/create_workflows.md) - Build plugin-first workflow
  plans.
- [Integrate a Downstream Service](how-to/integrate_downstream_service.md) -
  Consume and report work lifecycle events.

### Explanation

Background, architecture, and design notes.

- [Architecture](explanation/architecture.md) - Workflow harness architecture.
- [Workflow Harness Refactor Plan](explanation/workflow_harness_refactor_implementation_plan.md)
  - First-release implementation plan.
- [Downstream Consumer Implementation](explanation/downstream_consumer_implementation.md)
  - Kafka consumer implementation summary.

### Reference

Technical specifications and command details.

- [CLI Commands](reference/cli.md) - Command surface reference.
- [Configuration](reference/configuration.md) - Configuration model reference.
- [Workflow Format](reference/workflow_format.md) - Plan file specification.
- [Downstream Consumer API](reference/downstream_consumer_api.md) - Consumer
  environment variables and lifecycle events.

## Documentation by Feature

### Getting Started

- [Quickstart Tutorial](tutorials/quickstart.md)
- [Configure Providers](how-to/configure_providers.md)

### Workflow Automation

- [Create Workflows](how-to/create_workflows.md)
- [Workflow Format Reference](reference/workflow_format.md)

### Command Surface

- [CLI Commands](reference/cli.md)
- [Configuration Options](reference/configuration.md)

### System Architecture

- [Architecture Overview](explanation/architecture.md)

## Finding What You Need

### I want to learn how to use XZardgz

Start with [Tutorials](tutorials/).

### I need to accomplish a specific task

Check [How-To Guides](how-to/).

### I want to understand how XZardgz works

Read [Explanation](explanation/).

### I need technical specifications

Look in [Reference](reference/).

## Contributing to Documentation

When adding documentation:

1. Choose the correct documentation category.
2. Use `lowercase_with_underscores.md` for filenames.
3. Use `docs/how-to` for task-oriented guides.
4. Update this index.
5. Follow guidelines in [AGENTS.md](../AGENTS.md).

## External Resources

- [Project Repository](https://github.com/xbcsmith/xzardgz) - Source code.
- [Issue Tracker](https://github.com/xbcsmith/xzardgz/issues) - Report problems.
