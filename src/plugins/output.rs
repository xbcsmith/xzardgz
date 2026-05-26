//! Output types produced by plugin runs.
//!
//! [`PluginOutput`] is the complete record returned by
//! [`WorkflowPlugin::run`][crate::plugins::trait_def::WorkflowPlugin::run].
//! It carries a summary, written file paths, structured findings, scores,
//! diagnostics, risk classification, report paths, provider metadata, and
//! token usage.
//!
//! [`TokenUsage`] captures the prompt and completion token counts reported by
//! the AI provider for a single plugin invocation.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostics;
use crate::providers::types::ProviderMetadata;
use crate::reports::findings::{PluginFinding, PluginFindings};
use crate::reports::risk_band::RiskBand;

// ---------------------------------------------------------------------------
// TokenUsage
// ---------------------------------------------------------------------------

/// Token usage reported by the AI provider for a plugin run.
///
/// Tracks how many input (prompt) and output (completion) tokens were consumed
/// by the provider call so that usage can be surfaced in reports and
/// cost-accounting dashboards.
///
/// # Examples
///
/// ```
/// use xzardgz::plugins::output::TokenUsage;
///
/// let usage = TokenUsage::new(100, 250);
/// assert_eq!(usage.input_tokens, 100);
/// assert_eq!(usage.output_tokens, 250);
/// assert_eq!(usage.total(), 350);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Number of input (prompt) tokens consumed.
    pub input_tokens: u64,
    /// Number of output (completion) tokens produced.
    pub output_tokens: u64,
}

impl TokenUsage {
    /// Creates a new `TokenUsage` with the given input and output token counts.
    ///
    /// # Arguments
    ///
    /// * `input_tokens` - Number of input (prompt) tokens consumed.
    /// * `output_tokens` - Number of output (completion) tokens produced.
    ///
    /// # Returns
    ///
    /// A new [`TokenUsage`] instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::output::TokenUsage;
    ///
    /// let usage = TokenUsage::new(512, 1024);
    /// assert_eq!(usage.input_tokens, 512);
    /// assert_eq!(usage.output_tokens, 1024);
    /// ```
    pub fn new(input_tokens: u64, output_tokens: u64) -> Self {
        Self {
            input_tokens,
            output_tokens,
        }
    }

    /// Returns the total token count (input + output).
    ///
    /// # Returns
    ///
    /// The sum of `input_tokens` and `output_tokens`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::output::TokenUsage;
    ///
    /// let usage = TokenUsage::new(300, 700);
    /// assert_eq!(usage.total(), 1000);
    /// ```
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

// ---------------------------------------------------------------------------
// PluginOutput
// ---------------------------------------------------------------------------

/// The complete output produced by a plugin run.
///
/// Returned by [`WorkflowPlugin::run`][crate::plugins::trait_def::WorkflowPlugin::run]
/// and persisted to the workspace state. Accumulates findings, written files,
/// named scores, report paths, diagnostics, risk classification, and optional
/// provider metadata and token usage.
///
/// Use [`PluginOutput::success`] or [`PluginOutput::failure`] to construct an
/// instance, then use the builder-style mutation methods to populate it.
///
/// # Examples
///
/// ```
/// use xzardgz::plugins::output::PluginOutput;
/// use xzardgz::reports::findings::PluginFinding;
/// use xzardgz::reports::risk_band::RiskBand;
/// use xzardgz::scanner::findings::FindingSeverity;
///
/// let mut output = PluginOutput::success("analysis complete");
/// assert!(output.completed);
/// output.add_finding(PluginFinding::new(
///     "example",
///     "Example Finding",
///     "An example finding.",
///     FindingSeverity::High,
///     0.9,
/// ));
/// assert_eq!(output.finding_count(), 1);
/// assert_eq!(output.risk_band, Some(RiskBand::High));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginOutput {
    /// One-line human-readable summary of the plugin result.
    pub summary: String,
    /// Paths of all files written by this plugin run.
    pub written_files: Vec<String>,
    /// Findings produced during this run.
    pub findings: Vec<PluginFinding>,
    /// Whether the plugin run completed successfully.
    pub completed: bool,
    /// Structured diagnostics collected during the run.
    pub diagnostics: Diagnostics,
    /// Named numeric scores (e.g. `"quality_score"`, `"security_score"`).
    pub scores: HashMap<String, f64>,
    /// Overall risk classification derived from findings.
    pub risk_band: Option<RiskBand>,
    /// Map of format label to list of written report paths.
    pub report_paths: HashMap<String, Vec<String>>,
    /// Metadata from the AI provider used for this run.
    pub provider_metadata: Option<ProviderMetadata>,
    /// Token usage for this run, if reported by the provider.
    pub token_usage: Option<TokenUsage>,
}

impl PluginOutput {
    /// Creates a successful `PluginOutput` with all collections empty.
    ///
    /// Sets `completed = true`.
    ///
    /// # Arguments
    ///
    /// * `summary` - One-line description of the successful result.
    ///
    /// # Returns
    ///
    /// A new `PluginOutput` with `completed = true` and empty collections.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::output::PluginOutput;
    ///
    /// let output = PluginOutput::success("review complete");
    /// assert!(output.completed);
    /// assert_eq!(output.summary, "review complete");
    /// assert!(output.findings.is_empty());
    /// assert!(output.written_files.is_empty());
    /// ```
    pub fn success(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            written_files: Vec::new(),
            findings: Vec::new(),
            completed: true,
            diagnostics: Diagnostics::new(),
            scores: HashMap::new(),
            risk_band: None,
            report_paths: HashMap::new(),
            provider_metadata: None,
            token_usage: None,
        }
    }

    /// Creates a failed `PluginOutput` with all collections empty.
    ///
    /// Sets `completed = false`.
    ///
    /// # Arguments
    ///
    /// * `summary` - One-line description of the failure reason.
    ///
    /// # Returns
    ///
    /// A new `PluginOutput` with `completed = false` and empty collections.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::output::PluginOutput;
    ///
    /// let output = PluginOutput::failure("provider unavailable");
    /// assert!(!output.completed);
    /// assert_eq!(output.summary, "provider unavailable");
    /// ```
    pub fn failure(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            written_files: Vec::new(),
            findings: Vec::new(),
            completed: false,
            diagnostics: Diagnostics::new(),
            scores: HashMap::new(),
            risk_band: None,
            report_paths: HashMap::new(),
            provider_metadata: None,
            token_usage: None,
        }
    }

    /// Appends a finding to this output and recomputes `risk_band`.
    ///
    /// After the finding is appended the `risk_band` field is updated to
    /// reflect the highest severity across all findings in `self.findings`.
    ///
    /// # Arguments
    ///
    /// * `finding` - The [`PluginFinding`] to append.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::output::PluginOutput;
    /// use xzardgz::reports::findings::PluginFinding;
    /// use xzardgz::reports::risk_band::RiskBand;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let mut output = PluginOutput::success("done");
    /// assert!(output.risk_band.is_none());
    ///
    /// output.add_finding(PluginFinding::new("k", "T", "D", FindingSeverity::High, 0.8));
    /// assert_eq!(output.finding_count(), 1);
    /// assert_eq!(output.risk_band, Some(RiskBand::High));
    ///
    /// output.add_finding(PluginFinding::new("k2", "T2", "D2", FindingSeverity::Critical, 0.95));
    /// assert_eq!(output.risk_band, Some(RiskBand::Critical));
    /// ```
    pub fn add_finding(&mut self, finding: PluginFinding) {
        self.findings.push(finding);
        let mut pf = PluginFindings::new();
        for f in &self.findings {
            pf.push(f.clone());
        }
        self.risk_band = pf.to_risk_band();
    }

    /// Appends a written file path to this output.
    ///
    /// # Arguments
    ///
    /// * `path` - Repository-relative or workspace-relative path of the written file.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::output::PluginOutput;
    ///
    /// let mut output = PluginOutput::success("done");
    /// output.add_written_file("workspace/reports/review.md");
    /// assert_eq!(output.written_files, vec!["workspace/reports/review.md"]);
    /// ```
    pub fn add_written_file(&mut self, path: impl Into<String>) {
        self.written_files.push(path.into());
    }

    /// Appends a report path under the given format label.
    ///
    /// If no entry exists for `format`, a new `Vec` is created. Otherwise the
    /// path is appended to the existing list.
    ///
    /// # Arguments
    ///
    /// * `format` - Format label (e.g. `"markdown"`, `"json"`, `"sarif"`).
    /// * `path` - Path of the report file written in this format.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::output::PluginOutput;
    ///
    /// let mut output = PluginOutput::success("done");
    /// output.add_report_path("markdown", "reports/review.md");
    /// output.add_report_path("json", "reports/review.json");
    /// output.add_report_path("markdown", "reports/summary.md");
    ///
    /// assert_eq!(output.report_paths["markdown"].len(), 2);
    /// assert_eq!(output.report_paths["json"].len(), 1);
    /// ```
    pub fn add_report_path(&mut self, format: impl Into<String>, path: impl Into<String>) {
        self.report_paths
            .entry(format.into())
            .or_default()
            .push(path.into());
    }

    /// Stores a named numeric score, overwriting any previous value for that name.
    ///
    /// # Arguments
    ///
    /// * `name` - Score key (e.g. `"quality_score"`, `"security_score"`).
    /// * `score` - The numeric value to store.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::output::PluginOutput;
    ///
    /// let mut output = PluginOutput::success("done");
    /// output.set_score("quality_score", 0.85);
    /// assert!((output.scores["quality_score"] - 0.85).abs() < f64::EPSILON);
    /// ```
    pub fn set_score(&mut self, name: impl Into<String>, score: f64) {
        self.scores.insert(name.into(), score);
    }

    /// Returns the total number of findings in this output.
    ///
    /// # Returns
    ///
    /// The length of `self.findings`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::output::PluginOutput;
    ///
    /// let output = PluginOutput::success("done");
    /// assert_eq!(output.finding_count(), 0);
    /// ```
    pub fn finding_count(&self) -> usize {
        self.findings.len()
    }

    /// Returns the overall risk band derived from all findings, or `None` if
    /// no findings have been added yet.
    ///
    /// This is a direct accessor for the `risk_band` field, which is
    /// automatically maintained by [`add_finding`][Self::add_finding].
    ///
    /// # Returns
    ///
    /// A copy of the `risk_band` field.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::output::PluginOutput;
    ///
    /// let output = PluginOutput::success("done");
    /// assert!(output.highest_risk().is_none());
    /// ```
    pub fn highest_risk(&self) -> Option<RiskBand> {
        self.risk_band
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::findings::PluginFinding;
    use crate::scanner::findings::FindingSeverity;

    /// Builds a minimal [`PluginFinding`] with the given severity for testing.
    fn make_finding(severity: FindingSeverity) -> PluginFinding {
        PluginFinding::new(
            "test_kind",
            "Test Title",
            "Test description.",
            severity,
            0.8,
        )
    }

    // ------------------------------------------------------------------
    // success / failure constructors
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_output_success_sets_completed_true() {
        let output = PluginOutput::success("all good");
        assert!(output.completed);
        assert_eq!(output.summary, "all good");
        assert!(output.findings.is_empty());
        assert!(output.written_files.is_empty());
        assert!(output.scores.is_empty());
        assert!(output.report_paths.is_empty());
        assert!(output.diagnostics.is_empty());
        assert!(output.risk_band.is_none());
        assert!(output.provider_metadata.is_none());
        assert!(output.token_usage.is_none());
    }

    #[test]
    fn test_plugin_output_failure_sets_completed_false() {
        let output = PluginOutput::failure("provider error");
        assert!(!output.completed);
        assert_eq!(output.summary, "provider error");
        assert!(output.findings.is_empty());
    }

    // ------------------------------------------------------------------
    // add_finding
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_output_add_finding_increases_count() {
        let mut output = PluginOutput::success("done");
        assert_eq!(output.finding_count(), 0);
        output.add_finding(make_finding(FindingSeverity::Low));
        assert_eq!(output.finding_count(), 1);
        output.add_finding(make_finding(FindingSeverity::High));
        assert_eq!(output.finding_count(), 2);
    }

    #[test]
    fn test_plugin_output_add_finding_updates_risk_band() {
        let mut output = PluginOutput::success("done");
        assert!(output.risk_band.is_none());

        output.add_finding(make_finding(FindingSeverity::Medium));
        assert_eq!(output.risk_band, Some(RiskBand::Medium));

        // Adding a higher-severity finding escalates the risk band.
        output.add_finding(make_finding(FindingSeverity::Critical));
        assert_eq!(output.risk_band, Some(RiskBand::Critical));
    }

    // ------------------------------------------------------------------
    // add_written_file
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_output_add_written_file_appends() {
        let mut output = PluginOutput::success("done");
        output.add_written_file("reports/result.md");
        output.add_written_file("reports/result.json");
        assert_eq!(
            output.written_files,
            vec!["reports/result.md", "reports/result.json"]
        );
    }

    // ------------------------------------------------------------------
    // add_report_path
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_output_add_report_path_appends() {
        let mut output = PluginOutput::success("done");
        output.add_report_path("markdown", "reports/review.md");
        output.add_report_path("json", "reports/review.json");
        output.add_report_path("markdown", "reports/summary.md");

        assert_eq!(
            output.report_paths["markdown"],
            vec!["reports/review.md", "reports/summary.md"]
        );
        assert_eq!(output.report_paths["json"], vec!["reports/review.json"]);
    }

    // ------------------------------------------------------------------
    // set_score
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_output_set_score_stores_value() {
        let mut output = PluginOutput::success("done");
        output.set_score("quality_score", 0.92);
        assert!((output.scores["quality_score"] - 0.92).abs() < f64::EPSILON);
    }

    // ------------------------------------------------------------------
    // TokenUsage
    // ------------------------------------------------------------------

    #[test]
    fn test_token_usage_total_sums_tokens() {
        let usage = TokenUsage::new(150, 350);
        assert_eq!(usage.total(), 500);
    }

    #[test]
    fn test_token_usage_new_sets_fields() {
        let usage = TokenUsage::new(100, 200);
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 200);
    }

    #[test]
    fn test_token_usage_default_is_zero() {
        let usage = TokenUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.total(), 0);
    }

    // ------------------------------------------------------------------
    // highest_risk
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_output_highest_risk_returns_none_when_no_findings() {
        let output = PluginOutput::success("done");
        assert!(output.highest_risk().is_none());
    }

    #[test]
    fn test_plugin_output_highest_risk_returns_current_risk_band() {
        let mut output = PluginOutput::success("done");
        output.add_finding(make_finding(FindingSeverity::High));
        assert_eq!(output.highest_risk(), Some(RiskBand::High));
    }
}
