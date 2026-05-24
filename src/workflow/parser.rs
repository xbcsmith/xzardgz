use crate::error::{PipelineError, Result};
use crate::workflow::plan::Plan;

/// Trait for types that can parse a workflow plan from a content string.
pub trait PlanParser {
    /// Parses a workflow [`Plan`] from the given `content` string.
    fn parse(&self, content: &str) -> Result<Plan>;
}

/// Parses workflow plans from YAML-formatted strings.
pub struct YamlPlanParser;

impl PlanParser for YamlPlanParser {
    fn parse(&self, content: &str) -> Result<Plan> {
        serde_yaml::from_str(content).map_err(|e| PipelineError::Workflow(e.to_string()))
    }
}

/// Parses workflow plans from JSON-formatted strings.
pub struct JsonPlanParser;

impl PlanParser for JsonPlanParser {
    fn parse(&self, content: &str) -> Result<Plan> {
        serde_json::from_str(content).map_err(|e| PipelineError::Workflow(e.to_string()))
    }
}

/// Parses workflow plans from Markdown files that contain a YAML or JSON fenced code block.
pub struct MarkdownPlanParser;

impl PlanParser for MarkdownPlanParser {
    fn parse(&self, content: &str) -> Result<Plan> {
        let lines: Vec<&str> = content.lines().collect();
        let mut in_block = false;
        let mut block_content = String::new();
        let mut format = "yaml";

        for line in lines {
            if line.trim().starts_with("```") {
                if in_block {
                    break;
                } else {
                    in_block = true;
                    let lang = line.trim().trim_start_matches("```").trim();
                    if lang == "json" {
                        format = "json";
                    }
                    continue;
                }
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
/// Supported formats: `yaml`, `yml`, `json`, `md`, `markdown`.
///
/// Returns `PipelineError::Workflow` for unsupported format strings or parse failures.
pub fn parse_plan(content: &str, format: &str) -> Result<Plan> {
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
