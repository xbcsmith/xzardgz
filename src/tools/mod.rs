use crate::error::XzardgzError;
use async_trait::async_trait;
use serde_json::Value;

pub mod executor;
pub mod file_ops;
pub mod git_ops;
pub mod registry;

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub output: String,
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            error: None,
        }
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            output: String::new(),
            error: Some(error.into()),
        }
    }
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, params: Value) -> Result<ToolResult, XzardgzError>;
}
