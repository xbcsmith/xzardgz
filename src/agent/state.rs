use crate::agent::context::ConversationContext;

/// Agent state management
#[derive(Debug, Clone)]
pub struct AgentState {
    pub conversation: ConversationContext,
    pub iterations: usize,
    pub max_iterations: usize,
}

impl AgentState {
    pub fn new(conversation: ConversationContext, max_iterations: usize) -> Self {
        Self {
            conversation,
            iterations: 0,
            max_iterations,
        }
    }

    pub fn increment_iteration(&mut self) {
        self.iterations += 1;
    }

    pub fn is_at_max_iterations(&self) -> bool {
        self.iterations >= self.max_iterations
    }

    pub fn reset_iterations(&mut self) {
        self.iterations = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_state() {
        let ctx = ConversationContext::new("test".to_string(), 100);
        let mut state = AgentState::new(ctx, 5);

        assert_eq!(state.iterations, 0);
        assert!(!state.is_at_max_iterations());

        for _ in 0..5 {
            state.increment_iteration();
        }

        assert!(state.is_at_max_iterations());
        assert_eq!(state.iterations, 5);

        state.reset_iterations();
        assert_eq!(state.iterations, 0);
    }
}
