//! Watcher task message types.
//!
//! This module defines [`WatcherTaskMessage`], the incoming task message
//! received from Kafka. It follows a CloudEvents-inspired envelope and
//! carries all parameters needed to dispatch a plugin execution.

use crate::error::{PipelineError, Result};
use crate::watcher::event_type::WatcherEventType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Schema version for [`WatcherTaskMessage`].
pub const WATCHER_TASK_VERSION: &str = "1";

// ---------------------------------------------------------------------------
// WatcherTaskMessage
// ---------------------------------------------------------------------------

/// A watcher task message received from Kafka.
///
/// Follows a CloudEvents-inspired envelope. The `data_content_type` is always
/// `"application/json"`.
///
/// # Examples
///
/// ```
/// use xzardgz::watcher::task::{WatcherTaskMessage, WATCHER_TASK_VERSION};
/// use xzardgz::watcher::event_type::WatcherEventType;
///
/// let task = WatcherTaskMessage::new(
///     "01ABCDEF",
///     WatcherEventType::TechnicalReviewTask,
///     "xzardgz://pipeline",
///     "https://github.com/example/repo",
///     "technical_review",
///     "corr-123",
/// );
/// assert_eq!(task.version, WATCHER_TASK_VERSION);
/// assert_eq!(task.spec_version, "1.0");
/// assert!(!task.dry_run);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherTaskMessage {
    /// Message schema version. Always [`WATCHER_TASK_VERSION`].
    pub version: String,
    /// Unique message identifier (ULID).
    pub id: String,
    /// CloudEvents spec version. Always `"1.0"`.
    pub spec_version: String,
    /// Watcher event type for this task.
    pub event_type: WatcherEventType,
    /// Source URI identifying the sender.
    pub source: String,
    /// Repository URL or identifier.
    pub repository: String,
    /// Target branch to analyze. `None` means the default branch.
    #[serde(default)]
    pub target_branch: Option<String>,
    /// Provider override (e.g. `"openai"`). `None` uses the global config.
    #[serde(default)]
    pub provider: Option<String>,
    /// Model override. `None` uses the global config.
    #[serde(default)]
    pub model: Option<String>,
    /// Plugin name to execute.
    pub plugin: String,
    /// Plugin-specific configuration as a JSON value.
    #[serde(default)]
    pub plugin_config: serde_json::Value,
    /// Whether this is a dry-run (no AI calls, no writes).
    #[serde(default)]
    pub dry_run: bool,
    /// Workspace directory override. `None` uses the global config.
    #[serde(default)]
    pub workspace_directory: Option<String>,
    /// Arbitrary metadata key-value pairs.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Report formats to produce (e.g. `["markdown", "json"]`).
    #[serde(default)]
    pub requested_report_formats: Vec<String>,
    /// Correlation ID linking task to result.
    pub correlation_id: String,
    /// Reply-topic override (if the operator allows it).
    #[serde(default)]
    pub reply_topic_override: Option<String>,
}

impl WatcherTaskMessage {
    /// Creates a new [`WatcherTaskMessage`] with required fields set and all optional
    /// fields defaulted to `None` or their zero values.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique message identifier (e.g. a ULID).
    /// * `event_type` - The watcher event type for this task.
    /// * `source` - Source URI identifying the sender.
    /// * `repository` - Repository URL or identifier.
    /// * `plugin` - Plugin name to execute.
    /// * `correlation_id` - Correlation ID linking task to result.
    ///
    /// # Returns
    ///
    /// A new [`WatcherTaskMessage`] with `version` set to [`WATCHER_TASK_VERSION`]
    /// and `spec_version` set to `"1.0"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::task::{WatcherTaskMessage, WATCHER_TASK_VERSION};
    /// use xzardgz::watcher::event_type::WatcherEventType;
    ///
    /// let msg = WatcherTaskMessage::new(
    ///     "01ABCDEF",
    ///     WatcherEventType::SecurityReviewTask,
    ///     "xzardgz://watcher",
    ///     "https://github.com/org/repo",
    ///     "security_review",
    ///     "corr-456",
    /// );
    /// assert_eq!(msg.version, WATCHER_TASK_VERSION);
    /// assert_eq!(msg.spec_version, "1.0");
    /// assert!(!msg.dry_run);
    /// ```
    pub fn new(
        id: impl Into<String>,
        event_type: WatcherEventType,
        source: impl Into<String>,
        repository: impl Into<String>,
        plugin: impl Into<String>,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            version: WATCHER_TASK_VERSION.to_string(),
            id: id.into(),
            spec_version: "1.0".to_string(),
            event_type,
            source: source.into(),
            repository: repository.into(),
            target_branch: None,
            provider: None,
            model: None,
            plugin: plugin.into(),
            plugin_config: serde_json::Value::Null,
            dry_run: false,
            workspace_directory: None,
            metadata: HashMap::new(),
            requested_report_formats: Vec::new(),
            correlation_id: correlation_id.into(),
            reply_topic_override: None,
        }
    }

    /// Deserializes a [`WatcherTaskMessage`] from a JSON string.
    ///
    /// # Arguments
    ///
    /// * `json` - A JSON string representation of the task message.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Watcher`] when the input is not valid JSON or
    /// does not match the expected schema.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::task::WatcherTaskMessage;
    /// use xzardgz::watcher::event_type::WatcherEventType;
    ///
    /// let msg = WatcherTaskMessage::new(
    ///     "01ABCDEF",
    ///     WatcherEventType::TechnicalReviewTask,
    ///     "xzardgz://watcher",
    ///     "https://github.com/org/repo",
    ///     "technical_review",
    ///     "corr-789",
    /// );
    /// let json = msg.to_json().unwrap();
    /// let restored = WatcherTaskMessage::from_json(&json).unwrap();
    /// assert_eq!(restored.id, "01ABCDEF");
    /// ```
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .map_err(|e| PipelineError::Watcher(format!("failed to deserialize task message: {e}")))
    }

    /// Serializes this [`WatcherTaskMessage`] to a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Watcher`] when serialization fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::task::WatcherTaskMessage;
    /// use xzardgz::watcher::event_type::WatcherEventType;
    ///
    /// let msg = WatcherTaskMessage::new(
    ///     "01ABCDEF",
    ///     WatcherEventType::TechnicalReviewTask,
    ///     "xzardgz://watcher",
    ///     "https://github.com/org/repo",
    ///     "technical_review",
    ///     "corr-789",
    /// );
    /// let json = msg.to_json().unwrap();
    /// assert!(json.contains("01ABCDEF"));
    /// ```
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|e| PipelineError::Watcher(format!("failed to serialize task message: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task() -> WatcherTaskMessage {
        WatcherTaskMessage::new(
            "01HTEST1234",
            WatcherEventType::TechnicalReviewTask,
            "xzardgz://test",
            "https://github.com/test/repo",
            "technical_review",
            "corr-001",
        )
    }

    #[test]
    fn test_watcher_task_message_new_sets_version() {
        let msg = make_task();
        assert_eq!(msg.version, WATCHER_TASK_VERSION);
    }

    #[test]
    fn test_watcher_task_message_new_sets_spec_version() {
        let msg = make_task();
        assert_eq!(msg.spec_version, "1.0");
    }

    #[test]
    fn test_watcher_task_message_new_sets_id() {
        let msg = make_task();
        assert_eq!(msg.id, "01HTEST1234");
    }

    #[test]
    fn test_watcher_task_message_new_sets_event_type() {
        let msg = make_task();
        assert_eq!(msg.event_type, WatcherEventType::TechnicalReviewTask);
    }

    #[test]
    fn test_watcher_task_message_new_sets_source() {
        let msg = make_task();
        assert_eq!(msg.source, "xzardgz://test");
    }

    #[test]
    fn test_watcher_task_message_new_sets_repository() {
        let msg = make_task();
        assert_eq!(msg.repository, "https://github.com/test/repo");
    }

    #[test]
    fn test_watcher_task_message_new_sets_plugin() {
        let msg = make_task();
        assert_eq!(msg.plugin, "technical_review");
    }

    #[test]
    fn test_watcher_task_message_new_sets_correlation_id() {
        let msg = make_task();
        assert_eq!(msg.correlation_id, "corr-001");
    }

    #[test]
    fn test_watcher_task_message_new_default_dry_run_is_false() {
        let msg = make_task();
        assert!(!msg.dry_run);
    }

    #[test]
    fn test_watcher_task_message_new_default_target_branch_is_none() {
        let msg = make_task();
        assert!(msg.target_branch.is_none());
    }

    #[test]
    fn test_watcher_task_message_new_default_provider_is_none() {
        let msg = make_task();
        assert!(msg.provider.is_none());
    }

    #[test]
    fn test_watcher_task_message_new_default_model_is_none() {
        let msg = make_task();
        assert!(msg.model.is_none());
    }

    #[test]
    fn test_watcher_task_message_new_default_workspace_directory_is_none() {
        let msg = make_task();
        assert!(msg.workspace_directory.is_none());
    }

    #[test]
    fn test_watcher_task_message_new_default_metadata_is_empty() {
        let msg = make_task();
        assert!(msg.metadata.is_empty());
    }

    #[test]
    fn test_watcher_task_message_new_default_report_formats_is_empty() {
        let msg = make_task();
        assert!(msg.requested_report_formats.is_empty());
    }

    #[test]
    fn test_watcher_task_message_new_default_reply_topic_override_is_none() {
        let msg = make_task();
        assert!(msg.reply_topic_override.is_none());
    }

    #[test]
    fn test_watcher_task_message_to_json_and_from_json_roundtrip() {
        let msg = make_task();
        // SAFETY: serialization of a well-formed in-memory struct cannot fail.
        let json = msg.to_json().unwrap();
        assert!(!json.is_empty());
        // SAFETY: we just serialized this string ourselves.
        let restored = WatcherTaskMessage::from_json(&json).unwrap();
        assert_eq!(restored.id, msg.id);
        assert_eq!(restored.event_type, msg.event_type);
        assert_eq!(restored.repository, msg.repository);
        assert_eq!(restored.plugin, msg.plugin);
        assert_eq!(restored.correlation_id, msg.correlation_id);
    }

    #[test]
    fn test_watcher_task_message_from_json_rejects_invalid_json() {
        let result = WatcherTaskMessage::from_json("not json at all {{");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, crate::error::PipelineError::Watcher(_)));
    }

    #[test]
    fn test_watcher_task_message_from_json_rejects_missing_required_fields() {
        let result = WatcherTaskMessage::from_json(r#"{"id": "abc"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_watcher_task_message_from_json_preserves_optional_fields() {
        let mut msg = make_task();
        msg.dry_run = true;
        msg.target_branch = Some("main".to_string());
        msg.provider = Some("openai".to_string());
        msg.model = Some("gpt-4o".to_string());
        msg.metadata
            .insert("platform".to_string(), "github".to_string());
        msg.requested_report_formats = vec!["markdown".to_string(), "json".to_string()];

        // SAFETY: well-formed struct.
        let json = msg.to_json().unwrap();
        // SAFETY: just serialized.
        let restored = WatcherTaskMessage::from_json(&json).unwrap();

        assert!(restored.dry_run);
        assert_eq!(restored.target_branch, Some("main".to_string()));
        assert_eq!(restored.provider, Some("openai".to_string()));
        assert_eq!(restored.model, Some("gpt-4o".to_string()));
        assert_eq!(
            restored.metadata.get("platform"),
            Some(&"github".to_string()),
        );
        assert_eq!(restored.requested_report_formats.len(), 2);
    }
}
