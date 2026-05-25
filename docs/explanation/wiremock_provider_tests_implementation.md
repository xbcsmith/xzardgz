# Wiremock Provider Tests Implementation

## Summary

This document explains the mock HTTP server tests added to the Ollama and OpenAI
providers, and the static metadata fallback test added to the model resolution
module. These tests verify provider behaviour without requiring live network
connections or real API credentials.

## Motivation

Provider implementations make HTTP calls to external services. Unit tests for
these call sites previously either avoided testing the HTTP path entirely or
relied on live endpoints that are unavailable in CI. The mock server layer
closes this gap: it intercepts real HTTP connections on the loopback interface,
verifies that the provider sends the expected request shape, and returns
controlled JSON payloads. This means:

- Tests exercise the full serialization and deserialization path.
- Tests are deterministic and work without network access.
- Failure modes (unreachable server, missing credentials) are exercised
  reproducibly.

## Dependency

`wiremock = "0.6"` was added to `[dev-dependencies]` in `Cargo.toml`. WireMock
starts an ephemeral HTTP server on a random loopback port for each test. Because
the server is started with `.await`, each mock server test uses
`#[tokio::test]`.

## Ollama Provider Tests

### test_ollama_list_models_returns_available_models_from_api_tags

Located in `src/providers/ollama.rs`.

A `MockServer` is started and a `GET /api/tags` handler is registered that
returns a two-item model list (`llama3:latest` and `mistral:7b`). An
`OllamaProvider` is constructed with `mock_server.uri()` as the base URL.
`list_models` is called and the returned slice is asserted to contain both model
IDs.

This test confirms:

- The provider issues a `GET` request to the correct path.
- The `OllamaTagsResponse` JSON shape is deserialized correctly.
- Each entry in the `models` array becomes a `ModelMetadata` record with the
  model name as the ID.

### test_ollama_list_models_falls_back_to_static_when_server_unavailable

An `OllamaProvider` is pointed at `http://127.0.0.1:1`, a port that always
refuses connections immediately (connection refused, not a timeout). The test
asserts that `list_models` returns `Ok` with a non-empty slice rather than
propagating the error. This validates the graceful degradation branch in
`list_models` that catches `reqwest` send errors and falls back to the
single-item configured-model list.

## OpenAI Provider Tests

### test_openai_list_models_returns_available_models_from_models_endpoint

Located in `src/providers/openai.rs`.

A `MockServer` is started with a `GET /v1/models` handler that requires an
`Authorization` header and returns three model entries. The `OpenAiConfig`
endpoint is set to `format!("{}/v1", mock_server.uri())` so that the URL
constructed inside `list_models` (`format!("{}/models", endpoint)`) resolves to
`/v1/models` on the mock server. `allow_insecure_endpoint` is set to `true`
because the mock server uses plain HTTP.

A test-only environment variable (`XZARDGZ_TEST_OAI_MOCK_KEY`) is set using an
`unsafe` block (required in Rust 2024 edition, where `std::env::set_var` is
unsafe) and removed after the provider call so cleanup happens even if
assertions fail. The test confirms that all three model IDs appear in the
returned slice.

### test_openai_list_models_falls_back_to_static_when_no_api_key

A synchronous `#[test]` that mirrors the pattern used by existing credential
tests in the module. `temp_env::with_var` ensures the API key environment
variable is absent for the duration of the closure. A new
`tokio::runtime::Runtime` is created inside the closure to drive the async
`list_models` call. The test asserts that the call succeeds and returns a
non-empty slice, confirming the early-return static fallback branch that fires
when no API key is present.

## Model Resolution Test

### test_resolve_with_static_uses_static_metadata_when_remote_unavailable_and_degraded_allowed

Located in `src/providers/model_resolution.rs`.

`ModelResolver::resolve_with_static` accepts a `MetadataSource` parameter that
callers supply to record whether metadata came from a live API or a static
fallback. This test passes `MetadataSource::Degraded` (the value providers use
when the remote endpoint was unreachable) and verifies that:

- The call succeeds (the model is found in the available slice).
- `result.metadata_source` equals `MetadataSource::Degraded`, confirming the
  resolver propagates the source value unchanged.
- `result.fallback_used` is `false`, confirming no model substitution occurred.

`config.model_metadata.allow_degraded_metadata` is set to `true` to reflect the
realistic configuration under which a caller would choose `Degraded` over
returning an error.

## Design Decisions

### Endpoint URL structure for OpenAI

The OpenAI `list_models` implementation appends `/models` to the configured
`endpoint` field. Because the real default endpoint is
`https://api.openai.com/v1`, the final URL is
`https://api.openai.com/v1/models`. Mock tests therefore set `endpoint` to
`format!("{}/v1", mock_server.uri())` so the assembled URL path is `/v1/models`,
matching the mounted mock path exactly.

### Environment variable safety

`std::env::set_var` and `std::env::remove_var` are `unsafe` in Rust 2024 edition
due to potential data races in multithreaded processes. All usages in the new
tests are guarded by `unsafe` blocks with SAFETY comments that explain the
invariant: each test uses a uniquely named environment variable that no other
test reads, eliminating the data race.

### Sync fallback test pattern

The no-API-key fallback test is a synchronous `#[test]` rather than
`#[tokio::test]`. This follows the convention established by the existing
credential-status tests in `openai.rs` and avoids nesting async runtimes.
`temp-env 0.3.6` does not include async support (its only dependency is
`parking_lot`), so an explicit `tokio::runtime::Runtime::new()` is used inside
the `temp_env::with_var` closure.
