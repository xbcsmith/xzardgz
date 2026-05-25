# WorkspaceState resolved_model Field Implementation

## Summary

This document explains the addition of the
`resolved_model: Option<ResolvedModel>` field to `WorkspaceState` in
`src/workspace/state.rs`, as required by Phase 9 specification section 9.4:
"Persist the resolved model record in workspace state."

## What Changed

### `src/workspace/state.rs`

Three modifications were made:

**Import** -- `ResolvedModel` is imported from the providers module:

```xzardgz/src/workspace/state.rs#L13
use crate::providers::model_resolution::ResolvedModel;
```

**Field** -- `resolved_model: Option<ResolvedModel>` was appended to
`WorkspaceState` after the `watcher_result_published` field:

```xzardgz/src/workspace/state.rs#L119-127
    /// The resolved model record from the model resolver.
    ///
    /// Set after the pre-flight model resolution pass. Contains the selected
    /// provider, model, capability flags, thinking mode, and any diagnostics
    /// produced during resolution. `None` before resolution runs or when the
    /// pipeline is resumed from a state that pre-dates this field.
    #[serde(default)]
    pub resolved_model: Option<ResolvedModel>,
```

**Constructor** -- `resolved_model: None` was added to the `WorkspaceState::new`
initializer so that freshly-created states always start with the field absent.

## Design Decisions

### Optional with `#[serde(default)]`

`resolved_model` is typed as `Option<ResolvedModel>` and carries
`#[serde(default)]`. This means:

- State files written before this field existed load without error; the field
  deserializes as `None`.
- Pipelines that have not yet completed model resolution (or that skip it) do
  not need to populate the field.
- Callers can detect whether resolution has run by checking
  `state.resolved_model.is_some()`.

### No Mandatory Population in `new()`

The field starts as `None` and is set by whichever pipeline stage performs model
resolution. Forcing a value in `new()` would require a `ResolvedModel` argument,
creating a backward-incompatible signature change with no benefit -- callers
that do not use model resolution would have to supply a dummy value.

### Placement in the Struct

The field is placed last (after `watcher_result_published`) to minimize diff
noise on existing state files and to follow the chronological order in which
fields are populated during a pipeline run.

## Tests Added

Five tests were added to the `#[cfg(test)]` block in `src/workspace/state.rs`:

| Test name                                                                 | What it verifies                                                                                                                      |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `test_workspace_state_resolved_model_is_none_by_default`                  | `new()` leaves the field as `None`                                                                                                    |
| `test_workspace_state_resolved_model_can_be_set_and_serialized`           | Setting the field and calling `to_yaml()` produces YAML containing the expected keys and model name                                   |
| `test_workspace_state_resolved_model_round_trips_through_yaml`            | A `ResolvedModel` with `ThinkingMode::Auto` requested and `ThinkingMode::Low` selected survives `to_yaml` then `load_from_str` intact |
| `test_workspace_state_with_fallback_resolved_model_round_trips`           | `fallback_used: true` and `fallback_reason` survive the round-trip                                                                    |
| `test_workspace_state_load_from_legacy_yaml_without_resolved_model_field` | A YAML document without the field deserializes successfully and yields `None`                                                         |

## Backward Compatibility

The `#[serde(default)]` attribute guarantees that any state file written by a
version of the binary that pre-dates this change will load without error. The
field will be `None` on load, which matches the pre-resolution semantics.

Forward compatibility (a new binary reading a file written by an even newer
binary) is preserved by the existing `serde_yaml` behavior: unknown fields are
silently ignored.
