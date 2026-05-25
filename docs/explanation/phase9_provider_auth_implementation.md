# Phase 9: Provider Abstraction and Authentication

## Overview

Phase 9 implements the full provider abstraction layer for XZardgz. OpenAI is
the default and primary provider. The phase delivers an expanded `Provider`
trait, four concrete provider implementations (OpenAI, Anthropic, Ollama, and
Copilot), a model capability resolver, and a multi-provider authentication
system housed in `src/auth/`.

The goal is to make the AI backend entirely interchangeable at runtime. A single
configuration key (`provider.default`) or a CLI flag (`--provider`) selects
which backend executes requests. All pipeline code above the provider layer is
unaware of which concrete implementation is in use.

## Provider Trait

The expanded `Provider` trait lives in `src/providers/base.rs`. Every provider
implementation must satisfy the following interface.

`provider_name()` returns the canonical lowercase string name for the provider
(for example, `"openai"` or `"anthropic"`). This name is used in configuration
keys, log messages, and the `CredentialSource` record.

`metadata()` returns static capability metadata for the provider as a whole.
This is a synchronous, infallible call used by the resolver before any network
activity.

`supports_thinking()` returns `true` when at least one of the provider's models
is capable of extended reasoning. This is checked by the model resolver when the
caller requests a `ThinkingMode` other than `None`.

`credential_status()` returns a `CredentialStatus` value describing whether
valid-looking credentials are present. This method never makes an API call; it
inspects the keyring and environment only. It is used by the pipeline's
pre-flight check to produce user-facing diagnostics before the first request is
sent.

`list_models()` returns a list of available model identifiers. For providers
with a reachable `/v1/models` (or equivalent) endpoint the list is fetched live;
for others a static table is returned. Errors during the live fetch fall back to
the static table rather than propagating.

`complete()` sends a standard chat completion request and returns the full
response once it is available.

`complete_with_thinking()` sends a thinking-aware chat completion request. The
default implementation provided by the trait simply delegates to `complete()`,
so providers that do not support thinking do not need to override this method.

`complete_streaming()` sends a chat completion request and returns a stream of
partial response chunks. Providers that cannot stream may return an error or
emit a single chunk containing the full response.

### Error Convention

All provider methods return `Result<_, PipelineError>`. Authentication failures
(HTTP 401 or 403) map to `PipelineError::Auth`. All other API errors, network
failures, and serialisation problems map to `PipelineError::Provider`. This
distinction allows the pipeline to surface actionable credential hints
separately from transient network problems.

## ThinkingMode Enum

`ThinkingMode` controls how the provider allocates reasoning token budget before
composing its response. It is set per-request by the model resolver and passed
through to `complete_with_thinking()`.

| Variant   | Behavior                                                 | Budget Tokens |
| --------- | -------------------------------------------------------- | ------------- |
| None      | Thinking is never requested                              | n/a           |
| Auto      | Thinking requested only when model supports it; uses Low | auto          |
| Low       | Explicit low thinking budget                             | 2 000         |
| Medium    | Explicit medium thinking budget                          | 8 000         |
| High      | Explicit high thinking budget                            | 16 000        |
| ExtraHigh | Explicit maximum thinking budget                         | 32 000        |

`Auto` never hard-fails on unsupported models. When a model does not declare
thinking support the resolver silently downgrades the effective mode to `None`
and records a `DiagnosticLevel::Info` entry in `ResolvedModel.diagnostics`. The
user sees an informational note rather than a fatal error.

Explicit levels (`Low` and above) follow a stricter policy. When
`allow_degraded_metadata` is `false` in `ModelMetadataConfig` and the selected
model does not support thinking, the resolver returns `PipelineError::Provider`
with a descriptive message. When `allow_degraded_metadata` is `true` the
resolver degrades to `None` and records a `DiagnosticLevel::Warning` entry.

## Provider Implementations

### OpenAI (`src/providers/openai.rs`)

OpenAI is the default provider. The configuration key
`provider.default = "openai"` selects it, and it is also the hard-coded fallback
when no provider is specified anywhere in the resolution chain. The
implementation supports any OpenAI-compatible endpoint, including Azure OpenAI
Service, LocalAI, and other drop-in replacements, by reading the base URL from
`provider.openai.base_url`. Endpoint security is enforced by default: the URL
must begin with `https://` unless `allow_insecure_endpoint = true` is set in the
provider configuration. The static model table includes `gpt-4.1`, There is no
hardcoded model list. The implementation always calls `GET {endpoint}/v1/models`
with the configured API key. Capabilities for each returned model ID are derived
by `infer_openai_capabilities()`, a pure function that applies pattern-based
rules: o-series names (`o1`, `o3`, `o4`, etc.) receive
`supports_thinking = true` and a 200 000-token context window; GPT-4 names
receive a 128 000-token window; non-chat model names (embeddings, Whisper,
DALL-E) receive `supports_tools = false`. When the live endpoint is unreachable,
credentials are absent, or the response cannot be parsed, the fallback is a
single-entry list for the currently configured model with inferred capabilities
— not a frozen list of specific model IDs.

### Anthropic (`src/providers/anthropic.rs`)

The Anthropic implementation targets `https://api.anthropic.com/v1/messages`
using the `x-api-key` authentication header and the `anthropic-version` header
required by the API. Thinking is supported on `claude-opus-4-5` and
`claude-3-5-sonnet-latest` via a `thinking` block in the request body.
System-role messages are extracted from the conversation and sent as the
top-level `system` field rather than embedded in the `messages` array, as
required by the Anthropic message format. Tool definitions use Anthropic's
`input_schema` field instead of OpenAI's `parameters` field. Capabilities are
derived by `infer_anthropic_capabilities()` at query time; the Anthropic models
endpoint (`GET https://api.anthropic.com/v1/models`) is called dynamically, and
on any failure the fallback is the single configured model with inferred
capabilities.

### Ollama (`src/providers/ollama.rs`)

Ollama targets a locally running inference server and requires no credentials.
The `credential_status()` method always returns `CredentialStatus::NotRequired`.
The model list is fetched live from the `/api/tags` endpoint; if that endpoint
is unreachable the list falls back to a single-element list containing the
currently configured model. Thinking is not supported by any Ollama model at
this time, so `supports_thinking()` returns `false` unconditionally.

### Copilot (`src/providers/copilot.rs`)

The Copilot provider authenticates via the GitHub OAuth device flow, which is
managed by a dedicated module at `src/providers/copilot_auth.rs`. The device
flow exchanges a device code for a user access token that is stored in the
keyring and refreshed automatically on expiry. No thinking support is provided.
The model list is static and reflects the models exposed through the GitHub
Copilot API at the time of implementation.

## Dynamic Capability Inference

No hardcoded model lists exist anywhere in the codebase. Every provider fetches
its model list from the live API at runtime and derives capabilities using a
pure inference function.

### OpenAI — `infer_openai_capabilities(model_id)`

| Pattern (lowercased ID)                                  | Capability inferred                               |
| -------------------------------------------------------- | ------------------------------------------------- |
| Starts with `o` + digit (`o1`, `o3`, `o4`, `o3-mini`...) | `supports_thinking = true`, 200 000-token context |
| Contains `gpt-4`                                         | 128 000-token context window                      |
| Contains `embedding`, `whisper`, `tts`, `dall-e`         | `supports_tools = false`                          |
| Exactly `o1` or `o1-preview`                             | `supports_streaming = false`                      |
| Contains `gpt-4o`, `gpt-4-vision`, `gpt-4.1`, `gpt-4.5`  | `supports_vision = true`                          |
| Anything else                                            | chat model, 16 384-token context                  |

### Anthropic — `infer_anthropic_capabilities(model_id)`

| Pattern (lowercased ID)                                                 | Capability inferred                               |
| ----------------------------------------------------------------------- | ------------------------------------------------- |
| Contains `claude-3`, `claude-4`, `claude-opus-4`, `claude-sonnet-4`     | `supports_tools = true`, `supports_vision = true` |
| Contains `claude-3-5`, `claude-3-7`, `claude-4`, or `claude-3` + `opus` | `supports_thinking = true`                        |
| Modern model (claude-3+)                                                | 200 000-token context window                      |

### Ollama — `infer_ollama_capabilities(model_id)`

The tag suffix (`:latest`, `:7b`) is stripped before matching.

| Pattern (base name)                                                 | Capability inferred      |
| ------------------------------------------------------------------- | ------------------------ |
| `mistral`, `llama3*`, `qwen`, `gemma2/3`, `mixtral`, `phi3/4`, etc. | `supports_tools = true`  |
| `llava`, `bakllava`, `vision`, `minicpm-v`, `moondream`, `cogvlm`   | `supports_vision = true` |
| Everything else                                                     | Conservative defaults    |

All Ollama models receive `context_window_tokens = 32_768`. The actual context
limit is controlled by Ollama's `num_ctx` parameter at runtime.

All inference functions are conservative: when a model name is ambiguous the
function returns `false` for uncertain capability flags.

## Model Capability Resolver

`ModelResolver` lives in `src/providers/model_resolution.rs`. Its responsibility
is to map a potentially underspecified (`provider`, `model`) pair — drawn from
multiple competing sources — to a fully resolved `ResolvedModel` that includes
verified capability flags and a record of any fallbacks that were applied.

### Provider Precedence (7 levels)

The resolver walks the following sources in order, stopping at the first
non-empty value:

1. CLI `--provider` flag
2. Watcher task `provider` field
3. Workflow plan provider
4. Plugin-specific provider override
5. `config.provider.default`
6. `"openai"` (hard-coded fallback)

### Model Precedence (7 levels)

Within the selected provider, the model is resolved by the same priority
descent:

1. CLI `--model` flag
2. Watcher task `model` field
3. Workflow plan model
4. Plugin-specific model override
5. `config.model_selection.preferred_models[0]`
6. Provider-specific config default model
7. First compatible model from the static or live model list

### ResolvedModel Fields

`ResolvedModel` is the output of a successful resolution pass. Its fields are:

- `requested_provider` — the provider string from the highest-priority source,
  or empty if none was specified.
- `selected_provider` — the provider name actually used after fallback.
- `requested_model` — the model string from the highest-priority source, or
  empty if none was specified.
- `selected_model` — the model identifier actually used after fallback.
- `fallback_used` — `true` when the selected provider or model differs from what
  was requested.
- `fallback_reason` — a human-readable explanation when `fallback_used` is
  `true`.
- `capabilities` — a copy of the capability flags for `selected_model`.
- `thinking_mode_requested` — the `ThinkingMode` value passed by the caller.
- `thinking_mode_selected` — the effective `ThinkingMode` after capability
  checking and possible downgrade.
- `metadata_source` — indicates whether capabilities came from a live API call
  or the static table.
- `diagnostics` — accumulated informational or warning entries describing
  decisions made during resolution.

`ResolvedModel` is persisted in workspace state so that subsequent pipeline
stages (including post-run reporting) can reference exactly which model was used
without repeating the resolution logic. It is also embedded in report envelopes
and watcher result messages so that downstream consumers can correlate output
with the model that produced it.

## Authentication System

The `src/auth/` module provides a unified interface for credential management
across all four providers.

### Module Structure

`types.rs` defines the shared vocabulary used throughout the auth system.
`AuthStatus` has four variants: `Authenticated` (a credential is present and has
been verified via a successful API call), `NotAuthenticated` (no credential was
found), `CredentialPresent` (a credential exists but has not been round-trip
verified), and `Unknown` (the check could not be completed). `CredentialSource`
records which mechanism supplied the credential — keyring or environment
variable — and, for environment variables, records the variable name. The value
itself is never stored in `CredentialSource`. `AllProvidersStatus` is a struct
that aggregates the `AuthStatus` for all four providers and is serialised
directly into the output of `auth status`.

`store.rs` defines the `SecretStore` trait with `get()`, `set()`, and `remove()`
methods. `KeyringStore` is the primary implementation, backed by the `keyring`
crate, which delegates to the OS-level secret storage mechanism (Keychain on
macOS, the Secret Service on Linux, and the Windows Credential Manager on
Windows). `EnvVarStore` is a read-only fallback that reads a named environment
variable; `set()` and `remove()` on `EnvVarStore` return
`Err(PipelineError::Auth(...))` because environment variables cannot be mutated
at runtime in a portable way. `EnvVarStore` is intended for CI/CD environments
where credentials are injected by the platform.

`openai.rs` provides `OpenAiAuth` with four methods: `status()` returns a
`CredentialStatus`, `get_key()` retrieves the raw key string, `set_key()` writes
to the keyring, and `remove_key()` deletes the keyring entry.

`anthropic.rs` provides `AnthropicAuth` with the same four-method interface as
`OpenAiAuth`, operating on a separate service name in the keyring.

`ollama.rs` provides `OllamaAuth`. Its `status()` always returns
`CredentialStatus::NotRequired` because Ollama does not use API keys.
`check_reachable()` is an async method that sends a lightweight request to the
configured Ollama base URL and returns `Ok(())` on success or a
`PipelineError::Provider` on failure.

`mod.rs` provides `ProviderAuthManager`. `from_config()` constructs the manager
from a `Config` reference. `status_all()` returns an `AllProvidersStatus` by
calling each provider's credential check in declaration order: OpenAI,
Anthropic, Ollama, Copilot. OpenAI appears first in the output per the Phase 9
specification.

## Secret Handling

The auth system enforces strict rules about the visibility of credential
material.

API keys are never written to log output, tracing spans, or error message
strings. When a credential operation fails the error text uses phrases such as
"credential not found" or "keyring access failed" without including the key
value or any prefix or suffix of it.

`KeyringStore` delegates all storage and retrieval to the `keyring` crate. The
crate uses the operating system's native secret storage, which means keys are
encrypted at rest and are not accessible to other users on the same machine.

`EnvVarStore` provides a read-only fallback path for environments where
injecting secrets via environment variables is the standard practice, such as
container-based CI/CD pipelines. Because it is read-only, accidental writes that
would surface the key in a process list or shell history are impossible.

`CredentialSource` records the source type and, for environment variable
sources, the variable name. It never records the credential value. This allows
the `auth status` command to display a useful origin summary (for example, "from
environment variable OPENAI_API_KEY") without revealing the secret.

Redacted output is used consistently in log messages and error reports: any code
path that previously held a secret value replaces it with the literal string
`"(redacted)"` before passing the value to any formatting or logging call.

## CLI auth Commands

The `xzardgz auth` subcommand family provides the user-facing interface to the
authentication system. Each subcommand maps to one or more calls into
`ProviderAuthManager` or the provider-specific auth structs.

| Command                      | Action                                  |
| ---------------------------- | --------------------------------------- |
| `auth status`                | Shows `AllProvidersStatus` summary      |
| `auth login <provider>`      | Shows current status and hints          |
| `auth logout <provider>`     | Removes keyring credential              |
| `auth validate`              | Shows credential presence (no API call) |
| `auth set-key <provider>`    | Reads key from stdin, stores in keyring |
| `auth remove-key <provider>` | Removes key from keyring                |

`auth status` calls `ProviderAuthManager::status_all()` and prints a formatted
table. `auth login` does not initiate a browser or device-flow session for most
providers; it prints the current credential status and the environment variable
or `auth set-key` command the user should use to provide credentials. For the
Copilot provider, `auth login` initiates the GitHub OAuth device flow.
`auth validate` is similar to `auth status` but explicitly documents that no
outbound network call is made: it only inspects the keyring and environment.

## Integration Points

The auth system and provider layer connect to the rest of the pipeline at
several well-defined seams.

`ProviderFactory::create_from_config()` reads `config.provider.default` and
instantiates the corresponding concrete provider. The factory is the only place
in the codebase where the concrete provider type names appear; all other code
holds a `Box<dyn Provider>`.

`ModelResolver::resolve_with_static()` is called by the pipeline executor before
any plugin execution begins. It returns a `ResolvedModel` or a `PipelineError`.
This ensures that model capability mismatches are caught before any token budget
is spent.

`Provider::credential_status()` is called by the pipeline's pre-flight check
immediately after provider construction. If the status is
`CredentialStatus::NotAuthenticated` the pipeline emits a targeted error message
and exits before sending any request.

`ProviderAuthManager::status_all()` is called by the `auth status` CLI command.
No other pipeline path calls this method; it exists solely to serve the
diagnostic commands.

`ResolvedModel` is stored in workspace state under the run's unique identifier.
It is also embedded in the report envelope so that every artefact produced by a
run carries a complete record of the model selection decisions that led to its
creation.

## Testing Approach

Each component in Phase 9 has a dedicated `#[cfg(test)]` module following the
`test_<function>_<condition>_<expected>` naming convention.

Provider implementation tests cover the static model tables (verifying that
every declared model has consistent capability flags), `credential_status()`
behaviour using `temp_env` to inject and remove environment variables without
affecting the test process, and endpoint validation logic confirming that
insecure URLs are rejected unless the override flag is set.

Model resolver tests cover the full 7-level precedence chain for both provider
and model selection. Dedicated tests verify that each level correctly shadows
all lower-priority sources. Fallback tests confirm that `fallback_used` and
`fallback_reason` are populated accurately. Thinking mode resolution tests cover
all six `ThinkingMode` variants against both supporting and non-supporting
models, and verify that `Auto` downgrades silently while explicit levels respect
the `allow_degraded_metadata` flag.

Auth store tests use `temp_env` to exercise `EnvVarStore` retrieval and confirm
that write operations return the expected error. `KeyringStore` tests verify the
service name constant used for each provider so that keyring entries do not
collide between providers or between the application and other software on the
same machine.

`ProviderAuthManager` tests confirm that `status_all()` always returns entries
for all four providers and that the ordering matches the specification (OpenAI
first).

Factory tests confirm that `ProviderFactory::create_from_config()` returns an
OpenAI instance when no provider is specified, that all four provider names
produce a successfully constructed instance, and that an unknown provider name
returns `Err(PipelineError::Provider(...))`.
