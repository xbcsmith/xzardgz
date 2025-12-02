use crate::agent::context::ConversationContext;
use crate::error::XzardgzError;
use crate::providers::base::Provider;
use crate::providers::types::{Message, Role};
use crate::tools::executor::ToolExecutionDispatcher;
use crate::tools::registry::ToolRegistry;
use std::sync::Arc;

#[allow(dead_code)]
use std::sync::Mutex;

#[allow(dead_code)]
pub struct Agent {
    provider: Arc<dyn Provider>,
    context: Mutex<ConversationContext>,
    tool_registry: Arc<ToolRegistry>,
    tool_dispatcher: ToolExecutionDispatcher,
}

impl Agent {
    pub fn new(
        provider: Arc<dyn Provider>,
        system_prompt: String,
        tool_registry: ToolRegistry,
    ) -> Self {
        let registry_arc = Arc::new(tool_registry);
        let dispatcher = ToolExecutionDispatcher::new(registry_arc.clone());

        Self {
            provider,
            context: Mutex::new(ConversationContext::new(system_prompt, 4096)), // Default context window
            tool_registry: registry_arc,
            tool_dispatcher: dispatcher,
        }
    }

    pub async fn run(&self, input: &str) -> Result<String, XzardgzError> {
        // 1. Add user message
        {
            let mut context = self.context.lock().map_err(|_| {
                XzardgzError::Workflow(crate::error::WorkflowError::Execution(
                    "Context lock poisoned".to_string(),
                ))
            })?;
            context.add_message(Message::user(input));
        }

        // 2. Loop for tool execution
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 5;

        loop {
            if iterations >= MAX_ITERATIONS {
                return Err(XzardgzError::Workflow(
                    crate::error::WorkflowError::Execution(
                        "Max agent iterations reached".to_string(),
                    ),
                ));
            }
            iterations += 1;

            // 3. Call provider
            let (messages, tools) = {
                let context = self.context.lock().map_err(|_| {
                    XzardgzError::Workflow(crate::error::WorkflowError::Execution(
                        "Context lock poisoned".to_string(),
                    ))
                })?;
                (
                    context.get_messages().to_vec(),
                    self.tool_registry.list_tools(),
                )
            };

            let response = self.provider.complete(&messages, &tools).await?;

            // 4. Process response
            {
                let mut context = self.context.lock().map_err(|_| {
                    XzardgzError::Workflow(crate::error::WorkflowError::Execution(
                        "Context lock poisoned".to_string(),
                    ))
                })?;
                context.add_message(response.clone());
            }

            if let Some(tool_calls) = &response.tool_calls {
                if tool_calls.is_empty() {
                    return Ok(response.content);
                }

                for call in tool_calls {
                    let result = self.tool_dispatcher.execute(call).await?;

                    // Add tool result message
                    let tool_msg = Message {
                        role: Role::Tool,
                        content: result.output, // Or error?
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
                // Loop continues to send tool results back to provider
            } else {
                // No tool calls, return content
                return Ok(response.content);
            }
        }
    }
}
