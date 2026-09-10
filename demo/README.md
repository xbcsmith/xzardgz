# Demo

Self-contained, runnable demonstrations for XZardgz. Each subdirectory contains
a `README.md` with step-by-step instructions and expected output that a new
contributor can follow from top to bottom with no other context.

Demos that include a `fixture-repo/` subdirectory can be run entirely offline
against the bundled repository without cloning any external project.

## Subdirectories

### `scan/`

End-to-end walkthrough for the `xzardgz scan` command. Scans the bundled
`fixture-repo/` Python data pipeline and produces a scan artifact JSON file.
Requires no API key or network access.

- `config.yaml` - Demo-specific scanner configuration.
- `fixture-repo/` - Minimal Python data-processing project.

See [scan/README.md](scan/README.md) for the step-by-step walkthrough.

### `security-review/`

End-to-end walkthrough for the `xzardgz run --plugin security-review` command.
Covers three scenarios: offline dry-run (no API key), live local run against the
bundled `fixture-repo/` Python auth service, and a live run against the real
public [pallets/jinja](https://github.com/pallets/jinja) GitHub repository.

- `config.yaml` - Demo-specific security review configuration.
- `fixture-repo/` - Minimal Python authentication service fixture.

See [security-review/README.md](security-review/README.md) for the step-by-step
walkthrough.

### `git-pr/`

End-to-end walkthrough for the git write primitives and GitHub pull request
creation workflow. Covers branch creation, committing report output, pushing to
a remote, and opening a PR. Requires `XZARDGZ_GITHUB_TOKEN` for the PR creation
stage.

- `config.yaml` - Demo configuration with `pr.enabled: false` by default.
- `workflow.yaml` - Technical review workflow plan used as the PR source.
- `fixture-repo/` - Minimal placeholder repository for the git operations demo.

See [git-pr/README.md](git-pr/README.md) for the step-by-step walkthrough.

### `mcp/`

End-to-end walkthrough for MCP (Model Context Protocol) server configuration.
Validates and introspects a filesystem MCP server against the bundled
`fixture-repo/` Python project.

- `config.yaml` - Demo-specific xzardgz configuration.
- `mcp_server_config.yaml` - Annotated server definitions to copy into your own
  `config.yaml`.
- `fixture-repo/` - Minimal Python greeting service used as the analysis target.

See [mcp/README.md](mcp/README.md) for the step-by-step walkthrough.

### `watcher/`

End-to-end walkthrough for the Kafka-backed watcher mode. Covers offline dry-run
validation (no Kafka required), starting the live consumer loop, and publishing
XZepr-shaped `CloudEventMessage` task messages.

- `config.yaml` - Demo watcher configuration with matcher rules.
- `technical_review_task.json` - Reference payload schema for technical review
  tasks.
- `security_review_task.json` - Reference payload schema for security review
  tasks.

See [watcher/README.md](watcher/README.md) for the step-by-step walkthrough.

### `plans/`

Workflow plan files in `.yaml` format. Run them with `xzardgz run --plan`.

- `scan_only.yaml` - Scan a repository without running any plugins.
- `analyze_repo.yaml` - Scan and run the technical review plugin.
- `technical_review_local.yaml` - Comprehensive local technical review.
- `simple_security_review.yaml` - Scan and run the security review plugin.
- `security_review_local.yaml` - Thorough local security review with SARIF
  output.
- `create_pr.yaml` - Configuration snippet showing how to enable GitHub pull
  request creation.

See [plans/README.md](plans/README.md) for details.

### `kafka/`

Kafka configuration snippets to copy into `config.yaml`. Covers both a local
development setup and a production SASL/SSL setup.

- `kafka_config.yaml` - Annotated Kafka, topics, matcher, and watcher sections.

See [kafka/README.md](kafka/README.md) for details.

### `prompts/`

Example prompt template overrides. Copy the directory structure into your
project and reference it with `prompts.directories` in `config.yaml`.

- `technical_review/summary.md` - Override for the technical review summary
  prompt.
- `security_review/summary.md` - Override for the security review summary
  prompt.

See [prompts/README.md](prompts/README.md) for details.

## General Prerequisites

All demos assume:

- `xzardgz` is installed and on your `PATH` (`cargo install --path .`).
- You are running commands from the **repository root** unless a demo's README
  specifies otherwise.

Demos that call AI providers additionally require:

- `OPENAI_API_KEY` set in your environment (or the provider configured
  differently in `config.yaml`).

The `mcp/` demo also requires Node.js 18 or later for the `list-tools` step. The
`watcher/` live consumer loop requires a running Kafka broker. The `git-pr/` PR
creation stage requires `XZARDGZ_GITHUB_TOKEN`.

## Further Reading

- [Quickstart Tutorial](../docs/tutorials/quickstart.md)
- [CLI Reference](../docs/reference/cli.md)
- [Configuration Reference](../docs/reference/configuration.md)
- [Workflow Format Reference](../docs/reference/workflow_format.md)
