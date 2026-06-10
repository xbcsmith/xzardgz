# Phase 18: Documentation, Examples, and Deployment

## Overview

Phase 18 completes the first-release documentation set for XZardgz. It adds
reference documentation for all major subsystems, updates and expands example
files, adds deployment artifacts for container and CI use, and updates the
documentation index so a new user can configure OpenAI, run a local review,
start watcher mode, and understand report outputs from docs alone.

---

## Files Created

### Reference Documentation

| File                                        | Purpose                                                                                      |
| ------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `docs/reference/watcher_mode.md`            | Watcher configuration, matcher rules, health guidance, and Kubernetes deployment notes       |
| `docs/reference/kafka_schemas.md`           | Task and result message JSON schemas, field descriptions, and full examples                  |
| `docs/reference/authentication.md`          | Provider authentication mechanisms, auth commands, env vars, and security best practices     |
| `docs/reference/prompt_customization.md`    | Prompt resolution order, Handlebars variables, template layout, and override guide           |
| `docs/reference/mcp_configuration.md`       | MCP server definitions, transport types, tool allow-listing, and CLI commands                |
| `docs/reference/plugin_development.md`      | WorkflowPlugin trait, PluginContext, PluginOutput, PluginFinding, registration, and testing  |
| `docs/reference/technical_review_plugin.md` | Technical review configuration, dimensions, finding structure, and report formats            |
| `docs/reference/security_review_plugin.md`  | Security review configuration, scope areas, SARIF output, secret handling, and CI exit codes |
| `docs/reference/workspace_model.md`         | WorkspaceState schema, directory layout, resume behavior, and idempotency rules              |
| `docs/reference/scanner_artifacts.md`       | ScanResult and ScannedFile schemas, ignore rules, and scanner configuration                  |
| `docs/reference/governance.md`              | GovernanceConfig, validation functions, violations, and fail behavior                        |

### How-To Guides

| File                           | Purpose                                                                              |
| ------------------------------ | ------------------------------------------------------------------------------------ |
| `docs/how-to/setup_watcher.md` | Step-by-step watcher mode setup: Kafka config, matcher, validation, and operation    |
| `docs/how-to/deploy.md`        | Binary build, container build and run, GitHub Actions, Kubernetes watcher deployment |

### Example Files

| File                                           | Purpose                                                     |
| ---------------------------------------------- | ----------------------------------------------------------- |
| `examples/plans/analyze_repo.yaml`             | Updated from legacy format to current plan schema           |
| `examples/plans/simple_security_review.yaml`   | Updated from legacy format to current plan schema           |
| `examples/plans/technical_review_local.yaml`   | Local technical review with all focus areas                 |
| `examples/plans/security_review_local.yaml`    | Local security review with SARIF output                     |
| `examples/plans/scan_only.yaml`                | Scan-only plan with no plugin steps                         |
| `examples/watcher/technical_review_task.json`  | WatcherTaskMessage for technical review                     |
| `examples/watcher/security_review_task.json`   | WatcherTaskMessage for security review                      |
| `examples/kafka/kafka_config.yaml`             | Annotated Kafka config with PLAINTEXT and SASL/SSL sections |
| `examples/mcp/mcp_server_config.yaml`          | Annotated MCP server config with filesystem and git servers |
| `examples/prompts/README.md`                   | Prompt override directory guide                             |
| `examples/prompts/technical_review/summary.md` | Example technical review summary prompt override            |
| `examples/prompts/security_review/summary.md`  | Example security review summary prompt override             |

### Deployment Artifacts

| File                                     | Purpose                                                    |
| ---------------------------------------- | ---------------------------------------------------------- |
| `Dockerfile`                             | Multi-stage container image for CI and watcher deployments |
| `.github/workflows/security_review.yaml` | GitHub Actions workflow with SARIF upload                  |

### Documentation Index

| File                                                                  | Purpose                                                  |
| --------------------------------------------------------------------- | -------------------------------------------------------- |
| `docs/README.md`                                                      | Updated index linking all new and existing documentation |
| `docs/explanation/phase18_documentation_deployment_implementation.md` | This implementation summary                              |

---

## Design Decisions

### Reference documentation structure

All reference documents follow the Diataxis framework: they are
information-oriented technical specifications rather than tutorials or how-to
guides. Cross-links connect reference docs to related how-to guides so readers
can move from specification to action.

### Example plan format migration

The two existing example plan files (`analyze_repo.yaml` and
`simple_security_review.yaml`) used a legacy format with
`action.type: scan_repository` and `action.type: run_plugin`. Both were migrated
to the current plan format using `type: scan` and `type: plugin` step types
consistent with the workflow format specification.

### Kafka schema examples

The task message examples use the `xzardgz.technical_review.task` and
`xzardgz.security_review.task` event type strings established in Phase 14. The
result message examples show both success and failure shapes to make it clear
how consuming systems should handle both outcomes.

### Dockerfile layer ordering

The Dockerfile uses a dependency-caching layer that compiles an empty stub
binary before copying actual source code. This keeps dependency compilation
cached across source-only changes, which is the most common edit pattern.

### GitHub Actions SARIF upload

The SARIF upload step uses `if: always()` so security findings are uploaded even
when the run command exits with a non-zero status due to findings. This ensures
Code Scanning always receives results from the workflow run.

### Kubernetes readiness probe

The watcher readiness probe uses `xzardgz watch --dry-run` to verify that the
Kafka configuration is valid and the matcher is non-empty. A non-zero exit code
from dry-run indicates a misconfiguration before any real messages are consumed.

---

## Success Criteria Verification

| Criterion                                                | Status                                                                                                                                                                     |
| -------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A new user can configure OpenAI from docs alone          | `docs/reference/authentication.md` and `docs/how-to/configure_providers.md` cover the full flow                                                                            |
| A new user can run a local review from docs alone        | `docs/tutorials/quickstart.md` and updated plan examples cover the complete flow                                                                                           |
| A new user can start watcher mode from docs alone        | `docs/how-to/setup_watcher.md` provides step-by-step instructions                                                                                                          |
| A new user can understand report outputs from docs alone | `docs/reference/technical_review_plugin.md`, `docs/reference/security_review_plugin.md`, and `docs/reference/workspace_model.md` describe all report formats and locations |
| Complete first-release documentation set                 | All 11 reference docs, 2 new how-to guides, 12 example files, and 2 deployment artifacts created                                                                           |
| Updated examples                                         | Legacy plan files migrated to current format; 3 new plan examples added                                                                                                    |
| Deployment artifacts                                     | Dockerfile and GitHub Actions workflow created                                                                                                                             |
| GitHub Action sample                                     | `.github/workflows/security_review.yaml` with SARIF upload created                                                                                                         |

---

## Quality Gate Results

All Rust quality gates were verified across all phases:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

All Markdown files were linted and formatted:

```bash
markdownlint --fix --config .markdownlint.json <file>
prettier --write --parser markdown --prose-wrap always <file>
```

---

## Related Documentation

- `docs/explanation/phase14_watcher_implementation.md` - watcher mode design
- `docs/explanation/phase13_plugin_runtime_implementation.md` - plugin runtime
  design
- `docs/explanation/phase15_technical_review_implementation.md` - technical
  review plugin
- `docs/explanation/phase16_security_review_implementation.md` - security review
  plugin
- `docs/explanation/phase17_workflow_executor_implementation.md` - workflow
  executor integration
- `docs/reference/configuration.md` - full configuration reference
- `docs/reference/cli.md` - CLI command reference
