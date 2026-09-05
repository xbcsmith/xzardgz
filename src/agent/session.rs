//! Agent session orchestrating the provider-tool loop with sandbox enforcement.
//!
//! An [`AgentSession`] ties together a provider, an [`AgentContext`], and a
//! [`ToolRegistry`] to drive a bounded, tool-augmented conversation.  Each
//! call to [`AgentSession::run`] enters a loop that processes provider
//! responses, dispatches tool calls, and returns the first plain-text reply
//! or an error when the turn budget is exhausted.

use crate::agent::context::AgentContext;
use crate::error::{PipelineError, Result};
use crate::providers::base::Provider;
use crate::providers::types::{Message, Role};
use crate::tools::executor::ToolExecutionDispatcher;
use crate::tools::registry::ToolRegistry;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// AgentSession
// ---------------------------------------------------------------------------

/// Orchestrates a bounded, tool-augmented agent session.
///
/// An `AgentSession` drives a multi-turn provider-tool loop, persisting an
/// optional JSONL transcript and recording per-tool failure counts so that
/// callers can detect runaway tool use.
///
/// # Construction
///
/// Use [`AgentSession::new`] followed by the builder-style setters:
///
/// ```ignore
/// let session = AgentSession::new(provider, context, registry)
///     .with_max_turns(15)
///     .with_transcript(PathBuf::from("session.jsonl"))
///     .with_repeated_failure_threshold(5);
/// ```
pub struct AgentSession {
    provider: Arc<dyn Provider + Send + Sync>,
    context: Mutex<AgentContext>,
    tool_registry: Arc<ToolRegistry>,
    tool_dispatcher: ToolExecutionDispatcher,
    max_turns: usize,
    transcript_enabled: bool,
    transcript_path: Option<PathBuf>,
    failure_counts: Mutex<HashMap<String, usize>>,
    repeated_failure_threshold: usize,
}

impl AgentSession {
    /// Creates a new [`AgentSession`] with the given provider, context, and
    /// tool registry.
    ///
    /// Defaults: `max_turns = 10`, transcript disabled,
    /// `repeated_failure_threshold = 3`.
    ///
    /// # Arguments
    ///
    /// * `provider` - The AI provider backend to use for completions.
    /// * `context` - The session context, including system prompt and token budget.
    /// * `tool_registry` - Registry of tools the provider may invoke.
    pub fn new(
        provider: Arc<dyn Provider + Send + Sync>,
        context: AgentContext,
        tool_registry: ToolRegistry,
    ) -> Self {
        let registry_arc = Arc::new(tool_registry);
        let dispatcher = ToolExecutionDispatcher::new(registry_arc.clone());
        Self {
            provider,
            context: Mutex::new(context),
            tool_registry: registry_arc,
            tool_dispatcher: dispatcher,
            max_turns: 10,
            transcript_enabled: false,
            transcript_path: None,
            failure_counts: Mutex::new(HashMap::new()),
            repeated_failure_threshold: 3,
        }
    }

    /// Sets the maximum number of provider-tool loop turns before the session
    /// returns an error.
    ///
    /// # Arguments
    ///
    /// * `max_turns` - Maximum turns allowed in a single [`run`][Self::run] call.
    pub fn with_max_turns(mut self, max_turns: usize) -> Self {
        self.max_turns = max_turns;
        self
    }

    /// Enables JSONL transcript writing to the given path.
    ///
    /// Each message (user, assistant, and tool) will be appended as a single
    /// JSON line.  The file is created if it does not exist.
    ///
    /// # Arguments
    ///
    /// * `path` - Filesystem path to the transcript file.
    pub fn with_transcript(mut self, path: PathBuf) -> Self {
        self.transcript_enabled = true;
        self.transcript_path = Some(path);
        self
    }

    /// Sets the per-tool repeated-failure threshold.
    ///
    /// When a single tool accumulates this many failures within a session, a
    /// `tracing::warn!` is emitted.  The session continues regardless.
    ///
    /// # Arguments
    ///
    /// * `n` - Number of failures before a warning is emitted.
    pub fn with_repeated_failure_threshold(mut self, n: usize) -> Self {
        self.repeated_failure_threshold = n;
        self
    }

    /// Runs the agent with the provided user input.
    ///
    /// If the provider does not support tool calling, delegates immediately to
    /// [`run_single_turn`][Self::run_single_turn].  Otherwise enters a bounded
    /// multi-turn loop that processes tool calls until the provider returns a
    /// plain-text reply or `max_turns` is exhausted.
    ///
    /// # Arguments
    ///
    /// * `input` - The user message to send to the provider.
    ///
    /// # Returns
    ///
    /// The assistant's final plain-text response content.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Agent`] if `max_turns` is reached without a
    /// terminal response or if the context mutex becomes poisoned.
    /// Returns [`PipelineError::Provider`] on provider API failures.
    pub async fn run(&self, input: &str) -> Result<String> {
        // Delegate to single-turn path when provider does not support tools.
        if !self.provider.metadata().capabilities.tools {
            return self.run_single_turn(input).await;
        }

        // Add user message to context and persist to transcript.
        let user_msg = Message::user(input);
        {
            // SAFETY: context lock is only held for the duration of the push;
            // poisoning cannot occur in normal operation.
            let mut ctx = self
                .context
                .lock()
                .map_err(|_| PipelineError::Agent("context lock poisoned".to_string()))?;
            ctx.add_message(user_msg.clone());
        }
        self.persist_transcript(&user_msg)?;

        // Main provider-tool loop.
        for _turn in 0..self.max_turns {
            // Collect messages and available tools while holding the lock briefly.
            let (messages, tools) = {
                // SAFETY: lock is released at end of this block.
                let ctx = self
                    .context
                    .lock()
                    .map_err(|_| PipelineError::Agent("context lock poisoned".to_string()))?;
                (ctx.get_messages().to_vec(), self.tool_registry.list_tools())
            };

            let response = self.provider.complete(&messages, &tools).await?;

            // Store assistant response in context.
            {
                // SAFETY: lock is released at end of this block.
                let mut ctx = self
                    .context
                    .lock()
                    .map_err(|_| PipelineError::Agent("context lock poisoned".to_string()))?;
                ctx.add_message(response.clone());
            }
            self.persist_transcript(&response)?;

            // Dispatch tool calls if present; otherwise return the content.
            match &response.tool_calls {
                Some(calls) if !calls.is_empty() => {
                    for call in calls {
                        let content = match self.tool_dispatcher.execute(call).await {
                            Ok(result) => {
                                if let Some(ref err) = result.error {
                                    self.record_tool_failure(&call.function.name);
                                    format!("Tool error: {}", err)
                                } else {
                                    result.output.clone()
                                }
                            }
                            Err(e) => {
                                self.record_tool_failure(&call.function.name);
                                format!("Tool execution error: {}", e)
                            }
                        };

                        let tool_msg = Message {
                            role: Role::Tool,
                            content,
                            tool_calls: None,
                            tool_call_id: Some(call.id.clone()),
                            name: Some(call.function.name.clone()),
                        };

                        {
                            // SAFETY: lock is released at end of this block.
                            let mut ctx = self.context.lock().map_err(|_| {
                                PipelineError::Agent("context lock poisoned".to_string())
                            })?;
                            ctx.add_message(tool_msg.clone());
                        }
                        self.persist_transcript(&tool_msg)?;
                    }
                    // Continue to next turn to let the provider react to tool results.
                }
                _ => return Ok(response.content),
            }
        }

        Err(PipelineError::Agent("max turns reached".to_string()))
    }

    /// Runs a single provider completion without tool support.
    ///
    /// Used as a fallback when the provider does not advertise tool calling
    /// capability.
    ///
    /// # Arguments
    ///
    /// * `input` - The user message to send to the provider.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Provider`] on provider API failures or
    /// [`PipelineError::Agent`] if the context mutex is poisoned.
    async fn run_single_turn(&self, input: &str) -> Result<String> {
        // Add user message.
        {
            // SAFETY: lock is released at end of this block.
            let mut ctx = self
                .context
                .lock()
                .map_err(|_| PipelineError::Agent("context lock poisoned".to_string()))?;
            ctx.add_message(Message::user(input));
        }

        // Snapshot messages for the completion call.
        let messages = {
            // SAFETY: lock is released at end of this block.
            let ctx = self
                .context
                .lock()
                .map_err(|_| PipelineError::Agent("context lock poisoned".to_string()))?;
            ctx.get_messages().to_vec()
        };

        let response = self.provider.complete(&messages, &[]).await?;

        // Store response in context.
        {
            // SAFETY: lock is released at end of this block.
            let mut ctx = self
                .context
                .lock()
                .map_err(|_| PipelineError::Agent("context lock poisoned".to_string()))?;
            ctx.add_message(response.clone());
        }
        self.persist_transcript(&response)?;

        Ok(response.content)
    }

    /// Appends a single JSON line for `message` to the transcript file.
    ///
    /// Returns `Ok(())` immediately when the transcript is disabled or no
    /// path has been configured.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Agent`] if JSON serialization fails or if the
    /// transcript file cannot be opened or written.
    fn persist_transcript(&self, message: &Message) -> Result<()> {
        if !self.transcript_enabled {
            return Ok(());
        }
        let Some(ref path) = self.transcript_path else {
            return Ok(());
        };

        let line = serde_json::to_string(message)
            .map_err(|e| PipelineError::Agent(format!("transcript serialization failed: {}", e)))?;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| PipelineError::Agent(format!("transcript file error: {}", e)))?;

        writeln!(file, "{}", line)
            .map_err(|e| PipelineError::Agent(format!("transcript write error: {}", e)))?;

        Ok(())
    }

    /// Increments the failure count for `tool_name` and emits a warning when
    /// the repeated-failure threshold is reached.
    ///
    /// Returns the updated failure count for the named tool.
    ///
    /// # Arguments
    ///
    /// * `tool_name` - The name of the tool that failed.
    fn record_tool_failure(&self, tool_name: &str) -> usize {
        // SAFETY: failure_counts lock is only held briefly; poisoning cannot
        // occur in normal operation.
        let mut counts = self
            .failure_counts
            .lock()
            .expect("failure_counts lock poisoned");
        let count = counts.entry(tool_name.to_string()).or_insert(0);
        *count += 1;
        let result = *count;
        if result >= self.repeated_failure_threshold {
            tracing::warn!(
                tool = tool_name,
                count = result,
                "tool has failed repeatedly in this agent session"
            );
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::base::MockProvider;
    use crate::providers::types::{FunctionCall, ProviderCapabilities, ProviderMetadata, ToolCall};
    use crate::tools::registry::ToolRegistry;
    use tempfile::TempDir;

    /// Builds a [`MockProvider`] that advertises tool support and returns a
    /// simple assistant message with no tool calls on every completion request.
    fn make_simple_provider() -> MockProvider {
        let mut mock = MockProvider::new();
        mock.expect_metadata().returning(|| ProviderMetadata {
            name: "mock".to_string(),
            models: vec![],
            capabilities: ProviderCapabilities {
                streaming: false,
                tools: true,
                vision: false,
            },
        });
        mock.expect_complete()
            .returning(|_, _| Ok(Message::assistant("hello")));
        mock
    }

    #[tokio::test]
    async fn test_run_returns_text_response_when_no_tool_calls() {
        let provider = Arc::new(make_simple_provider());
        let context = AgentContext::new("system".to_string(), 4096);
        let registry = ToolRegistry::new();
        let session = AgentSession::new(provider, context, registry);
        let result = session.run("hello").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_run_uses_single_turn_fallback_when_provider_has_no_tool_support() {
        let mut mock = MockProvider::new();
        mock.expect_metadata().returning(|| ProviderMetadata {
            name: "mock".to_string(),
            models: vec![],
            capabilities: ProviderCapabilities {
                streaming: false,
                tools: false,
                vision: false,
            },
        });
        mock.expect_complete()
            .times(1)
            .returning(|_, _| Ok(Message::assistant("single turn response")));
        let provider = Arc::new(mock);
        let context = AgentContext::new("system".to_string(), 4096);
        let session = AgentSession::new(provider, context, ToolRegistry::new());
        let result = session.run("input").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "single turn response");
    }

    #[tokio::test]
    async fn test_run_reaches_max_turns_and_returns_error() {
        let mut mock = MockProvider::new();
        mock.expect_metadata().returning(|| ProviderMetadata {
            name: "mock".to_string(),
            models: vec![],
            capabilities: ProviderCapabilities {
                streaming: false,
                tools: true,
                vision: false,
            },
        });
        // Always returns a tool call for an unknown tool, causing failure each turn.
        mock.expect_complete().returning(|_, _| {
            let mut msg = Message::assistant("");
            msg.tool_calls = Some(vec![ToolCall {
                id: "tc1".to_string(),
                function: FunctionCall {
                    name: "no_such_tool".to_string(),
                    arguments: "{}".to_string(),
                },
            }]);
            Ok(msg)
        });
        let provider = Arc::new(mock);
        let context = AgentContext::new("system".to_string(), 4096);
        let session = AgentSession::new(provider, context, ToolRegistry::new()).with_max_turns(2);
        let result = session.run("loop forever").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("max turns"));
    }

    #[tokio::test]
    async fn test_run_tool_error_continues_session_without_abort() {
        let mut mock = MockProvider::new();
        mock.expect_metadata().returning(|| ProviderMetadata {
            name: "mock".to_string(),
            models: vec![],
            capabilities: ProviderCapabilities {
                streaming: false,
                tools: true,
                vision: false,
            },
        });
        // Always returns a failing tool call; session must not panic.
        mock.expect_complete().returning(|_, _| {
            let mut msg = Message::assistant("");
            msg.tool_calls = Some(vec![ToolCall {
                id: "tc1".to_string(),
                function: FunctionCall {
                    name: "no_such_tool".to_string(),
                    arguments: "{}".to_string(),
                },
            }]);
            Ok(msg)
        });
        let provider = Arc::new(mock);
        let context = AgentContext::new("system".to_string(), 4096);
        let session = AgentSession::new(provider, context, ToolRegistry::new()).with_max_turns(3);
        let result = session.run("loop forever").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("max turns"));
    }

    #[tokio::test]
    async fn test_with_transcript_writes_jsonl_file() {
        let tmp = TempDir::new().unwrap(); // SAFETY: TempDir::new only fails on IO errors in tests.
        let transcript_path = tmp.path().join("transcript.jsonl");
        let provider = Arc::new(make_simple_provider());
        let context = AgentContext::new("system".to_string(), 4096);
        let session = AgentSession::new(provider, context, ToolRegistry::new())
            .with_transcript(transcript_path.clone());
        let _ = session.run("write to transcript").await;
        assert!(transcript_path.exists(), "transcript file must be created");
        let content =
            std::fs::read_to_string(&transcript_path).expect("transcript file must be readable");
        // Must have at least two JSON lines: user message and assistant response.
        let lines: Vec<&str> = content.lines().collect();
        assert!(lines.len() >= 2, "transcript must have at least 2 lines");
        // Every line must be valid JSON.
        for line in &lines {
            assert!(
                serde_json::from_str::<serde_json::Value>(line).is_ok(),
                "each transcript line must be valid JSON: {}",
                line
            );
        }
    }

    #[test]
    fn test_record_tool_failure_increments_count() {
        let provider = Arc::new(make_simple_provider());
        let context = AgentContext::new("system".to_string(), 4096);
        let session = AgentSession::new(provider, context, ToolRegistry::new())
            .with_repeated_failure_threshold(3);
        assert_eq!(session.record_tool_failure("my_tool"), 1);
        assert_eq!(session.record_tool_failure("my_tool"), 2);
        assert_eq!(session.record_tool_failure("my_tool"), 3);
        assert_eq!(session.record_tool_failure("other_tool"), 1);
    }
}
