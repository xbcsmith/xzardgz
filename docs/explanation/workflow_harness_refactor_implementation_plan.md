# XZardgz Reposcan-Style Workflow Harness Implementation Plan

## Overview

This plan refactors XZardgz into a generic, reposcan-style AI workflow harness.
The refactor is a breaking first-release rewrite: old Doc Gen and Chat surfaces
will be removed rather than migrated, and the project will implement the
reposcan architecture and workflow patterns as the baseline product shape.

The first release will include local workflow execution, watcher mode, Kafka
task consumption, Kafka result publishing, workspace persistence, repository
scanning, governance checks, provider abstraction, authentication for all
supported providers, prompt templates, MCP client support, sandboxed tools,
plugin runtime, report infrastructure, a technical repository review plugin, and
a security repository review plugin with SARIF support.

All phases in this document are first-release work. Nothing marked here is
intended as a post-release follow-up unless it is explicitly listed as out of
scope.

## First-Release Product Decisions

- XZardgz becomes a generic workflow harness, not a documentation generator.
- Backward compatibility is not required.
- The `chat` command is removed entirely.
- The `generate` command is removed entirely.
- The `src/docgen` module tree is removed entirely.
- Doc Gen workflow actions, config fields, tests, and documentation are removed.
- The Export Restrictions plugin is not implemented.
- Export Restrictions config, events, scanner domain types, and scoring are not
  added.
- Watcher mode is required in the first release.
- Watcher results are published to Kafka in the first release.
- OpenAI is the default and primary provider.
- Authentication supports all configured providers, with OpenAI as the main
  path.
- Thinking mode auto detection is required for all providers that expose model
  capability metadata.
- Automatic model selection based on provider availability is required before
  plugin execution.
- The first built-in plugins are `technical-review` and `security-review`.
- Security review includes Markdown, JSON, and SARIF output.
- Documentation paths normalize from `docs/how_to` to `docs/how-to`.
- Implementation summaries remain in `docs/explanation`.

## Current State Analysis

### Existing Infrastructure

- `src/cli.rs` exposes `run`, `chat`, `auth`, and Doc Gen-specific `generate`.
- `src/main.rs` routes the current command enum.
- `src/commands` contains command handlers for `run`, `chat`, `auth`, and
  `generate`.
- `src/docgen` contains Diataxis documentation generation code.
- `src/config.rs` has a small provider, agent, repository, and documentation
  config model.
- `src/error.rs` includes Doc Gen-specific errors.
- `src/workflow` parses and executes simple dependency-ordered plans.
- `src/agent`, `src/providers`, and `src/tools` provide useful but incomplete
  foundations for a reposcan-style agent layer.
- `src/repository` contains basic scanner and git operations.
- `src/xzepr` contains useful Kafka, CloudEvents, and API client code that can
  be refactored into generic watcher infrastructure.
- `docs/how_to` exists and must be renamed to `docs/how-to`.
- `README.md`, `config.example.yaml`, `sample_plan.yaml`, and multiple docs
  still describe documentation generation as the product purpose.

### Identified Issues

- Product identity is documentation-generator oriented.
- Chat is a first-class command but must be deleted.
- Doc Gen is embedded in CLI, config, workflow actions, errors, tests, docs, and
  public module exports.
- Current workflow execution has no persistent workspace state.
- Current scanning returns file paths rather than structured scan artifacts.
- There is no governance module.
- There is no generic provider authentication layer.
- OpenAI is not implemented as the primary provider.
- Provider APIs are narrower than reposcan's workflow needs.
- Tools are not sandboxed.
- Tool execution errors can abort sessions instead of becoming structured tool
  results.
- Prompting is not a plugin-level externalized template system.
- There is no `WorkflowPlugin` trait or plugin registry.
- There is no plugin findings, report envelope, risk band, or formatter
  infrastructure.
- There is no investigation batching module.
- Watcher code is not integrated with workflow/plugin execution.
- Kafka result publishing is absent.
- MCP client support is absent.
- Deployment artifacts do not yet reflect the new workflow harness.

## Reposcan Architecture Mapping

### Implement Directly

The first release should implement these reposcan architecture concepts
directly:

- Subcommand architecture.
- Workspace state persistence.
- Incremental pipeline stages.
- CLI argument structs and command routing.
- Configuration loading, field-level merging, and validation.
- Git operations.
- Repository scanner.
- Scanner common infrastructure.
- Governance rules.
- Provider abstraction.
- Authentication system.
- Prompt system with externalized templates.
- Workspace management.
- Agent session orchestration.
- Sandbox model.
- Tool executor trait and tool registry builders.
- File tools.
- Delegation or subagent configuration where practical.
- Workflow plugin trait.
- Plugin findings and report infrastructure.
- Investigation module.
- Watcher mode.
- CloudEvents-style task and result messages.
- Kafka task consumption.
- Kafka result publishing.
- MCP client layer.
- Unified error handling.
- Testing strategy and quality gates.
- Binary, Docker, and GitHub Action deployment paths.
- Batch processing for large investigations.
- CI/CD integration behavior.
- Component dependency rules.

### Implement as Generic Equivalents

Reposcan contains Doc Gen-specific concepts that should be implemented as
generic workflow harness equivalents:

- Documentation analyzer becomes generic review planning and plugin preparation.
- Documentation generator becomes generic report generation and plugin output
  generation.
- Documentation categories become plugin report sections and review scopes.
- Generation scores become plugin confidence scores, risk bands, and
  diagnostics.
- Generated files become plugin-written reports and artifacts.
- Generate stage becomes plugin execution stage.
- Doc Gen prompt templates become technical and security review prompt
  templates.
- Single-turn Doc Gen fallback becomes generic single-turn plugin fallback.

### Explicitly Excluded

These reposcan concepts are not implemented because they conflict with the new
product direction:

- Doc Gen plugin.
- Diataxis generation.
- Documentation category generation.
- Export Restrictions plugin.
- Export Restrictions scanner domain model.
- Export Restrictions config.
- Export Restrictions event constants.
- Export Restrictions scoring.

## Target First-Release Module Layout

### Top-Level Modules

- `src/auth`
- `src/cli`
- `src/commands`
- `src/config`
- `src/error.rs`
- `src/git`
- `src/governance`
- `src/scanner`
- `src/providers`
- `src/prompts`
- `src/agent`
- `src/tools`
- `src/plugins`
- `src/investigation`
- `src/reports`
- `src/workspace`
- `src/workflow`
- `src/watcher`
- `src/mcp`
- `src/telemetry.rs`

### Modules to Delete

- `src/docgen`
- `src/commands/chat.rs`
- `src/commands/generate.rs`

### Modules to Refactor or Move

- Move or replace `src/repository/scanner.rs` with `src/scanner`.
- Move or replace `src/repository/git.rs` with `src/git`.
- Refactor `src/xzepr` into `src/watcher`, or wrap the useful Kafka and
  CloudEvents components behind a generic watcher facade.
- Keep `src/providers`, but expand it for OpenAI-first multi-provider support.
- Keep `src/tools`, but sandbox and restructure it.

## Target First-Release CLI

### Required Commands

#### `xzardgz run`

Runs a local workflow plan or direct plugin invocation. It creates or resumes a
workspace, opens or clones a repository, scans the repository, executes a
plugin, writes reports, and prints a final result summary.

Required arguments and options:

- Repository path or URL.
- Target branch.
- Plugin name.
- Provider override.
- Model override.
- Dry-run flag.
- Workspace directory.
- Output directory.
- OpenAI-compatible API endpoint.
- Ollama host.
- Insecure endpoint opt-in.
- Config file path.
- Scan artifact override.
- Transcript tracing toggle.
- Maximum findings.
- Report formats.

#### `xzardgz scan`

Runs repository scanning only. It writes a structured scan artifact to a
workspace or configured output path. This command supports debugging, CI
preflight, plugin development, and watcher troubleshooting.

#### `xzardgz plugin`

Provides plugin operations:

- List available plugins.
- Show plugin configuration schema.
- Run a plugin against a workspace.
- Run a plugin against an existing scan artifact.
- Validate plugin configuration.
- Show plugin report formats.

First-release plugins are `technical-review` and `security-review`.

#### `xzardgz watch`

Starts watcher mode. It consumes Kafka task messages, validates matcher rules,
rejects all messages when matcher config is empty, routes valid tasks to the
workflow/plugin engine, persists workspace state, writes reports, and publishes
result messages to Kafka.

Required watcher options:

- Config file path.
- Provider override.
- Model override.
- Workspace directory.
- Kafka brokers override.
- Kafka input topic override.
- Kafka output topic override.
- Matcher config override.
- Dry-run flag.
- Once mode for tests and batch jobs.
- Maximum concurrent tasks.
- Result publishing toggle, default enabled.

#### `xzardgz auth`

Manages provider authentication for all supported providers. OpenAI is the
default provider when omitted.

Required subcommands:

- `login openai`
- `login anthropic`
- `login copilot`
- `login ollama`
- `logout <provider>`
- `status`
- `validate`
- `set-key <provider>`
- `remove-key <provider>`

#### `xzardgz prompts`

Manages prompt templates:

- Export built-in prompts.
- Validate configured prompt directories.
- Show prompt resolution order.
- Show plugin prompt template names.
- Render a prompt with test context for debugging.

#### `xzardgz mcp`

Manages MCP client configuration:

- Validate MCP server configuration.
- List configured MCP servers.
- List tools exposed by a server.
- Test tool discovery.
- Test tool invocation with safe sample input.

### Removed Commands

- `xzardgz chat`
- `xzardgz generate`

## Target First-Release Pipeline Stages

The first-release workspace stage model should include:

- `Created`
- `Pulled`
- `Scanned`
- `Analyzed`
- `PluginPrepared`
- `PluginRunning`
- `PluginCompleted`
- `Reported`
- `ResultPublished`
- `Completed`
- `Failed`

Reposcan stages that are PR-specific, such as `Generated`, `Committed`,
`Pushed`, and `PrCreated`, should not be copied literally unless the first
release later adds a PR-authoring workflow. Review/reporting stages are the
correct generic harness equivalent.

## Implementation Phases

## Phase 1: Remove Legacy Product Surface

### 1.1 Foundation Work

- Remove `Chat` from `src/cli.rs`.
- Remove chat routing from `src/main.rs`.
- Remove `pub mod chat` from `src/commands/mod.rs`.
- Delete `src/commands/chat.rs`.
- Remove all tests and docs for interactive chat.
- Ensure `xzardgz chat` is invalid.
- Remove `Generate` from `src/cli.rs`.
- Remove generate routing from `src/main.rs`.
- Remove `pub mod generate` from `src/commands/mod.rs`.
- Delete `src/commands/generate.rs`.
- Delete `src/docgen`.
- Remove `pub mod docgen` from `src/lib.rs`.
- Remove `DocGenError` and Doc Gen top-level error variants from `src/error.rs`.
- Delete or rewrite `tests/unit/docgen_tests.rs`.
- Remove Diataxis generation behavior.
- Remove documentation category types.
- Remove `GenerateDocumentation` and `generate_docs` workflow actions.
- Rename `docs/how_to` to `docs/how-to`.

### 1.2 Add Foundation Functionality

- Establish the new command surface around `run`, `scan`, `plugin`, `watch`,
  `auth`, `prompts`, and `mcp`.
- Rewrite the crate-level product description as a generic workflow harness.
- Define canonical plugin identifiers `technical-review` and `security-review`.
- Define canonical event identifiers for technical and security review tasks.
- Define common vocabulary for workflow, workspace, scan artifact, plugin,
  finding, report, watcher task, and watcher result.

### 1.3 Integrate Foundation Work

- Rewrite `README.md` for the workflow harness.
- Rewrite `docs/explanation/architecture.md` around the new architecture.
- Rewrite `docs/reference/cli.md` for the new command surface.
- Rewrite `docs/reference/configuration.md` for the new config model.
- Rewrite `docs/reference/workflow_format.md` for plugin-first workflows.
- Rewrite `config.example.yaml` using only first-release config sections.
- Rewrite `sample_plan.yaml` to run a plugin workflow.
- Remove compatibility language from docs.
- Remove emojis from touched docs.

### 1.4 Testing Requirements

- Test CLI rejects `chat`.
- Test CLI rejects `generate`.
- Test CLI accepts `run`, `scan`, `plugin`, `watch`, `auth`, `prompts`, and
  `mcp`.
- Test old Doc Gen workflow actions fail validation.
- Test no public module named `docgen` exists.
- Test docs and README links use `docs/how-to`.

### 1.5 Deliverables

- Legacy commands removed.
- Doc Gen removed.
- Product docs rewritten.
- Documentation directory normalized.
- New command surface documented.

### 1.6 Success Criteria

- No active code references to Doc Gen remain.
- No active code references to Chat remain.
- The project identity is workflow harness, not documentation generator.
- Old command and config users are intentionally broken.

## Phase 2: Error Handling and Diagnostics Foundation

### 2.1 Foundation Work

- Replace the current top-level error model with a reposcan-style
  `PipelineError`, or retain the current type name only if naming consistency is
  more important than direct architectural mirroring.
- Define `pub type Result<T> = std::result::Result<T, PipelineError>`.
- Convert external errors at module boundaries.
- Preserve meaningful context in every error.
- Avoid string-only errors where structured variants are possible.

### 2.2 Add Foundation Functionality

Error variants should include:

- Config.
- Git.
- Scanner.
- Provider.
- Auth.
- Governance.
- Agent.
- Tool.
- Plugin.
- Prompt.
- Report.
- Workspace.
- Workflow.
- Watcher.
- Kafka.
- MCP.
- MCP transport.
- MCP server not found.
- MCP tool not found.
- MCP protocol version mismatch.
- MCP timeout.
- MCP auth.
- MCP elicitation.
- MCP task.
- IO.
- Custom.

### 2.3 Diagnostics

- Add structured diagnostics for config warnings, scan warnings, plugin
  warnings, provider fallback warnings, watcher routing warnings, Kafka publish
  warnings, and MCP warnings.
- Persist diagnostics into workspace state.
- Include diagnostics in watcher result messages.
- Include diagnostics in JSON reports.

### 2.4 Testing Requirements

- Test error display strings.
- Test source error conversion.
- Test structured errors for missing plugin, missing MCP server, invalid config,
  invalid watcher matcher, and Kafka publish failure.
- Test diagnostics serialization and deserialization.
- Test diagnostics persistence in workspace state.

### 2.5 Deliverables

- Unified top-level error type.
- Unified `Result<T>` alias.
- Structured diagnostics model.

### 2.6 Success Criteria

- All modules use the unified error model.
- Errors contain enough context for CLI output and watcher result messages.
- Diagnostics are serializable and persisted.

## Phase 3: Configuration System

### 3.1 Foundation Work

- Implement compiled defaults.
- Implement config file loading from `.yaml` files.
- Implement field-level merging.
- Implement environment variable overrides.
- Implement CLI overrides.
- Implement strict validation.
- Reject legacy config fields.
- Do not support `.yml` examples or generated config files.

### 3.2 Top-Level Config Sections

The first-release `Config` should include:

- `provider`.
- `provider_defaults`.
- `openai`.
- `anthropic`.
- `ollama`.
- `copilot`.
- `scanner`.
- `git`.
- `workspace`.
- `plugins`.
- `technical_review`.
- `security_review`.
- `governance`.
- `watcher`.
- `kafka`.
- `topics`.
- `matcher`.
- `subagent`.
- `trace_transcript`.
- `scan_output`.
- `reports`.
- `mcp`.
- `model_metadata`.
- `model_selection`.
- `project`.

Do not include `documentation` or `export_scan`.

### 3.3 Provider Defaults

- Default provider is OpenAI.
- OpenAI supports API key configuration, API endpoint override, model,
  temperature, max tokens, retry count, timeout, and insecure endpoint opt-in.
- Anthropic supports equivalent provider settings where applicable.
- Ollama supports host, model, context length, timeout, and temperature.
- Copilot supports existing OAuth/keychain behavior if retained.
- OpenAI-compatible custom endpoints are supported through the OpenAI provider
  config, guarded by endpoint security checks.

### 3.4 Model Selection Configuration

The first-release config must include a `model_selection` section with these
fields and defaults:

- `enabled`: boolean, default `true`.
- `auto_fallback`: boolean, default `true`.
- `require_tools`: boolean, default `true`.
- `require_structured_output`: boolean, default `true`.
- `min_context_tokens`: integer, default `16000`.
- `allow_degraded_metadata`: boolean, default `true`.
- `preferred_models`: ordered list of model names, default provider-specific.
- `fallback_models`: ordered list of model names, default provider-specific.

Model selection config must be merged with provider defaults, plugin-specific
model overrides, workflow plan overrides, watcher task overrides, and CLI
overrides. The resolved provider, resolved model, selected fallback, thinking
mode decision, and model capability diagnostics must be persisted in workspace
state, reports, and watcher result messages.

### 3.5 Plugin Configuration

- `plugins.enabled` lists enabled plugin identifiers.
- `technical_review` config controls technical review behavior.
- `security_review` config controls security review behavior.
- Plugin-specific config can be overridden in workflow plans and watcher task
  messages.
- Event-provided plugin config must pass strict validation before execution.

### 3.6 Watcher and Kafka Configuration

- `watcher.enabled` controls watcher mode availability.
- `watcher.max_concurrent_tasks` controls concurrency.
- `watcher.result_publish_enabled` defaults to true.
- `kafka` controls brokers, group ID, security protocol, SASL, SSL, timeout, and
  offset behavior.
- `topics.task` controls the input topic.
- `topics.result` controls the result topic.
- `matcher` controls allowed event types, repositories, packages, platforms,
  plugins, and metadata filters.
- Empty matcher config rejects all messages.

### 3.7 MCP Configuration

- `mcp.servers` defines server name, command, args, environment, timeout,
  transport, allowed tools, and auth settings.
- MCP tools must be explicitly allowed before plugin use.
- MCP timeouts must be bounded.

### 3.8 Testing Requirements

- Test default config uses OpenAI.
- Test default model selection is enabled.
- Test `model_selection.auto_fallback` defaults to true.
- Test `model_selection.require_tools` defaults to true.
- Test `model_selection.require_structured_output` defaults to true.
- Test `model_selection.min_context_tokens` defaults to `16000`.
- Test config file loading.
- Test field-level merging.
- Test environment overrides.
- Test CLI overrides.
- Test strict rejection of legacy `documentation` and `export_scan` sections.
- Test endpoint security validation.
- Test empty matcher rejects all.
- Test plugin config validation.
- Test model selection config validation.
- Test model selection override merging.
- Test Kafka config validation.
- Test MCP config validation.

### 3.9 Deliverables

- Generic first-release config model.
- Strict validation.
- Model selection config schema.
- Updated `config.example.yaml`.
- Updated configuration reference docs.

### 3.10 Success Criteria

- A complete first-release config can drive local and watcher workflows.
- Invalid legacy config fails fast with actionable errors.
- OpenAI is the default provider path.
- Model selection config can resolve defaults, preferred models, and fallback
  models without implementation-specific assumptions.

## Phase 4: CLI, Command Routing, and Workflow Model

### 4.1 Foundation Work

- Restructure `src/cli.rs` into a reposcan-style CLI module or submodule tree if
  needed.
- Define argument structs for `run`, `scan`, `plugin`, `watch`, `auth`,
  `prompts`, and `mcp`.
- Keep command routing thin in `src/main.rs`.
- Move command behavior into `src/commands` handlers.

### 4.2 Workflow Plan Model

- Replace the current Doc Gen-oriented plan action enum.
- Support workflow metadata, repository target, target branch, workspace,
  provider override, model override, scan options, plugin steps, plugin config,
  report formats, and dry-run behavior.
- Support dependency ordering where useful.
- Support incremental execution from existing workspace state.
- Support direct plugin execution from CLI without a plan file.
- Reject old actions without migration.

### 4.3 Command Behavior

- `run` loads config, applies CLI overrides, initializes workspace, opens or
  clones repository, scans, runs plugin, writes reports, and optionally exits
  nonzero on configured findings.
- `scan` runs scanner only and writes scan artifacts.
- `plugin list` prints built-in plugin metadata.
- `plugin schema` prints plugin config schema.
- `plugin run` runs a plugin from a workspace or scan artifact.
- `watch` starts watcher processing and publishes results.
- `auth` manages provider credentials.
- `prompts` manages prompt templates.
- `mcp` validates MCP configuration and discovery.

### 4.4 Testing Requirements

- Test CLI parsing for all commands.
- Test command routing.
- Test workflow validation.
- Test direct plugin invocation validation.
- Test old workflow action rejection.
- Test CLI override application.

### 4.5 Deliverables

- Complete first-release CLI surface.
- Plugin-first workflow model.
- Command handlers for all first-release commands.

### 4.6 Success Criteria

- CLI matches the first-release command design.
- Workflow plans are plugin-first and no legacy actions are accepted.
- Command handlers are thin orchestration layers over reusable modules.

## Phase 5: Workspace Management

### 5.1 Foundation Work

- Add `src/workspace`.
- Implement workspace directory creation.
- Implement deterministic repository hashing.
- Implement workspace ID creation using ULID or ULID-compatible identifiers.
- Implement RFC 3339 timestamps.
- Implement workspace state load/save.
- Implement idempotent stage transitions.
- Implement artifact path helpers.

### 5.2 Workspace State

`WorkspaceState` should include:

- State version.
- Workspace ID.
- Repository URL.
- Repository hash.
- Local repository path.
- Branch name.
- Target branch.
- Current stage.
- Scan result summary or path.
- Plugin outputs.
- Written files.
- Stage timestamps.
- Created timestamp.
- Updated timestamp.
- Report paths.
- Plugin scores.
- Plugin diagnostics.
- Scan artifact path.
- Scan artifact version.
- Scan artifact created timestamp.
- Scan artifact head commit.
- Watcher task ID when applicable.
- Watcher result publish status when applicable.

### 5.3 Directory Layout

Each workspace should have predictable locations for:

- State file.
- Repository checkout or local repository reference metadata.
- Scan artifacts.
- Plugin intermediate data.
- Reports.
- Transcripts.
- Diagnostics.
- Watcher task and result snapshots.

### 5.4 Idempotency Rules

- Loading an existing workspace must not destroy state.
- Re-running scan can update scan artifact metadata.
- Re-running a plugin can replace or version plugin output according to config.
- Failed stages must preserve diagnostics.
- Result publishing must be retryable without rerunning the entire plugin if
  reports already exist.

### 5.5 Testing Requirements

- Test workspace creation.
- Test state save and load.
- Test stage transitions.
- Test idempotent reload.
- Test artifact path creation.
- Test failed-stage persistence.
- Test result-publish retry state.
- Test RFC 3339 timestamp serialization.

### 5.6 Deliverables

- Workspace module.
- Workspace state model.
- Artifact path management.
- Idempotent resume behavior.

### 5.7 Success Criteria

- Local and watcher workflows use the same workspace state machinery.
- Workflows can resume safely after scan, plugin, report, or publish failures.

## Phase 6: Git Operations

### 6.1 Foundation Work

- Add `src/git` or refactor existing repository git code into it.
- Support local repository detection.
- Support remote clone.
- Support target branch checkout.
- Support current branch detection.
- Support head commit detection.
- Support repository URL normalization.
- Support clean/dirty status reporting.

### 6.2 Governance Integration

- Validate branch names through governance rules.
- Validate repository paths before use.
- Validate remote URLs where applicable.
- Avoid destructive git operations unless explicitly configured.

### 6.3 Testing Requirements

- Test local repository opening.
- Test non-repository path errors.
- Test branch validation.
- Test head commit detection.
- Test dirty status detection.
- Test repository URL hashing.

### 6.4 Deliverables

- Git module.
- Git metadata feeding workspace and scan artifacts.

### 6.5 Success Criteria

- Workflows can operate on local and remote repositories.
- Scan artifacts include meaningful git metadata.

## Phase 7: Repository Scanner and Scanner Common Infrastructure

### 7.1 Foundation Work

- Add `src/scanner`.
- Ensure scanner has no AI dependency.
- Respect `.gitignore` by default.
- Support configured exclusions.
- Support max file size limits.
- Support hidden file handling through config.
- Support binary file detection.
- Support deterministic traversal order.

### 7.2 Scan Result Model

`ScanResult` should include:

- Repository structure.
- Language statistics.
- Primary language.
- Frameworks.
- Documentation inventory.
- Governance rules discovered in repo.
- CLI commands.
- Public APIs.
- Entrypoints.
- Config surface.
- Key files.
- Dependency manifests.
- Test files.
- Build files.
- Security-relevant files.
- Schema version.
- Repository URL.
- Repository name.
- Head commit.
- Scan timestamp.

### 7.3 Scanner Common Infrastructure

Implement generic scanner infrastructure from reposcan:

- `PatternSet` with keywords, dependencies, and file names.
- `PatternRegistry` for scanner/plugin pattern lookup.
- `FindingSeverity` for pre-AI scan findings.
- `ScanFinding` with kind, file, line, evidence, and severity.
- `PluginContentScanner` for plugin-specific content preselection.
- `CrossCutHook` for reusable scan hooks.
- `ScoringSignal`.
- `ScoringInput`.
- `ConfidenceScorer`.
- Parallel file scanning with bounded concurrency.

### 7.4 Plugin-Oriented Preselection

Scanner output should help plugins prioritize:

- Entrypoints.
- Public APIs.
- Configuration surfaces.
- Dependency manifests.
- Files with risky patterns.
- Files with secrets-like patterns.
- Files with unsafe Rust.
- Files with command execution.
- Files with network clients.
- Files with authentication or authorization logic.
- Tests and missing-test signals.

### 7.5 Testing Requirements

- Test empty repository scanning.
- Test Rust repository scanning.
- Test mixed-language scanning.
- Test ignored files.
- Test max file size filtering.
- Test binary skipping.
- Test language statistics.
- Test framework detection.
- Test CLI command detection.
- Test public API detection.
- Test entrypoint detection.
- Test config surface detection.
- Test dependency manifest detection.
- Test security preselection patterns.
- Test deterministic output ordering.

### 7.6 Deliverables

- Structured scanner module.
- Versioned scan artifact.
- Common scanner infrastructure.
- Plugin preselection metadata.

### 7.7 Success Criteria

- Scanner output is deterministic, versioned, and independent of AI providers.
- Plugins can run from scan artifacts without re-scanning when appropriate.

## Phase 8: Governance System

### 8.1 Foundation Work

- Add `src/governance`.
- Implement governance rule sources:
  - Repository file.
  - Embedded defaults.
  - Derived from repository.
- Implement enforcement levels:
  - Required.
  - Recommended.
  - Optional.

### 8.2 Validation Functions

Governance should validate:

- Branch names.
- File paths.
- Output paths.
- Report paths.
- Workspace paths.
- Content safety rules.
- Plugin names.
- Event types.
- Provider endpoint security.

### 8.3 Integration

- Use governance in CLI validation.
- Use governance in workflow validation.
- Use governance in workspace and report path validation.
- Use governance in watcher event validation.
- Include governance results in scan artifacts and diagnostics.

### 8.4 Testing Requirements

- Test embedded default rules.
- Test repository rule loading.
- Test branch validation.
- Test path validation.
- Test event type validation.
- Test provider endpoint validation.
- Test required rule failures stop execution.
- Test recommended rule warnings become diagnostics.

### 8.5 Deliverables

- Governance module.
- Validation APIs.
- Governance diagnostics integration.

### 8.6 Success Criteria

- Unsafe or invalid workflow inputs fail before plugin execution.
- Governance diagnostics are visible in CLI, workspace, reports, and watcher
  results.

## Phase 9: Provider Abstraction and Authentication

### 9.1 Foundation Work

- Expand `src/providers` into a reposcan-style provider abstraction.
- Add OpenAI provider as the primary/default provider.
- Add Anthropic provider if not already present.
- Keep or refactor Ollama provider.
- Keep or refactor Copilot provider if viable.
- Support OpenAI-compatible endpoints.
- Enforce endpoint security by default.

### 9.2 Provider Trait

The provider trait should support:

- Provider name.
- Credential validation.
- Authentication status.
- Tool-enabled chat/completion.
- Generic structured generation for plugin outputs.
- Model metadata retrieval.
- Thinking support detection.
- Streaming where supported.
- Retry and timeout behavior.

Do not add Doc Gen-specific methods. Reposcan's documentation-specific provider
methods should be replaced with generic plugin planning and report-generation
methods or implemented as prompts over `chat_with_tools`.

### 9.3 Thinking Mode

Implement a `ThinkingMode` enum with:

- None.
- Auto.
- Low.
- Medium.
- High.
- Extra high.

Providers that do not support thinking should report unsupported behavior
without failing normal workflow execution.

`ThinkingMode::Auto` must use provider and model capability metadata to decide
whether thinking should be requested for a specific execution. The decision
rules are:

- `ThinkingMode::None` never requests thinking.
- `ThinkingMode::Auto` requests provider-appropriate default thinking only when
  the selected provider and selected model report thinking support.
- `ThinkingMode::Auto` disables thinking and records a diagnostic when the
  selected provider or selected model does not support thinking.
- Explicit `Low`, `Medium`, `High`, or `ExtraHigh` thinking requires provider
  and model support unless degraded execution is enabled.
- Explicit unsupported thinking fails fast when degraded execution is disabled.
- Missing remote model metadata falls back to static metadata when available.
- Missing remote and static metadata disables thinking, records a diagnostic,
  and continues only when degraded metadata is allowed.

### 9.4 Model Capability Resolution

Add a `ModelResolver` service in `src/providers` or a dedicated
`src/providers/model_resolution` module. The resolver must run before scanner
AI-assisted analysis, plugin execution, report generation, and watcher task
execution.

Provider resolution precedence is:

1. CLI `--provider`.
2. Watcher task `provider`.
3. Workflow plan provider.
4. Plugin-specific provider override.
5. Config `provider.default`.
6. OpenAI.

Model resolution precedence is:

1. CLI `--model`.
2. Watcher task `model`.
3. Workflow plan model.
4. Plugin-specific model override.
5. Provider-specific config default model.
6. Global provider default model.
7. First available compatible model from provider metadata.

The resolver must validate the selected model against required execution
capabilities:

- Tool calling when `model_selection.require_tools` is true.
- Structured JSON output when `model_selection.require_structured_output` is
  true.
- Minimum context length from `model_selection.min_context_tokens`.
- Thinking support when explicit thinking is requested.
- Streaming support only when streaming is required by the caller.

Provider-specific availability checks are:

- OpenAI: query the OpenAI-compatible models endpoint when credentials and
  endpoint allow it; otherwise validate against static OpenAI model metadata.
- Anthropic: query available models when supported; otherwise validate against
  static Anthropic model metadata.
- Ollama: query the local `/api/tags` endpoint.
- Copilot: validate against static Copilot model metadata and authentication
  status.

Unavailable model behavior is:

- If the requested model is available and compatible, use it.
- If the requested model is unavailable and `model_selection.auto_fallback` is
  true, select the first compatible model from the provider's fallback list or
  available model metadata.
- If the requested model is unavailable and `model_selection.auto_fallback` is
  false, return a provider/model validation error.
- If model metadata is unavailable and `model_selection.allow_degraded_metadata`
  is true, use static metadata, continue execution, and record a diagnostic.
- If model metadata is unavailable and `model_selection.allow_degraded_metadata`
  is false, return a provider/model validation error.

The resolved model record must include:

- Requested provider.
- Selected provider.
- Requested model.
- Selected model.
- Whether fallback was used.
- Fallback reason.
- Capability metadata.
- Thinking mode requested.
- Thinking mode selected.
- Metadata source, such as remote, static, or degraded.
- Diagnostics.

Persist the resolved model record in:

- Workspace state.
- Report envelope.
- Watcher result message.
- Transcript metadata when transcript tracing is enabled.

### 9.5 Authentication System

Add `src/auth` for all provider auth flows:

- OpenAI API key storage, validation, status, removal, and environment fallback.
- Anthropic API key storage, validation, status, removal, and environment
  fallback.
- Ollama host validation and model availability checks.
- Copilot OAuth/keychain support if retained.
- Keychain integration for secrets where available.
- Secure fallback to environment variables.
- No hardcoded credentials.
- Redacted output in logs and diagnostics.

### 9.6 OpenAI Focus

- `provider.default` should be `openai`.
- `auth status` should show OpenAI first.
- `auth validate` should validate OpenAI first unless a provider is specified.
- Docs and examples should use OpenAI by default.
- Watcher examples should use OpenAI unless overridden.

### 9.7 Testing Requirements

- Test provider factory defaults to OpenAI.
- Test OpenAI provider config.
- Test OpenAI-compatible endpoint validation.
- Test insecure endpoint rejection by default.
- Test insecure endpoint opt-in.
- Test auth status with env var credentials.
- Test key storage abstraction with mocks.
- Test Ollama unauthenticated local behavior.
- Test Anthropic and Copilot validation paths with mocks.
- Test provider metadata retrieval with mocks.
- Test CLI model override wins over config default.
- Test watcher task model override wins over config default.
- Test workflow plan model override wins over plugin and config defaults.
- Test plugin model override is used when no CLI, watcher, or workflow override
  exists.
- Test configured default model is used when available.
- Test fallback model is selected when the requested model is unavailable and
  auto fallback is enabled.
- Test provider/model validation error when the requested model is unavailable
  and auto fallback is disabled.
- Test `ThinkingMode::Auto` enables thinking for a thinking-capable model.
- Test `ThinkingMode::Auto` disables thinking for a non-thinking model and
  records a diagnostic.
- Test explicit `ThinkingMode::High` fails when unsupported and degraded
  execution is disabled.
- Test explicit `ThinkingMode::High` degrades with diagnostics when unsupported
  and degraded execution is enabled.
- Test Ollama model availability through `/api/tags` with a mock server.
- Test OpenAI model availability through the OpenAI-compatible models endpoint
  with a mock server.
- Test static metadata fallback when remote metadata is unavailable and degraded
  metadata is allowed.
- Test resolved model metadata appears in workspace state, report envelope, and
  watcher result messages.
- Test thinking support detection.
- Test tool-enabled completion with mock providers.

### 9.8 Deliverables

- OpenAI-first provider stack.
- Multi-provider auth system.
- Model capability resolver.
- Automatic model selection and fallback.
- Thinking mode auto detection.
- Provider metadata and thinking support.
- Secure endpoint validation.

### 9.9 Success Criteria

- OpenAI is the default path for local workflows and watcher tasks.
- All supported providers have auth status and validation flows.
- Provider APIs support plugin execution and tool calls.
- Model resolution selects a compatible available model before plugin execution.
- Thinking mode auto detection selects supported thinking behavior or records a
  deterministic diagnostic when thinking is unavailable.

## Phase 10: Prompt System

### 10.1 Foundation Work

- Add `src/prompts`.
- Use externalized prompt templates.
- Prefer Tera for template rendering to align with reposcan.
- Support embedded fallback prompt assets.
- Support configured prompt directories.
- Support prompt export command.

### 10.2 Resolution Order

Prompt resolution order should be:

1. Plugin-specific configured prompt directory.
2. Global configured prompt directory.
3. Built-in embedded prompt assets.
4. Explicit missing-template error.

### 10.3 Template Layout

Each plugin should support prompt templates for:

- System prompt.
- Task prompt.
- Batch investigation prompt.
- Findings normalization prompt.
- Report summary prompt.
- Verification prompt.

### 10.4 Integration

- `technical-review` uses technical review prompts.
- `security-review` uses security review prompts.
- MCP-enabled tools can be described in prompts.
- Prompt rendering should include scan artifact summaries, plugin config,
  workspace metadata, and selected files.
- Prompt transcripts can be persisted when trace config is enabled.

### 10.5 Testing Requirements

- Test configured prompt directory resolution.
- Test embedded fallback resolution.
- Test missing-template errors.
- Test template rendering.
- Test prompt export command.
- Test prompt validation command.

### 10.6 Deliverables

- Prompt loader.
- Prompt renderer.
- Built-in plugin prompts.
- Prompt CLI support.

### 10.7 Success Criteria

- Plugins do not hardcode large prompts in Rust logic.
- Users can export and override built-in prompts.

## Phase 11: Agent Layer, Sandbox, and Tools

### 11.1 Agent Session

- Refactor `src/agent` around `AgentSession`.
- Define `AgentContext` with messages, workspace metadata, scan artifact
  references, plugin metadata, provider metadata, and trace settings.
- Support bounded max turns.
- Support generic single-turn fallback for providers without tool support.
- Support transcript persistence when enabled.

### 11.2 Tool Executor Trait

- Define a `ToolExecutor` trait with `tool_definition` and async `execute`.
- Ensure tool definitions are serializable for providers.
- Ensure tool execution returns structured tool results.
- Ensure tool errors do not abort agent sessions.
- Add diagnostics for repeated tool failures.

### 11.3 Sandbox Model

- Add `PathValidator`.
- Define read zones and write zones.
- Default review plugin access is read-only.
- Write access is limited to workspace/report directories.
- Reject path traversal.
- Reject unauthorized absolute paths.
- Handle symlinks safely.
- Validate paths before all file operations.

### 11.4 File Tools

Read-only registry tools should include:

- Read file.
- List directory.
- Search file contents.
- Find files by glob.
- Read scan artifact.
- Read workspace metadata.

Read-write registry tools should add:

- Write file.
- Create directory.
- Write report artifact.
- Append diagnostic.

Subagent registry tools should add delegation only if subagent support is
included in the first release.

### 11.5 Registry Builders

Implement registry builder functions:

- Build read-only registry.
- Build read-write registry.
- Build subagent registry.
- Build MCP-augmented registry.

### 11.6 Testing Requirements

- Test agent max-turn behavior.
- Test tool call dispatch.
- Test tool errors are returned to the model instead of aborting.
- Test read-only registry rejects writes.
- Test path traversal rejection.
- Test absolute path rejection outside allowlist.
- Test symlink handling.
- Test transcript persistence.
- Test single-turn fallback.

### 11.7 Deliverables

- Reposcan-style agent session.
- Sandboxed tools.
- Tool registry builders.
- Safe file tools.

### 11.8 Success Criteria

- Plugins can use tools safely.
- Tool failures are observable and non-fatal to the agent loop.
- Review plugins cannot modify repositories by default.

## Phase 12: MCP Client Layer

### 12.1 Foundation Work

- Add `src/mcp`.
- Implement MCP server configuration parsing.
- Implement transport initialization.
- Implement protocol version negotiation.
- Implement tool discovery.
- Implement tool invocation.
- Implement timeout handling.
- Implement auth handling where configured.

### 12.2 Integration

- MCP tools can be added to tool registries when explicitly configured.
- Plugins can opt into configured MCP tools.
- MCP failures should produce diagnostics and structured errors.
- `xzardgz mcp` can validate configuration and list tools.
- Watcher tasks can reference allowed MCP tool profiles only if enabled in
  config.

### 12.3 Testing Requirements

- Test MCP config validation.
- Test missing server errors.
- Test missing tool errors.
- Test protocol version mismatch.
- Test timeout behavior.
- Test auth error handling.
- Test mock tool discovery.
- Test mock tool invocation.

### 12.4 Deliverables

- MCP client module.
- MCP CLI command.
- MCP tool registry integration.

### 12.5 Success Criteria

- MCP is available as a first-release extension point without weakening sandbox
  rules.

## Phase 13: Plugin Runtime, Reports, and Investigation Module

### 13.1 Workflow Plugin Trait

Add `src/plugins` with a `WorkflowPlugin` trait supporting:

- Plugin name.
- Plugin metadata.
- Config schema or config description.
- Async run method.
- Supported report formats.
- Required tool access level.

### 13.2 Plugin Context

`PluginContext` should include:

- Effective config.
- Workspace manager.
- Workspace state.
- Scan result.
- Provider.
- Prompt loader.
- Tool registry.
- Report writer.
- Governance results.
- Diagnostics collector.
- Optional watcher task metadata.

### 13.3 Plugin Output

`PluginOutput` should include:

- Summary.
- Written files.
- Findings.
- Completed flag.
- Diagnostics.
- Scores.
- Risk band.
- Report paths.
- Provider metadata.
- Token usage if available.

### 13.4 Plugin Registry

- Register built-in plugins.
- Reject unknown plugins.
- Reject disabled plugins.
- Support plugin metadata listing.
- Support plugin config validation.
- Support workflow and watcher dispatch through the same registry.

### 13.5 Report Infrastructure

Add `src/reports` with:

- `PluginFindings`.
- `ReportEnvelope`.
- `RiskBand`.
- `PluginReportFormatter`.
- Markdown report writer.
- JSON report writer.
- SARIF report writer for security review.
- Report path validation.
- Report metadata stamping.

Report envelopes should include:

- Report ID.
- Generated timestamp.
- Plugin name.
- Repository name.
- Repository URL.
- Head commit.
- Workspace ID.
- Scan artifact version.
- Provider metadata.
- Model metadata.
- Findings.
- Diagnostics.

### 13.6 Investigation Module

Add `src/investigation` with:

- `FileMatchEntry`.
- `InvestigationScope`.
- `InvestigationStrategy`.
- Single-session strategy.
- Batched-session strategy.
- Batch count and batch size configuration.
- Clean-verification turns.
- File match map splitting.
- Investigation turn computation.

### 13.7 Scoring

- Use scanner signals and AI confidence to compute finding confidence.
- Compute plugin-level risk bands.
- Persist scores to workspace state.
- Include scores in JSON reports and watcher results.

### 13.8 Testing Requirements

- Test plugin registry known plugin dispatch.
- Test plugin registry unknown plugin rejection.
- Test disabled plugin rejection.
- Test plugin output serialization.
- Test report envelope serialization.
- Test Markdown report rendering.
- Test JSON report rendering.
- Test SARIF writer with security findings.
- Test investigation strategy selection.
- Test file match batching.
- Test confidence scoring.
- Test risk band calculation.

### 13.9 Deliverables

- Plugin runtime.
- Plugin registry.
- Report infrastructure.
- Investigation module.
- Scoring support.

### 13.10 Success Criteria

- A mock plugin can run from local and watcher execution paths.
- Plugin reports are versioned, validated, and persisted.
- Large repositories can be investigated in bounded batches.

## Phase 14: Watcher Mode and Kafka Result Publishing

### 14.1 Foundation Work

- Add `src/watcher`.
- Refactor useful `src/xzepr` code into generic watcher modules.
- Implement Kafka consumer for task messages.
- Implement Kafka producer for result messages.
- Implement CloudEvents-style task and result envelopes.
- Implement matcher rules.
- Implement reject-by-default behavior for empty matcher config.
- Implement concurrency limits.
- Implement once mode for tests and batch jobs.

### 14.2 Watcher Task Message

Watcher task data should include:

- Repository.
- Target branch.
- Stage.
- Provider.
- Model.
- Plugin.
- Plugin config.
- Dry run.
- Workspace directory.
- Metadata.
- Requested report formats.
- Correlation ID.
- Reply topic override if allowed.

### 14.3 Watcher Result Message

Watcher result data should include:

- Success flag.
- Errors.
- Diagnostics.
- Repository.
- Target branch.
- Plugin.
- Workspace ID.
- Workspace path.
- Scan artifact path.
- Report paths.
- Findings summary.
- Risk band.
- SARIF path for security review when generated.
- Provider metadata.
- Model metadata.
- Started timestamp.
- Completed timestamp.
- Correlation ID.
- Original task ID.

### 14.4 Event Types

First-release event types should include:

- Technical review task.
- Security review task.
- Technical review result.
- Security review result.

Do not add Doc Gen, Export Restrictions, or Copyright Headers event types unless
those plugins are actually implemented.

### 14.5 Kafka Publishing

- Result publishing is enabled by default.
- Publish success and failure results.
- Include diagnostics in failure results.
- If publishing fails after successful plugin execution, persist publish failure
  state and allow retry.
- Do not rerun expensive plugin work solely to retry publishing.
- Support producer security settings from the shared Kafka config.

### 14.6 Watcher Execution Flow

- Consume task message.
- Deserialize CloudEvent envelope.
- Apply matcher.
- Reject unknown event types.
- Reject unknown plugins.
- Validate plugin config.
- Create or resume workspace.
- Pull or open repository.
- Scan repository or load scan artifact.
- Execute plugin through shared plugin registry.
- Write reports.
- Update workspace state.
- Publish result message to Kafka.
- Commit consumed message according to configured semantics.

### 14.7 Testing Requirements

- Test task deserialization.
- Test result serialization.
- Test empty matcher rejects all.
- Test matcher allows configured technical review event.
- Test matcher allows configured security review event.
- Test unknown plugin rejection.
- Test invalid plugin config rejection.
- Test successful result publishing with mock producer.
- Test failure result publishing with mock producer.
- Test publish failure state persistence.
- Test once mode.
- Test concurrency limits.
- Test consumer offset behavior through mocks where possible.

### 14.8 Deliverables

- Required watcher mode.
- Kafka task consumer.
- Kafka result producer.
- CloudEvents task and result models.
- Safe matcher.
- Shared plugin dispatch from watcher.

### 14.9 Success Criteria

- `xzardgz watch` can process technical and security review tasks.
- All successful and failed watcher executions publish result messages to Kafka
  unless explicitly disabled.
- Empty matcher config processes no events.

## Phase 15: Technical Review Plugin

### 15.1 Feature Work

- Add `src/plugins/technical_review`.
- Implement `TechnicalReviewPlugin`.
- Implement `TechnicalReviewConfig`.
- Implement `TechnicalReviewFinding`.
- Implement technical review report formatting.
- Implement technical review prompt templates.

### 15.2 Configuration

`TechnicalReviewConfig` should include:

- Enabled flag.
- Prompt directory.
- Output formats.
- Maximum files.
- Maximum findings.
- Severity threshold.
- Include tests flag.
- Include docs flag.
- Focus areas.
- Batch size.
- Model override.
- Verification turns.
- Confidence threshold.

### 15.3 Finding Model

Technical review findings should include:

- Category.
- Severity.
- File.
- Line.
- Symbol.
- Evidence.
- Impact.
- Recommendation.
- Confidence.
- Related files.
- References where applicable.

### 15.4 Review Dimensions

Technical review should evaluate:

- Architecture.
- Modularity.
- Maintainability.
- Error handling.
- Testing posture.
- Dependency hygiene.
- CLI usability.
- API usability.
- Configuration ergonomics.
- Observability.
- Documentation coverage.
- Performance risks.
- Build and release hygiene.
- Operational readiness.

### 15.5 Scanner Integration

Use `ScanResult` to prioritize:

- Entrypoints.
- Public APIs.
- Config surfaces.
- Key files.
- Tests.
- Build files.
- Dependency manifests.
- Documentation files.
- Files with high fan-in signals where detectable.

### 15.6 Reports

Technical review writes:

- `technical_review.md`.
- `technical_review.json`.

The Markdown report should be human-readable. The JSON report should use the
shared report envelope.

### 15.7 Watcher Integration

- Technical review watcher tasks route to `technical-review`.
- Technical review result messages include findings summary, report paths, risk
  band, diagnostics, workspace ID, and provider metadata.
- Local and watcher execution use identical plugin code paths.

### 15.8 Testing Requirements

- Test config defaults.
- Test config validation.
- Test finding serialization.
- Test Markdown rendering.
- Test JSON rendering.
- Test file prioritization.
- Test empty repository behavior.
- Test oversized repository batching.
- Test mock-provider execution.
- Test local workflow artifact generation.
- Test watcher task execution.
- Test Kafka result content for technical review.

### 15.9 Deliverables

- Built-in `technical-review` plugin.
- Technical review prompts.
- Technical review findings model.
- Markdown and JSON report support.
- Local workflow examples.
- Watcher event examples.
- Reference docs.

### 15.10 Success Criteria

- `xzardgz run` can execute `technical-review`.
- `xzardgz watch` can execute `technical-review` tasks and publish results.
- Technical review runs read-only against the repository.
- Reports contain evidence, impact, and recommendations.

## Phase 16: Security Review Plugin with SARIF

### 16.1 Feature Work

- Add `src/plugins/security_review`.
- Implement `SecurityReviewPlugin`.
- Implement `SecurityReviewConfig`.
- Implement `SecurityReviewFinding`.
- Implement security review report formatting.
- Implement security review prompt templates.
- Implement SARIF conversion.

### 16.2 Configuration

`SecurityReviewConfig` should include:

- Enabled flag.
- Prompt directory.
- Output formats.
- Maximum findings.
- Severity threshold.
- Secret scanning settings.
- Dependency scanning settings.
- Unsafe-code checks.
- Auth checks.
- Endpoint checks.
- Command execution checks.
- Deserialization checks.
- Cryptography checks.
- Fail-on-critical flag.
- Batch size.
- Model override.
- Verification turns.
- Confidence threshold.
- SARIF output flag, default enabled when security review runs in CI mode.

### 16.3 Finding Model

Security review findings should include:

- Category.
- Severity.
- CWE mapping where applicable.
- OWASP mapping where applicable.
- File.
- Line.
- Symbol.
- Evidence.
- Exploitability.
- Impact.
- Remediation.
- Confidence.
- False-positive notes.
- SARIF rule ID.
- SARIF help URI where applicable.

### 16.4 Security Scope

Security review should inspect and prioritize:

- Potential secrets.
- Dependency manifests.
- Authentication code.
- Authorization code.
- Request handlers.
- Route definitions.
- File operations.
- Command execution.
- Deserialization.
- Cryptography usage.
- Unsafe Rust.
- Environment variable handling.
- Network clients.
- Hardcoded endpoints.
- Logging of sensitive data.
- Error messages that leak sensitive details.
- TLS and certificate handling.
- Input validation boundaries.
- Output sanitization boundaries.

### 16.5 Secret Handling

- Do not dump full secret values into reports.
- Redact secret evidence.
- Include enough evidence to locate the issue.
- Include confidence and false-positive guidance.
- Ensure Kafka result messages never include raw secrets.

### 16.6 Reports

Security review writes:

- `security_review.md`.
- `security_review.json`.
- `security_review.sarif`.

SARIF support should include:

- SARIF version.
- Tool metadata.
- Rules.
- Results.
- Locations.
- Severity mapping.
- Help text.
- Fingerprints where practical.

### 16.7 CI Behavior

- `fail_on_critical` causes nonzero exit for local/CI execution when critical
  findings are present.
- Watcher mode should publish failure status according to config while still
  publishing report paths and findings summaries.
- Severity thresholds control which findings are included and which fail the
  run.

### 16.8 Watcher Integration

- Security review watcher tasks route to `security-review`.
- Security review result messages include findings summary, report paths, SARIF
  path, risk band, diagnostics, workspace ID, and provider metadata.
- Local and watcher execution use identical plugin code paths.

### 16.9 Testing Requirements

- Test config defaults.
- Test config validation.
- Test static secret pattern matching.
- Test secret redaction.
- Test dependency manifest detection.
- Test unsafe Rust detection.
- Test command execution detection.
- Test auth-related preselection.
- Test finding serialization.
- Test Markdown rendering.
- Test JSON rendering.
- Test SARIF rendering.
- Test SARIF schema shape.
- Test mock-provider execution.
- Test local workflow artifact generation.
- Test watcher task execution.
- Test Kafka result content for security review.
- Test `fail_on_critical` behavior.
- Test Export Restrictions names and config are absent.

### 16.10 Deliverables

- Built-in `security-review` plugin.
- Security review prompts.
- Security findings model.
- Markdown, JSON, and SARIF report support.
- Local workflow examples.
- Watcher event examples.
- Reference docs.

### 16.11 Success Criteria

- `xzardgz run` can execute `security-review`.
- `xzardgz watch` can execute `security-review` tasks and publish results.
- Security review runs read-only against the repository.
- Security review produces valid SARIF.
- Critical findings can fail CI when configured.

## Phase 17: Workflow Executor Integration

### 17.1 Foundation Work

- Refactor `src/workflow/executor.rs` around the first-release pipeline stages.
- Make the workflow executor the shared path for CLI and watcher execution.
- Support local plan execution.
- Support direct plugin execution.
- Support watcher task execution.
- Support scan-only execution.
- Support resume from workspace.

### 17.2 Execution Flow

Workflow execution should:

- Load config.
- Apply overrides.
- Validate governance.
- Initialize workspace.
- Resolve repository.
- Run git preparation.
- Run scanner or load scan artifact.
- Persist scan artifact.
- Prepare plugin context.
- Execute plugin.
- Write reports.
- Update workspace state.
- Publish watcher result when invoked by watcher.
- Return structured execution result.

### 17.3 Dry Run

- Dry run validates config, workflow, plugin config, watcher task, repository
  access, workspace paths, and report paths.
- Dry run should not call AI providers unless explicitly configured for provider
  validation.
- Dry run should not publish Kafka results unless explicitly configured.

### 17.4 Testing Requirements

- Test local workflow success.
- Test local workflow plugin failure.
- Test scan-only execution.
- Test resume from scan artifact.
- Test watcher execution path.
- Test dry-run behavior.
- Test stage updates.
- Test report persistence.
- Test publish-on-watcher path.

### 17.5 Deliverables

- Shared workflow executor.
- Execution result model.
- Resume support.
- Dry-run support.

### 17.6 Success Criteria

- CLI and watcher use the same workflow execution engine.
- Workspace state accurately reflects execution progress and failures.

## Phase 18: Documentation, Examples, and Deployment

### 18.1 Documentation

Update or create docs for:

- Architecture.
- CLI reference.
- Configuration reference.
- Workflow format.
- Watcher mode.
- Kafka task and result schemas.
- Authentication.
- Provider configuration.
- Prompt customization.
- MCP configuration.
- Plugin development.
- Technical review plugin.
- Security review plugin.
- SARIF output.
- Workspace model.
- Scanner artifacts.
- Governance.
- Deployment.

### 18.2 Examples

Add or update examples for:

- Local technical review workflow.
- Local security review workflow.
- Scan-only workflow.
- Watcher technical review task.
- Watcher security review task.
- Kafka config.
- OpenAI auth setup.
- Prompt override directory.
- MCP server config.

### 18.3 Deployment

Add or update:

- Binary build instructions.
- Dockerfile if absent.
- Container runtime config docs.
- GitHub Action example.
- CI example for security review with SARIF upload.
- Kubernetes-style watcher deployment notes if appropriate.
- Health or readiness guidance for watcher deployments.

### 18.4 Documentation Rules

- Use `.yaml` for YAML files.
- Use lowercase underscore Markdown filenames, except `README.md`.
- Keep implementation summaries in `docs/explanation`.
- Use `docs/how-to` for task-oriented docs.
- Do not use emojis.
- Ensure internal links match renamed paths.

### 18.5 Testing Requirements

- Test examples parse as config or workflows where possible.
- Test docs links where tooling exists.
- Run Markdown linting and formatting on changed docs.

### 18.6 Deliverables

- Complete first-release documentation set.
- Updated examples.
- Deployment artifacts.
- GitHub Action sample.

### 18.7 Success Criteria

- A new user can configure OpenAI, run a local review, start watcher mode, and
  understand report outputs from docs alone.

## Phase 19: End-to-End Hardening and Quality Gates

### 19.1 Integration Tests

Add end-to-end tests for:

- Local technical review with mock provider.
- Local security review with mock provider.
- Scan-only workflow.
- Direct plugin execution.
- Watcher technical review task with mock Kafka.
- Watcher security review task with mock Kafka.
- Kafka result publishing success.
- Kafka result publishing failure and retry state.
- SARIF generation.
- OpenAI auth validation with mock HTTP.
- MCP tool discovery with mock server.

### 19.2 Coverage Goals

- Maintain greater than 80 percent coverage where coverage tooling is available.
- Prioritize public APIs and failure paths.
- Test success, failure, and edge cases for all public functions.

### 19.3 Static Quality

- Ensure every public module, function, struct, enum, and trait has doc
  comments.
- Avoid `unwrap()` and `expect()` without explicit justification comments.
- Avoid ignored errors.
- Avoid recoverable `panic!` paths.
- Remove stale modules and docs.
- Remove unused dependencies where possible.

### 19.4 Quality Gate Commands

Before claiming implementation complete, run:

- `cargo fmt --all`.
- `cargo check --all-targets --all-features`.
- `cargo clippy --all-targets --all-features -- -D warnings`.
- `cargo test --all-features`.
- Markdown lint and formatting for changed Markdown files.

### 19.5 Deliverables

- Passing quality gates.
- End-to-end test coverage.
- First-release readiness checklist.

### 19.6 Success Criteria

- All first-release commands work.
- Both plugins work locally and through watcher mode.
- Kafka result publishing works.
- Security review emits SARIF.
- OpenAI is the default provider and auth path.
- No Chat, Doc Gen, or Export Restrictions code remains.

## Component Dependency Rules

### Allowed Dependencies

- CLI may depend on config, commands, workflow, workspace, and plugin metadata.
- Commands may orchestrate config, workspace, workflow, watcher, auth, prompts,
  and MCP modules.
- Workflow may depend on workspace, scanner, git, governance, plugins, reports,
  providers, and watcher result publishing interfaces.
- Plugins may depend on scanner artifacts, prompts, agent sessions, reports,
  investigation, governance diagnostics, and tools.
- Watcher may depend on config, workflow, workspace, Kafka, matcher, and plugin
  names.
- Reports may depend on plugin findings and workspace metadata.
- Tools may depend on sandbox validation and workspace paths.
- Providers may depend on auth and provider-specific HTTP clients.
- MCP may expose tools through the tool registry when explicitly configured.

### Forbidden Dependencies

- Scanner must not depend on providers, agents, prompts, plugins, watcher, or
  MCP.
- Providers must not depend on workflow or plugin modules.
- Governance must not depend on providers or plugins.
- Watcher must not contain plugin-specific review logic.
- Plugins must not bypass sandboxed tools for filesystem access.
- Report writers must not write outside validated report paths.
- Auth must not log or serialize raw secrets.

## First-Release Acceptance Criteria

The first release is complete only when all of the following are true:

- `xzardgz chat` is gone.
- `xzardgz generate` is gone.
- `src/docgen` is gone.
- Old Doc Gen workflow actions are invalid.
- Export Restrictions code and config are absent.
- OpenAI is the default provider.
- Provider auth supports OpenAI, Anthropic, Ollama, and Copilot if retained.
- Automatic model selection chooses an available compatible model before plugin
  execution.
- Model fallback behavior is deterministic and recorded when the requested model
  is unavailable.
- Thinking mode auto detection enables thinking only for providers and models
  that support it.
- `xzardgz run` can run `technical-review`.
- `xzardgz run` can run `security-review`.
- `xzardgz scan` writes a structured scan artifact.
- `xzardgz plugin` lists and validates built-in plugins.
- `xzardgz watch` consumes Kafka task messages.
- Watcher matcher config rejects all messages when empty.
- Watcher publishes success results to Kafka.
- Watcher publishes failure results to Kafka.
- Workspace state supports resume and publish retry.
- Technical review emits Markdown and JSON reports.
- Security review emits Markdown, JSON, and SARIF reports.
- Security review can fail CI on critical findings when configured.
- MCP client support is present and gated by config.
- Prompt export and validation are present.
- Governance validation is enforced.
- File tools are sandboxed.
- Tool errors do not abort agent sessions.
- Scanner has no AI dependency.
- `docs/how-to` replaces `docs/how_to`.
- All public Rust items introduced or changed have doc comments.
- Required cargo and Markdown quality gates pass.

## Risks and Mitigations

### Provider Complexity

Risk: OpenAI, Anthropic, Ollama, and Copilot auth and provider behavior differ.

Mitigation: Build shared provider config and metadata abstractions, use mock
providers heavily in tests, and make provider-specific features explicit through
capability flags.

### Watcher and Kafka Reliability

Risk: Publishing results to Kafka adds failure modes after plugin execution has
already succeeded.

Mitigation: Persist result payloads and publish status in workspace state so
publishing can be retried without rerunning expensive scans or AI reviews.

### Plugin Output Quality

Risk: AI-generated findings may be noisy or inconsistent.

Mitigation: Use scanner preselection, structured findings, confidence scoring,
verification turns, severity thresholds, and report normalization prompts.

### Secret Leakage

Risk: Security review may expose raw secret values in reports or Kafka results.

Mitigation: Redact evidence, centralize secret evidence formatting, test
redaction, and avoid including raw matched values in result messages.

### Scope Size

Risk: Implementing all reposcan architecture concepts in the first release is a
large refactor.

Mitigation: Execute phases in dependency order, keep all implementation behind
shared infrastructure, and use mock providers/Kafka/MCP to get high-confidence
coverage before real integrations.

## Recommended Execution Sequence

1. Remove Chat, Doc Gen, and legacy docs/config.
2. Establish unified errors and diagnostics.
3. Implement first-release config model.
4. Implement CLI and workflow schema.
5. Implement workspace state and artifact management.
6. Implement git operations.
7. Implement scanner and scanner common infrastructure.
8. Implement governance validation.
9. Implement OpenAI-first providers, auth, model resolution, and thinking mode
   auto detection.
10. Implement prompt system.
11. Implement agent session, sandbox, and tools.
12. Implement MCP client layer.
13. Implement plugin runtime, reports, and investigation.
14. Implement watcher and Kafka result publishing.
15. Implement technical review plugin.
16. Implement security review plugin with SARIF.
17. Integrate shared workflow executor paths.
18. Complete docs, examples, and deployment artifacts.
19. Run end-to-end hardening and quality gates.
