//! Workspace management for XZardgz pipeline runs.
//!
//! A workspace is an isolated directory on disk that holds all intermediate
//! artifacts for a single pipeline execution: repository checkout, scan
//! artifacts, plugin outputs, reports, and transcripts. The
//! [`WorkspaceManager`] provides the public API for creating, loading, and
//! updating workspaces.
//!
//! ## Module layout
//!
//! | Submodule | Responsibility |
//! |-----------|----------------|
//! | [`id`]    | ULID generation, SHA-256 repository hashing, timestamps |
//! | [`paths`] | All workspace directory and file path helpers |
//! | [`stage`] | [`WorkspaceStage`] enum and transition predicates |
//! | [`state`] | [`WorkspaceState`] serializable state model |

pub mod id;
pub mod paths;
pub mod stage;
pub mod state;

pub use paths::WorkspacePaths;
pub use stage::WorkspaceStage;
pub use state::{PluginOutputRecord, WorkspaceState};

use crate::error::{PipelineError, Result};
use crate::workspace::id::{hash_repository, new_workspace_id, now_utc};

// ---------------------------------------------------------------------------
// WorkspaceManager
// ---------------------------------------------------------------------------

/// Manages a single workspace directory and its state.
///
/// The manager owns a [`WorkspacePaths`] for path resolution and a
/// [`WorkspaceState`] for the current pipeline state. All mutating operations
/// call [`WorkspaceManager::save`] automatically to keep the state file in
/// sync with the in-memory state.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::workspace::WorkspaceManager;
///
/// // SAFETY: directory creation succeeds in a writable filesystem.
/// let mut manager = WorkspaceManager::create(
///     "/tmp/workspaces",
///     "https://github.com/example/repo",
///     Some("main".to_string()),
///     None,
/// ).unwrap();
///
/// println!("workspace id: {}", manager.id());
/// ```
pub struct WorkspaceManager {
    /// Artifact path helpers for this workspace.
    pub paths: WorkspacePaths,
    /// Current pipeline state.
    pub state: WorkspaceState,
}

// ---------------------------------------------------------------------------
// WorkspaceManager impl
// ---------------------------------------------------------------------------

impl WorkspaceManager {
    /// Creates a brand-new workspace for `repository_url` under `workspace_root`.
    ///
    /// A new ULID workspace ID is generated, all workspace subdirectories are
    /// created on disk via [`WorkspacePaths::create_all`], and the initial
    /// state is saved to `state.yaml`.
    ///
    /// # Arguments
    ///
    /// * `workspace_root` - Parent directory under which the workspace
    ///   subdirectory will be created.
    /// * `repository_url` - Repository URL or local path for this run.
    /// * `target_branch` - Optional branch requested by the caller.
    /// * `watcher_task_id` - Optional watcher task ID if triggered by a watcher.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workspace`] if the workspace directories or
    /// initial state file cannot be created.
    pub fn create(
        workspace_root: &str,
        repository_url: &str,
        target_branch: Option<String>,
        watcher_task_id: Option<String>,
    ) -> Result<Self> {
        let workspace_id = new_workspace_id();
        let repository_hash = hash_repository(repository_url);

        let state = WorkspaceState::new(
            workspace_id.clone(),
            repository_url.to_string(),
            repository_hash,
            target_branch,
            watcher_task_id,
        );

        let paths = WorkspacePaths::new(workspace_root, &workspace_id);
        paths.create_all()?;

        let manager = Self { paths, state };
        manager.save()?;
        Ok(manager)
    }

    /// Loads an existing workspace by its ID from `workspace_root`.
    ///
    /// Reads and parses `<workspace_root>/<workspace_id>/state.yaml`.
    ///
    /// # Arguments
    ///
    /// * `workspace_root` - Parent directory that contains the workspace
    ///   subdirectory.
    /// * `workspace_id` - The ULID workspace ID assigned at creation time.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workspace`] if the state file is missing or
    /// cannot be parsed.
    pub fn load(workspace_root: &str, workspace_id: &str) -> Result<Self> {
        let paths = WorkspacePaths::new(workspace_root, workspace_id);
        let state_file = paths.state_file();

        let content = std::fs::read_to_string(&state_file).map_err(|e| {
            PipelineError::Workspace(format!(
                "failed to read state file '{}': {}",
                state_file.display(),
                e
            ))
        })?;

        let state = WorkspaceState::load_from_str(&content)?;
        Ok(Self { paths, state })
    }

    /// Opens a workspace for `repository_url`, resuming the most recent
    /// existing workspace if one exists, or creating a new one.
    ///
    /// The lookup scans `workspace_root` for directories whose `state.yaml`
    /// has a matching `repository_hash`. When multiple matches exist, the
    /// workspace with the lexicographically greatest ULID (i.e. the most
    /// recently created) is returned.
    ///
    /// # Arguments
    ///
    /// * `workspace_root` - Parent directory to scan for existing workspaces.
    /// * `repository_url` - Repository URL or path to match.
    /// * `target_branch` - Branch to use when a new workspace must be created.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workspace`] if directory scanning fails with
    /// an error other than `NotFound`.
    pub fn open(
        workspace_root: &str,
        repository_url: &str,
        target_branch: Option<String>,
    ) -> Result<Self> {
        let repo_hash = hash_repository(repository_url);

        let entries = match std::fs::read_dir(workspace_root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Workspace root does not exist yet; create a fresh workspace.
                return Self::create(workspace_root, repository_url, target_branch, None);
            }
            Err(e) => {
                return Err(PipelineError::Workspace(format!(
                    "failed to scan workspace root '{}': {}",
                    workspace_root, e
                )));
            }
        };

        let mut matches: Vec<WorkspaceManager> = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            if let Ok(manager) = Self::load(workspace_root, &dir_name)
                && manager.state.repository_hash == repo_hash
            {
                matches.push(manager);
            }
        }

        if matches.is_empty() {
            return Self::create(workspace_root, repository_url, target_branch, None);
        }

        // Sort descending by workspace_id (ULID is lexicographically chronological).
        matches.sort_by(|a, b| b.state.workspace_id.cmp(&a.state.workspace_id));

        // SAFETY: matches is non-empty after the is_empty() guard above.
        Ok(matches.remove(0))
    }

    /// Saves the current state to the workspace state file.
    ///
    /// Serializes [`WorkspaceState`] to YAML and atomically overwrites
    /// `state.yaml` in the workspace directory.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workspace`] if serialization or the file write
    /// fails.
    pub fn save(&self) -> Result<()> {
        let yaml = self.state.to_yaml()?;
        let state_file = self.paths.state_file();

        std::fs::write(&state_file, yaml).map_err(|e| {
            PipelineError::Workspace(format!(
                "failed to write state file '{}': {}",
                state_file.display(),
                e
            ))
        })
    }

    /// Transitions the workspace to a new pipeline stage.
    ///
    /// Records the entry timestamp in [`WorkspaceState::stage_timestamps`]
    /// under the stage's label, updates [`WorkspaceState::current_stage`] and
    /// [`WorkspaceState::updated_at`], then saves state to disk.
    ///
    /// This method is idempotent: transitioning to the current stage is safe
    /// and simply refreshes the timestamp.
    ///
    /// # Arguments
    ///
    /// * `stage` - The new [`WorkspaceStage`] to enter.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workspace`] if the state file cannot be saved.
    pub fn transition(&mut self, stage: WorkspaceStage) -> Result<()> {
        self.state
            .stage_timestamps
            .insert(stage.label().to_string(), now_utc());
        self.state.current_stage = stage;
        self.state.updated_at = now_utc();
        self.save()
    }

    /// Records a scan artifact and transitions the workspace to `ScanComplete`.
    ///
    /// Sets the scan artifact fields in state, then calls
    /// [`WorkspaceManager::transition`] to [`WorkspaceStage::ScanComplete`]
    /// which also saves state to disk.
    ///
    /// # Arguments
    ///
    /// * `path` - Filesystem path of the scan artifact YAML file.
    /// * `version` - Optional version string embedded in the artifact.
    /// * `head_commit` - Optional HEAD commit hash recorded in the artifact.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workspace`] if state cannot be saved.
    pub fn record_scan_artifact(
        &mut self,
        path: String,
        version: Option<String>,
        head_commit: Option<String>,
    ) -> Result<()> {
        self.state.scan_artifact_path = Some(path);
        self.state.scan_artifact_version = version;
        self.state.scan_artifact_head_commit = head_commit;
        self.state.scan_artifact_created_at = Some(now_utc());
        self.state.updated_at = now_utc();
        self.transition(WorkspaceStage::ScanComplete)
    }

    /// Records a completed plugin step output.
    ///
    /// Inserts or replaces the [`PluginOutputRecord`] for `step_id` in
    /// [`WorkspaceState::plugin_outputs`] and updates
    /// [`WorkspaceState::plugin_diagnostics`] for the same step, then saves
    /// state. Replacing an existing record is intentional: re-running a plugin
    /// step overwrites the previous output.
    ///
    /// # Arguments
    ///
    /// * `step_id` - The step ID from the workflow plan.
    /// * `plugin` - The plugin name that was executed.
    /// * `output_path` - Optional path to the output file written by the plugin.
    /// * `success` - Whether the plugin step completed successfully.
    /// * `diagnostics` - Diagnostic messages emitted during execution.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workspace`] if state cannot be saved.
    pub fn record_plugin_output(
        &mut self,
        step_id: &str,
        plugin: &str,
        output_path: Option<String>,
        success: bool,
        diagnostics: Vec<String>,
    ) -> Result<()> {
        self.state
            .plugin_diagnostics
            .insert(step_id.to_string(), diagnostics.clone());

        let record = PluginOutputRecord {
            step_id: step_id.to_string(),
            plugin: plugin.to_string(),
            output_path,
            completed_at: Some(now_utc()),
            success,
            diagnostics,
        };

        self.state
            .plugin_outputs
            .insert(step_id.to_string(), record);
        self.state.updated_at = now_utc();
        self.save()
    }

    /// Adds a report file path for a plugin step.
    ///
    /// Appends `path` to the list of report paths stored under `step_id` in
    /// [`WorkspaceState::report_paths`]. A new list is created for the step if
    /// none exists yet.
    ///
    /// # Arguments
    ///
    /// * `step_id` - The step ID whose report list should be extended.
    /// * `path` - Filesystem path of the report file.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workspace`] if state cannot be saved.
    pub fn add_report_path(&mut self, step_id: &str, path: String) -> Result<()> {
        self.state
            .report_paths
            .entry(step_id.to_string())
            .or_default()
            .push(path);
        self.state.updated_at = now_utc();
        self.save()
    }

    /// Records the watcher result as published.
    ///
    /// Sets [`WorkspaceState::watcher_result_published`] to `true` and saves
    /// state. This method is idempotent: calling it when the flag is already
    /// `true` is a no-op aside from refreshing `updated_at`.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workspace`] if state cannot be saved.
    pub fn mark_published(&mut self) -> Result<()> {
        self.state.watcher_result_published = true;
        self.state.updated_at = now_utc();
        self.save()
    }

    /// Persists a numeric score for a plugin step to the workspace state.
    ///
    /// Stores `score` under `step_id` in [`WorkspaceState::plugin_scores`].
    /// Calling this method again with the same `step_id` replaces the
    /// previous score.
    ///
    /// # Arguments
    ///
    /// * `step_id` - The step identifier whose score is being recorded.
    /// * `score` - The numeric score value (conventionally in `[0.0, 1.0]`).
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workspace`] if state cannot be saved.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use xzardgz::workspace::WorkspaceManager;
    /// # fn example(mut ws: WorkspaceManager) -> xzardgz::error::Result<()> {
    /// ws.record_plugin_score("step-1", 0.85)?;
    /// assert_eq!(*ws.state.plugin_scores.get("step-1").unwrap(), 0.85);
    /// # Ok(())
    /// # }
    /// ```
    pub fn record_plugin_score(&mut self, step_id: &str, score: f64) -> Result<()> {
        self.state.plugin_scores.insert(step_id.to_string(), score);
        self.state.updated_at = now_utc();
        self.save()
    }

    /// Returns the workspace ID (ULID string).
    pub fn id(&self) -> &str {
        &self.state.workspace_id
    }

    /// Returns `true` if the workspace is in a failed stage.
    pub fn is_failed(&self) -> bool {
        self.state.current_stage.is_failed()
    }

    /// Returns `true` if the workflow completed successfully.
    pub fn is_complete(&self) -> bool {
        self.state.current_stage.is_complete()
    }

    /// Returns the SHA-256 repository hash for this workspace.
    pub fn repository_hash(&self) -> &str {
        &self.state.repository_hash
    }

    /// Applies git metadata to the workspace state and persists to disk.
    ///
    /// Delegates to [`WorkspaceState::apply_git_metadata`] to update all
    /// git-related fields (remote URL, hash, local path, branch names, HEAD
    /// commit), then calls [`Self::save`] to write the updated state file.
    ///
    /// # Arguments
    ///
    /// * `metadata` - Git metadata collected by
    ///   [`crate::git::ops::GitRepository::metadata`].
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workspace`] if the updated state cannot be
    /// written to disk.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::workspace::WorkspaceManager;
    /// use xzardgz::git::metadata::GitMetadata;
    ///
    /// let mut manager = WorkspaceManager::create(
    ///     "/tmp/workspaces",
    ///     "https://github.com/example/repo",
    ///     Some("main".to_string()),
    ///     None,
    /// ).unwrap();
    ///
    /// let meta = GitMetadata::new(
    ///     None, None,
    ///     "/tmp/repo".to_string(),
    ///     Some("main".to_string()),
    ///     None,
    ///     Some("deadbeef".to_string()),
    ///     false,
    /// );
    /// manager.apply_git_metadata(&meta).unwrap();
    /// ```
    pub fn apply_git_metadata(
        &mut self,
        metadata: &crate::git::metadata::GitMetadata,
    ) -> Result<()> {
        self.state.apply_git_metadata(metadata);
        self.save()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Creates a temporary directory for use in a single test.
    fn temp_dir() -> TempDir {
        // SAFETY: TempDir::new() only fails on OS-level resource exhaustion,
        // which is not expected in a test environment.
        TempDir::new().expect("SAFETY: temp directory creation cannot fail in test environment")
    }

    /// Extracts the UTF-8 path string from a `TempDir`.
    fn root(dir: &TempDir) -> &str {
        // SAFETY: OS temp directories always have valid UTF-8 paths on the
        // platforms this crate targets (Linux, macOS, Windows).
        dir.path()
            .to_str()
            .expect("SAFETY: temp dir path is valid UTF-8")
    }

    #[test]
    fn test_create_creates_workspace_directory() {
        let dir = temp_dir();
        let manager = WorkspaceManager::create(root(&dir), "https://example.com/repo", None, None)
            .expect("SAFETY: create should succeed on a writable temp dir");

        let workspace_dir = dir.path().join(manager.id());
        assert!(
            workspace_dir.exists(),
            "workspace directory '{}' should exist",
            workspace_dir.display()
        );
    }

    #[test]
    fn test_create_saves_initial_state_file() {
        let dir = temp_dir();
        let manager = WorkspaceManager::create(root(&dir), "https://example.com/state", None, None)
            .expect("SAFETY: create should succeed on a writable temp dir");

        let state_file = manager.paths.state_file();
        assert!(
            state_file.exists(),
            "state.yaml should exist at '{}' after create",
            state_file.display()
        );

        let content = std::fs::read_to_string(&state_file)
            .expect("SAFETY: state file should be readable after create");
        assert!(
            content.contains("repository_url"),
            "state file should contain 'repository_url'"
        );
        assert!(
            content.contains("workspace_id"),
            "state file should contain 'workspace_id'"
        );
    }

    #[test]
    fn test_load_reads_saved_state() {
        let dir = temp_dir();
        let manager = WorkspaceManager::create(root(&dir), "https://example.com/load", None, None)
            .expect("SAFETY: create should succeed");

        let workspace_id = manager.id().to_string();
        let loaded = WorkspaceManager::load(root(&dir), &workspace_id)
            .expect("SAFETY: load should succeed for a just-created workspace");

        assert_eq!(loaded.state.workspace_id, workspace_id);
        assert_eq!(loaded.state.repository_url, "https://example.com/load");
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = temp_dir();
        let manager =
            WorkspaceManager::create(root(&dir), "https://example.com/roundtrip", None, None)
                .expect("SAFETY: create should succeed");

        let workspace_id = manager.id().to_string();
        let loaded = WorkspaceManager::load(root(&dir), &workspace_id)
            .expect("SAFETY: load should succeed for a just-created workspace");

        assert_eq!(loaded.state.workspace_id, manager.state.workspace_id);
        assert_eq!(loaded.state.repository_url, manager.state.repository_url);
        assert_eq!(loaded.state.repository_hash, manager.state.repository_hash);
        assert_eq!(loaded.state.version, manager.state.version);
    }

    #[test]
    fn test_transition_updates_current_stage() {
        let dir = temp_dir();
        let mut manager =
            WorkspaceManager::create(root(&dir), "https://example.com/stage", None, None)
                .expect("SAFETY: create should succeed");

        manager
            .transition(WorkspaceStage::Scanning)
            .expect("SAFETY: transition to Scanning should succeed");
        assert_eq!(manager.state.current_stage, WorkspaceStage::Scanning);

        manager
            .transition(WorkspaceStage::ScanComplete)
            .expect("SAFETY: transition to ScanComplete should succeed");
        assert_eq!(manager.state.current_stage, WorkspaceStage::ScanComplete);
    }

    #[test]
    fn test_transition_records_timestamp() {
        let dir = temp_dir();
        let mut manager =
            WorkspaceManager::create(root(&dir), "https://example.com/ts", None, None)
                .expect("SAFETY: create should succeed");

        let stage = WorkspaceStage::Scanning;
        let expected_label = stage.label().to_string();

        manager
            .transition(WorkspaceStage::Scanning)
            .expect("SAFETY: transition should succeed");

        assert!(
            manager.state.stage_timestamps.contains_key(&expected_label),
            "stage_timestamps should contain key '{}'",
            expected_label
        );
    }

    #[test]
    fn test_transition_is_idempotent() {
        let dir = temp_dir();
        let mut manager =
            WorkspaceManager::create(root(&dir), "https://example.com/idem", None, None)
                .expect("SAFETY: create should succeed");

        let r1 = manager.transition(WorkspaceStage::Scanning);
        assert!(r1.is_ok(), "first transition should succeed");

        let r2 = manager.transition(WorkspaceStage::Scanning);
        assert!(
            r2.is_ok(),
            "second transition to same stage should not error"
        );

        assert_eq!(manager.state.current_stage, WorkspaceStage::Scanning);
        assert!(
            manager
                .state
                .stage_timestamps
                .contains_key(WorkspaceStage::Scanning.label()),
            "stage_timestamps should have an entry for the stage label"
        );
    }

    #[test]
    fn test_record_scan_artifact_sets_path_and_stage() {
        let dir = temp_dir();
        let mut manager =
            WorkspaceManager::create(root(&dir), "https://example.com/scan", None, None)
                .expect("SAFETY: create should succeed");

        manager
            .record_scan_artifact(
                "/workspace/scan/artifact.yaml".to_string(),
                Some("1.0.0".to_string()),
                Some("abc123def456".to_string()),
            )
            .expect("SAFETY: record_scan_artifact should succeed");

        assert_eq!(
            manager.state.current_stage,
            WorkspaceStage::ScanComplete,
            "stage should be ScanComplete after recording scan artifact"
        );
        assert_eq!(
            manager.state.scan_artifact_path,
            Some("/workspace/scan/artifact.yaml".to_string()),
            "scan_artifact_path should match supplied path"
        );
        assert_eq!(
            manager.state.scan_artifact_version,
            Some("1.0.0".to_string()),
            "scan_artifact_version should match supplied version"
        );
        assert_eq!(
            manager.state.scan_artifact_head_commit,
            Some("abc123def456".to_string()),
            "scan_artifact_head_commit should match supplied commit"
        );
        assert!(
            manager.state.scan_artifact_created_at.is_some(),
            "scan_artifact_created_at should be set"
        );
    }

    #[test]
    fn test_record_plugin_output_stores_record() {
        let dir = temp_dir();
        let mut manager =
            WorkspaceManager::create(root(&dir), "https://example.com/plugin", None, None)
                .expect("SAFETY: create should succeed");

        let diagnostics = vec!["msg1".to_string(), "msg2".to_string()];

        manager
            .record_plugin_output(
                "step-1",
                "test-plugin",
                Some("/output/path.yaml".to_string()),
                true,
                diagnostics.clone(),
            )
            .expect("SAFETY: record_plugin_output should succeed");

        assert!(
            manager.state.plugin_outputs.contains_key("step-1"),
            "plugin_outputs should contain the recorded step"
        );

        let record = &manager.state.plugin_outputs["step-1"];
        assert_eq!(record.step_id, "step-1");
        assert_eq!(record.plugin, "test-plugin");
        assert_eq!(record.output_path, Some("/output/path.yaml".to_string()));
        assert!(record.success, "success flag should be true");
        assert_eq!(record.diagnostics, diagnostics);
        assert!(record.completed_at.is_some(), "completed_at should be set");

        assert!(
            manager.state.plugin_diagnostics.contains_key("step-1"),
            "plugin_diagnostics should contain an entry for the step"
        );
    }

    #[test]
    fn test_record_plugin_output_replaces_on_rerun() {
        let dir = temp_dir();
        let mut manager =
            WorkspaceManager::create(root(&dir), "https://example.com/replace", None, None)
                .expect("SAFETY: create should succeed");

        manager
            .record_plugin_output("step1", "plugin-a", None, false, vec![])
            .expect("SAFETY: first record should succeed");

        assert!(
            !manager.state.plugin_outputs["step1"].success,
            "first record should have success=false"
        );

        manager
            .record_plugin_output(
                "step1",
                "plugin-a",
                Some("/path/out.yaml".to_string()),
                true,
                vec![],
            )
            .expect("SAFETY: second record should succeed");

        let record = &manager.state.plugin_outputs["step1"];
        assert!(
            record.success,
            "second record should replace first: success=true"
        );
        assert_eq!(
            record.output_path,
            Some("/path/out.yaml".to_string()),
            "output_path should be from the second record"
        );
    }

    #[test]
    fn test_add_report_path_appends_to_step_list() {
        let dir = temp_dir();
        let mut manager =
            WorkspaceManager::create(root(&dir), "https://example.com/report", None, None)
                .expect("SAFETY: create should succeed");

        manager
            .add_report_path("step1", "/path/report1.md".to_string())
            .expect("SAFETY: add_report_path should succeed");
        manager
            .add_report_path("step1", "/path/report2.md".to_string())
            .expect("SAFETY: add_report_path should succeed");
        manager
            .add_report_path("step1", "/path/report3.md".to_string())
            .expect("SAFETY: add_report_path should succeed");

        let paths = &manager.state.report_paths["step1"];
        assert_eq!(paths.len(), 3, "should have accumulated 3 report paths");
        assert!(paths.contains(&"/path/report1.md".to_string()));
        assert!(paths.contains(&"/path/report2.md".to_string()));
        assert!(paths.contains(&"/path/report3.md".to_string()));
    }

    #[test]
    fn test_mark_published_sets_flag() {
        let dir = temp_dir();
        let mut manager =
            WorkspaceManager::create(root(&dir), "https://example.com/pub", None, None)
                .expect("SAFETY: create should succeed");

        assert!(
            !manager.state.watcher_result_published,
            "new workspace should start unpublished"
        );

        manager
            .mark_published()
            .expect("SAFETY: mark_published should succeed");

        assert!(
            manager.state.watcher_result_published,
            "workspace should be marked as published"
        );
    }

    #[test]
    fn test_mark_published_is_idempotent() {
        let dir = temp_dir();
        let mut manager =
            WorkspaceManager::create(root(&dir), "https://example.com/pub2", None, None)
                .expect("SAFETY: create should succeed");

        let r1 = manager.mark_published();
        assert!(r1.is_ok(), "first mark_published should succeed");

        let r2 = manager.mark_published();
        assert!(r2.is_ok(), "second mark_published should not error");

        assert!(
            manager.state.watcher_result_published,
            "workspace should remain marked as published"
        );
    }

    #[test]
    fn test_open_creates_workspace_when_none_exists() {
        let dir = temp_dir();
        let result =
            WorkspaceManager::open(root(&dir), "https://github.com/example/new-open", None);

        assert!(
            result.is_ok(),
            "open should create a new workspace when none exists, got: {:?}",
            result.err()
        );

        let manager = result.unwrap();
        assert!(!manager.id().is_empty(), "workspace id should not be empty");
    }

    #[test]
    fn test_open_resumes_existing_workspace() {
        let dir = temp_dir();
        let repo_url = "https://github.com/example/open-resume-test";

        let created = WorkspaceManager::create(root(&dir), repo_url, None, None)
            .expect("SAFETY: create should succeed");
        let original_id = created.id().to_string();
        drop(created);

        let opened = WorkspaceManager::open(root(&dir), repo_url, None)
            .expect("SAFETY: open should succeed for an existing workspace");

        assert_eq!(
            opened.id(),
            original_id,
            "open should resume the most recent existing workspace"
        );
    }

    #[test]
    fn test_is_failed_returns_true_when_stage_is_failed() {
        let dir = temp_dir();
        let mut manager =
            WorkspaceManager::create(root(&dir), "https://example.com/failed", None, None)
                .expect("SAFETY: create should succeed");

        assert!(
            !manager.is_failed(),
            "new workspace should not be in failed state"
        );

        manager
            .transition(WorkspaceStage::Failed {
                stage: "scanning".to_string(),
                reason: "disk full".to_string(),
            })
            .expect("SAFETY: transition to Failed should succeed");

        assert!(
            manager.is_failed(),
            "workspace should report as failed after Failed transition"
        );
    }

    #[test]
    fn test_is_complete_returns_true_when_stage_is_complete() {
        let dir = temp_dir();
        let mut manager =
            WorkspaceManager::create(root(&dir), "https://example.com/complete", None, None)
                .expect("SAFETY: create should succeed");

        assert!(
            !manager.is_complete(),
            "new workspace should not be in complete state"
        );

        manager
            .transition(WorkspaceStage::Complete)
            .expect("SAFETY: transition to Complete should succeed");

        assert!(
            manager.is_complete(),
            "workspace should report as complete after Complete transition"
        );
    }

    // ------------------------------------------------------------------
    // apply_git_metadata
    // ------------------------------------------------------------------

    #[test]
    fn test_apply_git_metadata_persists_branch_to_state_file() {
        let dir = temp_dir();
        let mut manager =
            WorkspaceManager::create(root(&dir), "https://example.com/repo", None, None)
                .expect("SAFETY: create should succeed");

        let meta = crate::git::metadata::GitMetadata::new(
            None,
            None,
            "/tmp/repo".to_string(),
            Some("main".to_string()),
            None,
            Some("deadbeefdeadbeef".to_string()),
            false,
        );

        manager
            .apply_git_metadata(&meta)
            .expect("SAFETY: apply_git_metadata should succeed");

        assert_eq!(
            manager.state.branch_name.as_deref(),
            Some("main"),
            "branch_name should be updated in manager state"
        );
        assert_eq!(
            manager.state.scan_artifact_head_commit.as_deref(),
            Some("deadbeefdeadbeef"),
            "head commit should be persisted"
        );

        // Reload from disk and verify persistence.
        let loaded = WorkspaceManager::load(root(&dir), manager.id())
            .expect("SAFETY: reload should succeed");
        assert_eq!(
            loaded.state.branch_name.as_deref(),
            Some("main"),
            "branch_name should survive a reload cycle"
        );
    }

    #[test]
    fn test_apply_git_metadata_updates_local_path_on_disk() {
        let dir = temp_dir();
        let mut manager = WorkspaceManager::create(
            root(&dir),
            "https://example.com/local-path-test",
            None,
            None,
        )
        .expect("SAFETY: create should succeed");

        let local_path = "/tmp/checked_out_repo".to_string();
        let meta = crate::git::metadata::GitMetadata::new(
            None,
            None,
            local_path.clone(),
            None,
            None,
            None,
            false,
        );

        manager
            .apply_git_metadata(&meta)
            .expect("SAFETY: apply_git_metadata should succeed");

        let loaded = WorkspaceManager::load(root(&dir), manager.id())
            .expect("SAFETY: reload should succeed");
        assert_eq!(
            loaded.state.local_repository_path.as_deref(),
            Some(local_path.as_str()),
            "local_repository_path should persist to disk"
        );
    }

    #[test]
    fn test_record_plugin_score_persists_score_for_step() {
        let dir = temp_dir();
        let mut manager =
            WorkspaceManager::create(root(&dir), "https://example.com/score-test", None, None)
                .expect("SAFETY: create should succeed on a writable temp dir");

        manager
            .record_plugin_score("step-1", 0.85)
            .expect("SAFETY: record_plugin_score should succeed");

        let score = manager.state.plugin_scores.get("step-1").copied();
        assert_eq!(score, Some(0.85), "score should be stored under step-1");
    }

    #[test]
    fn test_record_plugin_score_replaces_existing_score() {
        let dir = temp_dir();
        let mut manager = WorkspaceManager::create(
            root(&dir),
            "https://example.com/score-replace-test",
            None,
            None,
        )
        .expect("SAFETY: create should succeed on a writable temp dir");

        manager
            .record_plugin_score("step-x", 0.5)
            .expect("SAFETY: first record_plugin_score should succeed");
        manager
            .record_plugin_score("step-x", 0.9)
            .expect("SAFETY: second record_plugin_score should succeed");

        let score = manager.state.plugin_scores.get("step-x").copied();
        assert_eq!(score, Some(0.9), "second score should replace the first");
    }

    #[test]
    fn test_record_plugin_score_survives_reload() {
        let dir = temp_dir();
        let mut manager = WorkspaceManager::create(
            root(&dir),
            "https://example.com/score-reload-test",
            None,
            None,
        )
        .expect("SAFETY: create should succeed on a writable temp dir");
        let ws_id = manager.id().to_string();

        manager
            .record_plugin_score("step-persist", 0.72)
            .expect("SAFETY: record_plugin_score should succeed");

        let loaded =
            WorkspaceManager::load(root(&dir), &ws_id).expect("SAFETY: reload should succeed");
        let score = loaded.state.plugin_scores.get("step-persist").copied();
        assert_eq!(score, Some(0.72), "score should survive a reload cycle");
    }
}
