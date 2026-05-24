//! Workspace state model and serialization.
//!
//! This module defines [`WorkspaceState`], the complete serializable snapshot
//! of a pipeline run, and [`PluginOutputRecord`], which captures per-step
//! plugin execution results. Both types round-trip through YAML via
//! `serde_yaml`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{PipelineError, Result};
use crate::workspace::id::{WORKSPACE_STATE_VERSION, now_utc};
use crate::workspace::stage::WorkspaceStage;

// ---------------------------------------------------------------------------
// PluginOutputRecord
// ---------------------------------------------------------------------------

/// Persistent record of a single plugin step execution.
///
/// One record is stored per `step_id` in [`WorkspaceState::plugin_outputs`].
/// Re-running a step replaces the previous record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginOutputRecord {
    /// The step ID within the workflow plan.
    pub step_id: String,
    /// The plugin name that was executed.
    pub plugin: String,
    /// Path to the plugin output file, if written.
    #[serde(default)]
    pub output_path: Option<String>,
    /// Timestamp when the step completed.
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    /// Whether the step completed successfully.
    #[serde(default)]
    pub success: bool,
    /// Diagnostic messages collected during execution.
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

// ---------------------------------------------------------------------------
// WorkspaceState
// ---------------------------------------------------------------------------

/// Complete, serializable state of a workspace pipeline run.
///
/// This state is written to `state.yaml` in the workspace directory and
/// loaded on resume. All fields use `#[serde(default)]` on optional items so
/// that a partial state file written by an older version can still be loaded.
///
/// The file is managed by [`crate::workspace::WorkspaceManager`], which calls
/// [`WorkspaceState::to_yaml`] and [`WorkspaceState::load_from_str`]
/// automatically on every mutating operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceState {
    /// State file schema version. Always `"1"` for this release.
    pub version: String,
    /// Unique workspace identifier (ULID).
    pub workspace_id: String,
    /// Repository URL or path used to create this workspace.
    pub repository_url: String,
    /// SHA-256 hex hash of `repository_url` for fast lookup.
    pub repository_hash: String,
    /// Local filesystem path where the repository is checked out.
    #[serde(default)]
    pub local_repository_path: Option<String>,
    /// The branch name currently checked out.
    #[serde(default)]
    pub branch_name: Option<String>,
    /// The branch that was requested by the workflow plan or CLI.
    #[serde(default)]
    pub target_branch: Option<String>,
    /// Current pipeline stage.
    pub current_stage: WorkspaceStage,
    /// Path to the scan artifact YAML file.
    #[serde(default)]
    pub scan_artifact_path: Option<String>,
    /// Version string embedded in the scan artifact.
    #[serde(default)]
    pub scan_artifact_version: Option<String>,
    /// When the scan artifact was created.
    #[serde(default)]
    pub scan_artifact_created_at: Option<DateTime<Utc>>,
    /// HEAD commit hash at the time the scan artifact was created.
    #[serde(default)]
    pub scan_artifact_head_commit: Option<String>,
    /// Map of `step_id` -> plugin output record for completed plugin steps.
    #[serde(default)]
    pub plugin_outputs: HashMap<String, PluginOutputRecord>,
    /// All files written by this workspace run (for auditing and cleanup).
    #[serde(default)]
    pub written_files: Vec<String>,
    /// Map of stage label -> timestamp when that stage was entered.
    #[serde(default)]
    pub stage_timestamps: HashMap<String, DateTime<Utc>>,
    /// When this workspace was first created.
    pub created_at: DateTime<Utc>,
    /// When this workspace state was last updated.
    pub updated_at: DateTime<Utc>,
    /// Map of `step_id` -> list of report file paths produced.
    #[serde(default)]
    pub report_paths: HashMap<String, Vec<String>>,
    /// Map of `step_id` -> numeric score produced by the plugin.
    #[serde(default)]
    pub plugin_scores: HashMap<String, f64>,
    /// Map of `step_id` -> list of diagnostic messages from the plugin.
    #[serde(default)]
    pub plugin_diagnostics: HashMap<String, Vec<String>>,
    /// Watcher task ID when this workspace was created from a watcher message.
    #[serde(default)]
    pub watcher_task_id: Option<String>,
    /// Whether the watcher result has been successfully published.
    #[serde(default)]
    pub watcher_result_published: bool,
}

// ---------------------------------------------------------------------------
// WorkspaceState impl
// ---------------------------------------------------------------------------

impl WorkspaceState {
    /// Creates a new `WorkspaceState` for a repository.
    ///
    /// Sets `version`, `workspace_id`, `repository_url`, `repository_hash`,
    /// `target_branch`, `watcher_task_id`, `current_stage`
    /// ([`WorkspaceStage::Initializing`]), `created_at`, and `updated_at`.
    /// All other fields are left at their type defaults.
    ///
    /// # Arguments
    ///
    /// * `workspace_id` - Unique ULID workspace identifier.
    /// * `repository_url` - Repository URL or local path.
    /// * `repository_hash` - SHA-256 hex digest of `repository_url`.
    /// * `target_branch` - Optional branch name requested by the caller.
    /// * `watcher_task_id` - Optional watcher task ID for watcher-triggered runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::state::WorkspaceState;
    /// use xzardgz::workspace::id::{new_workspace_id, hash_repository};
    ///
    /// let id = new_workspace_id();
    /// let hash = hash_repository("https://github.com/example/repo");
    /// let state = WorkspaceState::new(
    ///     id.clone(),
    ///     "https://github.com/example/repo".to_string(),
    ///     hash,
    ///     None,
    ///     None,
    /// );
    /// assert_eq!(state.workspace_id, id);
    /// ```
    pub fn new(
        workspace_id: String,
        repository_url: String,
        repository_hash: String,
        target_branch: Option<String>,
        watcher_task_id: Option<String>,
    ) -> Self {
        let now = now_utc();
        Self {
            version: WORKSPACE_STATE_VERSION.to_string(),
            workspace_id,
            repository_url,
            repository_hash,
            local_repository_path: None,
            branch_name: None,
            target_branch,
            current_stage: WorkspaceStage::Initializing,
            scan_artifact_path: None,
            scan_artifact_version: None,
            scan_artifact_created_at: None,
            scan_artifact_head_commit: None,
            plugin_outputs: HashMap::new(),
            written_files: Vec::new(),
            stage_timestamps: HashMap::new(),
            created_at: now,
            updated_at: now,
            report_paths: HashMap::new(),
            plugin_scores: HashMap::new(),
            plugin_diagnostics: HashMap::new(),
            watcher_task_id,
            watcher_result_published: false,
        }
    }

    /// Parses a `WorkspaceState` from a YAML string.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workspace`] if the string is not valid YAML
    /// or cannot be deserialized into a [`WorkspaceState`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::state::WorkspaceState;
    /// use xzardgz::workspace::id::{new_workspace_id, hash_repository};
    ///
    /// let state = WorkspaceState::new(
    ///     new_workspace_id(),
    ///     "https://github.com/example/repo".to_string(),
    ///     hash_repository("https://github.com/example/repo"),
    ///     None,
    ///     None,
    /// );
    /// // SAFETY: Serialization of a valid WorkspaceState cannot fail.
    /// let yaml = state.to_yaml().unwrap();
    /// // SAFETY: Deserializing just-serialized state cannot fail.
    /// let loaded = WorkspaceState::load_from_str(&yaml).unwrap();
    /// assert_eq!(loaded.repository_url, "https://github.com/example/repo");
    /// ```
    pub fn load_from_str(content: &str) -> Result<Self> {
        serde_yaml::from_str(content).map_err(|e| {
            PipelineError::Workspace(format!("failed to parse workspace state: {}", e))
        })
    }

    /// Serializes the state to a YAML string.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workspace`] if the value cannot be serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::state::WorkspaceState;
    /// use xzardgz::workspace::id::{new_workspace_id, hash_repository};
    ///
    /// let state = WorkspaceState::new(
    ///     new_workspace_id(),
    ///     "https://github.com/example/repo".to_string(),
    ///     hash_repository("https://github.com/example/repo"),
    ///     None,
    ///     None,
    /// );
    /// // SAFETY: Serialization of a valid WorkspaceState cannot fail.
    /// let yaml = state.to_yaml().unwrap();
    /// assert!(yaml.contains("repository_url"));
    /// ```
    pub fn to_yaml(&self) -> Result<String> {
        serde_yaml::to_string(self).map_err(|e| {
            PipelineError::Workspace(format!("failed to serialize workspace state: {}", e))
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::id::{hash_repository, new_workspace_id};

    /// Builds a minimal valid `WorkspaceState` for use in tests.
    fn make_state() -> WorkspaceState {
        WorkspaceState::new(
            new_workspace_id(),
            "https://github.com/example/repo".to_string(),
            hash_repository("https://github.com/example/repo"),
            Some("main".to_string()),
            None,
        )
    }

    #[test]
    fn test_new_creates_state_with_correct_fields() {
        let id = new_workspace_id();
        let url = "https://github.com/example/repo".to_string();
        let hash = hash_repository(&url);

        let state = WorkspaceState::new(
            id.clone(),
            url.clone(),
            hash.clone(),
            Some("main".to_string()),
            Some("task-123".to_string()),
        );

        assert_eq!(state.workspace_id, id, "workspace_id mismatch");
        assert_eq!(state.repository_url, url, "repository_url mismatch");
        assert_eq!(state.repository_hash, hash, "repository_hash mismatch");
        assert_eq!(
            state.target_branch,
            Some("main".to_string()),
            "target_branch mismatch"
        );
        assert_eq!(
            state.watcher_task_id,
            Some("task-123".to_string()),
            "watcher_task_id mismatch"
        );
        assert!(!state.watcher_result_published, "should start unpublished");
        assert!(
            state.plugin_outputs.is_empty(),
            "plugin_outputs should be empty"
        );
        assert!(
            state.written_files.is_empty(),
            "written_files should be empty"
        );
        assert!(
            state.report_paths.is_empty(),
            "report_paths should be empty"
        );
        assert!(
            state.stage_timestamps.is_empty(),
            "stage_timestamps should be empty"
        );
        assert!(
            state.local_repository_path.is_none(),
            "local_repository_path should be None"
        );
        assert!(state.branch_name.is_none(), "branch_name should be None");
    }

    #[test]
    fn test_new_sets_initializing_stage() {
        let state = make_state();
        assert_eq!(
            state.current_stage,
            WorkspaceStage::Initializing,
            "new state should start in Initializing stage"
        );
    }

    #[test]
    fn test_new_sets_version_to_state_version_constant() {
        let state = make_state();
        assert_eq!(
            state.version, WORKSPACE_STATE_VERSION,
            "version should equal WORKSPACE_STATE_VERSION"
        );
        assert_eq!(state.version, "1", "version should be the string '1'");
    }

    #[test]
    fn test_to_yaml_produces_valid_yaml() {
        let state = make_state();
        let result = state.to_yaml();

        assert!(
            result.is_ok(),
            "to_yaml should succeed for a valid state, got: {:?}",
            result.err()
        );

        let yaml = result.unwrap();
        assert!(
            yaml.contains("repository_url"),
            "YAML should contain 'repository_url'"
        );
        assert!(
            yaml.contains("workspace_id"),
            "YAML should contain 'workspace_id'"
        );
        assert!(yaml.contains("version"), "YAML should contain 'version'");
        assert!(
            yaml.contains("current_stage"),
            "YAML should contain 'current_stage'"
        );
    }

    #[test]
    fn test_load_from_str_round_trips_state() {
        let state = make_state();

        // SAFETY: Serialization of a just-constructed WorkspaceState cannot fail.
        let yaml = state.to_yaml().expect("SAFETY: serialization cannot fail");

        let loaded =
            // SAFETY: Deserializing a just-serialized value cannot fail.
            WorkspaceState::load_from_str(&yaml).expect("SAFETY: round-trip cannot fail");

        assert_eq!(
            loaded.workspace_id, state.workspace_id,
            "workspace_id mismatch"
        );
        assert_eq!(
            loaded.repository_url, state.repository_url,
            "repository_url mismatch"
        );
        assert_eq!(
            loaded.repository_hash, state.repository_hash,
            "repository_hash mismatch"
        );
        assert_eq!(loaded.version, state.version, "version mismatch");
        assert_eq!(
            loaded.current_stage, state.current_stage,
            "current_stage mismatch"
        );
        assert_eq!(
            loaded.target_branch, state.target_branch,
            "target_branch mismatch"
        );
    }

    #[test]
    fn test_load_from_str_rejects_invalid_yaml() {
        // Syntactically invalid YAML -- unclosed flow sequence.
        let result = WorkspaceState::load_from_str("key: [unclosed bracket\nanother: value");

        assert!(result.is_err(), "invalid YAML should be rejected");

        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("workspace error"),
            "error should be reported as a workspace error, got: {}",
            msg
        );
    }

    #[test]
    fn test_plugin_output_record_defaults() {
        // Verify direct construction with default optional fields.
        let record = PluginOutputRecord {
            step_id: "step1".to_string(),
            plugin: "test-plugin".to_string(),
            output_path: None,
            completed_at: None,
            success: false,
            diagnostics: vec![],
        };

        assert!(!record.success, "success should default to false");
        assert!(
            record.output_path.is_none(),
            "output_path should default to None"
        );
        assert!(
            record.completed_at.is_none(),
            "completed_at should default to None"
        );
        assert!(
            record.diagnostics.is_empty(),
            "diagnostics should default to empty"
        );

        // Verify serde #[serde(default)] fills in omitted optional fields.
        let yaml = "step_id: step2\nplugin: other-plugin\n";
        let deserialized: std::result::Result<PluginOutputRecord, _> = serde_yaml::from_str(yaml);

        assert!(
            deserialized.is_ok(),
            "minimal YAML should deserialize successfully, got: {:?}",
            deserialized.err()
        );

        let r = deserialized.unwrap();
        assert!(!r.success, "serde default: success should be false");
        assert!(
            r.output_path.is_none(),
            "serde default: output_path should be None"
        );
        assert!(
            r.diagnostics.is_empty(),
            "serde default: diagnostics should be empty"
        );
    }
}
