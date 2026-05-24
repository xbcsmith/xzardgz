//! Workflow plan validation and direct invocation builder.
//!
//! This module provides functions to detect legacy workflow action types
//! before deserialization, structurally validate parsed [`WorkflowPlan`]
//! instances, and construct plans programmatically for CLI direct invocation.

use crate::error::{PipelineError, Result};
use crate::workflow::plan::{PLAN_VERSION, PlanReportOptions, PluginStep, WorkflowPlan};

/// Legacy action type strings from the pre-version-1 workflow plan format.
///
/// These strings are searched for in raw plan content before deserialization
/// to provide early, helpful rejection messages.
const LEGACY_TYPES: &[&str] = &[
    "scan_repository",
    "analyze_code",
    "run_plugin",
    "execute_command",
    "agent_task",
    "generate_docs",
];

/// Checks raw plan content (YAML or JSON string) for legacy action type fields.
///
/// This function runs **before** deserialization to provide helpful rejection
/// messages when a plan file uses the old `action: { type: ... }` format that
/// was replaced by the plugin-first step model in schema version 1.
///
/// Detection strategy: the content is scanned for patterns of the form
/// `type: <legacy>` (YAML) or `"type": "<legacy>"` (JSON). If either pattern
/// is found the content is rejected with a migration hint.
///
/// # Arguments
///
/// * `raw_content` - Raw plan file content as a YAML or JSON string.
///
/// # Errors
///
/// Returns [`PipelineError::Workflow`] with a migration hint if any legacy
/// action type pattern is detected in `raw_content`.
///
/// # Examples
///
/// ```
/// use xzardgz::workflow::validator::check_for_legacy_actions;
///
/// let legacy = "action:\n  type: scan_repository\n";
/// assert!(check_for_legacy_actions(legacy).is_err());
///
/// let modern = "version: \"1\"\nsteps:\n  - id: s1\n    plugin: technical-review\n";
/// assert!(check_for_legacy_actions(modern).is_ok());
/// ```
pub fn check_for_legacy_actions(raw_content: &str) -> Result<()> {
    for &legacy_type in LEGACY_TYPES {
        // YAML form: `type: scan_repository`
        let yaml_pattern = format!("type: {}", legacy_type);
        // JSON form: `"type": "scan_repository"`
        let json_pattern = format!("\"type\": \"{}\"", legacy_type);

        if raw_content.contains(&yaml_pattern) || raw_content.contains(&json_pattern) {
            return Err(PipelineError::Workflow(format!(
                "legacy workflow action type '{}' is not supported in version 1 plans; \
                 migrate to plugin-first step format with 'plugin: {}' field",
                legacy_type, legacy_type
            )));
        }
    }
    Ok(())
}

/// Validates a [`WorkflowPlan`] for structural correctness.
///
/// This is a thin wrapper around [`WorkflowPlan::validate`] that follows the
/// same calling convention as the other functions in this module, making it
/// easy to call from the parser pipeline.
///
/// # Arguments
///
/// * `plan` - A reference to the [`WorkflowPlan`] to validate.
///
/// # Errors
///
/// Returns [`PipelineError::Workflow`] if any structural check fails.
/// See [`WorkflowPlan::validate`] for the full list of checks performed.
///
/// # Examples
///
/// ```
/// use xzardgz::workflow::plan::WorkflowPlan;
/// use xzardgz::workflow::validator::validate_plan;
///
/// let plan = WorkflowPlan::direct_plugin_invocation("technical-review", ".");
/// assert!(validate_plan(&plan).is_ok());
/// ```
pub fn validate_plan(plan: &WorkflowPlan) -> Result<()> {
    plan.validate()
}

/// Creates a [`WorkflowPlan`] from CLI override values for direct plugin
/// invocation.
///
/// Used when the user runs `xzardgz run --plugin <name>` without supplying a
/// plan file. All optional parameters correspond to CLI flags; omitting them
/// leaves the plan fields at their default values.
///
/// # Arguments
///
/// * `plugin` - Plugin name to invoke.
/// * `repository` - Repository path or URL to process.
/// * `branch` - Optional branch override.
/// * `provider` - Optional provider override (e.g., `"openai"`).
/// * `model` - Optional model identifier override.
/// * `workspace` - Optional workspace directory override.
/// * `dry_run` - When `true`, no providers are invoked and no reports are
///   written.
/// * `max_findings` - Optional per-step cap on the number of findings
///   reported.
/// * `report_formats` - Requested output formats (`"json"`, `"markdown"`,
///   `"sarif"`). An empty slice means use the workspace default.
///
/// # Returns
///
/// A fully-initialized, immediately-valid [`WorkflowPlan`] with a single
/// plugin execution step.
///
/// # Examples
///
/// ```
/// use xzardgz::workflow::validator::build_direct_invocation_plan;
///
/// let plan = build_direct_invocation_plan(
///     "security-review",
///     ".",
///     None,
///     None,
///     None,
///     None,
///     false,
///     None,
///     vec![],
/// );
/// assert_eq!(plan.steps.len(), 1);
/// assert_eq!(plan.steps[0].plugin, "security-review");
/// ```
// All nine parameters map directly to independent CLI flags. Grouping them
// into a struct would require callers to import an extra type for a single
// call site. The allow attribute is intentional.
#[allow(clippy::too_many_arguments)]
pub fn build_direct_invocation_plan(
    plugin: &str,
    repository: &str,
    branch: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    workspace: Option<String>,
    dry_run: bool,
    max_findings: Option<u32>,
    report_formats: Vec<String>,
) -> WorkflowPlan {
    let (step_report_formats, reports) = if report_formats.is_empty() {
        (None, None)
    } else {
        let step_formats = Some(report_formats.clone());
        let plan_reports = Some(PlanReportOptions {
            output_dir: None,
            formats: Some(report_formats),
            overwrite: None,
        });
        (step_formats, plan_reports)
    };

    WorkflowPlan {
        version: PLAN_VERSION.to_string(),
        name: format!("direct-{}", plugin),
        description: Some(format!("Direct invocation of plugin '{}'", plugin)),
        repository: repository.to_string(),
        branch,
        workspace,
        provider,
        model,
        scan: None,
        steps: vec![PluginStep {
            id: "step1".to_string(),
            description: Some(format!("Run plugin '{}'", plugin)),
            plugin: plugin.to_string(),
            config: None,
            dependencies: vec![],
            report_formats: step_report_formats,
            max_findings,
            severity_threshold: None,
        }],
        reports,
        dry_run,
        resume: false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_for_legacy_actions_rejects_scan_repository() {
        let yaml = "action:\n  type: scan_repository\n";
        let result = check_for_legacy_actions(yaml);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("scan_repository"),
            "error should name the rejected type"
        );
    }

    #[test]
    fn test_check_for_legacy_actions_rejects_analyze_code() {
        let yaml = "action:\n  type: analyze_code\n";
        let result = check_for_legacy_actions(yaml);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("analyze_code"),
            "error should name the rejected type"
        );
    }

    #[test]
    fn test_check_for_legacy_actions_rejects_generate_docs() {
        let yaml = "action:\n  type: generate_docs\n";
        let result = check_for_legacy_actions(yaml);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("generate_docs"),
            "error should name the rejected type"
        );
    }

    #[test]
    fn test_check_for_legacy_actions_rejects_execute_command() {
        // Test YAML form
        let yaml = "action:\n  type: execute_command\n";
        let result = check_for_legacy_actions(yaml);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("execute_command"),
            "error should name the rejected type"
        );

        // Test JSON form
        let json = "\"type\": \"execute_command\"";
        let result = check_for_legacy_actions(json);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("execute_command"),
            "JSON form should also be rejected"
        );
    }

    #[test]
    fn test_check_for_legacy_actions_accepts_plugin_first_content() {
        let yaml = concat!(
            "version: \"1\"\n",
            "name: My Plan\n",
            "repository: \".\"\n",
            "steps:\n",
            "  - id: step1\n",
            "    plugin: technical-review\n",
        );
        let result = check_for_legacy_actions(yaml);
        assert!(
            result.is_ok(),
            "plugin-first content should be accepted, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_build_direct_invocation_plan_creates_valid_plan() {
        let plan = build_direct_invocation_plan(
            "security-review",
            ".",
            None,
            None,
            None,
            None,
            false,
            None,
            vec![],
        );
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].plugin, "security-review");
        assert_eq!(plan.repository, ".");
        assert!(!plan.dry_run);
        assert!(
            validate_plan(&plan).is_ok(),
            "built plan should pass validation"
        );
    }
}
