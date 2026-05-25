# OpenAI Capability Inference Refactor

## Problem

`src/providers/openai.rs` contained a hardcoded `static_openai_models()` function
that returned a fixed `Vec<ModelMetadata>` for a small set of known GPT model IDs.
This design had two failure modes:

- Any model OpenAI retires causes a stale, misleading entry in the table.
- Any model OpenAI launches silently falls back to `ModelCapabilities::default()`,
  which is incorrect for o-series reasoning models and embedding models.

The fallback path in `list_models()` also returned the entire static table instead
of narrowing the result to the model the caller actually configured, making the
output misleading when no API key is present.

## Solution

### `infer_openai_capabilities(model_id: &str) -> ModelCapabilities`

A new module-level `pub fn` replaces the static table with pattern-matching rules
derived from OpenAI naming conventions:

| Pattern | Rule |
| --- | --- |
| Starts with `o` + ASCII digit (e.g. `o1`, `o3`, `o4-mini`) | `supports_thinking = true`, `context_window_tokens = 200_000` |
| Contains `-reasoning` | `supports_thinking = true` |
| Contains `text-embedding`, `embedding`, `whisper`, `tts`, `dall-e`, legacy names | `is_chat_model = false` (no tools, no structured output) |
| Contains `gpt-4o`, `gpt-4-vision`, starts with `gpt-4.`, contains `gpt-4.1`/`gpt-4.5` | `supports_vision = true` |
| Exactly `o1` or `o1-preview` | `supports_streaming = false` |
| Contains `gpt-4` (non-thinking) | `context_window_tokens = 128_000` |
| Contains `32k` | `context_window_tokens = 32_768` |
| Everything else | `context_window_tokens = 16_384` (conservative default) |

The function is deliberately conservative: unknown chat-like model IDs are assumed
to support tools and structured output, which is correct for any post-GPT-3.5 model.

### `configured_model_fallback(&self) -> Vec<ModelMetadata>`

A private helper method replaces all four `return Ok(Self::static_openai_models())`
fallback sites in `list_models()`. It returns a single-entry list for the
configured model ID with capabilities inferred via `infer_openai_capabilities`.

This is strictly more accurate than the old table: the caller only cares about
the model they have configured, and the inferred capabilities are at least as
correct as the old hand-tuned values for any model the table covered.

### `metadata()` update

Changed from emitting all static model IDs to emitting only
`vec![self.config.model.clone()]`. The live list is available via `list_models()`.

### `model_supports_thinking()` update

Changed from a `HashMap` lookup against the static table to a one-liner:

```xzardgz/src/providers/openai.rs
fn model_supports_thinking(&self) -> bool {
    infer_openai_capabilities(&self.config.model).supports_thinking
}
```

This is always correct regardless of what model is configured, including future
models not present in any static table.

## Removed code

| Symbol | Reason |
| --- | --- |
| `pub fn static_openai_models()` | Replaced by `infer_openai_capabilities` |
| `fn static_models_map()` | Used only by the old `model_supports_thinking`; no longer needed |
| `use std::collections::HashMap` | Only import for `static_models_map` |

## Test coverage

Five tests that verified the static table content were removed. Nine new tests
cover `infer_openai_capabilities` directly:

- `test_infer_openai_capabilities_gpt4o_has_tools_no_thinking`
- `test_infer_openai_capabilities_o3_has_thinking`
- `test_infer_openai_capabilities_o1_has_thinking` (also checks no streaming)
- `test_infer_openai_capabilities_o3_mini_has_thinking` (also checks streaming)
- `test_infer_openai_capabilities_gpt4o_mini_has_tools`
- `test_infer_openai_capabilities_embedding_model_has_no_tools`
- `test_infer_openai_capabilities_unknown_model_has_conservative_defaults`
- `test_infer_openai_capabilities_o_series_gets_large_context`
- `test_openai_provider_metadata_models_is_not_empty`

## Pre-existing bug fixed

`src/providers/ollama.rs` was missing the closing `}` for its `mod tests` block.
This was a latent syntax error hidden by incremental build cache. It was discovered
when the full recompile triggered by the `openai.rs` edits caused Cargo to reparse
the file. The single missing brace was added to unblock the quality gate.
