//! Workflow plan parsers for YAML, JSON, and Markdown sources.
//!
//! This module provides the [`PlanParser`] trait and three concrete
//! implementations: [`YamlPlanParser`], [`JsonPlanParser`], and
//! [`MarkdownPlanParser`]. Each parser checks for legacy action types before
//! deserialization and validates the resulting plan before returning it.
//!
//! Use [`parse_plan`] to select the correct parser by format string.

use crate::error::{PipelineError, Result};
use crate::workflow::plan::WorkflowPlan;
use crate::workflow::validator::{check_for_legacy_actions, validate_plan};

/// Trait for types that can parse a [`WorkflowPlan`] from a content string.
///
/// Implementations must:
/// 1. Check for legacy action types before deserialization.
/// 2. Deserialize the content into a [`WorkflowPlan`].
/// 3. Validate the resulting plan.
pub trait PlanParser {
    /// Parses a [`WorkflowPlan`] from the given `content` string.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workflow`] if legacy action types are detected,
    /// deserialization fails, or plan validation fails.
    fn parse(&self, content: &str) -> Result<WorkflowPlan>;
}

/// Parses workflow plans from YAML-formatted strings.
///
/// Checks for legacy action type patterns and validates the resulting plan
/// before returning it.
pub struct YamlPlanParser;

impl PlanParser for YamlPlanParser {
    /// Parses a [`WorkflowPlan`] from `content` as YAML.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workflow`] if:
    /// - `content` contains a legacy `action: { type: ... }` block.
    /// - `content` is not valid YAML or does not match the [`WorkflowPlan`] shape.
    /// - The parsed plan fails structural validation.
    fn parse(&self, content: &str) -> Result<WorkflowPlan> {
        check_for_legacy_actions(content)?;
        let plan: WorkflowPlan =
            serde_yaml::from_str(content).map_err(|e| PipelineError::Workflow(e.to_string()))?;
        validate_plan(&plan)?;
        Ok(plan)
    }
}

/// Parses workflow plans from JSON-formatted strings.
///
/// Checks for legacy action type patterns and validates the resulting plan
/// before returning it.
pub struct JsonPlanParser;

impl PlanParser for JsonPlanParser {
    /// Parses a [`WorkflowPlan`] from `content` as JSON.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workflow`] if:
    /// - `content` contains a legacy `"type": "..."` action block.
    /// - `content` is not valid JSON or does not match the [`WorkflowPlan`] shape.
    /// - The parsed plan fails structural validation.
    fn parse(&self, content: &str) -> Result<WorkflowPlan> {
        check_for_legacy_actions(content)?;
        let plan: WorkflowPlan =
            serde_json::from_str(content).map_err(|e| PipelineError::Workflow(e.to_string()))?;
        validate_plan(&plan)?;
        Ok(plan)
    }
}

/// Parses workflow plans from Markdown files that contain a YAML or JSON
/// fenced code block.
///
/// The parser extracts the first fenced code block from the Markdown source
/// and forwards its content to [`parse_plan`] with the detected format.
pub struct MarkdownPlanParser;

impl PlanParser for MarkdownPlanParser {
    /// Parses a [`WorkflowPlan`] from the first fenced code block in `content`.
    ///
    /// The opening fence line may include a language tag (`yaml`, `json`, or
    /// nothing). When no language tag is present, YAML is assumed.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workflow`] if:
    /// - No fenced code block is found.
    /// - The extracted block fails any check performed by [`YamlPlanParser`]
    ///   or [`JsonPlanParser`].
    fn parse(&self, content: &str) -> Result<WorkflowPlan> {
        let mut in_block = false;
        let mut block_content = String::new();
        let mut format = "yaml";

        for line in content.lines() {
            if line.trim().starts_with("```") {
                if in_block {
                    break;
                }
                in_block = true;
                let lang = line.trim().trim_start_matches("```").trim();
                if lang == "json" {
                    format = "json";
                }
                continue;
            }
            if in_block {
                block_content.push_str(line);
                block_content.push('\n');
            }
        }

        if block_content.is_empty() {
            return Err(PipelineError::Workflow(
                "no code block found in markdown".to_string(),
            ));
        }

        parse_plan(&block_content, format)
    }
}

/// Parses a workflow plan from `content` using the specified `format`.
///
/// This function selects the correct [`PlanParser`] implementation and
/// delegates to it. Legacy action detection and plan validation are performed
/// inside the chosen parser.
///
/// # Arguments
///
/// * `content` - The raw plan file content.
/// * `format` - The format string. Supported values: `yaml`, `yml`, `json`,
///   `md`, `markdown`.
///
/// # Errors
///
/// Returns [`PipelineError::Workflow`] for unsupported format strings or any
/// failure detected by the underlying parser.
///
/// # Examples
///
/// ```
/// use xzardgz::workflow::parser::parse_plan;
///
/// let yaml = "version: \"1\"\nname: My Plan\nrepository: \".\"\nsteps:\n  - id: s1\n    plugin: technical-review\n";
/// let plan = parse_plan(yaml, "yaml").unwrap();
/// assert_eq!(plan.name, "My Plan");
/// ```
pub fn parse_plan(content: &str, format: &str) -> Result<WorkflowPlan> {
    match format {
        "yaml" | "yml" => YamlPlanParser.parse(content),
        "json" => JsonPlanParser.parse(content),
        "md" | "markdown" => MarkdownPlanParser.parse(content),
        _ => Err(PipelineError::Workflow(format!(
            "unsupported format: {}",
            format
        ))),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_YAML: &str = concat!(
        "version: \"1\"\n",
        "name: Test Plan\n",
        "repository: \".\"\n",
        "steps:\n",
        "  - id: step1\n",
        "    plugin: technical-review\n",
    );

    const VALID_JSON: &str = r#"{
        "version": "1",
        "name": "Test Plan",
        "repository": ".",
        "steps": [
            {
                "id": "step1",
                "plugin": "technical-review"
            }
        ]
    }"#;

    #[test]
    fn test_yaml_parser_parses_valid_plugin_first_plan() {
        let result = YamlPlanParser.parse(VALID_YAML);
        assert!(
            result.is_ok(),
            "valid YAML plan should parse: {:?}",
            result.err()
        );
        let plan = result.unwrap();
        assert_eq!(plan.name, "Test Plan");
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].plugin, "technical-review");
    }

    #[test]
    fn test_json_parser_parses_valid_plugin_first_plan() {
        let result = JsonPlanParser.parse(VALID_JSON);
        assert!(
            result.is_ok(),
            "valid JSON plan should parse: {:?}",
            result.err()
        );
        let plan = result.unwrap();
        assert_eq!(plan.name, "Test Plan");
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].plugin, "technical-review");
    }

    #[test]
    fn test_yaml_parser_rejects_legacy_scan_repository_action() {
        let yaml = concat!(
            "name: Legacy Plan\n",
            "description: A plan using old action format\n",
            "steps:\n",
            "  - id: step1\n",
            "    description: Scan repo\n",
            "    action:\n",
            "      type: scan_repository\n",
            "      params: null\n",
        );
        let result = YamlPlanParser.parse(yaml);
        assert!(
            result.is_err(),
            "legacy scan_repository action must be rejected"
        );
        assert!(
            result.unwrap_err().to_string().contains("scan_repository"),
            "error message should name the legacy type"
        );
    }

    #[test]
    fn test_yaml_parser_rejects_legacy_generate_docs_action() {
        let yaml = concat!(
            "name: Legacy Doc Gen Plan\n",
            "description: A plan using the removed documentation generation action\n",
            "steps:\n",
            "  - id: step1\n",
            "    description: Generate docs\n",
            "    action:\n",
            "      type: generate_docs\n",
            "      params:\n",
            "        category: tutorial\n",
        );
        let result = YamlPlanParser.parse(yaml);
        assert!(
            result.is_err(),
            "legacy generate_docs action must be rejected"
        );
        assert!(
            result.unwrap_err().to_string().contains("generate_docs"),
            "error message should name the legacy type"
        );
    }

    #[test]
    fn test_yaml_parser_rejects_wrong_version() {
        let yaml = concat!(
            "version: \"2\"\n",
            "name: Test Plan\n",
            "repository: \".\"\n",
            "steps:\n",
            "  - id: step1\n",
            "    plugin: technical-review\n",
        );
        let result = YamlPlanParser.parse(yaml);
        assert!(result.is_err(), "plan with version 2 must be rejected");
        assert!(
            result.unwrap_err().to_string().contains("is not supported"),
            "error message should describe the version rejection"
        );
    }

    #[test]
    fn test_markdown_parser_parses_valid_plan() {
        let md = concat!(
            "# My Workflow Plan\n",
            "\n",
            "This plan runs a technical review.\n",
            "\n",
            "```yaml\n",
            "version: \"1\"\n",
            "name: Test Plan\n",
            "repository: \".\"\n",
            "steps:\n",
            "  - id: step1\n",
            "    plugin: technical-review\n",
            "```\n",
        );
        let result = MarkdownPlanParser.parse(md);
        assert!(
            result.is_ok(),
            "valid markdown plan should parse: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap().name, "Test Plan");
    }

    #[test]
    fn test_parse_plan_rejects_unsupported_format() {
        let result = parse_plan("anything", "toml");
        assert!(result.is_err(), "unsupported format must be rejected");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unsupported format"),
            "error should describe the format problem"
        );
    }
}
