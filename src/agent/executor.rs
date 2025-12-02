use crate::agent::context::ConversationContext;
use crate::error::XzardgzError;
use crate::providers::base::Provider;
use crate::providers::types::{Message, Role};
use crate::tools::executor::ToolExecutionDispatcher;
use std::sync::{Arc, Mutex};

/// Manages the execution loop for agent interactions
pub struct AgentExecutor {
    provider: Arc<dyn Provider>,
    context: Mutex<ConversationContext>,
    tool_dispatcher: ToolExecutionDispatcher,
    max_iterations: usize,
}

impl AgentExecutor {
    pub fn new(
        provider: Arc<dyn Provider>,
        context: ConversationContext,
        tool_dispatcher: ToolExecutionDispatcher,
    ) -> Self {
        Self {
            provider,
            context: Mutex::new(context),
            tool_dispatcher,
            max_iterations: 5,
        }
    }

    /// Execute a user input and return the final response
    pub async fn execute(&self, input: &str) -> Result<String, XzardgzError> {
        // Add user message
        {
            let mut context = self.context.lock().map_err(|_| {
                XzardgzError::Workflow(crate::error::WorkflowError::Execution(
                    "Context lock poisoned".to_string(),
                ))
            })?;
            context.add_message(Message::user(input));
        }

        // Execute loop
        for iteration in 0..self.max_iterations {
            tracing::debug!("Agent iteration {}/{}", iteration + 1, self.max_iterations);

            // Get conversation state and call provider
            let (messages, tools) = {
                let context = self.context.lock().map_err(|_| {
                    XzardgzError::Workflow(crate::error::WorkflowError::Execution(
                        "Context lock poisoned".to_string(),
                    ))
                })?;
                (context.get_messages().to_vec(), vec![])
            };

            let response = self.provider.complete(&messages, &tools).await?;

            // Add assistant response
            {
                let mut context = self.context.lock().map_err(|_| {
                    XzardgzError::Workflow(crate::error::WorkflowError::Execution(
                        "Context lock poisoned".to_string(),
                    ))
                })?;
                context.add_message(response.clone());
            }

            // Check for tool calls
            if let Some(tool_calls) = &response.tool_calls {
                if tool_calls.is_empty() {
                    return Ok(response.content);
                }

                // Execute tools
                for call in tool_calls {
                    let result = self.tool_dispatcher.execute(call).await?;

                    // Add tool result message
                    let tool_msg = Message {
                        role: Role::Tool,
                        content: result.output,
                        tool_calls: None,
                        tool_call_id: Some(call.id.clone()),
                        name: Some(call.function.name.clone()),
                    };

                    let mut context = self.context.lock().map_err(|_| {
                        XzardgzError::Workflow(crate::error::WorkflowError::Execution(
                            "Context lock poisoned".to_string(),
                        ))
                    })?;
                    context.add_message(tool_msg);
                }
                // Continue loop to send tool results back
            } else {
                // No tool calls, return final response
                return Ok(response.content);
            }
        }

        Err(XzardgzError::Workflow(
            crate::error::WorkflowError::Execution("Max agent iterations reached".to_string()),
        ))
    }
}
