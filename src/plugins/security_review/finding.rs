//! The [`SecurityReviewFinding`] data type.
//!
//! A security finding records a single security observation made by the
//! security review plugin. Compared with the technical review finding it
//! carries additional security-specific fields: CWE and OWASP mappings,
//! exploitability assessment, false-positive guidance, and SARIF rule
//! identifiers used to produce valid SARIF 2.1.0 output.
//!
//! Secret evidence is redacted via [`SecurityReviewFinding::redact_evidence`]
//! before it is stored, so raw credential values never appear in reports or
//! Kafka messages.

use serde::{Deserialize, Serialize};

use crate::reports::findings::PluginFinding;
use crate::scanner::findings::FindingSeverity;

// ---------------------------------------------------------------------------
// SecurityReviewFinding
// ---------------------------------------------------------------------------

/// A finding produced by the security review plugin.
///
/// Each finding maps to one security category (e.g. `"injection"`,
/// `"secrets_exposure"`) and records evidence, exploitability, impact,
/// and concrete remediation guidance. CWE and OWASP fields allow the finding
/// to be cross-referenced with industry-standard vulnerability catalogues.
/// The `sarif_rule_id` is derived from the category at construction time and
/// is used to emit valid SARIF 2.1.0 output.
///
/// # Examples
///
/// ```
/// use xzardgz::plugins::security_review::finding::SecurityReviewFinding;
/// use xzardgz::scanner::findings::FindingSeverity;
///
/// let f = SecurityReviewFinding::new(
///     "sql_injection",
///     FindingSeverity::High,
///     "Unsanitized user input passed to SQL query.",
///     "Trivially exploitable via standard SQL injection techniques.",
///     "Attacker can read, modify, or delete database contents.",
///     "Use parameterized queries or a prepared statement API.",
///     0.95,
/// );
/// assert_eq!(f.category, "sql_injection");
/// assert_eq!(f.sarif_rule_id, "sql_injection");
/// assert!(f.cwe.is_none());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReviewFinding {
    /// Security category identifier (e.g. `"injection"`, `"secrets_exposure"`).
    pub category: String,
    /// Severity of the finding.
    pub severity: FindingSeverity,
    /// CWE identifier (e.g. `"CWE-89"`), if applicable.
    pub cwe: Option<String>,
    /// OWASP category (e.g. `"A03:2021 Injection"`), if applicable.
    pub owasp: Option<String>,
    /// Repository-relative file path, if applicable.
    pub file: Option<String>,
    /// 1-based line number, if known.
    pub line: Option<u32>,
    /// Symbol name (function, struct, module) where applicable.
    pub symbol: Option<String>,
    /// What was observed in the code; may contain `[REDACTED]` placeholders.
    pub evidence: String,
    /// Assessment of how easy the vulnerability is to exploit.
    pub exploitability: String,
    /// Why this vulnerability matters for the system.
    pub impact: String,
    /// Concrete steps to remediate the finding.
    pub remediation: String,
    /// AI confidence in `[0.0, 1.0]`.
    pub confidence: f64,
    /// Notes explaining why this may be a false positive, if applicable.
    pub false_positive_notes: Option<String>,
    /// SARIF 2.1.0 rule identifier, derived from `category` at construction.
    pub sarif_rule_id: String,
    /// URI pointing to additional SARIF rule help documentation.
    pub sarif_help_uri: Option<String>,
}

impl SecurityReviewFinding {
    /// Creates a new finding with the required fields.
    ///
    /// Optional fields default to `None`. Use the builder methods
    /// [`with_location`][Self::with_location], [`with_symbol`][Self::with_symbol],
    /// [`with_cwe`][Self::with_cwe], [`with_owasp`][Self::with_owasp],
    /// [`with_false_positive_notes`][Self::with_false_positive_notes], and
    /// [`with_sarif_help_uri`][Self::with_sarif_help_uri] to populate them.
    ///
    /// The `sarif_rule_id` is derived automatically from `category` by
    /// lowercasing and replacing spaces with underscores.
    ///
    /// Evidence is passed through [`Self::redact_evidence`] before storage so
    /// that raw secrets are never persisted.
    ///
    /// # Arguments
    ///
    /// * `category`       - Security category identifier (e.g. `"sql_injection"`).
    /// * `severity`       - Severity classification.
    /// * `evidence`       - Observation text; secrets will be redacted automatically.
    /// * `exploitability` - Assessment of how easy the vulnerability is to exploit.
    /// * `impact`         - Why this vulnerability matters.
    /// * `remediation`    - Concrete remediation steps.
    /// * `confidence`     - AI confidence in `[0.0, 1.0]`.
    ///
    /// # Returns
    ///
    /// A `SecurityReviewFinding` with all optional fields set to `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::security_review::finding::SecurityReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = SecurityReviewFinding::new(
    ///     "xss",
    ///     FindingSeverity::Medium,
    ///     "User input echoed without escaping.",
    ///     "Exploitable via crafted link.",
    ///     "Allows script injection in victim browsers.",
    ///     "Escape all user-controlled output.",
    ///     0.8,
    /// );
    /// assert_eq!(f.sarif_rule_id, "xss");
    /// assert!(f.file.is_none());
    /// assert!(f.cwe.is_none());
    /// ```
    pub fn new(
        category: impl Into<String>,
        severity: FindingSeverity,
        evidence: impl Into<String>,
        exploitability: impl Into<String>,
        impact: impl Into<String>,
        remediation: impl Into<String>,
        confidence: f64,
    ) -> Self {
        let category = category.into();
        let sarif_rule_id = category.to_lowercase().replace(' ', "_");
        let evidence_str: String = evidence.into();
        let evidence = Self::redact_evidence(&evidence_str);
        Self {
            category,
            severity,
            cwe: None,
            owasp: None,
            file: None,
            line: None,
            symbol: None,
            evidence,
            exploitability: exploitability.into(),
            impact: impact.into(),
            remediation: remediation.into(),
            confidence,
            false_positive_notes: None,
            sarif_rule_id,
            sarif_help_uri: None,
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
    /// use xzardgz::plugins::security_review::finding::SecurityReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = SecurityReviewFinding::new(
    ///     "xss", FindingSeverity::Medium, "e", "easy", "i", "r", 0.5,
    /// ).with_location("src/handlers/auth.rs", Some(88));
    ///
    /// assert_eq!(f.file.as_deref(), Some("src/handlers/auth.rs"));
    /// assert_eq!(f.line, Some(88));
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
    /// * `symbol` - Name of the function, struct, or module where the finding occurs.
    ///
    /// # Returns
    ///
    /// `self` with `symbol` set.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::security_review::finding::SecurityReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = SecurityReviewFinding::new(
    ///     "xss", FindingSeverity::Medium, "e", "easy", "i", "r", 0.5,
    /// ).with_symbol("render_user_input");
    ///
    /// assert_eq!(f.symbol.as_deref(), Some("render_user_input"));
    /// ```
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    /// Attaches a CWE identifier to this finding.
    ///
    /// # Arguments
    ///
    /// * `cwe` - Common Weakness Enumeration identifier (e.g. `"CWE-89"`).
    ///
    /// # Returns
    ///
    /// `self` with `cwe` set.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::security_review::finding::SecurityReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = SecurityReviewFinding::new(
    ///     "sql_injection", FindingSeverity::High, "e", "easy", "i", "r", 0.9,
    /// ).with_cwe("CWE-89");
    ///
    /// assert_eq!(f.cwe.as_deref(), Some("CWE-89"));
    /// ```
    pub fn with_cwe(mut self, cwe: impl Into<String>) -> Self {
        self.cwe = Some(cwe.into());
        self
    }

    /// Attaches an OWASP category to this finding.
    ///
    /// # Arguments
    ///
    /// * `owasp` - OWASP Top 10 category (e.g. `"A03:2021 Injection"`).
    ///
    /// # Returns
    ///
    /// `self` with `owasp` set.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::security_review::finding::SecurityReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = SecurityReviewFinding::new(
    ///     "sql_injection", FindingSeverity::High, "e", "easy", "i", "r", 0.9,
    /// ).with_owasp("A03:2021 Injection");
    ///
    /// assert_eq!(f.owasp.as_deref(), Some("A03:2021 Injection"));
    /// ```
    pub fn with_owasp(mut self, owasp: impl Into<String>) -> Self {
        self.owasp = Some(owasp.into());
        self
    }

    /// Attaches false-positive guidance notes to this finding.
    ///
    /// # Arguments
    ///
    /// * `notes` - Free-text explanation of why this result may be a false positive.
    ///
    /// # Returns
    ///
    /// `self` with `false_positive_notes` set.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::security_review::finding::SecurityReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = SecurityReviewFinding::new(
    ///     "xss", FindingSeverity::Low, "e", "easy", "i", "r", 0.4,
    /// ).with_false_positive_notes("Only triggered in test harness, not production.");
    ///
    /// assert!(f.false_positive_notes.is_some());
    /// ```
    pub fn with_false_positive_notes(mut self, notes: impl Into<String>) -> Self {
        self.false_positive_notes = Some(notes.into());
        self
    }

    /// Attaches a SARIF help URI to this finding.
    ///
    /// # Arguments
    ///
    /// * `uri` - URL pointing to documentation for the SARIF rule.
    ///
    /// # Returns
    ///
    /// `self` with `sarif_help_uri` set.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::security_review::finding::SecurityReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = SecurityReviewFinding::new(
    ///     "sql_injection", FindingSeverity::High, "e", "easy", "i", "r", 0.9,
    /// ).with_sarif_help_uri("https://cwe.mitre.org/data/definitions/89.html");
    ///
    /// assert_eq!(
    ///     f.sarif_help_uri.as_deref(),
    ///     Some("https://cwe.mitre.org/data/definitions/89.html"),
    /// );
    /// ```
    pub fn with_sarif_help_uri(mut self, uri: impl Into<String>) -> Self {
        self.sarif_help_uri = Some(uri.into());
        self
    }

    /// Converts this finding to a [`PluginFinding`] for inclusion in a report envelope.
    ///
    /// The resulting `PluginFinding` has:
    ///
    /// - `kind` = `self.sarif_rule_id`
    /// - `title` = `"<SEVERITY>: <first 60 chars of impact>"`
    /// - `description` = evidence + exploitability + impact + remediation sections,
    ///   with optional CWE and OWASP appended when present
    /// - `file_path` and `line` from `self.file` and `self.line`
    /// - `tags` = category + optional symbol, CWE, and OWASP values
    ///
    /// # Returns
    ///
    /// A new [`PluginFinding`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::security_review::finding::SecurityReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let f = SecurityReviewFinding::new(
    ///     "sql_injection", FindingSeverity::High,
    ///     "Raw input in query.", "Easy to exploit.", "Full DB access.", "Use params.", 0.9,
    /// );
    /// let pf = f.to_plugin_finding();
    /// assert_eq!(pf.kind, "sql_injection");
    /// assert!(pf.title.starts_with("HIGH:"));
    /// assert!(pf.description.contains("Exploitability:"));
    /// assert!(pf.description.contains("Impact:"));
    /// assert!(pf.description.contains("Remediation:"));
    /// ```
    pub fn to_plugin_finding(&self) -> PluginFinding {
        let impact_short: String = self.impact.chars().take(60).collect();
        let title = format!("{}: {}", self.severity.label(), impact_short);

        let mut description = self.evidence.clone();
        description.push_str("\n\nExploitability: ");
        description.push_str(&self.exploitability);
        description.push_str("\n\nImpact: ");
        description.push_str(&self.impact);
        description.push_str("\n\nRemediation: ");
        description.push_str(&self.remediation);
        if let Some(ref cwe) = self.cwe {
            description.push_str("\n\nCWE: ");
            description.push_str(cwe);
        }
        if let Some(ref owasp) = self.owasp {
            description.push_str("\n\nOWASP: ");
            description.push_str(owasp);
        }

        let mut finding = PluginFinding::new(
            &self.sarif_rule_id,
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
        if let Some(ref cwe) = self.cwe {
            tags.push(cwe.clone());
        }
        if let Some(ref owasp) = self.owasp {
            tags.push(owasp.clone());
        }
        finding = finding.with_tags(tags);

        finding
    }

    /// Attempts to parse a [`SecurityReviewFinding`] from an AI JSON response object.
    ///
    /// Required JSON fields: `category` (string), `evidence` (string),
    /// `impact` (string), `remediation` (string).
    ///
    /// Optional fields: `exploitability` (string, defaults to `"Unknown"`),
    /// `severity` (string, defaults to `"medium"`), `confidence` (float,
    /// defaults to `0.5`), `file` (string), `line` (integer), `symbol`
    /// (string), `cwe` (string), `owasp` (string), `false_positive_notes`
    /// (string), `sarif_help_uri` (string).
    ///
    /// # Arguments
    ///
    /// * `value` - A JSON object, typically one element of the AI response
    ///   `"findings"` array.
    ///
    /// # Returns
    ///
    /// `Some(SecurityReviewFinding)` when all required fields are present and
    /// valid. `None` when required fields are missing or the value is not an
    /// object.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::security_review::finding::SecurityReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// let json = serde_json::json!({
    ///     "category": "sql_injection",
    ///     "severity": "high",
    ///     "evidence": "Raw user input in SQL query.",
    ///     "impact": "Full database compromise.",
    ///     "remediation": "Use parameterized queries.",
    ///     "confidence": 0.95
    /// });
    /// let finding = SecurityReviewFinding::from_json_value(&json).unwrap();
    /// assert_eq!(finding.category, "sql_injection");
    /// assert_eq!(finding.severity, FindingSeverity::High);
    /// ```
    pub fn from_json_value(value: &serde_json::Value) -> Option<Self> {
        let obj = value.as_object()?;

        let category = obj.get("category")?.as_str()?.to_string();
        let evidence = obj.get("evidence")?.as_str()?.to_string();
        let impact = obj.get("impact")?.as_str()?.to_string();
        let remediation = obj.get("remediation")?.as_str()?.to_string();

        let exploitability = obj
            .get("exploitability")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

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
            exploitability,
            impact,
            remediation,
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

        if let Some(cwe) = obj.get("cwe").and_then(|v| v.as_str())
            && !cwe.is_empty()
        {
            finding = finding.with_cwe(cwe);
        }

        if let Some(owasp) = obj.get("owasp").and_then(|v| v.as_str())
            && !owasp.is_empty()
        {
            finding = finding.with_owasp(owasp);
        }

        if let Some(notes) = obj.get("false_positive_notes").and_then(|v| v.as_str())
            && !notes.is_empty()
        {
            finding = finding.with_false_positive_notes(notes);
        }

        if let Some(uri) = obj.get("sarif_help_uri").and_then(|v| v.as_str())
            && !uri.is_empty()
        {
            finding = finding.with_sarif_help_uri(uri);
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
    /// use xzardgz::plugins::security_review::finding::SecurityReviewFinding;
    /// use xzardgz::scanner::findings::FindingSeverity;
    ///
    /// assert_eq!(SecurityReviewFinding::parse_severity("high"), FindingSeverity::High);
    /// assert_eq!(SecurityReviewFinding::parse_severity("CRITICAL"), FindingSeverity::Critical);
    /// assert_eq!(SecurityReviewFinding::parse_severity("unknown"), FindingSeverity::Medium);
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

    /// Redacts known secret patterns from an evidence string.
    ///
    /// The following patterns trigger redaction (case-insensitive):
    /// `password=`, `secret=`, `api_key=`, `apikey=`, `token=`,
    /// `private_key=`, `access_key=`. Any value following such a key is
    /// replaced with `[REDACTED]`.
    ///
    /// Lines containing `-----BEGIN` are replaced entirely with
    /// `[REDACTED KEY MATERIAL]`.
    ///
    /// # Arguments
    ///
    /// * `evidence` - Raw evidence string that may contain secrets.
    ///
    /// # Returns
    ///
    /// A new `String` with sensitive values replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::security_review::finding::SecurityReviewFinding;
    ///
    /// let redacted = SecurityReviewFinding::redact_evidence("password=hunter2 rest");
    /// assert!(redacted.contains("[REDACTED]"));
    /// assert!(!redacted.contains("hunter2"));
    ///
    /// let redacted = SecurityReviewFinding::redact_evidence("plain text, no secrets");
    /// assert_eq!(redacted, "plain text, no secrets");
    /// ```
    pub fn redact_evidence(evidence: &str) -> String {
        let mut result = evidence.to_string();

        // Handle -----BEGIN blocks
        if result.contains("-----BEGIN") {
            result = result
                .lines()
                .map(|line| {
                    if line.contains("-----BEGIN") {
                        "[REDACTED KEY MATERIAL]"
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
        }

        // Handle key=value patterns
        for pattern in &[
            "password=",
            "secret=",
            "api_key=",
            "apikey=",
            "token=",
            "private_key=",
            "access_key=",
        ] {
            let lower = result.to_lowercase();
            if let Some(pos) = lower.find(pattern) {
                let prefix_end = pos + pattern.len();
                // Find end of value: whitespace, quote, comma, semicolon, or end of string
                let value_end = result[prefix_end..]
                    .find(|c: char| {
                        c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == ';'
                    })
                    .map(|rel| prefix_end + rel)
                    .unwrap_or(result.len());
                if value_end > prefix_end {
                    result = format!(
                        "{}[REDACTED]{}",
                        &result[..prefix_end],
                        &result[value_end..]
                    );
                }
            }
        }

        result
    }
}

// ---------------------------------------------------------------------------
// ScoringInput helpers
// ---------------------------------------------------------------------------

/// Returns the Negative signal weight for a given severity.
///
/// Weights are calibrated so that higher severities produce larger
/// proportional deductions from the baseline confidence score.
fn severity_to_negative_weight(severity: FindingSeverity) -> f64 {
    match severity {
        FindingSeverity::Info => 0.1,
        FindingSeverity::Low => 0.2,
        FindingSeverity::Medium => 0.4,
        FindingSeverity::High => 0.6,
        FindingSeverity::Critical => 0.8,
    }
}

/// Returns `true` when the category string describes a credential or secret
/// exposure, which triggers an [`crate::scanner::scoring::ScoringSignal::AbsoluteViolation`]
/// signal for Critical-severity findings.
fn is_credential_category(category: &str) -> bool {
    let lower = category.to_lowercase();
    lower.contains("secret")
        || lower.contains("credential")
        || lower.contains("hardcoded")
        || lower.contains("api_key")
        || lower.contains("password")
}

// ---------------------------------------------------------------------------
// ScoringInput
// ---------------------------------------------------------------------------

impl crate::scanner::scoring::ScoringInput for SecurityReviewFinding {
    /// Returns the ordered scoring signals for this finding.
    ///
    /// Always emits one [`crate::scanner::scoring::ScoringSignal::Negative`]
    /// whose weight is proportional to the finding's severity.  Additionally
    /// emits a [`crate::scanner::scoring::ScoringSignal::AbsoluteViolation`]
    /// when the finding is `Critical` severity AND the category identifies a
    /// credential-exposure condition (contains `"secret"`, `"credential"`,
    /// `"hardcoded"`, `"api_key"`, or `"password"`).
    fn signals(&self) -> Vec<crate::scanner::scoring::ScoringSignal> {
        let mut signals = Vec::new();

        signals.push(crate::scanner::scoring::ScoringSignal::Negative {
            label: format!("severity_{}", self.severity.as_str()),
            weight: severity_to_negative_weight(self.severity),
        });

        if self.severity == FindingSeverity::Critical && is_credential_category(&self.category) {
            signals.push(crate::scanner::scoring::ScoringSignal::AbsoluteViolation {
                reason: format!(
                    "critical credential exposure detected in category '{}'",
                    self.category
                ),
            });
        }

        signals
    }

    /// Returns a human-readable context string for AI prompt construction.
    fn context_for_ai(&self) -> String {
        format!(
            "Category: {}\nSeverity: {}\nEvidence: {}\nExploitability: {}\nImpact: {}",
            self.category,
            self.severity.as_str(),
            self.evidence,
            self.exploitability,
            self.impact,
        )
    }

    /// Returns the plugin identifier.
    fn plugin_name(&self) -> &str {
        "security_review"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::scoring::ScoringInput;

    fn make_finding() -> SecurityReviewFinding {
        SecurityReviewFinding::new(
            "sql_injection",
            FindingSeverity::High,
            "Unsanitized input detected.",
            "Easy: input directly interpolated into query.",
            "Attacker can read or modify the database.",
            "Use parameterized queries.",
            0.9,
        )
    }

    // ------------------------------------------------------------------
    // new
    // ------------------------------------------------------------------

    #[test]
    fn test_security_review_finding_new_sets_required_fields() {
        let f = make_finding();
        assert_eq!(f.category, "sql_injection");
        assert_eq!(f.severity, FindingSeverity::High);
        assert_eq!(f.evidence, "Unsanitized input detected.");
        assert_eq!(
            f.exploitability,
            "Easy: input directly interpolated into query."
        );
        assert_eq!(f.impact, "Attacker can read or modify the database.");
        assert_eq!(f.remediation, "Use parameterized queries.");
        assert!((f.confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_security_review_finding_new_optional_fields_default_to_none() {
        let f = make_finding();
        assert!(f.file.is_none());
        assert!(f.line.is_none());
        assert!(f.symbol.is_none());
        assert!(f.cwe.is_none());
        assert!(f.owasp.is_none());
        assert!(f.false_positive_notes.is_none());
        assert!(f.sarif_help_uri.is_none());
    }

    #[test]
    fn test_security_review_finding_new_sarif_rule_id_derived_from_category() {
        let f = SecurityReviewFinding::new(
            "SQL Injection",
            FindingSeverity::High,
            "e",
            "easy",
            "i",
            "r",
            0.5,
        );
        assert_eq!(f.sarif_rule_id, "sql_injection");
    }

    // ------------------------------------------------------------------
    // with_location
    // ------------------------------------------------------------------

    #[test]
    fn test_security_review_finding_with_location_sets_file_and_line() {
        let f = make_finding().with_location("src/db/query.rs", Some(42));
        assert_eq!(f.file.as_deref(), Some("src/db/query.rs"));
        assert_eq!(f.line, Some(42));
    }

    #[test]
    fn test_security_review_finding_with_location_accepts_none_line() {
        let f = make_finding().with_location("src/db/query.rs", None);
        assert_eq!(f.file.as_deref(), Some("src/db/query.rs"));
        assert!(f.line.is_none());
    }

    // ------------------------------------------------------------------
    // with_symbol
    // ------------------------------------------------------------------

    #[test]
    fn test_security_review_finding_with_symbol_sets_symbol() {
        let f = make_finding().with_symbol("execute_query");
        assert_eq!(f.symbol.as_deref(), Some("execute_query"));
    }

    // ------------------------------------------------------------------
    // with_cwe
    // ------------------------------------------------------------------

    #[test]
    fn test_security_review_finding_with_cwe_sets_cwe() {
        let f = make_finding().with_cwe("CWE-89");
        assert_eq!(f.cwe.as_deref(), Some("CWE-89"));
    }

    // ------------------------------------------------------------------
    // with_owasp
    // ------------------------------------------------------------------

    #[test]
    fn test_security_review_finding_with_owasp_sets_owasp() {
        let f = make_finding().with_owasp("A03:2021 Injection");
        assert_eq!(f.owasp.as_deref(), Some("A03:2021 Injection"));
    }

    // ------------------------------------------------------------------
    // with_false_positive_notes
    // ------------------------------------------------------------------

    #[test]
    fn test_security_review_finding_with_false_positive_notes_sets_notes() {
        let f = make_finding().with_false_positive_notes("Only reachable in test environment.");
        assert_eq!(
            f.false_positive_notes.as_deref(),
            Some("Only reachable in test environment.")
        );
    }

    // ------------------------------------------------------------------
    // with_sarif_help_uri
    // ------------------------------------------------------------------

    #[test]
    fn test_security_review_finding_with_sarif_help_uri_sets_uri() {
        let f =
            make_finding().with_sarif_help_uri("https://cwe.mitre.org/data/definitions/89.html");
        assert_eq!(
            f.sarif_help_uri.as_deref(),
            Some("https://cwe.mitre.org/data/definitions/89.html")
        );
    }

    // ------------------------------------------------------------------
    // to_plugin_finding
    // ------------------------------------------------------------------

    #[test]
    fn test_to_plugin_finding_kind_equals_sarif_rule_id() {
        let pf = make_finding().to_plugin_finding();
        assert_eq!(pf.kind, "sql_injection");
    }

    #[test]
    fn test_to_plugin_finding_title_starts_with_severity_label() {
        let pf = make_finding().to_plugin_finding();
        assert!(pf.title.starts_with("HIGH:"));
    }

    #[test]
    fn test_to_plugin_finding_description_contains_evidence() {
        let pf = make_finding().to_plugin_finding();
        assert!(pf.description.contains("Unsanitized input detected."));
    }

    #[test]
    fn test_to_plugin_finding_description_contains_exploitability_section() {
        let pf = make_finding().to_plugin_finding();
        assert!(
            pf.description
                .contains("Exploitability: Easy: input directly interpolated into query.")
        );
    }

    #[test]
    fn test_to_plugin_finding_description_contains_impact_section() {
        let pf = make_finding().to_plugin_finding();
        assert!(
            pf.description
                .contains("Impact: Attacker can read or modify the database.")
        );
    }

    #[test]
    fn test_to_plugin_finding_description_contains_remediation_section() {
        let pf = make_finding().to_plugin_finding();
        assert!(
            pf.description
                .contains("Remediation: Use parameterized queries.")
        );
    }

    #[test]
    fn test_to_plugin_finding_with_cwe_description_contains_cwe() {
        let f = make_finding().with_cwe("CWE-89");
        let pf = f.to_plugin_finding();
        assert!(pf.description.contains("CWE: CWE-89"));
    }

    #[test]
    fn test_to_plugin_finding_with_owasp_description_contains_owasp() {
        let f = make_finding().with_owasp("A03:2021 Injection");
        let pf = f.to_plugin_finding();
        assert!(pf.description.contains("OWASP: A03:2021 Injection"));
    }

    #[test]
    fn test_to_plugin_finding_tags_contain_category() {
        let pf = make_finding().to_plugin_finding();
        assert!(pf.tags.contains(&"sql_injection".to_string()));
    }

    #[test]
    fn test_to_plugin_finding_with_symbol_tags_contain_symbol() {
        let f = make_finding().with_symbol("execute_query");
        let pf = f.to_plugin_finding();
        assert!(pf.tags.contains(&"execute_query".to_string()));
    }

    // ------------------------------------------------------------------
    // parse_severity
    // ------------------------------------------------------------------

    #[test]
    fn test_parse_severity_lowercase_all_variants_correct() {
        assert_eq!(
            SecurityReviewFinding::parse_severity("info"),
            FindingSeverity::Info
        );
        assert_eq!(
            SecurityReviewFinding::parse_severity("low"),
            FindingSeverity::Low
        );
        assert_eq!(
            SecurityReviewFinding::parse_severity("medium"),
            FindingSeverity::Medium
        );
        assert_eq!(
            SecurityReviewFinding::parse_severity("high"),
            FindingSeverity::High
        );
        assert_eq!(
            SecurityReviewFinding::parse_severity("critical"),
            FindingSeverity::Critical
        );
    }

    #[test]
    fn test_parse_severity_uppercase_is_case_insensitive() {
        assert_eq!(
            SecurityReviewFinding::parse_severity("CRITICAL"),
            FindingSeverity::Critical
        );
        assert_eq!(
            SecurityReviewFinding::parse_severity("HIGH"),
            FindingSeverity::High
        );
    }

    #[test]
    fn test_parse_severity_unknown_string_returns_medium() {
        assert_eq!(
            SecurityReviewFinding::parse_severity("unknown"),
            FindingSeverity::Medium
        );
    }

    // ------------------------------------------------------------------
    // redact_evidence
    // ------------------------------------------------------------------

    #[test]
    fn test_redact_evidence_plain_text_unchanged() {
        let result = SecurityReviewFinding::redact_evidence("No sensitive data here.");
        assert_eq!(result, "No sensitive data here.");
    }

    #[test]
    fn test_redact_evidence_password_pattern_is_redacted() {
        let result =
            SecurityReviewFinding::redact_evidence("password=mysecretpassword rest of line");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("mysecretpassword"));
    }

    #[test]
    fn test_redact_evidence_api_key_pattern_is_redacted() {
        let result = SecurityReviewFinding::redact_evidence("api_key=ABCDEFGH123456");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("ABCDEFGH123456"));
    }

    #[test]
    fn test_redact_evidence_begin_block_is_redacted() {
        let result = SecurityReviewFinding::redact_evidence(
            "-----BEGIN RSA PRIVATE KEY-----\nkey data here",
        );
        assert!(result.contains("[REDACTED KEY MATERIAL]"));
        assert!(!result.contains("BEGIN RSA PRIVATE KEY"));
    }

    #[test]
    fn test_redact_evidence_token_pattern_is_redacted() {
        let result = SecurityReviewFinding::redact_evidence("token=ghp_supersecrettoken123");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("ghp_supersecrettoken123"));
    }

    // ------------------------------------------------------------------
    // from_json_value
    // ------------------------------------------------------------------

    #[test]
    fn test_from_json_value_with_all_required_fields_returns_some() {
        let json = serde_json::json!({
            "category": "sql_injection",
            "severity": "high",
            "evidence": "Raw input in query.",
            "impact": "Full database compromise.",
            "remediation": "Use parameterized queries.",
            "confidence": 0.95
        });
        let finding = SecurityReviewFinding::from_json_value(&json);
        assert!(finding.is_some());
        let f = finding.unwrap();
        assert_eq!(f.category, "sql_injection");
        assert_eq!(f.severity, FindingSeverity::High);
        assert_eq!(f.evidence, "Raw input in query.");
        assert!((f.confidence - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn test_from_json_value_missing_category_returns_none() {
        let json = serde_json::json!({
            "evidence": "e",
            "impact": "i",
            "remediation": "r"
        });
        assert!(SecurityReviewFinding::from_json_value(&json).is_none());
    }

    #[test]
    fn test_from_json_value_missing_evidence_returns_none() {
        let json = serde_json::json!({
            "category": "sql_injection",
            "impact": "i",
            "remediation": "r"
        });
        assert!(SecurityReviewFinding::from_json_value(&json).is_none());
    }

    #[test]
    fn test_from_json_value_missing_impact_returns_none() {
        let json = serde_json::json!({
            "category": "sql_injection",
            "evidence": "e",
            "remediation": "r"
        });
        assert!(SecurityReviewFinding::from_json_value(&json).is_none());
    }

    #[test]
    fn test_from_json_value_missing_remediation_returns_none() {
        let json = serde_json::json!({
            "category": "sql_injection",
            "evidence": "e",
            "impact": "i"
        });
        assert!(SecurityReviewFinding::from_json_value(&json).is_none());
    }

    #[test]
    fn test_from_json_value_with_non_object_returns_none() {
        assert!(SecurityReviewFinding::from_json_value(&serde_json::Value::Null).is_none());
        assert!(SecurityReviewFinding::from_json_value(&serde_json::json!("string")).is_none());
        assert!(SecurityReviewFinding::from_json_value(&serde_json::json!(42)).is_none());
    }

    #[test]
    fn test_from_json_value_missing_severity_defaults_to_medium() {
        let json = serde_json::json!({
            "category": "sql_injection",
            "evidence": "e",
            "impact": "i",
            "remediation": "r"
        });
        // SAFETY: all required fields are present.
        let f = SecurityReviewFinding::from_json_value(&json).unwrap();
        assert_eq!(f.severity, FindingSeverity::Medium);
    }

    #[test]
    fn test_from_json_value_missing_confidence_defaults_to_half() {
        let json = serde_json::json!({
            "category": "sql_injection",
            "evidence": "e",
            "impact": "i",
            "remediation": "r"
        });
        // SAFETY: all required fields are present.
        let f = SecurityReviewFinding::from_json_value(&json).unwrap();
        assert!((f.confidence - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_from_json_value_parses_optional_cwe_and_owasp() {
        let json = serde_json::json!({
            "category": "sql_injection",
            "evidence": "e",
            "impact": "i",
            "remediation": "r",
            "cwe": "CWE-89",
            "owasp": "A03:2021 Injection"
        });
        // SAFETY: all required fields are present.
        let f = SecurityReviewFinding::from_json_value(&json).unwrap();
        assert_eq!(f.cwe.as_deref(), Some("CWE-89"));
        assert_eq!(f.owasp.as_deref(), Some("A03:2021 Injection"));
    }

    // ------------------------------------------------------------------
    // Serde round-trip
    // ------------------------------------------------------------------

    #[test]
    fn test_security_review_finding_serde_roundtrip() {
        let f = make_finding()
            .with_location("src/db/query.rs", Some(55))
            .with_symbol("run_query")
            .with_cwe("CWE-89")
            .with_owasp("A03:2021 Injection");
        // SAFETY: SecurityReviewFinding is always serializable.
        let json = serde_json::to_string(&f).unwrap();
        // SAFETY: we just serialized this value.
        let restored: SecurityReviewFinding = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.category, f.category);
        assert_eq!(restored.severity, f.severity);
        assert_eq!(restored.file, f.file);
        assert_eq!(restored.symbol, f.symbol);
        assert_eq!(restored.cwe, f.cwe);
        assert_eq!(restored.owasp, f.owasp);
        assert_eq!(restored.sarif_rule_id, f.sarif_rule_id);
    }

    // -------  ScoringInput -------

    #[test]
    fn test_scoring_input_signals_info_severity_emits_small_negative() {
        let _f = make_finding();
        // make_finding uses High by default — build an Info one
        let f = SecurityReviewFinding::new("xss", FindingSeverity::Info, "e", "exp", "i", "r", 0.5);
        let signals = f.signals();
        assert_eq!(signals.len(), 1);
        match &signals[0] {
            crate::scanner::scoring::ScoringSignal::Negative { weight, .. } => {
                assert!((weight - 0.1).abs() < 1e-9);
            }
            _ => panic!("expected Negative signal"),
        }
    }

    #[test]
    fn test_scoring_input_signals_critical_non_credential_emits_single_negative() {
        let f = SecurityReviewFinding::new(
            "injection",
            FindingSeverity::Critical,
            "e",
            "exp",
            "i",
            "r",
            0.9,
        );
        let signals = f.signals();
        // No AbsoluteViolation because category != credential
        assert_eq!(signals.len(), 1);
        match &signals[0] {
            crate::scanner::scoring::ScoringSignal::Negative { weight, .. } => {
                assert!((weight - 0.8).abs() < 1e-9);
            }
            _ => panic!("expected Negative signal"),
        }
    }

    #[test]
    fn test_scoring_input_signals_critical_secrets_exposure_emits_violation() {
        let f = SecurityReviewFinding::new(
            "secrets_exposure",
            FindingSeverity::Critical,
            "Hardcoded API key found.",
            "Trivial.",
            "Full access.",
            "Use env vars.",
            0.95,
        );
        let signals = f.signals();
        assert_eq!(signals.len(), 2, "must have Negative + AbsoluteViolation");
        assert!(signals[1].is_absolute_violation());
    }

    #[test]
    fn test_scoring_input_signals_critical_credential_category_emits_violation() {
        let f = SecurityReviewFinding::new(
            "hardcoded_credential",
            FindingSeverity::Critical,
            "e",
            "exp",
            "i",
            "r",
            0.9,
        );
        let signals = f.signals();
        assert!(signals.iter().any(|s| s.is_absolute_violation()));
    }

    #[test]
    fn test_scoring_input_plugin_name_is_security_review() {
        let f = make_finding();
        use crate::scanner::scoring::ScoringInput;
        assert_eq!(f.plugin_name(), "security_review");
    }

    #[test]
    fn test_scoring_input_context_for_ai_contains_category_and_evidence() {
        let f = SecurityReviewFinding::new(
            "sql_injection",
            FindingSeverity::High,
            "Raw user input in SQL.",
            "Easy.",
            "Data loss.",
            "Use params.",
            0.9,
        );
        use crate::scanner::scoring::ScoringInput;
        let ctx = f.context_for_ai();
        assert!(ctx.contains("sql_injection"));
        assert!(ctx.contains("Raw user input in SQL."));
    }

    #[test]
    fn test_scoring_input_severity_weights_are_monotonically_increasing() {
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
            let f = SecurityReviewFinding::new("cat", sev, "e", "exp", "i", "r", 0.5);
            match f.signals().into_iter().next().unwrap() {
                crate::scanner::scoring::ScoringSignal::Negative { weight, .. } => weight,
                _ => panic!("expected Negative"),
            }
        })
        .collect();

        for i in 1..weights.len() {
            assert!(
                weights[i] > weights[i - 1],
                "weight for severity {} must exceed severity {}",
                i,
                i - 1
            );
        }
    }
}
