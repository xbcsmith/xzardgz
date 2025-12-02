use crate::error::XzardgzError;
use crate::workflow::plan::Plan;
use serde_json;
use serde_yaml;

pub trait PlanParser {
    fn parse(&self, content: &str) -> Result<Plan, XzardgzError>;
}

pub struct YamlPlanParser;

impl PlanParser for YamlPlanParser {
    fn parse(&self, content: &str) -> Result<Plan, XzardgzError> {
        serde_yaml::from_str(content)
            .map_err(|e| XzardgzError::Workflow(crate::error::WorkflowError::Parse(e.to_string())))
    }
}

pub struct JsonPlanParser;

impl PlanParser for JsonPlanParser {
    fn parse(&self, content: &str) -> Result<Plan, XzardgzError> {
        serde_json::from_str(content)
            .map_err(|e| XzardgzError::Workflow(crate::error::WorkflowError::Parse(e.to_string())))
    }
}

pub struct MarkdownPlanParser;

impl PlanParser for MarkdownPlanParser {
    fn parse(&self, content: &str) -> Result<Plan, XzardgzError> {
        // Simple extraction: look for ```yaml or ```json blocks
        let lines: Vec<&str> = content.lines().collect();
        let mut in_block = false;
        let mut block_content = String::new();
        let mut format = "yaml"; // Default to yaml if unspecified but inside block

        for line in lines {
            if line.trim().starts_with("```") {
                if in_block {
                    // End of block
                    break;
                } else {
                    // Start of block
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
            return Err(XzardgzError::Workflow(crate::error::WorkflowError::Parse(
                "No code block found in markdown".to_string(),
            )));
        }

        parse_plan(&block_content, format)
    }
}

pub fn parse_plan(content: &str, format: &str) -> Result<Plan, XzardgzError> {
    match format {
        "yaml" | "yml" => YamlPlanParser.parse(content),
        "json" => JsonPlanParser.parse(content),
        "md" | "markdown" => MarkdownPlanParser.parse(content),
        _ => Err(XzardgzError::Workflow(crate::error::WorkflowError::Parse(
            format!("Unsupported format: {}", format),
        ))),
    }
}
