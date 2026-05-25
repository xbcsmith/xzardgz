# Phase 9: Provider Abstraction and Authentication Implementation

## Overview

Phase 9 builds the complete AI provider abstraction layer for XZardgz. The phase
delivers an expanded `Provider` trait, four concrete provider implementations
(OpenAI, Anthropic, Ollama, and Copilot), a model capability resolver, and a
multi-provider authentication system.

OpenAI is the default and primary provider. The configuration key
`provider.default` or the CLI flag `--provider` selects which backend executes
requests at runtime. All pipeline code above the provider layer holds an
`Arc<dyn Provider>` and is unaware of the concrete type in use.

## Module Layout

### providers/

| File                  | Purpose                                                |
| --------------------- | ------------------------------------------------------ |
| `base.rs`             | `Provider` trait definition and mock                   |
| `types.rs`            | `ThinkingMode`, `Message`, `Tool`, `ModelCapabilities` |
| `openai.rs`           | `OpenAiProvider` and `infer_openai_capabilities`       |
| `anthropic.rs`        | `AnthropicProvider` and `infer_anthropic_capabilities` |
| `ollama.rs`           | `OllamaProvider` and `infer_ollama_capabilities`       |
| `copilot.rs`          | `CopilotProvider`                                      |
| `copilot_auth.rs`     | `CopilotAuth` OAuth device flow                        |
| `factory.rs`          | `ProviderFactory`                                      |
| `model_resolution.rs` | `ResolutionContext`, `ResolvedModel`, `ModelResolver`  |
| `mod.rs`              | Public re-exports                                      |

### auth/

| File           | Purpose                                                |
| -------------- | ------------------------------------------------------ |
| `mod.rs`       | `ProviderAuthManager`                                  |
| `openai.rs`    | `OpenAiAuth`                                           |
| `anthropic.rs` | `AnthropicAuth`                                        |
| `ollama.rs`    | `OllamaAuth`                                           |
| `store.rs`     | `SecretStore` trait, `KeyringStore`, `EnvVarStore`     |
| `types.rs`     | `AuthStatus`, `CredentialSource`, `AllProvidersStatus` |

## Provider Trait

The `Provider` trait lives in `src/providers/base.rs`. It is annotated with
`#[async_trait]` and bounded by `Send + Sync` so that instances can be held in
`Arc<dyn Provider + Send + Sync>` and shared across async tasks.

The trait is also annotated with `#[cfg_attr(test, mockall::automock)]`,
generating a `MockProvider` for unit testing without real API calls.

### Methods

`provider_name() -> &str` returns the canonical lowercase name for the provider,
for example `"openai"` or `"ollama"`. This name appears in configuration keys,
log messages, and `CredentialSource` records.

`metadata() -> ProviderMetadata` returns static capability metadata for the
provider as a whole. The call is synchronous and infallible; it is used by the
resolver before any network activity is initiated.

`supports_thinking() -> bool` returns `true` when at least one of the provider's
models supports extended reasoning. This is a fast synchronous check used to
gate thinking-mode requests before the more expensive `list_models` call.

`credential_status() -> CredentialStatus` inspects the system keyring and
environment variables and returns one of three values:

- `CredentialStatus::Present` — credentials found in at least one location.
- `CredentialStatus::Missing` — no credentials found anywhere.
- `CredentialStatus::Unknown` — status cannot be determined.

This method never makes a network call. It is used by the pipeline pre-flight
check to surface credential problems before the first token is spent.

`list_models() -> Result<Vec<ModelMetadata>>` returns available model
identifiers with inferred capability flags. Implementations fall back to a
static metadata list when the provider endpoint is offline or unauthenticated,
returning `Ok(static_list)` rather than propagating a network error. The
`MetadataSource` field of `ResolvedModel` records which source was used.

`complete(messages, tools) -> Result<Message>` sends a standard chat completion
request and returns the full assistant response.

`complete_with_thinking(messages, tools, thinking_mode) -> Result<Message>`
sends a thinking-aware completion request. The default trait implementation
ignores `thinking_mode` and delegates to `complete`. Providers that support
extended reasoning override this method to forward the token budget to the API.

`complete_streaming(messages, tools) -> Result<Pin<Box<dyn Stream<Item = Result<Message>> + Send>>>`
sends a chat completion request and returns a pinned, boxed async stream. Each
item is a partial assistant `Message` containing the latest token delta. Callers
accumulate the `content` fields to reconstruct the full response.

### Error Convention

All async methods return `crate::error::Result<T>`, which is
`Result<T, PipelineError>`. Authentication failures (HTTP 401 and 403) map to
`PipelineError::Auth`. All other API errors, network failures, and
deserialisation problems map to `PipelineError::Provider`. The separation lets
the pipeline surface targeted credential hints separately from transient network
problems.

## ThinkingMode Enum

`ThinkingMode` is defined in `src/providers/types.rs` and controls how the
provider allocates a reasoning token budget before composing its response. It is
set per-request by the model resolver and forwarded to `complete_with_thinking`.

| Variant     | Behavior                                                    | Budget Tokens |
| ----------- | ----------------------------------------------------------- | ------------- |
| `None`      | Extended reasoning is never requested                       | n/a           |
| `Auto`      | Requested only when model supports it; falls back to `None` | n/a           |
| `Low`       | Explicit low reasoning budget                               | 2,000         |
| `Medium`    | Explicit medium reasoning budget                            | 8,000         |
| `High`      | Explicit high reasoning budget                              | 16,000        |
| `ExtraHigh` | Explicit maximum reasoning budget                           | 32,000        |

`ThinkingMode::None` is the default variant.

### Helper Methods

`requests_thinking()` returns `true` for all variants except `None`. This
distinguishes "never use thinking" from "use thinking if available".

`requires_thinking_support()` returns `true` only for explicit levels (`Low`,
`Medium`, `High`, `ExtraHigh`). `Auto` and `None` return `false` because they
either opt out or degrade gracefully when thinking is unavailable.

`budget_tokens()` returns `Option<u32>`. `None` and `Auto` return `None` because
the caller or provider chooses the budget. The explicit levels return the values
shown in the table above.

### Degradation Policy

`Auto` never hard-fails on models that lack thinking support. The resolver
silently downgrades to `ThinkingMode::None` and adds a `DiagnosticLevel::Info`
entry to `ResolvedModel.diagnostics`.

Explicit levels (`Low` and above) follow stricter rules. When
`allow_degraded_metadata` is `false` and the selected model does not support
thinking, the resolver returns `PipelineError::Provider`. When
`allow_degraded_metadata` is `true` the resolver downgrades to `None` and
records a `DiagnosticLevel::Warning`.

## Provider Implementations

### OpenAI (`src/providers/openai.rs`)

`OpenAiProvider` is the default provider. It is selected when
`config.provider.default` is `"openai"` or when the field is empty.

The implementation targets any OpenAI-compatible endpoint, including Azure
OpenAI Service, LocalAI, and other drop-in replacements, using the base URL from
`config.openai.endpoint`. Endpoint security is enforced by default: the URL must
start with `https://` unless `config.openai.allow_insecure_endpoint` is `true`.
Attempting to construct an `OpenAiProvider` with an `http://` endpoint and the
flag unset returns `PipelineError::Config`.

Thinking support is provided through the `reasoning_effort` field in the OpenAI
request body. `complete_with_thinking` maps `ThinkingMode` values to `"low"`,
`"medium"`, and `"high"` strings for the o-series reasoning models.

The live model list is fetched from `GET {endpoint}/models`. Capabilities for
each returned model ID are derived by `infer_openai_capabilities`. When the
endpoint is unreachable or the API key is absent, the fallback is a single-entry
list for the currently configured model.

API keys are read from the environment variable named by
`config.openai.api_key_env` (default: `OPENAI_API_KEY`).

### Anthropic (`src/providers/anthropic.rs`)

`AnthropicProvider` targets the Anthropic Messages API at
`https://api.anthropic.com/v1/messages`. Requests carry the `x-api-key` header
for authentication and the `anthropic-version` header required by the API.

The Anthropic wire format differs from OpenAI in two ways. First, system-role
messages are extracted from the conversation and sent as the top-level `system`
field rather than embedded in the `messages` array. Second, tool definitions use
an `input_schema` field instead of `parameters`.

Thinking support is implemented via a `thinking` block in the request body,
which carries a `budget_tokens` value derived from the `ThinkingMode` passed to
`complete_with_thinking`.

The live model list is fetched from `GET https://api.anthropic.com/v1/models`.
Capabilities are derived by `infer_anthropic_capabilities`. The static fallback
is a single-entry list for the configured model.

API keys are read from the environment variable named by
`config.anthropic.api_key_env` (default: `ANTHROPIC_API_KEY`).

### Ollama (`src/providers/ollama.rs`)

`OllamaProvider` targets a locally running Ollama inference server and requires
no credentials. `credential_status()` always returns `CredentialStatus::Present`
to indicate the provider is unconditionally usable.

`supports_thinking()` returns `false` unconditionally because no Ollama model
currently supports extended reasoning. `complete_with_thinking` ignores the
`thinking_mode` argument and delegates to `complete`.

The live model list is fetched from `GET {host}/api/tags`. On any network
failure the fallback is a single-entry list for the configured model.

The Ollama base URL defaults to `http://localhost:11434`. The actual context
window is controlled by Ollama's `num_ctx` parameter at runtime rather than by
the model name.

### Copilot (`src/providers/copilot.rs`)

`CopilotProvider` targets `https://api.githubcopilot.com/chat/completions` using
OAuth tokens managed by `CopilotAuth` in `src/providers/copilot_auth.rs`. The
device flow exchanges a device code for a user access token that is stored in
the keyring under service `"xzardgz-copilot"` and refreshed automatically on
expiry.

`supports_thinking()` returns `false`. `complete_with_thinking` delegates
directly to `complete`.

The model list is static because Copilot does not expose a model listing
endpoint. `list_models()` always returns the compiled-in table.

Tools are not forwarded to the Copilot API (`supports_tools = false`).

## Dynamic Capability Inference

No hardcoded model list is used anywhere in the codebase. Every provider fetches
its model list from the live API at runtime and derives capabilities using a
pure inference function. All inference functions are conservative: when a model
name is ambiguous the function returns `false` for uncertain capability flags.

### OpenAI: `infer_openai_capabilities(model_id)`

The function lowercases the input before pattern matching.

| Pattern                                          | Capability inferred                           |
| ------------------------------------------------ | --------------------------------------------- |
| Starts with `o` + digit (`o1`, `o3`, `o3-mini`)  | `supports_thinking = true`, 200,000-token ctx |
| Contains `-reasoning`                            | `supports_thinking = true`                    |
| Contains `gpt-4`                                 | 128,000-token context window                  |
| Contains `embedding`, `whisper`, `tts`, `dall-e` | `supports_tools = false`                      |
| Exactly `o1` or `o1-preview`                     | `supports_streaming = false`                  |
| Contains `gpt-4o`, `gpt-4-vision`, `gpt-4.1`     | `supports_vision = true`                      |
| Any other chat model                             | `supports_tools = true`, 16,384-token ctx     |

### Anthropic: `infer_anthropic_capabilities(model_id)`

The function lowercases the input before pattern matching. Modern models are
those whose ID contains `claude-3`, `claude-4`, `claude-opus-4`,
`claude-sonnet-4`, or `claude-haiku-4`.

| Pattern                                                         | Capability inferred                 |
| --------------------------------------------------------------- | ----------------------------------- |
| Modern model (`claude-3+`, `claude-4+`)                         | `supports_tools`, `supports_vision` |
| Contains `claude-3-5`, `claude-3.5`, `claude-3-7`, `claude-3.7` | `supports_thinking = true`          |
| Contains `claude-4`, `claude-opus-4`, `claude-sonnet-4`         | `supports_thinking = true`          |
| `claude-3` + `opus`                                             | `supports_thinking = true`          |
| Modern model                                                    | 200,000-token context window        |
| Other                                                           | 100,000-token context window        |

All Claude models receive `supports_streaming = true`.

### Ollama: `infer_ollama_capabilities(model_id)`

The tag suffix (`:latest`, `:7b`, `:instruct`) is stripped before matching.
`supports_thinking` is always `false` and `supports_structured_output` is always
`false`. All Ollama models receive a 32,768-token context window.

| Base name pattern                                           | Capability inferred      |
| ----------------------------------------------------------- | ------------------------ |
| `mistral`, `llama3*`, `qwen`, `gemma2`, `gemma3`, `mixtral` | `supports_tools = true`  |
| `command-r`, `phi3`, `phi4`, `solar`                        | `supports_tools = true`  |
| `llava`, `bakllava`, `vision`, `minicpm-v`, `moondream`     | `supports_vision = true` |
| `cogvlm`                                                    | `supports_vision = true` |
| Anything else                                               | Conservative defaults    |

## Model Capability Resolver

`ModelResolver` is a stateless service defined in
`src/providers/model_resolution.rs`. It maps a potentially underspecified
`(provider, model)` pair drawn from multiple competing sources to a fully
resolved `ResolvedModel` that includes verified capability flags and a complete
record of every selection decision.

### ResolutionContext

`ResolutionContext` carries the inputs to the resolution process. Build it using
the fluent builder API.

| Field               | Source level  | Description                         |
| ------------------- | ------------- | ----------------------------------- |
| `cli_provider`      | CLI (highest) | Provider from `--provider` flag     |
| `cli_model`         | CLI (highest) | Model from `--model` flag           |
| `watcher_provider`  | Watcher       | Provider from watcher task metadata |
| `watcher_model`     | Watcher       | Model from watcher task metadata    |
| `workflow_provider` | Workflow      | Provider declared in workflow plan  |
| `workflow_model`    | Workflow      | Model declared in workflow plan     |
| `plugin_provider`   | Plugin        | Provider preference from plugin     |
| `plugin_model`      | Plugin        | Model preference from plugin        |
| `thinking_mode`     | Any source    | Requested `ThinkingMode`            |

Builder methods: `new()`, `with_cli(provider, model)`, `with_watcher(p, m)`,
`with_workflow(p, m)`, `with_plugin(p, m)`, `with_thinking_mode(mode)`.

`effective_provider(config_default)` walks five levels: CLI, watcher, workflow,
plugin, `config_default`. If all are `None` it falls back to the hard-coded
string `"openai"`.

`effective_model_override()` walks four levels: CLI, watcher, workflow, plugin.
Returns `None` when no level specifies a model; the caller then consults
`config.model_selection.preferred_models` or the provider default.

### Provider Precedence

Resolution stops at the first non-empty value in this order:

1. CLI `--provider` flag (`cli_provider`)
2. Watcher task `provider` field (`watcher_provider`)
3. Workflow plan provider (`workflow_provider`)
4. Plugin preference (`plugin_provider`)
5. `config.provider.default`
6. Hard-coded fallback `"openai"`

### Model Precedence

Within the selected provider, model resolution follows this order:

1. CLI `--model` flag (`cli_model`)
2. Watcher task `model` field (`watcher_model`)
3. Workflow plan model (`workflow_model`)
4. Plugin preference (`plugin_model`)
5. `config.model_selection.preferred_models[0]`
6. Provider-specific config default model (passed as parameter)
7. First compatible model from the available model list

### ResolvedModel

`ResolvedModel` is the output of a successful resolution pass. It is
serialisable and persisted in workspace state.

| Field                     | Type                | Meaning                                     |
| ------------------------- | ------------------- | ------------------------------------------- |
| `requested_provider`      | `String`            | Provider from the highest-priority source   |
| `selected_provider`       | `String`            | Provider actually used after fallback       |
| `requested_model`         | `Option<String>`    | Model from the highest-priority source      |
| `selected_model`          | `String`            | Model identifier actually used              |
| `fallback_used`           | `bool`              | `true` when selected differs from requested |
| `fallback_reason`         | `Option<String>`    | Human-readable fallback explanation         |
| `capabilities`            | `ModelCapabilities` | Capability flags for the selected model     |
| `thinking_mode_requested` | `ThinkingMode`      | Mode requested by caller                    |
| `thinking_mode_selected`  | `ThinkingMode`      | Effective mode after capability adjustment  |
| `metadata_source`         | `MetadataSource`    | Source of capability data                   |
| `diagnostics`             | `Diagnostics`       | Warnings and info from resolution           |

`MetadataSource` has three variants: `Remote` (live API response), `Static`
(compiled-in table), and `Degraded` (fallback because the remote call failed).

`ResolvedModel` is persisted in workspace state under the run's unique
identifier and is embedded in report envelopes so that every artefact carries a
complete record of the model selection decisions that produced it.

## Authentication System

The `src/auth/` module provides a unified interface for credential management
across all four providers.

### SecretStore Trait (`src/auth/store.rs`)

`SecretStore` is the interface for named secret storage backends.
Implementations must never log or include secret values in error messages.

| Method                                      | Description                                  |
| ------------------------------------------- | -------------------------------------------- |
| `service_name() -> &str`                    | Returns the service or namespace identifier  |
| `get_secret(key) -> Result<Option<String>>` | Returns value or `None` when absent          |
| `set_secret(key, value) -> Result<()>`      | Stores value, replacing any existing entry   |
| `delete_secret(key) -> Result<()>`          | Deletes entry; succeeds silently when absent |

### KeyringStore

`KeyringStore` delegates to the `keyring` crate, which uses the operating
system's native secure storage:

- macOS: system Keychain via the `apple-native` feature.
- Windows: Windows Credential Manager via `windows-native`.
- Linux: Secret Service (e.g. GNOME Keyring or KWallet).

`KeyringStore::new(service)` takes the service name string, for example
`"xzardgz-openai"`. All retrieval failures, including "entry not found", are
mapped to `Ok(None)` so that callers do not need to distinguish backend errors
from missing credentials. All delete failures are similarly swallowed and
treated as success.

### EnvVarStore

`EnvVarStore` is a read-only fallback that reads environment variables. The
variable name is constructed by concatenating the prefix and the key, then
uppercasing the result. For example, prefix `"XZARDGZ_"` with key `"api_key"`
reads `XZARDGZ_API_KEY`.

`set_secret` and `delete_secret` always return `PipelineError::Auth` with the
message `"EnvVarStore is read-only"`. This prevents accidental writes that would
expose the secret value in a process listing or shell history.

`EnvVarStore` is the preferred credential source for container-based CI/CD
pipelines where secrets are injected by the platform via environment variables.

### OpenAiAuth (`src/auth/openai.rs`)

`OpenAiAuth` manages the OpenAI API key using the following lookup order:

1. Environment variable named by `api_key_env` (default: `OPENAI_API_KEY`).
2. OS keyring entry under service `"xzardgz-openai"`, key `"api-key"`.

| Constant          | Value            |
| ----------------- | ---------------- |
| `KEYRING_SERVICE` | `xzardgz-openai` |
| `KEYRING_KEY`     | `api-key`        |

Methods: `new(api_key_env)`, `get_key() -> Option<String>`,
`status() -> AuthStatus`, `set_key(key) -> Result<()>`,
`remove_key() -> Result<()>`.

`status()` returns `AuthStatus::CredentialPresent` when a non-empty key is found
in either location, `AuthStatus::NotAuthenticated` when neither location has a
key, and `AuthStatus::Unknown` when the keyring backend itself returns an error.

### AnthropicAuth (`src/auth/anthropic.rs`)

`AnthropicAuth` follows the same four-method interface as `OpenAiAuth`,
operating on its own keyring service name.

| Constant          | Value               |
| ----------------- | ------------------- |
| `KEYRING_SERVICE` | `xzardgz-anthropic` |
| `KEYRING_KEY`     | `api-key`           |

Lookup order:

1. Environment variable named by `api_key_env` (default: `ANTHROPIC_API_KEY`).
2. OS keyring entry under service `"xzardgz-anthropic"`, key `"api-key"`.

### OllamaAuth (`src/auth/ollama.rs`)

`OllamaAuth` provides host validation only. Ollama requires no credentials.
`status()` unconditionally returns `AuthStatus::CredentialPresent` so that
`ProviderAuthManager::status_all()` can report a uniform status structure for
all providers.

`check_reachable() -> Result<bool>` sends a `GET {host}/api/tags` request:

- `Ok(true)` when the server returns a 2xx response.
- `Ok(false)` on connection refused, timeout, or DNS resolution failure.
- `Err(PipelineError::Provider)` only for unexpected non-network errors such as
  URL parse failures.

### AuthStatus and CredentialSource (`src/auth/types.rs`)

`AuthStatus` has four variants:

| Variant             | Meaning                                                |
| ------------------- | ------------------------------------------------------ |
| `Authenticated`     | Credential present and verified via a live API call    |
| `NotAuthenticated`  | No credential found in any location                    |
| `CredentialPresent` | Credential exists but has not been round-trip verified |
| `Unknown`           | Status check failed for a non-auth reason              |

`Authenticated` and `CredentialPresent` carry a `source: CredentialSource`
field. `NotAuthenticated` and `Unknown` carry a `reason: String` field.

`has_credentials()` returns `true` for `Authenticated` and `CredentialPresent`;
`false` for all other variants.

`summary()` returns a one-line human-readable string safe for log output. It
never contains the credential value.

`CredentialSource` records the origin of a credential without exposing its
value:

| Variant               | Contents                           | Label format      |
| --------------------- | ---------------------------------- | ----------------- |
| `EnvironmentVariable` | `name: String` (the variable name) | `env:NAME`        |
| `Keyring`             | `service: String` (service name)   | `keyring:SERVICE` |

### ProviderAuthManager (`src/auth/mod.rs`)

`ProviderAuthManager` aggregates all per-provider auth helpers and is used by
the `auth` CLI commands.

`from_config(config)` constructs the manager from a `Config` reference, passing
each provider's relevant configuration sub-fields to its helper.

`status_all() -> AllProvidersStatus` calls each helper's `status()` method and
returns an `AllProvidersStatus` struct with fields for `openai`, `anthropic`,
`ollama`, and `copilot`. OpenAI is evaluated first and listed first in all
output per the Phase 9 specification.

Copilot authentication is managed externally via the OAuth device flow in
`src/providers/copilot_auth.rs`. The `copilot` field in `AllProvidersStatus` is
always reported as `AuthStatus::Unknown` with the reason message
`"copilot auth managed separately via OAuth"`.

## Secret Handling Principles

The authentication system enforces the following rules uniformly:

API key values are never written to log output, tracing spans, or error message
strings. When a credential operation fails, error text uses generic phrases such
as `"keyring write error"` or `"credential not found"` without any portion of
the secret value.

`KeyringStore` delegates storage and retrieval to the `keyring` crate. Keys are
encrypted at rest by the OS and are not readable by other users on the same
machine.

`EnvVarStore` is intentionally read-only. `set_secret` and `delete_secret`
return errors, preventing any code path from accidentally writing a secret to a
location that would expose it in a process listing.

`CredentialSource` records the source type and, for environment variable
sources, the variable name. It never records the credential value. This allows
`auth status` to display a useful origin summary such as `"env:OPENAI_API_KEY"`
without revealing the secret.

Output that previously held a secret value is replaced with the literal string
`"(redacted)"` before being passed to any formatting or logging call.

## CLI auth Commands

The `xzardgz auth` subcommand family is implemented in `src/commands/auth.rs`
and provides the user-facing interface to the authentication system.

| Command                      | Action                                                |
| ---------------------------- | ----------------------------------------------------- |
| `auth status`                | Prints `AllProvidersStatus` summary for all providers |
| `auth login <provider>`      | Prints current credential status and usage hints      |
| `auth logout <provider>`     | Removes keyring credential for the provider           |
| `auth validate`              | Shows credential presence without any network call    |
| `auth set-key <provider>`    | Reads key from stdin (no echo) and stores in keyring  |
| `auth remove-key <provider>` | Removes the stored key from the keyring               |

`auth login` for OpenAI and Anthropic prints the current status and instructs
the user to run `auth set-key`. It does not initiate a browser session. For
Copilot, `auth login` initiates the GitHub OAuth device flow via `CopilotAuth`.

`auth validate` is functionally equivalent to `auth status` but explicitly
documents that no outbound network call is made. It is intended for use in
scripts that need to verify credential presence before starting a long run.

`auth set-key` reads one line from stdin with no echo. When stdin is at EOF (for
example in automated tests) the empty input path prints `"No key provided."` and
returns `Ok(())` without writing anything to the keyring.

The entry points are `execute(command)` (uses `Config::default()`) and
`execute_with_config(command, config)` (accepts an explicit config for
programmatic use and testing).

## Integration Points

### ProviderFactory

`ProviderFactory::create_from_config(config) -> Result<Arc<dyn Provider>>` is
the single place in the codebase where concrete provider type names appear. The
dispatch table is:

| `config.provider.default` | Concrete type                  |
| ------------------------- | ------------------------------ |
| `"openai"` or `""`        | `OpenAiProvider`               |
| `"anthropic"`             | `AnthropicProvider`            |
| `"ollama"`                | `OllamaProvider`               |
| `"copilot"`               | `CopilotProvider`              |
| Any other value           | `Err(PipelineError::Provider)` |

All other pipeline code holds `Arc<dyn Provider>` and is fully decoupled from
the concrete type.

`default_provider_name()` returns the static string `"openai"`.

### ModelResolver

`ModelResolver::resolve_with_static(ctx, config, available, provider_default, source)`
is called by the pipeline executor before plugin execution begins. A
`PipelineError` is returned immediately on unsatisfied capability requirements,
ensuring that model mismatches are caught before any tokens are spent.

### Credential Status Pre-Flight

`Provider::credential_status()` is called during the pipeline pre-flight check
immediately after `ProviderFactory::create_from_config`. A result of
`CredentialStatus::Missing` causes the pipeline to emit a user-facing error
message identifying the missing credential and the environment variable or
`auth set-key` command needed to fix it.

### Workspace State Persistence

`ResolvedModel` is written to workspace state under the current run's unique
identifier after every successful resolution pass. It is also embedded in report
envelopes so that every artefact produced by a run carries a complete record of
the model and provider selection decisions.

## Testing Approach

Each component has a `#[cfg(test)]` module with tests named using the
`test_<function>_<condition>_<expected>` convention.

### Provider Trait Tests

`base.rs` tests use the generated `MockProvider` to verify that each trait
method behaves correctly in isolation. Tests cover `provider_name`,
`supports_thinking`, `metadata`, `credential_status` (all three variants),
`list_models` (success, empty list, error propagation), and `complete` ( success
and `Auth` error propagation).

### Provider Implementation Tests

Each provider module tests `from_config`, `provider_name`, `supports_thinking`,
`metadata` field values, `credential_status` using `temp_env` to inject and
remove environment variables, and the capability inference function with
representative and edge-case model IDs.

`OpenAiProvider` additionally tests insecure endpoint rejection and the
`allow_insecure_endpoint` override flag.

### Capability Inference Tests

`infer_openai_capabilities` tests cover GPT-4o (tools, no thinking), o-series
(thinking, large context), o1 (no streaming), vision models, and embedding
models (no tools). `infer_anthropic_capabilities` tests cover claude-3.5-sonnet
(thinking), claude-3-haiku (no thinking), claude-4 (thinking), and unknown
models (conservative defaults). `infer_ollama_capabilities` tests cover llama3
(tools), llava (vision), qwen (tools), tag stripping, and unknown models.

### Model Resolver Tests

`model_resolution.rs` tests verify the complete provider precedence chain (CLI
beats watcher, watcher beats workflow, and so on), the model precedence chain,
fallback population when the requested model is unavailable, and
`allow_degraded_metadata` behaviour. All six `ThinkingMode` variants are tested
against both thinking-capable and non-thinking models.

### Auth Store Tests

`store.rs` tests verify `service_name` round-trips, `EnvVarStore` env-var-name
uppercasing, `EnvVarStore::get_secret` with and without the target variable set
using `temp_env`, and `EnvVarStore` write-rejection errors. `KeyringStore`
service name tests confirm that the service string stored at construction is
returned unchanged.

### Auth Manager Tests

`mod.rs` tests verify that `status_all()` returns non-panicking results for all
four providers, that Ollama always reports `CredentialPresent`, and that Copilot
always reports `Unknown`.

### Factory Tests

`factory.rs` tests confirm that each provider name string produces the correct
concrete type, that an empty provider string selects OpenAI, and that an unknown
name returns `Err(PipelineError::Provider)` with an error message containing the
unknown name.
