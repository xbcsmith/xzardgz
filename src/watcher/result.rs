//! Watcher result message types.
//!
//! This module defines [`WatcherResultMessage`], published to Kafka after a
//! plugin run completes, and [`FindingsSummary`], a compact count of findings
//! grouped by severity label.

use crate::diagnostics::Diagnostic;
use crate::error::{PipelineError, Result};
use crate::providers::types::ProviderMetadata;
use crate::reports::risk_band::RiskBand;
use crate::watcher::event_type::WatcherEventType;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Schema version for [`WatcherResultMessage`].
pub const WATCHER_RESULT_VERSION: &str = "1";

// ---------------------------------------------------------------------------
// FindingsSummary
// ---------------------------------------------------------------------------

/// Summary of findings produced by a plugin run.
///
/// # Examples
///
/// ```
/// use xzardgz::watcher::result::FindingsSummary;
///
/// let mut summary = FindingsSummary::new();
/// summary.add("critical");
/// summary.add("high");
/// summary.add("critical");
/// assert_eq!(summary.total, 3);
/// assert_eq!(*summary.by_severity.get("critical").unwrap(), 2);
/// assert_eq!(*summary.by_severity.get("high").unwrap(), 1);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FindingsSummary {
    /// Total number of findings.
    pub total: usize,
    /// Finding count grouped by severity label (e.g. `"critical": 2, "high": 5`).
    pub by_severity: HashMap<String, usize>,
}

impl FindingsSummary {
    /// Creates an empty [`FindingsSummary`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::result::FindingsSummary;
    ///
    /// let summary = FindingsSummary::new();
    /// assert_eq!(summary.total, 0);
    /// assert!(summary.by_severity.is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Increments the count for the given `severity` label and the `total`.
    ///
    /// # Arguments
    ///
    /// * `severity` - The severity label to increment.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::result::FindingsSummary;
    ///
    /// let mut s = FindingsSummary::new();
    /// s.add("high");
    /// s.add("high");
    /// s.add("low");
    /// assert_eq!(s.total, 3);
    /// assert_eq!(*s.by_severity.get("high").unwrap(), 2);
    /// assert_eq!(*s.by_severity.get("low").unwrap(), 1);
    /// ```
    pub fn add(&mut self, severity: impl Into<String>) {
        let key = severity.into();
        *self.by_severity.entry(key).or_insert(0) += 1;
        self.total += 1;
    }
}

// ---------------------------------------------------------------------------
// WatcherResultMessage
// ---------------------------------------------------------------------------

/// A watcher result message published to Kafka after a plugin run.
///
/// # Examples
///
/// ```
/// use xzardgz::watcher::result::{WatcherResultMessage, WATCHER_RESULT_VERSION};
/// use xzardgz::watcher::event_type::WatcherEventType;
///
/// let started = chrono::Utc::now();
/// let result = WatcherResultMessage::new(
///     "res-01",
///     WatcherEventType::TechnicalReviewResult,
///     "xzardgz://pipeline",
///     "https://github.com/org/repo",
///     "technical_review",
///     "ws-abc",
///     "corr-123",
///     "task-001",
///     started,
/// );
/// assert_eq!(result.version, WATCHER_RESULT_VERSION);
/// assert!(!result.success);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherResultMessage {
    /// Message schema version. Always [`WATCHER_RESULT_VERSION`].
    pub version: String,
    /// Unique result message identifier (ULID).
    pub id: String,
    /// CloudEvents spec version. Always `"1.0"`.
    pub spec_version: String,
    /// Result event type.
    pub event_type: WatcherEventType,
    /// Source URI identifying the publisher.
    pub source: String,
    /// Whether the plugin run completed successfully.
    pub success: bool,
    /// Error messages if the run failed.
    #[serde(default)]
    pub errors: Vec<String>,
    /// Diagnostics collected during the run.
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
    /// Repository URL or identifier.
    pub repository: String,
    /// Branch that was analyzed.
    #[serde(default)]
    pub target_branch: Option<String>,
    /// Plugin name that was executed.
    pub plugin: String,
    /// Workspace identifier.
    pub workspace_id: String,
    /// Workspace filesystem path.
    #[serde(default)]
    pub workspace_path: Option<String>,
    /// Path to the scan artifact file.
    #[serde(default)]
    pub scan_artifact_path: Option<String>,
    /// Map of format label to written report paths.
    #[serde(default)]
    pub report_paths: HashMap<String, Vec<String>>,
    /// Summary of findings produced.
    pub findings_summary: FindingsSummary,
    /// Overall risk band derived from findings.
    #[serde(default)]
    pub risk_band: Option<RiskBand>,
    /// Path to the SARIF report, if generated.
    #[serde(default)]
    pub sarif_path: Option<String>,
    /// AI provider metadata.
    #[serde(default)]
    pub provider_metadata: Option<ProviderMetadata>,
    /// Model identifier used for analysis.
    #[serde(default)]
    pub model_id: Option<String>,
    /// UTC timestamp when the run started.
    pub started_at: chrono::DateTime<Utc>,
    /// UTC timestamp when the run completed.
    pub completed_at: chrono::DateTime<Utc>,
    /// Correlation ID linking this result to the original task.
    pub correlation_id: String,
    /// Original task message ID.
    pub original_task_id: String,
}

impl WatcherResultMessage {
    /// Creates a new [`WatcherResultMessage`] with required fields set and all optional
    /// fields defaulted to `None` or their zero values.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique result message identifier (ULID).
    /// * `event_type` - The result event type.
    /// * `source` - Source URI identifying the publisher.
    /// * `repository` - Repository URL or identifier.
    /// * `plugin` - Plugin name that was executed.
    /// * `workspace_id` - Workspace identifier.
    /// * `correlation_id` - Correlation ID linking this result to the original task.
    /// * `original_task_id` - Original task message ID.
    /// * `started_at` - UTC timestamp when the run started.
    ///
    /// # Returns
    ///
    /// A new [`WatcherResultMessage`] with `completed_at` set to the current UTC time.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::result::{WatcherResultMessage, WATCHER_RESULT_VERSION};
    /// use xzardgz::watcher::event_type::WatcherEventType;
    ///
    /// let started = chrono::Utc::now();
    /// let msg = WatcherResultMessage::new(
    ///     "res-01",
    ///     WatcherEventType::TechnicalReviewResult,
    ///     "xzardgz://pipeline",
    ///     "https://github.com/org/repo",
    ///     "technical_review",
    ///     "ws-001",
    ///     "corr-123",
    ///     "task-001",
    ///     started,
    /// );
    /// assert_eq!(msg.version, WATCHER_RESULT_VERSION);
    /// assert!(!msg.success);
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        event_type: WatcherEventType,
        source: impl Into<String>,
        repository: impl Into<String>,
        plugin: impl Into<String>,
        workspace_id: impl Into<String>,
        correlation_id: impl Into<String>,
        original_task_id: impl Into<String>,
        started_at: chrono::DateTime<Utc>,
    ) -> Self {
        Self {
            version: WATCHER_RESULT_VERSION.to_string(),
            id: id.into(),
            spec_version: "1.0".to_string(),
            event_type,
            source: source.into(),
            success: false,
            errors: Vec::new(),
            diagnostics: Vec::new(),
            repository: repository.into(),
            target_branch: None,
            plugin: plugin.into(),
            workspace_id: workspace_id.into(),
            workspace_path: None,
            scan_artifact_path: None,
            report_paths: HashMap::new(),
            findings_summary: FindingsSummary::new(),
            risk_band: None,
            sarif_path: None,
            provider_metadata: None,
            model_id: None,
            started_at,
            completed_at: Utc::now(),
            correlation_id: correlation_id.into(),
            original_task_id: original_task_id.into(),
        }
    }

    /// Marks the result as successful and returns `self`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::result::WatcherResultMessage;
    /// use xzardgz::watcher::event_type::WatcherEventType;
    ///
    /// let result = WatcherResultMessage::new(
    ///     "r1", WatcherEventType::TechnicalReviewResult,
    ///     "src", "repo", "plugin", "ws", "corr", "task",
    ///     chrono::Utc::now(),
    /// ).success_result();
    /// assert!(result.success);
    /// ```
    pub fn success_result(mut self) -> Self {
        self.success = true;
        self
    }

    /// Appends an error message and marks the result as failed.
    ///
    /// # Arguments
    ///
    /// * `error` - The error message to append to `errors`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::result::WatcherResultMessage;
    /// use xzardgz::watcher::event_type::WatcherEventType;
    ///
    /// let result = WatcherResultMessage::new(
    ///     "r1", WatcherEventType::TechnicalReviewResult,
    ///     "src", "repo", "plugin", "ws", "corr", "task",
    ///     chrono::Utc::now(),
    /// ).success_result().with_error("plugin panicked");
    /// assert!(!result.success);
    /// assert_eq!(result.errors[0], "plugin panicked");
    /// ```
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.errors.push(error.into());
        self.success = false;
        self
    }

    /// Deserializes a [`WatcherResultMessage`] from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Watcher`] when the input is not valid JSON or
    /// does not match the expected schema.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::result::WatcherResultMessage;
    /// use xzardgz::watcher::event_type::WatcherEventType;
    ///
    /// let msg = WatcherResultMessage::new(
    ///     "res-01", WatcherEventType::TechnicalReviewResult,
    ///     "src", "repo", "plugin", "ws", "corr", "task",
    ///     chrono::Utc::now(),
    /// );
    /// let json = msg.to_json().unwrap();
    /// let restored = WatcherResultMessage::from_json(&json).unwrap();
    /// assert_eq!(restored.id, "res-01");
    /// ```
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| {
            PipelineError::Watcher(format!("failed to deserialize result message: {e}"))
        })
    }

    /// Serializes this [`WatcherResultMessage`] to a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Watcher`] when serialization fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::result::WatcherResultMessage;
    /// use xzardgz::watcher::event_type::WatcherEventType;
    ///
    /// let msg = WatcherResultMessage::new(
    ///     "res-01", WatcherEventType::TechnicalReviewResult,
    ///     "src", "repo", "plugin", "ws", "corr", "task",
    ///     chrono::Utc::now(),
    /// );
    /// let json = msg.to_json().unwrap();
    /// assert!(json.contains("res-01"));
    /// ```
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|e| PipelineError::Watcher(format!("failed to serialize result message: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::event_type::WatcherEventType;

    fn make_result() -> WatcherResultMessage {
        WatcherResultMessage::new(
            "res-test-01",
            WatcherEventType::TechnicalReviewResult,
            "xzardgz://test",
            "https://github.com/test/repo",
            "technical_review",
            "ws-test-001",
            "corr-001",
            "task-001",
            Utc::now(),
        )
    }

    #[test]
    fn test_watcher_result_message_new_sets_version() {
        assert_eq!(make_result().version, WATCHER_RESULT_VERSION);
    }

    #[test]
    fn test_watcher_result_message_new_sets_spec_version() {
        assert_eq!(make_result().spec_version, "1.0");
    }

    #[test]
    fn test_watcher_result_message_new_sets_id() {
        assert_eq!(make_result().id, "res-test-01");
    }

    #[test]
    fn test_watcher_result_message_new_sets_success_false() {
        assert!(!make_result().success);
    }

    #[test]
    fn test_watcher_result_message_new_sets_event_type() {
        assert_eq!(
            make_result().event_type,
            WatcherEventType::TechnicalReviewResult
        );
    }

    #[test]
    fn test_watcher_result_message_new_sets_source() {
        assert_eq!(make_result().source, "xzardgz://test");
    }

    #[test]
    fn test_watcher_result_message_new_sets_repository() {
        assert_eq!(make_result().repository, "https://github.com/test/repo");
    }

    #[test]
    fn test_watcher_result_message_new_sets_plugin() {
        assert_eq!(make_result().plugin, "technical_review");
    }

    #[test]
    fn test_watcher_result_message_new_sets_workspace_id() {
        assert_eq!(make_result().workspace_id, "ws-test-001");
    }

    #[test]
    fn test_watcher_result_message_new_sets_correlation_id() {
        assert_eq!(make_result().correlation_id, "corr-001");
    }

    #[test]
    fn test_watcher_result_message_new_sets_original_task_id() {
        assert_eq!(make_result().original_task_id, "task-001");
    }

    #[test]
    fn test_watcher_result_message_new_errors_is_empty() {
        assert!(make_result().errors.is_empty());
    }

    #[test]
    fn test_watcher_result_message_new_diagnostics_is_empty() {
        assert!(make_result().diagnostics.is_empty());
    }

    #[test]
    fn test_watcher_result_message_new_target_branch_is_none() {
        assert!(make_result().target_branch.is_none());
    }

    #[test]
    fn test_watcher_result_message_new_risk_band_is_none() {
        assert!(make_result().risk_band.is_none());
    }

    #[test]
    fn test_watcher_result_message_new_report_paths_is_empty() {
        assert!(make_result().report_paths.is_empty());
    }

    #[test]
    fn test_watcher_result_message_new_workspace_path_is_none() {
        assert!(make_result().workspace_path.is_none());
    }

    #[test]
    fn test_watcher_result_message_new_scan_artifact_path_is_none() {
        assert!(make_result().scan_artifact_path.is_none());
    }

    #[test]
    fn test_watcher_result_message_success_result_marks_success_true() {
        assert!(make_result().success_result().success);
    }

    #[test]
    fn test_watcher_result_message_with_error_appends_error_and_clears_success() {
        let msg = make_result()
            .success_result()
            .with_error("something failed");
        assert!(!msg.success);
        assert_eq!(msg.errors.len(), 1);
        assert_eq!(msg.errors[0], "something failed");
    }

    #[test]
    fn test_watcher_result_message_with_error_multiple_errors_accumulate() {
        let msg = make_result()
            .with_error("error one")
            .with_error("error two");
        assert!(!msg.success);
        assert_eq!(msg.errors.len(), 2);
        assert_eq!(msg.errors[0], "error one");
        assert_eq!(msg.errors[1], "error two");
    }

    #[test]
    fn test_watcher_result_message_to_json_and_from_json_roundtrip() {
        let msg = make_result();
        // SAFETY: serialization of a well-formed in-memory struct cannot fail.
        let json = msg.to_json().unwrap();
        assert!(!json.is_empty());
        // SAFETY: we just serialized this string.
        let restored = WatcherResultMessage::from_json(&json).unwrap();
        assert_eq!(restored.id, msg.id);
        assert_eq!(restored.event_type, msg.event_type);
        assert_eq!(restored.repository, msg.repository);
        assert_eq!(restored.plugin, msg.plugin);
        assert_eq!(restored.correlation_id, msg.correlation_id);
        assert_eq!(restored.original_task_id, msg.original_task_id);
    }

    #[test]
    fn test_watcher_result_message_from_json_rejects_invalid_json() {
        let result = WatcherResultMessage::from_json("not json at all {{");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, crate::error::PipelineError::Watcher(_)));
    }

    #[test]
    fn test_findings_summary_new_initializes_empty() {
        let summary = FindingsSummary::new();
        assert_eq!(summary.total, 0);
        assert!(summary.by_severity.is_empty());
    }

    #[test]
    fn test_findings_summary_add_increments_total() {
        let mut summary = FindingsSummary::new();
        summary.add("critical");
        assert_eq!(summary.total, 1);
        summary.add("high");
        assert_eq!(summary.total, 2);
    }

    #[test]
    fn test_findings_summary_add_increments_severity_count() {
        let mut summary = FindingsSummary::new();
        summary.add("critical");
        summary.add("critical");
        summary.add("high");
        assert_eq!(*summary.by_severity.get("critical").unwrap(), 2);
        assert_eq!(*summary.by_severity.get("high").unwrap(), 1);
    }

    #[test]
    fn test_findings_summary_add_new_severity_starts_at_one() {
        let mut summary = FindingsSummary::new();
        summary.add("low");
        assert_eq!(*summary.by_severity.get("low").unwrap(), 1);
        assert_eq!(summary.total, 1);
    }

    #[test]
    fn test_findings_summary_add_multiple_severities_independent() {
        let mut summary = FindingsSummary::new();
        summary.add("critical");
        summary.add("high");
        summary.add("medium");
        summary.add("low");
        assert_eq!(summary.total, 4);
        assert_eq!(summary.by_severity.len(), 4);
    }
}
