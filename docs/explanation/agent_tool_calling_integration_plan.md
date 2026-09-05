# Agent Tool-Calling Integration Implementation Plan

## Overview

xzardgz ships three separate multi-turn, tool-calling agent-loop
implementations (`src/agent/core.rs::Agent`, `src/agent/executor.rs::AgentExecutor`,
`src/agent/session.rs::AgentSession`) alongside a fully sandboxed tool registry
(`src/tools/sandbox.rs`, `src/tools/registry.rs`). None of the three is used by
the two shipped plugins: `security_review` and `technical_review` both call
`Provider::complete(&messages, &[] as &[Tool])` directly, single-shot, so the
model never gets to read a file, grep for a pattern, or otherwise investigate
the repository before producing findings. This plan consolidates the agent
layer down to one implementation and wires it into both plugins so analysis
becomes genuinely investigative rather than a single fixed-context prompt.

There is no backward-compatibility requirement: this plan does not preserve a
single-shot fallback path for providers without tool-calling support. A
provider that cannot call tools cannot run these plugins, full stop.

## Current State Analysis

### Existing Infrastructure

- `src/agent/session.rs::AgentSession` is the most complete of the three
  agent-loop implementations: configurable `max_turns` (default 10), JSONL
  transcript persistence, and per-tool repeated-failure-count tracking.
- `src/tools/sandbox.rs::PathValidator` and `src/tools/registry.rs` already
  provide `build_read_only_registry`/`build_read_write_registry`, and
  `src/workflow/executor.rs` (around line 478) already constructs a
  per-plugin `ToolRegistry` scoped correctly by `WorkflowPlugin::required_tool_access()`.
  This wiring exists and is correct; it is simply never handed to anything
  that drives a tool-calling loop.
- `src/providers/base.rs::Provider` already exposes `metadata().capabilities.tools`,
  so tool-calling support is queryable per-provider today.

### Identified Issues

- A repository-wide search confirms `src/agent/core.rs::Agent` and
  `src/agent/executor.rs::AgentExecutor` have **zero references anywhere in
  the codebase outside their own file** (not from plugins, not from tests,
  not from the workflow executor). They are unused, uncalled prototypes.
  `AgentSession` likewise has no external callers today, but its shape
  (transcript persistence, failure tracking) marks it as the intended
  canonical implementation.
- `src/agent/executor.rs::AgentExecutor` additionally has a latent bug: it
  calls `complete` with an empty tool list (`vec![]`) regardless of what the
  registry contains, meaning even if it were wired up today it would not
  actually expose tools to the model.
- `plugins/security_review/plugin.rs` (around line 190) and
  `plugins/technical_review/plugin.rs` call `provider.complete` directly with
  no tools, so all analysis is bounded by whatever content is manually
  pre-fetched into the prompt.

## Implementation Phases

### Phase 1: Consolidate the Agent Layer

#### 1.1 Foundation Work

Confirm (already done via `grep -rn "AgentExecutor"` / `grep -rn "agent::core"`
across `src/`) that `Agent` and `AgentExecutor` have no external callers, so
deleting them carries no migration burden.

#### 1.2 Add Foundation Functionality

Delete `src/agent/core.rs` and `src/agent/executor.rs` in full. Retain
`src/agent/session.rs::AgentSession` as the sole agent-loop implementation.
Update `src/agent/mod.rs` and `src/lib.rs` module declarations/re-exports
accordingly.

#### 1.3 Integrate Foundation Work

No call sites require updating, since nothing outside the deleted modules'
own files referenced them. Sweep `src/agent/mod.rs` doc comments and any
crate-level architecture docs referencing the three-implementation state.

#### 1.4 Testing Requirements

Run the full test suite after deletion to confirm no residual references or
broken doctests. Add a `mod.rs`-level doc comment asserting `AgentSession` is
the only agent-loop type, so a future contributor does not reintroduce the
duplication.

#### 1.5 Deliverables

A single `agent::session::AgentSession` implementation; two dead modules
removed.

#### 1.6 Success Criteria

`cargo build` and `cargo test --no-run` succeed with no residual references
to `Agent` or `AgentExecutor` anywhere in `src/` or `tests/`.

### Phase 2: Wire AgentSession into Plugin Execution

#### 2.1 Feature Work

Extend `PluginContext` (`src/plugins/context.rs`) to carry an `AgentSession`
handle (or the pieces needed to construct one on demand: provider, the
per-plugin `ToolRegistry` already built in `workflow/executor.rs:478`, and a
`max_turns` value). Session construction requires
`provider.metadata().capabilities.tools == true`; when false, return a
`PipelineError::Provider` ("this provider does not support tool calling")
before any plugin work begins — this is the hard requirement replacing any
single-shot fallback.

#### 2.2 Integrate Feature

Replace the direct `provider.complete(&messages, &[] as &[Tool])` calls in
`plugins/security_review/plugin.rs` and `plugins/technical_review/plugin.rs`
with an `AgentSession::run(system_prompt, task_prompt)`-style multi-turn call
that exposes the plugin's already-selected tool set (read-only or read-write
per `required_tool_access()`) to the model.

#### 2.3 Configuration Updates

Add a `max_turns` field (sensible default, e.g. 15) to `SecurityReviewConfig`
and `TechnicalReviewConfig`, or introduce a small shared
`AgentSessionConfig { max_turns }` struct embedded in both if a shared
plugin-config concept does not already exist.

#### 2.4 Testing Requirements

Rewrite plugin tests to mock a tool-calling provider (`mockall`-based
`Provider::complete` returning scripted tool-call turns followed by a final
JSON response) and assert that at least one tool call (e.g. `read_file`)
occurs before the plugin's findings are parsed. Existing single-shot mocked
tests must be replaced, not kept alongside, since there is no single-shot
path left to exercise.

#### 2.5 Deliverables

Both shipped plugins run their AI analysis through `AgentSession`; the
single-shot `provider.complete` call sites are gone from plugin code.

#### 2.6 Success Criteria

Plugin unit tests exercise at least one multi-turn tool-call round trip.
Existing end-to-end workflow tests (`src/workflow/executor.rs`'s test module)
continue to pass with plugins now driving a real tool-calling session instead
of a single completion call. Attempting to run either plugin against a
provider whose `capabilities.tools` is `false` fails fast with a clear error
rather than silently degrading.
