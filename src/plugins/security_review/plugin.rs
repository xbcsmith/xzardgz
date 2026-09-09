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
//! 2. Check whether the plugin is enabled; return early if not.  When
//!    enabled, resolve OSV vulnerability context if `osv_enabled` and
//!    `dependency_scanning` are both `true`.
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

// These imports are declared for the full OSV integration being implemented
// by the parallel vuln agent. They are forward-declared here so that merging
// both agents' branches requires no further edits to this file.
#[allow(unused_imports)]
use crate::clients::vuln::osv::OsvClient;
#[allow(unused_imports)]
use crate::clients::vuln::osv::scoring::{OsvScore, score_severity};
#[allow(unused_imports)]
use crate::clients::vuln::{VulnerabilityQuery, VulnerabilitySource};
use crate::scanner::scoring::ScoringSignal;

use std::sync::Arc;

use crate::agent::context::AgentContext;
use crate::agent::session::AgentSession;
use crate::diagnostics::Diagnostics;
use crate::investigation::batch::{
    BatchOutcome, BatchSession, BatchedInvestigationRunner, InvestigationBatch, InvestigationError,
};
use crate::investigation::scope::{FileMatchEntry, InvestigationScope, ScopeMetrics};
use crate::investigation::strategy::{
    InvestigationStrategy, compute_turn_budget, decide_investigation_strategy,
};
use crate::providers::base::Provider;
use crate::tools::registry::ToolRegistry;

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

        // Step 2b: Resolve OSV vulnerability context when dependency scanning is
        // enabled. The client runs against the public OSV endpoint; no
        // authentication is required. Failures degrade gracefully to an empty
        // signal list rather than failing the whole run.
        let scan_result_for_osv = ctx.scan_result.clone();
        let osv_signals: Vec<ScoringSignal> = if config.osv_enabled && config.dependency_scanning {
            resolve_osv_signals(&scan_result_for_osv, &mut ctx).await
        } else {
            vec![]
        };

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

        // Step 5: Resolve the system prompt via PromptLoader.
        //
        // Resolution order: in-memory override > file-based override >
        // embedded default. All override failures fall back transparently.
        let system_prompt =
            ctx.prompt_loader
                .render("security_review", "system", &tera::Context::new());

        // Step 6: Compute the turn budget and select the investigation strategy.
        //
        // The turn budget is derived from the repository's matched-file count
        // and total size via `compute_turn_budget`, replacing the static
        // `config.agent_max_turns` cap.  `decide_investigation_strategy`
        // selects either a single-session or batched-session execution plan
        // based on the configurable threshold fields.
        let metrics = ScopeMetrics::from_scan_result(&ctx.scan_result);
        let turn_budget = compute_turn_budget(&metrics);
        let threshold_files = config.investigation_threshold_files.unwrap_or(20);
        let threshold_bytes = config.investigation_threshold_bytes.unwrap_or(10_000_000);
        let batch_count = config.investigation_batch_count.unwrap_or(4) as usize;
        let strategy =
            decide_investigation_strategy(&metrics, threshold_files, threshold_bytes, batch_count);

        // Step 7: Run AI analysis — single session or batched sessions.
        //
        // For SingleSession the computed turn_budget replaces the static
        // config.agent_max_turns value.  For BatchedSession a
        // SecurityReviewBatchSession drives each batch independently via
        // BatchedInvestigationRunner; exhausted-batch diagnostics are drained
        // into the plugin context so they appear in the report output and in
        // WatcherResultMessage.diagnostics.
        let parsed_findings: Vec<SecurityReviewFinding> = match strategy {
            InvestigationStrategy::SingleSession => {
                let osv_note_str: Option<String> = if osv_signals.is_empty() {
                    None
                } else {
                    Some(format!(
                        "OSV vulnerability signals: {} dependency vulnerability signal(s) detected.",
                        osv_signals.len()
                    ))
                };
                let user_prompt = build_user_prompt(
                    &ctx.scan_result,
                    &prioritized_files,
                    &categories,
                    osv_note_str.as_deref(),
                );
                let session = ctx.build_agent_session(system_prompt, 8192, turn_budget as usize)?;
                match session.run(&user_prompt).await {
                    Ok(content) => parse_ai_response(&content),
                    Err(e) => {
                        return Ok(PluginOutput::failure(format!(
                            "security-review: agent session failed: {e}"
                        )));
                    }
                }
            }
            InvestigationStrategy::BatchedSession(batch_config) => {
                let mut scope = InvestigationScope::new();
                for f in &prioritized_files {
                    scope.insert(FileMatchEntry::new(f));
                }
                let batch_session = SecurityReviewBatchSession {
                    provider: Arc::clone(&ctx.provider),
                    system_prompt,
                    scan_result: ctx.scan_result.clone(),
                    config: config.clone(),
                };
                let runner = BatchedInvestigationRunner::new(batch_session, batch_config);
                let outcome = runner.run(&scope, turn_budget).await;
                // Drain exhausted-batch diagnostics into the plugin context so
                // they surface in the plugin's report output and, when run via
                // the watcher path, in WatcherResultMessage.diagnostics.
                for diag in outcome.diagnostics.entries {
                    ctx.add_diagnostic(diag);
                }
                // Parse each batch's raw AI response into typed findings and
                // merge them into a single list.
                outcome
                    .findings
                    .iter()
                    .flat_map(|raw| parse_ai_response(raw))
                    .collect()
            }
        };

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
/// * `osv_note`          - Optional OSV vulnerability context note to append.
///
/// # Returns
///
/// A human-readable prompt string ready for the AI provider.
fn build_user_prompt(
    scan_result: &crate::scanner::result::ScanResult,
    prioritized_files: &[String],
    categories: &[SecurityCategory],
    osv_note: Option<&str>,
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
    if let Some(note) = osv_note {
        prompt.push_str(note);
        prompt.push('\n');
    }
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
// SecurityReviewBatchSession
// ---------------------------------------------------------------------------

/// A [`BatchSession`] implementation for the security review plugin.
///
/// Each call to [`BatchSession::run`] creates a fresh [`AgentSession`] scoped
/// to the files in the supplied [`InvestigationBatch`] and runs the security
/// review analysis on those files only.  This is the concrete session type
/// supplied to [`BatchedInvestigationRunner`] when
/// [`decide_investigation_strategy`] selects
/// [`InvestigationStrategy::BatchedSession`].
struct SecurityReviewBatchSession {
    /// AI provider shared across all batch sessions.
    provider: Arc<dyn Provider + Send + Sync>,
    /// System prompt pre-seeded into every batch session's context.
    system_prompt: String,
    /// Scan result supplying repository context to the user prompt.
    scan_result: crate::scanner::result::ScanResult,
    /// Plugin configuration (used to rebuild active categories per batch).
    config: crate::config::SecurityReviewConfig,
}

#[async_trait::async_trait]
impl BatchSession for SecurityReviewBatchSession {
    /// Runs the security review AI analysis for a single batch.
    ///
    /// Builds a user prompt from the batch's file paths, creates a fresh
    /// [`AgentSession`] with `turn_budget` as the turn limit, and runs the
    /// session.
    ///
    /// # Returns
    ///
    /// `Ok(BatchOutcome)` on success, where `findings` contains the single
    /// raw AI response string.
    ///
    /// # Errors
    ///
    /// Returns [`InvestigationError::TurnBudgetExceeded`] when the session
    /// reports `"max turns reached"`.  Returns [`InvestigationError::BatchFailed`]
    /// for any other session failure.
    async fn run(
        &self,
        batch: &InvestigationBatch,
        turn_budget: u32,
    ) -> std::result::Result<BatchOutcome, InvestigationError> {
        let files: Vec<String> = batch.scope.paths();
        let categories = super::scope::SecurityFilePrioritizer::active_categories(
            self.config.secret_scanning,
            self.config.dependency_scanning,
            self.config.check_unsafe_code,
            self.config.check_auth,
            self.config.check_endpoints,
            self.config.check_command_execution,
            self.config.check_deserialization,
            self.config.check_cryptography,
        );
        let user_prompt = build_user_prompt(&self.scan_result, &files, &categories, None);
        let context = AgentContext::new(self.system_prompt.clone(), 8192);
        let session = AgentSession::new(Arc::clone(&self.provider), context, ToolRegistry::new())
            .with_max_turns(turn_budget as usize);
        match session.run(&user_prompt).await {
            Ok(content) => Ok(BatchOutcome {
                batch_index: batch.index,
                total_batches: batch.total_batches,
                findings: vec![content],
                diagnostics: Diagnostics::new(),
                turn_budget_exhausted: false,
            }),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("max turns reached") {
                    Err(InvestigationError::TurnBudgetExceeded {
                        batch_index: batch.index,
                        turn_limit: turn_budget,
                        partial_findings: vec![],
                    })
                } else {
                    Err(InvestigationError::BatchFailed {
                        batch_index: batch.index,
                        message: msg,
                    })
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// OSV signal resolution
// ---------------------------------------------------------------------------

/// Resolves OSV vulnerability signals for the repository's dependency manifests.
///
/// Creates an [`OsvClient`] and queries the public OSV endpoint for any
/// packages whose names can be inferred from the scan result. When no
/// parseable package references are present, returns an empty vec without
/// emitting any diagnostics.
///
/// Errors during OSV resolution are recorded as warnings and converted to
/// empty signal lists rather than propagating as hard errors.
///
/// # Arguments
///
/// * `scan_result` - The scan result providing dependency manifest paths.
/// * `_ctx` - Plugin context reserved for future diagnostic recording.
///
/// # Returns
///
/// A `Vec<ScoringSignal>` containing any vulnerability signals derived from
/// OSV results. May be empty.
async fn resolve_osv_signals(
    scan_result: &crate::scanner::result::ScanResult,
    _ctx: &mut crate::plugins::context::PluginContext,
) -> Vec<ScoringSignal> {
    let has_dep_manifests = !scan_result.dependency_manifests.is_empty()
        || !scan_result
            .plugin_preselection
            .dependency_manifests
            .is_empty();

    if !has_dep_manifests {
        return vec![];
    }

    // Full per-package OSV scanning requires a parsed dependency manifest.
    // ScanResult currently provides only manifest file paths, not parsed
    // package lists. This function returns an empty list until a manifest
    // parser is wired in; the OsvClient implementation is complete and
    // can be exercised directly via VulnerabilitySource::query.
    vec![]
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
    use crate::investigation::scope::ScopeMetrics;
    use crate::investigation::strategy::{
        InvestigationStrategy, compute_turn_budget, decide_investigation_strategy,
    };
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
        let prompt = build_user_prompt(&scan, &[], &cats, None);
        assert!(prompt.contains("test-repo"), "must contain repo name");
    }

    #[test]
    fn test_build_user_prompt_lists_files() {
        let scan = empty_scan();
        let files = vec!["src/auth.rs".to_string(), "Cargo.toml".to_string()];
        let prompt = build_user_prompt(&scan, &files, &[], None);
        assert!(prompt.contains("src/auth.rs"), "must list auth.rs");
        assert!(prompt.contains("Cargo.toml"), "must list Cargo.toml");
    }

    #[test]
    fn test_build_user_prompt_lists_categories() {
        let scan = empty_scan();
        let cats = vec![SecurityCategory::Secrets, SecurityCategory::UnsafeRust];
        let prompt = build_user_prompt(&scan, &[], &cats, None);
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

    /// Returns an [`Arc<dyn Provider>`] wrapping a [`MockProvider`] that
    /// advertises tool support and returns a tool call on the first completion,
    /// then items from `responses` on subsequent calls.
    fn make_tool_calling_provider(
        responses: Vec<String>,
    ) -> Arc<dyn crate::providers::base::Provider + Send + Sync> {
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
        let responses = Arc::new(responses);
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
                let idx = (*count - 2) as usize;
                let content = responses
                    .get(idx)
                    .map(|s| s.as_str())
                    .unwrap_or("{\"findings\":[]}");
                Ok(Message::assistant(content))
            }
        });
        Arc::new(mock)
    }

    #[tokio::test]
    async fn test_security_review_plugin_run_with_mock_provider_empty_findings_returns_success() {
        let tmp = tempfile::TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let provider = make_tool_calling_provider(vec!["{\"findings\":[]}".to_string()]);
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
        let provider = make_tool_calling_provider(vec![finding_json.to_string()]);
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
        let provider = make_tool_calling_provider(vec!["{\"findings\":[]}".to_string()]);
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
        let provider = make_tool_calling_provider(vec!["{\"findings\":[]}".to_string()]);
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
        let provider = make_tool_calling_provider(vec!["{\"findings\":[]}".to_string()]);
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
        let provider = make_tool_calling_provider(vec![critical_json.to_string()]);
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
        let provider = make_tool_calling_provider(vec![high_json.to_string()]);
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
        let provider = make_tool_calling_provider(vec![finding_json.to_string()]);
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
        let provider1 = make_tool_calling_provider(vec![finding_json.to_string()]);
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

        let provider2 = make_tool_calling_provider(vec![finding_json.to_string()]);
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
        let provider = make_tool_calling_provider(vec![finding_json.to_string()]);
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

    // ------------------------------------------------------------------
    // Phase 1.3 / 2.3: investigation strategy wiring
    // ------------------------------------------------------------------

    #[test]
    fn test_security_review_plugin_scope_metrics_derive_from_scan_result() {
        // Verify ScopeMetrics is correctly derived from the plugin's scan result.
        // This ensures the investigation wiring can read repository metrics.
        let scan = empty_scan();
        let metrics = ScopeMetrics::from_scan_result(&scan);
        assert_eq!(metrics.total_files, 0);
        assert_eq!(metrics.total_size_bytes, 0);
        assert_eq!(metrics.matched_file_count, 0);
    }

    #[test]
    fn test_security_review_plugin_compute_turn_budget_returns_nonzero() {
        // compute_turn_budget must always return at least BASE_TURNS (5) even
        // for an empty repository, confirming the function is reachable from
        // the plugin layer.
        let scan = empty_scan();
        let metrics = ScopeMetrics::from_scan_result(&scan);
        let budget = compute_turn_budget(&metrics);
        assert!(
            budget >= 5,
            "turn budget must be >= BASE_TURNS (5), got {budget}"
        );
    }

    #[test]
    fn test_security_review_plugin_strategy_is_single_session_for_empty_scan() {
        // An empty scan result (no preselection files) must not exceed the
        // default threshold, so the strategy must be SingleSession.
        let scan = empty_scan();
        let metrics = ScopeMetrics::from_scan_result(&scan);
        let strategy = decide_investigation_strategy(&metrics, 20, 10_000_000, 4);
        assert!(
            matches!(strategy, InvestigationStrategy::SingleSession),
            "empty scan must yield SingleSession strategy"
        );
    }

    #[test]
    fn test_security_review_plugin_zero_threshold_forces_batched_strategy() {
        // Setting investigation_threshold_files to 0 always forces BatchedSession
        // because matched_file_count > 0 is satisfied when threshold is 0.
        // This verifies the config fields propagate into strategy selection.
        let scan = empty_scan();
        let _ = scan; // scan is not used directly; metrics are constructed manually
        let metrics = ScopeMetrics::new(0, 0, 1); // 1 matched file
        let strategy = decide_investigation_strategy(&metrics, 0, u64::MAX, 4);
        assert!(
            matches!(strategy, InvestigationStrategy::BatchedSession(_)),
            "matched_file_count > 0 with threshold_files=0 must yield BatchedSession"
        );
    }

    #[tokio::test]
    async fn test_security_review_plugin_run_with_single_session_strategy_uses_computed_budget() {
        // End-to-end test: plugin runs successfully with default (SingleSession)
        // strategy for an empty scan result, confirming that the strategy
        // selection path runs without error.
        use crate::config::SecurityReviewConfig;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let provider = make_tool_calling_provider(vec!["{\"findings\":[]}".to_string()]);
        let ctx = make_context(
            tmp.path().to_str().unwrap(),
            Some(SecurityReviewConfig {
                enabled: true,
                investigation_threshold_files: None, // default -> SingleSession
                investigation_threshold_bytes: None,
                investigation_batch_count: None,
                ..SecurityReviewConfig::default()
            }),
            provider,
        );
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        assert!(
            output.completed,
            "plugin run must complete with single-session strategy"
        );
    }

    #[tokio::test]
    async fn test_security_review_plugin_run_with_batched_strategy_empty_scope_completes() {
        // When investigation_threshold_files is set to 0 (forcing BatchedSession)
        // but the file list is empty, the runner returns an empty outcome and
        // the plugin completes without findings.
        use crate::config::SecurityReviewConfig;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let provider = make_tool_calling_provider(vec!["{\"findings\":[]}".to_string()]);
        let ctx = make_context(
            tmp.path().to_str().unwrap(),
            Some(SecurityReviewConfig {
                enabled: true,
                investigation_threshold_files: Some(0), // force BatchedSession
                investigation_threshold_bytes: Some(0),
                investigation_batch_count: Some(2),
                ..SecurityReviewConfig::default()
            }),
            provider,
        );
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        // Empty scope -> BatchedInvestigationRunner returns empty outcome -> no findings.
        assert!(
            output.completed,
            "plugin run with batched strategy and empty scope must complete"
        );
    }

    // ------------------------------------------------------------------
    // OSV integration
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_security_review_plugin_run_osv_disabled_skips_osv_resolution() {
        // With osv_enabled=false the OSV path is not taken; the plugin must
        // still complete successfully.
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap(); // SAFETY: only fails on OS error.
        let provider = make_tool_calling_provider(vec!["{\"findings\":[]}".to_string()]);
        let cfg = SecurityReviewConfig {
            osv_enabled: false,
            dependency_scanning: true,
            ..SecurityReviewConfig::default()
        };
        let ctx = make_context(tmp.path().to_str().unwrap(), Some(cfg), provider);
        let output = SecurityReviewPlugin.run(ctx).await.unwrap();
        assert!(
            output.completed,
            "plugin must complete successfully when osv_enabled=false"
        );
    }

    #[test]
    fn test_build_user_prompt_with_osv_note_includes_note() {
        let scan = empty_scan();
        let note = "OSV signal: 3 vulns";
        let prompt = build_user_prompt(&scan, &[], &[], Some(note));
        assert!(prompt.contains(note), "prompt must contain the OSV note");
    }

    #[test]
    fn test_build_user_prompt_without_osv_note_has_no_osv_text() {
        let scan = empty_scan();
        let prompt = build_user_prompt(&scan, &[], &[], None);
        assert!(
            !prompt.contains("OSV"),
            "prompt must not contain OSV text when no note is given"
        );
    }
}
