use crate::error::Result;
use async_trait::async_trait;
use serde_json::Value;

pub mod executor;
pub mod file_ops;
pub mod git_ops;
pub mod registry;

/// Result type for tool executions, containing output and an optional error message.
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// The standard output produced by the tool execution.
    pub output: String,
    /// An optional error message when the tool encounters a non-fatal error.
    pub error: Option<String>,
}

impl ToolResult {
    /// Creates a successful `ToolResult` with the given output string.
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            error: None,
        }
    }

    /// Creates a failed `ToolResult` with an empty output and the given error message.
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            output: String::new(),
            error: Some(error.into()),
        }
    }
}

/// Trait for types that can execute a named tool with JSON parameters.
///
/// Implementations must be `Send + Sync` to support concurrent execution.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Executes the tool with the provided JSON `params`, returning a [`ToolResult`].
    async fn execute(&self, params: Value) -> Result<ToolResult>;
}
