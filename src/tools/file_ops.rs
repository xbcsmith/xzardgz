use crate::error::{PipelineError, Result};
use crate::providers::types::Tool;
use crate::tools::{ToolExecutor, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use serde_json::json;
use std::fs;
use std::path::Path;

/// Tool that reads the full contents of a file from the local filesystem.
pub struct ReadFileTool;

impl ReadFileTool {
    /// Returns the tool definition for `read_file`, including its JSON parameter schema.
    pub fn definition() -> Tool {
        Tool {
            name: "read_file".to_string(),
            description: "Read contents of a file".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file"
                    }
                },
                "required": ["path"]
            }),
        }
    }
}

#[async_trait]
impl ToolExecutor for ReadFileTool {
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing path parameter".to_string()))?;

        let path = Path::new(path_str);
        if !path.exists() {
            return Ok(ToolResult::failure(format!("File not found: {}", path_str)));
        }

        match fs::read_to_string(path) {
            Ok(content) => Ok(ToolResult::success(content)),
            Err(e) => Ok(ToolResult::failure(format!("Failed to read file: {}", e))),
        }
    }
}

/// Tool that writes content to a file on the local filesystem.
pub struct WriteFileTool;

impl WriteFileTool {
    /// Returns the tool definition for `write_file`, including its JSON parameter schema.
    pub fn definition() -> Tool {
        Tool {
            name: "write_file".to_string(),
            description: "Write content to a file".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }
}

#[async_trait]
impl ToolExecutor for WriteFileTool {
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing path parameter".to_string()))?;
        let content = params["content"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing content parameter".to_string()))?;

        match fs::write(path_str, content) {
            Ok(_) => Ok(ToolResult::success(format!(
                "Successfully wrote to {}",
                path_str
            ))),
            Err(e) => Ok(ToolResult::failure(format!("Failed to write file: {}", e))),
        }
    }
}
