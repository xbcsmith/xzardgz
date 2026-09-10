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
- [Setup Watcher Mode](how-to/setup_watcher.md) - Configure and run Kafka-backed
  watcher mode.
- [Deploy](how-to/deploy.md) - Binary build, container, GitHub Actions, and
  Kubernetes deployment.
- [Integrate a Downstream Service](how-to/integrate_downstream_service.md) -
  Consume and report work lifecycle events.

### Explanation

Background, architecture, and design notes.

- [Architecture](explanation/architecture.md) - Workflow harness architecture
  and component overview.
- [Workflow Harness Refactor Plan](explanation/workflow_harness_refactor_implementation_plan.md) -
  First-release implementation plan.
- [Downstream Consumer Implementation](explanation/downstream_consumer_implementation.md) -
  Kafka consumer implementation summary.

### Reference

Technical specifications and command details.

- [CLI Commands](reference/cli.md) - Full command surface reference.
- [Configuration](reference/configuration.md) - Configuration model reference.
- [Workflow Format](reference/workflow_format.md) - Plan file specification.
- [Authentication](reference/authentication.md) - Provider authentication and
  credential management.
- [Watcher Mode](reference/watcher_mode.md) - Watcher configuration, matcher
  rules, and operational guidance.
- [Kafka Schemas](reference/kafka_schemas.md) - Task and result message JSON
  schemas and examples.
- [Technical Review Plugin](reference/technical_review_plugin.md) - Technical
  review plugin configuration and dimensions.
- [Security Review Plugin](reference/security_review_plugin.md) - Security
  review plugin, SARIF output, and CI integration.
- [Workspace Model](reference/workspace_model.md) - Workspace state, directory
  layout, and resume behavior.
- [Scanner Artifacts](reference/scanner_artifacts.md) - Scan artifact schema,
  ignore rules, and scanner configuration.
- [Governance](reference/governance.md) - Governance checks, violation handling,
  and configuration.
- [Plugin Development](reference/plugin_development.md) - WorkflowPlugin trait,
  PluginContext, PluginOutput, and registration.
- [Prompt Customization](reference/prompt_customization.md) - Prompt resolution
  order, template format, and override guide.
- [MCP Configuration](reference/mcp_configuration.md) - MCP server definitions,
  tool allow-listing, and CLI commands.
- [Downstream Consumer API](reference/downstream_consumer_api.md) - Consumer
  environment variables and lifecycle events.

## Documentation by Feature

### Getting Started

- [Quickstart Tutorial](tutorials/quickstart.md)
- [Configure Providers](how-to/configure_providers.md)
- [Authentication Reference](reference/authentication.md)

### Workflow Automation

- [Create Workflows](how-to/create_workflows.md)
- [Workflow Format Reference](reference/workflow_format.md)
- [Technical Review Plugin](reference/technical_review_plugin.md)
- [Security Review Plugin](reference/security_review_plugin.md)

### Command Surface

- [CLI Commands](reference/cli.md)
- [Configuration Options](reference/configuration.md)

### Watcher and Kafka Integration

- [Setup Watcher Mode](how-to/setup_watcher.md)
- [Watcher Mode Reference](reference/watcher_mode.md)
- [Kafka Schemas](reference/kafka_schemas.md)

### Deployment

- [Deploy](how-to/deploy.md)
- [GitHub Actions example](.github/workflows/security_review.yaml)
- [Dockerfile](../Dockerfile)

### Plugin and Prompt Development

- [Plugin Development Reference](reference/plugin_development.md)
- [Prompt Customization Reference](reference/prompt_customization.md)

### System Architecture

- [Architecture Overview](explanation/architecture.md)
- [Scanner Artifacts](reference/scanner_artifacts.md)
- [Workspace Model](reference/workspace_model.md)
- [Governance](reference/governance.md)
- [MCP Configuration](reference/mcp_configuration.md)

## Demos

Self-contained, runnable demonstrations are in the [`demo/`](../demo/)
directory. Each subdirectory contains a `README.md` with step-by-step
instructions and expected output.

- [`demo/mcp/`](../demo/mcp/) - Validate and introspect MCP server configuration
  against a bundled fixture repository.
- [`demo/plans/`](../demo/plans/) - Workflow plan files for common scenarios.
- [`demo/watcher/`](../demo/watcher/) - Example Kafka task messages for testing
  watcher mode.
- [`demo/kafka/`](../demo/kafka/) - Kafka configuration snippets.
- [`demo/prompts/`](../demo/prompts/) - Prompt template override examples.

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
