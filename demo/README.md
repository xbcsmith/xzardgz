# Demo

This directory contains runnable example files for XZardgz. Each subdirectory
covers a different configuration or workflow surface. Copy the files you need
into your own project and adjust them to match your environment.

## Subdirectories

### `plans/`

Workflow plan files in `.yaml` format. Run them with `xzardgz run --plan`.

- `scan_only.yaml` - Scan a repository without running any plugins.
- `analyze_repo.yaml` - Scan and run the technical review plugin.
- `technical_review_local.yaml` - Comprehensive local technical review.
- `simple_security_review.yaml` - Scan and run the security review plugin.
- `security_review_local.yaml` - Thorough local security review with SARIF
  output.

See [plans/README.md](plans/README.md) for details.

### `watcher/`

Example Kafka task messages in JSON format. Publish them to your watcher input
topic to test watcher mode without a live CI system.

- `technical_review_task.json` - Task message for a technical review.
- `security_review_task.json` - Task message for a security review.

See [watcher/README.md](watcher/README.md) for details.

### `kafka/`

Kafka configuration snippets to copy into `config.yaml`. Covers both a local
development setup and a production SASL/SSL setup.

- `kafka_config.yaml` - Annotated Kafka, topics, matcher, and watcher sections.

See [kafka/README.md](kafka/README.md) for details.

### `mcp/`

MCP (Model Context Protocol) server configuration snippets. Copy server
definitions into the `mcp.servers` section of `config.yaml`.

- `mcp_server_config.yaml` - Filesystem and git MCP server definitions.

See [mcp/README.md](mcp/README.md) for details.

### `prompts/`

Example prompt template overrides. Copy the directory structure into your
project and reference it with `prompts.directories` in `config.yaml`.

- `technical_review/summary.md` - Override for the technical review summary
  prompt.
- `security_review/summary.md` - Override for the security review summary
  prompt.

See [prompts/README.md](prompts/README.md) for details.

## Prerequisites

All examples assume:

- `xzardgz` is installed and on your `PATH`.
- `OPENAI_API_KEY` is set in your environment (or the provider is configured
  differently in `config.yaml`).
- You are running commands from the root of the repository you want to analyse.

## Further Reading

- [Quickstart Tutorial](../docs/tutorials/quickstart.md)
- [CLI Reference](../docs/reference/cli.md)
- [Configuration Reference](../docs/reference/configuration.md)
- [Workflow Format Reference](../docs/reference/workflow_format.md)
