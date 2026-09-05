# Agent Tool-Calling Integration: Phase 1 Implementation

This document records what was actually built for Phase 1 ("Consolidate the
Agent Layer") of
[`agent_tool_calling_integration_plan.md`](agent_tool_calling_integration_plan.md),
including the reasoning behind each deletion and what the canonical agent loop
looks like after the cleanup.

## Summary

Phase 1's goal was to eliminate duplicate, unused agent-loop implementations
from `src/agent/` and leave a single, well-understood entry point. Before
removing anything, every candidate struct was confirmed to have zero external
call sites in `src/` or `tests/` -- only self-references within each file's own
`impl` block and test module. That confirmation was made via `grep` before any
file was touched.

Two files were deleted in full: `src/agent/core.rs` (containing `Agent`) and
`src/agent/executor.rs` (containing `AgentExecutor`). Neither had any external
callers, so no other files required updating. The `src/agent/mod.rs` module
manifest was trimmed to remove the two dead declarations and was given a
module-level doc comment naming `AgentSession` as the sole loop implementation.
All remaining files -- `session.rs`, `context.rs`, `message.rs`, and `state.rs`
-- were left entirely unchanged.

## What was deleted and why

### `src/agent/core.rs` -- `Agent`

`Agent` was a multi-turn tool-calling loop that:

- held conversation history behind a `Mutex<ConversationContext>`,
- ran up to a hard-coded maximum of five iterations with no override surface,
- had no mechanism for persisting transcripts or tracking per-tool failure
  counts.

It was confirmed to be an unused prototype: `grep` found no reference to `Agent`
(other than the struct definition itself and its own `impl` blocks) anywhere in
`src/` or `tests/`. The entire file was deleted.

### `src/agent/executor.rs` -- `AgentExecutor`

`AgentExecutor` was a second multi-turn tool-calling loop that:

- also hard-coded a cap of five iterations,
- carried a latent correctness bug: its call to `complete` always passed an
  empty tool list (`vec![]`) regardless of what the tool registry contained,
  meaning the model could never observe or call any registered tool,
- had no transcript persistence or per-tool failure tracking.

It was also confirmed to be an unused prototype by `grep`. The entire file was
deleted.

The bug is noted here not because it required a fix (the code is gone) but
because it illustrates why a dead code path with a subtle logic error is more
dangerous than no code at all: it could have been reached by accident and
silently produced incorrect behaviour.

## What was retained unchanged

### `src/agent/session.rs` -- `AgentSession`

`AgentSession` is the canonical agent loop. It differs from the deleted
prototypes in every dimension that matters:

- `max_turns` is configurable at construction time; there is no hard-coded
  constant.
- Each turn's request and response are appended to a JSONL transcript file,
  giving an auditable record of every conversation.
- The session tracks consecutive failures per tool name and surfaces that
  information to callers.
- The tool list passed to `complete` is drawn from the live registry, not an
  empty placeholder.

Nothing in `AgentSession` was changed.

### Supporting types

`src/agent/context.rs`, `src/agent/message.rs`, and `src/agent/state.rs` provide
the data types that `AgentSession` and its tests depend on. They are not loop
implementations; they were not changed.

## Changes to `src/agent/mod.rs`

Two lines were removed:

```rust
pub mod core;
pub mod executor;
```

A module-level doc comment was added asserting `AgentSession` (in `session.rs`)
as the sole agent-loop implementation and explaining that `context.rs`,
`message.rs`, and `state.rs` are supporting types, not loop implementations.
This makes the intent explicit to any future reader without requiring them to
diff against a plan document.

## Call-site impact

None. Because neither `Agent` nor `AgentExecutor` was referenced outside its own
file, no call site anywhere in `src/` or `tests/` required updating. This was a
pre-condition for the deletion, not an assumption: it was verified by grep
before any file was modified.

## Verification

Full quality-gate sequence run in the mandated order, all clean:

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

All tests passed, 0 failed.

Phase 1's literal success criterion also holds: `cargo build` and
`cargo test --no-run` both succeed, and there are no residual references to
`Agent` or `AgentExecutor` anywhere in `src/` or `tests/`.
