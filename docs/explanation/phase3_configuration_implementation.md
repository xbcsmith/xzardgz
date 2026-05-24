# Phase 3: Configuration System

## Overview

Phase 3 delivers the full configuration system for XZardgz. Before this phase,
the application used a minimal `Config` struct that could express basic provider
and workspace settings but could not represent the complete pipeline surface:
watcher topics, Kafka security options, MCP server registrations, model
selection constraints, plugin-specific tuning, governance rules, or scan output
paths.

Phase 3 replaces that minimal struct with a twenty-six-section configuration
schema that covers every runtime concern of the pipeline. Each section maps
directly to a domain in the pipeline, enabling operators to tune behaviour
through a single `config.yaml` file without modifying source code.

Equally important is how the configuration is loaded. Phase 3 establishes a
four-layer loading pipeline that gives operators predictable, composable control:
compiled defaults are always present, a partial config file overrides only the
fields it specifies, environment variables override individual fields at deploy
time, and programmatic or CLI overrides apply last. The result is a system where
a minimal config file is valid, a complete config file is supported, and
deployment-specific secrets never have to appear in version-controlled files.

---

## Components

The following twenty-six top-level configuration sections are introduced in
Phase 3. Each section corresponds to a named struct in `src/config.rs`.

| Field name | Struct | Purpose |
| :--- | :--- | :--- |
| `provider` | `ProviderTopConfig` | Active provider selection and fallback flag |
| `provider_defaults` | `ProviderDefaultsConfig` | Shared temperature, timeout, retry, token limits |
| `openai` | `OpenAiConfig` | OpenAI API key env var, model, endpoint, security flag |
| `anthropic` | `AnthropicConfig` | Anthropic API key env var and model |
| `ollama` | `OllamaConfig` | Ollama host, model, and context window |
| `copilot` | `CopilotConfig` | GitHub Copilot model and auth store |
| `scanner` | `ScannerConfig` | File discovery options and ignore patterns |
| `git` | `GitConfig` | Clone depth, tag fetching, workspace cleanup |
| `workspace` | `WorkspaceConfig` | Workspace root, resume, and failed-run retention |
| `plugins` | `PluginsConfig` | Default plugin and enabled plugin list |
| `technical_review` | `TechnicalReviewConfig` | Finding limits, severity, focus areas, report formats |
| `security_review` | `SecurityReviewConfig` | Finding limits, severity, SARIF output, report formats |
| `governance` | `GovernanceConfig` | Rules path, enabled flag, violation behaviour |
| `watcher` | `WatcherConfig` | Watcher enabled flag, concurrency, once mode |
| `kafka` | `KafkaConfig` | Brokers, group ID, SASL and TLS settings |
| `topics` | `TopicsConfig` | Task and result topic names |
| `matcher` | `MatcherConfig` | Event type, repository, plugin, platform filters |
| `prompts` | `PromptsConfig` | Prompt search directories and override policy |
| `subagent` | `SubagentConfig` | Subagent enabled flag and maximum recursion depth |
| `trace_transcript` | `TraceTranscriptConfig` | Transcript capture, secret redaction, output path |
| `scan_output` | `ScanOutputConfig` | Scan result path, format, and overwrite flag |
| `reports` | `ReportsConfig` | Report output directory, formats, diagnostics flag |
| `mcp` | `McpConfig` | MCP server registry, global allowed tools, default timeout |
| `model_metadata` | `ModelMetadataConfig` | Model capability cache path and refresh policy |
| `model_selection` | `ModelSelectionConfig` | Capability requirements and preferred/fallback model lists |
| `project` | `ProjectConfig` | Project name, owner, and classification tags |

---

## Config Loading Pipeline

Configuration is assembled through four layers applied in priority order. Each
layer can only override fields it explicitly specifies; all other fields retain
the value from the layer below.

### Layer 1: Compiled defaults

`Config::default()` produces a fully populated `Config` struct. Every field has
a valid, conservative value. This guarantees the application can run without any
config file present.

### Layer 2: Config file

When `config.yaml` is present, it is deserialized over the defaults. Fields
present in the file replace the corresponding default. Fields absent from the
file retain the compiled default. This behaviour is described in detail in the
field-level merging section below.

The loader explicitly rejects `config.yml`. Only `config.yaml` is supported.
This avoids the ambiguity of maintaining two canonical names for the same file
and matches the project-wide file extension rule.

### Layer 3: Environment variable overrides

After the file is loaded, specific environment variables are checked. If set,
they override the corresponding field regardless of what the config file
specified. This layer is intended for deployment-time injection of secrets and
environment-specific endpoints.

### Layer 4: CLI and programmatic overrides

The `ConfigOverrides` struct allows callers (the CLI and the watcher task
dispatcher) to supply field overrides after the file and environment layers have
been applied. This is the highest-priority layer and is used for values that
come from command-line flags or from a watcher task message.

---

## Field-Level Merging

Field-level merging is the mechanism that makes partial config files valid.
Without it, a config file that omitted the `kafka` section would lose the
compiled defaults for that section rather than retaining them.

The mechanism relies on serde's `#[serde(default)]` attribute. When applied to a
struct field, it instructs serde to call the field's `Default::default()`
implementation whenever the key is absent from the deserialized input. Because
every config struct implements `Default` with sensible values, an absent section
is equivalent to the section being present with all default values.

This means a minimal config file such as:

```xzardgz/config.example.yaml#L1-4
provider:
  default: "ollama"
  allow_fallback: true
```

is fully valid. All other sections are populated from `Config::default()`.

Field-level merging does not perform deep merging of list values. If a config
file specifies `scanner.ignore_patterns`, the file's list replaces the compiled
default list entirely. Operators who want to extend the default list must
reproduce it in full within the config file.

---

## Environment Variable Overrides

The following environment variables are recognised. If set to a non-empty string,
the variable overrides the corresponding config field after the config file has
been loaded.

| Variable | Target field |
| :--- | :--- |
| `XZARDGZ_PROVIDER` | `provider.default` |
| `XZARDGZ_OPENAI_ENDPOINT` | `openai.endpoint` |
| `XZARDGZ_OPENAI_MODEL` | `openai.model` |
| `XZARDGZ_OLLAMA_HOST` | `ollama.host` |
| `XZARDGZ_OLLAMA_MODEL` | `ollama.model` |

API keys are handled differently. The `openai.api_key_env` and
`anthropic.api_key_env` fields are themselves the names of environment variables
to read at runtime, not the key values. This allows operators to use any secret
name without the config system needing to know about it.

---

## Strict Validation

After the four loading layers have been applied, the config is validated. Errors
at this stage produce actionable messages that identify the exact field and the
corrective action required.

### Legacy field rejection

Two top-level keys that existed in earlier drafts of the config schema are now
explicitly rejected: `documentation` and `export_scan`. If either key is present
in `config.yaml`, the loader returns an error with the field name and the
replacement field that should be used instead. This prevents silent mis-routing
of configuration that was written for an earlier version of the schema.

### Provider name validation

`provider.default` must be one of the four recognised provider identifiers:
`openai`, `anthropic`, `ollama`, or `copilot`. Any other value is rejected
with an error listing the accepted values.

### OpenAI endpoint security

`openai.endpoint` must begin with `https://` unless `allow_insecure_endpoint`
is explicitly set to `true`. This default-on check prevents accidental
cleartext transmission of API keys when a custom endpoint is configured. Local
development environments that run an HTTP proxy can set `allow_insecure_endpoint:
true` to opt out.

### Model selection token minimum

`model_selection.min_context_tokens` must be greater than zero. A zero or
negative value would cause the capability filter to reject every available model,
which is almost certainly a configuration error rather than deliberate intent.

### MCP server timeout

Each entry in `mcp.servers` has an optional `timeout_seconds` field. When
present, it must be greater than zero. A zero timeout would cause every MCP
tool call to time out immediately.

---

## Model Selection Configuration

`ModelSelectionConfig` expresses the capability constraints that must be
satisfied before a model is considered eligible for use. This is used by the
provider factory and by the workflow planner when selecting a model at runtime.

```xzardgz/config.example.yaml#L148-157
model_selection:
  enabled: true
  auto_fallback: true
  require_tools: true
  require_structured_output: true
  min_context_tokens: 16000
  preferred_models:
    - "gpt-4.1-mini"
  fallback_models:
    - "gpt-4.1"
```

The fields have the following semantics:

- `enabled` - when false, model selection is bypassed and the configured
  provider's default model is used unconditionally.
- `auto_fallback` - when true, if the preferred model fails a capability check
  the system moves to the next model in `fallback_models`.
- `require_tools` - the selected model must support tool calling.
- `require_structured_output` - the selected model must support structured JSON
  output mode.
- `min_context_tokens` - the selected model must have a context window of at
  least this many tokens.
- `preferred_models` - ordered list of model identifiers to try first.
- `fallback_models` - ordered list of model identifiers to try if no preferred
  model satisfies the capability constraints.

### ModelSelectionOverrides

`ModelSelectionOverrides` is a partial struct used to merge model selection
settings from multiple sources without requiring a full `ModelSelectionConfig`
to be present at each source. It carries the same fields as
`ModelSelectionConfig` but all are optional. A merge applies each non-None
field from the override onto the base config.

The sources that can produce a `ModelSelectionOverrides` value and the order in
which they are applied are:

1. `provider_defaults` (base layer)
2. Plugin configuration (`technical_review`, `security_review`)
3. Workflow plan fields
4. Watcher task message fields
5. CLI flags via `ConfigOverrides`

This ordering means a CLI flag always wins over a plugin default, and a plugin
default always wins over the global provider default.

---

## Provider Configuration

### Provider selection

`provider.default` names the active provider. At runtime, `ProviderFactory`
reads this field and instantiates the corresponding provider implementation.
`provider.allow_fallback` controls whether the factory may try an alternative
provider if the selected one is unavailable or returns a transient error.

The default provider is `openai`. This reflects the expectation that most
production deployments will use the OpenAI API. Operators running local-only
environments should set `provider.default: "ollama"`.

### The provider_defaults layer

`provider_defaults` supplies shared defaults for fields that appear across
multiple provider implementations: `temperature`, `timeout_seconds`,
`max_retries`, and `max_tokens`. Individual provider configs do not repeat these
fields. Instead, the pipeline reads `provider_defaults` as the base and applies
provider-specific overrides on top. This prevents the same value from needing to
be duplicated in four places when a global change is needed.

### OpenAI configuration

`OpenAiConfig` holds `api_key_env`, `model`, `endpoint`, and
`allow_insecure_endpoint`. The API key is never stored directly; only the name
of the environment variable that holds it is stored. The endpoint defaults to
the public OpenAI API but can be replaced with a compatible endpoint such as
Azure OpenAI or a local proxy.

### Anthropic configuration

`AnthropicConfig` holds `api_key_env` and `model`. The Anthropic provider
follows the same key-by-reference pattern as OpenAI.

### Ollama configuration

`OllamaConfig` holds `host`, `model`, and `context_length`. The context length
is exposed explicitly because Ollama models have varying context windows and
the appropriate value depends on the model loaded in the Ollama server.

### Copilot configuration

`CopilotConfig` holds `model` and `auth_store`. The `auth_store` field
identifies where the GitHub Copilot authentication token is stored. The default
value `"keychain"` instructs the provider to read the token from the operating
system keychain.

---

## Watcher and Kafka Configuration

The watcher subsystem is composed of four config sections that work together.

### WatcherConfig

`WatcherConfig` controls the watcher process itself. `enabled` must be `true`
for the watcher to start. `max_concurrent_tasks` limits how many pipeline runs
execute in parallel. `result_publish_enabled` controls whether results are
published back to the Kafka result topic. `once` causes the watcher to process
one batch and exit, which is useful for smoke-testing the pipeline end-to-end
without a long-running process.

### KafkaConfig

`KafkaConfig` holds the Kafka connection parameters. `brokers` is a list of
bootstrap addresses. `group_id` identifies the consumer group. The
`security_protocol`, `sasl_mechanism`, `sasl_username_env`, `sasl_password_env`,
`ssl_ca_location` fields cover the full range of Kafka authentication options
from unauthenticated `PLAINTEXT` through SASL-authenticated `SASL_SSL`. As with
provider API keys, SASL credentials are stored as environment variable names
rather than literal values.

### TopicsConfig

`TopicsConfig` names the Kafka topics. `task` is the topic from which the
watcher reads incoming task messages. `result` is the topic to which the watcher
publishes result messages. These are separated to allow independent retention
and access control policies on each topic.

### MatcherConfig and the is_empty guard

`MatcherConfig` holds four filter sets: `event_types`, `repositories`,
`plugins`, and `platforms`, plus a `metadata` map for arbitrary key-value
matching. The watcher uses this config to decide whether an incoming message
should be routed to the pipeline.

`MatcherConfig::is_empty()` returns `true` when `event_types`, `repositories`,
and `plugins` are all empty. An empty matcher is treated as matching nothing at
runtime. This is a deliberate safety default: if an operator deploys the watcher
without configuring any routing rules, it processes no messages rather than
processing all messages indiscriminately. Operators who want to accept all
messages of any type must explicitly add at least one event type to the matcher.

---

## MCP Configuration

`McpConfig` is the registry for Model Context Protocol servers. It has three
fields: `servers`, `allowed_tools`, and `timeout_seconds`.

`servers` is a list of `McpServerConfig` entries. Each entry describes a single
MCP server process:

- `name` - identifier used to reference this server in tool calls.
- `command` - the executable to launch.
- `args` - command-line arguments passed to the executable.
- `env` - environment variables injected into the server process.
- `timeout_seconds` - per-server timeout; overrides the global `mcp.timeout_seconds`.
- `transport` - transport type, typically `"stdio"`.
- `allowed_tools` - list of tool names this server is permitted to expose.
- `auth` - optional authentication configuration for servers that require it.

`allowed_tools` at the top `McpConfig` level is a map from server name to a list
of permitted tool names. It provides a secondary allow-list that is checked in
addition to the per-server `allowed_tools` field. This allows a central policy
to restrict tool access without modifying individual server entries.

`timeout_seconds` at the `McpConfig` level is the default timeout applied to any
server that does not specify its own `timeout_seconds`. The validation rule
ensures each explicit timeout is greater than zero.

---

## Plugin Configuration

### PluginsConfig

`PluginsConfig.default` names the plugin that runs when no explicit plugin is
requested. `PluginsConfig.enabled` is the list of plugins available for use.
A plugin that is not in `enabled` cannot be invoked, even if it is named in a
watcher task message.

### TechnicalReviewConfig

`TechnicalReviewConfig` controls the technical review plugin:

- `max_findings` - caps the number of findings the plugin may report per run.
- `severity_threshold` - the minimum severity level a finding must meet to be
  included in the report.
- `focus_areas` - an ordered list of concern areas (for example `"architecture"`,
  `"reliability"`) that the prompt directs the model to prioritize.
- `report_formats` - list of output formats; `"markdown"` and `"json"` are
  always supported.

### SecurityReviewConfig

`SecurityReviewConfig` mirrors `TechnicalReviewConfig` with one additional field:

- `include_sarif` - when `true`, the plugin produces a SARIF 2.1.0 report
  alongside the other formats. SARIF is the standard interchange format for
  static analysis results and is understood by GitHub Code Scanning.

---

## Implementation Details

### No .yml support

The config loader accepts only `config.yaml`. Files named `config.yml` are not
read and do not trigger a warning. This is consistent with the project-wide rule
that `.yaml` is the canonical extension and avoids the complexity of checking
two paths.

### Legacy field rejection via two-phase parse

Detecting legacy fields requires a different approach than normal serde
deserialization, which silently ignores unknown keys by default. Phase 3 uses a
two-phase parse: the raw YAML is first deserialized into a
`serde_yaml::Value` (a generic map), which is then inspected for the presence of
`documentation` or `export_scan` keys before the typed deserialization pass
runs. If either key is found, an error is returned immediately with the key name
and a message indicating the replacement field. This approach avoids the
complexity of writing a custom serde visitor while still catching the legacy
keys reliably.

### ConfigOverrides struct

`ConfigOverrides` is a plain struct with optional fields for the settings most
commonly overridden at CLI or task dispatch time: `provider`, `openai_model`,
`ollama_model`, `ollama_host`, and `workspace_root`. The `apply` method on
`Config` takes a `ConfigOverrides` value and applies each non-None field. This
keeps override logic out of the CLI argument parser and out of the watcher task
handler, centralising it in `src/config.rs`.

### ModelSelectionOverrides merge strategy

`ModelSelectionOverrides::merge_into` takes a mutable reference to a
`ModelSelectionConfig` and applies each non-None field from the override. List
fields (`preferred_models`, `fallback_models`) replace the target list entirely
when present in the override. Scalar fields (`auto_fallback`, `require_tools`,
`require_structured_output`, `min_context_tokens`) replace the corresponding
target field. The caller controls merge ordering by controlling the sequence of
`merge_into` calls.

### ProviderFactory::create_from_config

The provider factory was updated from `create(&Config, provider_name: &str)` to
`create_from_config(&Config)`. The new signature reads `config.provider.default`
internally, which eliminates the duplication of provider selection logic across
the several call sites that previously had to pass the name explicitly. The
function dispatches on the four recognised provider names: `"ollama"` and
`"copilot"` are implemented in Phase 3; `"openai"` and `"anthropic"` return a
descriptive not-yet-implemented error that will be resolved in Phase 9.

---

## Testing

Test coverage for Phase 3 spans config loading, validation, environment variable
handling, override merging, and matcher semantics.

### Config loading tests

- `Config::default()` produces a fully populated struct with no `Option::None`
  fields that should have a value.
- Loading a partial YAML string that specifies only `provider.default` leaves all
  other fields at their compiled defaults.
- Loading a complete YAML string (matching `config.example.yaml`) produces the
  expected values for all twenty-six sections.
- Attempting to load a file named `config.yml` returns an error identifying the
  wrong extension.

### Legacy rejection tests

- A YAML string containing a top-level `documentation` key is rejected with an
  error message that names the key.
- A YAML string containing a top-level `export_scan` key is rejected with an
  error message that names the key.

### Validation tests

- A config with `provider.default: "unknown-provider"` fails validation and lists
  the accepted values in the error message.
- A config with `openai.endpoint: "http://api.openai.com/v1"` and
  `allow_insecure_endpoint: false` fails validation.
- The same config with `allow_insecure_endpoint: true` passes validation.
- A config with `model_selection.min_context_tokens: 0` fails validation.
- A config with an MCP server entry where `timeout_seconds: 0` fails validation.

### Environment variable tests

- Setting `XZARDGZ_PROVIDER` to `"ollama"` overrides `provider.default`.
- Setting `XZARDGZ_OLLAMA_HOST` to a custom address overrides `ollama.host`.
- Unsetting the variable after a set leaves the file-loaded value in place.

### Override and merge tests

- `ConfigOverrides` with only `provider` set updates `config.provider.default`
  and leaves all other fields unchanged.
- `ModelSelectionOverrides` with `preferred_models` set replaces the list in the
  target config; scalar fields not present in the override are unchanged.
- Sequential `merge_into` calls apply in order: the last call wins for any field
  set by multiple overrides.

### Matcher tests

- `MatcherConfig::is_empty()` returns `true` for a default-constructed matcher.
- `is_empty()` returns `false` when `event_types` is non-empty.
- `is_empty()` returns `false` when `repositories` is non-empty.
- `is_empty()` returns `false` when `plugins` is non-empty.
- A matcher with only `platforms` set returns `true` from `is_empty()` because
  `platforms` alone is not sufficient to route a message.

All tests use `#[cfg(test)]` modules in the same file as the implementation and
follow the structure required by the project coding standards.

---

## Success Criteria

The following criteria define a complete Phase 3 implementation, corresponding
to the 3.10 success criteria in the project plan.

**Config schema completeness**

- All twenty-six top-level sections are present in the `Config` struct with the
  correct field names and types.
- `Config::default()` compiles and produces sensible values for every field.
- `config.example.yaml` matches the `Config` struct field names exactly and can
  be deserialized without error.

**Loading pipeline**

- A missing `config.yaml` is not an error; compiled defaults are used.
- A partial `config.yaml` leaves unspecified fields at their compiled defaults.
- `config.yml` is rejected with a clear error message.
- All five environment variable overrides are applied after file loading.
- `ConfigOverrides::apply` updates only the fields it carries; others are
  unchanged.

**Validation**

- Unknown provider names are rejected with the accepted-values list in the error.
- HTTP OpenAI endpoints are rejected unless `allow_insecure_endpoint: true`.
- `model_selection.min_context_tokens: 0` is rejected.
- MCP server `timeout_seconds: 0` is rejected.
- `documentation` and `export_scan` top-level keys are rejected with actionable
  messages.

**Model selection**

- `ModelSelectionConfig` has all seven fields with the correct defaults.
- `ModelSelectionOverrides::merge_into` applies non-None fields and leaves others
  unchanged; merge order test passes.

**Matcher**

- `MatcherConfig::is_empty()` returns `true` for a default-constructed matcher
  and `false` when any of `event_types`, `repositories`, or `plugins` is
  non-empty.

**Provider factory**

- `ProviderFactory::create_from_config` reads `provider.default` and dispatches
  correctly to `"ollama"` and `"copilot"` implementations.
- `"openai"` and `"anthropic"` return a not-yet-implemented error that includes
  the provider name and a reference to Phase 9.

**Quality gates**

- `cargo fmt --all` passes with no formatting changes needed.
- `cargo check --all-targets --all-features` passes with zero compilation errors.
- `cargo clippy --all-targets --all-features -- -D warnings` passes with zero
  warnings.
- `cargo test --all-features` passes; coverage exceeds 80% for new code in
  `src/config.rs`.
