use crate::error::XzardgzError;
use crate::providers::types::ToolCall;
use crate::tools::ToolResult;
use crate::tools::registry::ToolRegistry;
use serde_json::Value;
use std::sync::Arc;

pub struct ToolExecutionDispatcher {
    registry: Arc<ToolRegistry>,
}

impl ToolExecutionDispatcher {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }

    pub async fn execute(&self, tool_call: &ToolCall) -> Result<ToolResult, XzardgzError> {
        let function = &tool_call.function;
        let executor = self.registry.get_executor(&function.name).ok_or_else(|| {
            XzardgzError::Workflow(crate::error::WorkflowError::Execution(format!(
                "Tool not found: {}",
                function.name
            )))
        })?;

        // Parse arguments if they are a string (JSON), otherwise use as is?
        // ToolCall.function.arguments is String (JSON).
        let params: Value = serde_json::from_str(&function.arguments).map_err(|e| {
            XzardgzError::Workflow(crate::error::WorkflowError::Execution(format!(
                "Invalid tool arguments: {}",
                e
            )))
        })?;

        executor.execute(params).await
    }
}
