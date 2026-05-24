//! Workspace directory layout and artifact path helpers.
//!
//! This module provides [`WorkspacePaths`], which encapsulates the
//! deterministic directory layout for a single workspace.  All paths can be
//! reconstructed from the workspace root and workspace ID without reading the
//! state file.

use crate::error::{PipelineError, Result};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// WorkspacePaths
// ---------------------------------------------------------------------------

/// Manages the directory layout and artifact paths for a single workspace.
///
/// All paths are derived deterministically from the workspace root and ID, so
/// they can be reconstructed at any time without loading the state file.
///
/// # Directory layout
///
/// ```text
/// <root>/<workspace_id>/
/// ├── state.yaml
/// ├── repo/
/// ├── scan/
/// │   └── artifact.yaml
/// ├── plugins/
/// │   └── <step_id>/
/// │       └── output.json
/// ├── reports/
/// │   └── <step_id>/
/// ├── transcripts/
/// │   └── <step_id>.jsonl
/// ├── diagnostics/
/// │   └── diagnostics.yaml
/// └── watcher/
///     ├── task.json
///     └── result.json
/// ```
pub struct WorkspacePaths {
    /// Root directory for this workspace (i.e., `<workspace_root>/<workspace_id>`).
    pub root: PathBuf,
}

impl WorkspacePaths {
    /// Creates a `WorkspacePaths` rooted at `<workspace_root>/<workspace_id>`.
    ///
    /// # Arguments
    ///
    /// * `workspace_root` - Parent directory that holds all workspace subdirectories.
    /// * `workspace_id` - Unique identifier for this workspace (typically a ULID).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// use std::path::Path;
    ///
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "01JXYZ");
    /// assert_eq!(paths.root, Path::new("/tmp/workspaces/01JXYZ"));
    /// ```
    pub fn new(workspace_root: impl AsRef<std::path::Path>, workspace_id: &str) -> Self {
        Self {
            root: workspace_root.as_ref().join(workspace_id),
        }
    }

    /// Path to the workspace state file.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// assert_eq!(paths.state_file().file_name().unwrap(), "state.yaml");
    /// ```
    pub fn state_file(&self) -> PathBuf {
        self.root.join("state.yaml")
    }

    /// Path to the repository checkout directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// assert_eq!(paths.repo_dir().file_name().unwrap(), "repo");
    /// ```
    pub fn repo_dir(&self) -> PathBuf {
        self.root.join("repo")
    }

    /// Path to the scan artifact directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// assert_eq!(paths.scan_dir().file_name().unwrap(), "scan");
    /// ```
    pub fn scan_dir(&self) -> PathBuf {
        self.root.join("scan")
    }

    /// Path to the scan artifact YAML file.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// assert_eq!(paths.scan_artifact().file_name().unwrap(), "artifact.yaml");
    /// ```
    pub fn scan_artifact(&self) -> PathBuf {
        self.scan_dir().join("artifact.yaml")
    }

    /// Path to the plugin intermediate data directory for a step.
    ///
    /// # Arguments
    ///
    /// * `step_id` - The step identifier used to namespace plugin data.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// let dir = paths.plugin_dir("my-step");
    /// assert_eq!(dir.file_name().unwrap(), "my-step");
    /// ```
    pub fn plugin_dir(&self, step_id: &str) -> PathBuf {
        self.root.join("plugins").join(step_id)
    }

    /// Path to the plugin output JSON file for a step.
    ///
    /// # Arguments
    ///
    /// * `step_id` - The step identifier used to namespace plugin data.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// assert_eq!(paths.plugin_output("my-step").file_name().unwrap(), "output.json");
    /// ```
    pub fn plugin_output(&self, step_id: &str) -> PathBuf {
        self.plugin_dir(step_id).join("output.json")
    }

    /// Path to the reports root directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// assert_eq!(paths.reports_dir().file_name().unwrap(), "reports");
    /// ```
    pub fn reports_dir(&self) -> PathBuf {
        self.root.join("reports")
    }

    /// Path to the per-step reports directory.
    ///
    /// # Arguments
    ///
    /// * `step_id` - The step identifier used to namespace report output.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// let dir = paths.step_reports_dir("my-step");
    /// assert_eq!(dir.file_name().unwrap(), "my-step");
    /// ```
    pub fn step_reports_dir(&self, step_id: &str) -> PathBuf {
        self.reports_dir().join(step_id)
    }

    /// Path to the transcripts directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// assert_eq!(paths.transcripts_dir().file_name().unwrap(), "transcripts");
    /// ```
    pub fn transcripts_dir(&self) -> PathBuf {
        self.root.join("transcripts")
    }

    /// Path to the transcript file for a step.
    ///
    /// The file name is `<step_id>.jsonl`.
    ///
    /// # Arguments
    ///
    /// * `step_id` - The step identifier used to namespace transcript data.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// let transcript = paths.step_transcript("my-step");
    /// assert_eq!(transcript.file_name().unwrap(), "my-step.jsonl");
    /// ```
    pub fn step_transcript(&self, step_id: &str) -> PathBuf {
        self.transcripts_dir().join(format!("{}.jsonl", step_id))
    }

    /// Path to the diagnostics directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// assert_eq!(paths.diagnostics_dir().file_name().unwrap(), "diagnostics");
    /// ```
    pub fn diagnostics_dir(&self) -> PathBuf {
        self.root.join("diagnostics")
    }

    /// Path to the diagnostics YAML file.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// assert_eq!(paths.diagnostics_file().file_name().unwrap(), "diagnostics.yaml");
    /// ```
    pub fn diagnostics_file(&self) -> PathBuf {
        self.diagnostics_dir().join("diagnostics.yaml")
    }

    /// Path to the watcher snapshot directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// assert_eq!(paths.watcher_dir().file_name().unwrap(), "watcher");
    /// ```
    pub fn watcher_dir(&self) -> PathBuf {
        self.root.join("watcher")
    }

    /// Path to the watcher task snapshot JSON file.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// assert_eq!(paths.watcher_task_snapshot().file_name().unwrap(), "task.json");
    /// ```
    pub fn watcher_task_snapshot(&self) -> PathBuf {
        self.watcher_dir().join("task.json")
    }

    /// Path to the watcher result snapshot JSON file.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::workspace::paths::WorkspacePaths;
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// assert_eq!(paths.watcher_result_snapshot().file_name().unwrap(), "result.json");
    /// ```
    pub fn watcher_result_snapshot(&self) -> PathBuf {
        self.watcher_dir().join("result.json")
    }

    /// Creates all workspace subdirectories.
    ///
    /// Uses `std::fs::create_dir_all` for each subdirectory so this is
    /// idempotent - calling it on an existing workspace is safe.
    ///
    /// # Errors
    ///
    /// Returns `PipelineError::Workspace` if any directory cannot be created.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::workspace::paths::WorkspacePaths;
    ///
    /// let paths = WorkspacePaths::new("/tmp/workspaces", "ID1");
    /// paths.create_all().expect("workspace directories must be created");
    /// ```
    pub fn create_all(&self) -> Result<()> {
        let dirs = [
            self.root.clone(),
            self.repo_dir(),
            self.scan_dir(),
            self.root.join("plugins"),
            self.reports_dir(),
            self.transcripts_dir(),
            self.diagnostics_dir(),
            self.watcher_dir(),
        ];
        for dir in &dirs {
            std::fs::create_dir_all(dir).map_err(|e| PipelineError::Workspace(e.to_string()))?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_correct_root_path() {
        let paths = WorkspacePaths::new("/some/root", "MY_ID");
        assert_eq!(paths.root, std::path::Path::new("/some/root/MY_ID"));
    }

    #[test]
    fn test_state_file_is_in_root() {
        let paths = WorkspacePaths::new("/some/root", "ID1");
        assert_eq!(paths.state_file(), paths.root.join("state.yaml"));
    }

    #[test]
    fn test_repo_dir_is_in_root() {
        let paths = WorkspacePaths::new("/some/root", "ID1");
        assert_eq!(paths.repo_dir(), paths.root.join("repo"));
    }

    #[test]
    fn test_scan_artifact_is_in_scan_dir() {
        let paths = WorkspacePaths::new("/some/root", "ID1");
        assert_eq!(
            paths.scan_artifact(),
            paths.scan_dir().join("artifact.yaml")
        );
    }

    #[test]
    fn test_plugin_output_includes_step_id() {
        let paths = WorkspacePaths::new("/some/root", "ID1");
        let output = paths.plugin_output("step-abc");
        assert!(
            output.to_string_lossy().contains("step-abc"),
            "plugin output path must contain the step_id"
        );
        assert_eq!(output.file_name().unwrap(), "output.json");
    }

    #[test]
    fn test_step_transcript_has_jsonl_extension() {
        let paths = WorkspacePaths::new("/some/root", "ID1");
        let transcript = paths.step_transcript("my-step");
        let name = transcript.file_name().unwrap().to_string_lossy();
        assert_eq!(name.as_ref(), "my-step.jsonl");
    }

    #[test]
    fn test_diagnostics_file_is_in_diagnostics_dir() {
        let paths = WorkspacePaths::new("/some/root", "ID1");
        assert_eq!(
            paths.diagnostics_file(),
            paths.diagnostics_dir().join("diagnostics.yaml")
        );
    }

    #[test]
    fn test_watcher_task_snapshot_is_in_watcher_dir() {
        let paths = WorkspacePaths::new("/some/root", "ID1");
        assert_eq!(
            paths.watcher_task_snapshot(),
            paths.watcher_dir().join("task.json")
        );
    }

    #[test]
    fn test_create_all_creates_workspace_directories() {
        let tmp = tempfile::TempDir::new().expect("must create temp dir");
        let paths = WorkspacePaths::new(tmp.path(), "TEST_ID");
        paths.create_all().expect("create_all must succeed");

        assert!(paths.root.is_dir(), "root must exist");
        assert!(paths.repo_dir().is_dir(), "repo must exist");
        assert!(paths.scan_dir().is_dir(), "scan must exist");
        assert!(paths.root.join("plugins").is_dir(), "plugins must exist");
        assert!(paths.reports_dir().is_dir(), "reports must exist");
        assert!(paths.transcripts_dir().is_dir(), "transcripts must exist");
        assert!(paths.diagnostics_dir().is_dir(), "diagnostics must exist");
        assert!(paths.watcher_dir().is_dir(), "watcher must exist");
    }

    #[test]
    fn test_create_all_is_idempotent() {
        let tmp = tempfile::TempDir::new().expect("must create temp dir");
        let paths = WorkspacePaths::new(tmp.path(), "TEST_IDEMPOTENT");

        // Calling create_all twice must not return an error.
        paths.create_all().expect("first call must succeed");
        paths
            .create_all()
            .expect("second call must also succeed (idempotent)");

        assert!(
            paths.root.is_dir(),
            "root must still exist after second call"
        );
    }
}
