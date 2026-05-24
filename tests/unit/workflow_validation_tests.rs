//! Workflow validation integration tests.
//!
//! Tests covering [`WorkflowPlan`] structural validation, the
//! [`build_direct_invocation_plan`] builder, and legacy action detection via
//! [`check_for_legacy_actions`].

use xzardgz::workflow::plan::{PLAN_VERSION, PluginStep, WorkflowPlan};
use xzardgz::workflow::validator::{
    build_direct_invocation_plan, check_for_legacy_actions, validate_plan,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Constructs a minimal, immediately-valid [`WorkflowPlan`] for use in tests.
fn make_valid_plan() -> WorkflowPlan {
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
    }
}

// ---------------------------------------------------------------------------
// validate_plan tests
// ---------------------------------------------------------------------------

/// A valid plan with one step passes validation without error.
#[test]
fn test_validate_plan_passes_for_valid_plan() {
    let plan = make_valid_plan();
    let result = validate_plan(&plan);
    assert!(
        result.is_ok(),
        "valid plan should pass validation, got: {:?}",
        result.err()
    );
}

/// A plan with no steps is rejected with a descriptive error.
#[test]
fn test_validate_plan_rejects_empty_steps() {
    let mut plan = make_valid_plan();
    plan.steps = vec![];
    let result = validate_plan(&plan);
    assert!(result.is_err(), "plan with no steps should be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("at least one step"),
        "error should mention missing steps, got: {msg}"
    );
}

/// A plan where two steps share the same ID is rejected.
#[test]
fn test_validate_plan_rejects_duplicate_step_ids() {
    let mut plan = make_valid_plan();
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
    let result = validate_plan(&plan);
    assert!(
        result.is_err(),
        "plan with duplicate step ids should be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("duplicate step id"),
        "error should mention duplicate id, got: {msg}"
    );
}

/// A step dependency referencing a non-existent step ID is rejected.
#[test]
fn test_validate_plan_rejects_unknown_dependency() {
    let mut plan = make_valid_plan();
    plan.steps[0].dependencies = vec!["nonexistent-step".to_string()];
    let result = validate_plan(&plan);
    assert!(
        result.is_err(),
        "plan with unknown dependency should be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("unknown step id"),
        "error should mention unknown dependency, got: {msg}"
    );
}

/// A plan declaring an unsupported schema version is rejected.
#[test]
fn test_validate_plan_rejects_wrong_version() {
    let mut plan = make_valid_plan();
    plan.version = "99".to_string();
    let result = validate_plan(&plan);
    assert!(
        result.is_err(),
        "plan with unsupported version should be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("is not supported"),
        "error should describe the version problem, got: {msg}"
    );
}

/// A plan with an empty or whitespace-only name is rejected.
#[test]
fn test_validate_plan_rejects_empty_name() {
    let mut plan = make_valid_plan();
    plan.name = "   ".to_string();
    let result = validate_plan(&plan);
    assert!(result.is_err(), "plan with empty name should be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("name must not be empty"),
        "error should mention empty name, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// build_direct_invocation_plan tests
// ---------------------------------------------------------------------------

/// The builder produces a plan that passes validation immediately.
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
    assert_eq!(plan.steps.len(), 1, "plan should have exactly one step");
    assert_eq!(plan.steps[0].plugin, "security-review");
    assert_eq!(plan.repository, ".");
    let result = validate_plan(&plan);
    assert!(
        result.is_ok(),
        "directly-built plan should pass validation, got: {:?}",
        result.err()
    );
}

/// When `dry_run` is `true`, the resulting plan reflects that flag.
#[test]
fn test_build_direct_invocation_plan_with_dry_run_flag() {
    let plan = build_direct_invocation_plan(
        "technical-review",
        "/repo",
        None,
        None,
        None,
        None,
        true,
        None,
        vec![],
    );
    assert!(plan.dry_run, "plan should have dry_run set to true");
    assert!(
        plan.is_dry_run(),
        "is_dry_run() should return true for a dry-run plan"
    );
    assert!(
        validate_plan(&plan).is_ok(),
        "dry-run plan should still be valid"
    );
}

/// Report formats supplied to the builder are stored on the step and in the
/// plan-level report configuration.
#[test]
fn test_build_direct_invocation_plan_with_report_formats() {
    let formats = vec!["json".to_string(), "markdown".to_string()];
    let plan = build_direct_invocation_plan(
        "technical-review",
        ".",
        None,
        None,
        None,
        None,
        false,
        None,
        formats.clone(),
    );

    let step_formats = plan.steps[0]
        .report_formats
        .as_ref()
        .expect("step should have report_formats when formats are provided");
    assert_eq!(
        step_formats, &formats,
        "step report formats should match the supplied list"
    );

    let plan_formats = plan
        .reports
        .as_ref()
        .and_then(|r| r.formats.as_ref())
        .expect("plan should have report formats when formats are provided");
    assert_eq!(
        plan_formats, &formats,
        "plan report formats should match the supplied list"
    );
    assert!(
        validate_plan(&plan).is_ok(),
        "plan with report formats should be valid"
    );
}

// ---------------------------------------------------------------------------
// check_for_legacy_actions tests
// ---------------------------------------------------------------------------

/// The YAML form of the legacy `scan_repository` action type is rejected.
#[test]
fn test_check_for_legacy_actions_rejects_yaml_scan_repository() {
    let yaml = "action:\n  type: scan_repository\n";
    let result = check_for_legacy_actions(yaml);
    assert!(result.is_err(), "legacy scan_repository should be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("scan_repository"),
        "error should name the rejected type, got: {msg}"
    );
}

/// The JSON form of the legacy `execute_command` action type is rejected.
#[test]
fn test_check_for_legacy_actions_rejects_json_execute_command() {
    let json = r#"{ "type": "execute_command", "params": {} }"#;
    let result = check_for_legacy_actions(json);
    assert!(
        result.is_err(),
        "legacy execute_command in JSON should be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("execute_command"),
        "error should name the rejected type, got: {msg}"
    );
}

/// Valid plugin-first content that contains no legacy action patterns is
/// accepted without error.
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

/// The `agent_task` legacy type is also detected and rejected.
#[test]
fn test_check_for_legacy_actions_rejects_yaml_agent_task() {
    let yaml = "steps:\n  - id: s1\n    action:\n      type: agent_task\n";
    let result = check_for_legacy_actions(yaml);
    assert!(result.is_err(), "legacy agent_task should be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("agent_task"),
        "error should name the rejected type, got: {msg}"
    );
}

/// The `run_plugin` legacy type is also detected and rejected.
#[test]
fn test_check_for_legacy_actions_rejects_yaml_run_plugin() {
    let yaml = "steps:\n  - id: s1\n    action:\n      type: run_plugin\n";
    let result = check_for_legacy_actions(yaml);
    assert!(result.is_err(), "legacy run_plugin should be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("run_plugin"),
        "error should name the rejected type, got: {msg}"
    );
}
