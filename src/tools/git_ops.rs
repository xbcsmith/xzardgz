use crate::error::{PipelineError, Result};
use crate::providers::types::Tool;
use crate::tools::{ToolExecutor, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use serde_json::json;
use std::process::Command;

/// Tool that retrieves the current git status of the repository.
pub struct GitStatusTool;

impl GitStatusTool {
    /// Returns the tool definition for `git_status`, including its JSON parameter schema.
    pub fn definition() -> Tool {
        Tool {
            name: "git_status".to_string(),
            description: "Get git status".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }
}

#[async_trait]
impl ToolExecutor for GitStatusTool {
    async fn execute(&self, _params: Value) -> Result<ToolResult> {
        let output = Command::new("git")
            .arg("status")
            .output()
            .map_err(|e| PipelineError::Git(e.to_string()))?;

        if output.status.success() {
            Ok(ToolResult::success(String::from_utf8_lossy(&output.stdout)))
        } else {
            Ok(ToolResult::failure(String::from_utf8_lossy(&output.stderr)))
        }
    }
}
