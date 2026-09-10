//! Workflow plan model for the XZardgz pipeline.
//!
//! This module defines the plugin-first workflow plan model introduced in
//! schema version 1. Plans describe a sequence of plugin execution steps
//! against a target repository, with optional overrides for provider, model,
//! scanner, and report output configuration.

use crate::error::{PipelineError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Schema version string expected in all plan files.
pub const PLAN_VERSION: &str = "1";

/// Top-level workflow plan (plugin-first model).
///
/// A `WorkflowPlan` describes the full execution context for a pipeline run:
/// the target repository, provider and model overrides, scanner configuration,
/// and the ordered list of plugin steps to execute.
///
/// All plans must declare `version: "1"`. Plans with unknown version values
/// are rejected during validation.
///
/// # Examples
///
/// ```
/// use xzardgz::workflow::plan::WorkflowPlan;
///
/// let plan = WorkflowPlan::direct_plugin_invocation("technical-review", ".");
/// assert!(plan.validate().is_ok());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPlan {
    /// Schema version. Must equal [`PLAN_VERSION`] (`"1"`). All other values
    /// are rejected during validation.
    pub version: String,

    /// Human-readable plan name. Must not be empty.
    pub name: String,

    /// Optional description of the plan's purpose.
    #[serde(default)]
    pub description: Option<String>,

    /// Repository path (local) or URL (remote) to process.
    pub repository: String,

    /// Target branch to check out. Uses the default branch when omitted.
    #[serde(default)]
    pub branch: Option<String>,

    /// Workspace directory for intermediate artifacts.
    ///
    /// Defaults to `.xzardgz/workspace` when not specified.
    #[serde(default)]
    pub workspace: Option<String>,

    /// Provider override (`openai`, `anthropic`, `ollama`, `copilot`).
    ///
    /// Falls back to the global configuration when omitted.
    #[serde(default)]
    pub provider: Option<String>,

    /// Model identifier override.
    ///
    /// Falls back to the global configuration when omitted.
    #[serde(default)]
    pub model: Option<String>,

    /// Optional scanner configuration overrides for this plan.
    #[serde(default)]
    pub scan: Option<PlanScanOptions>,

    /// Plugin execution steps. At least one step is required.
    #[serde(default)]
    pub steps: Vec<PluginStep>,

    /// Report output configuration.
    #[serde(default)]
    pub reports: Option<PlanReportOptions>,

    /// When `true`, performs a dry run without invoking providers or writing
    /// reports.
    #[serde(default)]
    pub dry_run: bool,

    /// When `true`, resumes from existing workspace state instead of starting
    /// fresh.
    #[serde(default)]
    pub resume: bool,

    /// Optional correlation identifier for this run.
    ///
    /// When set, this value is used as the run's `correlation_id` rather than
    /// generating a fresh ULID. Watcher-triggered runs populate this from the
    /// inbound task's `correlation_id`; CLI-triggered runs may supply it via
    /// the `--correlation-id` flag.
    ///
    /// `None` means the executor will generate a fresh ULID for this run.
    #[serde(default)]
    pub correlation_id: Option<String>,
}

/// Plugin execution step within a [`WorkflowPlan`].
///
/// Each step declares the plugin to run, optional per-plugin configuration,
/// and dependency ordering constraints relative to other steps in the same
/// plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStep {
    /// Unique step identifier within the plan.
    pub id: String,

    /// Optional human-readable description of what this step does.
    #[serde(default)]
    pub description: Option<String>,

    /// Plugin name to execute (e.g., `"technical-review"`, `"security-review"`).
    pub plugin: String,

    /// Plugin-specific configuration as an arbitrary JSON/YAML object.
    ///
    /// The shape of this value is defined by the individual plugin.
    #[serde(default)]
    pub config: Option<serde_json::Value>,

    /// IDs of steps that must complete successfully before this step begins.
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Report formats for this step.
    ///
    /// Overrides the plan-level report formats when present.
    #[serde(default)]
    pub report_formats: Option<Vec<String>>,

    /// Maximum number of findings to report for this step.
    #[serde(default)]
    pub max_findings: Option<u32>,

    /// Minimum severity threshold (`info`, `low`, `medium`, `high`, `critical`).
    ///
    /// Findings below this severity are omitted from the report.
    #[serde(default)]
    pub severity_threshold: Option<String>,
}

/// Scanner configuration overrides for a plan.
///
/// All fields are optional. Omitted fields fall back to the global scanner
/// defaults defined in the tool configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanScanOptions {
    /// Include hidden files (dotfiles and hidden directories) in the scan.
    #[serde(default)]
    pub include_hidden: Option<bool>,

    /// Maximum file size in bytes to include in the scan.
    ///
    /// Files larger than this threshold are skipped.
    #[serde(default)]
    pub max_file_size_bytes: Option<u64>,

    /// Additional gitignore-style patterns to exclude from the scan.
    #[serde(default)]
    pub ignore_patterns: Option<Vec<String>>,
}

/// Report output configuration for a plan.
///
/// All fields are optional. Omitted fields fall back to workspace defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanReportOptions {
    /// Output directory for reports.
    ///
    /// Overrides the workspace default when specified.
    #[serde(default)]
    pub output_dir: Option<String>,

    /// Output formats to generate (`json`, `markdown`, `sarif`).
    #[serde(default)]
    pub formats: Option<Vec<String>>,

    /// When `true`, overwrites existing report files instead of failing.
    #[serde(default)]
    pub overwrite: Option<bool>,
}

impl WorkflowPlan {
    /// Validates the structural correctness of the plan.
    ///
    /// The following checks are performed in order:
    ///
    /// 1. `version` must equal [`PLAN_VERSION`] (`"1"`).
    /// 2. `name` must not be empty or whitespace-only.
    /// 3. `steps` must contain at least one entry.
    /// 4. Every step `id` must be unique within the plan.
    /// 5. Every step `plugin` value must be a non-empty string.
    /// 6. Every step dependency ID must reference a step that exists in the
    ///    plan.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workflow`] with a descriptive message if any
    /// validation check fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workflow::plan::WorkflowPlan;
    ///
    /// let plan = WorkflowPlan::direct_plugin_invocation("technical-review", ".");
    /// assert!(plan.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<()> {
        if self.version != PLAN_VERSION {
            return Err(PipelineError::Workflow(format!(
                "plan version '{}' is not supported; expected version '{}'",
                self.version, PLAN_VERSION
            )));
        }

        if self.name.trim().is_empty() {
            return Err(PipelineError::Workflow(
                "workflow plan name must not be empty".to_string(),
            ));
        }

        if self.steps.is_empty() {
            return Err(PipelineError::Workflow(
                "workflow plan must contain at least one step".to_string(),
            ));
        }

        let mut ids: HashSet<&str> = HashSet::new();
        for step in &self.steps {
            if step.plugin.trim().is_empty() {
                return Err(PipelineError::Workflow(format!(
                    "step '{}' has an empty plugin name",
                    step.id
                )));
            }
            if !ids.insert(step.id.as_str()) {
                return Err(PipelineError::Workflow(format!(
                    "duplicate step id '{}'",
                    step.id
                )));
            }
        }

        for step in &self.steps {
            for dep in &step.dependencies {
                if !ids.contains(dep.as_str()) {
                    return Err(PipelineError::Workflow(format!(
                        "step '{}' depends on unknown step id '{}'",
                        step.id, dep
                    )));
                }
            }
        }

        Ok(())
    }

    /// Returns `true` when the plan is configured for a dry run.
    ///
    /// In dry-run mode the executor skips provider invocations and report
    /// writes, making it safe to test plan structure without side effects.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workflow::plan::WorkflowPlan;
    ///
    /// let mut plan = WorkflowPlan::direct_plugin_invocation("technical-review", ".");
    /// assert!(!plan.is_dry_run());
    /// plan.dry_run = true;
    /// assert!(plan.is_dry_run());
    /// ```
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    /// Creates a minimal single-step [`WorkflowPlan`] for direct plugin
    /// invocation from CLI arguments.
    ///
    /// This constructor is used when the user runs
    /// `xzardgz run --plugin <name>` without supplying a plan file. The
    /// resulting plan is immediately valid and can be passed directly to the
    /// executor.
    ///
    /// # Arguments
    ///
    /// * `plugin` - The plugin name to invoke.
    /// * `repository` - The repository path or URL to process.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workflow::plan::WorkflowPlan;
    ///
    /// let plan = WorkflowPlan::direct_plugin_invocation("security-review", ".");
    /// assert_eq!(plan.steps.len(), 1);
    /// assert_eq!(plan.steps[0].plugin, "security-review");
    /// assert!(plan.validate().is_ok());
    /// ```
    pub fn direct_plugin_invocation(plugin: &str, repository: &str) -> Self {
        Self {
            version: PLAN_VERSION.to_string(),
            name: format!("direct-{}", plugin),
            description: Some(format!("Direct invocation of plugin '{}'", plugin)),
            repository: repository.to_string(),
            branch: None,
            workspace: None,
            provider: None,
            model: None,
            scan: None,
            steps: vec![PluginStep {
                id: "step1".to_string(),
                description: Some(format!("Run plugin '{}'", plugin)),
                plugin: plugin.to_string(),
                config: None,
                dependencies: vec![],
                report_formats: None,
                max_findings: None,
                severity_threshold: None,
            }],
            reports: None,
            dry_run: false,
            resume: false,
            correlation_id: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_plan() -> WorkflowPlan {
        WorkflowPlan {
            version: PLAN_VERSION.to_string(),
            name: "Test Plan".to_string(),
            description: None,
            repository: ".".to_string(),
            branch: None,
            workspace: None,
            provider: None,
            model: None,
            scan: None,
            steps: vec![PluginStep {
                id: "step1".to_string(),
                description: None,
                plugin: "technical-review".to_string(),
                config: None,
                dependencies: vec![],
                report_formats: None,
                max_findings: None,
                severity_threshold: None,
            }],
            reports: None,
            dry_run: false,
            resume: false,
            correlation_id: None,
        }
    }

    #[test]
    fn test_workflow_plan_validate_accepts_valid_plan() {
        let plan = valid_plan();
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn test_workflow_plan_validate_rejects_wrong_version() {
        let mut plan = valid_plan();
        plan.version = "2".to_string();
        let result = plan.validate();
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("plan version '2' is not supported"),
            "expected version rejection message, got: {msg}"
        );
        assert!(
            msg.contains("expected version '1'"),
            "expected hint about version 1, got: {msg}"
        );
    }

    #[test]
    fn test_workflow_plan_validate_rejects_empty_name() {
        let mut plan = valid_plan();
        plan.name = "   ".to_string();
        let result = plan.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("name must not be empty"),
            "expected empty-name error message"
        );
    }

    #[test]
    fn test_workflow_plan_validate_rejects_empty_steps() {
        let mut plan = valid_plan();
        plan.steps = vec![];
        let result = plan.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("at least one step"),
            "expected empty-steps error message"
        );
    }

    #[test]
    fn test_workflow_plan_validate_rejects_duplicate_step_ids() {
        let mut plan = valid_plan();
        plan.steps.push(PluginStep {
            id: "step1".to_string(),
            description: None,
            plugin: "security-review".to_string(),
            config: None,
            dependencies: vec![],
            report_formats: None,
            max_findings: None,
            severity_threshold: None,
        });
        let result = plan.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("duplicate step id 'step1'"),
            "expected duplicate-id error message"
        );
    }

    #[test]
    fn test_workflow_plan_validate_rejects_unknown_dependency() {
        let mut plan = valid_plan();
        plan.steps[0].dependencies = vec!["nonexistent".to_string()];
        let result = plan.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unknown step id 'nonexistent'"),
            "expected unknown-dependency error message"
        );
    }

    #[test]
    fn test_workflow_plan_is_dry_run_returns_flag_value() {
        let mut plan = valid_plan();
        assert!(!plan.is_dry_run(), "dry_run should default to false");
        plan.dry_run = true;
        assert!(plan.is_dry_run(), "dry_run should be true after assignment");
    }

    #[test]
    fn test_direct_plugin_invocation_creates_single_step_plan() {
        let plan = WorkflowPlan::direct_plugin_invocation("security-review", "/repo");
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].plugin, "security-review");
        assert_eq!(plan.repository, "/repo");
        assert_eq!(plan.version, PLAN_VERSION);
        assert!(
            plan.validate().is_ok(),
            "direct invocation plan should be immediately valid"
        );
    }
}
