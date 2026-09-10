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
        correlation_id: None,
    }
}

/// Applies CLI-level overrides onto an already-constructed or parsed
/// [`WorkflowPlan`], in place.
///
/// Used by the `run` command handler to merge `--branch`, `--workspace`,
/// `--dry-run`, `--resume`, `--max-findings`, `--report-format`, and
/// `--output-dir` CLI flags onto a plan, whether it came from
/// [`build_direct_invocation_plan`] or was parsed from a plan file. Only
/// supplied overrides are applied; omitted ones (`None`, `false`, or empty)
/// leave the plan's existing value unchanged.
///
/// `dry_run` and `resume` are one-directional: passing `true` forces the
/// corresponding field to `true`, but passing `false` never forces it back
/// to `false` -- a plan file's own `dry_run: true` / `resume: true` is never
/// silently undone by the flag's absence on the command line.
///
/// # Arguments
///
/// * `plan` - The plan to mutate in place.
/// * `branch` - Optional branch override.
/// * `workspace` - Optional workspace root override.
/// * `dry_run` - When `true`, forces `plan.dry_run = true`.
/// * `resume` - When `true`, forces `plan.resume = true`.
/// * `max_findings` - Optional per-step finding cap, applied to every step.
/// * `report_formats` - Report formats; applied to every step and the
///   plan-level report options when non-empty.
/// * `output_dir` - Optional report output directory override.
///
/// # Examples
///
/// ```
/// use xzardgz::workflow::plan::WorkflowPlan;
/// use xzardgz::workflow::validator::apply_run_overrides;
///
/// let mut plan = WorkflowPlan::direct_plugin_invocation("security-review", ".");
/// apply_run_overrides(&mut plan, None, None, true, false, None, vec![], None);
/// assert!(plan.dry_run);
/// ```
#[allow(clippy::too_many_arguments)]
pub fn apply_run_overrides(
    plan: &mut WorkflowPlan,
    branch: Option<String>,
    workspace: Option<String>,
    dry_run: bool,
    resume: bool,
    max_findings: Option<u32>,
    report_formats: Vec<String>,
    output_dir: Option<String>,
) {
    if branch.is_some() {
        plan.branch = branch;
    }
    if workspace.is_some() {
        plan.workspace = workspace;
    }
    if dry_run {
        plan.dry_run = true;
    }
    if resume {
        plan.resume = true;
    }
    if let Some(max_findings) = max_findings {
        for step in &mut plan.steps {
            step.max_findings = Some(max_findings);
        }
    }
    if !report_formats.is_empty() {
        for step in &mut plan.steps {
            step.report_formats = Some(report_formats.clone());
        }
    }
    if !report_formats.is_empty() || output_dir.is_some() {
        let formats = if report_formats.is_empty() {
            None
        } else {
            Some(report_formats)
        };
        match plan.reports.as_mut() {
            Some(r) => {
                if formats.is_some() {
                    r.formats = formats;
                }
                if output_dir.is_some() {
                    r.output_dir = output_dir;
                }
            }
            None => {
                plan.reports = Some(PlanReportOptions {
                    output_dir,
                    formats,
                    overwrite: None,
                });
            }
        }
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

    // ------------------------------------------------------------------
    // apply_run_overrides
    // ------------------------------------------------------------------

    #[test]
    fn test_apply_run_overrides_sets_branch_and_workspace() {
        let mut plan = WorkflowPlan::direct_plugin_invocation("security-review", ".");
        apply_run_overrides(
            &mut plan,
            Some("feature-x".to_string()),
            Some("/tmp/ws".to_string()),
            false,
            false,
            None,
            vec![],
            None,
        );
        assert_eq!(plan.branch.as_deref(), Some("feature-x"));
        assert_eq!(plan.workspace.as_deref(), Some("/tmp/ws"));
    }

    #[test]
    fn test_apply_run_overrides_true_flags_force_true_but_omitted_does_not_reset() {
        let mut plan = WorkflowPlan::direct_plugin_invocation("security-review", ".");
        plan.dry_run = true;
        plan.resume = true;

        // Passing `false` for both must not undo the plan's existing `true`.
        apply_run_overrides(&mut plan, None, None, false, false, None, vec![], None);
        assert!(plan.dry_run, "false must not reset an existing true");
        assert!(plan.resume, "false must not reset an existing true");

        let mut fresh = WorkflowPlan::direct_plugin_invocation("security-review", ".");
        apply_run_overrides(&mut fresh, None, None, true, true, None, vec![], None);
        assert!(fresh.dry_run);
        assert!(fresh.resume);
    }

    #[test]
    fn test_apply_run_overrides_applies_max_findings_to_every_step() {
        let mut plan = WorkflowPlan::direct_plugin_invocation("security-review", ".");
        plan.steps.push(PluginStep {
            id: "step2".to_string(),
            description: None,
            plugin: "technical-review".to_string(),
            config: None,
            dependencies: vec![],
            report_formats: None,
            max_findings: None,
            severity_threshold: None,
        });

        apply_run_overrides(&mut plan, None, None, false, false, Some(25), vec![], None);

        for step in &plan.steps {
            assert_eq!(step.max_findings, Some(25));
        }
    }

    #[test]
    fn test_apply_run_overrides_sets_report_formats_on_steps_and_plan() {
        let mut plan = WorkflowPlan::direct_plugin_invocation("security-review", ".");
        apply_run_overrides(
            &mut plan,
            None,
            None,
            false,
            false,
            None,
            vec!["json".to_string(), "sarif".to_string()],
            None,
        );

        assert_eq!(
            plan.steps[0].report_formats,
            Some(vec!["json".to_string(), "sarif".to_string()])
        );
        let reports = plan.reports.expect("reports options must be populated");
        assert_eq!(
            reports.formats,
            Some(vec!["json".to_string(), "sarif".to_string()])
        );
    }

    #[test]
    fn test_apply_run_overrides_sets_output_dir_without_clobbering_existing_formats() {
        let mut plan = WorkflowPlan::direct_plugin_invocation("security-review", ".");
        plan.reports = Some(PlanReportOptions {
            output_dir: None,
            formats: Some(vec!["markdown".to_string()]),
            overwrite: None,
        });

        apply_run_overrides(
            &mut plan,
            None,
            None,
            false,
            false,
            None,
            vec![],
            Some("/tmp/out".to_string()),
        );

        let reports = plan.reports.expect("reports options must remain populated");
        assert_eq!(reports.output_dir.as_deref(), Some("/tmp/out"));
        assert_eq!(reports.formats, Some(vec!["markdown".to_string()]));
    }

    #[test]
    fn test_apply_run_overrides_no_overrides_leaves_plan_unchanged() {
        let plan = WorkflowPlan::direct_plugin_invocation("security-review", ".");
        let mut mutated = plan.clone();
        apply_run_overrides(&mut mutated, None, None, false, false, None, vec![], None);
        assert_eq!(mutated.branch, plan.branch);
        assert_eq!(mutated.workspace, plan.workspace);
        assert_eq!(mutated.dry_run, plan.dry_run);
        assert_eq!(mutated.resume, plan.resume);
        assert_eq!(mutated.steps[0].max_findings, plan.steps[0].max_findings);
        assert_eq!(
            mutated.steps[0].report_formats,
            plan.steps[0].report_formats
        );
    }
}
