//! Integration tests for the workspace module.
//!
//! All tests use `tempfile::TempDir` for isolation and verify the
//! on-disk behaviour of [`WorkspaceManager`] end-to-end.

use std::collections::HashSet;

use tempfile::TempDir;
use xzardgz::workspace::id::{hash_repository, new_workspace_id, now_rfc3339};
use xzardgz::workspace::stage::WorkspaceStage;
use xzardgz::workspace::state::WorkspaceState;
use xzardgz::workspace::{WorkspaceManager, WorkspacePaths};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Test 1 -- create produces valid state.yaml
// ---------------------------------------------------------------------------

/// A freshly created workspace writes a valid, parseable `state.yaml` with
/// all required fields populated correctly.
#[test]
fn test_workspace_create_produces_valid_state_file() {
    let dir = temp_dir();
    let repo_url = "https://github.com/example/valid-state";

    let manager = WorkspaceManager::create(root(&dir), repo_url, Some("main".to_string()), None)
        .expect("SAFETY: create should succeed on a writable temp dir");

    let state_file = manager.paths.state_file();
    assert!(
        state_file.exists(),
        "state.yaml should exist at '{}'",
        state_file.display()
    );

    let raw = std::fs::read_to_string(&state_file).expect("SAFETY: state file should be readable");

    let loaded = WorkspaceState::load_from_str(&raw)
        .expect("SAFETY: state.yaml written by create should be valid");

    assert_eq!(
        loaded.workspace_id, manager.state.workspace_id,
        "workspace_id should match"
    );
    assert_eq!(
        loaded.repository_url, repo_url,
        "repository_url should match"
    );
    assert_eq!(
        loaded.target_branch,
        Some("main".to_string()),
        "target_branch should be preserved"
    );
    assert_eq!(
        loaded.current_stage,
        WorkspaceStage::Initializing,
        "initial stage should be Initializing"
    );
    assert_eq!(loaded.version, "1", "schema version should be '1'");
}

// ---------------------------------------------------------------------------
// Test 2 -- load by ID returns correct state
// ---------------------------------------------------------------------------

/// Loading a workspace by its ULID returns the same state that was created.
#[test]
fn test_workspace_load_by_id_returns_correct_state() {
    let dir = temp_dir();
    let repo_url = "https://github.com/example/load-by-id";

    let created = WorkspaceManager::create(root(&dir), repo_url, None, None)
        .expect("SAFETY: create should succeed");

    let workspace_id = created.id().to_string();

    let loaded = WorkspaceManager::load(root(&dir), &workspace_id)
        .expect("SAFETY: load should succeed for a just-created workspace");

    assert_eq!(
        loaded.state.workspace_id, workspace_id,
        "workspace_id should round-trip"
    );
    assert_eq!(
        loaded.state.repository_url, repo_url,
        "repository_url should round-trip"
    );
    assert_eq!(
        loaded.state.repository_hash, created.state.repository_hash,
        "repository_hash should round-trip"
    );
}

// ---------------------------------------------------------------------------
// Test 3 -- open creates new workspace when none matches
// ---------------------------------------------------------------------------

/// `open` creates a brand-new workspace when no existing workspace in the
/// root has a matching repository hash.
#[test]
fn test_workspace_open_creates_new_when_no_match() {
    let dir = temp_dir();
    let repo_url = "https://github.com/example/brand-new-open";

    let result = WorkspaceManager::open(root(&dir), repo_url, None);

    assert!(
        result.is_ok(),
        "open should create a new workspace when none exists, got: {:?}",
        result.err()
    );

    let manager = result.unwrap();
    assert!(!manager.id().is_empty(), "workspace id must not be empty");
    assert_eq!(manager.state.repository_url, repo_url);
}

// ---------------------------------------------------------------------------
// Test 4 -- open resumes most recent matching workspace
// ---------------------------------------------------------------------------

/// When an existing workspace with a matching repository hash exists, `open`
/// resumes it rather than creating a new one.
#[test]
fn test_workspace_open_resumes_most_recent_matching_workspace() {
    let dir = temp_dir();
    let repo_url = "https://github.com/example/resume-me";

    let created = WorkspaceManager::create(root(&dir), repo_url, None, None)
        .expect("SAFETY: create should succeed");
    let original_id = created.id().to_string();
    drop(created);

    let opened =
        WorkspaceManager::open(root(&dir), repo_url, None).expect("SAFETY: open should succeed");

    assert_eq!(
        opened.id(),
        original_id,
        "open should return the existing workspace, not create a new one"
    );
}

// ---------------------------------------------------------------------------
// Test 5 -- transition to Scanning
// ---------------------------------------------------------------------------

/// Transitioning to `Scanning` updates `current_stage` and persists to disk.
#[test]
fn test_workspace_transition_to_scanning() {
    let dir = temp_dir();
    let mut manager =
        WorkspaceManager::create(root(&dir), "https://example.com/scanning", None, None)
            .expect("SAFETY: create should succeed");

    manager
        .transition(WorkspaceStage::Scanning)
        .expect("SAFETY: transition to Scanning should succeed");

    assert_eq!(
        manager.state.current_stage,
        WorkspaceStage::Scanning,
        "current_stage should be Scanning"
    );

    // Verify persistence.
    let id = manager.id().to_string();
    let reloaded = WorkspaceManager::load(root(&dir), &id).expect("SAFETY: load should succeed");
    assert_eq!(
        reloaded.state.current_stage,
        WorkspaceStage::Scanning,
        "persisted stage should be Scanning"
    );
}

// ---------------------------------------------------------------------------
// Test 6 -- transition to Failed preserves plugin outputs
// ---------------------------------------------------------------------------

/// Transitioning to `Failed` does not wipe any previously recorded plugin
/// output records.
#[test]
fn test_workspace_transition_to_failed_preserves_plugin_outputs() {
    let dir = temp_dir();
    let mut manager =
        WorkspaceManager::create(root(&dir), "https://example.com/fail-preserve", None, None)
            .expect("SAFETY: create should succeed");

    let id = manager.id().to_string();

    manager
        .record_plugin_output("step1", "test-plugin", None, true, vec![])
        .expect("SAFETY: record_plugin_output should succeed");

    manager
        .transition(WorkspaceStage::Failed {
            stage: "plugin_running".to_string(),
            reason: "test-induced failure".to_string(),
        })
        .expect("SAFETY: transition to Failed should succeed");

    let reloaded = WorkspaceManager::load(root(&dir), &id).expect("SAFETY: load should succeed");

    assert!(
        reloaded.state.plugin_outputs.contains_key("step1"),
        "plugin outputs must survive a transition to Failed"
    );
}

// ---------------------------------------------------------------------------
// Test 7 -- record_scan_artifact sets ScanComplete stage
// ---------------------------------------------------------------------------

/// `record_scan_artifact` sets all scan fields and transitions to
/// `ScanComplete`.
#[test]
fn test_workspace_record_scan_artifact_sets_scan_complete_stage() {
    let dir = temp_dir();
    let mut manager =
        WorkspaceManager::create(root(&dir), "https://example.com/scan-complete", None, None)
            .expect("SAFETY: create should succeed");

    let artifact_path = "/ws/scan/artifact.yaml".to_string();

    manager
        .record_scan_artifact(
            artifact_path.clone(),
            Some("1.2.3".to_string()),
            Some("deadbeef".to_string()),
        )
        .expect("SAFETY: record_scan_artifact should succeed");

    assert_eq!(
        manager.state.current_stage,
        WorkspaceStage::ScanComplete,
        "stage should be ScanComplete"
    );
    assert_eq!(
        manager.state.scan_artifact_path,
        Some(artifact_path),
        "scan_artifact_path should be set"
    );
    assert!(
        manager.state.scan_artifact_created_at.is_some(),
        "scan_artifact_created_at should be set"
    );
}

// ---------------------------------------------------------------------------
// Test 8 -- record_plugin_output can be replaced
// ---------------------------------------------------------------------------

/// Recording the same step twice with different values results in the latest
/// record being stored (replace semantics).
#[test]
fn test_workspace_record_plugin_output_can_be_replaced() {
    let dir = temp_dir();
    let mut manager =
        WorkspaceManager::create(root(&dir), "https://example.com/replace-output", None, None)
            .expect("SAFETY: create should succeed");

    manager
        .record_plugin_output("step1", "plugin-x", None, false, vec!["warn1".to_string()])
        .expect("SAFETY: first record should succeed");

    assert!(
        !manager.state.plugin_outputs["step1"].success,
        "first record: success should be false"
    );

    manager
        .record_plugin_output(
            "step1",
            "plugin-x",
            Some("/new/output.yaml".to_string()),
            true,
            vec![],
        )
        .expect("SAFETY: second record should succeed");

    let record = &manager.state.plugin_outputs["step1"];
    assert!(record.success, "second record should replace: success=true");
    assert_eq!(
        record.output_path,
        Some("/new/output.yaml".to_string()),
        "output_path should reflect the second record"
    );
}

// ---------------------------------------------------------------------------
// Test 9 -- add_report_path accumulates
// ---------------------------------------------------------------------------

/// Adding three report paths for the same step produces a list with all three.
#[test]
fn test_workspace_add_report_path_accumulates() {
    let dir = temp_dir();
    let mut manager =
        WorkspaceManager::create(root(&dir), "https://example.com/accumulate", None, None)
            .expect("SAFETY: create should succeed");

    for i in 1..=3 {
        manager
            .add_report_path("step1", format!("/reports/step1/report{}.md", i))
            .expect("SAFETY: add_report_path should succeed");
    }

    let paths = manager
        .state
        .report_paths
        .get("step1")
        .expect("SAFETY: step1 should have report paths");

    assert_eq!(
        paths.len(),
        3,
        "should have exactly 3 accumulated report paths"
    );
    assert!(paths.contains(&"/reports/step1/report1.md".to_string()));
    assert!(paths.contains(&"/reports/step1/report2.md".to_string()));
    assert!(paths.contains(&"/reports/step1/report3.md".to_string()));
}

// ---------------------------------------------------------------------------
// Test 10 -- mark_published is idempotent
// ---------------------------------------------------------------------------

/// Calling `mark_published` twice does not return an error and the flag
/// remains `true`.
#[test]
fn test_workspace_mark_published_is_idempotent() {
    let dir = temp_dir();
    let mut manager =
        WorkspaceManager::create(root(&dir), "https://example.com/pub-idem", None, None)
            .expect("SAFETY: create should succeed");

    let r1 = manager.mark_published();
    assert!(r1.is_ok(), "first mark_published should succeed");

    let r2 = manager.mark_published();
    assert!(r2.is_ok(), "second mark_published should not error");

    assert!(
        manager.state.watcher_result_published,
        "watcher_result_published should be true after idempotent calls"
    );
}

// ---------------------------------------------------------------------------
// Test 11 -- state serializes RFC 3339 timestamps
// ---------------------------------------------------------------------------

/// The `created_at` timestamp round-trips correctly as an RFC 3339 string
/// through YAML serialization and deserialization.
#[test]
fn test_workspace_state_serializes_rfc3339_timestamps() {
    let dir = temp_dir();
    let manager =
        WorkspaceManager::create(root(&dir), "https://example.com/timestamps", None, None)
            .expect("SAFETY: create should succeed");

    // SAFETY: serialization of a valid WorkspaceState cannot fail.
    let yaml = manager
        .state
        .to_yaml()
        .expect("SAFETY: to_yaml cannot fail");

    // SAFETY: deserializing just-serialized state cannot fail.
    let loaded = WorkspaceState::load_from_str(&yaml).expect("SAFETY: round-trip cannot fail");

    assert_eq!(
        loaded.created_at.to_rfc3339(),
        manager.state.created_at.to_rfc3339(),
        "created_at should round-trip exactly as RFC 3339"
    );

    // Verify now_rfc3339() itself also produces a well-formed RFC 3339 string.
    let current_time = now_rfc3339();
    assert!(
        current_time.contains('T'),
        "now_rfc3339 should produce a string with RFC 3339 'T' separator, got: {}",
        current_time
    );
}

// ---------------------------------------------------------------------------
// Test 12 -- failed stage preserves diagnostics
// ---------------------------------------------------------------------------

/// Diagnostics recorded by a plugin step survive a transition to `Failed` and
/// a subsequent reload from disk.
#[test]
fn test_workspace_failed_stage_preserves_diagnostics() {
    let dir = temp_dir();
    let mut manager =
        WorkspaceManager::create(root(&dir), "https://example.com/diag-survive", None, None)
            .expect("SAFETY: create should succeed");

    let id = manager.id().to_string();

    manager
        .record_plugin_output(
            "step1",
            "test-plugin",
            None,
            false,
            vec![
                "error: something broke".to_string(),
                "hint: try again".to_string(),
            ],
        )
        .expect("SAFETY: record_plugin_output should succeed");

    manager
        .transition(WorkspaceStage::Failed {
            stage: "plugin_running".to_string(),
            reason: "plugin exited with non-zero status".to_string(),
        })
        .expect("SAFETY: transition to Failed should succeed");

    let reloaded = WorkspaceManager::load(root(&dir), &id).expect("SAFETY: load should succeed");

    assert!(
        reloaded.state.plugin_diagnostics.contains_key("step1"),
        "plugin_diagnostics for step1 should survive Failed transition and reload"
    );

    let diags = &reloaded.state.plugin_diagnostics["step1"];
    assert_eq!(diags.len(), 2, "should have exactly 2 diagnostic messages");
    assert_eq!(diags[0], "error: something broke");
    assert_eq!(diags[1], "hint: try again");
}

// ---------------------------------------------------------------------------
// Test 13 -- ID generation creates unique IDs
// ---------------------------------------------------------------------------

/// Generating 10 workspace IDs in sequence produces 10 distinct values.
#[test]
fn test_workspace_id_generation_creates_unique_ids() {
    let ids: HashSet<String> = (0..10).map(|_| new_workspace_id()).collect();
    assert_eq!(
        ids.len(),
        10,
        "all 10 generated workspace IDs should be unique"
    );
}

// ---------------------------------------------------------------------------
// Test 14 -- repository hash is stable across create and reload
// ---------------------------------------------------------------------------

/// The `repository_hash` computed at creation time is identical to the hash
/// computed independently from the same URL.
#[test]
fn test_workspace_repository_hash_is_stable_across_create_and_reload() {
    let dir = temp_dir();
    let repo_url = "https://github.com/example/stable-hash";

    let manager = WorkspaceManager::create(root(&dir), repo_url, None, None)
        .expect("SAFETY: create should succeed");

    let expected_hash = hash_repository(repo_url);
    assert_eq!(
        manager.state.repository_hash, expected_hash,
        "repository_hash at creation should equal hash_repository(url)"
    );

    let id = manager.id().to_string();
    let reloaded = WorkspaceManager::load(root(&dir), &id).expect("SAFETY: load should succeed");

    assert_eq!(
        reloaded.state.repository_hash, expected_hash,
        "repository_hash should be stable after reload from disk"
    );
}

// ---------------------------------------------------------------------------
// Test 15 -- WorkspacePaths::create_all is idempotent
// ---------------------------------------------------------------------------

/// Calling `create_all` on an already-created workspace does not return an
/// error.
#[test]
fn test_workspace_paths_create_all_is_idempotent() {
    let dir = temp_dir();
    let manager =
        WorkspaceManager::create(root(&dir), "https://example.com/paths-idem", None, None)
            .expect("SAFETY: create should succeed");

    // Access the paths field explicitly via the WorkspacePaths type.
    let paths: &WorkspacePaths = &manager.paths;

    let result = paths.create_all();
    assert!(
        result.is_ok(),
        "first extra create_all should be idempotent, got: {:?}",
        result.err()
    );

    let result2 = manager.paths.create_all();
    assert!(
        result2.is_ok(),
        "second extra create_all should also be idempotent, got: {:?}",
        result2.err()
    );
}
