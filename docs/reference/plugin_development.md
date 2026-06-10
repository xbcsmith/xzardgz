# Plugin Development Reference

## Overview

XZardgz plugins are self-contained analysis units that receive a repository scan
result, interact with an AI provider, and produce structured findings and
reports. The plugin system is trait-based: any type that implements
`WorkflowPlugin` can be registered and invoked through the same pipeline
infrastructure used by the built-in `technical-review` and `security-review`
plugins.

Plugins run inside the pipeline's tool and governance sandboxes. They declare
what level of filesystem access they need, receive a pre-built AI provider
reference, and return a structured output that the report infrastructure
processes into Markdown, JSON, and optionally SARIF files.

---

## WorkflowPlugin Trait

Every plugin implements the `WorkflowPlugin` trait defined in
`src/plugins/trait_def.rs`:

```rust
#[async_trait]
pub trait WorkflowPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn metadata(&self) -> PluginMetadata;
    fn supported_formats(&self) -> Vec<String>;
    fn required_tool_access(&self) -> ToolAccessLevel;
    async fn run(&self, ctx: PluginContext) -> Result<PluginOutput>;
}
```

### Method Descriptions

| Method                   | Description                                                                                                                                                                                              |
| ------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `name()`                 | Returns the plugin's stable identifier string, such as `"technical-review"`. This value is used in configuration, CLI flags, and report filenames.                                                       |
| `metadata()`             | Returns a `PluginMetadata` struct describing the plugin for the `plugin list` command and report headers.                                                                                                |
| `supported_formats()`    | Returns the list of report format strings the plugin can produce, such as `["markdown", "json"]`. The pipeline intersects this list with the configured `report_formats` to decide which writers to run. |
| `required_tool_access()` | Declares the filesystem access level the plugin needs. The pipeline configures the `ToolRegistry` in `PluginContext` accordingly before calling `run`.                                                   |
| `run(ctx)`               | Performs the analysis and returns a `PluginOutput`. This is the only async method. All AI provider calls, file reads, and tool invocations happen here.                                                  |

### Trait Bounds

`WorkflowPlugin` requires `Send + Sync` because plugin instances are stored in
`Arc<dyn WorkflowPlugin>` and shared across async tasks. Plugin implementations
must not hold non-Send state.

---

## PluginContext Reference

`PluginContext` is passed by value to `run`. It carries all pipeline resources
available during a plugin invocation.

```rust
pub struct PluginContext {
    pub config: Arc<Config>,
    pub workspace: Arc<WorkspaceManager>,
    pub state: WorkspaceState,
    pub scan_result: ScanResult,
    pub provider: Arc<dyn Provider + Send + Sync>,
    pub tool_registry: ToolRegistry,
    pub governance: GovernanceChecker,
    pub diagnostics: Diagnostics,
    pub watcher_task_id: Option<String>,
    pub prompts: HashMap<String, String>,
}
```

### PluginContext Fields

| Field             | Type                              | Description                                                               |
| ----------------- | --------------------------------- | ------------------------------------------------------------------------- |
| `config`          | `Arc<Config>`                     | Effective pipeline configuration after all overrides are applied.         |
| `workspace`       | `Arc<WorkspaceManager>`           | Manages workspace directories and state persistence.                      |
| `state`           | `WorkspaceState`                  | Snapshot of the workspace state at invocation time.                       |
| `scan_result`     | `ScanResult`                      | Repository scan artifact with file list, language breakdown, metadata.    |
| `provider`        | `Arc<dyn Provider + Send + Sync>` | Active AI provider. Plugins call this to request completions.             |
| `tool_registry`   | `ToolRegistry`                    | Tools available to the plugin, pre-configured for `required_tool_access`. |
| `governance`      | `GovernanceChecker`               | Checks repository state against configured governance rules.              |
| `diagnostics`     | `Diagnostics`                     | Accumulates non-fatal diagnostic messages during the run.                 |
| `watcher_task_id` | `Option<String>`                  | Watcher task message ID, or `None` for direct CLI invocations.            |
| `prompts`         | `HashMap<String, String>`         | Pre-loaded prompt templates keyed by template name.                       |

### Builder Methods

Two builder methods allow optional fields to be set after initial construction:

```rust
ctx.with_watcher_task_id(task_id: String) -> PluginContext
ctx.with_prompts(prompts: HashMap<String, String>) -> PluginContext
```

These return a new `PluginContext` with the field replaced. The primary
constructor does not require these fields.

---

## PluginOutput Reference

`PluginOutput` is the return type of `run`. It is constructed using the
`success` or `failure` constructor and then populated with findings, scores, and
report paths using mutation methods.

```rust
pub struct PluginOutput {
    pub summary: String,
    pub written_files: Vec<String>,
    pub findings: Vec<PluginFinding>,
    pub completed: bool,
    pub diagnostics: Diagnostics,
    pub scores: HashMap<String, f64>,
    pub risk_band: Option<RiskBand>,
    pub report_paths: HashMap<String, Vec<String>>,
    pub provider_metadata: Option<ProviderMetadata>,
    pub token_usage: Option<TokenUsage>,
}
```

### PluginOutput Fields

| Field               | Type                           | Description                                                          |
| ------------------- | ------------------------------ | -------------------------------------------------------------------- |
| `summary`           | `String`                       | Human-readable summary of the analysis outcome.                      |
| `written_files`     | `Vec<String>`                  | Paths of all files written during the run, in write order.           |
| `findings`          | `Vec<PluginFinding>`           | Structured findings produced during analysis.                        |
| `completed`         | `bool`                         | `true` on success; `false` for graceful failures.                    |
| `diagnostics`       | `Diagnostics`                  | Diagnostic messages collected during the run.                        |
| `scores`            | `HashMap<String, f64>`         | Named numeric scores, such as `"overall"` or per-dimension scores.   |
| `risk_band`         | `Option<RiskBand>`             | Recomputed on each `add_finding` call; `None` until first finding.   |
| `report_paths`      | `HashMap<String, Vec<String>>` | Report paths keyed by format label such as `"markdown"` or `"json"`. |
| `provider_metadata` | `Option<ProviderMetadata>`     | AI provider metadata captured at invocation time.                    |
| `token_usage`       | `Option<TokenUsage>`           | Input and output token counts for the AI provider calls.             |

### Constructors

```rust
PluginOutput::success(summary: impl Into<String>) -> PluginOutput
PluginOutput::failure(summary: impl Into<String>) -> PluginOutput
```

`success` sets `completed = true`. `failure` sets `completed = false`.

### Mutation Methods

```rust
output.add_finding(finding: PluginFinding)
output.add_written_file(path: impl Into<String>)
output.add_report_path(format: impl Into<String>, path: impl Into<String>)
output.set_score(name: impl Into<String>, value: f64)
```

`add_finding` automatically recomputes `risk_band` after each call by evaluating
the highest severity across all current findings.

---

## PluginFinding Reference

`PluginFinding` is the standard finding type used across all plugins and by the
report infrastructure.

```rust
pub struct PluginFinding {
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: FindingSeverity,
    pub file_path: Option<String>,
    pub line_number: Option<u32>,
    pub recommendation: String,
    pub confidence: f64,
}
```

### PluginFinding Fields

| Field            | Type              | Description                                                 |
| ---------------- | ----------------- | ----------------------------------------------------------- |
| `id`             | `String`          | Stable identifier for the finding; used for deduplication.  |
| `title`          | `String`          | Short title shown in report headers.                        |
| `description`    | `String`          | Detailed explanation of the observed issue.                 |
| `severity`       | `FindingSeverity` | Severity level; see severity table below.                   |
| `file_path`      | `Option<String>`  | Repository-relative path to the affected file, or `None`.   |
| `line_number`    | `Option<u32>`     | Line number within the file, or `None` if not determinable. |
| `recommendation` | `String`          | Actionable remediation step.                                |
| `confidence`     | `f64`             | AI confidence score in the range `[0.0, 1.0]`.              |

### Severity Levels

| Variant    | String value | Description                                                  |
| ---------- | ------------ | ------------------------------------------------------------ |
| `Critical` | `"critical"` | Immediate risk; blocks release in strict governance modes.   |
| `High`     | `"high"`     | Significant risk; should be addressed before next release.   |
| `Medium`   | `"medium"`   | Moderate risk; should be tracked and addressed promptly.     |
| `Low`      | `"low"`      | Minor risk or informational; address as part of normal work. |
| `Info`     | `"info"`     | Observation only; no remediation required.                   |

---

## ToolAccessLevel

`ToolAccessLevel` declares the filesystem access a plugin requires. The pipeline
runner reads this value and configures the `ToolRegistry` in `PluginContext`
before calling `run`.

```rust
pub enum ToolAccessLevel {
    None,
    ReadOnly,
    ReadWrite,
    Full,
}
```

| Variant     | Description                                                                       |
| ----------- | --------------------------------------------------------------------------------- |
| `None`      | No filesystem access. The plugin works only with the scan result and AI provider. |
| `ReadOnly`  | Read-only access to the workspace and repository files.                           |
| `ReadWrite` | Full read-write access to the workspace sandbox.                                  |
| `Full`      | Read-write access plus access to MCP tools and external tools.                    |

Prefer `None` or `ReadOnly` unless the plugin must write artifacts directly. The
pipeline's governance system may block certain access levels based on policy.

---

## PluginMetadata

`PluginMetadata` is returned by `metadata()` and is used in `plugin list` output
and report headers.

```rust
pub struct PluginMetadata {
    pub name: String,
    pub description: String,
    pub version: String,
    pub config_schema: Option<serde_json::Value>,
}
```

| Field           | Type                        | Description                                                         |
| --------------- | --------------------------- | ------------------------------------------------------------------- |
| `name`          | `String`                    | Plugin identifier, matching the value returned by `name()`.         |
| `description`   | `String`                    | One-sentence description shown in `plugin list` output.             |
| `version`       | `String`                    | Semantic version string, such as `"1.0.0"`.                         |
| `config_schema` | `Option<serde_json::Value>` | Optional JSON Schema describing the plugin's configuration section. |

Construct `PluginMetadata` with:

```rust
PluginMetadata::new(
    name: impl Into<String>,
    description: impl Into<String>,
    version: impl Into<String>,
) -> PluginMetadata
```

This leaves `config_schema` as `None`.

---

## Registration and Configuration

### Registering a Plugin

Plugins are registered in `PluginRegistry` before pipeline execution begins. The
registry maps plugin names to `Arc<dyn WorkflowPlugin>` instances.

```rust
let mut registry = PluginRegistry::new();
registry.register(Arc::new(MyPlugin::new()));
```

The registry key is the value returned by `plugin.name()`. Registering two
plugins with the same name overwrites the first.

### `plugins.enabled` Configuration

A plugin must appear in the `plugins.enabled` list in the configuration file
before it can be invoked. A plugin that is registered in the registry but not in
`enabled` cannot be used, even if named explicitly on the CLI.

```yaml
plugins:
  default: "technical-review"
  enabled:
    - "technical-review"
    - "security-review"
    - "my-custom-plugin"
```

The `default` field selects the plugin used when no `--plugin` flag is passed to
`xzardgz run`.

### Disabling a Plugin at Runtime

Call `registry.disable(name)` to mark a plugin as disabled without removing it.
Disabled plugins return `PipelineError::Plugin("plugin '...' is disabled")` from
`registry.get()`. This is useful for temporarily disabling a plugin without
removing it from the registry.

---

## Report Format Support

### `supported_formats()`

Return the list of format strings the plugin can produce. The pipeline
intersects this list with the `report_formats` value in the plugin's
configuration section:

```rust
fn supported_formats(&self) -> Vec<String> {
    vec![
        "markdown".to_string(),
        "json".to_string(),
        "sarif".to_string(),
    ]
}
```

### How Writers Work

The report infrastructure provides three writers that implement
`PluginReportFormatter`:

| Writer                 | Format   | Extension     | Description                                      |
| ---------------------- | -------- | ------------- | ------------------------------------------------ |
| `MarkdownReportWriter` | markdown | `.md`         | Human-readable report with findings table        |
| `JsonReportWriter`     | json     | `.json`       | Machine-readable `ReportEnvelope` in pretty JSON |
| `SarifReportWriter`    | sarif    | `.sarif.json` | SARIF 2.1.0 for GitHub Code Scanning integration |

Each writer calls `validate_report_path` before any filesystem access. Plugins
do not invoke writers directly; they populate `PluginOutput` and the pipeline
runner invokes the appropriate writers based on `supported_formats()` and the
configured formats.

### Report Formats

#### Markdown

The Markdown report contains a summary section, a findings table with severity,
file, line, and recommendation columns, and optionally a diagnostics section.
Output files use the `.md` extension.

#### JSON

The JSON report serializes a `ReportEnvelope` to pretty-printed JSON. The
envelope includes provenance fields (report ID, timestamp, plugin name,
repository name and URL, commit, workspace ID), analysis fields (provider
metadata, model ID, findings, diagnostics, risk band), and scores. Output files
use the `.json` extension.

#### SARIF

The SARIF 2.1.0 report is compatible with GitHub Code Scanning and other SARIF
consumers. Severity levels map to SARIF notification levels as follows:

| Severity   | SARIF level |
| ---------- | ----------- |
| `critical` | `"error"`   |
| `high`     | `"error"`   |
| `medium`   | `"warning"` |
| `low`      | `"note"`    |
| `info`     | `"note"`    |

Output files use the `.sarif.json` extension. The rule list is deduplicated and
sorted by rule ID for stable output across runs.

---

## Testing Plugins

### Unit Testing with Mock Components

Test a plugin's `run` method by constructing a `PluginContext` with mock
components. The `Provider` trait has a `MockProvider` available in test builds
via `mockall`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_my_plugin_run_with_empty_scan() {
        let dir = TempDir::new().unwrap();
        let config = Arc::new(Config::default());
        let workspace = Arc::new(
            WorkspaceManager::new(dir.path()).unwrap()
        );
        let state = WorkspaceState::default();
        let scan_result = ScanResult::empty();
        let provider = Arc::new(MockProvider::new());
        let ctx = PluginContext::new(
            config,
            workspace,
            state,
            scan_result,
            provider,
            ToolRegistry::empty(),
            GovernanceChecker::disabled(),
            Diagnostics::new(),
        );

        let plugin = MyPlugin::new();
        let output = plugin.run(ctx).await.unwrap();

        assert!(output.completed);
        assert!(output.findings.is_empty());
    }
}
```

### Testing Findings and Risk Band

Verify that `add_finding` recomputes `risk_band` correctly:

```rust
#[test]
fn test_output_risk_band_updates_on_add_finding() {
    let mut output = PluginOutput::success("analysis complete");
    assert!(output.risk_band.is_none());

    output.add_finding(PluginFinding {
        id: "FIND-001".to_string(),
        title: "Example finding".to_string(),
        description: "A high severity issue was found.".to_string(),
        severity: FindingSeverity::High,
        file_path: Some("src/lib.rs".to_string()),
        line_number: Some(42),
        recommendation: "Fix the issue.".to_string(),
        confidence: 0.9,
    });

    assert_eq!(output.risk_band, Some(RiskBand::High));
}
```

### Testing the Plugin Registry

Verify that a plugin registered and enabled can be retrieved, and that a
disabled plugin returns the expected error:

```rust
#[test]
fn test_registry_disabled_plugin_returns_error() {
    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(MyPlugin::new()));
    registry.disable("my-plugin");

    let result = registry.get("my-plugin");
    assert!(result.is_err());
}
```

---

## Best Practices

### Use Diagnostics for Non-fatal Issues

Add informational or warning messages to `ctx.diagnostics` rather than returning
early with an error when an issue does not prevent the analysis from completing.
This allows the pipeline to include diagnostics in the report without treating
them as failures.

```rust
ctx.diagnostics.warn("Skipping binary file: src/assets/logo.png");
```

### Set Risk Band Through `add_finding`

Do not set `risk_band` directly. Call `output.add_finding()` for each finding
and let the infrastructure recompute the risk band from the highest severity
across all findings. This keeps the risk band consistent with the findings list.

### Keep Plugin Logic in `run()`

All analysis logic belongs in `run()`. Avoid performing AI calls or file
operations in constructors or helper methods called at registration time. Plugin
instances are created once and may be invoked multiple times.

### Declare the Minimum Required Tool Access

Return the most restrictive `ToolAccessLevel` that still allows the plugin to
function. Plugins that only read the scan result and call the AI provider should
return `ToolAccessLevel::None`. This improves security posture and allows the
pipeline to run the plugin in stricter sandbox configurations.

### Write Reports Through `PluginOutput`

Record written file paths in `output` using `add_written_file` and
`add_report_path`. Do not write to the filesystem outside the workspace sandbox
without recording the paths. The pipeline uses these lists for cleanup,
transcript capture, and result publishing.

### Use `PluginOutput::failure` for Graceful Errors

When a non-fatal failure prevents the plugin from producing a complete analysis
(for example, the AI provider returns an unexpected response), return
`PluginOutput::failure(summary)` with `completed = false` rather than
propagating a `Result::Err`. Reserve `Result::Err` for unrecoverable errors that
should abort the entire pipeline run.

```rust
// Graceful failure: analysis could not complete, but the run is not aborted.
return Ok(PluginOutput::failure(
    "Provider returned an unrecognized response format. Analysis incomplete."
));
```

### Include a Stable `id` in Each Finding

Set the `id` field on every `PluginFinding` to a value that is stable across
runs for the same issue. A stable ID enables downstream consumers to deduplicate
findings and track issue lifecycle. A common pattern is to derive the ID from
the plugin name, category, file path, and a hash of the evidence text.
