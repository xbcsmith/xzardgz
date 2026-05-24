use crate::agent::message::Message;
use crate::error::Result;

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
