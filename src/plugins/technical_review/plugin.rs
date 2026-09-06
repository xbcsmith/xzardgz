//! `TechnicalReviewPlugin` implementation.
//!
//! This module contains [`TechnicalReviewPlugin`], the built-in plugin that
//! performs a multi-dimensional technical review of a repository by combining
//! static scan data from [`ScanResult`] with AI-assisted analysis from the
//! configured [`Provider`] via a multi-turn [`AgentSession`].
//!
//! # Execution Flow
//!
//! 1. Validate the [`TechnicalReviewConfig`] from the pipeline config.
//! 2. Check whether the plugin is enabled; return early if not.
//! 3. Use [`FilePrioritizer`] to select the most relevant files.
//! 4. Determine active [`ReviewDimension`]s from `focus_areas`.
//! 5. Build a system prompt and a user prompt from scan metadata.
//! 6. Build an [`AgentSession`] via [`PluginContext::build_agent_session`];
//!    fail immediately with [`PipelineError::Provider`] if the provider does
//!    not support tool calling.
//! 7. Run the agent session with the user prompt.
//! 8. Parse the JSON response into [`TechnicalReviewFinding`]s.
//! 9. Filter by severity threshold and confidence threshold.
//! 10. Cap findings at `max_findings`.
//! 11. Compute the overall [`RiskBand`] from the findings.
//! 12. Write `technical_review.md` and `technical_review.json` reports.
//! 13. Return a [`PluginOutput`] with report paths and risk band.

use std::path::PathBuf;

use async_trait::async_trait;
use ulid::Ulid;

use crate::diagnostics::{Diagnostic, DiagnosticCategory};
use crate::error::Result;
use crate::plugins::context::{PluginContext, ToolAccessLevel};
use crate::plugins::output::PluginOutput;
use crate::plugins::trait_def::{PluginMetadata, WorkflowPlugin};
use crate::reports::findings::{PluginFinding, PluginFindings};
use crate::scanner::scoring::{ConfidenceScorer, ScoringConfig};

use super::config::validate_technical_review_config;
use super::dimensions::ReviewDimension;
use super::finding::TechnicalReviewFinding;
use super::prioritizer::FilePrioritizer;
use super::report::{TechnicalReviewJsonReport, TechnicalReviewMarkdownReport};

// ---------------------------------------------------------------------------
// Default prompts
// ---------------------------------------------------------------------------

/// Default system prompt for the technical review plugin.
const DEFAULT_SYSTEM_PROMPT: &str = "\
You are a senior software architect performing a technical review of a codebase. \
Analyze the provided repository information and return your findings as a JSON object. \
The JSON object must have exactly one top-level key: \"findings\", whose value is an array. \
Each element of the findings array must be a JSON object with these fields: \
\"category\" (string - the review dimension), \
\"severity\" (string - one of: info, low, medium, high, critical), \
\"file\" (string or null - repository-relative file path), \
\"line\" (number or null - 1-based line number), \
\"symbol\" (string or null - function, struct, or module name), \
\"evidence\" (string - what you observed), \
\"impact\" (string - why it matters), \
\"recommendation\" (string - concrete steps to address this), \
\"confidence\" (number - your confidence in [0.0, 1.0]), \
\"related_files\" (array of strings - other affected files), \
\"references\" (array of strings - external documentation links). \
Return ONLY the JSON object. Do not include any other text, markdown, or explanation.";

// ---------------------------------------------------------------------------
// TechnicalReviewPlugin
// ---------------------------------------------------------------------------

/// Built-in technical review plugin.
///
/// Evaluates a codebase across 14 review dimensions by combining static
/// repository scan data with AI-assisted analysis. Produces human-readable
/// Markdown and machine-readable JSON reports.
///
/// # Examples
///
/// ```
/// use xzardgz::plugins::technical_review::plugin::TechnicalReviewPlugin;
/// use xzardgz::plugins::trait_def::WorkflowPlugin;
///
/// let plugin = TechnicalReviewPlugin;
/// assert_eq!(plugin.name(), "technical-review");
/// assert_eq!(plugin.supported_formats(), vec!["markdown", "json"]);
/// ```
pub struct TechnicalReviewPlugin;

#[async_trait]
impl WorkflowPlugin for TechnicalReviewPlugin {
    /// Returns `"technical-review"`.
    fn name(&self) -> &str {
        "technical-review"
    }

    /// Returns metadata for the technical review plugin.
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            "technical-review",
            "1.0.0",
            "Evaluates codebase architecture, quality, and operational readiness \
             across 14 review dimensions.",
        )
    }

    /// Returns `["markdown", "json"]`.
    fn supported_formats(&self) -> Vec<String> {
        vec!["markdown".to_string(), "json".to_string()]
    }

    /// Returns [`ToolAccessLevel::ReadOnly`].
    fn required_tool_access(&self) -> ToolAccessLevel {
        ToolAccessLevel::ReadOnly
    }

    /// Runs the technical review analysis.
    ///
    /// See [module-level documentation][self] for the detailed execution flow.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Plugin`] for unrecoverable configuration
    /// failures.  Provider failures are surfaced as
    /// [`PluginOutput::failure`] rather than hard errors so the pipeline can
    /// continue.
    async fn run(&self, mut ctx: PluginContext) -> Result<PluginOutput> {
        let config = ctx.config.technical_review.clone();

        // Step 1: Validate configuration.
        if let Err(e) = validate_technical_review_config(&config) {
            return Ok(PluginOutput::failure(format!(
                "technical-review: config validation failed: {e}"
            )));
        }

        // Step 2: Check enabled flag.
        if !config.enabled {
            return Ok(PluginOutput::success("technical review disabled"));
        }

        // Step 3: Prioritize files from the scan result.
        let prioritized_files = FilePrioritizer::capped_flat_list(
            &ctx.scan_result,
            config.max_files,
            config.include_tests,
            config.include_docs,
        );

        if prioritized_files.is_empty() {
            ctx.add_diagnostic(Diagnostic::warning(
                DiagnosticCategory::Plugin,
                "technical-review: no files selected for analysis; the scan result may be empty",
            ));
        }

        // Step 4: Determine active review dimensions.
        let dimensions = ReviewDimension::from_focus_areas(&config.focus_areas);

        // Step 5: Build prompts.
        let system_prompt = ctx
            .prompts
            .get("technical_review_system")
            .cloned()
            .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string());

        let user_prompt = build_user_prompt(&ctx.scan_result, &prioritized_files, &dimensions);

        // Step 6: Build the agent session and run the multi-turn analysis.
        // Returns a hard error when the provider does not support tool calling.
        let session =
            ctx.build_agent_session(system_prompt, 8192, config.agent_max_turns as usize)?;
        let response_content = match session.run(&user_prompt).await {
            Ok(content) => content,
            Err(e) => {
                return Ok(PluginOutput::failure(format!(
                    "technical-review: agent session failed: {e}"
                )));
            }
        };

        // Step 7: Parse findings from the response.
        let parsed_findings = parse_ai_response(&response_content, &dimensions);

        // Step 8: Score each finding and filter by blended confidence.
        let scoring_cfg = ScoringConfig {
            ai_confidence_weight: config.ai_confidence_weight,
            ai_analysis_enabled: config.ai_analysis_enabled,
            review_violations: false,
        };
        let scorer = ConfidenceScorer::new(scoring_cfg);

        let mut scored: Vec<(
            TechnicalReviewFinding,
            crate::scanner::scoring::ScoringResult,
        )> = parsed_findings
            .into_iter()
            .map(|f| {
                let ai_score = Some(f.confidence);
                let result = scorer.score(&f, ai_score);
                (f, result)
            })
            .collect();

        // AbsoluteViolation findings bypass the threshold; all others must meet it.
        scored.retain(|(_, result)| {
            result.has_violations() || result.blended_score >= config.confidence_threshold
        });

        // Step 9 (originally "Filter by confidence threshold"):
        // Filter by severity threshold.
        let min_severity = TechnicalReviewFinding::parse_severity(&config.severity_threshold);
        scored.retain(|(f, _)| f.severity >= min_severity);

        // Step 10: Cap findings at max_findings.
        if config.max_findings > 0 && scored.len() > config.max_findings as usize {
            scored.truncate(config.max_findings as usize);
        }

        // Step 10: Compute risk band.
        let risk_band = if scored.is_empty() {
            None
        } else {
            let mut plugin_findings = PluginFindings::new();
            for (f, result) in &scored {
                plugin_findings.push(f.to_plugin_finding().with_scoring(result));
            }
            plugin_findings.to_risk_band()
        };

        // Build pre-scored PluginFinding list for output and JSON report.
        let plugin_findings_vec: Vec<PluginFinding> = scored
            .iter()
            .map(|(f, result)| f.to_plugin_finding().with_scoring(result))
            .collect();

        // Raw findings for Markdown report.
        let raw_findings: Vec<TechnicalReviewFinding> =
            scored.iter().map(|(f, _)| f.clone()).collect();

        // Step 11: Write reports.
        let workspace_id = ctx.workspace_id().to_string();
        let report_id = Ulid::new().to_string();

        let reports_dir = build_reports_dir(&ctx);

        let md_path = reports_dir.join("technical_review.md");
        let json_path = reports_dir.join("technical_review.json");

        if let Err(e) = TechnicalReviewMarkdownReport::write(
            &raw_findings,
            &ctx.scan_result,
            &workspace_id,
            risk_band,
            &md_path,
        ) {
            ctx.add_diagnostic(Diagnostic::warning(
                DiagnosticCategory::Plugin,
                format!("technical-review: failed to write markdown report: {e}"),
            ));
        }

        if let Err(e) = TechnicalReviewJsonReport::write(
            &plugin_findings_vec,
            &ctx.scan_result,
            &workspace_id,
            &report_id,
            risk_band,
            &json_path,
        ) {
            ctx.add_diagnostic(Diagnostic::warning(
                DiagnosticCategory::Plugin,
                format!("technical-review: failed to write json report: {e}"),
            ));
        }

        // Step 12: Build and return PluginOutput.
        let summary = format!(
            "technical-review complete: {} finding(s), risk band: {}",
            scored.len(),
            risk_band.map(|b| b.as_str()).unwrap_or("none"),
        );

        let mut output = PluginOutput::success(summary);
        output.risk_band = risk_band;

        for pf in &plugin_findings_vec {
            output.add_finding(pf.clone());
        }

        output.add_report_path("markdown", md_path.to_string_lossy().to_string());
        output.add_report_path("json", json_path.to_string_lossy().to_string());
        output.add_written_file(md_path.to_string_lossy().to_string());
        output.add_written_file(json_path.to_string_lossy().to_string());

        output.set_score("finding_count", scored.len() as f64);

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Computes the reports output directory from the plugin context.
///
/// Prefers `config.reports.output_dir` when non-empty, otherwise falls back
/// to `<workspace_root>/<workspace_id>/reports`.
fn build_reports_dir(ctx: &PluginContext) -> PathBuf {
    if !ctx.config.reports.output_dir.is_empty() {
        PathBuf::from(&ctx.config.reports.output_dir)
    } else {
        ctx.workspace.paths.reports_dir()
    }
}

/// Builds the user-facing prompt from scan metadata, prioritized files, and dimensions.
fn build_user_prompt(
    scan_result: &crate::scanner::result::ScanResult,
    prioritized_files: &[String],
    dimensions: &[ReviewDimension],
) -> String {
    let mut prompt = String::new();

    if let Some(ref name) = scan_result.repository_name {
        prompt.push_str(&format!("Repository: {}\n", name));
    }
    if let Some(ref lang) = scan_result.primary_language {
        prompt.push_str(&format!("Primary language: {}\n", lang));
    }
    if !scan_result.frameworks.is_empty() {
        prompt.push_str(&format!(
            "Frameworks: {}\n",
            scan_result.frameworks.join(", ")
        ));
    }

    prompt.push('\n');

    if prioritized_files.is_empty() {
        prompt.push_str("Key files for review: (none identified)\n");
    } else {
        prompt.push_str("Key files for review:\n");
        for file in prioritized_files {
            prompt.push_str(&format!("- {}\n", file));
        }
    }

    prompt.push('\n');

    let dim_names: Vec<&str> = dimensions.iter().map(|d| d.as_str()).collect();
    if dim_names.is_empty() {
        prompt.push_str("Focus areas: all\n");
    } else {
        prompt.push_str(&format!("Focus areas: {}\n", dim_names.join(", ")));
    }

    prompt.push('\n');
    prompt.push_str("Provide your technical review findings.\n");

    prompt
}

/// Attempts to parse [`TechnicalReviewFinding`]s from an AI response string.
///
/// Expects the response to contain a JSON object with a `"findings"` array.
/// Returns an empty vector on any parse failure.
fn parse_ai_response(
    content: &str,
    _dimensions: &[ReviewDimension],
) -> Vec<TechnicalReviewFinding> {
    if content.is_empty() {
        return Vec::new();
    }

    // Extract a JSON object from the content. Some providers wrap JSON in
    // markdown code fences; strip them if present.
    let json_str = extract_json(content);

    let value: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let findings_array = match value.get("findings").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return Vec::new(),
    };

    findings_array
        .iter()
        .filter_map(TechnicalReviewFinding::from_json_value)
        .collect()
}

/// Strips markdown code fences and extracts the embedded JSON string.
///
/// If no code fence is found, returns the original content trimmed.
fn extract_json(content: &str) -> &str {
    let trimmed = content.trim();

    // Handle ```json ... ``` or ``` ... ``` wrapping.
    if let Some(inner) = trimmed.strip_prefix("```json")
        && let Some(end) = inner.rfind("```")
    {
        return inner[..end].trim();
    }
    if let Some(inner) = trimmed.strip_prefix("```")
        && let Some(end) = inner.rfind("```")
    {
        return inner[..end].trim();
    }

    trimmed
}

#[cfg(test)]
/// Filters findings to those at or above the minimum severity threshold.
///
/// Parses the threshold string using [`TechnicalReviewFinding::parse_severity`]
/// as the minimum; findings strictly below it are discarded.
fn filter_by_severity(
    findings: Vec<TechnicalReviewFinding>,
    threshold: &str,
) -> Vec<TechnicalReviewFinding> {
    let min_severity = TechnicalReviewFinding::parse_severity(threshold);
    findings
        .into_iter()
        .filter(|f| f.severity >= min_severity)
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, GovernanceConfig, TechnicalReviewConfig};
    use crate::error::PipelineError;
    use crate::governance::GovernanceChecker;
    use crate::plugins::context::PluginContext;
    use crate::providers::base::MockProvider;
    use crate::providers::types::{
        FunctionCall, Message, ProviderCapabilities, ProviderMetadata, ToolCall,
    };
    use crate::scanner::findings::FindingSeverity;
    use crate::scanner::result::{PluginPreselection, SCAN_RESULT_VERSION, ScanResult};
    use crate::tools::registry::ToolRegistry;
    use crate::workspace::WorkspaceManager;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    // ------------------------------------------------------------------
    // Helpers
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

    fn make_context(
        root: &str,
        scan: ScanResult,
        provider: Arc<dyn crate::providers::base::Provider + Send + Sync>,
    ) -> PluginContext {
        make_context_with_config(root, scan, None, provider)
    }

    /// Builds a [`PluginContext`] with an optional [`TechnicalReviewConfig`] override.
    ///
    /// When `config_override` is `None`, [`TechnicalReviewConfig::default`] is used.
    fn make_context_with_config(
        root: &str,
        scan: ScanResult,
        config_override: Option<TechnicalReviewConfig>,
        provider: Arc<dyn crate::providers::base::Provider + Send + Sync>,
    ) -> PluginContext {
        // SAFETY: WorkspaceManager::create only fails on I/O errors; temp dirs are writable.
        let manager = WorkspaceManager::create(root, "test://repo", None, None).unwrap();
        let state = manager.state.clone();
        let workspace = Arc::new(manager);
        let mut config = Config::default();
        if let Some(tech_cfg) = config_override {
            config.technical_review = tech_cfg;
        }
        let config = Arc::new(config);
        let tool_registry = ToolRegistry::new();
        let governance_cfg = GovernanceConfig {
            enabled: false,
            rules_path: String::new(),
            fail_on_violation: false,
        };
        // SAFETY: from_config with empty rules_path and disabled governance cannot fail.
        let governance = GovernanceChecker::from_config(&governance_cfg).unwrap();
        PluginContext::new(
            config,
            workspace,
            state,
            scan,
            provider,
            tool_registry,
            governance,
        )
    }

    // ------------------------------------------------------------------
    // Static method tests
    // ------------------------------------------------------------------

    #[test]
    fn test_technical_review_plugin_name_returns_technical_review() {
        assert_eq!(TechnicalReviewPlugin.name(), "technical-review");
    }

    #[test]
    fn test_technical_review_plugin_metadata_has_correct_name() {
        let meta = TechnicalReviewPlugin.metadata();
        assert_eq!(meta.name, "technical-review");
    }

    #[test]
    fn test_technical_review_plugin_metadata_has_version() {
        let meta = TechnicalReviewPlugin.metadata();
        assert_eq!(meta.version, "1.0.0");
    }

    #[test]
    fn test_technical_review_plugin_metadata_has_description() {
        let meta = TechnicalReviewPlugin.metadata();
        assert!(!meta.description.is_empty());
    }

    #[test]
    fn test_technical_review_plugin_supported_formats_includes_markdown_and_json() {
        let formats = TechnicalReviewPlugin.supported_formats();
        assert!(formats.contains(&"markdown".to_string()));
        assert!(formats.contains(&"json".to_string()));
    }

    #[test]
    fn test_technical_review_plugin_required_tool_access_is_read_only() {
        assert_eq!(
            TechnicalReviewPlugin.required_tool_access(),
            ToolAccessLevel::ReadOnly
        );
    }

    // ------------------------------------------------------------------
    // parse_ai_response
    // ------------------------------------------------------------------

    #[test]
    fn test_parse_ai_response_with_valid_json_returns_findings() {
        let json = r#"{"findings":[{"category":"architecture","severity":"high","file":null,"line":null,"symbol":null,"evidence":"Monolithic design","impact":"Hard to scale","recommendation":"Decompose services","confidence":0.8,"related_files":[],"references":[]}]}"#;
        let dims = ReviewDimension::all();
        let findings = parse_ai_response(json, &dims);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].category, "architecture");
    }

    #[test]
    fn test_parse_ai_response_with_empty_findings_array_returns_empty() {
        let json = r#"{"findings":[]}"#;
        let findings = parse_ai_response(json, &[]);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_parse_ai_response_with_invalid_json_returns_empty() {
        let findings = parse_ai_response("not json at all", &[]);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_parse_ai_response_with_empty_string_returns_empty() {
        let findings = parse_ai_response("", &[]);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_parse_ai_response_with_no_findings_key_returns_empty() {
        let json = r#"{"results":[]}"#;
        let findings = parse_ai_response(json, &[]);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_parse_ai_response_with_markdown_code_fence_strips_fence() {
        let content = "```json\n{\"findings\":[]}\n```";
        let findings = parse_ai_response(content, &[]);
        assert!(findings.is_empty(), "should parse through code fence");
    }

    #[test]
    fn test_parse_ai_response_skips_malformed_finding_objects() {
        // Second finding is missing required fields; should be skipped.
        let json = r#"{"findings":[
            {"category":"architecture","severity":"high","evidence":"E","impact":"I","recommendation":"R","confidence":0.9,"related_files":[],"references":[]},
            {"category":"bad"}
        ]}"#;
        let findings = parse_ai_response(json, &[]);
        assert_eq!(findings.len(), 1, "only valid findings should be returned");
    }

    // ------------------------------------------------------------------
    // filter_by_severity
    // ------------------------------------------------------------------

    #[test]
    fn test_filter_by_severity_medium_threshold_excludes_info_and_low() {
        let make = |sev| TechnicalReviewFinding::new("arch", sev, "e", "i", "r", 0.8);
        let findings = vec![
            make(FindingSeverity::Info),
            make(FindingSeverity::Low),
            make(FindingSeverity::Medium),
            make(FindingSeverity::High),
            make(FindingSeverity::Critical),
        ];
        let filtered = filter_by_severity(findings, "medium");
        assert_eq!(filtered.len(), 3);
        assert!(
            filtered
                .iter()
                .all(|f| f.severity >= FindingSeverity::Medium)
        );
    }

    #[test]
    fn test_filter_by_severity_info_threshold_includes_all() {
        let make = |sev| TechnicalReviewFinding::new("arch", sev, "e", "i", "r", 0.8);
        let findings = vec![
            make(FindingSeverity::Info),
            make(FindingSeverity::Low),
            make(FindingSeverity::High),
        ];
        let filtered = filter_by_severity(findings, "info");
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn test_filter_by_severity_critical_threshold_excludes_lower() {
        let make = |sev| TechnicalReviewFinding::new("arch", sev, "e", "i", "r", 0.8);
        let findings = vec![
            make(FindingSeverity::Low),
            make(FindingSeverity::High),
            make(FindingSeverity::Critical),
        ];
        let filtered = filter_by_severity(findings, "critical");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].severity, FindingSeverity::Critical);
    }

    // ------------------------------------------------------------------
    // build_user_prompt
    // ------------------------------------------------------------------

    #[test]
    fn test_build_user_prompt_contains_repo_name() {
        let scan = empty_scan();
        let dims = vec![ReviewDimension::Architecture];
        let prompt = build_user_prompt(&scan, &[], &dims);
        assert!(prompt.contains("test-repo"), "must contain repo name");
    }

    #[test]
    fn test_build_user_prompt_contains_primary_language() {
        let scan = empty_scan();
        let dims = vec![ReviewDimension::Architecture];
        let prompt = build_user_prompt(&scan, &[], &dims);
        assert!(prompt.contains("Rust"), "must contain primary language");
    }

    #[test]
    fn test_build_user_prompt_contains_dimension_names() {
        let scan = empty_scan();
        let dims = vec![
            ReviewDimension::Architecture,
            ReviewDimension::ErrorHandling,
        ];
        let prompt = build_user_prompt(&scan, &[], &dims);
        assert!(prompt.contains("architecture"), "must contain architecture");
        assert!(
            prompt.contains("error_handling"),
            "must contain error_handling"
        );
    }

    #[test]
    fn test_build_user_prompt_lists_files() {
        let scan = empty_scan();
        let files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
        let dims = vec![ReviewDimension::Architecture];
        let prompt = build_user_prompt(&scan, &files, &dims);
        assert!(prompt.contains("src/main.rs"), "must list main.rs");
        assert!(prompt.contains("src/lib.rs"), "must list lib.rs");
    }

    #[test]
    fn test_build_user_prompt_empty_files_shows_placeholder() {
        let scan = empty_scan();
        let dims = vec![ReviewDimension::Architecture];
        let prompt = build_user_prompt(&scan, &[], &dims);
        assert!(
            prompt.contains("none identified"),
            "must show placeholder for empty file list"
        );
    }

    // ------------------------------------------------------------------
    // extract_json
    // ------------------------------------------------------------------

    #[test]
    fn test_extract_json_plain_json_returns_trimmed() {
        let input = "  {\"key\": \"value\"}  ";
        assert_eq!(extract_json(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_extract_json_code_fence_json_strips_fence() {
        let input = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_extract_json_bare_code_fence_strips_fence() {
        let input = "```\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json(input), "{\"key\": \"value\"}");
    }

    // ------------------------------------------------------------------
    // Async integration tests (requires tokio)
    // ------------------------------------------------------------------

    /// Returns a MockProvider that advertises tool support and returns a
    /// tool call on the first completion, then `final_json` on the second.
    fn make_tool_calling_provider(final_json: &'static str) -> MockProvider {
        let mut mock = MockProvider::new();
        mock.expect_metadata().returning(|| ProviderMetadata {
            name: "mock-tools".to_string(),
            models: vec![],
            capabilities: ProviderCapabilities {
                streaming: false,
                tools: true,
                vision: false,
            },
        });
        let call_count = Arc::new(Mutex::new(0u32));
        mock.expect_complete().returning(move |_, _| {
            let mut count = call_count.lock().expect("mutex poisoned");
            *count += 1;
            if *count == 1 {
                let mut msg = Message::assistant("");
                msg.tool_calls = Some(vec![ToolCall {
                    id: "tc1".to_string(),
                    function: FunctionCall {
                        name: "read_file".to_string(),
                        arguments: "{\"path\":\"src/main.rs\"}".to_string(),
                    },
                }]);
                Ok(msg)
            } else {
                Ok(Message::assistant(final_json))
            }
        });
        mock
    }

    #[tokio::test]
    async fn test_technical_review_plugin_run_with_mock_provider_empty_findings_returns_success() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mock = make_tool_calling_provider(r#"{"findings":[]}"#);
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let ctx = make_context(tmp.path().to_str().unwrap(), empty_scan(), provider);
        let output = TechnicalReviewPlugin.run(ctx).await.unwrap();
        assert!(output.completed, "plugin must report completed");
    }

    #[tokio::test]
    async fn test_technical_review_plugin_run_with_mock_provider_with_findings_returns_success() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let finding_json = concat!(
            "{\"findings\":[{\"category\":\"architecture\",\"severity\":\"high\",",
            "\"file\":null,\"line\":null,\"symbol\":null,",
            "\"evidence\":\"Monolithic design observed.\",",
            "\"impact\":\"Difficult to maintain and scale.\",",
            "\"recommendation\":\"Consider decomposing into smaller modules.\",",
            "\"confidence\":0.85,\"related_files\":[],\"references\":[]}]}"
        );
        let mock = make_tool_calling_provider(finding_json);
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        // confidence_threshold = 0.0 so the test verifies end-to-end finding
        // processing rather than the threshold filter (tested separately).
        let cfg = TechnicalReviewConfig {
            confidence_threshold: 0.0,
            ..TechnicalReviewConfig::default()
        };
        let ctx = make_context_with_config(
            tmp.path().to_str().unwrap(),
            empty_scan(),
            Some(cfg),
            provider,
        );
        let output = TechnicalReviewPlugin.run(ctx).await.unwrap();
        assert!(output.completed, "plugin must report completed");
        assert_eq!(output.findings.len(), 1, "should have one finding");
    }

    #[tokio::test]
    async fn test_technical_review_plugin_run_provider_error_returns_failure_output() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mut mock = MockProvider::new();
        mock.expect_metadata().returning(|| ProviderMetadata {
            name: "mock-tools".to_string(),
            models: vec![],
            capabilities: ProviderCapabilities {
                streaming: false,
                tools: true,
                vision: false,
            },
        });
        mock.expect_complete()
            .returning(|_, _| Err(PipelineError::Provider("api unavailable".to_string())));
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let ctx = make_context(tmp.path().to_str().unwrap(), empty_scan(), provider);
        // SAFETY: run() returns Ok(PluginOutput::failure) rather than Err on provider errors.
        let output = TechnicalReviewPlugin.run(ctx).await.unwrap();
        assert!(
            !output.completed,
            "plugin must report not completed on error"
        );
    }

    #[tokio::test]
    async fn test_technical_review_plugin_run_no_tool_support_returns_hard_error() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mut mock = MockProvider::new();
        mock.expect_metadata().returning(|| ProviderMetadata {
            name: "mock-no-tools".to_string(),
            models: vec![],
            capabilities: ProviderCapabilities {
                streaming: false,
                tools: false,
                vision: false,
            },
        });
        mock.expect_complete().never();
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let ctx = make_context(tmp.path().to_str().unwrap(), empty_scan(), provider);
        let result = TechnicalReviewPlugin.run(ctx).await;
        assert!(
            result.is_err(),
            "must return a hard error when provider lacks tool support"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("tool calling"),
            "error must mention tool calling, got: {msg}"
        );
    }

    #[tokio::test]
    async fn test_technical_review_plugin_run_exercises_multi_turn_tool_call_round_trip() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let call_count = Arc::new(Mutex::new(0u32));
        let call_count_check = Arc::clone(&call_count);
        let mut mock = MockProvider::new();
        mock.expect_metadata().returning(|| ProviderMetadata {
            name: "mock-tools".to_string(),
            models: vec![],
            capabilities: ProviderCapabilities {
                streaming: false,
                tools: true,
                vision: false,
            },
        });
        mock.expect_complete().returning(move |_, _| {
            let mut count = call_count.lock().expect("mutex poisoned");
            *count += 1;
            if *count == 1 {
                let mut msg = Message::assistant("");
                msg.tool_calls = Some(vec![ToolCall {
                    id: "tc1".to_string(),
                    function: FunctionCall {
                        name: "read_file".to_string(),
                        arguments: "{\"path\":\"src/lib.rs\"}".to_string(),
                    },
                }]);
                Ok(msg)
            } else {
                Ok(Message::assistant("{\"findings\":[]}"))
            }
        });
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let ctx = make_context(tmp.path().to_str().unwrap(), empty_scan(), provider);
        let output = TechnicalReviewPlugin.run(ctx).await.unwrap();
        assert!(
            output.completed,
            "plugin must complete after multi-turn session"
        );
        let final_count = *call_count_check.lock().expect("mutex poisoned");
        assert!(
            final_count >= 2,
            "at least two complete() calls required for tool-call round trip, got {final_count}"
        );
    }

    #[tokio::test]
    async fn test_technical_review_plugin_run_disabled_returns_success_immediately() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mut mock = MockProvider::new();
        // Neither metadata() nor complete() is called when plugin is disabled.
        mock.expect_metadata().never();
        mock.expect_complete().never();
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let manager =
            WorkspaceManager::create(tmp.path().to_str().unwrap(), "test://repo", None, None)
                .unwrap();
        let state = manager.state.clone();
        let workspace = Arc::new(manager);
        let mut config = Config::default();
        config.technical_review.enabled = false;
        let config = Arc::new(config);
        let tool_registry = ToolRegistry::new();
        let gov_cfg = GovernanceConfig {
            enabled: false,
            rules_path: String::new(),
            fail_on_violation: false,
        };
        let governance = GovernanceChecker::from_config(&gov_cfg).unwrap();
        let ctx = PluginContext::new(
            config,
            workspace,
            state,
            empty_scan(),
            provider,
            tool_registry,
            governance,
        );
        let output = TechnicalReviewPlugin.run(ctx).await.unwrap();
        assert!(output.completed, "disabled plugin must return completed");
        assert!(
            output.summary.contains("disabled"),
            "summary must mention disabled"
        );
    }

    #[tokio::test]
    async fn test_technical_review_plugin_run_writes_markdown_report() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mock = make_tool_calling_provider("{\"findings\":[]}");
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let ctx = make_context(tmp.path().to_str().unwrap(), empty_scan(), provider);
        let output = TechnicalReviewPlugin.run(ctx).await.unwrap();
        assert!(
            output
                .report_paths
                .values()
                .flatten()
                .any(|p| p.ends_with("technical_review.md")),
            "output must include markdown report path"
        );
    }

    #[tokio::test]
    async fn test_technical_review_plugin_run_writes_json_report() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mock = make_tool_calling_provider("{\"findings\":[]}");
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let ctx = make_context(tmp.path().to_str().unwrap(), empty_scan(), provider);
        let output = TechnicalReviewPlugin.run(ctx).await.unwrap();
        assert!(
            output
                .report_paths
                .values()
                .flatten()
                .any(|p| p.ends_with("technical_review.json")),
            "output must include json report path"
        );
    }

    // ------------------------------------------------------------------
    // Phase 2 scoring integration tests
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_technical_review_plugin_absolute_violation_finding_always_included() {
        // A critical dependency finding triggers AbsoluteViolation and must
        // be included regardless of the confidence_threshold.
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let finding_json = concat!(
            "{\"findings\":[{\"category\":\"dependency_hygiene\",\"severity\":\"critical\",",
            "\"file\":null,\"line\":null,\"symbol\":null,",
            "\"evidence\":\"Vulnerable dependency with known CVE.\",",
            "\"impact\":\"Remote code execution risk.\",",
            "\"recommendation\":\"Upgrade to a patched version.\",",
            "\"confidence\":0.9,\"related_files\":[],\"references\":[]}]}"
        );
        let mock = make_tool_calling_provider(finding_json);
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        // Set a very high confidence_threshold; the AbsoluteViolation must bypass it.
        let cfg = TechnicalReviewConfig {
            confidence_threshold: 0.99,
            ai_confidence_weight: 0.5,
            ai_analysis_enabled: true,
            ..TechnicalReviewConfig::default()
        };
        let ctx = make_context_with_config(
            tmp.path().to_str().unwrap(),
            empty_scan(),
            Some(cfg),
            provider,
        );
        // SAFETY: run() should not fail.
        let output = TechnicalReviewPlugin.run(ctx).await.unwrap();
        assert!(output.completed, "plugin must complete");
        assert_eq!(
            output.findings.len(),
            1,
            "absolute violation finding must always be included despite high threshold"
        );
        // AbsoluteViolation floors the static score to VIOLATION_FLOOR (0.0).
        assert!(
            output.findings[0].static_score.abs() < 1e-9,
            "AbsoluteViolation must floor static_score to 0.0"
        );
        // Blended = 0.0 * (1 - 0.5) + 0.9 * 0.5 = 0.45.
        assert!(
            (output.findings[0].confidence - 0.45).abs() < 1e-9,
            "blended must equal 0.0 * 0.5 + 0.9 * 0.5 = 0.45"
        );
    }

    #[tokio::test]
    async fn test_technical_review_plugin_ai_weight_shifts_blended_confidence() {
        // Higher ai_confidence_weight must produce a blended score closer to
        // the AI-reported confidence.
        let tmp_low = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let tmp_high = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let finding_json = concat!(
            "{\"findings\":[{\"category\":\"architecture\",\"severity\":\"high\",",
            "\"file\":null,\"line\":null,\"symbol\":null,",
            "\"evidence\":\"Tight coupling between modules.\",",
            "\"impact\":\"Reduced testability and maintainability.\",",
            "\"recommendation\":\"Introduce dependency inversion.\",",
            "\"confidence\":0.95,\"related_files\":[],\"references\":[]}]}"
        );
        let mock1 = make_tool_calling_provider(finding_json);
        let provider1: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock1);
        let cfg_low_weight = TechnicalReviewConfig {
            ai_confidence_weight: 0.1,
            confidence_threshold: 0.0, // include all findings
            ..TechnicalReviewConfig::default()
        };
        let ctx1 = make_context_with_config(
            tmp_low.path().to_str().unwrap(),
            empty_scan(),
            Some(cfg_low_weight),
            provider1,
        );
        // SAFETY: run() should not fail.
        let out_low = TechnicalReviewPlugin.run(ctx1).await.unwrap();

        let mock2 = make_tool_calling_provider(finding_json);
        let provider2: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock2);
        let cfg_high_weight = TechnicalReviewConfig {
            ai_confidence_weight: 0.9,
            confidence_threshold: 0.0, // include all findings
            ..TechnicalReviewConfig::default()
        };
        let ctx2 = make_context_with_config(
            tmp_high.path().to_str().unwrap(),
            empty_scan(),
            Some(cfg_high_weight),
            provider2,
        );
        // SAFETY: run() should not fail.
        let out_high = TechnicalReviewPlugin.run(ctx2).await.unwrap();

        assert_eq!(out_low.findings.len(), 1);
        assert_eq!(out_high.findings.len(), 1);
        // Higher AI weight -> blended score closer to AI confidence (0.95).
        assert!(
            out_high.findings[0].confidence > out_low.findings[0].confidence,
            "higher ai_confidence_weight must produce higher blended confidence"
        );
    }

    #[tokio::test]
    async fn test_technical_review_plugin_ai_disabled_blended_equals_static_score() {
        // When ai_analysis_enabled = false, the output confidence must equal
        // the static score, matching the behaviour of a run with no AI provider.
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let finding_json = concat!(
            "{\"findings\":[{\"category\":\"architecture\",\"severity\":\"medium\",",
            "\"file\":null,\"line\":null,\"symbol\":null,",
            "\"evidence\":\"Code style inconsistency across modules.\",",
            "\"impact\":\"Reduced readability.\",",
            "\"recommendation\":\"Apply consistent formatting.\",",
            "\"confidence\":0.85,\"related_files\":[],\"references\":[]}]}"
        );
        let mock = make_tool_calling_provider(finding_json);
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let cfg = TechnicalReviewConfig {
            ai_analysis_enabled: false,
            confidence_threshold: 0.0, // include all findings
            ..TechnicalReviewConfig::default()
        };
        let ctx = make_context_with_config(
            tmp.path().to_str().unwrap(),
            empty_scan(),
            Some(cfg),
            provider,
        );
        // SAFETY: run() should not fail.
        let output = TechnicalReviewPlugin.run(ctx).await.unwrap();
        assert!(output.completed, "plugin must complete");
        assert_eq!(output.findings.len(), 1);
        // ai_score must not be surfaced when AI analysis is disabled.
        assert!(
            output.findings[0].ai_score.is_none(),
            "ai_score must be None when ai_analysis_enabled = false"
        );
        // Blended score equals static score (no AI input).
        let blended = output.findings[0].confidence;
        let static_s = output.findings[0].static_score;
        assert!(
            (blended - static_s).abs() < 1e-9,
            "blended must equal static when AI is disabled"
        );
    }

    #[tokio::test]
    async fn test_technical_review_plugin_run_invalid_config_returns_failure() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mut mock = MockProvider::new();
        // Neither metadata() nor complete() is called when config is invalid.
        mock.expect_metadata().never();
        mock.expect_complete().never();
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let manager =
            WorkspaceManager::create(tmp.path().to_str().unwrap(), "test://repo", None, None)
                .unwrap();
        let state = manager.state.clone();
        let workspace = Arc::new(manager);
        let mut config = Config::default();
        config.technical_review.max_findings = 0;
        let config = Arc::new(config);
        let tool_registry = ToolRegistry::new();
        let gov_cfg = GovernanceConfig {
            enabled: false,
            rules_path: String::new(),
            fail_on_violation: false,
        };
        let governance = GovernanceChecker::from_config(&gov_cfg).unwrap();
        let ctx = PluginContext::new(
            config,
            workspace,
            state,
            empty_scan(),
            provider,
            tool_registry,
            governance,
        );
        // SAFETY: run() returns Ok(PluginOutput::failure) on config error, not Err.
        let output = TechnicalReviewPlugin.run(ctx).await.unwrap();
        assert!(
            !output.completed,
            "invalid config must produce a failure output"
        );
    }
}
