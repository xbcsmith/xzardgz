use crate::agent::message::Message;
use crate::error::Result;
use crate::providers::types::ProviderMetadata;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Maintains the message history and token budget for a single agent
/// conversation.
///
/// Messages are stored in insertion order.  When the estimated token count
/// exceeds the configured budget, [`compact_if_needed`] trims the oldest
/// messages until the conversation fits within the limit.
///
/// [`compact_if_needed`]: ConversationContext::compact_if_needed
#[derive(Debug, Clone)]
pub struct ConversationContext {
    messages: Vec<Message>,
    #[allow(dead_code)]
    system_prompt: String,
    max_tokens: usize,
}

impl ConversationContext {
    /// Creates a new [`ConversationContext`] with the given system prompt and
    /// maximum token budget.
    pub fn new(system_prompt: String, max_tokens: usize) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt,
            max_tokens,
        }
    }

    /// Appends a message to the end of the conversation history.
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Returns a slice of all messages currently in the conversation history.
    pub fn get_messages(&self) -> &[Message] {
        &self.messages
    }

    /// Removes all messages from the conversation history.
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Returns a rough estimate of the number of tokens used by the current
    /// conversation history (approximately 4 characters per token).
    pub fn current_tokens(&self) -> usize {
        // Very rough estimation: 4 chars per token
        let content_len: usize = self.messages.iter().map(|m| m.content.len()).sum();
        content_len / 4
    }

    /// Removes the oldest messages from the history until the estimated token
    /// count is within `max_tokens`.
    ///
    /// Returns `true` if any messages were removed, `false` if the
    /// conversation was already within budget.
    pub fn compact_if_needed(&mut self) -> Result<bool> {
        if self.current_tokens() > self.max_tokens {
            while self.current_tokens() > self.max_tokens && !self.messages.is_empty() {
                self.messages.remove(0);
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

// ---------------------------------------------------------------------------
// AgentContext
// ---------------------------------------------------------------------------

/// Full context for an agent session, including message history,
/// workspace metadata, scan artifact references, plugin metadata,
/// provider metadata, and trace settings.
///
/// Unlike [`ConversationContext`], which covers only the message window,
/// `AgentContext` carries all session-scoped metadata needed by plugins,
/// tools, and the orchestration layer.
pub struct AgentContext {
    messages: Vec<Message>,
    #[allow(dead_code)]
    system_prompt: String,
    max_tokens: usize,
    /// Workspace identifier for this session.
    pub workspace_id: Option<String>,
    /// Root path of the workspace on disk.
    pub workspace_root: Option<PathBuf>,
    /// Path to the scan artifact YAML file.
    pub scan_artifact_path: Option<PathBuf>,
    /// Plugin-scoped metadata passed into the agent.
    pub plugin_metadata: HashMap<String, Value>,
    /// Provider metadata for capability checks.
    pub provider_metadata: Option<ProviderMetadata>,
    /// Whether trace logging is enabled for this session.
    pub trace_enabled: bool,
    /// Step identifier for transcript namespacing.
    pub step_id: Option<String>,
}

impl AgentContext {
    /// Creates a new [`AgentContext`] with the given system prompt and maximum
    /// token budget.
    ///
    /// All optional fields are `None`, maps are empty, and `trace_enabled` is
    /// `false`.
    ///
    /// # Arguments
    ///
    /// * `system_prompt` - The system instruction for this session.
    /// * `max_tokens` - Upper bound on the estimated token budget.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::agent::context::AgentContext;
    ///
    /// let ctx = AgentContext::new("You are a helpful assistant.".to_string(), 4096);
    /// assert!(ctx.get_messages().is_empty());
    /// ```
    pub fn new(system_prompt: String, max_tokens: usize) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt,
            max_tokens,
            workspace_id: None,
            workspace_root: None,
            scan_artifact_path: None,
            plugin_metadata: HashMap::new(),
            provider_metadata: None,
            trace_enabled: false,
            step_id: None,
        }
    }

    /// Sets the workspace identifier and root path for this session.
    ///
    /// # Arguments
    ///
    /// * `workspace_id` - Unique identifier for the workspace.
    /// * `workspace_root` - Filesystem root of the workspace.
    ///
    /// # Returns
    ///
    /// `self` with `workspace_id` and `workspace_root` populated.
    pub fn with_workspace(mut self, workspace_id: String, workspace_root: PathBuf) -> Self {
        self.workspace_id = Some(workspace_id);
        self.workspace_root = Some(workspace_root);
        self
    }

    /// Sets the path to the scan artifact YAML file for this session.
    ///
    /// # Arguments
    ///
    /// * `path` - Filesystem path to the scan artifact.
    ///
    /// # Returns
    ///
    /// `self` with `scan_artifact_path` populated.
    pub fn with_scan_artifact(mut self, path: PathBuf) -> Self {
        self.scan_artifact_path = Some(path);
        self
    }

    /// Sets the step identifier for transcript namespacing.
    ///
    /// # Arguments
    ///
    /// * `step_id` - Identifier for the current pipeline step.
    ///
    /// # Returns
    ///
    /// `self` with `step_id` populated.
    pub fn with_step_id(mut self, step_id: String) -> Self {
        self.step_id = Some(step_id);
        self
    }

    /// Sets the provider metadata for capability checks.
    ///
    /// # Arguments
    ///
    /// * `meta` - Provider metadata to attach to this context.
    ///
    /// # Returns
    ///
    /// `self` with `provider_metadata` populated.
    pub fn with_provider_metadata(mut self, meta: ProviderMetadata) -> Self {
        self.provider_metadata = Some(meta);
        self
    }

    /// Enables or disables trace logging for this session.
    ///
    /// # Arguments
    ///
    /// * `enabled` - `true` to enable trace logging.
    ///
    /// # Returns
    ///
    /// `self` with `trace_enabled` set accordingly.
    pub fn with_trace(mut self, enabled: bool) -> Self {
        self.trace_enabled = enabled;
        self
    }

    /// Appends a message to the end of the conversation history.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to append.
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Returns a slice of all messages currently in the conversation history.
    pub fn get_messages(&self) -> &[Message] {
        &self.messages
    }

    /// Removes all messages from the conversation history.
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Returns a rough estimate of the number of tokens used by the current
    /// conversation history (approximately 4 characters per token).
    pub fn current_tokens(&self) -> usize {
        self.messages.iter().map(|m| m.content.len()).sum::<usize>() / 4
    }

    /// Removes the oldest messages from the history until the estimated token
    /// count is within `max_tokens`.
    ///
    /// Returns `true` if any messages were removed, `false` if the
    /// conversation was already within budget.
    ///
    /// # Errors
    ///
    /// This implementation is infallible; the `Result` return type is
    /// reserved for future compaction strategies that may fail.
    pub fn compact_if_needed(&mut self) -> crate::error::Result<bool> {
        if self.current_tokens() > self.max_tokens {
            while self.current_tokens() > self.max_tokens && !self.messages.is_empty() {
                self.messages.remove(0);
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_context_new_has_empty_messages() {
        let ctx = AgentContext::new("system".to_string(), 4096);
        assert!(ctx.get_messages().is_empty());
        assert!(ctx.workspace_id.is_none());
        assert!(ctx.workspace_root.is_none());
        assert!(ctx.scan_artifact_path.is_none());
        assert!(ctx.plugin_metadata.is_empty());
        assert!(ctx.provider_metadata.is_none());
        assert!(!ctx.trace_enabled);
        assert!(ctx.step_id.is_none());
    }

    #[test]
    fn test_agent_context_with_workspace_sets_fields() {
        let root = PathBuf::from("/tmp/workspace");
        let ctx = AgentContext::new("system".to_string(), 4096)
            .with_workspace("ws-001".to_string(), root.clone());
        assert_eq!(ctx.workspace_id.as_deref(), Some("ws-001"));
        assert_eq!(ctx.workspace_root.as_ref(), Some(&root));
    }

    #[test]
    fn test_agent_context_with_scan_artifact_sets_path() {
        let artifact = PathBuf::from("/tmp/scan.yaml");
        let ctx =
            AgentContext::new("system".to_string(), 4096).with_scan_artifact(artifact.clone());
        assert_eq!(ctx.scan_artifact_path.as_ref(), Some(&artifact));
    }

    #[test]
    fn test_agent_context_add_message_appends() {
        let mut ctx = AgentContext::new("system".to_string(), 4096);
        assert_eq!(ctx.get_messages().len(), 0);
        ctx.add_message(Message::user("hello"));
        assert_eq!(ctx.get_messages().len(), 1);
        ctx.add_message(Message::assistant("world"));
        assert_eq!(ctx.get_messages().len(), 2);
    }

    #[test]
    fn test_agent_context_compact_if_needed_trims_messages() {
        // max_tokens = 15; two messages each with 40 chars = 80 total / 4 = 20 tokens > 15
        // After removing first (40 chars remaining / 4 = 10 tokens <= 15), one message stays.
        let mut ctx = AgentContext::new("system".to_string(), 15);
        ctx.add_message(Message::user("a".repeat(40)));
        ctx.add_message(Message::user("b".repeat(40)));
        assert_eq!(ctx.get_messages().len(), 2);
        let result = ctx.compact_if_needed();
        assert!(result.is_ok());
        assert!(result.unwrap(), "expected compact_if_needed to return true");
        assert_eq!(
            ctx.get_messages().len(),
            1,
            "expected one message to remain after compaction"
        );
    }

    #[test]
    fn test_agent_context_with_trace_sets_flag() {
        let ctx = AgentContext::new("system".to_string(), 4096).with_trace(true);
        assert!(ctx.trace_enabled);
        let ctx2 = AgentContext::new("system".to_string(), 4096).with_trace(false);
        assert!(!ctx2.trace_enabled);
    }
}
