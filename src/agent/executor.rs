use crate::agent::context::ConversationContext;
use crate::error::{PipelineError, Result};
use crate::providers::base::Provider;
use crate::providers::types::{Message, Role};
use crate::tools::executor::ToolExecutionDispatcher;
use std::sync::{Arc, Mutex};

/// Manages the execution loop for agent interactions.
///
/// [`AgentExecutor`] owns the conversation context and drives the
/// provider-tool loop up to a configurable iteration cap (default: 5).
pub struct AgentExecutor {
    provider: Arc<dyn Provider>,
    context: Mutex<ConversationContext>,
    tool_dispatcher: ToolExecutionDispatcher,
    max_iterations: usize,
}

impl AgentExecutor {
    /// Creates a new [`AgentExecutor`] with the given provider, initial
    /// context, and tool dispatcher.
    ///
    /// The maximum iteration count defaults to 5 and can be changed by
    /// calling the builder-style setters added in future versions.
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

    /// Executes a user input through the provider-tool loop, handles any tool
    /// calls, and returns the final text response.
    ///
    /// Iteration is capped at `max_iterations` (default: 5).  Returns
    /// [`PipelineError::Agent`] if the cap is exceeded or the context mutex
    /// becomes poisoned.
    pub async fn execute(&self, input: &str) -> Result<String> {
        // Add user message
        {
            let mut context = self
                .context
                .lock()
                .map_err(|_| PipelineError::Agent("context lock poisoned".to_string()))?;
            context.add_message(Message::user(input));
        }

        // Execute loop
        for iteration in 0..self.max_iterations {
            tracing::debug!("Agent iteration {}/{}", iteration + 1, self.max_iterations);

            let (messages, tools) = {
                let context = self
                    .context
                    .lock()
                    .map_err(|_| PipelineError::Agent("context lock poisoned".to_string()))?;
                (context.get_messages().to_vec(), vec![])
            };

            let response = self.provider.complete(&messages, &tools).await?;

            {
                let mut context = self
                    .context
                    .lock()
                    .map_err(|_| PipelineError::Agent("context lock poisoned".to_string()))?;
                context.add_message(response.clone());
            }

            if let Some(tool_calls) = &response.tool_calls {
                if tool_calls.is_empty() {
                    return Ok(response.content);
                }

                for call in tool_calls {
                    let result = self.tool_dispatcher.execute(call).await?;

                    let tool_msg = Message {
                        role: Role::Tool,
                        content: result.output,
                        tool_calls: None,
                        tool_call_id: Some(call.id.clone()),
                        name: Some(call.function.name.clone()),
                    };

                    let mut context = self
                        .context
                        .lock()
                        .map_err(|_| PipelineError::Agent("context lock poisoned".to_string()))?;
                    context.add_message(tool_msg);
                }
            } else {
                return Ok(response.content);
            }
        }

        Err(PipelineError::Agent(
            "max agent iterations reached".to_string(),
        ))
    }
}
