//! Watcher mode and Kafka result publishing.
//!
//! This module implements the event-driven watcher loop for the XZardgz
//! pipeline.  The watcher consumes [`WatcherTaskMessage`] records from a Kafka
//! task topic, validates and dispatches them via [`WatcherExecutor`], and
//! publishes [`WatcherResultMessage`] records to the Kafka result topic via a
//! [`ResultPublisher`].
//!
//! # Architecture
//!
//! | Component | Responsibility |
//! |-----------|----------------|
//! | [`event_type`] | Event type enum and string constants |
//! | [`task`] | Inbound task message schema |
//! | [`result`] | Outbound result message schema |
//! | [`matcher`] | Task routing / allow-list filtering |
//! | [`publisher`] | Result publishing trait and Kafka implementation |
//! | [`executor`] | Core task processing (validation, plugin dispatch, result assembly) |
//!
//! # Typical flow
//!
//! 1. A CI system or external trigger publishes a [`WatcherTaskMessage`] to
//!    the Kafka task topic.
//! 2. The watcher consumer polls the topic and deserializes the message.
//! 3. A [`WatcherMatcher`] evaluates whether this instance should handle the
//!    task.
//! 4. If accepted, a [`WatcherExecutor`] validates the plugin, runs it (stub
//!    in Phase 14; full execution in Phase 17), and builds a
//!    [`WatcherResultMessage`].
//! 5. The [`KafkaResultPublisher`] (or a mock during tests) serializes the
//!    result and sends it to the Kafka result topic.
//! 6. If publishing fails, [`PublishFailureState`] can be persisted to disk
//!    for retry without re-running the plugin.

pub mod event_type;
pub mod executor;
pub mod matcher;
pub mod publisher;
pub mod result;
pub mod task;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use event_type::{
    EVENT_SECURITY_REVIEW_RESULT, EVENT_SECURITY_REVIEW_TASK, EVENT_TECHNICAL_REVIEW_RESULT,
    EVENT_TECHNICAL_REVIEW_TASK, WatcherEventType,
};
pub use executor::WatcherExecutor;
pub use matcher::WatcherMatcher;
pub use publisher::{KafkaResultPublisher, PublishFailureState, ResultPublisher};
pub use result::{FindingsSummary, WATCHER_RESULT_VERSION, WatcherResultMessage};
pub use task::{WATCHER_TASK_VERSION, WatcherTaskMessage};
