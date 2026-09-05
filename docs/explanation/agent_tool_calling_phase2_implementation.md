# Agent Tool-Calling Integration: Phase 2 Implementation

This document records what was actually built for Phase 2 ("Wire AgentSession
into Plugin Execution") of
[`agent_tool_calling_integration_plan.md`](agent_tool_calling_integration_plan.md),
including the reasoning behind each structural decision and how the multi-turn
tool-calling loop is now wired into both shipped plugins.

## Summary

Phase 2 wires `AgentSession` into both built-in plugins (`security-review` and
`technical-review`), replacing the direct single-shot
`Provider::complete(&messages, &[] as &[Tool])` calls. Plugins now drive a
multi-turn, tool-augmented loop that gives the AI provider read access to the
sandboxed repository.

The change is hard-breaking by design: a provider that does not advertise
`capabilities.tools == true` will cause both plugins to return a
`PipelineError::Provider` immediately rather than silently degrading to a
single-shot completion with no tool access.

## Changes Made

### `src/agent/session.rs` -- Provider type widened

`AgentSession.provider` was changed from `Arc<dyn Provider>` to
`Arc<dyn Provider + Send + Sync>` to match the type used in
`PluginContext.provider`. Because `Provider: Send + Sync` is already declared as
a supertrait bound, every concrete implementation already satisfies this
constraint and no callers required any change.

### `src/config.rs` -- `agent_max_turns` added to both plugin configs

Added `pub agent_max_turns: u32` (serialization default: 15) to both
`TechnicalReviewConfig` and `SecurityReviewConfig`. Two helper functions,
`default_agent_max_turns()` and `default_sec_agent_max_turns()`, each returning
`15u32`, were added to supply the `#[serde(default = "...")]` attribute. Both
`Default` implementations were updated to set the field to `15`.

The value 15 gives the model enough turns to read several files, run a grep, and
still produce a response without enabling runaway loops.

### `src/plugins/context.rs` -- `build_agent_session` method

Added a new public method to `PluginContext`:

```rust
pub fn build_agent_session(
    &mut self,
    system_prompt: String,
    max_tokens: usize,
    max_turns: usize,
) -> Result<AgentSession>
```

The method:

- Checks `self.provider.metadata().capabilities.tools`. When `false`, it returns
  `Err(PipelineError::Provider("this provider does not support tool calling"))`
  immediately. This is the hard requirement from section 2.1 of the plan: no
  single-shot fallback.
- Constructs an `AgentContext` pre-seeded with the system message so the
  provider's conversation history begins with the system instruction.
- Takes ownership of `self.tool_registry` via `std::mem::replace`, leaving an
  empty registry in the context. This ensures the session owns all the tools.
- Returns an `AgentSession` configured with `max_turns`.

Two tests were added:

- `test_plugin_context_build_agent_session_returns_err_when_provider_lacks_tools`:
  mock provider advertises `tools: false`; asserts `Err` is returned.
- `test_plugin_context_build_agent_session_returns_ok_when_provider_supports_tools`:
  mock provider advertises `tools: true`; asserts `Ok` is returned.

### `src/plugins/security_review/plugin.rs` -- Replaced provider.complete with AgentSession

In `SecurityReviewPlugin::run`:

- Removed the
  `let messages = vec![...]; ctx.provider.complete(&messages, &[] as &[Tool]).await`
  call.
- Added
  `let session = ctx.build_agent_session(system_prompt, 8192, config.agent_max_turns as usize)?;`
  followed by `session.run(&user_prompt).await` to obtain the response string.
- The `?` on `build_agent_session` propagates `PipelineError::Provider` as a
  hard error when the provider lacks tool support.
- Errors from `session.run()` are caught and returned as
  `PluginOutput::failure`.
- Removed the `use crate::providers::types::{Message, Tool}` import, which is no
  longer needed in the plugin code.

Tests were rewritten to exercise the multi-turn path:

- `test_security_review_plugin_run_with_mock_provider_empty_findings_returns_success`:
  mock sets `expect_metadata()` with `tools: true`, uses a closure counter to
  return a tool-call message on turn 1 and the final JSON on turn 2.
- `test_security_review_plugin_run_with_mock_provider_with_findings_returns_success`:
  same multi-turn setup; final turn returns a finding.
- `test_security_review_plugin_run_provider_error_returns_failure_output`: mock
  returns `tools: true` from `metadata()`, then errors on `complete()`; session
  propagates the error and the plugin returns `PluginOutput::failure`.
- `test_security_review_plugin_run_disabled_returns_success_immediately`: no
  change -- the plugin returns before calling `build_agent_session`.
- Report-writing tests (`_writes_markdown_report`, `_json_report`,
  `_sarif_report`) and `_fail_on_critical_*` tests were updated to the
  multi-turn mock pattern.
- NEW `test_security_review_plugin_run_no_tool_support_returns_hard_error`: mock
  returns `tools: false`; asserts `plugin.run(ctx).await` returns `Err(...)`
  with a message containing "tool calling".
- NEW
  `test_security_review_plugin_run_exercises_multi_turn_tool_call_round_trip`:
  uses a counter closure to assert that `complete()` is called at least twice:
  once for the tool-call turn, and once after the tool result is injected.

### `src/plugins/technical_review/plugin.rs` -- Same pattern as security_review

The identical structural change was applied: the direct `provider.complete` call
was replaced with `ctx.build_agent_session(...)?` followed by
`session.run(...)`, and all tests were rewired with multi-turn mock
expectations. No logic diverges from the security review plugin.

## Success Criteria Met

All criteria from section 2.6 of the plan were met:

- Plugin unit tests exercise at least one multi-turn tool-call round trip.
- Existing end-to-end workflow tests continue to pass.
- Attempting to run either plugin against a provider whose `capabilities.tools`
  is `false` returns
  `Err(PipelineError::Provider("this provider does not support tool calling"))`
  rather than silently degrading.
- Single-shot `provider.complete` call sites are removed from both plugins.

## Verification

Full quality-gate sequence run in the mandated order, all clean:

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

All tests passed, 0 failed.
