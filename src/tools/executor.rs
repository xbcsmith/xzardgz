use crate::error::{PipelineError, Result};
use crate::providers::types::ToolCall;
use crate::tools::ToolResult;
use crate::tools::registry::ToolRegistry;
use serde_json::Value;
use std::sync::Arc;

/// Dispatches tool calls to the appropriate registered executor.
pub struct ToolExecutionDispatcher {
    registry: Arc<ToolRegistry>,
}

impl ToolExecutionDispatcher {
    /// Creates a new `ToolExecutionDispatcher` backed by the given [`ToolRegistry`].
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }

    /// Looks up and executes the tool identified by the given [`ToolCall`].
    ///
    /// Returns `PipelineError::Tool` if the named tool is not found in the registry,
    /// or if the argument JSON cannot be parsed.
    pub async fn execute(&self, tool_call: &ToolCall) -> Result<ToolResult> {
        let function = &tool_call.function;
        let executor = self
            .registry
            .get_executor(&function.name)
            .ok_or_else(|| PipelineError::Tool(format!("tool not found: {}", function.name)))?;

        let params: Value = serde_json::from_str(&function.arguments)
            .map_err(|e| PipelineError::Tool(format!("invalid tool arguments: {}", e)))?;

        executor.execute(params).await
    }
}
