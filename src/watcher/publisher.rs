//! Watcher result publisher trait and Kafka implementation.
//!
//! This module defines the [`ResultPublisher`] trait, the
//! [`KafkaResultPublisher`] that sends result messages to the Kafka result
//! topic, and [`PublishFailureState`], a persisted record of a result message
//! that failed to publish so it can be retried later.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use rdkafka::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use serde::{Deserialize, Serialize};

use crate::config::{KafkaConfig, TopicsConfig};
use crate::error::{PipelineError, Result};
use crate::watcher::result::WatcherResultMessage;

// ---------------------------------------------------------------------------
// ResultPublisher
// ---------------------------------------------------------------------------

/// Trait for publishing watcher result messages to a message bus.
///
/// Implemented by [`KafkaResultPublisher`] for production and
/// [`MockResultPublisher`][crate::watcher::publisher] in tests.
///
/// # Examples
///
/// ```no_run
/// use async_trait::async_trait;
/// use xzardgz::watcher::publisher::ResultPublisher;
/// use xzardgz::watcher::result::WatcherResultMessage;
/// use xzardgz::error::Result;
///
/// struct NoopPublisher;
///
/// #[async_trait]
/// impl ResultPublisher for NoopPublisher {
///     async fn publish(&self, _result: &WatcherResultMessage) -> Result<()> {
///         Ok(())
///     }
/// }
/// ```
#[async_trait]
pub trait ResultPublisher: Send + Sync {
    /// Publishes a result message to the configured topic.
    ///
    /// # Arguments
    ///
    /// * `result` - The result message to publish.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Kafka`] on publish failure.
    async fn publish(&self, result: &WatcherResultMessage) -> Result<()>;
}

// ---------------------------------------------------------------------------
// PublishFailureState
// ---------------------------------------------------------------------------

/// Persisted state for a result message that failed to publish.
///
/// When publishing fails after a successful plugin run, the failure is stored
/// here so the result can be retried without rerunning the expensive plugin.
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use xzardgz::watcher::event_type::WatcherEventType;
/// use xzardgz::watcher::publisher::PublishFailureState;
/// use xzardgz::watcher::result::WatcherResultMessage;
///
/// let result = WatcherResultMessage::new(
///     "res-001",
///     WatcherEventType::TechnicalReviewResult,
///     "xzardgz-watcher",
///     "github.com/org/repo",
///     "technical_review",
///     "ws-001",
///     "corr-001",
///     "task-001",
///     Utc::now(),
/// );
///
/// let state = PublishFailureState::new("task-001", result);
/// assert_eq!(state.task_id, "task-001");
/// assert_eq!(state.attempts, 0);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishFailureState {
    /// ID of the original task.
    pub task_id: String,

    /// The result message that failed to publish.
    pub result: WatcherResultMessage,

    /// Number of publish attempts made so far.
    pub attempts: u32,

    /// Last error message encountered.
    pub last_error: Option<String>,

    /// Timestamp of the last attempt.
    pub last_attempt_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl PublishFailureState {
    /// Creates a new [`PublishFailureState`] for the given task and result.
    ///
    /// Initializes `attempts` to `0`, `last_error` to `None`, and
    /// `last_attempt_at` to `None`.
    ///
    /// # Arguments
    ///
    /// * `task_id` - ID of the original task that produced this result.
    /// * `result` - The result message that failed to publish.
    ///
    /// # Returns
    ///
    /// A new `PublishFailureState` with zero attempts recorded.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use xzardgz::watcher::event_type::WatcherEventType;
    /// use xzardgz::watcher::publisher::PublishFailureState;
    /// use xzardgz::watcher::result::WatcherResultMessage;
    ///
    /// let result = WatcherResultMessage::new(
    ///     "res-001",
    ///     WatcherEventType::TechnicalReviewResult,
    ///     "xzardgz-watcher",
    ///     "github.com/org/repo",
    ///     "technical_review",
    ///     "ws-001",
    ///     "corr-001",
    ///     "task-001",
    ///     Utc::now(),
    /// );
    ///
    /// let state = PublishFailureState::new("task-001", result);
    /// assert_eq!(state.attempts, 0);
    /// assert!(state.last_error.is_none());
    /// ```
    pub fn new(task_id: impl Into<String>, result: WatcherResultMessage) -> Self {
        Self {
            task_id: task_id.into(),
            result,
            attempts: 0,
            last_error: None,
            last_attempt_at: None,
        }
    }

    /// Records a failed publish attempt.
    ///
    /// Increments `attempts`, sets `last_error` to `error`, and sets
    /// `last_attempt_at` to the current UTC time.
    ///
    /// # Arguments
    ///
    /// * `error` - A human-readable description of the failure.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use xzardgz::watcher::event_type::WatcherEventType;
    /// use xzardgz::watcher::publisher::PublishFailureState;
    /// use xzardgz::watcher::result::WatcherResultMessage;
    ///
    /// let result = WatcherResultMessage::new(
    ///     "res-001",
    ///     WatcherEventType::TechnicalReviewResult,
    ///     "xzardgz-watcher",
    ///     "github.com/org/repo",
    ///     "technical_review",
    ///     "ws-001",
    ///     "corr-001",
    ///     "task-001",
    ///     Utc::now(),
    /// );
    ///
    /// let mut state = PublishFailureState::new("task-001", result);
    /// state.record_failure("broker unreachable");
    /// assert_eq!(state.attempts, 1);
    /// assert_eq!(state.last_error.as_deref(), Some("broker unreachable"));
    /// assert!(state.last_attempt_at.is_some());
    /// ```
    pub fn record_failure(&mut self, error: impl Into<String>) {
        self.attempts += 1;
        self.last_error = Some(error.into());
        self.last_attempt_at = Some(chrono::Utc::now());
    }

    /// Serializes this state to a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Kafka`] if serialization fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use xzardgz::watcher::event_type::WatcherEventType;
    /// use xzardgz::watcher::publisher::PublishFailureState;
    /// use xzardgz::watcher::result::WatcherResultMessage;
    ///
    /// let result = WatcherResultMessage::new(
    ///     "res-001",
    ///     WatcherEventType::TechnicalReviewResult,
    ///     "xzardgz-watcher",
    ///     "github.com/org/repo",
    ///     "technical_review",
    ///     "ws-001",
    ///     "corr-001",
    ///     "task-001",
    ///     Utc::now(),
    /// );
    /// let state = PublishFailureState::new("task-001", result);
    /// let json = state.to_json().unwrap();
    /// assert!(json.contains("task-001"));
    /// ```
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| {
            PipelineError::Kafka(format!("failed to serialize publish failure state: {e}"))
        })
    }

    /// Deserializes a [`PublishFailureState`] from a JSON string.
    ///
    /// # Arguments
    ///
    /// * `json` - A JSON string representation of the state.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Kafka`] if `json` is invalid or does not match
    /// the expected schema.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use xzardgz::watcher::event_type::WatcherEventType;
    /// use xzardgz::watcher::publisher::PublishFailureState;
    /// use xzardgz::watcher::result::WatcherResultMessage;
    ///
    /// let result = WatcherResultMessage::new(
    ///     "res-001",
    ///     WatcherEventType::TechnicalReviewResult,
    ///     "xzardgz-watcher",
    ///     "github.com/org/repo",
    ///     "technical_review",
    ///     "ws-001",
    ///     "corr-001",
    ///     "task-001",
    ///     Utc::now(),
    /// );
    /// let state = PublishFailureState::new("task-001", result);
    /// let json = state.to_json().unwrap();
    /// let restored = PublishFailureState::from_json(&json).unwrap();
    /// assert_eq!(restored.task_id, "task-001");
    /// ```
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| {
            PipelineError::Kafka(format!("failed to deserialize publish failure state: {e}"))
        })
    }

    /// Persists this state to a JSON file at `path`.
    ///
    /// Parent directories are created automatically.
    ///
    /// # Arguments
    ///
    /// * `path` - Destination file path for the persisted state.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Kafka`] if directory creation, serialization,
    /// or file writing fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use chrono::Utc;
    /// use xzardgz::watcher::event_type::WatcherEventType;
    /// use xzardgz::watcher::publisher::PublishFailureState;
    /// use xzardgz::watcher::result::WatcherResultMessage;
    ///
    /// let result = WatcherResultMessage::new(
    ///     "res-001",
    ///     WatcherEventType::TechnicalReviewResult,
    ///     "xzardgz-watcher",
    ///     "github.com/org/repo",
    ///     "technical_review",
    ///     "ws-001",
    ///     "corr-001",
    ///     "task-001",
    ///     Utc::now(),
    /// );
    /// let state = PublishFailureState::new("task-001", result);
    /// state.persist(Path::new("/tmp/failures/task-001.json")).unwrap();
    /// ```
    pub fn persist(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                PipelineError::Kafka(format!(
                    "failed to create parent directories for failure state: {e}"
                ))
            })?;
        }
        let json = self.to_json()?;
        std::fs::write(path, json).map_err(|e| {
            PipelineError::Kafka(format!(
                "failed to write publish failure state to {}: {e}",
                path.display()
            ))
        })
    }

    /// Loads a [`PublishFailureState`] from `path`, returning `None` when the
    /// file does not exist.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to read.
    ///
    /// # Returns
    ///
    /// `Ok(Some(state))` when the file exists and parses successfully.
    /// `Ok(None)` when the file does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Kafka`] if reading or parsing fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use xzardgz::watcher::publisher::PublishFailureState;
    ///
    /// let result = PublishFailureState::load(Path::new("/nonexistent/path.json")).unwrap();
    /// assert!(result.is_none());
    /// ```
    pub fn load(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let json = std::fs::read_to_string(path).map_err(|e| {
            PipelineError::Kafka(format!(
                "failed to read publish failure state from {}: {e}",
                path.display()
            ))
        })?;
        let state = Self::from_json(&json)?;
        Ok(Some(state))
    }
}

// ---------------------------------------------------------------------------
// KafkaResultPublisher
// ---------------------------------------------------------------------------

/// Kafka-backed result publisher.
///
/// Uses `rdkafka` [`FutureProducer`] to publish [`WatcherResultMessage`] JSON
/// payloads to the configured result topic.  The correlation ID is used as the
/// Kafka message key to enable per-correlation compaction or ordered delivery.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::config::{KafkaConfig, TopicsConfig};
/// use xzardgz::watcher::publisher::KafkaResultPublisher;
///
/// let kafka = KafkaConfig::default();
/// let topics = TopicsConfig::default();
/// let publisher = KafkaResultPublisher::new(&kafka, &topics).unwrap();
/// ```
pub struct KafkaResultPublisher {
    topic: String,
    producer: FutureProducer,
}

impl KafkaResultPublisher {
    /// Creates a new [`KafkaResultPublisher`] from the given configs.
    ///
    /// Constructs an `rdkafka` [`ClientConfig`] from the Kafka configuration,
    /// builds a [`FutureProducer`], and stores the result topic name.
    ///
    /// SASL credentials are read from the environment variable names specified
    /// in `kafka_config.sasl_username_env` and `kafka_config.sasl_password_env`.
    ///
    /// # Arguments
    ///
    /// * `kafka_config` - Kafka broker and security configuration.
    /// * `topics` - Kafka topic names.
    ///
    /// # Errors
    ///
    /// - Returns [`PipelineError::Auth`] if a required SASL credential
    ///   environment variable is not set.
    /// - Returns [`PipelineError::Kafka`] if the producer cannot be created
    ///   (e.g. invalid broker address format).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::config::{KafkaConfig, TopicsConfig};
    /// use xzardgz::watcher::publisher::KafkaResultPublisher;
    ///
    /// let kafka = KafkaConfig::default();
    /// let topics = TopicsConfig::default();
    /// let publisher = KafkaResultPublisher::new(&kafka, &topics).unwrap();
    /// ```
    pub fn new(kafka_config: &KafkaConfig, topics: &TopicsConfig) -> Result<Self> {
        let mut client_config = ClientConfig::new();

        client_config.set("bootstrap.servers", kafka_config.brokers.join(","));
        client_config.set("group.id", &kafka_config.group_id);
        client_config.set("security.protocol", &kafka_config.security_protocol);

        if let Some(ref mechanism) = kafka_config.sasl_mechanism {
            client_config.set("sasl.mechanism", mechanism);
        }

        if let Some(ref username_env) = kafka_config.sasl_username_env {
            let username = std::env::var(username_env).map_err(|_| {
                PipelineError::Auth(format!(
                    "SASL username environment variable '{username_env}' is not set"
                ))
            })?;
            client_config.set("sasl.username", username);
        }

        if let Some(ref password_env) = kafka_config.sasl_password_env {
            let password = std::env::var(password_env).map_err(|_| {
                PipelineError::Auth(format!(
                    "SASL password environment variable '{password_env}' is not set"
                ))
            })?;
            client_config.set("sasl.password", password);
        }

        if let Some(ref ca_location) = kafka_config.ssl_ca_location {
            client_config.set("ssl.ca.location", ca_location);
        }

        let producer: FutureProducer = client_config
            .create()
            .map_err(|e| PipelineError::Kafka(format!("failed to create Kafka producer: {e}")))?;

        Ok(Self {
            topic: topics.result.clone(),
            producer,
        })
    }
}

#[async_trait]
impl ResultPublisher for KafkaResultPublisher {
    /// Publishes a [`WatcherResultMessage`] to the configured Kafka result topic.
    ///
    /// Serializes the result to JSON and sends it with `correlation_id` as the
    /// message key.  Waits up to 5 seconds for delivery confirmation.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Kafka`] if serialization or delivery fails.
    async fn publish(&self, result: &WatcherResultMessage) -> Result<()> {
        let json = result.to_json()?;

        let record = FutureRecord::to(&self.topic)
            .payload(json.as_bytes())
            .key(result.correlation_id.as_str());

        self.producer
            .send(record, Duration::from_secs(5))
            .await
            .map_err(|(e, _)| {
                PipelineError::Kafka(format!("failed to deliver result message to Kafka: {e}"))
            })?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    use crate::watcher::event_type::WatcherEventType;

    /// Builds a minimal [`WatcherResultMessage`] for use in publisher tests.
    fn make_result() -> WatcherResultMessage {
        WatcherResultMessage::new(
            "pub-res-001",
            WatcherEventType::TechnicalReviewResult,
            "xzardgz-watcher",
            "github.com/test/repo",
            "technical_review",
            "ws-pub-001",
            "pub-corr-001",
            "pub-task-001",
            Utc::now(),
        )
    }

    // ------------------------------------------------------------------
    // PublishFailureState::new
    // ------------------------------------------------------------------

    #[test]
    fn test_publish_failure_state_new_sets_task_id() {
        let state = PublishFailureState::new("t-001", make_result());
        assert_eq!(state.task_id, "t-001");
    }

    #[test]
    fn test_publish_failure_state_new_sets_zero_attempts() {
        let state = PublishFailureState::new("t-001", make_result());
        assert_eq!(state.attempts, 0);
    }

    #[test]
    fn test_publish_failure_state_new_last_error_is_none() {
        let state = PublishFailureState::new("t-001", make_result());
        assert!(state.last_error.is_none());
    }

    #[test]
    fn test_publish_failure_state_new_last_attempt_at_is_none() {
        let state = PublishFailureState::new("t-001", make_result());
        assert!(state.last_attempt_at.is_none());
    }

    // ------------------------------------------------------------------
    // PublishFailureState::record_failure
    // ------------------------------------------------------------------

    #[test]
    fn test_publish_failure_state_record_failure_increments_attempts() {
        let mut state = PublishFailureState::new("t-001", make_result());
        state.record_failure("first error");
        assert_eq!(state.attempts, 1);
        state.record_failure("second error");
        assert_eq!(state.attempts, 2);
    }

    #[test]
    fn test_publish_failure_state_record_failure_sets_last_error() {
        let mut state = PublishFailureState::new("t-001", make_result());
        state.record_failure("broker down");
        assert_eq!(state.last_error.as_deref(), Some("broker down"));
    }

    #[test]
    fn test_publish_failure_state_record_failure_sets_last_attempt_at() {
        let mut state = PublishFailureState::new("t-001", make_result());
        state.record_failure("timeout");
        assert!(state.last_attempt_at.is_some());
    }

    #[test]
    fn test_publish_failure_state_record_failure_updates_last_error_on_second_call() {
        let mut state = PublishFailureState::new("t-001", make_result());
        state.record_failure("first");
        state.record_failure("second");
        assert_eq!(state.last_error.as_deref(), Some("second"));
    }

    // ------------------------------------------------------------------
    // PublishFailureState::to_json / from_json
    // ------------------------------------------------------------------

    #[test]
    fn test_publish_failure_state_to_json_produces_non_empty_string() {
        let state = PublishFailureState::new("t-001", make_result());
        // SAFETY: well-formed struct; serialization cannot fail.
        let json = state.to_json().unwrap();
        assert!(!json.is_empty());
    }

    #[test]
    fn test_publish_failure_state_to_json_contains_task_id() {
        let state = PublishFailureState::new("t-serialize-001", make_result());
        // SAFETY: well-formed struct; serialization cannot fail.
        let json = state.to_json().unwrap();
        assert!(json.contains("t-serialize-001"));
    }

    #[test]
    fn test_publish_failure_state_from_json_invalid_returns_err() {
        let result = PublishFailureState::from_json("not valid json");
        assert!(result.is_err());
        assert!(matches!(result, Err(PipelineError::Kafka(_))));
    }

    #[test]
    fn test_publish_failure_state_to_json_from_json_roundtrip() {
        let mut state = PublishFailureState::new("t-rt-001", make_result());
        state.record_failure("test error");

        // SAFETY: well-formed struct; serialization cannot fail.
        let json = state.to_json().unwrap();
        // SAFETY: we just serialized this string.
        let restored = PublishFailureState::from_json(&json).unwrap();

        assert_eq!(restored.task_id, state.task_id);
        assert_eq!(restored.attempts, state.attempts);
        assert_eq!(restored.last_error, state.last_error);
    }

    // ------------------------------------------------------------------
    // PublishFailureState::persist / load
    // ------------------------------------------------------------------

    #[test]
    fn test_publish_failure_state_persist_creates_file() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("failure.json");

        let state = PublishFailureState::new("t-persist-001", make_result());
        // SAFETY: temp dir is writable in standard test environments.
        state.persist(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_publish_failure_state_persist_creates_parent_directories() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("nested").join("dir").join("failure.json");

        let state = PublishFailureState::new("t-nested-001", make_result());
        // SAFETY: temp dir is writable in standard test environments.
        state.persist(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_publish_failure_state_load_returns_none_when_file_absent() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("nonexistent.json");

        let result = PublishFailureState::load(&path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_publish_failure_state_persist_and_load_roundtrip() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("persist_load.json");

        let mut state = PublishFailureState::new("t-load-001", make_result());
        state.record_failure("network error");

        // SAFETY: temp dir is writable in standard test environments.
        state.persist(&path).unwrap();

        // SAFETY: we just wrote the file; it exists and contains valid JSON.
        let loaded = PublishFailureState::load(&path).unwrap().unwrap();

        assert_eq!(loaded.task_id, state.task_id);
        assert_eq!(loaded.attempts, state.attempts);
        assert_eq!(loaded.last_error, state.last_error);
        assert_eq!(loaded.result.id, state.result.id);
    }
}
