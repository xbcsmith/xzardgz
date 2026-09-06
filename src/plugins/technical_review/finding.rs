//! The [`TechnicalReviewFinding`] data type.
//!
//! A finding captures a single observation made by the technical review plugin
//! against a specific review dimension.  It records what was observed
//! (`evidence`), why it matters (`impact`), and how to address it
//! (`recommendation`), together with source location, AI confidence, and
//! cross-reference metadata.
//!
//! Use [`TechnicalReviewFinding::from_json_value`] to parse AI responses and
//! [`TechnicalReviewFinding::to_plugin_finding`] to convert to the common
//! report format.

use serde::{Deserialize, Serialize};

use crate::reports::findings::PluginFinding;
use crate::scanner::findings::FindingSeverity;

// ---------------------------------------------------------------------------
// TechnicalReviewFinding
// ---------------------------------------------------------------------------

/// A finding produced by the technical review plugin.
///
/// Each finding is associated with one [`ReviewDimension`][super::dimensions::ReviewDimension]
/// category and records evidence, impact, and a concrete recommendation for the
/// codebase being reviewed.
///
/// # Examples
///
/// ```
/// use xzardgz::plugins::technical_review::finding::TechnicalReviewFinding;
/// use xzardgz::scanner::findings::FindingSeverity;
///
/// let f = TechnicalReviewFinding::new(
///     "architecture",
///     FindingSeverity::High,
///     "Circular dependency between modules A and B.",
///     "Increases coupling and makes unit testing harder.",
///     "Extract the shared logic into a dedicated utility crate.",
///     0.85,
/// );
/// assert_eq!(f.category, "architecture");
/// assert_eq!(f.severity, FindingSeverity::High);
/// assert!(f.file.is_none());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalReviewFinding {
    /// Review dimension category (e.g. `"architecture"`, `"error_handling"`).
    pub category: String,
    /// Severity of the finding.
    pub severity: FindingSeverity,
    /// Repository-relative file path, if applicable.
    pub file: Option<String>,
    /// 1-based line number, if known.
    pub line: Option<u32>,
    /// Symbol name (function, struct, module) where applicable.
    pub symbol: Option<String>,
    /// What was observed in the code.
    pub evidence: String,
    /// Why this matters for the codebase.
    pub impact: String,
    /// Concrete steps to address the finding.
    pub recommendation: String,
    /// AI confidence in `[0.0, 1.0]`.
    pub confidence: f64,
    /// Other files related to this finding.
    pub related_files: Vec<String>,
    /// External references (RFCs, docs, best-practice links).
    pub references: Vec<String>,
}

impl TechnicalReviewFinding {
    /// Creates a new finding with the required fields.
    ///
    /// Optional fields default to `None` or empty collections.  Use the
    /// builder methods [`with_location`][Self::with_location],
    /// [`with_symbol`][Self::with_symbol],
    /// [`with_related_files`][Self::with_related_files], and
    /// [`with_references`][Self::with_references] to populate them.
    ///
    /// # Arguments
    ///
    /// * `category`       - Dimension category identifier (e.g. `"architecture"`).
    /// * `severity`       - Severity classification.
    /// * `evidence`       - Observation text: what was found in the code.
    /// * `impact`         - Why this observation matters.
    /// * `recommendation` - Concrete remediation steps.
    /// * `confidence`     - AI confidence in `[0.0, 1.0]`.
    ///
    /// # Returns
    ///
    /// A `TechnicalReviewFinding` with `file = None`, `line = None`,
    /// `symbol = None`, and empty `related_files` / `references`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::finding::TechnicalReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = TechnicalReviewFinding::new(
    ///     "error_handling",
    ///     FindingSeverity::Medium,
    ///     "Several Result values are silently discarded.",
    ///     "Errors will go undetected in production.",
    ///     "Propagate errors with the ? operator or log them explicitly.",
    ///     0.75,
    /// );
    /// assert!(f.file.is_none());
    /// assert!(f.related_files.is_empty());
    /// ```
    pub fn new(
        category: impl Into<String>,
        severity: FindingSeverity,
        evidence: impl Into<String>,
        impact: impl Into<String>,
        recommendation: impl Into<String>,
        confidence: f64,
    ) -> Self {
        Self {
            category: category.into(),
            severity,
            file: None,
            line: None,
            symbol: None,
            evidence: evidence.into(),
            impact: impact.into(),
            recommendation: recommendation.into(),
            confidence,
            related_files: Vec::new(),
            references: Vec::new(),
        }
    }

    /// Attaches a source file location to this finding.
    ///
    /// # Arguments
    ///
    /// * `file` - Repository-relative path of the affected file.
    /// * `line` - 1-based line number, or `None` if the exact line is unknown.
    ///
    /// # Returns
    ///
    /// `self` with `file` and `line` set.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::finding::TechnicalReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = TechnicalReviewFinding::new(
    ///     "architecture", FindingSeverity::Low, "e", "i", "r", 0.5,
    /// ).with_location("src/lib.rs", Some(42));
    ///
    /// assert_eq!(f.file.as_deref(), Some("src/lib.rs"));
    /// assert_eq!(f.line, Some(42));
    /// ```
    pub fn with_location(mut self, file: impl Into<String>, line: Option<u32>) -> Self {
        self.file = Some(file.into());
        self.line = line;
        self
    }

    /// Attaches a symbol name to this finding.
    ///
    /// # Arguments
    ///
    /// * `symbol` - Function, struct, module, or other symbol name.
    ///
    /// # Returns
    ///
    /// `self` with `symbol` set.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::finding::TechnicalReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = TechnicalReviewFinding::new(
    ///     "maintainability", FindingSeverity::Low, "e", "i", "r", 0.6,
    /// ).with_symbol("process_request");
    ///
    /// assert_eq!(f.symbol.as_deref(), Some("process_request"));
    /// ```
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    /// Attaches related file paths to this finding.
    ///
    /// # Arguments
    ///
    /// * `files` - Repository-relative paths of related files.
    ///
    /// # Returns
    ///
    /// `self` with `related_files` set.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::finding::TechnicalReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = TechnicalReviewFinding::new(
    ///     "architecture", FindingSeverity::High, "e", "i", "r", 0.8,
    /// ).with_related_files(vec!["src/a.rs".to_string(), "src/b.rs".to_string()]);
    ///
    /// assert_eq!(f.related_files.len(), 2);
    /// ```
    pub fn with_related_files(mut self, files: Vec<String>) -> Self {
        self.related_files = files;
        self
    }

    /// Attaches external references to this finding.
    ///
    /// # Arguments
    ///
    /// * `refs` - URLs or identifiers (RFCs, docs, best-practice links).
    ///
    /// # Returns
    ///
    /// `self` with `references` set.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::finding::TechnicalReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = TechnicalReviewFinding::new(
    ///     "dependency_hygiene", FindingSeverity::Medium, "e", "i", "r", 0.7,
    /// ).with_references(vec!["https://doc.rust-lang.org".to_string()]);
    ///
    /// assert_eq!(f.references.len(), 1);
    /// ```
    pub fn with_references(mut self, refs: Vec<String>) -> Self {
        self.references = refs;
        self
    }

    /// Converts this finding to a [`PluginFinding`] for inclusion in a
    /// [`ReportEnvelope`][crate::reports::envelope::ReportEnvelope].
    ///
    /// The resulting `PluginFinding` has:
    /// - `kind` = `self.category`
    /// - `title` = `"<SEVERITY>: <first 60 chars of impact>"`
    /// - `description` = evidence + `"\n\nImpact: "` + impact + `"\n\nRecommendation: "` + recommendation
    /// - `file_path` and `line` from `self.file` and `self.line`
    /// - `tags` = category + optional symbol name
    ///
    /// # Returns
    ///
    /// A new [`PluginFinding`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::finding::TechnicalReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = TechnicalReviewFinding::new(
    ///     "architecture", FindingSeverity::High,
    ///     "Circular dependency found.", "Increases coupling.", "Refactor.", 0.9,
    /// );
    /// let pf = f.to_plugin_finding();
    /// assert_eq!(pf.kind, "architecture");
    /// assert!(pf.title.starts_with("HIGH:"));
    /// assert!(pf.description.contains("Circular dependency found."));
    /// assert!(pf.description.contains("Impact: Increases coupling."));
    /// assert!(pf.description.contains("Recommendation: Refactor."));
    /// ```
    pub fn to_plugin_finding(&self) -> PluginFinding {
        let impact_short: String = self.impact.chars().take(60).collect();
        let title = format!("{}: {}", self.severity.label(), impact_short);

        let description = format!(
            "{}\n\nImpact: {}\n\nRecommendation: {}",
            self.evidence, self.impact, self.recommendation
        );

        let mut finding = PluginFinding::new(
            &self.category,
            title,
            description,
            self.severity,
            self.confidence,
        );

        if let Some(ref file) = self.file {
            finding = finding.with_location(file.as_str(), self.line);
        }

        let mut tags = vec![self.category.clone()];
        if let Some(ref sym) = self.symbol {
            tags.push(sym.clone());
        }
        finding = finding.with_tags(tags);

        finding
    }

    /// Attempts to parse a [`TechnicalReviewFinding`] from an AI JSON response
    /// object.
    ///
    /// Required JSON fields: `category` (string), `evidence` (string),
    /// `impact` (string), `recommendation` (string).
    ///
    /// Optional fields: `severity` (string, defaults to `"medium"`),
    /// `confidence` (float, defaults to `0.5`), `file` (string), `line`
    /// (integer), `symbol` (string), `related_files` (array of strings),
    /// `references` (array of strings).
    ///
    /// # Arguments
    ///
    /// * `value` - A JSON object, typically one element of the AI response
    ///   `"findings"` array.
    ///
    /// # Returns
    ///
    /// `Some(TechnicalReviewFinding)` when all required fields are present and
    /// valid.  `None` when required fields are missing or the value is not an
    /// object.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::finding::TechnicalReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let json = serde_json::json!({
    ///     "category": "architecture",
    ///     "severity": "high",
    ///     "evidence": "Circular dependency detected.",
    ///     "impact": "Increases coupling.",
    ///     "recommendation": "Extract shared logic.",
    ///     "confidence": 0.85
    /// });
    /// let finding = TechnicalReviewFinding::from_json_value(&json).unwrap();
    /// assert_eq!(finding.category, "architecture");
    /// assert_eq!(finding.severity, FindingSeverity::High);
    /// ```
    pub fn from_json_value(value: &serde_json::Value) -> Option<Self> {
        let obj = value.as_object()?;

        let category = obj.get("category")?.as_str()?.to_string();
        let evidence = obj.get("evidence")?.as_str()?.to_string();
        let impact = obj.get("impact")?.as_str()?.to_string();
        let recommendation = obj.get("recommendation")?.as_str()?.to_string();

        let severity_str = obj
            .get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("medium");
        let severity = Self::parse_severity(severity_str);

        let confidence = obj
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5);

        let mut finding = Self::new(
            category,
            severity,
            evidence,
            impact,
            recommendation,
            confidence,
        );

        if let Some(file) = obj.get("file").and_then(|v| v.as_str())
            && !file.is_empty()
        {
            let line = obj.get("line").and_then(|v| v.as_u64()).map(|l| l as u32);
            finding = finding.with_location(file, line);
        }

        if let Some(symbol) = obj.get("symbol").and_then(|v| v.as_str())
            && !symbol.is_empty()
        {
            finding = finding.with_symbol(symbol);
        }

        if let Some(arr) = obj.get("related_files").and_then(|v| v.as_array()) {
            let files: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if !files.is_empty() {
                finding = finding.with_related_files(files);
            }
        }

        if let Some(arr) = obj.get("references").and_then(|v| v.as_array()) {
            let refs: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if !refs.is_empty() {
                finding = finding.with_references(refs);
            }
        }

        Some(finding)
    }

    /// Attempts to parse a [`FindingSeverity`] from a string (case-insensitive).
    ///
    /// Returns [`FindingSeverity::Medium`] as a fallback for unrecognised strings.
    ///
    /// # Arguments
    ///
    /// * `s` - Severity string (e.g. `"high"`, `"CRITICAL"`).
    ///
    /// # Returns
    ///
    /// The corresponding [`FindingSeverity`] variant, or `Medium` for unknown inputs.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::finding::TechnicalReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// assert_eq!(TechnicalReviewFinding::parse_severity("high"), FindingSeverity::High);
    /// assert_eq!(TechnicalReviewFinding::parse_severity("CRITICAL"), FindingSeverity::Critical);
    /// assert_eq!(TechnicalReviewFinding::parse_severity("unknown"), FindingSeverity::Medium);
    /// assert_eq!(TechnicalReviewFinding::parse_severity(""), FindingSeverity::Medium);
    /// ```
    pub fn parse_severity(s: &str) -> FindingSeverity {
        match s.to_lowercase().as_str() {
            "info" => FindingSeverity::Info,
            "low" => FindingSeverity::Low,
            "medium" => FindingSeverity::Medium,
            "high" => FindingSeverity::High,
            "critical" => FindingSeverity::Critical,
            _ => FindingSeverity::Medium,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers for ScoringInput
// ---------------------------------------------------------------------------

/// Returns the [`Negative`][crate::scanner::scoring::ScoringSignal::Negative]
/// signal weight for a given [`FindingSeverity`].
///
/// Weights are calibrated so that higher severities produce proportionally
/// larger deductions from the baseline confidence score.
fn severity_to_tech_negative_weight(severity: FindingSeverity) -> f64 {
    match severity {
        FindingSeverity::Info => 0.1,
        FindingSeverity::Low => 0.2,
        FindingSeverity::Medium => 0.4,
        FindingSeverity::High => 0.6,
        FindingSeverity::Critical => 0.8,
    }
}

/// Returns `true` when the category string describes a dependency or
/// supply-chain dimension, which triggers an
/// [`AbsoluteViolation`][crate::scanner::scoring::ScoringSignal::AbsoluteViolation]
/// signal for `Critical`-severity findings.
fn is_critical_dependency_category(category: &str) -> bool {
    let lower = category.to_lowercase();
    lower.contains("dependency") || lower.contains("supply_chain")
}

// ---------------------------------------------------------------------------
// ScoringInput
// ---------------------------------------------------------------------------

impl crate::scanner::scoring::ScoringInput for TechnicalReviewFinding {
    /// Returns the ordered scoring signals for this finding.
    ///
    /// Always emits one [`ScoringSignal::Negative`][crate::scanner::scoring::ScoringSignal::Negative]
    /// whose weight is proportional to the finding's severity.  For
    /// `Critical`-severity findings whose category identifies a
    /// dependency or supply-chain dimension (contains `"dependency"` or
    /// `"supply_chain"`), also emits a
    /// [`ScoringSignal::AbsoluteViolation`][crate::scanner::scoring::ScoringSignal::AbsoluteViolation].
    fn signals(&self) -> Vec<crate::scanner::scoring::ScoringSignal> {
        let mut signals = Vec::new();

        signals.push(crate::scanner::scoring::ScoringSignal::Negative {
            label: format!("severity_{}", self.severity.as_str()),
            weight: severity_to_tech_negative_weight(self.severity),
        });

        if self.severity == FindingSeverity::Critical
            && is_critical_dependency_category(&self.category)
        {
            signals.push(crate::scanner::scoring::ScoringSignal::AbsoluteViolation {
                reason: format!(
                    "critical dependency violation in category '{}'",
                    self.category
                ),
            });
        }

        signals
    }

    /// Returns a human-readable context string for AI prompt construction.
    fn context_for_ai(&self) -> String {
        format!(
            "Category: {}\nSeverity: {}\nEvidence: {}\nImpact: {}",
            self.category,
            self.severity.as_str(),
            self.evidence,
            self.impact,
        )
    }

    /// Returns the plugin identifier.
    fn plugin_name(&self) -> &str {
        "technical_review"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::scoring::ScoringInput;

    fn make_finding() -> TechnicalReviewFinding {
        TechnicalReviewFinding::new(
            "architecture",
            FindingSeverity::High,
            "Circular dependency detected.",
            "Increases coupling.",
            "Refactor the dependency.",
            0.85,
        )
    }

    // ------------------------------------------------------------------
    // new
    // ------------------------------------------------------------------

    #[test]
    fn test_technical_review_finding_new_sets_required_fields() {
        let f = make_finding();
        assert_eq!(f.category, "architecture");
        assert_eq!(f.severity, FindingSeverity::High);
        assert_eq!(f.evidence, "Circular dependency detected.");
        assert_eq!(f.impact, "Increases coupling.");
        assert_eq!(f.recommendation, "Refactor the dependency.");
        assert!((f.confidence - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_technical_review_finding_new_optional_fields_default_to_none_and_empty() {
        let f = make_finding();
        assert!(f.file.is_none());
        assert!(f.line.is_none());
        assert!(f.symbol.is_none());
        assert!(f.related_files.is_empty());
        assert!(f.references.is_empty());
    }

    // ------------------------------------------------------------------
    // with_location
    // ------------------------------------------------------------------

    #[test]
    fn test_technical_review_finding_with_location_sets_file_and_line() {
        let f = make_finding().with_location("src/lib.rs", Some(42));
        assert_eq!(f.file.as_deref(), Some("src/lib.rs"));
        assert_eq!(f.line, Some(42));
    }

    #[test]
    fn test_technical_review_finding_with_location_accepts_none_line() {
        let f = make_finding().with_location("src/main.rs", None);
        assert_eq!(f.file.as_deref(), Some("src/main.rs"));
        assert!(f.line.is_none());
    }

    // ------------------------------------------------------------------
    // with_symbol
    // ------------------------------------------------------------------

    #[test]
    fn test_technical_review_finding_with_symbol_sets_symbol() {
        let f = make_finding().with_symbol("my_function");
        assert_eq!(f.symbol.as_deref(), Some("my_function"));
    }

    // ------------------------------------------------------------------
    // with_related_files
    // ------------------------------------------------------------------

    #[test]
    fn test_technical_review_finding_with_related_files_sets_files() {
        let f =
            make_finding().with_related_files(vec!["src/a.rs".to_string(), "src/b.rs".to_string()]);
        assert_eq!(f.related_files.len(), 2);
        assert_eq!(f.related_files[0], "src/a.rs");
    }

    #[test]
    fn test_technical_review_finding_with_related_files_empty_clears() {
        let f = make_finding().with_related_files(vec![]);
        assert!(f.related_files.is_empty());
    }

    // ------------------------------------------------------------------
    // with_references
    // ------------------------------------------------------------------

    #[test]
    fn test_technical_review_finding_with_references_sets_refs() {
        let f = make_finding().with_references(vec!["https://doc.rust-lang.org".to_string()]);
        assert_eq!(f.references.len(), 1);
        assert_eq!(f.references[0], "https://doc.rust-lang.org");
    }

    // ------------------------------------------------------------------
    // parse_severity
    // ------------------------------------------------------------------

    #[test]
    fn test_parse_severity_lowercase_all_variants_correct() {
        assert_eq!(
            TechnicalReviewFinding::parse_severity("info"),
            FindingSeverity::Info
        );
        assert_eq!(
            TechnicalReviewFinding::parse_severity("low"),
            FindingSeverity::Low
        );
        assert_eq!(
            TechnicalReviewFinding::parse_severity("medium"),
            FindingSeverity::Medium
        );
        assert_eq!(
            TechnicalReviewFinding::parse_severity("high"),
            FindingSeverity::High
        );
        assert_eq!(
            TechnicalReviewFinding::parse_severity("critical"),
            FindingSeverity::Critical
        );
    }

    #[test]
    fn test_parse_severity_uppercase_is_case_insensitive() {
        assert_eq!(
            TechnicalReviewFinding::parse_severity("CRITICAL"),
            FindingSeverity::Critical
        );
        assert_eq!(
            TechnicalReviewFinding::parse_severity("HIGH"),
            FindingSeverity::High
        );
    }

    #[test]
    fn test_parse_severity_unknown_string_returns_medium() {
        assert_eq!(
            TechnicalReviewFinding::parse_severity("unknown"),
            FindingSeverity::Medium
        );
    }

    #[test]
    fn test_parse_severity_empty_string_returns_medium() {
        assert_eq!(
            TechnicalReviewFinding::parse_severity(""),
            FindingSeverity::Medium
        );
    }

    // ------------------------------------------------------------------
    // to_plugin_finding
    // ------------------------------------------------------------------

    #[test]
    fn test_to_plugin_finding_kind_equals_category() {
        let pf = make_finding().to_plugin_finding();
        assert_eq!(pf.kind, "architecture");
    }

    #[test]
    fn test_to_plugin_finding_title_starts_with_severity_label() {
        let pf = make_finding().to_plugin_finding();
        assert!(pf.title.starts_with("HIGH:"));
    }

    #[test]
    fn test_to_plugin_finding_title_truncates_impact_at_60_chars() {
        let long_impact = "A".repeat(100);
        let f = TechnicalReviewFinding::new(
            "architecture",
            FindingSeverity::Low,
            "e",
            &long_impact,
            "r",
            0.5,
        );
        let pf = f.to_plugin_finding();
        // "LOW: " + 60 chars = 65 chars
        assert_eq!(pf.title.len(), "LOW: ".len() + 60);
    }

    #[test]
    fn test_to_plugin_finding_description_contains_evidence() {
        let pf = make_finding().to_plugin_finding();
        assert!(pf.description.contains("Circular dependency detected."));
    }

    #[test]
    fn test_to_plugin_finding_description_contains_impact_section() {
        let pf = make_finding().to_plugin_finding();
        assert!(pf.description.contains("Impact: Increases coupling."));
    }

    #[test]
    fn test_to_plugin_finding_description_contains_recommendation_section() {
        let pf = make_finding().to_plugin_finding();
        assert!(
            pf.description
                .contains("Recommendation: Refactor the dependency.")
        );
    }

    #[test]
    fn test_to_plugin_finding_with_location_sets_file_path_and_line() {
        let f = make_finding().with_location("src/core.rs", Some(10));
        let pf = f.to_plugin_finding();
        assert_eq!(pf.file_path.as_deref(), Some("src/core.rs"));
        assert_eq!(pf.line, Some(10));
    }

    #[test]
    fn test_to_plugin_finding_without_location_file_path_is_none() {
        let pf = make_finding().to_plugin_finding();
        assert!(pf.file_path.is_none());
        assert!(pf.line.is_none());
    }

    #[test]
    fn test_to_plugin_finding_tags_contain_category() {
        let pf = make_finding().to_plugin_finding();
        assert!(pf.tags.contains(&"architecture".to_string()));
    }

    #[test]
    fn test_to_plugin_finding_with_symbol_tags_contain_symbol() {
        let f = make_finding().with_symbol("init");
        let pf = f.to_plugin_finding();
        assert!(pf.tags.contains(&"init".to_string()));
    }

    #[test]
    fn test_to_plugin_finding_severity_and_confidence_preserved() {
        let pf = make_finding().to_plugin_finding();
        assert_eq!(pf.severity, FindingSeverity::High);
        assert!((pf.confidence - 0.85).abs() < f64::EPSILON);
    }

    // ------------------------------------------------------------------
    // from_json_value
    // ------------------------------------------------------------------

    #[test]
    fn test_from_json_value_with_all_required_fields_returns_some() {
        let json = serde_json::json!({
            "category": "architecture",
            "severity": "high",
            "evidence": "Found circular dep.",
            "impact": "Increases coupling.",
            "recommendation": "Refactor.",
            "confidence": 0.9
        });
        let finding = TechnicalReviewFinding::from_json_value(&json);
        assert!(finding.is_some());
        let f = finding.unwrap();
        assert_eq!(f.category, "architecture");
        assert_eq!(f.severity, FindingSeverity::High);
        assert_eq!(f.evidence, "Found circular dep.");
        assert!((f.confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_from_json_value_with_missing_category_returns_none() {
        let json = serde_json::json!({
            "severity": "high",
            "evidence": "e",
            "impact": "i",
            "recommendation": "r"
        });
        assert!(TechnicalReviewFinding::from_json_value(&json).is_none());
    }

    #[test]
    fn test_from_json_value_with_missing_evidence_returns_none() {
        let json = serde_json::json!({
            "category": "architecture",
            "severity": "high",
            "impact": "i",
            "recommendation": "r"
        });
        assert!(TechnicalReviewFinding::from_json_value(&json).is_none());
    }

    #[test]
    fn test_from_json_value_with_missing_impact_returns_none() {
        let json = serde_json::json!({
            "category": "architecture",
            "severity": "high",
            "evidence": "e",
            "recommendation": "r"
        });
        assert!(TechnicalReviewFinding::from_json_value(&json).is_none());
    }

    #[test]
    fn test_from_json_value_with_missing_recommendation_returns_none() {
        let json = serde_json::json!({
            "category": "architecture",
            "severity": "high",
            "evidence": "e",
            "impact": "i"
        });
        assert!(TechnicalReviewFinding::from_json_value(&json).is_none());
    }

    #[test]
    fn test_from_json_value_with_non_object_returns_none() {
        assert!(TechnicalReviewFinding::from_json_value(&serde_json::Value::Null).is_none());
        assert!(TechnicalReviewFinding::from_json_value(&serde_json::json!("string")).is_none());
        assert!(TechnicalReviewFinding::from_json_value(&serde_json::json!(42)).is_none());
    }

    #[test]
    fn test_from_json_value_missing_severity_defaults_to_medium() {
        let json = serde_json::json!({
            "category": "architecture",
            "evidence": "e",
            "impact": "i",
            "recommendation": "r"
        });
        // SAFETY: all required fields are present.
        let f = TechnicalReviewFinding::from_json_value(&json).unwrap();
        assert_eq!(f.severity, FindingSeverity::Medium);
    }

    #[test]
    fn test_from_json_value_missing_confidence_defaults_to_half() {
        let json = serde_json::json!({
            "category": "architecture",
            "evidence": "e",
            "impact": "i",
            "recommendation": "r"
        });
        // SAFETY: all required fields are present.
        let f = TechnicalReviewFinding::from_json_value(&json).unwrap();
        assert!((f.confidence - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_from_json_value_optional_file_and_line_are_parsed() {
        let json = serde_json::json!({
            "category": "architecture",
            "evidence": "e",
            "impact": "i",
            "recommendation": "r",
            "file": "src/lib.rs",
            "line": 99
        });
        // SAFETY: all required fields are present.
        let f = TechnicalReviewFinding::from_json_value(&json).unwrap();
        assert_eq!(f.file.as_deref(), Some("src/lib.rs"));
        assert_eq!(f.line, Some(99));
    }

    #[test]
    fn test_from_json_value_optional_symbol_is_parsed() {
        let json = serde_json::json!({
            "category": "architecture",
            "evidence": "e",
            "impact": "i",
            "recommendation": "r",
            "symbol": "my_fn"
        });
        // SAFETY: all required fields are present.
        let f = TechnicalReviewFinding::from_json_value(&json).unwrap();
        assert_eq!(f.symbol.as_deref(), Some("my_fn"));
    }

    #[test]
    fn test_from_json_value_related_files_are_parsed() {
        let json = serde_json::json!({
            "category": "architecture",
            "evidence": "e",
            "impact": "i",
            "recommendation": "r",
            "related_files": ["a.rs", "b.rs"]
        });
        // SAFETY: all required fields are present.
        let f = TechnicalReviewFinding::from_json_value(&json).unwrap();
        assert_eq!(f.related_files, vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn test_from_json_value_references_are_parsed() {
        let json = serde_json::json!({
            "category": "architecture",
            "evidence": "e",
            "impact": "i",
            "recommendation": "r",
            "references": ["https://example.com"]
        });
        // SAFETY: all required fields are present.
        let f = TechnicalReviewFinding::from_json_value(&json).unwrap();
        assert_eq!(f.references, vec!["https://example.com"]);
    }

    // ------------------------------------------------------------------
    // Serde round-trip
    // ------------------------------------------------------------------

    #[test]
    fn test_technical_review_finding_serde_roundtrip() {
        let f = make_finding()
            .with_location("src/lib.rs", Some(10))
            .with_symbol("init_app");
        // SAFETY: TechnicalReviewFinding is always serializable.
        let json = serde_json::to_string(&f).unwrap();
        // SAFETY: we just serialized this value.
        let restored: TechnicalReviewFinding = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.category, f.category);
        assert_eq!(restored.severity, f.severity);
        assert_eq!(restored.file, f.file);
        assert_eq!(restored.symbol, f.symbol);
    }

    // -------  ScoringInput -------

    #[test]
    fn test_tech_scoring_input_signals_info_severity_emits_small_negative() {
        let f =
            TechnicalReviewFinding::new("architecture", FindingSeverity::Info, "e", "i", "r", 0.5);
        let signals = f.signals();
        assert_eq!(signals.len(), 1);
        match &signals[0] {
            crate::scanner::scoring::ScoringSignal::Negative { weight, .. } => {
                assert!((weight - 0.1).abs() < 1e-9);
            }
            _ => panic!("expected Negative"),
        }
    }

    #[test]
    fn test_tech_scoring_input_signals_critical_non_dependency_emits_single_negative() {
        let f = TechnicalReviewFinding::new(
            "architecture",
            FindingSeverity::Critical,
            "e",
            "i",
            "r",
            0.9,
        );
        let signals = f.signals();
        assert_eq!(signals.len(), 1);
        match &signals[0] {
            crate::scanner::scoring::ScoringSignal::Negative { weight, .. } => {
                assert!((weight - 0.8).abs() < 1e-9);
            }
            _ => panic!("expected Negative"),
        }
    }

    #[test]
    fn test_tech_scoring_input_signals_critical_dependency_hygiene_emits_violation() {
        let f = TechnicalReviewFinding::new(
            "dependency_hygiene",
            FindingSeverity::Critical,
            "e",
            "i",
            "r",
            0.9,
        );
        let signals = f.signals();
        assert_eq!(signals.len(), 2, "must have Negative + AbsoluteViolation");
        assert!(signals[1].is_absolute_violation());
    }

    #[test]
    fn test_tech_scoring_input_signals_critical_supply_chain_emits_violation() {
        let f = TechnicalReviewFinding::new(
            "supply_chain",
            FindingSeverity::Critical,
            "e",
            "i",
            "r",
            0.9,
        );
        let signals = f.signals();
        assert!(signals.iter().any(|s| s.is_absolute_violation()));
    }

    #[test]
    fn test_tech_scoring_input_plugin_name_is_technical_review() {
        let f = TechnicalReviewFinding::new("cat", FindingSeverity::Low, "e", "i", "r", 0.5);
        use crate::scanner::scoring::ScoringInput;
        assert_eq!(f.plugin_name(), "technical_review");
    }

    #[test]
    fn test_tech_scoring_input_context_for_ai_contains_category_and_evidence() {
        let f = TechnicalReviewFinding::new(
            "error_handling",
            FindingSeverity::High,
            "Errors silently discarded.",
            "Bugs go undetected.",
            "Use ? operator.",
            0.8,
        );
        use crate::scanner::scoring::ScoringInput;
        let ctx = f.context_for_ai();
        assert!(ctx.contains("error_handling"));
        assert!(ctx.contains("Errors silently discarded."));
    }

    #[test]
    fn test_tech_scoring_input_severity_weights_are_monotonically_increasing() {
        use crate::scanner::scoring::ScoringInput;
        let weights: Vec<f64> = [
            FindingSeverity::Info,
            FindingSeverity::Low,
            FindingSeverity::Medium,
            FindingSeverity::High,
            FindingSeverity::Critical,
        ]
        .iter()
        .map(|&sev| {
            let f = TechnicalReviewFinding::new("cat", sev, "e", "i", "r", 0.5);
            match f.signals().into_iter().next().unwrap() {
                crate::scanner::scoring::ScoringSignal::Negative { weight, .. } => weight,
                _ => panic!("expected Negative"),
            }
        })
        .collect();

        for i in 1..weights.len() {
            assert!(
                weights[i] > weights[i - 1],
                "weight at index {} must exceed weight at {}",
                i,
                i - 1
            );
        }
    }
}
