//! Plugin finding types for report generation.
//!
//! This module defines [`PluginFinding`] (a single issue surfaced by a plugin)
//! and [`PluginFindings`] (an ordered collection of findings). Together they
//! form the core payload carried inside a [`crate::reports::envelope::ReportEnvelope`].

use crate::reports::risk_band::RiskBand;
use crate::scanner::findings::FindingSeverity;
use crate::scanner::scoring::ScoringResult;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Private serde defaults
// ---------------------------------------------------------------------------

/// Returns the default `static_score` for a [`PluginFinding`] that was not
/// produced via the confidence scorer (full confidence assumed).
fn default_finding_static_score() -> f64 {
    1.0
}

// ---------------------------------------------------------------------------
// PluginFinding
// ---------------------------------------------------------------------------

/// A single finding produced by a plugin during analysis.
///
/// A finding combines a machine-readable `kind` identifier with human-readable
/// title and description text, an optional source location, a severity
/// classification, an AI confidence score, and free-form tags.
///
/// # Examples
///
/// ```
/// use xzardgz::reports::findings::PluginFinding;
/// use xzardgz::scanner::findings::FindingSeverity;
///
/// let finding = PluginFinding::new(
///     "hardcoded_secret",
///     "Hardcoded API key",
///     "An API key was found embedded in source code.",
///     FindingSeverity::Critical,
///     0.95,
/// )
/// .with_location("src/config.rs", Some(42))
/// .with_tags(vec!["secrets".to_string(), "pii".to_string()]);
///
/// assert_eq!(finding.kind, "hardcoded_secret");
/// assert_eq!(finding.severity, FindingSeverity::Critical);
/// assert_eq!(finding.line, Some(42));
/// assert_eq!(finding.tags.len(), 2);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginFinding {
    /// Category or rule identifier (e.g. `"hardcoded_secret"`, `"sql_injection"`).
    pub kind: String,
    /// Short human-readable title.
    pub title: String,
    /// Detailed description of the finding.
    pub description: String,
    /// Repository-relative file path, if applicable.
    pub file_path: Option<String>,
    /// 1-based line number, if known.
    pub line: Option<u32>,
    /// Severity classification.
    pub severity: FindingSeverity,
    /// AI or scanner confidence in the range `[0.0, 1.0]`.
    pub confidence: f64,
    /// Optional tags for grouping or filtering.
    pub tags: Vec<String>,
    /// Static confidence score produced by the baseline fold, before the AI
    /// blend step.
    ///
    /// Defaults to `1.0` for findings created by code paths that do not use
    /// [`ConfidenceScorer`][crate::scanner::scoring::ConfidenceScorer].  Set
    /// via [`with_scoring`][Self::with_scoring].
    #[serde(default = "default_finding_static_score")]
    pub static_score: f64,
    /// Raw AI-reported confidence for this finding, if the AI leg was active
    /// and returned a valid score.
    ///
    /// `None` when `ai_analysis_enabled` was `false` or the AI call failed.
    /// Set via [`with_scoring`][Self::with_scoring].
    #[serde(default)]
    pub ai_score: Option<f64>,
}

impl PluginFinding {
    /// Creates a new `PluginFinding` with the required fields.
    ///
    /// The optional `file_path` and `line` fields default to `None` and `tags`
    /// defaults to an empty vector. Use [`PluginFinding::with_location`] and
    /// [`PluginFinding::with_tags`] to populate them in a builder style.
    ///
    /// # Arguments
    ///
    /// * `kind`        - Category or rule identifier.
    /// * `title`       - Short human-readable title.
    /// * `description` - Detailed description.
    /// * `severity`    - Severity classification.
    /// * `confidence`  - AI confidence in the range `[0.0, 1.0]`.
    ///
    /// # Returns
    ///
    /// A new [`PluginFinding`] with `file_path = None`, `line = None`, and
    /// `tags = []`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::findings::PluginFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = PluginFinding::new(
    ///     "sql_injection",
    ///     "SQL Injection",
    ///     "Unsanitized input passed to SQL query.",
    ///     FindingSeverity::High,
    ///     0.88,
    /// );
    /// assert_eq!(f.kind, "sql_injection");
    /// assert!(f.file_path.is_none());
    /// assert!(f.tags.is_empty());
    /// ```
    pub fn new(
        kind: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        severity: FindingSeverity,
        confidence: f64,
    ) -> Self {
        Self {
            kind: kind.into(),
            title: title.into(),
            description: description.into(),
            file_path: None,
            line: None,
            severity,
            confidence,
            tags: Vec::new(),
            static_score: 1.0,
            ai_score: None,
        }
    }

    /// Attaches a source file location to this finding.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Repository-relative path of the affected file.
    /// * `line`      - 1-based line number, or `None` if the exact line is unknown.
    ///
    /// # Returns
    ///
    /// `self` with `file_path` and `line` populated.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::findings::PluginFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = PluginFinding::new("xss", "XSS", "Cross-site scripting.", FindingSeverity::High, 0.7)
    ///     .with_location("templates/index.html", Some(55));
    ///
    /// assert_eq!(f.file_path.as_deref(), Some("templates/index.html"));
    /// assert_eq!(f.line, Some(55));
    /// ```
    pub fn with_location(mut self, file_path: impl Into<String>, line: Option<u32>) -> Self {
        self.file_path = Some(file_path.into());
        self.line = line;
        self
    }

    /// Attaches tags to this finding.
    ///
    /// # Arguments
    ///
    /// * `tags` - Free-form strings used for grouping or filtering.
    ///
    /// # Returns
    ///
    /// `self` with the provided tags set.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::findings::PluginFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = PluginFinding::new("owasp_a3", "Sensitive Exposure", "...", FindingSeverity::Medium, 0.5)
    ///     .with_tags(vec!["owasp".to_string(), "pii".to_string()]);
    ///
    /// assert_eq!(f.tags, vec!["owasp", "pii"]);
    /// ```
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Updates the confidence audit fields from a [`ScoringResult`].
    ///
    /// Sets `confidence` to the blended score, `static_score` to the
    /// static fold result, and `ai_score` to the AI-leg result.  Call
    /// this after [`ConfidenceScorer::score`][crate::scanner::scoring::ConfidenceScorer::score]
    /// has been computed for this finding.
    ///
    /// # Arguments
    ///
    /// * `result` - The [`ScoringResult`] produced by the confidence scorer.
    ///
    /// # Returns
    ///
    /// `self` with confidence audit fields updated.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::findings::PluginFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    /// use xzardgz::scanner::scoring::ScoringResult;
    ///
    /// let result = ScoringResult {
    ///     static_score: 0.4,
    ///     ai_score: Some(0.9),
    ///     blended_score: 0.65,
    ///     violation_reasons: vec![],
    /// };
    ///
    /// let f = PluginFinding::new(
    ///     "injection", "Title", "Desc", FindingSeverity::High, 0.9,
    /// )
    /// .with_scoring(&result);
    ///
    /// assert!((f.confidence - 0.65).abs() < 1e-9);
    /// assert!((f.static_score - 0.4).abs() < 1e-9);
    /// assert_eq!(f.ai_score, Some(0.9));
    /// ```
    pub fn with_scoring(mut self, result: &ScoringResult) -> Self {
        self.confidence = result.blended_score;
        self.static_score = result.static_score;
        self.ai_score = result.ai_score;
        self
    }
}

// ---------------------------------------------------------------------------
// PluginFindings
// ---------------------------------------------------------------------------

/// Aggregated collection of findings produced by a plugin.
///
/// Provides convenience methods for filtering by severity, computing the
/// overall highest severity, and deriving a [`RiskBand`].
///
/// # Examples
///
/// ```
/// use xzardgz::reports::findings::{PluginFinding, PluginFindings};
/// use xzardgz::reports::risk_band::RiskBand;
/// use xzardgz::scanner::findings::FindingSeverity;
///
/// let mut findings = PluginFindings::new();
/// findings.push(PluginFinding::new("a", "Title A", "Desc", FindingSeverity::High, 0.8));
/// findings.push(PluginFinding::new("b", "Title B", "Desc", FindingSeverity::Low, 0.2));
///
/// assert_eq!(findings.len(), 2);
/// assert_eq!(findings.highest_severity(), Some(FindingSeverity::High));
/// assert_eq!(findings.to_risk_band(), Some(RiskBand::High));
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginFindings {
    /// Ordered list of findings collected from the plugin run.
    pub findings: Vec<PluginFinding>,
}

impl PluginFindings {
    /// Creates a new, empty `PluginFindings` collection.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::findings::PluginFindings;
    ///
    /// let findings = PluginFindings::new();
    /// assert!(findings.is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a finding to the collection.
    ///
    /// # Arguments
    ///
    /// * `finding` - The [`PluginFinding`] to append.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::findings::{PluginFinding, PluginFindings};
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let mut findings = PluginFindings::new();
    /// findings.push(PluginFinding::new("k", "T", "D", FindingSeverity::Low, 0.1));
    /// assert_eq!(findings.len(), 1);
    /// ```
    pub fn push(&mut self, finding: PluginFinding) {
        self.findings.push(finding);
    }

    /// Returns references to all findings whose severity matches `severity`.
    ///
    /// # Arguments
    ///
    /// * `severity` - The severity level to filter on.
    ///
    /// # Returns
    ///
    /// A `Vec` of references to the matching [`PluginFinding`]s.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::findings::{PluginFinding, PluginFindings};
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let mut findings = PluginFindings::new();
    /// findings.push(PluginFinding::new("a", "T", "D", FindingSeverity::High, 0.8));
    /// findings.push(PluginFinding::new("b", "T", "D", FindingSeverity::Low, 0.2));
    /// findings.push(PluginFinding::new("c", "T", "D", FindingSeverity::High, 0.9));
    ///
    /// assert_eq!(findings.by_severity(FindingSeverity::High).len(), 2);
    /// assert_eq!(findings.by_severity(FindingSeverity::Low).len(), 1);
    /// ```
    pub fn by_severity(&self, severity: FindingSeverity) -> Vec<&PluginFinding> {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .collect()
    }

    /// Returns the highest severity present across all findings, or `None` if
    /// the collection is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::findings::{PluginFinding, PluginFindings};
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let mut findings = PluginFindings::new();
    /// assert!(findings.highest_severity().is_none());
    ///
    /// findings.push(PluginFinding::new("a", "T", "D", FindingSeverity::Medium, 0.5));
    /// findings.push(PluginFinding::new("b", "T", "D", FindingSeverity::Critical, 0.9));
    /// assert_eq!(findings.highest_severity(), Some(FindingSeverity::Critical));
    /// ```
    pub fn highest_severity(&self) -> Option<FindingSeverity> {
        self.findings.iter().map(|f| f.severity).max()
    }

    /// Returns the total number of findings in the collection.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::findings::PluginFindings;
    ///
    /// let findings = PluginFindings::new();
    /// assert_eq!(findings.len(), 0);
    /// ```
    pub fn len(&self) -> usize {
        self.findings.len()
    }

    /// Returns `true` if the collection contains no findings.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::findings::PluginFindings;
    ///
    /// let findings = PluginFindings::new();
    /// assert!(findings.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }

    /// Derives a [`RiskBand`] from the highest-severity finding, or returns
    /// `None` if the collection is empty.
    ///
    /// | Highest severity | Risk band            |
    /// |-----------------|----------------------|
    /// | `Info` or `Low` | [`RiskBand::Low`]    |
    /// | `Medium`        | [`RiskBand::Medium`] |
    /// | `High`          | [`RiskBand::High`]   |
    /// | `Critical`      | [`RiskBand::Critical`] |
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::findings::{PluginFinding, PluginFindings};
    /// use xzardgz::reports::risk_band::RiskBand;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let mut findings = PluginFindings::new();
    /// assert!(findings.to_risk_band().is_none());
    ///
    /// findings.push(PluginFinding::new("a", "T", "D", FindingSeverity::Info, 0.1));
    /// assert_eq!(findings.to_risk_band(), Some(RiskBand::Low));
    /// ```
    pub fn to_risk_band(&self) -> Option<RiskBand> {
        let sev = self.highest_severity()?;
        let band = match sev {
            FindingSeverity::Info | FindingSeverity::Low => RiskBand::Low,
            FindingSeverity::Medium => RiskBand::Medium,
            FindingSeverity::High => RiskBand::High,
            FindingSeverity::Critical => RiskBand::Critical,
        };
        Some(band)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::scoring::ScoringResult;

    // ------------------------------------------------------------------
    // PluginFinding::new
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_finding_new_sets_required_fields() {
        let f = PluginFinding::new(
            "sql_injection",
            "SQL Injection",
            "Unsanitized input.",
            FindingSeverity::High,
            0.88,
        );
        assert_eq!(f.kind, "sql_injection");
        assert_eq!(f.title, "SQL Injection");
        assert_eq!(f.description, "Unsanitized input.");
        assert_eq!(f.severity, FindingSeverity::High);
        assert!((f.confidence - 0.88).abs() < f64::EPSILON);
    }

    #[test]
    fn test_plugin_finding_new_optional_fields_default_to_none_and_empty() {
        let f = PluginFinding::new("k", "t", "d", FindingSeverity::Low, 0.1);
        assert!(f.file_path.is_none());
        assert!(f.line.is_none());
        assert!(f.tags.is_empty());
    }

    // ------------------------------------------------------------------
    // PluginFinding::with_location
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_finding_with_location_sets_file_and_line() {
        let f = PluginFinding::new("k", "t", "d", FindingSeverity::Medium, 0.5)
            .with_location("src/lib.rs", Some(42));
        assert_eq!(f.file_path.as_deref(), Some("src/lib.rs"));
        assert_eq!(f.line, Some(42));
    }

    #[test]
    fn test_plugin_finding_with_location_accepts_none_line() {
        let f = PluginFinding::new("k", "t", "d", FindingSeverity::Low, 0.2)
            .with_location("main.py", None);
        assert_eq!(f.file_path.as_deref(), Some("main.py"));
        assert!(f.line.is_none());
    }

    // ------------------------------------------------------------------
    // PluginFinding::with_tags
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_finding_with_tags_sets_tags() {
        let f = PluginFinding::new("k", "t", "d", FindingSeverity::Critical, 0.9)
            .with_tags(vec!["owasp".to_string(), "pii".to_string()]);
        assert_eq!(f.tags, vec!["owasp", "pii"]);
    }

    #[test]
    fn test_plugin_finding_with_tags_empty_vec_clears_tags() {
        let f = PluginFinding::new("k", "t", "d", FindingSeverity::Low, 0.1).with_tags(vec![]);
        assert!(f.tags.is_empty());
    }

    // ------------------------------------------------------------------
    // with_scoring
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_finding_with_scoring_sets_blended_confidence() {
        let result = ScoringResult {
            static_score: 0.4,
            ai_score: Some(0.9),
            blended_score: 0.65,
            violation_reasons: vec![],
        };
        let f = PluginFinding::new("k", "t", "d", FindingSeverity::High, 0.9).with_scoring(&result);
        assert!((f.confidence - 0.65).abs() < 1e-9);
    }

    #[test]
    fn test_plugin_finding_with_scoring_sets_static_score() {
        let result = ScoringResult {
            static_score: 0.4,
            ai_score: Some(0.9),
            blended_score: 0.65,
            violation_reasons: vec![],
        };
        let f = PluginFinding::new("k", "t", "d", FindingSeverity::High, 0.9).with_scoring(&result);
        assert!((f.static_score - 0.4).abs() < 1e-9);
    }

    #[test]
    fn test_plugin_finding_with_scoring_sets_ai_score() {
        let result = ScoringResult {
            static_score: 0.4,
            ai_score: Some(0.9),
            blended_score: 0.65,
            violation_reasons: vec![],
        };
        let f = PluginFinding::new("k", "t", "d", FindingSeverity::High, 0.9).with_scoring(&result);
        assert_eq!(f.ai_score, Some(0.9));
    }

    #[test]
    fn test_plugin_finding_with_scoring_none_ai_score() {
        let result = ScoringResult {
            static_score: 0.8,
            ai_score: None,
            blended_score: 0.8,
            violation_reasons: vec![],
        };
        let f =
            PluginFinding::new("k", "t", "d", FindingSeverity::Medium, 0.5).with_scoring(&result);
        assert!(f.ai_score.is_none());
        assert!((f.confidence - 0.8).abs() < 1e-9);
    }

    #[test]
    fn test_plugin_finding_default_static_score_is_one() {
        let f = PluginFinding::new("k", "t", "d", FindingSeverity::Low, 0.5);
        assert!((f.static_score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_plugin_finding_default_ai_score_is_none() {
        let f = PluginFinding::new("k", "t", "d", FindingSeverity::Low, 0.5);
        assert!(f.ai_score.is_none());
    }

    #[test]
    fn test_plugin_finding_with_scoring_serde_roundtrip() {
        let result = ScoringResult {
            static_score: 0.4,
            ai_score: Some(0.85),
            blended_score: 0.625,
            violation_reasons: vec![],
        };
        let f = PluginFinding::new("injection", "Title", "Desc", FindingSeverity::High, 0.85)
            .with_scoring(&result);
        // SAFETY: PluginFinding with valid data cannot fail serialization.
        let json = serde_json::to_string(&f).unwrap();
        // SAFETY: we just serialized this.
        let restored: PluginFinding = serde_json::from_str(&json).unwrap();
        assert!((restored.static_score - 0.4).abs() < 1e-9);
        assert_eq!(restored.ai_score, Some(0.85));
        assert!((restored.confidence - 0.625).abs() < 1e-9);
    }

    // ------------------------------------------------------------------
    // PluginFindings collection
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_findings_new_creates_empty_collection() {
        let findings = PluginFindings::new();
        assert!(findings.is_empty());
        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn test_plugin_findings_push_increases_len() {
        let mut findings = PluginFindings::new();
        findings.push(PluginFinding::new("a", "t", "d", FindingSeverity::Low, 0.1));
        assert_eq!(findings.len(), 1);
        findings.push(PluginFinding::new(
            "b",
            "t",
            "d",
            FindingSeverity::High,
            0.8,
        ));
        assert_eq!(findings.len(), 2);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_plugin_findings_by_severity_returns_only_matching() {
        let mut findings = PluginFindings::new();
        findings.push(PluginFinding::new(
            "a",
            "t",
            "d",
            FindingSeverity::High,
            0.8,
        ));
        findings.push(PluginFinding::new("b", "t", "d", FindingSeverity::Low, 0.2));
        findings.push(PluginFinding::new(
            "c",
            "t",
            "d",
            FindingSeverity::High,
            0.9,
        ));
        findings.push(PluginFinding::new(
            "d",
            "t",
            "d",
            FindingSeverity::Medium,
            0.5,
        ));

        let high = findings.by_severity(FindingSeverity::High);
        assert_eq!(high.len(), 2);
        assert!(high.iter().all(|f| f.severity == FindingSeverity::High));

        let low = findings.by_severity(FindingSeverity::Low);
        assert_eq!(low.len(), 1);

        let critical = findings.by_severity(FindingSeverity::Critical);
        assert!(critical.is_empty());
    }

    #[test]
    fn test_plugin_findings_highest_severity_empty_returns_none() {
        let findings = PluginFindings::new();
        assert!(findings.highest_severity().is_none());
    }

    #[test]
    fn test_plugin_findings_highest_severity_returns_max_severity() {
        let mut findings = PluginFindings::new();
        findings.push(PluginFinding::new("a", "t", "d", FindingSeverity::Low, 0.2));
        findings.push(PluginFinding::new(
            "b",
            "t",
            "d",
            FindingSeverity::High,
            0.7,
        ));
        findings.push(PluginFinding::new(
            "c",
            "t",
            "d",
            FindingSeverity::Medium,
            0.5,
        ));
        assert_eq!(findings.highest_severity(), Some(FindingSeverity::High));
    }

    #[test]
    fn test_plugin_findings_highest_severity_single_finding() {
        let mut findings = PluginFindings::new();
        findings.push(PluginFinding::new(
            "a",
            "t",
            "d",
            FindingSeverity::Critical,
            0.95,
        ));
        assert_eq!(findings.highest_severity(), Some(FindingSeverity::Critical));
    }

    // ------------------------------------------------------------------
    // PluginFindings::to_risk_band
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_findings_to_risk_band_empty_returns_none() {
        let findings = PluginFindings::new();
        assert!(findings.to_risk_band().is_none());
    }

    #[test]
    fn test_plugin_findings_to_risk_band_info_returns_low() {
        let mut findings = PluginFindings::new();
        findings.push(PluginFinding::new(
            "a",
            "t",
            "d",
            FindingSeverity::Info,
            0.05,
        ));
        assert_eq!(findings.to_risk_band(), Some(RiskBand::Low));
    }

    #[test]
    fn test_plugin_findings_to_risk_band_low_returns_low() {
        let mut findings = PluginFindings::new();
        findings.push(PluginFinding::new(
            "a",
            "t",
            "d",
            FindingSeverity::Low,
            0.15,
        ));
        assert_eq!(findings.to_risk_band(), Some(RiskBand::Low));
    }

    #[test]
    fn test_plugin_findings_to_risk_band_medium_returns_medium() {
        let mut findings = PluginFindings::new();
        findings.push(PluginFinding::new(
            "a",
            "t",
            "d",
            FindingSeverity::Medium,
            0.4,
        ));
        assert_eq!(findings.to_risk_band(), Some(RiskBand::Medium));
    }

    #[test]
    fn test_plugin_findings_to_risk_band_high_returns_high() {
        let mut findings = PluginFindings::new();
        findings.push(PluginFinding::new(
            "a",
            "t",
            "d",
            FindingSeverity::High,
            0.7,
        ));
        assert_eq!(findings.to_risk_band(), Some(RiskBand::High));
    }

    #[test]
    fn test_plugin_findings_to_risk_band_critical_returns_critical() {
        let mut findings = PluginFindings::new();
        findings.push(PluginFinding::new(
            "a",
            "t",
            "d",
            FindingSeverity::Critical,
            0.95,
        ));
        assert_eq!(findings.to_risk_band(), Some(RiskBand::Critical));
    }

    #[test]
    fn test_plugin_findings_to_risk_band_uses_highest_severity() {
        let mut findings = PluginFindings::new();
        findings.push(PluginFinding::new("a", "t", "d", FindingSeverity::Low, 0.2));
        findings.push(PluginFinding::new(
            "b",
            "t",
            "d",
            FindingSeverity::High,
            0.8,
        ));
        findings.push(PluginFinding::new(
            "c",
            "t",
            "d",
            FindingSeverity::Medium,
            0.5,
        ));
        // Highest is High, so result is RiskBand::High.
        assert_eq!(findings.to_risk_band(), Some(RiskBand::High));
    }

    // ------------------------------------------------------------------
    // Serde round-trip
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_findings_serde_roundtrip() {
        let mut findings = PluginFindings::new();
        findings.push(
            PluginFinding::new(
                "xss",
                "XSS",
                "Cross-site scripting.",
                FindingSeverity::High,
                0.8,
            )
            .with_location("templates/index.html", Some(12))
            .with_tags(vec!["owasp".to_string()]),
        );
        // SAFETY: PluginFindings with valid data cannot fail serialization.
        let json = serde_json::to_string(&findings).unwrap();
        // SAFETY: we serialized this ourselves so it is valid JSON.
        let restored: PluginFindings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored.findings[0].kind, "xss");
        assert_eq!(restored.findings[0].severity, FindingSeverity::High);
        assert_eq!(restored.findings[0].line, Some(12));
    }
}
