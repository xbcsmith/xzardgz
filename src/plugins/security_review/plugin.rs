//! `SecurityReviewPlugin` implementation.
//!
//! This module contains [`SecurityReviewPlugin`], the built-in plugin that
//! performs a security review of a repository by combining static scan data
//! from [`ScanResult`] with AI-assisted analysis from the configured
//! [`Provider`] via a multi-turn [`AgentSession`].
//!
//! # Execution Flow
//!
//! 1. Validate the [`SecurityReviewConfig`] from the pipeline config.
//! 2. Check whether the plugin is enabled; return early if not.
//! 3. Use [`SecurityFilePrioritizer`] to select the most relevant files.
//! 4. Determine active [`SecurityCategory`] list from config flags.
//! 5. Build system and user prompts.
//! 6. Build an [`AgentSession`] via [`PluginContext::build_agent_session`];
//!    fail immediately with [`PipelineError::Provider`] if the provider does
//!    not support tool calling.
//! 7. Run the agent session with the user prompt.
//! 8. Parse the JSON response into [`SecurityReviewFinding`]s.
//! 9. Filter by confidence threshold.
//! 10. Filter by severity threshold.
//! 11. Cap findings at `max_findings`.
//! 12. Compute the overall [`RiskBand`].
//! 13. Write `security_review.md`, `security_review.json`, and optionally
//!     `security_review.sarif`.
//! 14. If `fail_on_critical` is set and critical findings are present, set
//!     `output.completed = false` to signal failure.
//! 15. Return a [`PluginOutput`] with report paths and risk band.

use std::path::PathBuf;

use async_trait::async_trait;
use ulid::Ulid;

use crate::diagnostics::{Diagnostic, DiagnosticCategory};
use crate::error::Result;
use crate::plugins::context::{PluginContext, ToolAccessLevel};
use crate::plugins::output::PluginOutput;
use crate::plugins::trait_def::{PluginMetadata, WorkflowPlugin};
use crate::reports::findings::{PluginFinding, PluginFindings};
use crate::scanner::findings::FindingSeverity;
use crate::scanner::scoring::{ConfidenceScorer, ScoringConfig};

use super::config::validate_security_review_config;
use super::finding::SecurityReviewFinding;
use super::report::{
    SecurityReviewJsonReport, SecurityReviewMarkdownReport, SecurityReviewSarifReport,
};
use super::scope::{SecurityCategory, SecurityFilePrioritizer};

// ---------------------------------------------------------------------------
// Default system prompt
// ---------------------------------------------------------------------------

/// Default system prompt for the security review plugin.
const DEFAULT_SYSTEM_PROMPT: &str = "\
You are a senior security engineer performing a security review of a codebase. \
Analyze the provided repository information and return your findings as a JSON object. \
The JSON object must have exactly one top-level key: \"findings\", whose value is an array. \
Each element of the findings array must be a JSON object with these fields: \
\"category\" (string - security category, e.g. secrets, injection, auth, unsafe_rust), \
\"severity\" (string - one of: info, low, medium, high, critical), \
\"file\" (string or null - repository-relative file path), \
\"line\" (number or null - 1-based line number), \
\"symbol\" (string or null - function, struct, or module name), \
\"evidence\" (string - what you observed, DO NOT include raw secret values), \
\"exploitability\" (string - how easily this could be exploited), \
\"impact\" (string - why it matters for security), \
\"remediation\" (string - concrete steps to fix this), \
\"confidence\" (number - your confidence in [0.0, 1.0]), \
\"cwe\" (string or null - CWE identifier e.g. CWE-89), \
\"owasp\" (string or null - OWASP category e.g. A03:2021), \
\"false_positive_notes\" (string or null - guidance for false positive analysis), \
\"sarif_help_uri\" (string or null - URL for further documentation). \
IMPORTANT: Never include raw secret values, API keys, passwords, or tokens in evidence fields. \
Return ONLY the JSON object. Do not include any other text, markdown, or explanation.";

// ---------------------------------------------------------------------------
// SecurityReviewPlugin
// ---------------------------------------------------------------------------

/// Built-in security review plugin.
///
/// Performs AI-assisted security analysis of a repository across up to 19
/// security categories. Produces human-readable Markdown, machine-readable
/// JSON, and SARIF 2.1.0 reports.
///
/// # Examples
///
/// ```
/// use xzardgz::plugins::security_review::plugin::SecurityReviewPlugin;
/// use xzardgz::plugins::trait_def::WorkflowPlugin;
///
/// let plugin = SecurityReviewPlugin;
/// assert_eq!(plugin.name(), "security-review");
/// assert!(plugin.supported_formats().contains(&"sarif".to_string()));
/// ```
pub struct SecurityReviewPlugin;

#[async_trait]
impl WorkflowPlugin for SecurityReviewPlugin {
    /// Returns `"security-review"`.
    fn name(&self) -> &str {
        "security-review"
    }

    /// Returns metadata for the security review plugin.
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            "security-review",
            "1.0.0",
            "Performs AI-assisted security review of a repository, \
             producing Markdown, JSON, and SARIF 2.1.0 reports.",
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

    /// Runs the security review analysis.
    ///
    /// See [module-level documentation][self] for the detailed execution flow.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::PipelineError::Plugin`] for unrecoverable
    /// configuration failures. Provider failures are surfaced as
    /// [`PluginOutput::failure`] rather than hard errors so the pipeline can
    /// continue.
    async fn run(&self, mut ctx: PluginContext) -> Result<PluginOutput> {
        // Step 1: Validate configuration.
        let config = ctx.config.security_review.clone();
        if let Err(e) = validate_security_review_config(&config) {
            return Ok(PluginOutput::failure(format!(
                "security-review: config validation failed: {e}"
            )));
        }

        // Step 2: Check enabled flag.
        if !config.enabled {
            return Ok(PluginOutput::success("security review disabled"));
        }

        // Step 3: Prioritize files from the scan result.
        let prioritized_files = SecurityFilePrioritizer::capped_flat_list(
            &ctx.scan_result,
            config.batch_size,
            config.dependency_scanning,
        );

        if prioritized_files.is_empty() {
            ctx.add_diagnostic(Diagnostic::warning(
                DiagnosticCategory::Plugin,
                "security-review: no files selected for analysis; the scan result may be empty",
            ));
        }

        // Step 4: Determine active security categories.
        let categories = SecurityFilePrioritizer::active_categories(
            config.secret_scanning,
            config.dependency_scanning,
            config.check_unsafe_code,
            config.check_auth,
            config.check_endpoints,
            config.check_command_execution,
            config.check_deserialization,
            config.check_cryptography,
        );

        // Step 5: Build prompts.
        let system_prompt = ctx
            .prompts
            .get("security_review_system")
            .cloned()
            .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string());

        let user_prompt = build_user_prompt(&ctx.scan_result, &prioritized_files, &categories);

        // Step 6: Build the agent session and run the multi-turn analysis.
        // Returns a hard error when the provider does not support tool calling.
        let session =
            ctx.build_agent_session(system_prompt, 8192, config.agent_max_turns as usize)?;
        let response_content = match session.run(&user_prompt).await {
            Ok(content) => content,
            Err(e) => {
                return Ok(PluginOutput::failure(format!(
                    "security-review: agent session failed: {e}"
                )));
            }
        };

        // Step 7: Parse findings from the response.
        let parsed_findings = parse_ai_response(&response_content);

        // Step 8: Score each finding and filter by blended confidence.
        //
        // The AI's self-reported confidence is used as the AI score in the blend.
        // Findings that trigger an AbsoluteViolation signal always pass the
        // threshold regardless of the blended value, because a deterministic
        // rule breach must never be silently dropped.
        let scoring_cfg = ScoringConfig {
            ai_confidence_weight: config.ai_confidence_weight,
            ai_analysis_enabled: config.ai_analysis_enabled,
            review_violations: false,
        };
        let scorer = ConfidenceScorer::new(scoring_cfg);

        let mut scored: Vec<(
            SecurityReviewFinding,
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

        // Step 9: Filter by severity threshold.
        let min_severity = SecurityReviewFinding::parse_severity(&config.severity_threshold);
        scored.retain(|(f, _)| f.severity >= min_severity);

        // Step 10: Cap findings at max_findings.
        if config.max_findings > 0 && scored.len() > config.max_findings as usize {
            scored.truncate(config.max_findings as usize);
        }

        // Step 11: Compute risk band.
        let risk_band = if scored.is_empty() {
            None
        } else {
            let mut plugin_findings = PluginFindings::new();
            for (f, result) in &scored {
                plugin_findings.push(f.to_plugin_finding().with_scoring(result));
            }
            plugin_findings.to_risk_band()
        };

        // Build the pre-scored PluginFinding list for reports and output.
        let plugin_findings_vec: Vec<PluginFinding> = scored
            .iter()
            .map(|(f, result)| f.to_plugin_finding().with_scoring(result))
            .collect();

        // Ref to SecurityReviewFinding slice for Markdown/SARIF reports.
        let raw_findings: Vec<&SecurityReviewFinding> = scored.iter().map(|(f, _)| f).collect();

        // Step 12: Write reports.
        let workspace_id = ctx.workspace_id().to_string();
        let report_id = Ulid::new().to_string();
        let reports_dir = build_reports_dir(&ctx);

        let md_path = reports_dir.join("security_review.md");
        let json_path = reports_dir.join("security_review.json");
        let sarif_path = reports_dir.join("security_review.sarif");

        if let Err(e) = SecurityReviewMarkdownReport::write(
            &raw_findings
                .iter()
                .map(|f| (*f).clone())
                .collect::<Vec<_>>(),
            &ctx.scan_result,
            &workspace_id,
            risk_band,
            &md_path,
        ) {
            ctx.add_diagnostic(Diagnostic::warning(
                DiagnosticCategory::Plugin,
                format!("security-review: failed to write markdown report: {e}"),
            ));
        }

        if let Err(e) = SecurityReviewJsonReport::write(
            &plugin_findings_vec,
            &ctx.scan_result,
            &workspace_id,
            &report_id,
            risk_band,
            &json_path,
        ) {
            ctx.add_diagnostic(Diagnostic::warning(
                DiagnosticCategory::Plugin,
                format!("security-review: failed to write json report: {e}"),
            ));
        }

        if config.include_sarif
            && let Err(e) = SecurityReviewSarifReport::write(
                &raw_findings
                    .iter()
                    .map(|f| (*f).clone())
                    .collect::<Vec<_>>(),
                &workspace_id,
                &sarif_path,
            )
        {
            ctx.add_diagnostic(Diagnostic::warning(
                DiagnosticCategory::Plugin,
                format!("security-review: failed to write sarif report: {e}"),
            ));
        }

        // Step 13: Check fail_on_critical.
        let has_critical = scored
            .iter()
            .any(|(f, _)| f.severity == FindingSeverity::Critical);

        let summary = format!(
            "security-review complete: {} finding(s), risk band: {}",
            scored.len(),
            risk_band.map(|b| b.as_str()).unwrap_or("none"),
        );

        let mut output = if config.fail_on_critical && has_critical {
            PluginOutput::failure(format!(
                "{} (fail_on_critical: critical findings present)",
                summary
            ))
        } else {
            PluginOutput::success(summary)
        };

        output.risk_band = risk_band;

        for pf in &plugin_findings_vec {
            output.add_finding(pf.clone());
        }

        output.add_report_path("markdown", md_path.to_string_lossy().to_string());
        output.add_report_path("json", json_path.to_string_lossy().to_string());
        if config.include_sarif {
            output.add_report_path("sarif", sarif_path.to_string_lossy().to_string());
        }
        output.add_written_file(md_path.to_string_lossy().to_string());
        output.add_written_file(json_path.to_string_lossy().to_string());
        if config.include_sarif {
            output.add_written_file(sarif_path.to_string_lossy().to_string());
        }

        output.set_score("finding_count", scored.len() as f64);

        // Step 14: Return completed output.
        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Computes the reports output directory from the plugin context.
///
/// Prefers `config.reports.output_dir` when non-empty, otherwise falls back
/// to the workspace-managed reports directory.
fn build_reports_dir(ctx: &PluginContext) -> PathBuf {
    if !ctx.config.reports.output_dir.is_empty() {
        PathBuf::from(&ctx.config.reports.output_dir)
    } else {
        ctx.workspace.paths.reports_dir()
    }
}

/// Builds the user-facing prompt from scan metadata, prioritized files, and
/// the active security categories.
///
/// # Arguments
///
/// * `scan_result`       - Repository scan containing name, language, etc.
/// * `prioritized_files` - Ordered list of files to review.
/// * `categories`        - Active security categories derived from config flags.
///
/// # Returns
///
/// A human-readable prompt string ready for the AI provider.
fn build_user_prompt(
    scan_result: &crate::scanner::result::ScanResult,
    prioritized_files: &[String],
    categories: &[SecurityCategory],
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

    let cat_names: Vec<&str> = categories.iter().map(|c| c.as_str()).collect();
    if cat_names.is_empty() {
        prompt.push_str("Security categories: all\n");
    } else {
        prompt.push_str(&format!("Security categories: {}\n", cat_names.join(", ")));
    }

    prompt.push('\n');
    prompt.push_str("Provide your security review findings.\n");

    prompt
}

/// Attempts to parse [`SecurityReviewFinding`]s from an AI response string.
///
/// Expects the response to contain a JSON object with a `"findings"` array.
/// Returns an empty vector on any parse failure; no diagnostic is emitted
/// from this function.
///
/// # Arguments
///
/// * `content` - Raw text content from the AI provider.
///
/// # Returns
///
/// A `Vec<SecurityReviewFinding>` parsed from the JSON; may be empty.
fn parse_ai_response(content: &str) -> Vec<SecurityReviewFinding> {
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
        .filter_map(SecurityReviewFinding::from_json_value)
        .collect()
}

/// Strips markdown code fences and extracts the embedded JSON string.
///
/// Handles both ` ```json ... ``` ` and ` ``` ... ``` ` wrapping. If no code
/// fence is found, returns the original content trimmed of whitespace.
///
/// # Arguments
///
/// * `content` - The raw string that may or may not contain code fences.
///
/// # Returns
///
/// A string slice pointing into `content` with fences removed.
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
/// The threshold string is parsed with
/// [`SecurityReviewFinding::parse_severity`]. Findings strictly below the
/// threshold are discarded.
fn filter_by_severity(
    findings: Vec<SecurityReviewFinding>,
    threshold: &str,
) -> Vec<SecurityReviewFinding> {
    let min_severity = SecurityReviewFinding::parse_severity(threshold);
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
    use crate::config::{Config, GovernanceConfig, SecurityReviewConfig};
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
        config_override: Option<SecurityReviewConfig>,
        provider: Arc<dyn crate::providers::base::Provider + Send + Sync>,
    ) -> PluginContext {
        // SAFETY: WorkspaceManager::create only fails on I/O errors; temp dirs are writable.
        let manager = WorkspaceManager::create(root, "test://repo", None, None).unwrap();
        let state = manager.state.clone();
        let workspace = Arc::new(manager);

        let mut config = Config::default();
        if let Some(sec_cfg) = config_override {
            config.security_review = sec_cfg;
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
            empty_scan(),
            provider,
            tool_registry,
            governance,
        )
    }

    // ------------------------------------------------------------------
    // Static method tests
    // ------------------------------------------------------------------

    #[test]
    fn test_security_review_plugin_name_returns_security_review() {
        assert_eq!(SecurityReviewPlugin.name(), "security-review");
    }

    #[test]
    fn test_security_review_plugin_metadata_has_correct_name() {
        let meta = SecurityReviewPlugin.metadata();
        assert_eq!(meta.name, "security-review");
    }

    #[test]
    fn test_security_review_plugin_metadata_has_version() {
        let meta = SecurityReviewPlugin.metadata();
        assert_eq!(meta.version, "1.0.0");
    }

    #[test]
    fn test_security_review_plugin_metadata_has_description() {
        let meta = SecurityReviewPlugin.metadata();
        assert!(!meta.description.is_empty());
    }

    #[test]
    fn test_security_review_plugin_supported_formats_includes_markdown_json_sarif() {
        let formats = SecurityReviewPlugin.supported_formats();
        assert!(formats.contains(&"markdown".to_string()));
        assert!(formats.contains(&"json".to_string()));
        assert!(formats.contains(&"sarif".to_string()));
    }

    #[test]
    fn test_security_review_plugin_required_tool_access_is_read_only() {
        assert_eq!(
            SecurityReviewPlugin.required_tool_access(),
            ToolAccessLevel::ReadOnly
        );
    }

    // ------------------------------------------------------------------
    // parse_ai_response
    // ------------------------------------------------------------------

    #[test]
    fn test_parse_ai_response_with_valid_json_returns_findings() {
        let json = r#"{"findings":[{"category":"secrets","severity":"high","evidence":"API key found","exploitability":"High","impact":"Credential exposure","remediation":"Move to env var","confidence":0.9}]}"#;
        let findings = parse_ai_response(json);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].category, "secrets");
    }

    #[test]
    fn test_parse_ai_response_with_empty_findings_array_returns_empty() {
        let json = r#"{"findings":[]}"#;
        let findings = parse_ai_response(json);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_parse_ai_response_with_invalid_json_returns_empty() {
        let findings = parse_ai_response("not json at all");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_parse_ai_response_with_empty_string_returns_empty() {
        let findings = parse_ai_response("");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_parse_ai_response_with_markdown_code_fence_strips_fence() {
        let content = "```json\n{\"findings\":[]}\n```";
        let findings = parse_ai_response(content);
        assert!(findings.is_empty(), "should parse through code fence");
    }

    // ------------------------------------------------------------------
    // filter_by_severity
    // ------------------------------------------------------------------

    #[test]
    fn test_filter_by_severity_medium_threshold_excludes_info_and_low() {
        let make = |sev| {
            SecurityReviewFinding::new(
                "secrets",
                sev,
                "evidence",
                "exploitability",
                "impact",
                "remediation",
                0.8,
            )
        };
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
        let make = |sev| {
            SecurityReviewFinding::new(
                "auth",
                sev,
                "evidence",
                "exploitability",
                "impact",
                "remediation",
                0.8,
            )
        };
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
        let make = |sev| {
            SecurityReviewFinding::new(
                "unsafe_rust",
                sev,
                "evidence",
                "exploitability",
                "impact",
                "remediation",
                0.8,
            )
        };
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
        let cats = vec![SecurityCategory::Secrets];
        let prompt = build_user_prompt(&scan, &[], &cats);
        assert!(prompt.contains("test-repo"), "must contain repo name");
    }

    #[test]
    fn test_build_user_prompt_lists_files() {
        let scan = empty_scan();
        let files = vec!["src/auth.rs".to_string(), "Cargo.toml".to_string()];
        let prompt = build_user_prompt(&scan, &files, &[]);
        assert!(prompt.contains("src/auth.rs"), "must list auth.rs");
        assert!(prompt.contains("Cargo.toml"), "must list Cargo.toml");
    }

    #[test]
    fn test_build_user_prompt_lists_categories() {
        let scan = empty_scan();
        let cats = vec![SecurityCategory::Secrets, SecurityCategory::UnsafeRust];
        let prompt = build_user_prompt(&scan, &[], &cats);
        assert!(prompt.contains("secrets"), "must contain secrets category");
        assert!(
            prompt.contains("unsafe_rust"),
            "must contain unsafe_rust category"
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

    // ------------------------------------------------------------------
    // Async integration tests (requires tokio)
    // ------------------------------------------------------------------

    /// Returns a [`MockProvider`] that advertises tool support and returns a
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
    async fn test_security_review_plugin_run_with_mock_provider_empty_findings_returns_success() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mock = make_tool_calling_provider("{\"findings\":[]}");
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let ctx = make_context(tmp.path().to_str().unwrap(), None, provider);
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        assert!(output.completed, "plugin must report completed");
    }

    #[tokio::test]
    async fn test_security_review_plugin_run_with_mock_provider_with_findings_returns_success() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let finding_json = concat!(
            "{\"findings\":[{\"category\":\"secrets\",\"severity\":\"high\",",
            "\"evidence\":\"API key found in source\",",
            "\"exploitability\":\"Trivial - key is directly usable\",",
            "\"impact\":\"Full credential compromise\",",
            "\"remediation\":\"Move to environment variable\",",
            "\"confidence\":0.9}]}"
        );
        let mock = make_tool_calling_provider(finding_json);
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let ctx = make_context(tmp.path().to_str().unwrap(), None, provider);
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        assert!(output.completed, "plugin must report completed");
        assert_eq!(output.findings.len(), 1, "should have one finding");
    }

    #[tokio::test]
    async fn test_security_review_plugin_run_provider_error_returns_failure_output() {
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
        let ctx = make_context(tmp.path().to_str().unwrap(), None, provider);
        // SAFETY: run() returns Ok(PluginOutput::failure) rather than Err on provider errors.
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        assert!(
            !output.completed,
            "plugin must report not completed on provider error"
        );
    }

    #[tokio::test]
    async fn test_security_review_plugin_run_no_tool_support_returns_hard_error() {
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
        let ctx = make_context(tmp.path().to_str().unwrap(), None, provider);
        let result = SecurityReviewPlugin.run(ctx).await;
        assert!(
            result.is_err(),
            "must return a hard error when provider lacks tool support"
        );
        let msg = result.expect_err("expected Err").to_string();
        assert!(
            msg.contains("tool calling"),
            "error must mention tool calling, got: {msg}"
        );
    }

    #[tokio::test]
    async fn test_security_review_plugin_run_exercises_multi_turn_tool_call_round_trip() {
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
        let ctx = make_context(tmp.path().to_str().unwrap(), None, provider);
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
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
    async fn test_security_review_plugin_run_disabled_returns_success_immediately() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mut mock = MockProvider::new();
        // Neither metadata() nor complete() is called when plugin is disabled.
        mock.expect_metadata().never();
        mock.expect_complete().never();
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let disabled_cfg = SecurityReviewConfig {
            enabled: false,
            ..SecurityReviewConfig::default()
        };
        let ctx = make_context(tmp.path().to_str().unwrap(), Some(disabled_cfg), provider);
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        assert!(output.completed, "disabled plugin must return completed");
        assert!(
            output.summary.contains("disabled"),
            "summary must mention disabled"
        );
    }

    #[tokio::test]
    async fn test_security_review_plugin_run_writes_markdown_report() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mock = make_tool_calling_provider("{\"findings\":[]}");
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let ctx = make_context(tmp.path().to_str().unwrap(), None, provider);
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        assert!(
            output
                .report_paths
                .values()
                .flatten()
                .any(|p| p.ends_with("security_review.md")),
            "output must include markdown report path"
        );
    }

    #[tokio::test]
    async fn test_security_review_plugin_run_writes_json_report() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mock = make_tool_calling_provider("{\"findings\":[]}");
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let ctx = make_context(tmp.path().to_str().unwrap(), None, provider);
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        assert!(
            output
                .report_paths
                .values()
                .flatten()
                .any(|p| p.ends_with("security_review.json")),
            "output must include json report path"
        );
    }

    #[tokio::test]
    async fn test_security_review_plugin_run_writes_sarif_report() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mock = make_tool_calling_provider("{\"findings\":[]}");
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let ctx = make_context(tmp.path().to_str().unwrap(), None, provider);
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        assert!(
            output
                .report_paths
                .values()
                .flatten()
                .any(|p| p.ends_with("security_review.sarif")),
            "output must include sarif report path"
        );
    }

    #[tokio::test]
    async fn test_security_review_plugin_run_invalid_config_returns_failure() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let mut mock = MockProvider::new();
        // Neither metadata() nor complete() is called when config is invalid.
        mock.expect_metadata().never();
        mock.expect_complete().never();
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let invalid_cfg = SecurityReviewConfig {
            max_findings: 0,
            ..SecurityReviewConfig::default()
        };
        let ctx = make_context(tmp.path().to_str().unwrap(), Some(invalid_cfg), provider);
        // SAFETY: run() returns Ok(PluginOutput::failure) on config error, not Err.
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        assert!(
            !output.completed,
            "invalid config must produce a failure output"
        );
    }

    #[tokio::test]
    async fn test_security_review_plugin_run_fail_on_critical_with_critical_finding_returns_failure()
     {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let critical_json = concat!(
            "{\"findings\":[{\"category\":\"secrets\",\"severity\":\"critical\",",
            "\"evidence\":\"Hardcoded production credentials detected\",",
            "\"exploitability\":\"Trivial - credentials are directly usable\",",
            "\"impact\":\"Full system compromise possible\",",
            "\"remediation\":\"Remove credentials and rotate immediately\",",
            "\"confidence\":0.95}]}"
        );
        let mock = make_tool_calling_provider(critical_json);
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let cfg = SecurityReviewConfig {
            fail_on_critical: true,
            ..SecurityReviewConfig::default()
        };
        let ctx = make_context(tmp.path().to_str().unwrap(), Some(cfg), provider);
        // SAFETY: run() returns Ok(PluginOutput::failure) when fail_on_critical triggers.
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        assert!(
            !output.completed,
            "fail_on_critical with a critical finding must produce a failure output"
        );
    }

    #[tokio::test]
    async fn test_security_review_plugin_run_fail_on_critical_without_critical_finding_returns_success()
     {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let high_json = concat!(
            "{\"findings\":[{\"category\":\"auth\",\"severity\":\"high\",",
            "\"evidence\":\"Missing rate limiting on login endpoint\",",
            "\"exploitability\":\"Moderate effort required\",",
            "\"impact\":\"Account takeover via brute force\",",
            "\"remediation\":\"Implement rate limiting and account lockout\",",
            "\"confidence\":0.85}]}"
        );
        let mock = make_tool_calling_provider(high_json);
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let cfg = SecurityReviewConfig {
            fail_on_critical: true,
            ..SecurityReviewConfig::default()
        };
        let ctx = make_context(tmp.path().to_str().unwrap(), Some(cfg), provider);
        // SAFETY: run() returns success when fail_on_critical is set but no critical findings exist.
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        assert!(
            output.completed,
            "fail_on_critical with only high findings must still succeed"
        );
    }

    // ------------------------------------------------------------------
    // Phase 2 scoring integration tests
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_security_review_plugin_absolute_violation_finding_always_included() {
        // A critical credential finding triggers AbsoluteViolation and must
        // be included regardless of the confidence_threshold.
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let finding_json = concat!(
            "{\"findings\":[{\"category\":\"secrets_exposure\",\"severity\":\"critical\",",
            "\"evidence\":\"Hardcoded API key found.\",",
            "\"exploitability\":\"Trivially exploitable.\",",
            "\"impact\":\"Full API access.\",",
            "\"remediation\":\"Use environment variables.\",",
            "\"confidence\":0.9}]}"
        );
        let mock = make_tool_calling_provider(finding_json);
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        // Set a very high confidence_threshold; the AbsoluteViolation must bypass it.
        let cfg = SecurityReviewConfig {
            confidence_threshold: 0.99,
            ai_confidence_weight: 0.5,
            ai_analysis_enabled: true,
            ..SecurityReviewConfig::default()
        };
        let ctx = make_context(tmp.path().to_str().unwrap(), Some(cfg), provider);
        // SAFETY: run() should not fail.
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        assert!(output.completed, "plugin must complete");
        assert_eq!(
            output.findings.len(),
            1,
            "absolute violation finding must always be included despite high threshold"
        );
        // The blended confidence must be lower than the raw AI confidence (0.9).
        assert!(
            output.findings[0].confidence < 0.9,
            "blended score must be lower than raw AI confidence for a violation finding"
        );
    }

    #[tokio::test]
    async fn test_security_review_plugin_ai_weight_shifts_blended_confidence() {
        // Higher ai_confidence_weight must produce a blended score closer to
        // the AI-reported confidence.
        let tmp_low = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let tmp_high = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let finding_json = concat!(
            "{\"findings\":[{\"category\":\"injection\",\"severity\":\"high\",",
            "\"evidence\":\"SQL injection found.\",",
            "\"exploitability\":\"Easy.\",",
            "\"impact\":\"Data leak.\",",
            "\"remediation\":\"Use params.\",",
            "\"confidence\":0.95}]}"
        );
        let mock1 = make_tool_calling_provider(finding_json);
        let provider1: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock1);
        let cfg_low_weight = SecurityReviewConfig {
            ai_confidence_weight: 0.1,
            confidence_threshold: 0.0, // include all findings
            ..SecurityReviewConfig::default()
        };
        let ctx1 = make_context(
            tmp_low.path().to_str().unwrap(),
            Some(cfg_low_weight),
            provider1,
        );
        // SAFETY: run() should not fail.
        let out_low = SecurityReviewPlugin.run(ctx1).await.unwrap();

        let mock2 = make_tool_calling_provider(finding_json);
        let provider2: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock2);
        let cfg_high_weight = SecurityReviewConfig {
            ai_confidence_weight: 0.9,
            confidence_threshold: 0.0, // include all findings
            ..SecurityReviewConfig::default()
        };
        let ctx2 = make_context(
            tmp_high.path().to_str().unwrap(),
            Some(cfg_high_weight),
            provider2,
        );
        // SAFETY: run() should not fail.
        let out_high = SecurityReviewPlugin.run(ctx2).await.unwrap();

        assert_eq!(out_low.findings.len(), 1);
        assert_eq!(out_high.findings.len(), 1);
        // Higher AI weight -> blended score closer to AI confidence (0.95).
        assert!(
            out_high.findings[0].confidence > out_low.findings[0].confidence,
            "higher ai_confidence_weight must produce higher blended confidence"
        );
    }

    #[tokio::test]
    async fn test_security_review_plugin_ai_disabled_blended_equals_static_score() {
        // When ai_analysis_enabled = false, the output confidence must equal the
        // static score, matching the behaviour of a run with no AI provider.
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let finding_json = concat!(
            "{\"findings\":[{\"category\":\"injection\",\"severity\":\"medium\",",
            "\"evidence\":\"Input echoed without sanitisation.\",",
            "\"exploitability\":\"Moderate.\",",
            "\"impact\":\"XSS possible.\",",
            "\"remediation\":\"Escape output.\",",
            "\"confidence\":0.85}]}"
        );
        let mock = make_tool_calling_provider(finding_json);
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> = Arc::new(mock);
        let cfg = SecurityReviewConfig {
            ai_analysis_enabled: false,
            confidence_threshold: 0.0, // include all findings
            ..SecurityReviewConfig::default()
        };
        let ctx = make_context(tmp.path().to_str().unwrap(), Some(cfg), provider);
        // SAFETY: run() should not fail.
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
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
}
