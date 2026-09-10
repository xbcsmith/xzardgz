//! Workspace pipeline stage model.
//!
//! This module defines the [`WorkspaceStage`] enum, which tracks the current
//! execution stage of a workspace pipeline run.  Both local and watcher
//! workflows share this stage model.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// WorkspaceStage
// ---------------------------------------------------------------------------

/// The current execution stage of a workspace pipeline run.
///
/// Stages progress from `Initializing` through scanning, plugin execution,
/// report writing, and publishing to a terminal `Complete` or `Failed` state.
/// Both local and watcher workflows share this stage model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceStage {
    /// Workspace is being set up; no work has started.
    Initializing,

    /// Repository scan is in progress.
    Scanning,

    /// Repository scan completed and artifact is written.
    ScanComplete,

    /// A plugin step is currently executing.
    PluginRunning {
        /// ID of the plugin step being executed.
        step_id: String,
    },

    /// A plugin step completed successfully.
    PluginComplete {
        /// ID of the plugin step that completed.
        step_id: String,
    },

    /// Reports are being written.
    ReportWriting,

    /// All reports have been written successfully.
    ReportComplete,

    /// A pull request is being created for a feature branch.
    PrCreating {
        /// The feature branch being submitted as a PR.
        branch: String,
    },

    /// A pull request was successfully created.
    PrComplete {
        /// The feature branch submitted as a PR.
        branch: String,
        /// The GitHub pull request number.
        pr_number: u64,
        /// The GitHub pull request HTML URL.
        pr_url: String,
    },

    /// Watcher result is being published to Kafka.
    Publishing,

    /// Workflow completed successfully.
    Complete,

    /// A stage failed; diagnostics are preserved.
    Failed {
        /// Human-readable label of the stage where failure occurred.
        stage: String,
        /// Description of the failure reason.
        reason: String,
    },
}

impl WorkspaceStage {
    /// Returns `true` if this stage represents successful workflow completion.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::stage::WorkspaceStage;
    /// assert!(WorkspaceStage::Complete.is_complete());
    /// assert!(!WorkspaceStage::Scanning.is_complete());
    /// ```
    pub fn is_complete(&self) -> bool {
        matches!(self, WorkspaceStage::Complete)
    }

    /// Returns `true` if this stage represents a failure.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::stage::WorkspaceStage;
    /// let failed = WorkspaceStage::Failed {
    ///     stage: "scanning".to_string(),
    ///     reason: "timeout".to_string(),
    /// };
    /// assert!(failed.is_failed());
    /// assert!(!WorkspaceStage::Complete.is_failed());
    /// ```
    pub fn is_failed(&self) -> bool {
        matches!(self, WorkspaceStage::Failed { .. })
    }

    /// Returns `true` if this stage is a terminal state (complete or failed).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::stage::WorkspaceStage;
    /// assert!(WorkspaceStage::Complete.is_terminal());
    /// let failed = WorkspaceStage::Failed {
    ///     stage: "scanning".to_string(),
    ///     reason: "io error".to_string(),
    /// };
    /// assert!(failed.is_terminal());
    /// assert!(!WorkspaceStage::Scanning.is_terminal());
    /// ```
    pub fn is_terminal(&self) -> bool {
        self.is_complete() || self.is_failed()
    }

    /// Returns a short, human-readable label for the stage.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::stage::WorkspaceStage;
    /// assert_eq!(WorkspaceStage::Scanning.label(), "scanning");
    /// assert_eq!(WorkspaceStage::Complete.label(), "complete");
    /// ```
    pub fn label(&self) -> &str {
        match self {
            WorkspaceStage::Initializing => "initializing",
            WorkspaceStage::Scanning => "scanning",
            WorkspaceStage::ScanComplete => "scan_complete",
            WorkspaceStage::PluginRunning { .. } => "plugin_running",
            WorkspaceStage::PluginComplete { .. } => "plugin_complete",
            WorkspaceStage::ReportWriting => "report_writing",
            WorkspaceStage::ReportComplete => "report_complete",
            WorkspaceStage::PrCreating { .. } => "pr_creating",
            WorkspaceStage::PrComplete { .. } => "pr_complete",
            WorkspaceStage::Publishing => "publishing",
            WorkspaceStage::Complete => "complete",
            WorkspaceStage::Failed { .. } => "failed",
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_failed() -> WorkspaceStage {
        WorkspaceStage::Failed {
            stage: "scanning".to_string(),
            reason: "disk full".to_string(),
        }
    }

    fn make_plugin_running() -> WorkspaceStage {
        WorkspaceStage::PluginRunning {
            step_id: "step-1".to_string(),
        }
    }

    fn make_plugin_complete() -> WorkspaceStage {
        WorkspaceStage::PluginComplete {
            step_id: "step-1".to_string(),
        }
    }

    // ------------------------------------------------------------------
    // is_complete
    // ------------------------------------------------------------------

    #[test]
    fn test_is_complete_returns_true_only_for_complete() {
        assert!(WorkspaceStage::Complete.is_complete());
    }

    #[test]
    fn test_is_complete_returns_false_for_other_stages() {
        let stages = vec![
            WorkspaceStage::Initializing,
            WorkspaceStage::Scanning,
            WorkspaceStage::ScanComplete,
            make_plugin_running(),
            make_plugin_complete(),
            WorkspaceStage::ReportWriting,
            WorkspaceStage::ReportComplete,
            WorkspaceStage::PrCreating {
                branch: "feature/test".to_string(),
            },
            WorkspaceStage::PrComplete {
                branch: "feature/test".to_string(),
                pr_number: 1,
                pr_url: "https://github.com/owner/repo/pull/1".to_string(),
            },
            WorkspaceStage::Publishing,
            make_failed(),
        ];
        for stage in stages {
            assert!(
                !stage.is_complete(),
                "expected is_complete() == false for {:?}",
                stage
            );
        }
    }

    // ------------------------------------------------------------------
    // is_failed
    // ------------------------------------------------------------------

    #[test]
    fn test_is_failed_returns_true_only_for_failed() {
        assert!(make_failed().is_failed());
    }

    #[test]
    fn test_is_failed_returns_false_for_complete() {
        assert!(!WorkspaceStage::Complete.is_failed());
    }

    // ------------------------------------------------------------------
    // is_terminal
    // ------------------------------------------------------------------

    #[test]
    fn test_is_terminal_returns_true_for_complete() {
        assert!(WorkspaceStage::Complete.is_terminal());
    }

    #[test]
    fn test_is_terminal_returns_true_for_failed() {
        assert!(make_failed().is_terminal());
    }

    #[test]
    fn test_is_terminal_returns_false_for_scanning() {
        assert!(!WorkspaceStage::Scanning.is_terminal());
    }

    // ------------------------------------------------------------------
    // label
    // ------------------------------------------------------------------

    #[test]
    fn test_label_returns_correct_strings() {
        assert_eq!(WorkspaceStage::Initializing.label(), "initializing");
        assert_eq!(WorkspaceStage::Scanning.label(), "scanning");
        assert_eq!(WorkspaceStage::ScanComplete.label(), "scan_complete");
        assert_eq!(make_plugin_running().label(), "plugin_running");
        assert_eq!(make_plugin_complete().label(), "plugin_complete");
        assert_eq!(WorkspaceStage::ReportWriting.label(), "report_writing");
        assert_eq!(WorkspaceStage::ReportComplete.label(), "report_complete");
        assert_eq!(
            WorkspaceStage::PrCreating {
                branch: "feature/test".to_string(),
            }
            .label(),
            "pr_creating"
        );
        assert_eq!(
            WorkspaceStage::PrComplete {
                branch: "feature/test".to_string(),
                pr_number: 42,
                pr_url: "https://github.com/owner/repo/pull/42".to_string(),
            }
            .label(),
            "pr_complete"
        );
        assert_eq!(WorkspaceStage::Publishing.label(), "publishing");
        assert_eq!(WorkspaceStage::Complete.label(), "complete");
        assert_eq!(make_failed().label(), "failed");
    }

    #[test]
    fn test_pr_creating_label_returns_expected_string() {
        let stage = WorkspaceStage::PrCreating {
            branch: "feature/test".to_string(),
        };
        assert_eq!(stage.label(), "pr_creating");
    }

    #[test]
    fn test_pr_complete_label_returns_expected_string() {
        let stage = WorkspaceStage::PrComplete {
            branch: "feature/test".to_string(),
            pr_number: 42,
            pr_url: "https://github.com/owner/repo/pull/42".to_string(),
        };
        assert_eq!(stage.label(), "pr_complete");
    }

    #[test]
    fn test_pr_creating_is_not_complete_or_failed() {
        let stage = WorkspaceStage::PrCreating {
            branch: "feature/test".to_string(),
        };
        assert!(!stage.is_complete());
        assert!(!stage.is_failed());
        assert!(!stage.is_terminal());
    }

    #[test]
    fn test_pr_complete_is_not_terminal() {
        let stage = WorkspaceStage::PrComplete {
            branch: "feature/test".to_string(),
            pr_number: 1,
            pr_url: "https://github.com/owner/repo/pull/1".to_string(),
        };
        assert!(!stage.is_terminal());
    }

    #[test]
    fn test_pr_creating_serializes_and_deserializes_correctly() {
        let stage = WorkspaceStage::PrCreating {
            branch: "feature/my-branch".to_string(),
        };
        let yaml = serde_yaml::to_string(&stage).expect("serialization must not fail");
        let restored: WorkspaceStage =
            serde_yaml::from_str(&yaml).expect("deserialization must not fail");
        assert_eq!(stage, restored);
    }

    #[test]
    fn test_pr_complete_serializes_and_deserializes_correctly() {
        let stage = WorkspaceStage::PrComplete {
            branch: "feature/my-branch".to_string(),
            pr_number: 99,
            pr_url: "https://github.com/owner/repo/pull/99".to_string(),
        };
        let yaml = serde_yaml::to_string(&stage).expect("serialization must not fail");
        let restored: WorkspaceStage =
            serde_yaml::from_str(&yaml).expect("deserialization must not fail");
        assert_eq!(stage, restored);
    }

    // ------------------------------------------------------------------
    // serde round-trip
    // ------------------------------------------------------------------

    #[test]
    fn test_stage_serializes_to_yaml() {
        // Test a plain variant
        let stage = WorkspaceStage::Scanning;
        let yaml = serde_yaml::to_string(&stage).expect("serialization must not fail");
        let restored: WorkspaceStage =
            serde_yaml::from_str(&yaml).expect("deserialization must not fail");
        assert_eq!(stage, restored);

        // Test a variant with fields
        let stage_with_fields = WorkspaceStage::Failed {
            stage: "report_writing".to_string(),
            reason: "permission denied".to_string(),
        };
        let yaml2 = serde_yaml::to_string(&stage_with_fields).expect("serialization must not fail");
        let restored2: WorkspaceStage =
            serde_yaml::from_str(&yaml2).expect("deserialization must not fail");
        assert_eq!(stage_with_fields, restored2);

        // Test Complete round-trips correctly
        let complete = WorkspaceStage::Complete;
        let yaml3 = serde_yaml::to_string(&complete).expect("serialization must not fail");
        let restored3: WorkspaceStage =
            serde_yaml::from_str(&yaml3).expect("deserialization must not fail");
        assert_eq!(complete, restored3);
    }
}
