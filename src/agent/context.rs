use crate::agent::message::Message;
use crate::error::XzardgzError;

#[derive(Debug, Clone)]
pub struct ConversationContext {
    messages: Vec<Message>,
    #[allow(dead_code)]
    system_prompt: String,
    max_tokens: usize,
}

impl ConversationContext {
    pub fn new(system_prompt: String, max_tokens: usize) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt,
            max_tokens,
        }
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn get_messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    // Placeholder for token counting
    pub fn current_tokens(&self) -> usize {
        // Very rough estimation: 4 chars per token
        let content_len: usize = self.messages.iter().map(|m| m.content.len()).sum();
        content_len / 4
    }

    pub fn compact_if_needed(&mut self) -> Result<bool, XzardgzError> {
        if self.current_tokens() > self.max_tokens {
            // Simple strategy: Remove oldest non-system messages
            // Keep system prompt (it's separate)
            // Keep last N messages?

            // For now, just remove the second message (after system if we treated system as message 0,
            // but here system_prompt is separate).
            // So remove messages[0] until we fit.

            while self.current_tokens() > self.max_tokens && !self.messages.is_empty() {
                self.messages.remove(0);
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
