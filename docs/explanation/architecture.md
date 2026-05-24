# XZardgz Architecture

## Overview

XZardgz is a generic AI workflow harness for repository-oriented automation. It
loads configuration, prepares a workspace, scans a repository, resolves provider
and model settings, runs plugins, writes reports, and can process tasks from a
watcher queue.

The target command surface is `run`, `scan`, `plugin`, `watch`, `auth`,
`prompts`, and `mcp`.

## System Context

XZardgz sits between repositories, AI providers, workflow plugins, local
workspace state, optional MCP servers, and optional Kafka topics.

```text
User or watcher task
        |
        v
CLI command router
        |
        v
Workflow harness -- configuration, auth, prompts, model selection
        |
        +--> Repository scanner --> scan artifact
        +--> Plugin runtime -----> findings and reports
        +--> MCP client ---------> allowed external tools
        +--> Watcher publisher --> result messages
```

## Core Components

### CLI Layer

The CLI parses command arguments and routes each command to a thin handler.
Handlers should translate user input into reusable workflow operations instead
of embedding business logic.

Commands:

- `run`: Execute a local plan or direct plugin invocation.
- `scan`: Build a structured repository scan artifact.
- `plugin`: List, inspect, validate, or run workflow plugins.
- `watch`: Process queued tasks and publish results.
- `auth`: Manage provider credentials.
- `prompts`: Manage prompt templates.
- `mcp`: Validate MCP servers and inspect tools.

### Configuration System

Configuration is loaded from `.yaml` files, environment variables, and CLI
overrides. The first-release model is strict: unknown legacy sections are
rejected, and only workflow harness sections are accepted.

Primary sections include:

- `provider` and `provider_defaults`
- Provider-specific sections such as `openai`, `anthropic`, `ollama`, and
  `copilot`
- `scanner`, `git`, and `workspace`
- `plugins`, `technical_review`, and `security_review`
- `governance`
- `watcher`, `kafka`, `topics`, and `matcher`
- `prompts`, `mcp`, and `subagent`
- `model_metadata` and `model_selection`
- `scan_output`, `reports`, `trace_transcript`, and `project`

### Workspace Management

A workspace stores state for each workflow run. It records repository metadata,
scan artifacts, selected provider and model information, plugin state, report
paths, transcripts when enabled, watcher task metadata, and final status.

Workspace stages are intended to be incremental so interrupted work can be
inspected or resumed where safe.

### Repository Scanner

The scanner reads repository files, applies ignore rules, collects structure and
metadata, and writes a scan artifact. Plugins consume the scan artifact rather
than reimplementing repository discovery.

The scan artifact supports local debugging, CI preflight checks, plugin
development, and watcher troubleshooting.

### Provider and Model Layer

The provider layer abstracts OpenAI, Anthropic, Ollama, and Copilot. The model
selection layer resolves provider defaults, workflow overrides, watcher task
overrides, plugin requirements, and CLI flags into a concrete model decision.

Provider diagnostics are persisted so reports and watcher results can explain
which model was used and whether a fallback occurred.

### Authentication

Authentication is managed through the `auth` command. The target behavior
supports login, logout, status, validation, key setting, and key removal across
all configured providers.

Secrets should be loaded from environment variables, keychains, or configured
secret stores. Reports and logs must not include secret values.

### Prompt System

Prompt templates are externalized and resolved by a clear search order. The
`prompts` command can export built-ins, validate prompt directories, show
resolution order, list plugin templates, and render prompts with safe test
context.

### MCP Client

The MCP client validates configured servers, discovers exposed tools, and allows
safe tool calls only when tools are explicitly enabled. MCP integration is an
extension point for plugin execution, not a replacement for local sandboxing.

### Plugin Runtime

Plugins receive a workflow context, scan artifact, provider access, configured
prompts, and an output writer. The first built-in plugin identifiers are:

- `technical-review`
- `security-review`

Plugin output is normalized into findings, diagnostics, artifacts, and reports.
Security review can include SARIF output when configured.

### Reports

Reports collect plugin metadata, scan metadata, findings, diagnostics, provider
selection details, artifact paths, and final status. Supported formats are
configured per workflow or plugin.

### Watcher Mode

Watcher mode consumes Kafka task messages, validates matcher rules, rejects
messages when matcher configuration is empty, executes accepted tasks through
the workflow harness, and publishes result messages.

Watcher results include status, report paths, finding counts, selected provider
and model details, and structured diagnostics.

## Data Flow

### Local Plugin Workflow

1. Parse CLI arguments or a plan file.
2. Load configuration and apply overrides.
3. Create or resume a workspace.
4. Open or clone the target repository.
5. Scan the repository and persist the scan artifact.
6. Resolve provider, model, prompts, and plugin configuration.
7. Execute the selected plugin.
8. Write reports and artifacts.
9. Persist final workspace state.
10. Print a summary and exit with the configured status behavior.

### Watcher Workflow

1. Consume a task message.
2. Validate message shape and matcher rules.
3. Resolve workflow, repository, plugin, and provider overrides.
4. Execute the same harness path used by local workflows.
5. Publish a structured result message when enabled.
6. Commit or reject the task according to watcher policy.

## Target Module Layout

```text
src/
├── auth/
├── cli/
├── commands/
├── config/
├── git/
├── governance/
├── scanner/
├── providers/
├── prompts/
├── agent/
├── tools/
├── plugins/
├── investigation/
├── reports/
├── workspace/
├── workflow/
├── watcher/
├── mcp/
└── telemetry.rs
```

## Configuration Principles

- Use `.yaml` files only.
- Prefer OpenAI as the default provider while supporting configured
  alternatives.
- Reject legacy sections that do not belong to the workflow harness model.
- Keep endpoint security validation explicit.
- Treat empty watcher matcher configuration as reject-all.
- Persist model selection diagnostics for auditability.

## Error Handling

Errors should be structured and actionable. Recoverable operations return
`Result<T, E>`, and errors should preserve command context, configuration path,
workspace path, plugin name, provider name, and task identifiers when available.

Tool execution failures should become structured tool results when a plugin can
continue safely. Fatal configuration, authentication, or validation errors
should stop before provider or plugin execution.

## Security Considerations

- Do not write provider secrets to reports, transcripts, logs, or watcher
  results.
- Validate repository URLs and endpoint overrides.
- Require explicit opt-in for insecure provider endpoints.
- Restrict MCP tools through allow lists.
- Apply sandbox rules to file and process tools.
- Keep watcher matcher rules strict enough to avoid unintended task execution.

## Testing Strategy

The first-release test strategy should cover:

- CLI parsing and command routing.
- Strict configuration validation.
- Workspace creation and persistence.
- Scanner output and ignore behavior.
- Provider and model selection diagnostics.
- Prompt resolution.
- Plugin execution and report generation.
- Watcher task acceptance, rejection, and result publishing.
- MCP server validation and safe tool discovery.

## Future Extensibility

The workflow harness is designed for additional plugins, report formats,
providers, scanner enrichments, and watcher integrations. New capabilities
should depend on generic workflow, scanner, provider, prompt, and report
interfaces rather than on single-purpose product flows.

## Conclusion

XZardgz is structured as a reusable workflow harness. Its architecture separates
command routing, configuration, scanning, provider interaction, prompt
resolution, plugin execution, reporting, watcher processing, and MCP integration
so each area can evolve independently.
