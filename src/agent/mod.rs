//! Agent session orchestration for the xzardgz pipeline.
//!
//! This module contains exactly one agent-loop implementation:
//! [`session::AgentSession`]. There is intentionally only one. Do not
//! introduce additional agent-loop types here; extend `AgentSession` instead.
//!
//! # Module layout
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`session`] | `AgentSession` — the sole bounded, tool-augmented loop |
//! | [`context`] | `AgentContext` (session metadata) and `ConversationContext` (message window) |
//! | [`message`] | Re-exports of provider message types for agent consumers |
//! | [`state`] | `AgentState` — lightweight iteration-tracking helper |

pub mod context;
pub mod message;
pub mod session;
pub mod state;
