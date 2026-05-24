# Legacy Product Surface Removal Implementation

## Overview

This work implements Phase 1 from the workflow harness refactor plan. XZardgz no
longer exposes the legacy Chat or Doc Gen product surface. The public command
surface now reflects the generic workflow harness direction with `run`, `scan`,
`plugin`, `watch`, `auth`, `prompts`, and `mcp` commands.

The implementation removes the Doc Gen module tree, removes the `chat` and
`generate` command handlers, removes documentation generation errors and
workflow actions, updates tests, and rewrites public-facing documentation and
examples.

## Components

- `src/cli.rs`: Replaced the legacy `Chat` and `Generate` commands with the
  Phase 1 workflow harness command surface.
- `src/main.rs`: Removed legacy command routing and routed the new command
  variants to Phase 1 command handlers.
- `src/commands/mod.rs`: Removed `chat` and `generate` exports and added `scan`,
  `plugin`, `watch`, `prompts`, and `mcp` exports.
- `src/commands/scan.rs`: Added a Phase 1 scan command placeholder.
- `src/commands/plugin.rs`: Added a Phase 1 plugin command placeholder.
- `src/commands/watch.rs`: Added a Phase 1 watcher command placeholder.
- `src/commands/prompts.rs`: Added a Phase 1 prompts command placeholder.
- `src/commands/mcp.rs`: Added a Phase 1 MCP command placeholder.
- `src/commands/chat.rs`: Deleted the legacy Chat command handler.
- `src/commands/generate.rs`: Deleted the legacy Doc Gen command handler.
- `src/docgen`: Deleted the legacy documentation generation module tree.
- `src/lib.rs`: Removed the public `docgen` module export.
- `src/error.rs`: Removed `DocGenError` and the top-level Doc Gen error variant.
- `src/config.rs`: Removed the legacy `DocumentationConfig` field.
- `src/workflow/plan.rs`: Removed `GenerateDocumentation` and `DocCategory`, and
  added a plugin-oriented `RunPlugin` action.
- `src/workflow/executor.rs`: Removed the legacy documentation generation branch
  and added a Phase 1 plugin execution branch.
- `tests/unit/cli_tests.rs`: Added CLI tests for accepted and rejected commands.
- `tests/unit/parser_tests.rs`: Added a parser test proving `generate_docs` is
  rejected.
- `tests/unit/docgen_tests.rs`: Deleted legacy Doc Gen tests.
- `tests/unit.rs`: Removed the Doc Gen test module and added CLI tests.
- `README.md`: Rewritten around the generic workflow harness identity.
- `docs/README.md`: Updated documentation index and normalized how-to links.
- `docs/explanation/architecture.md`: Rewritten for the workflow harness
  architecture.
- `docs/reference/cli.md`: Replaced legacy command documentation with the Phase
  1 command surface.
- `docs/reference/configuration.md`: Documented first-release configuration
  sections and validation rules.
- `docs/reference/workflow_format.md`: Replaced legacy action examples with a
  plugin-first workflow format.
- `docs/tutorials/quickstart.md`: Updated quickstart commands for scanning and
  plugin execution.
- `docs/how-to/configure_providers.md`: Updated provider setup for the workflow
  harness provider model.
- `docs/how-to/create_workflows.md`: Updated workflow creation guidance for scan
  and plugin steps.
- `config.example.yaml`: Replaced legacy configuration with first-release
  workflow harness sections.
- `sample_plan.yaml`: Replaced legacy actions with a parser-compatible plugin
  workflow.
- `examples/plans/analyze_repo.yaml`: Replaced legacy Doc Gen examples with a
  technical review workflow.
- `examples/plans/simple_security_review.yaml`: Replaced the legacy tutorial
  generation example with a security review workflow.
- `docs/explanation/workflow_harness_refactor_implementation_plan.md`:
  Normalized Phase 1 how-to path wording.

## Implementation Details

The active CLI command set now contains:

- `run`
- `scan`
- `plugin`
- `watch`
- `auth`
- `prompts`
- `mcp`

The legacy `chat` and `generate` commands are no longer represented in the Clap
command enum or command routing. Their handler files were removed.

The legacy `src/docgen` module tree was removed and is no longer exported from
`src/lib.rs`. Doc Gen-specific workflow structures were also removed from
`src/workflow/plan.rs`, so `generate_docs` no longer deserializes as a valid
workflow action.

The new `scan`, `plugin`, `watch`, `prompts`, and `mcp` handlers intentionally
provide Phase 1 placeholders. Full command behavior is scheduled in later phases
of the refactor plan, but the command surface is available and validated now.

Task-oriented documentation links use `docs/how-to`. Markdown filenames remain
lowercase with underscores, except for the allowed `README.md` files.

## Testing Results

The required Rust quality gates passed:

- `cargo fmt --all`
- `cargo check --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`

Markdown formatting and linting passed for the Phase 1 public-facing Markdown
files updated by this work:

- `README.md`
- `docs/README.md`
- `docs/explanation/architecture.md`
- `docs/explanation/legacy_product_surface_removal_implementation.md`
- `docs/explanation/workflow_harness_refactor_implementation_plan.md`
- `docs/how-to/configure_providers.md`
- `docs/how-to/create_workflows.md`
- `docs/reference/cli.md`
- `docs/reference/configuration.md`
- `docs/reference/workflow_format.md`
- `docs/tutorials/quickstart.md`

Mechanical checks also passed for active Rust source paths:

- No `Commands::Chat`, `commands::chat`, or `pub mod chat` references remain.
- No `Commands::Generate`, `commands::generate`, or `pub mod generate`
  references remain.
- No `crate::docgen`, `pub mod docgen`, `DocGenError`, `GenerateDocumentation`,
  or `generate_docs` references remain in active Rust source.
- `src/docgen` no longer exists.
- `src/commands/chat.rs` no longer exists.
- `src/commands/generate.rs` no longer exists.
- `docs/how_to` no longer exists.
- No `.yml` files are present in the project.
