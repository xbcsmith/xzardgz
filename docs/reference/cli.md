# CLI Reference

## Overview

The XZardgz CLI exposes a workflow harness command surface. Commands are grouped
around local workflow execution, scan artifact creation, plugin operations,
watcher processing, provider authentication, prompt management, and MCP
inspection.

## Global Options

Common options may be accepted by multiple commands:

- `--config <PATH>`: Load configuration from a `.yaml` file.
- `--provider <NAME>`: Override the configured provider.
- `--model <NAME>`: Override the configured model.
- `--workspace <PATH>`: Use a specific workspace root or workspace instance.
- `--dry-run`: Validate and plan work without performing external side effects.
- `--trace-transcript`: Persist provider and tool transcripts when supported.

## Commands

### `run`

Run a local workflow plan or direct plugin invocation.

```bash
xzardgz run \
  --repository <PATH_OR_URL> \
  --plugin <PLUGIN> \
  --config <CONFIG_FILE> \
  --output <OUTPUT_DIR>
```

Important options:

- `--plan <PLAN_FILE>`: Load workflow settings from a plan file.
- `--repository <PATH_OR_URL>`: Repository path or clone URL.
- `--branch <BRANCH>`: Target branch.
- `--plugin <PLUGIN>`: Plugin identifier, such as `technical-review` or
  `security-review`.
- `--scan-artifact <PATH>`: Reuse an existing scan artifact.
- `--max-findings <N>`: Limit findings returned by a plugin.
- `--report-format <FORMAT>`: Request report formats such as `markdown`, `json`,
  or `sarif`.
- `--output <PATH>`: Report output directory.

### `scan`

Run repository scanning only and write a structured scan artifact.

```bash
xzardgz scan --repository <PATH_OR_URL> --output <SCAN_ARTIFACT>
```

Typical uses:

- CI preflight checks.
- Plugin development.
- Debugging matcher or ignore rules.
- Producing reusable input for `plugin run`.

### `plugin`

Inspect, validate, and run plugins.

```bash
xzardgz plugin list
xzardgz plugin schema <PLUGIN>
xzardgz plugin validate <PLUGIN> --config <CONFIG_FILE>
xzardgz plugin run <PLUGIN> --workspace <WORKSPACE>
xzardgz plugin run <PLUGIN> --scan-artifact <SCAN_ARTIFACT>
```

Subcommands:

- `list`: Show available plugin identifiers and metadata.
- `schema <PLUGIN>`: Print a plugin configuration schema.
- `validate <PLUGIN>`: Validate plugin-specific configuration.
- `run <PLUGIN>`: Run a plugin against a workspace or scan artifact.
- `formats <PLUGIN>`: Show supported report formats.

### `watch`

Start Kafka-backed watcher mode.

```bash
xzardgz watch --config <CONFIG_FILE>
```

Important options:

- `--kafka-brokers <BROKERS>`: Override configured Kafka brokers.
- `--input-topic <TOPIC>`: Override the task topic.
- `--output-topic <TOPIC>`: Override the result topic.
- `--matcher <PATH>`: Override matcher configuration.
- `--once`: Process a bounded batch for tests or scheduled jobs.
- `--max-concurrent-tasks <N>`: Limit concurrent task execution.
- `--no-publish-results`: Disable result publishing for local validation.

### `auth`

Manage provider authentication.

```bash
xzardgz auth login openai
xzardgz auth login anthropic
xzardgz auth login copilot
xzardgz auth login ollama
xzardgz auth status
xzardgz auth validate
xzardgz auth set-key openai
xzardgz auth remove-key openai
xzardgz auth logout openai
```

Supported provider names are `openai`, `anthropic`, `copilot`, and `ollama`.

### `prompts`

Manage prompt templates.

```bash
xzardgz prompts export --output ./prompts
xzardgz prompts validate --dir ./prompts
xzardgz prompts resolution-order
xzardgz prompts list --plugin technical-review
xzardgz prompts render --template technical-review/summary --context context.json
```

Typical uses:

- Export built-in templates for review.
- Validate custom prompt directories.
- Inspect prompt resolution order.
- Render a prompt with test context before plugin execution.

### `mcp`

Validate MCP client configuration and inspect available tools.

```bash
xzardgz mcp validate --config <CONFIG_FILE>
xzardgz mcp servers --config <CONFIG_FILE>
xzardgz mcp tools <SERVER> --config <CONFIG_FILE>
xzardgz mcp test-tool <SERVER> <TOOL> --config <CONFIG_FILE>
```

MCP tools must be explicitly allowed in configuration before plugins can use
them.

## Exit Behavior

Commands should return a nonzero status for invalid configuration,
authentication failure, repository access failure, plugin failure, report write
failure, and watcher validation failure. Plugin findings may also produce a
nonzero status when configured by workflow policy.
