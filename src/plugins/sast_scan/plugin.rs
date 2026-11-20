//! `SastScanPlugin` implementation.
//!
//! This module contains [`SastScanPlugin`], the built-in plugin that runs the
//! SAST engine against the workspace repository and produces a Markdown report.
//!
//! # Execution Flow
//!
//! 1. Read [`SastScanConfig`] from the pipeline configuration; return early if
//!    `enabled` is `false`.
//! 2. Build a [`ResolveConfig`] from `config.rulesets` and call
//!    [`resolve_rules`] to load the full compiled rule set.
//! 3. Construct a [`SastEngine`] from `config.sast` and load the rules.
//! 4. Determine the scan root from the workspace (`repo_dir()`).
//! 5. Run [`SastEngine::scan`] to obtain the raw [`SastScanReport`].
//! 6. Apply the severity threshold filter and the `max_findings` cap.
//! 7. Build a [`SastScanResult`] and score it with [`ConfidenceScorer`].
//! 8. Apply the confidence threshold filter (violations always bypass it).
//! 9. Render the Markdown report with [`build_sast_markdown_report`].
//! 10. Write the report to `{reports_dir}/sast_scan.md` and populate the
//!     [`PluginOutput`].

use std::collections::HashSet;
use std::path::PathBuf;

use async_trait::async_trait;

use crate::error::Result;
use crate::plugins::context::{PluginContext, ToolAccessLevel};
use crate::plugins::output::PluginOutput;
use crate::plugins::trait_def::{PluginMetadata, WorkflowPlugin};
use crate::scanner::sast::rule::metadata::Severity;
use crate::scanner::sast::rule::resolve::{ResolveConfig, resolve_rules};
use crate::scanner::sast::{SastEngine, SastScanReport};
use crate::scanner::scoring::{ConfidenceScorer, ScoringConfig};

use super::report::build_sast_markdown_report;
use super::scoring::SastScanResult;

// ---------------------------------------------------------------------------
// SastScanPlugin
// ---------------------------------------------------------------------------

/// SAST scan workflow plugin.
///
/// Runs the full SAST engine pipeline and produces a scored Markdown report.
/// No AI provider call is made in Phase 7; the blended score equals the static
/// score unless `ai_analysis_enabled` is explicitly set to `true` in the
/// plugin configuration.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::plugins::sast_scan::SastScanPlugin;
/// use xzardgz::plugins::trait_def::WorkflowPlugin;
///
/// let plugin = SastScanPlugin;
/// assert_eq!(plugin.name(), "sast_scan");
/// ```
pub struct SastScanPlugin;

#[async_trait]
impl WorkflowPlugin for SastScanPlugin {
    /// Returns `"sast_scan"`.
    fn name(&self) -> &str {
        "sast_scan"
    }

    /// Returns metadata for the SAST scan plugin.
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            "sast_scan",
            "1.0.0",
            "Runs the SAST engine against the repository, scores matches with the \
             confidence scorer, and produces Markdown, JSON, and SARIF reports.",
        )
    }

    /// Returns `["markdown", "json", "sarif"]`.
    fn supported_formats(&self) -> Vec<String> {
        vec![
            "markdown".to_string(),
            "json".to_string(),
            "sarif".to_string(),
        ]
    }

    /// Returns [`ToolAccessLevel::ReadOnly`].
    fn required_tool_access(&self) -> ToolAccessLevel {
        ToolAccessLevel::ReadOnly
    }

    /// Runs the SAST scan pipeline.
    ///
    /// See [module-level documentation][self] for the detailed execution flow.
    ///
    /// # Errors
    ///
    /// Returns `Ok(PluginOutput::failure(_))` for all recoverable pipeline
    /// errors (rule resolution, engine construction, scan failure). The only
    /// hard error path is an I/O failure writing the final report to disk.
    async fn run(&self, ctx: PluginContext) -> Result<PluginOutput> {
        // Step 1: Read configuration and check enabled flag.
        let config = ctx.config.sast_scan.clone();
        if !config.enabled {
            return Ok(PluginOutput::success("sast_scan: disabled"));
        }

        // Step 2: Resolve rules from the configured rulesets.
        let resolve_cfg = ResolveConfig {
            cli_rulesets: config.rulesets.clone(),
            cli_ruleset_paths: vec![],
        };
        let resolved = match resolve_rules(&resolve_cfg) {
            Ok(r) => r,
            Err(e) => {
                return Ok(PluginOutput::failure(format!(
                    "sast_scan: rule resolution failed: {e}"
                )));
            }
        };

        // Step 3: Extract rules and collect unique ruleset identifiers.
        let mut seen_rulesets: HashSet<String> = HashSet::new();
        let mut ruleset_ids: Vec<String> = Vec::new();
        let rules = resolved
            .into_iter()
            .map(|nr| {
                if !nr.ruleset_id.is_empty() && seen_rulesets.insert(nr.ruleset_id.clone()) {
                    ruleset_ids.push(nr.ruleset_id.clone());
                }
                nr.rule
            })
            .collect::<Vec<_>>();
        ruleset_ids.sort();

        // Step 4: Build and configure the SAST engine.
        let engine_config = ctx.config.sast.clone();
        let mut engine = match SastEngine::new(engine_config) {
            Ok(e) => e,
            Err(e) => {
                return Ok(PluginOutput::failure(format!(
                    "sast_scan: engine initialisation failed: {e}"
                )));
            }
        };

        if let Err(e) = engine.with_rules(rules) {
            return Ok(PluginOutput::failure(format!(
                "sast_scan: failed to load rules: {e}"
            )));
        }

        // Step 5: Determine the scan root and run the engine.
        let root: PathBuf = ctx.workspace.paths.repo_dir();
        let raw_report: SastScanReport = match engine.scan(&root) {
            Ok(r) => r,
            Err(e) => {
                return Ok(PluginOutput::failure(format!(
                    "sast_scan: scan failed: {e}"
                )));
            }
        };

        // Step 6: Apply severity threshold filter and max_findings cap.
        let mut filtered: Vec<_> = raw_report
            .matches
            .into_iter()
            .filter(|m| severity_passes_threshold(&m.severity, &config.severity_threshold))
            .collect();
        filtered.truncate(config.max_findings);

        // Step 7: Build the SastScanResult and score it.
        let scan_result = SastScanResult {
            matches: filtered,
            ruleset_ids,
            scan_root: root,
            duration_ms: raw_report.duration_ms,
        };

        let scorer = ConfidenceScorer::new(ScoringConfig {
            ai_confidence_weight: config.ai_confidence_weight,
            ai_analysis_enabled: config.ai_analysis_enabled,
            review_violations: false,
        });
        // Phase 7: no AI provider call; pass None for the AI score.
        let scoring_result = scorer.score(&scan_result, None);

        // Step 8: Apply the confidence threshold filter.
        // AbsoluteViolation findings bypass the threshold via has_violations().
        if !(scoring_result.has_violations()
            || scoring_result.blended_score >= config.confidence_threshold)
        {
            return Ok(PluginOutput::success(
                "sast_scan: no findings above confidence threshold",
            ));
        }

        // Step 9: Render the Markdown report.
        let markdown = build_sast_markdown_report(&scan_result, &scoring_result);

        // Step 10: Write the report to disk and build the PluginOutput.
        let reports_dir = ctx.workspace.paths.reports_dir();
        if let Err(e) = std::fs::create_dir_all(&reports_dir) {
            return Ok(PluginOutput::failure(format!(
                "sast_scan: failed to create reports directory: {e}"
            )));
        }
        let report_path = reports_dir.join("sast_scan.md");
        if let Err(e) = std::fs::write(&report_path, &markdown) {
            return Ok(PluginOutput::failure(format!(
                "sast_scan: failed to write report: {e}"
            )));
        }

        let match_count = scan_result.matches.len();
        let summary = format!("sast_scan: {} finding(s)", match_count);
        let mut output = PluginOutput::success(summary);
        output.add_written_file(report_path.to_string_lossy().to_string());
        output.add_report_path("markdown", report_path.to_string_lossy().to_string());
        output.set_score("match_count", match_count as f64);
        output.set_score("static_score", scoring_result.static_score);
        output.set_score("blended_score", scoring_result.blended_score);

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns `true` when `severity` meets or exceeds the configured threshold.
///
/// Threshold values are compared by numeric level:
///
/// | Threshold string | Minimum level accepted |
/// |------------------|------------------------|
/// | `"info"`         | Info, Warning, Error   |
/// | `"warning"`      | Warning, Error         |
/// | `"error"`        | Error only             |
///
/// Unknown threshold strings default to `"warning"` behaviour.
///
/// # Arguments
///
/// * `severity`  - Severity of the SAST match to test.
/// * `threshold` - Case-insensitive threshold string from the plugin config.
///
/// # Returns
///
/// `true` if `severity` is at or above the threshold level.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::rule::metadata::Severity;
/// // (The function is private to the plugin module; this is illustrative.)
/// ```
fn severity_passes_threshold(severity: &Severity, threshold: &str) -> bool {
    let threshold_level = match threshold.to_lowercase().as_str() {
        "info" => 0u8,
        "warning" => 1,
        "error" => 2,
        _ => 1,
    };
    let match_level = match severity {
        Severity::Info => 0u8,
        Severity::Warning => 1,
        Severity::Error => 2,
    };
    match_level >= threshold_level
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    use chrono::Utc;

    use crate::config::{Config, GovernanceConfig};
    use crate::governance::GovernanceChecker;
    use crate::plugins::context::PluginContext;
    use crate::providers::base::MockProvider;
    use crate::scanner::result::{PluginPreselection, SCAN_RESULT_VERSION, ScanResult};
    use crate::tools::registry::ToolRegistry;
    use crate::workspace::WorkspaceManager;

    // ------------------------------------------------------------------
    // Static method tests
    // ------------------------------------------------------------------

    #[test]
    fn test_sast_scan_plugin_name_returns_sast_scan() {
        assert_eq!(SastScanPlugin.name(), "sast_scan");
    }

    #[test]
    fn test_sast_scan_plugin_supported_formats_includes_markdown_json_sarif() {
        let formats = SastScanPlugin.supported_formats();
        assert!(formats.contains(&"markdown".to_string()));
        assert!(formats.contains(&"json".to_string()));
        assert!(formats.contains(&"sarif".to_string()));
    }

    #[test]
    fn test_sast_scan_plugin_required_tool_access_is_read_only() {
        assert_eq!(
            SastScanPlugin.required_tool_access(),
            ToolAccessLevel::ReadOnly
        );
    }

    // ------------------------------------------------------------------
    // severity_passes_threshold tests
    // ------------------------------------------------------------------

    #[test]
    fn test_severity_passes_threshold_error_passes_warning_threshold() {
        assert!(severity_passes_threshold(&Severity::Error, "warning"));
    }

    #[test]
    fn test_severity_passes_threshold_info_does_not_pass_warning_threshold() {
        assert!(!severity_passes_threshold(&Severity::Info, "warning"));
    }

    #[test]
    fn test_severity_passes_threshold_info_passes_info_threshold() {
        assert!(severity_passes_threshold(&Severity::Info, "info"));
    }

    #[test]
    fn test_severity_passes_threshold_warning_does_not_pass_error_threshold() {
        assert!(!severity_passes_threshold(&Severity::Warning, "error"));
    }

    #[test]
    fn test_severity_passes_threshold_unknown_string_defaults_to_warning() {
        // "unknown" defaults to warning (level 1); Info (level 0) should fail.
        assert!(!severity_passes_threshold(&Severity::Info, "unknown"));
        // Warning (level 1) should pass.
        assert!(severity_passes_threshold(&Severity::Warning, "unknown"));
    }

    // ------------------------------------------------------------------
    // run() integration tests
    // ------------------------------------------------------------------

    fn empty_scan() -> ScanResult {
        ScanResult {
            version: SCAN_RESULT_VERSION.to_string(),
            repository_url: Some("https://github.com/org/test-repo".to_string()),
            repository_name: Some("test-repo".to_string()),
            head_commit: None,
            scan_timestamp: Utc::now(),
            repository_structure: vec![],
            language_statistics: HashMap::new(),
            primary_language: Some("Rust".to_string()),
            frameworks: vec![],
            documentation_inventory: vec![],
            governance_rules: vec![],
            cli_commands: vec![],
            public_apis: vec![],
            entrypoints: vec![],
            config_surface: vec![],
            key_files: vec![],
            dependency_manifests: vec![],
            test_files: vec![],
            build_files: vec![],
            security_relevant_files: vec![],
            findings: vec![],
            plugin_preselection: PluginPreselection::default(),
        }
    }

    fn make_context(root: &str, config_override: Option<Config>) -> PluginContext {
        // SAFETY: WorkspaceManager::create only fails on I/O errors; temp dirs
        // are always writable.
        let manager = WorkspaceManager::create(root, "test://repo", None, None, None).unwrap();
        let state = manager.state.clone();
        let workspace = Arc::new(manager);

        let config = Arc::new(config_override.unwrap_or_default());

        let tool_registry = ToolRegistry::new();
        let governance_cfg = GovernanceConfig {
            enabled: false,
            rules_path: String::new(),
            fail_on_violation: false,
        };
        // SAFETY: disabled governance with an empty rules_path cannot fail.
        let governance = GovernanceChecker::from_config(&governance_cfg).unwrap();

        // SastScanPlugin does not call any provider methods, so a bare mock
        // with no expectations is sufficient.
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> =
            Arc::new(MockProvider::new());

        PluginContext::new(
            config,
            workspace,
            state,
            empty_scan(),
            provider,
            tool_registry,
            governance,
        )
    }

    #[tokio::test]
    async fn test_sast_scan_plugin_run_disabled_returns_success() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mut config = Config::default();
        config.sast_scan.enabled = false;
        let ctx = make_context(tmp.path().to_str().unwrap(), Some(config));
        let output = SastScanPlugin.run(ctx).await.unwrap();
        assert!(output.completed, "disabled plugin must report completed");
        assert!(
            output.summary.contains("disabled"),
            "summary must mention disabled"
        );
    }

    #[tokio::test]
    async fn test_sast_scan_plugin_run_on_fixture_workspace_produces_markdown_report() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.

        // Create the workspace and write a Rust file containing Md5::new()
        // into the repo subdirectory so the SAST engine has something to scan.
        let manager = WorkspaceManager::create(
            tmp.path().to_str().unwrap(),
            "test://repo",
            None,
            None,
            None,
        )
        .unwrap();
        let repo_dir = manager.paths.repo_dir();
        // SAFETY: temp dir is writable; creation of a subdirectory cannot fail.
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("lib.rs"),
            // Intentionally uses MD5 to trigger the builtin:crypto rule.
            "use md5::Md5;\npub fn digest() { let _h = Md5::new(); }\n",
        )
        .unwrap();

        let state = manager.state.clone();
        let workspace = Arc::new(manager);

        let mut config = Config::default();
        config.sast_scan.enabled = true;
        config.sast_scan.rulesets = vec!["builtin:crypto".to_string()];
        // Disable AI so no provider call is attempted.
        config.sast_scan.ai_analysis_enabled = false;
        let config = Arc::new(config);

        let tool_registry = ToolRegistry::new();
        let governance_cfg = GovernanceConfig {
            enabled: false,
            rules_path: String::new(),
            fail_on_violation: false,
        };
        // SAFETY: disabled governance with empty rules cannot fail.
        let governance = GovernanceChecker::from_config(&governance_cfg).unwrap();
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> =
            Arc::new(MockProvider::new());

        let ctx = PluginContext::new(
            config,
            workspace,
            state,
            empty_scan(),
            provider,
            tool_registry,
            governance,
        );

        let output = SastScanPlugin.run(ctx).await.unwrap();

        assert!(output.completed, "plugin must report completed");
        // The scan should detect at least the MD5 usage rule.
        assert!(
            !output.written_files.is_empty(),
            "plugin must write at least one report file"
        );

        // Read the written report and verify it contains a rule identifier.
        let report_path = &output.written_files[0];
        let content = std::fs::read_to_string(report_path).expect("report file must be readable");
        assert!(
            content.to_lowercase().contains("md5"),
            "report must reference the MD5 rule; got:\n{}",
            &content[..content.len().min(500)]
        );
    }
}
