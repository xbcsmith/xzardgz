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
use crate::providers::model_resolution::ResolvedModel;
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
    /// Correlation identifier for tracing this run end to end.
    ///
    /// Populated when the workspace is created. Survives `--resume` so that
    /// all stages of a single logical run share the same identifier.
    /// Empty string in state files written before Phase 3 was deployed.
    #[serde(default)]
    pub correlation_id: String,
    /// The resolved model record from the model resolver.
    ///
    /// Set after the pre-flight model resolution pass. Contains the selected
    /// provider, model, capability flags, thinking mode, and any diagnostics
    /// produced during resolution. `None` before resolution runs or when the
    /// pipeline is resumed from a state that pre-dates this field.
    #[serde(default)]
    pub resolved_model: Option<ResolvedModel>,
}

// ---------------------------------------------------------------------------
// WorkspaceState impl
// ---------------------------------------------------------------------------

impl WorkspaceState {
    /// Creates a new `WorkspaceState` for a repository.
    ///
    /// Sets `version`, `workspace_id`, `repository_url`, `repository_hash`,
    /// `target_branch`, `watcher_task_id`, `correlation_id`,
    /// `current_stage` ([`WorkspaceStage::Initializing`]), `created_at`, and
    /// `updated_at`. All other fields are left at their type defaults.
    ///
    /// # Arguments
    ///
    /// * `workspace_id` - Unique ULID workspace identifier.
    /// * `repository_url` - Repository URL or local path.
    /// * `repository_hash` - SHA-256 hex digest of `repository_url`.
    /// * `target_branch` - Optional branch name requested by the caller.
    /// * `watcher_task_id` - Optional watcher task ID for watcher-triggered runs.
    /// * `correlation_id` - Tracing identifier for this logical run.
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
    ///     "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
    /// );
    /// assert_eq!(state.workspace_id, id);
    /// assert_eq!(state.correlation_id, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
    /// ```
    pub fn new(
        workspace_id: String,
        repository_url: String,
        repository_hash: String,
        target_branch: Option<String>,
        watcher_task_id: Option<String>,
        correlation_id: String,
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
            correlation_id,
            resolved_model: None,
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
    ///     String::new(),
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
    ///     String::new(),
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

    /// Applies git metadata collected from a repository checkout to this state.
    ///
    /// Updates the repository URL (when the metadata includes a remote origin),
    /// the repository hash, the local checkout path, the current branch name,
    /// the target branch, and the HEAD commit used by scan artifacts. The
    /// `updated_at` timestamp is always refreshed.
    ///
    /// Fields are only overwritten when the corresponding `metadata` field is
    /// `Some(_)`, so callers can apply partial metadata without clobbering
    /// previously recorded values.
    ///
    /// # Arguments
    ///
    /// * `metadata` - Git metadata collected by [`crate::git::ops::GitRepository::metadata`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::state::WorkspaceState;
    /// use xzardgz::workspace::id::{new_workspace_id, hash_repository};
    /// use xzardgz::git::metadata::GitMetadata;
    ///
    /// let mut state = WorkspaceState::new(
    ///     new_workspace_id(),
    ///     "https://github.com/example/repo".to_string(),
    ///     hash_repository("https://github.com/example/repo"),
    ///     None,
    ///     None,
    ///     String::new(),
    /// );
    ///
    /// let meta = GitMetadata::new(
    ///     None,
    ///     None,
    ///     "/tmp/repo".to_string(),
    ///     Some("main".to_string()),
    ///     None,
    ///     Some("abc123".to_string()),
    ///     false,
    /// );
    ///
    /// state.apply_git_metadata(&meta);
    /// assert_eq!(state.branch_name.as_deref(), Some("main"));
    /// assert_eq!(state.scan_artifact_head_commit.as_deref(), Some("abc123"));
    /// ```
    pub fn apply_git_metadata(&mut self, metadata: &crate::git::metadata::GitMetadata) {
        if let Some(ref url) = metadata.repository_url {
            self.repository_url = url.clone();
        }
        if let Some(ref hash) = metadata.repository_hash {
            self.repository_hash = hash.clone();
        }
        self.local_repository_path = Some(metadata.local_repository_path.clone());
        self.branch_name = metadata.branch_name.clone();
        if let Some(ref target) = metadata.target_branch {
            self.target_branch = Some(target.clone());
        }
        self.scan_artifact_head_commit = metadata.head_commit.clone();
        self.updated_at = now_utc();
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
        let id = new_workspace_id();
        let hash = hash_repository("https://github.com/example/repo");
        WorkspaceState::new(
            id,
            "https://github.com/example/repo".to_string(),
            hash,
            None,
            None,
            "test-correlation-id".to_string(),
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
            "test-correlation-id".to_string(),
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
        assert_eq!(
            state.correlation_id, "test-correlation-id",
            "correlation_id should match the value passed to new()"
        );
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

    // ------------------------------------------------------------------
    // apply_git_metadata
    // ------------------------------------------------------------------

    #[test]
    fn test_apply_git_metadata_updates_branch_name() {
        let mut state = make_state();
        let meta = crate::git::metadata::GitMetadata::new(
            None,
            None,
            "/tmp/repo".to_string(),
            Some("feature-x".to_string()),
            None,
            None,
            false,
        );

        state.apply_git_metadata(&meta);

        assert_eq!(
            state.branch_name.as_deref(),
            Some("feature-x"),
            "branch_name should be updated from metadata"
        );
    }

    #[test]
    fn test_apply_git_metadata_updates_local_repository_path() {
        let mut state = make_state();
        let meta = crate::git::metadata::GitMetadata::new(
            None,
            None,
            "/tmp/my_local_repo".to_string(),
            None,
            None,
            None,
            false,
        );

        state.apply_git_metadata(&meta);

        assert_eq!(
            state.local_repository_path.as_deref(),
            Some("/tmp/my_local_repo"),
            "local_repository_path should be updated from metadata"
        );
    }

    #[test]
    fn test_apply_git_metadata_updates_head_commit() {
        let mut state = make_state();
        let sha = "a".repeat(40);
        let meta = crate::git::metadata::GitMetadata::new(
            None,
            None,
            "/tmp/repo".to_string(),
            None,
            None,
            Some(sha.clone()),
            false,
        );

        state.apply_git_metadata(&meta);

        assert_eq!(
            state.scan_artifact_head_commit.as_deref(),
            Some(sha.as_str()),
            "scan_artifact_head_commit should be updated from metadata"
        );
    }

    #[test]
    fn test_apply_git_metadata_with_remote_url_overwrites_repository_url() {
        let mut state = make_state();
        let new_url = "https://github.com/example/new-repo".to_string();
        let meta = crate::git::metadata::GitMetadata::new(
            Some(new_url.clone()),
            Some("newhash".to_string()),
            "/tmp/repo".to_string(),
            None,
            None,
            None,
            false,
        );

        state.apply_git_metadata(&meta);

        assert_eq!(
            state.repository_url, new_url,
            "repository_url should be updated when metadata has remote"
        );
        assert_eq!(
            state.repository_hash, "newhash",
            "repository_hash should be updated when metadata has hash"
        );
    }

    #[test]
    fn test_apply_git_metadata_without_remote_preserves_existing_url() {
        let mut state = make_state();
        let original_url = state.repository_url.clone();
        let meta = crate::git::metadata::GitMetadata::new(
            None,
            None,
            "/tmp/repo".to_string(),
            None,
            None,
            None,
            false,
        );

        state.apply_git_metadata(&meta);

        assert_eq!(
            state.repository_url, original_url,
            "repository_url should not change when metadata has no remote"
        );
    }

    #[test]
    fn test_apply_git_metadata_updates_target_branch_when_present() {
        let mut state = make_state();
        let meta = crate::git::metadata::GitMetadata::new(
            None,
            None,
            "/tmp/repo".to_string(),
            Some("feature".to_string()),
            Some("main".to_string()),
            None,
            false,
        );

        state.apply_git_metadata(&meta);

        assert_eq!(
            state.target_branch.as_deref(),
            Some("main"),
            "target_branch should be updated from metadata"
        );
    }

    #[test]
    fn test_workspace_state_resolved_model_is_none_by_default() {
        let state = make_state();
        assert!(
            state.resolved_model.is_none(),
            "resolved_model should be None when not set"
        );
    }

    #[test]
    fn test_workspace_state_resolved_model_can_be_set_and_serialized() {
        use crate::diagnostics::Diagnostics;
        use crate::providers::model_resolution::ResolvedModel;
        use crate::providers::types::{MetadataSource, ModelCapabilities, ThinkingMode};

        let mut state = make_state();
        state.resolved_model = Some(ResolvedModel {
            requested_provider: "openai".to_string(),
            selected_provider: "openai".to_string(),
            requested_model: Some("gpt-4o".to_string()),
            selected_model: "gpt-4o".to_string(),
            fallback_used: false,
            fallback_reason: None,
            capabilities: ModelCapabilities::default(),
            thinking_mode_requested: ThinkingMode::None,
            thinking_mode_selected: ThinkingMode::None,
            metadata_source: MetadataSource::Static,
            diagnostics: Diagnostics::new(),
        });

        // SAFETY: serialization of a valid WorkspaceState cannot fail.
        let yaml = state.to_yaml().expect("SAFETY: serialization cannot fail");
        assert!(
            yaml.contains("resolved_model"),
            "YAML should contain 'resolved_model' when set"
        );
        assert!(
            yaml.contains("selected_model"),
            "YAML should contain 'selected_model' field"
        );
        assert!(
            yaml.contains("gpt-4o"),
            "YAML should contain the model name"
        );
    }

    #[test]
    fn test_workspace_state_resolved_model_round_trips_through_yaml() {
        use crate::diagnostics::Diagnostics;
        use crate::providers::model_resolution::ResolvedModel;
        use crate::providers::types::{MetadataSource, ModelCapabilities, ThinkingMode};

        let mut state = make_state();
        state.resolved_model = Some(ResolvedModel {
            requested_provider: "anthropic".to_string(),
            selected_provider: "anthropic".to_string(),
            requested_model: Some("claude-3-5-sonnet-20241022".to_string()),
            selected_model: "claude-3-5-sonnet-20241022".to_string(),
            fallback_used: false,
            fallback_reason: None,
            capabilities: ModelCapabilities::default(),
            thinking_mode_requested: ThinkingMode::Auto,
            thinking_mode_selected: ThinkingMode::Low,
            metadata_source: MetadataSource::Remote,
            diagnostics: Diagnostics::new(),
        });

        // SAFETY: serialization of a valid WorkspaceState cannot fail.
        let yaml = state.to_yaml().expect("SAFETY: serialization cannot fail");
        // SAFETY: round-trip of just-serialized state cannot fail.
        let loaded = WorkspaceState::load_from_str(&yaml).expect("SAFETY: round-trip cannot fail");

        let resolved = loaded
            .resolved_model
            .expect("resolved_model should be present after round-trip");
        assert_eq!(resolved.selected_provider, "anthropic");
        assert_eq!(resolved.selected_model, "claude-3-5-sonnet-20241022");
        assert_eq!(resolved.thinking_mode_requested, ThinkingMode::Auto);
        assert_eq!(resolved.thinking_mode_selected, ThinkingMode::Low);
        assert!(!resolved.fallback_used);
    }

    #[test]
    fn test_workspace_state_with_fallback_resolved_model_round_trips() {
        use crate::diagnostics::Diagnostics;
        use crate::providers::model_resolution::ResolvedModel;
        use crate::providers::types::{MetadataSource, ModelCapabilities, ThinkingMode};

        let mut state = make_state();
        state.resolved_model = Some(ResolvedModel {
            requested_provider: "openai".to_string(),
            selected_provider: "openai".to_string(),
            requested_model: Some("gpt-5".to_string()),
            selected_model: "gpt-4o".to_string(),
            fallback_used: true,
            fallback_reason: Some("gpt-5 not available".to_string()),
            capabilities: ModelCapabilities::default(),
            thinking_mode_requested: ThinkingMode::None,
            thinking_mode_selected: ThinkingMode::None,
            metadata_source: MetadataSource::Static,
            diagnostics: Diagnostics::new(),
        });

        // SAFETY: serialization of a valid WorkspaceState cannot fail.
        let yaml = state.to_yaml().expect("SAFETY: serialization cannot fail");
        // SAFETY: round-trip of just-serialized state cannot fail.
        let loaded = WorkspaceState::load_from_str(&yaml).expect("SAFETY: round-trip cannot fail");

        let resolved = loaded
            .resolved_model
            .expect("resolved_model should be present");
        assert!(resolved.fallback_used, "fallback_used should be true");
        assert_eq!(
            resolved.fallback_reason.as_deref(),
            Some("gpt-5 not available"),
            "fallback_reason should round-trip correctly"
        );
        assert_eq!(resolved.selected_model, "gpt-4o");
    }

    #[test]
    fn test_workspace_state_load_from_legacy_yaml_without_correlation_id_field() {
        // Simulates loading a pre-Phase-3 state file that has no correlation_id key.
        // The field must default to an empty string rather than failing to parse.
        let yaml = r#"
version: "1"
workspace_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV"
repository_url: "https://github.com/example/repo"
repository_hash: "abc123"
current_stage:
  kind: initializing
created_at: "2024-01-01T00:00:00Z"
updated_at: "2024-01-01T00:00:00Z"
"#;
        // SAFETY: the YAML above is hardcoded and valid.
        let state = WorkspaceState::load_from_str(yaml).unwrap();
        assert_eq!(state.correlation_id, "");
    }

    #[test]
    fn test_workspace_state_load_from_legacy_yaml_without_resolved_model_field() {
        // A state YAML that does NOT have resolved_model should still load
        // successfully due to #[serde(default)].
        let legacy_yaml = r#"
version: "1"
workspace_id: "01TESTLEGACY0000000000000"
repository_url: "https://github.com/example/legacy"
repository_hash: "abc123"
current_stage:
  kind: initializing
created_at: "2024-01-01T00:00:00Z"
updated_at: "2024-01-01T00:00:00Z"
"#;

        let loaded = WorkspaceState::load_from_str(legacy_yaml)
            .expect("legacy YAML without resolved_model should load successfully");
        assert!(
            loaded.resolved_model.is_none(),
            "resolved_model should be None when loading legacy state"
        );
    }

    #[test]
    fn test_apply_git_metadata_refreshes_updated_at() {
        let mut state = make_state();
        let original_updated_at = state.updated_at;

        // Small sleep to ensure timestamp difference.
        std::thread::sleep(std::time::Duration::from_millis(5));

        let meta = crate::git::metadata::GitMetadata::default();
        state.apply_git_metadata(&meta);

        assert!(
            state.updated_at >= original_updated_at,
            "updated_at should be refreshed after applying metadata"
        );
    }
}
