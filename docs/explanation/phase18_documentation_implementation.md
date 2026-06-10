# Phase 18: Documentation, Examples, and Deployment

## Overview

Phase 18 adds three reference documentation files to the XZardgz project as part
of the documentation, examples, and deployment milestone. These documents
provide operator- and developer-facing reference material for the prompt
customization system, MCP configuration, and plugin development API established
in earlier phases.

---

## Files Created

| File                                                       | Purpose                                                       |
| ---------------------------------------------------------- | ------------------------------------------------------------- |
| `docs/reference/prompt_customization.md`                   | Reference for externalizing and overriding prompt templates   |
| `docs/reference/mcp_configuration.md`                      | Reference for configuring MCP servers and tool allow-lists    |
| `docs/reference/plugin_development.md`                     | Reference for implementing the `WorkflowPlugin` trait         |
| `Dockerfile`                                               | Multi-stage container image for CI and watcher deployments    |
| `.github/workflows/security_review.yaml`                   | GitHub Actions workflow for automated security review         |
| `docs/how-to/deploy.md`                                    | How-to guide for binary, container, and Kubernetes deployment |
| `docs/how-to/setup_watcher.md`                             | How-to guide for configuring and running watcher mode         |
| `examples/watcher/technical_review_task.json`              | Reference task message for testing the watcher                |
| `docs/explanation/phase18_documentation_implementation.md` | This implementation summary                                   |

---

## Document Summaries

### prompt_customization.md

Covers the full prompt template lifecycle: resolution order (CLI directory,
project directory, configured directories, built-in fallback), the two-level
directory layout (`<plugin>/<template_name>.md`), Handlebars variable syntax,
available variables for `technical-review` and `security-review`, built-in
template inventory, the `prompts` configuration section, all `prompts` CLI
subcommands (`export`, `validate`, `resolution-order`, `list`, `render`), a
step-by-step override walkthrough, and best practices.

### mcp_configuration.md

Covers MCP server definitions and all their fields, the two transport types
(`stdio` and `sse`), the explicit tool allow-list model and why it is required,
all four `mcp` CLI subcommands (`validate`, `servers`, `tools`, `test-tool`),
two complete configuration examples (filesystem server and git server), security
considerations (allow-list discipline, timeout configuration, secret injection),
and a troubleshooting table for common validation and discovery failures.

### plugin_development.md

Covers the full `WorkflowPlugin` trait definition and each method's contract,
`PluginContext` with all fields described, `PluginOutput` with constructors and
mutation methods, `PluginFinding` with the five severity levels,
`ToolAccessLevel` variants, `PluginMetadata` construction, plugin registration
and the `plugins.enabled` configuration requirement, the three report writers
(`MarkdownReportWriter`, `JsonReportWriter`, `SarifReportWriter`) and their
format contracts, unit testing patterns using `MockProvider` and
`PluginContext::new`, and best practices covering diagnostics use, risk band
management, tool access minimization, and graceful error handling.

---

## Deployment Artifact Summaries

### Dockerfile

Two-stage build. The builder stage uses `rust:1.87-slim` and installs
`build-essential`, `cmake`, `clang`, `libssl-dev`, `libsasl2-dev`,
`librdkafka-dev`, and `pkg-config`. The dependency layer is cached behind a
separate `RUN` step using a stub `src/main.rs` so that source-only changes do
not trigger a full dependency rebuild.

The runtime stage uses `debian:bookworm-slim` and installs only `libssl3`,
`libsasl2-2`, and `ca-certificates`. The binary runs as the `xzardgz` non-root
system user with `WORKDIR /workspace`. The default command is `xzardgz --help`.

### security_review.yaml

GitHub Actions workflow triggered on push to `main` and on pull requests.
Permissions are scoped to `security-events: write` and `contents: read`.

Steps:

1. Checkout with `actions/checkout@v4`.
2. Cache `~/.cargo/registry`, `~/.cargo/git`, and `target/` keyed on
   `Cargo.lock` hash using `actions/cache@v4`.
3. Install `xzardgz` with `cargo install --path .`.
4. Write a minimal `config.yaml` enabling the security-review plugin with SARIF
   output to `sarif-results/`.
5. Run `xzardgz run --plugin security-review --output sarif-results`.
6. Upload SARIF to GitHub Code Scanning with
   `github/codeql-action/upload-sarif@v3` (runs on `if: always()`).

`OPENAI_API_KEY` is injected from a repository secret.

### docs/how-to/deploy.md

Covers the full deployment surface: local binary build (dev, release, install,
cross-compilation notes), container build and run commands for one-shot and
watcher modes, GitHub Actions integration steps, Kubernetes `Deployment`
configuration (ConfigMap volumes, Secrets, PVC, resource limits, liveness and
readiness probes, CronJob pattern), health and readiness CLI checks
(`--dry-run`, `--once`), and an environment variable reference table.

### docs/how-to/setup_watcher.md

Step-by-step guide for configuring and running watcher mode. Covers local and
SASL/SSL Kafka configuration, matcher configuration with the reject-by-default
safety rule, dry-run validation, starting the watcher, publishing a test task,
monitoring results, a complete configuration example, a matcher rules table,
once mode, and a troubleshooting table for the six most common failure modes.

### examples/watcher/technical_review_task.json

Minimal `WatcherTaskMessage` JSON payload for manual watcher testing. Uses the
`xzardgz.technical_review.requested` event type to match the default matcher
configuration in `config.example.yaml`.

---

## Quality Gate Results

All documentation quality gates passed:

```bash
markdownlint --fix --config .markdownlint.json \
  docs/reference/prompt_customization.md \
  docs/reference/mcp_configuration.md \
  docs/reference/plugin_development.md \
  docs/how-to/deploy.md \
  docs/how-to/setup_watcher.md
# clean

prettier --write --parser markdown --prose-wrap always \
  docs/reference/prompt_customization.md \
  docs/reference/mcp_configuration.md \
  docs/reference/plugin_development.md \
  docs/how-to/deploy.md \
  docs/how-to/setup_watcher.md
# clean
```

All Rust quality gates passed:

```bash
cargo fmt --all          # clean
cargo check --all-targets --all-features  # clean
cargo clippy --all-targets --all-features -- -D warnings  # clean
cargo test --all-features  # 1431 unit tests + 380 doc tests passed, 0 failed
```

---

## Related Documentation

- `docs/explanation/phase12_mcp_client_implementation.md` - MCP client
  architecture
- `docs/explanation/phase13_plugins_implementation.md` - plugin module
  architecture
- `docs/explanation/phase13_plugin_runtime_implementation.md` - plugin runtime
  design
- `docs/explanation/phase15_technical_review_implementation.md` - technical
  review plugin
- `docs/explanation/phase16_security_review_implementation.md` - security review
  plugin
- `docs/reference/configuration.md` - full configuration reference
- `docs/reference/cli.md` - CLI command reference
