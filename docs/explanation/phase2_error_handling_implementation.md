# Phase 2: Error Handling and Diagnostics Foundation

## Overview

Phase 2 establishes a unified error handling model and a structured diagnostics
system for the XZardgz pipeline. Before this phase, errors propagated through a
nested hierarchy where `XzardgzError` wrapped four independent sub-types
(`ConfigError`, `ProviderError`, `WorkflowError`, `RepositoryError`). This
arrangement required callers to wrap sub-type errors at every boundary and made
it difficult to introduce new error kinds without modifying multiple layers.

Phase 2 replaces this structure with `PipelineError`, a single flat enum that
covers every failure domain in the pipeline. A type alias preserves the
public-facing `XzardgzError` name so existing consumers are unaffected. A
`crate::error::Result<T>` alias then lets modules express their return types
concisely and consistently.

Alongside the error refactoring, Phase 2 introduces a structured diagnostics
module. Diagnostics are distinct from errors: they capture warnings and
informational observations that do not stop execution but are useful for reports,
watcher result messages, and workspace audit trails. They are fully serializable
to JSON, persistable to disk, filterable, and mergeable across pipeline stages.

Together, these two systems give every pipeline stage a coherent way to signal
failures and record non-fatal observations.

---

## Components

The following components are introduced or updated in Phase 2.

### New modules

- `src/error.rs` (rewritten) - `PipelineError` flat enum; `XzardgzError` type
  alias; `crate::error::Result<T>` alias; `From` conversions from legacy
  sub-types.
- `src/diagnostics.rs` (new) - `DiagnosticLevel`, `DiagnosticCategory`,
  `Diagnostic`, and `Diagnostics` types with full serde support, persistence,
  filtering, and merge operations.

### Updated modules

- `src/agent/context.rs` - `compact_if_needed` returns `crate::error::Result<bool>`
- `src/agent/core.rs` - `Agent::run` returns `crate::error::Result<String>`;
  lock errors map to `PipelineError::Agent`
- `src/agent/executor.rs` - `AgentExecutor::execute` returns `crate::error::Result<String>`
- `src/tools/mod.rs` - `ToolExecutor::execute` trait method uses `crate::error::Result<ToolResult>`
- `src/tools/executor.rs` - tool-not-found maps to `PipelineError::Tool`
- `src/tools/file_ops.rs` - missing-param errors map to `PipelineError::Tool`
- `src/tools/git_ops.rs` - git errors map to `PipelineError::Git`
- `src/workflow/executor.rs` - `WorkflowExecutor::execute` returns `Result<()>`;
  deadlock maps to `PipelineError::Workflow`
- `src/workflow/parser.rs` - parse errors map to `PipelineError::Workflow`
- `src/config.rs` - `Config::load` returns `crate::error::Result<Config>`;
  errors map to `PipelineError::Config`
- `src/providers/factory.rs` - `ProviderFactory::create` returns
  `Result<Arc<dyn Provider>>`; unknown provider maps to `PipelineError::Provider`

---

## Error System Design

### Why a flat enum

The nested hierarchy (`XzardgzError` wrapping `WorkflowError`, etc.) had two
problems. First, adding a new top-level error domain (for example, MCP transport
errors) required choosing an existing sub-type or adding a new wrapper variant,
neither of which maps naturally onto MCP-specific semantics. Second, the `?`
operator within a module that returned `WorkflowError` could not propagate a
`ProviderError` without an explicit `map_err`, even when the failure semantics
were clear.

`PipelineError` removes both problems. Every failure domain is a first-class
variant and all modules share a single error type through the `Result<T>` alias:

```rust
// src/error.rs
pub type XzardgzError = PipelineError;
pub type Result<T> = std::result::Result<T, PipelineError>;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("config error: {0}")]
    Config(String),
    #[error("git error: {0}")]
    Git(String),
    #[error("scanner error: {0}")]
    Scanner(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("auth error: {0}")]
    Auth(String),
    #[error("governance error: {0}")]
    Governance(String),
    #[error("agent error: {0}")]
    Agent(String),
    #[error("tool error: {0}")]
    Tool(String),
    #[error("plugin not found: {name}")]
    PluginNotFound { name: String },
    #[error("plugin error: {0}")]
    Plugin(String),
    #[error("prompt error: {0}")]
    Prompt(String),
    #[error("report error: {0}")]
    Report(String),
    #[error("workspace error: {0}")]
    Workspace(String),
    #[error("workflow error: {0}")]
    Workflow(String),
    #[error("watcher error: {0}")]
    Watcher(String),
    #[error("kafka error: {0}")]
    Kafka(String),
    #[error("mcp error: {0}")]
    Mcp(String),
    #[error("mcp transport error: {0}")]
    McpTransport(String),
    #[error("mcp server not found: {server}")]
    McpServerNotFound { server: String },
    #[error("mcp tool not found: server={server}, tool={tool}")]
    McpToolNotFound { server: String, tool: String },
    #[error("mcp protocol version mismatch: expected={expected}, got={got}")]
    McpProtocolVersionMismatch { expected: String, got: String },
    #[error("mcp timeout: server={server}, timeout_ms={timeout_ms}")]
    McpTimeout { server: String, timeout_ms: u64 },
    #[error("mcp auth error: {0}")]
    McpAuth(String),
    #[error("mcp elicitation error: {0}")]
    McpElicitation(String),
    #[error("mcp task error: {0}")]
    McpTask(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("error: {0}")]
    Custom(String),
}
```

### Structured MCP variants

MCP errors use structured variants instead of plain strings wherever the
information can guide automated recovery. `McpServerNotFound { server }` and
`McpToolNotFound { server, tool }` carry the identities of the missing resource
so callers can log them, fall back, or surface them in watcher result messages
without parsing an error string. `McpProtocolVersionMismatch` and `McpTimeout`
follow the same principle.

Generic `Mcp(String)`, `McpTransport(String)`, `McpAuth(String)`,
`McpElicitation(String)`, and `McpTask(String)` handle categories where a
structured variant would not add recovery value over the message text.

### Legacy sub-type strategy

The existing `ConfigError`, `ProviderError`, `WorkflowError`, and
`RepositoryError` enums are retained as module-internal types. Each gets a
`From` implementation into `PipelineError` so the `?` operator converts them
at module boundaries without requiring `map_err` everywhere:

```rust
impl From<ConfigError> for PipelineError {
    fn from(e: ConfigError) -> Self {
        PipelineError::Config(e.to_string())
    }
}

impl From<ProviderError> for PipelineError {
    fn from(e: ProviderError) -> Self {
        PipelineError::Provider(e.to_string())
    }
}

impl From<WorkflowError> for PipelineError {
    fn from(e: WorkflowError) -> Self {
        PipelineError::Workflow(e.to_string())
    }
}

impl From<RepositoryError> for PipelineError {
    fn from(e: RepositoryError) -> Self {
        PipelineError::Git(e.to_string())
    }
}
```

Provider and config modules can still construct `ConfigError::Load` or
`ProviderError::Auth` internally. At the point where those functions return
through a `crate::error::Result<T>` boundary, the compiler applies the `From`
conversion automatically. The legacy types are not exported from `src/lib.rs`.

### Backward compatibility

The type alias `pub type XzardgzError = PipelineError` means any code that
names `XzardgzError` continues to compile. No public API signature changes are
required in callers that already use `XzardgzError` as their error type. Tests
that match on `XzardgzError::Workflow(...)` can be migrated incrementally to
match on `PipelineError::Workflow(...)` without a flag day.

---

## Diagnostics System Design

### Motivation

Errors signal that execution cannot continue. Diagnostics record observations
that are worth knowing but do not stop the pipeline: a provider fallback, a
missing optional configuration key, a watcher message that matched a rule by a
partial match, or an MCP server that returned a deprecation notice. Without a
dedicated type, these observations are either dropped or embedded in log output
where they cannot be persisted, queried, or included in structured reports.

### Levels and categories

`DiagnosticLevel` has two variants, `Warning` and `Info`. `Warning` signals a
condition that may indicate a problem. `Info` records context without suggesting
a problem.

`DiagnosticCategory` identifies which pipeline stage produced the diagnostic:

```rust
pub enum DiagnosticCategory {
    Config,
    Scan,
    Plugin,
    ProviderFallback,
    WatcherRouting,
    KafkaPublish,
    Mcp,
}
```

The category is useful when a consumer wants to present diagnostics grouped by
stage, when a plugin wants to show only its own diagnostics, or when a watcher
result message should contain only routing-related observations.

### The Diagnostic struct

```rust
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub category: DiagnosticCategory,
    pub message: String,
    pub context: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

The `context` field is a free-form string for attaching an identifier such as a
plugin name, server name, or task ID to the entry without adding a new struct
field for every possible context type. The timestamp is always UTC so entries
from different pipeline stages can be sorted into a single ordered sequence.

### The Diagnostics collection

`Diagnostics` wraps a `Vec<Diagnostic>` and provides methods for construction,
filtering, serialization, persistence, and merge:

```rust
// Convenience push methods
diags.push_warning(DiagnosticCategory::Config, "unknown section ignored");
diags.push_warning_with_context(
    DiagnosticCategory::Plugin,
    "plugin returned no findings",
    "security-review",
);
diags.push_info(DiagnosticCategory::ProviderFallback, "fell back to ollama");

// Filtering
let all_warnings: Vec<&Diagnostic> = diags.warnings();
let mcp_entries: Vec<&Diagnostic> = diags.by_category(&DiagnosticCategory::Mcp);

// Serialization
let json: String = diags.to_json()?;
let restored: Diagnostics = Diagnostics::from_json(&json)?;

// Persistence
diags.persist(Path::new(".xzardgz/state/diagnostics.json"))?;
let loaded = Diagnostics::load(Path::new(".xzardgz/state/diagnostics.json"))?;

// Merge (pipeline stage aggregation)
stage_a_diags.merge(stage_b_diags);
```

### JSON serialization

All diagnostics types derive `serde::Serialize` and `serde::Deserialize`. The
JSON representation is a flat object with an `entries` array, matching the
struct layout directly. This makes the format predictable for report generators
and watcher result message consumers that read the JSON without going through
the Rust types.

### Persistence and workspace state

`Diagnostics::persist` writes the JSON representation to a file path, creating
parent directories automatically when they do not exist. This is intentional:
workspace state directories are created on demand, and diagnostic persistence
should not require callers to manage the directory separately.

`Diagnostics::load` returns an empty `Diagnostics` when the file does not
exist, rather than propagating a not-found error. The absence of a diagnostics
file is a valid starting state, not an error condition. Callers only need to
handle errors for malformed files.

---

## Module Integration

### The Result alias pattern

All updated modules import the crate-level alias:

```rust
use crate::error::Result;
```

Return types become `Result<T>` without repeating the error type. This is the
same pattern used by the standard library's `std::io` module and most
well-structured Rust crates. It makes function signatures shorter and ensures
all modules automatically share the same error type.

### The ? operator and From conversions

Modules that produce legacy sub-type errors can use `?` directly against a
`crate::error::Result<T>` return type because the `From` implementations are
in scope:

```rust
// In src/config.rs, Config::load returns crate::error::Result<Config>.
// ConfigError implements From<ConfigError> for PipelineError, so:
let content = std::fs::read_to_string("config.yaml")
    .map_err(|e| ConfigError::Load(e.to_string()))?;  // maps io::Error -> ConfigError
let file_config: Config = serde_yaml::from_str(&content)
    .map_err(|e| ConfigError::Load(e.to_string()))?;  // maps serde error -> ConfigError
// Both ? operators then convert ConfigError -> PipelineError automatically.
```

For errors that do not already have a legacy sub-type, modules construct
`PipelineError` variants directly:

```rust
// In src/tools/executor.rs, unknown tool maps directly to PipelineError::Tool.
let executor = self.registry.get_executor(&function.name).ok_or_else(|| {
    PipelineError::Tool(format!("tool not found: {}", function.name))
})?;
```

### Lock errors

Mutex lock failures in `src/agent/core.rs` previously mapped to
`WorkflowError::Execution`. Under the new model they map to `PipelineError::Agent`
because the failure originates in agent logic, not workflow execution:

```rust
let mut context = self.context.lock().map_err(|_| {
    PipelineError::Agent("context lock poisoned".to_string())
})?;
```

The distinction matters when filtering errors by variant to decide whether a
failure should retry, alert on a specific pipeline stage, or be reported in
structured output.

---

## Implementation Details

### Backward compatibility via type alias

`pub type XzardgzError = PipelineError` is the only backward-compatibility
mechanism required. It is a zero-cost type alias in Rust: the compiler treats
`XzardgzError` and `PipelineError` as the same type. No wrapper struct, no
additional `From` impls, and no runtime overhead are needed. Any code that
matches on `XzardgzError` variants needs to use the `PipelineError` variant
names, but since the nested wrapper variants (`Config(ConfigError)`,
`Provider(ProviderError)`, etc.) are replaced by the flat enum, matches must be
updated regardless.

### From implementations for legacy types

The `From` implementations follow a consistent strategy: convert the legacy
error to its string representation and wrap it in the semantically closest
`PipelineError` variant. This loses the structured information of the sub-type
(for example, which `ConfigError` variant fired), but that information is
captured in the message string. The tradeoff accepts some loss of matchability
on sub-type variants in exchange for a uniform error surface at all module
boundaries.

If a future phase needs sub-type matchability, the relevant `PipelineError`
variant can be promoted from `String` to a structured enum at that point without
changing the `From` conversion interface.

### Persistence via workspace state

Diagnostics are persisted to the workspace state directory alongside other
pipeline artifacts. The path follows the pattern
`.xzardgz/state/{stage}_diagnostics.json` so each pipeline stage's
observations can be loaded independently and merged into a combined report.
The `merge` method exists specifically to support this staged accumulation:

```rust
let mut combined = Diagnostics::new();
let config_diags = Diagnostics::load(&config_diag_path)?;
let scan_diags = Diagnostics::load(&scan_diag_path)?;
combined.merge(config_diags);
combined.merge(scan_diags);
report.set_diagnostics(combined);
```

### No panics at error sites

No module is permitted to call `unwrap()` or `expect()` at an error site
without a `// SAFETY:` comment explaining why the operation cannot fail.
The existing `context.lock().map_err(...)` pattern throughout `src/agent/core.rs`
satisfies this rule: the lock could theoretically be poisoned, so it is treated
as a fallible operation that maps to `PipelineError::Agent`.

---

## Testing

Test coverage for Phase 2 spans two areas: the error system and the diagnostics
system.

### Error system tests

Tests in `src/error.rs` cover:

- Display strings for all major `PipelineError` variants to confirm the
  `#[error(...)]` attributes produce the expected messages.
- `From` conversions for all four legacy sub-types (`ConfigError`,
  `ProviderError`, `WorkflowError`, `RepositoryError`) to confirm `?` will
  convert them correctly at module boundaries.
- Structured variant field access for `PluginNotFound { name }`,
  `McpServerNotFound { server }`, `McpToolNotFound { server, tool }`,
  `McpProtocolVersionMismatch { expected, got }`, and `McpTimeout { server,
  timeout_ms }`.

### Diagnostics system tests

Tests in `src/diagnostics.rs` cover:

- `Diagnostic::warning` creates an entry with `DiagnosticLevel::Warning` and no
  context.
- `Diagnostic::info` creates an entry with `DiagnosticLevel::Info` and no
  context.
- `Diagnostic::warning_with_context` sets the context field.
- `Diagnostics::push` increments the collection length.
- Convenience push methods (`push_warning`, `push_warning_with_context`,
  `push_info`) delegate correctly.
- `warnings()` returns only `Warning`-level entries.
- `by_category()` returns only entries matching the requested category.
- JSON roundtrip: serialize a two-entry collection, deserialize, confirm message
  and level fields.
- Persist and load roundtrip using a `tempfile::TempDir` to confirm the file is
  written and the loaded collection matches the original.
- `load()` returns an empty collection when the file does not exist (no error).
- `merge()` appends all entries from the second collection in order.
- `DiagnosticLevel::Display` produces `"WARNING"` and `"INFO"`.
- `DiagnosticCategory::Display` produces the snake-case string for each variant.
- `is_empty()` returns true for a new collection and false after a push.

All tests use `#[cfg(test)]` modules in the same file as the implementation and
follow the structure required by the project coding standards.

---

## Success Criteria

The following criteria define a complete Phase 2 implementation.

**Error system**

- `PipelineError` flat enum covers all pipeline domains; all variants are listed
  with no gaps for active modules.
- `pub type XzardgzError = PipelineError` compiles without type errors.
- Legacy sub-types (`ConfigError`, `ProviderError`, `WorkflowError`,
  `RepositoryError`) convert to `PipelineError` via `?`; `From` impl tests pass.
- All updated modules use `crate::error::Result<T>`; no bare sub-type results
  remain at public module boundaries.

**Diagnostics system**

- `Diagnostics` serializes and deserializes correctly; JSON roundtrip test passes.
- `Diagnostics` persists to and loads from disk; persist/load roundtrip test passes.
- Missing diagnostics file returns an empty collection without error;
  `load()` missing-file test passes.
- `Diagnostics::merge` appends all entries from the second collection in order;
  merge test passes.

**Quality gates**

- `cargo fmt --all` passes with no formatting changes needed.
- `cargo check --all-targets --all-features` passes with zero compilation errors.
- `cargo clippy --all-targets --all-features -- -D warnings` passes with zero
  warnings.
- `cargo test --all-features` passes; coverage exceeds 80% for new code.
