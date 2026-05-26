# Phase 13: Plugin Runtime Implementation

## Overview

Phase 13 adds the `src/plugins/` module to the XZardgz pipeline. This module
provides the core plugin infrastructure: a runtime context, structured output
types, a plugin trait, and a registry for managing named plugins.

## Files Created

| File                       | Purpose                                     |
| -------------------------- | ------------------------------------------- |
| `src/plugins/mod.rs`       | Module declarations and public re-exports   |
| `src/plugins/output.rs`    | `PluginOutput` and `TokenUsage` types       |
| `src/plugins/context.rs`   | `PluginContext` and `ToolAccessLevel`       |
| `src/plugins/trait_def.rs` | `WorkflowPlugin` trait and `PluginMetadata` |
| `src/plugins/registry.rs`  | `PluginRegistry` for named plugin dispatch  |

`src/lib.rs` was updated to declare `pub mod plugins;` alphabetically before
`pub mod mcp;`.

## Architecture

### ToolAccessLevel (`context.rs`)

A three-variant enum that declares the filesystem access a plugin requires:

- `None` - no file system access
- `ReadOnly` - read-only sandbox
- `ReadWrite` - full read-write sandbox

The plugin runner reads this value to configure the appropriate `ToolRegistry`
before invoking the plugin.

### PluginContext (`context.rs`)

The complete runtime context passed by value to `WorkflowPlugin::run`. It
carries:

- `Arc<Config>` - effective pipeline configuration
- `Arc<WorkspaceManager>` - workspace paths and state persistence
- `WorkspaceState` - snapshot of the workspace at invocation time
- `ScanResult` - the repository scan artifact
- `Arc<dyn Provider + Send + Sync>` - AI provider for completions
- `ToolRegistry` - tools available in the sandbox
- `GovernanceChecker` - policy enforcement
- `Diagnostics` - collector for run-scoped diagnostics
- `Option<String>` watcher task ID
- `HashMap<String, String>` prompt templates

Builder methods `with_watcher_task_id` and `with_prompts` allow the runner to
attach optional data after initial construction.

### TokenUsage (`output.rs`)

A small struct tracking `input_tokens` and `output_tokens` consumed by the AI
provider. The `total()` method returns their sum.

### PluginOutput (`output.rs`)

The complete result of a plugin run. Constructed via `PluginOutput::success` or
`PluginOutput::failure`. Mutation methods maintain invariants:

- `add_finding` appends a `PluginFinding` and recomputes `risk_band` by building
  a temporary `PluginFindings` collection and calling `to_risk_band()`.
- `add_written_file`, `add_report_path`, `set_score` append to their respective
  collections.
- `finding_count()` and `highest_risk()` provide read access.

### PluginMetadata (`trait_def.rs`)

A serializable struct holding `name`, `version`, `description`, and an optional
`config_schema: Option<serde_json::Value>`. Created via `PluginMetadata::new`
which leaves `config_schema` as `None`.

### WorkflowPlugin trait (`trait_def.rs`)

The core abstraction every analysis plugin implements:

```rust
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait WorkflowPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn metadata(&self) -> PluginMetadata;
    fn supported_formats(&self) -> Vec<String>;
    fn required_tool_access(&self) -> ToolAccessLevel;
    async fn run(&self, ctx: PluginContext) -> Result<PluginOutput>;
}
```

The `#[cfg_attr(test, mockall::automock)]` attribute generates
`MockWorkflowPlugin` in test builds, following the same pattern used for the
`Provider` trait.

### PluginRegistry (`registry.rs`)

A `HashMap<String, Arc<dyn WorkflowPlugin>>` paired with a `HashSet<String>` of
disabled names. Key operations:

- `register` stores by `plugin.name()`.
- `disable` adds a name to the disabled set without removing the plugin.
- `get` checks disabled first (returns
  `PipelineError::Plugin("... is disabled")`), then checks registration (returns
  `PipelineError::PluginNotFound` if absent).
- `list_plugins` collects all metadata and sorts by name for deterministic
  output.
- `validate_plugin_config` is a stub that delegates to `get` and returns
  `Ok(())`.

## Error Handling

| Condition                    | Error variant                                             |
| ---------------------------- | --------------------------------------------------------- |
| Plugin name in disabled set  | `PipelineError::Plugin("plugin '...' is disabled")`       |
| Plugin name not registered   | `PipelineError::PluginNotFound { name }`                  |
| Unrecoverable plugin failure | `PipelineError::Plugin(msg)` (returned by `run`)          |
| Graceful failure             | `PluginOutput::failure(summary)` with `completed = false` |

## Testing

42 unit tests cover all public types and functions across the four submodules.
Doctests cover all public methods with runnable examples.

Key test patterns:

- `output.rs` tests verify both constructors, all mutation methods, and the
  automatic `risk_band` recomputation after `add_finding`.
- `context.rs` tests use `tempfile::TempDir` and a real `WorkspaceManager`,
  `MockProvider` (from `crate::providers::base`), and a `GovernanceConfig` with
  `rules_path: String::new()` to avoid the project's `AGENTS.md` being parsed as
  YAML.
- `trait_def.rs` tests exercise the `mockall`-generated `MockWorkflowPlugin`.
- `registry.rs` tests use a concrete `MockPlugin` struct that implements
  `WorkflowPlugin` directly.

## Quality Gates

All four gates passed:

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

986 unit tests passed, 279 doctests passed, 0 failures.
