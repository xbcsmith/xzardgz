# Watcher / XZepr Integration Implementation Plan

## Overview

xzardgz has two separate, disconnected pieces of watcher infrastructure:
`src/watcher/*` (task/result message types, matcher, executor, publisher —
all fully modeled and tested, but with no actual Kafka consumer loop behind
them) and `src/xzepr/consumer/*` (a fully working `XzeprConsumer` Kafka poll
loop that deserializes XZepr's `CloudEventMessage` envelope, but is never
invoked from anywhere in the CLI or workflow layer). `commands::watch::execute`
builds a `WatcherExecutor` and `WatcherMatcher`, reports configuration, and
then prints "Watcher consumer loop is implemented in Phase 17" — it never
starts consuming anything.

This plan keeps `XzeprConsumer` as the one, sole Kafka consumption
mechanism — external producers targeting the task topic emit XZepr-shaped
`CloudEventMessage`s, not a separate bespoke xzardgz task schema — and wires
it into the already-correct `WatcherExecutor::process_task` dispatch path.
The results topic, which xzardgz itself owns and publishes to, keeps its own
schema (`WatcherResultMessage`) rather than adopting XZepr's
event/receiver/group entity model, since concepts like `event_receiver_id`
are XZepr-specific and meaningless for xzardgz-generated results. A required
correlation id is threaded from the inbound trigger through to every
outbound result, and is generated for every workflow run — watcher-triggered
or not — so any run can be traced end to end.

## Current State Analysis

### Existing Infrastructure

- `src/xzepr/consumer/kafka.rs::XzeprConsumer` implements a complete
  `StreamConsumer` poll loop: subscribe, stream, deserialize
  `CloudEventMessage`, dispatch to a `MessageHandler`, and commit the offset.
  This is the only real, working Kafka consumer in the codebase.
- `src/watcher/task.rs::WatcherTaskMessage` and
  `src/watcher/result.rs::WatcherResultMessage` both already carry a
  required `correlation_id: String` field, and it is already threaded through
  `watcher/matcher.rs`, `watcher/executor.rs::WatcherExecutor::process_task`
  (line 458), and `watcher/publisher.rs` (used as the Kafka message key at
  line 485). This part of the design already matches what was asked for.
- `src/watcher/executor.rs::WatcherExecutor::process_task` already correctly
  delegates to `WorkflowExecutor::execute(ExecutionInput::WatcherTask(...))`
  and is exercised end to end by its own test module using a local-directory
  repository and a `TestPlugin`.
- `src/xzepr/consumer/message.rs::CloudEventMessage` carries XZepr's full
  CloudEvents 1.0.1 envelope plus nested `EventEntity`,
  `EventReceiverEntity`, and `EventReceiverGroupEntity` — XZepr-specific
  concepts (`event_receiver_id`, `event_receiver_groups`) that do not belong
  in a result schema xzardgz itself owns.

### Identified Issues

- `commands::watch::execute` never calls `XzeprConsumer` (or any consumer);
  the only working Kafka consumption code in the repository is never invoked
  from the CLI.
- `CloudEventMessage` has no `correlation_id` concept anywhere in its
  envelope — only a top-level `id` and, nested inside, per-event `id` /
  `event_receiver_id`, none of which are usable as a correlation id since
  XZepr itself will not be modified to accommodate xzardgz's tracing needs.
  The correlation id must instead be a required key inside
  `EventEntity.payload` — the opaque, producer-defined JSON blob that the
  triggering plan/source already controls — not a field on the XZepr
  envelope.
- Two independent Kafka configuration surfaces exist —
  `KafkaConfig`/`TopicsConfig` (used conceptually by `watcher::*`) and
  `xzepr::consumer::config::KafkaConsumerConfig` (used by `XzeprConsumer`) —
  and need reconciling into one operator-facing configuration.

## Implementation Phases

### Phase 1: Define the XZepr Integration Boundary (Scoping)

#### 1.1 Foundation Work

Produce a written, field-by-field mapping from `CloudEventMessage` to the
parameters `WorkflowExecutor` needs to run a plugin: which `event_type`
values are accepted (an allow-list enforced by `WatcherMatcher`); where
`repository`, `plugin`, `target_branch`, `provider`, and `dry_run` are read
from (top-level `CloudEventMessage` fields versus keys inside
`EventEntity.payload`); and the exact key name and shape of `correlation_id`
within `EventEntity.payload`.

`correlation_id` is sourced from inside the payload, never from the XZepr
envelope — this project has no plans to modify how XZepr itself works, so no
envelope-level field (`CloudEventMessage.id`, `EventEntity.id`,
`event_receiver_id`, etc.) is a candidate. The triggering plan/source is
responsible for placing a `correlation_id` key in the JSON payload it sends
through XZepr; this is a contract with whatever upstream system produces
that payload, not with XZepr's own schema, so it can be settled without any
XZepr-side change.

#### 1.2 Add Foundation Functionality

None yet — this phase is design-only. Decide explicitly whether
`event_receivers` and `event_receiver_groups` are ever consulted for
dispatch purposes (current expectation: no, only `events` matters for
triggering a workflow run) or reserved for future use.

#### 1.3 Integrate Foundation Work

Circulate the mapping decision for review, since it is a contract shared
with the XZepr project's producer side, before any adapter code is written.

#### 1.4 Testing Requirements

Not applicable — design phase only.

#### 1.5 Deliverables

A written field-mapping table, and a fixed, documented payload key name
(e.g. `correlation_id`) that every upstream producer publishing through
XZepr must populate for a message to be dispatchable.

#### 1.6 Success Criteria

No ambiguity remains about which `CloudEventMessage`/payload field populates
each workflow-invocation parameter before Phase 2 implementation starts, and
the `correlation_id` payload key is documented for upstream producers to
adopt without requiring any change on the XZepr side.

### Phase 2: Wire XzeprConsumer into the Watcher Execution Path

#### 2.1 Feature Work

Implement a `CloudEventMessage -> WorkflowExecutor` adapter per the Phase 1
mapping. `WatcherTaskMessage` is retired as an inbound wire format (it may
remain as an internal DTO if convenient, but is no longer what is
deserialized off the wire). The adapter reads `correlation_id` from the
required payload key established in Phase 1 (`EventEntity.payload`, never
the envelope) and rejects the message — no workflow run is dispatched — when
that key is missing, per the requirement that every plan/source publishing
through XZepr must supply one.

#### 2.2 Integrate Feature

Implement `commands::watch::execute`'s consumer loop using
`XzeprConsumer::run` with a `MessageHandler` implementation that applies the
Phase 2.1 adapter and then calls `WatcherExecutor::process_task` (already
correct and tested), replacing the "Phase 17" stub print entirely.

#### 2.3 Configuration Updates

Reconcile `KafkaConfig`/`TopicsConfig` and
`xzepr::consumer::config::KafkaConsumerConfig` into one Kafka configuration
surface, so operators configure brokers and topics once.

#### 2.4 Testing Requirements

An integration test publishing a real XZepr-shaped `CloudEventMessage` to a
test topic and asserting a `WatcherResultMessage` with the correct,
Phase-1-derived `correlation_id` is published to the result topic.

#### 2.5 Deliverables

`xzardgz watch` consumes and dispatches tasks end to end.

#### 2.6 Success Criteria

`commands::watch`'s existing dry-run and once-mode tests continue to pass; a
new non-dry-run test exercises one full consume-execute-publish cycle.

### Phase 3: Correlation ID for Every Run

#### 3.1 Feature Work

Generate a ULID `correlation_id` for every `WorkflowExecutor` invocation that
does not already have one supplied externally — CLI-triggered `run`/`scan`
invocations get a freshly generated id; watcher-dispatched tasks use the id
resolved per Phase 1/2.

#### 3.2 Integrate Feature

Persist `correlation_id` on `WorkspaceState` so it survives `--resume`, and
include it in every report's metadata, not only watcher results.

#### 3.3 Configuration Updates

Add an optional `--correlation-id` CLI override for operators who want to
supply their own id (e.g. propagated from an upstream CI pipeline).

#### 3.4 Testing Requirements

Assert a CLI-triggered `run` produces a `WorkspaceState` and report with a
non-empty, stable `correlation_id`, and that `--resume` preserves the
original id rather than generating a new one.

#### 3.5 Deliverables

Every workflow run — watcher-triggered or not — is traceable by a single
`correlation_id` from trigger through to final report or result.

#### 3.6 Success Criteria

For a given run, the same `correlation_id` appears in `WorkspaceState`,
every generated report's metadata block, and (when applicable)
`WatcherResultMessage`.
