# Configuration Reference

## Configuration File

Create a `config.yaml` file in your project root or pass a file with `--config`.
XZardgz uses `.yaml` files and rejects legacy sections that are not part of the
workflow harness configuration model.

```yaml
provider:
  default: "openai"

provider_defaults:
  temperature: 0.2
  timeout_seconds: 120
  max_retries: 2

openai:
  api_key_env: "OPENAI_API_KEY"
  model: "gpt-4.1-mini"
  endpoint: "https://api.openai.com/v1"
  allow_insecure_endpoint: false

scanner:
  include_hidden: false
  ignore_patterns:
    - "target"
    - ".git"
    - "node_modules"

workspace:
  root: ".xzardgz/workspaces"
  resume: true

plugins:
  enabled:
    - "technical-review"
    - "security-review"

reports:
  output_dir: ".xzardgz/reports"
  formats:
    - "markdown"
    - "json"
```

See [config.example.yaml](../../config.example.yaml) for a broader example.

## Top-Level Sections

### `provider`

Selects the default provider and high-level provider behavior.

Common fields:

- `default`: Provider name. The first-release default is `openai`.
- `allow_fallback`: Whether model selection can use fallback providers.

### `provider_defaults`

Defines settings shared by provider-specific sections.

Common fields:

- `temperature`
- `timeout_seconds`
- `max_retries`
- `max_tokens`

### Provider Sections

Provider-specific sections include `openai`, `anthropic`, `ollama`, and
`copilot`.

Common fields:

- `model`: Default model for the provider.
- `api_key_env`: Environment variable that contains the API key, when needed.
- `endpoint`: API endpoint or host.
- `allow_insecure_endpoint`: Explicit opt-in for insecure endpoints.

### `scanner`

Controls repository scanning.

Common fields:

- `include_hidden`: Include hidden files outside ignored paths.
- `follow_symlinks`: Follow symlinks when scanning.
- `max_file_size_bytes`: Skip files above a configured size.
- `ignore_patterns`: Glob-like ignore patterns.

### `git`

Controls repository checkout behavior.

Common fields:

- `clone_depth`: Shallow clone depth.
- `fetch_tags`: Whether tags are fetched.
- `clean_before_run`: Whether to reset workspace repository state before a run.

### `workspace`

Controls workspace storage and resume behavior.

Common fields:

- `root`: Workspace root directory.
- `resume`: Resume existing workspace state when possible.
- `keep_failed`: Preserve failed workspaces for debugging.

### `plugins`

Defines enabled plugins.

Common fields:

- `enabled`: Ordered list of plugin identifiers.
- `default`: Default plugin for direct `run` invocations.

Built-in plugin identifiers:

- `technical-review`
- `security-review`

### `technical_review`

Configures the technical review plugin.

Common fields:

- `max_findings`
- `severity_threshold`
- `focus_areas`
- `report_formats`

### `security_review`

Configures the security review plugin.

Common fields:

- `max_findings`
- `severity_threshold`
- `include_sarif`
- `report_formats`

### `governance`

Controls repository policy checks.

Common fields:

- `enabled`
- `rules_path`
- `fail_on_violation`

### `watcher`

Controls watcher mode.

Common fields:

- `enabled`
- `max_concurrent_tasks`
- `result_publish_enabled`
- `once`

### `kafka`

Controls Kafka connectivity for watcher mode.

Common fields:

- `brokers`
- `group_id`
- `security_protocol`
- `sasl_mechanism`
- `sasl_username_env`
- `sasl_password_env`
- `ssl_ca_location`

### `topics`

Defines watcher topics.

Common fields:

- `task`: Input topic for task messages.
- `result`: Output topic for result messages.

### `matcher`

Restricts watcher task acceptance. Empty matcher configuration rejects all
messages.

Common fields:

- `event_types`
- `repositories`
- `plugins`
- `platforms`
- `metadata`

### `prompts`

Controls prompt template lookup.

Common fields:

- `directories`: Additional prompt template directories.
- `allow_overrides`: Allow project prompts to override built-ins.

### `mcp`

Configures MCP servers and allowed tools.

Common fields:

- `servers`: Named server definitions.
- `allowed_tools`: Explicit allow list per server.
- `timeout_seconds`: Tool discovery and invocation timeout.

### `model_metadata`

Controls model capability metadata.

Common fields:

- `cache_path`
- `refresh_on_start`
- `allow_degraded_metadata`

### `model_selection`

Controls automatic model selection.

Common fields:

- `enabled`
- `auto_fallback`
- `require_tools`
- `require_structured_output`
- `min_context_tokens`
- `preferred_models`
- `fallback_models`

### `scan_output`

Controls scan artifact writing.

Common fields:

- `path`
- `format`
- `overwrite`

### `reports`

Controls report writing.

Common fields:

- `output_dir`
- `formats`
- `overwrite`
- `include_diagnostics`

### `trace_transcript`

Controls transcript capture.

Common fields:

- `enabled`
- `redact_secrets`
- `output_dir`

### `project`

Stores project metadata for reports and watcher results.

Common fields:

- `name`
- `owner`
- `tags`

## Environment Variables

Common environment variables:

- `OPENAI_API_KEY`: OpenAI API key.
- `ANTHROPIC_API_KEY`: Anthropic API key.
- `XZARDGZ_CONFIG`: Default config path.
- `XZARDGZ_PROVIDER`: Provider override.
- `XZARDGZ_MODEL`: Model override.
- `XZARDGZ_WORKSPACE`: Workspace root override.
- `RUST_LOG`: Logging level, such as `info` or `debug`.

## Validation Rules

- Configuration files must use the `.yaml` extension.
- Unknown top-level sections are rejected.
- Provider endpoints must pass security validation.
- Insecure endpoints require explicit opt-in.
- Empty watcher matcher configuration rejects all messages.
- MCP tools must be explicitly allowed before plugin use.
- Provider secrets must be referenced indirectly and must not be written to
  reports or transcripts.
