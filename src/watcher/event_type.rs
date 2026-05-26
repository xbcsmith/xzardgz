//! Watcher event type definitions.
//!
//! This module defines the [`WatcherEventType`] enum representing the set of
//! event types understood by the watcher, along with canonical string constants
//! for each variant.

use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// String constants
// ---------------------------------------------------------------------------

/// Canonical event type string for a technical review task.
pub const EVENT_TECHNICAL_REVIEW_TASK: &str = "xzardgz.technical_review.task";

/// Canonical event type string for a technical review result.
pub const EVENT_TECHNICAL_REVIEW_RESULT: &str = "xzardgz.technical_review.result";

/// Canonical event type string for a security review task.
pub const EVENT_SECURITY_REVIEW_TASK: &str = "xzardgz.security_review.task";

/// Canonical event type string for a security review result.
pub const EVENT_SECURITY_REVIEW_RESULT: &str = "xzardgz.security_review.result";

// ---------------------------------------------------------------------------
// WatcherEventType
// ---------------------------------------------------------------------------

/// Supported watcher event types for the first release.
///
/// Only technical and security review task/result events are supported.
/// Unknown event type strings are rejected by the matcher.
///
/// # Examples
///
/// ```
/// use xzardgz::watcher::event_type::{WatcherEventType, EVENT_TECHNICAL_REVIEW_TASK};
///
/// let et = WatcherEventType::TechnicalReviewTask;
/// assert_eq!(et.as_str(), EVENT_TECHNICAL_REVIEW_TASK);
/// assert!(et.is_task());
/// assert!(!et.is_result());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatcherEventType {
    /// An incoming technical review task.
    TechnicalReviewTask,
    /// A published technical review result.
    TechnicalReviewResult,
    /// An incoming security review task.
    SecurityReviewTask,
    /// A published security review result.
    SecurityReviewResult,
}

impl WatcherEventType {
    /// Returns the canonical event type string for this variant.
    ///
    /// # Returns
    ///
    /// One of the `EVENT_*` constants defined in this module.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::event_type::{WatcherEventType, EVENT_SECURITY_REVIEW_RESULT};
    ///
    /// assert_eq!(
    ///     WatcherEventType::SecurityReviewResult.as_str(),
    ///     EVENT_SECURITY_REVIEW_RESULT,
    /// );
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TechnicalReviewTask => EVENT_TECHNICAL_REVIEW_TASK,
            Self::TechnicalReviewResult => EVENT_TECHNICAL_REVIEW_RESULT,
            Self::SecurityReviewTask => EVENT_SECURITY_REVIEW_TASK,
            Self::SecurityReviewResult => EVENT_SECURITY_REVIEW_RESULT,
        }
    }

    /// Parses an event type from its canonical string form.
    ///
    /// Returns `None` for unrecognised strings so callers can apply
    /// reject-by-default routing logic without panicking.
    ///
    /// # Arguments
    ///
    /// * `s` - The event type string to parse.
    ///
    /// # Returns
    ///
    /// `Some(WatcherEventType)` when `s` matches a known constant; `None`
    /// otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::event_type::{WatcherEventType, EVENT_SECURITY_REVIEW_TASK};
    ///
    /// let et = WatcherEventType::from_event_str(EVENT_SECURITY_REVIEW_TASK);
    /// assert_eq!(et, Some(WatcherEventType::SecurityReviewTask));
    ///
    /// assert_eq!(WatcherEventType::from_event_str("unknown.event"), None);
    /// ```
    #[allow(clippy::should_implement_trait)]
    pub fn from_event_str(s: &str) -> Option<Self> {
        match s {
            EVENT_TECHNICAL_REVIEW_TASK => Some(Self::TechnicalReviewTask),
            EVENT_TECHNICAL_REVIEW_RESULT => Some(Self::TechnicalReviewResult),
            EVENT_SECURITY_REVIEW_TASK => Some(Self::SecurityReviewTask),
            EVENT_SECURITY_REVIEW_RESULT => Some(Self::SecurityReviewResult),
            _ => None,
        }
    }

    /// Returns `true` when this variant represents a task (incoming work item).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::event_type::WatcherEventType;
    ///
    /// assert!(WatcherEventType::TechnicalReviewTask.is_task());
    /// assert!(WatcherEventType::SecurityReviewTask.is_task());
    /// assert!(!WatcherEventType::TechnicalReviewResult.is_task());
    /// assert!(!WatcherEventType::SecurityReviewResult.is_task());
    /// ```
    pub fn is_task(&self) -> bool {
        matches!(self, Self::TechnicalReviewTask | Self::SecurityReviewTask)
    }

    /// Returns `true` when this variant represents a result (outgoing completion event).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::watcher::event_type::WatcherEventType;
    ///
    /// assert!(WatcherEventType::TechnicalReviewResult.is_result());
    /// assert!(WatcherEventType::SecurityReviewResult.is_result());
    /// assert!(!WatcherEventType::TechnicalReviewTask.is_result());
    /// assert!(!WatcherEventType::SecurityReviewTask.is_result());
    /// ```
    pub fn is_result(&self) -> bool {
        matches!(
            self,
            Self::TechnicalReviewResult | Self::SecurityReviewResult
        )
    }
}

impl fmt::Display for WatcherEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watcher_event_type_as_str_technical_review_task() {
        assert_eq!(
            WatcherEventType::TechnicalReviewTask.as_str(),
            EVENT_TECHNICAL_REVIEW_TASK,
        );
    }

    #[test]
    fn test_watcher_event_type_as_str_technical_review_result() {
        assert_eq!(
            WatcherEventType::TechnicalReviewResult.as_str(),
            EVENT_TECHNICAL_REVIEW_RESULT,
        );
    }

    #[test]
    fn test_watcher_event_type_as_str_security_review_task() {
        assert_eq!(
            WatcherEventType::SecurityReviewTask.as_str(),
            EVENT_SECURITY_REVIEW_TASK,
        );
    }

    #[test]
    fn test_watcher_event_type_as_str_security_review_result() {
        assert_eq!(
            WatcherEventType::SecurityReviewResult.as_str(),
            EVENT_SECURITY_REVIEW_RESULT,
        );
    }

    #[test]
    fn test_watcher_event_type_from_event_str_technical_review_task() {
        assert_eq!(
            WatcherEventType::from_event_str(EVENT_TECHNICAL_REVIEW_TASK),
            Some(WatcherEventType::TechnicalReviewTask),
        );
    }

    #[test]
    fn test_watcher_event_type_from_event_str_technical_review_result() {
        assert_eq!(
            WatcherEventType::from_event_str(EVENT_TECHNICAL_REVIEW_RESULT),
            Some(WatcherEventType::TechnicalReviewResult),
        );
    }

    #[test]
    fn test_watcher_event_type_from_event_str_security_review_task() {
        assert_eq!(
            WatcherEventType::from_event_str(EVENT_SECURITY_REVIEW_TASK),
            Some(WatcherEventType::SecurityReviewTask),
        );
    }

    #[test]
    fn test_watcher_event_type_from_event_str_security_review_result() {
        assert_eq!(
            WatcherEventType::from_event_str(EVENT_SECURITY_REVIEW_RESULT),
            Some(WatcherEventType::SecurityReviewResult),
        );
    }

    #[test]
    fn test_watcher_event_type_from_event_str_unknown_returns_none() {
        assert_eq!(WatcherEventType::from_event_str("unknown.event.type"), None);
    }

    #[test]
    fn test_watcher_event_type_from_event_str_empty_string_returns_none() {
        assert_eq!(WatcherEventType::from_event_str(""), None);
    }

    #[test]
    fn test_watcher_event_type_from_event_str_partial_match_returns_none() {
        assert_eq!(
            WatcherEventType::from_event_str("xzardgz.technical_review"),
            None,
        );
    }

    #[test]
    fn test_watcher_event_type_roundtrip_as_str_and_from_event_str() {
        let variants = [
            WatcherEventType::TechnicalReviewTask,
            WatcherEventType::TechnicalReviewResult,
            WatcherEventType::SecurityReviewTask,
            WatcherEventType::SecurityReviewResult,
        ];
        for variant in &variants {
            let s = variant.as_str();
            let parsed = WatcherEventType::from_event_str(s);
            assert_eq!(parsed.as_ref(), Some(variant), "round-trip failed for {s}");
        }
    }

    #[test]
    fn test_watcher_event_type_is_task_technical_review_task_is_true() {
        assert!(WatcherEventType::TechnicalReviewTask.is_task());
    }

    #[test]
    fn test_watcher_event_type_is_task_security_review_task_is_true() {
        assert!(WatcherEventType::SecurityReviewTask.is_task());
    }

    #[test]
    fn test_watcher_event_type_is_task_technical_review_result_is_false() {
        assert!(!WatcherEventType::TechnicalReviewResult.is_task());
    }

    #[test]
    fn test_watcher_event_type_is_task_security_review_result_is_false() {
        assert!(!WatcherEventType::SecurityReviewResult.is_task());
    }

    #[test]
    fn test_watcher_event_type_is_result_technical_review_result_is_true() {
        assert!(WatcherEventType::TechnicalReviewResult.is_result());
    }

    #[test]
    fn test_watcher_event_type_is_result_security_review_result_is_true() {
        assert!(WatcherEventType::SecurityReviewResult.is_result());
    }

    #[test]
    fn test_watcher_event_type_is_result_technical_review_task_is_false() {
        assert!(!WatcherEventType::TechnicalReviewTask.is_result());
    }

    #[test]
    fn test_watcher_event_type_is_result_security_review_task_is_false() {
        assert!(!WatcherEventType::SecurityReviewTask.is_result());
    }

    #[test]
    fn test_watcher_event_type_display_matches_as_str_for_all_variants() {
        let variants = [
            WatcherEventType::TechnicalReviewTask,
            WatcherEventType::TechnicalReviewResult,
            WatcherEventType::SecurityReviewTask,
            WatcherEventType::SecurityReviewResult,
        ];
        for variant in &variants {
            assert_eq!(variant.to_string(), variant.as_str());
        }
    }

    #[test]
    fn test_watcher_event_type_unknown_strings_return_none() {
        let unknowns = [
            "xzardgz.foo",
            "technical_review",
            "TECHNICAL_REVIEW_TASK",
            "xzardgz.technical_review.task.extra",
            "",
        ];
        for s in &unknowns {
            assert_eq!(
                WatcherEventType::from_event_str(s),
                None,
                "expected None for {s:?}",
            );
        }
    }
}
