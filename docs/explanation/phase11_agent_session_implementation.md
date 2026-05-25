# Phase 11 Agent Session Implementation

## Overview

Phase 11 adds `AgentContext` and `AgentSession` to the `src/agent/` layer.
`AgentContext` extends the existing `ConversationContext` with session-scoped
metadata. `AgentSession` replaces the ad-hoc loop in `Agent` and `AgentExecutor`
with a structured, testable session type that enforces a turn budget, writes
JSONL transcripts, and records per-tool failure counts.

## Files Changed

| File                                                       | Change                                                |
| ---------------------------------------------------------- | ----------------------------------------------------- |
| `src/agent/context.rs`                                     | Added `AgentContext` struct, impl, and unit tests     |
| `src/agent/session.rs`                                     | New file: `AgentSession` struct, impl, and unit tests |
| `src/agent/mod.rs`                                         | Added `pub mod session;`                              |
| `docs/explanation/phase11_agent_session_implementation.md` | This document                                         |

## Design Decisions

### AgentContext vs ConversationContext

`ConversationContext` stores only the message window and token budget. It
remains unchanged and is still used by the legacy `Agent` and `AgentExecutor`
types.

`AgentContext` adds session-scoped metadata that plugins, tools, and the
orchestration layer need:

- `workspace_id` and `workspace_root` for tool sandbox scoping
- `scan_artifact_path` for scan result access
- `plugin_metadata` for plugin-specific key-value data
- `provider_metadata` for capability checks without re-fetching
- `trace_enabled` and `step_id` for transcript namespacing

The builder pattern (`with_workspace`, `with_scan_artifact`, etc.) keeps
construction readable and allows partial initialization without exposing mutable
setters.

### AgentSession Loop Design

The main `run` method:

1. Checks `provider.metadata().capabilities.tools` once at entry. Providers that
   do not support tool calling are routed to `run_single_turn`, which avoids
   sending an empty tools list in the request body.

2. Adds the user message to context and persists it to the transcript before
   entering the loop. This ensures the transcript contains the full
   conversation, not just assistant turns.

3. Iterates up to `max_turns`. Each turn:

   - Snapshots messages and tools from the context while briefly holding the
     mutex, then releases it before the async provider call.
   - Dispatches tool calls, routing both `Ok(result)` with a non-None `error`
     field and `Err(e)` cases through `record_tool_failure` so per-tool failure
     counts accumulate consistently.
   - Appends tool result messages to context and transcript before continuing.
   - Returns the assistant content on the first turn with no tool calls.

4. Returns `PipelineError::Agent("max turns reached")` if the loop exhausts
   `max_turns` without a terminal response.

### Transcript Format

Each message is serialized as a compact JSON object on its own line (JSONL). The
file is opened in append mode on every write so that multiple calls to `run` on
the same session path accumulate without truncating prior content. Serialization
uses `serde_json::to_string`, which relies on the `Serialize` derives already
present on `Message`, `Role`, `ToolCall`, and `FunctionCall`.

### Failure Tracking

`record_tool_failure` stores counts in a `Mutex<HashMap<String, usize>>`. The
`Mutex` is used rather than an atomic because the count is read-back for the
return value and because the map requires interior mutability over multiple
keys. The threshold warning uses `tracing::warn!` with structured fields so log
aggregation systems can filter on `tool` and `count`.

### Lock Strategy

Every lock on `self.context` and `self.failure_counts` is held for the minimum
scope: only for the duration of the mutation or snapshot, never across an
`.await` point. This prevents deadlocks and avoids holding locks during IO or
network calls.

## Token Estimation

`AgentContext::current_tokens` uses the same heuristic as `ConversationContext`:
total content bytes divided by four. This is a rough approximation suitable for
budget enforcement but not for precise billing or context-window management.

## Test Coverage

### context.rs tests

| Test                                                  | Scenario                                     |
| ----------------------------------------------------- | -------------------------------------------- |
| `test_agent_context_new_has_empty_messages`           | All fields at default after construction     |
| `test_agent_context_with_workspace_sets_fields`       | Builder sets workspace_id and workspace_root |
| `test_agent_context_with_scan_artifact_sets_path`     | Builder sets scan_artifact_path              |
| `test_agent_context_add_message_appends`              | Messages accumulate in order                 |
| `test_agent_context_compact_if_needed_trims_messages` | Oldest message removed when over budget      |
| `test_agent_context_with_trace_sets_flag`             | trace_enabled toggled correctly              |

### session.rs tests

| Test                                                                   | Scenario                                 |
| ---------------------------------------------------------------------- | ---------------------------------------- |
| `test_run_returns_text_response_when_no_tool_calls`                    | Happy path, no tool dispatch             |
| `test_run_uses_single_turn_fallback_when_provider_has_no_tool_support` | Single-turn path when tools=false        |
| `test_run_reaches_max_turns_and_returns_error`                         | Turn budget exhaustion returns error     |
| `test_run_tool_error_continues_session_without_abort`                  | Tool errors do not abort session         |
| `test_with_transcript_writes_jsonl_file`                               | Transcript written with valid JSON lines |
| `test_record_tool_failure_increments_count`                            | Per-tool counts and cross-tool isolation |

All tests use `MockProvider` generated by `mockall::automock` on the `Provider`
trait. The `ToolRegistry` is empty in most tests; unknown tool names cause the
dispatcher to return `PipelineError::Tool`, which `AgentSession` handles
gracefully by recording the failure and continuing the loop.
