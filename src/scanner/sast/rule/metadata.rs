//! Rule metadata types for SAST rules.
//!
//! These types model the structured metadata attached to each rule, including
//! severity, confidence, and categorisation fields. They are used by both the
//! rule parser and the match output projections.

use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// StringOrVec
// ---------------------------------------------------------------------------

/// A collection that deserializes from either a single YAML string or a
/// sequence of strings.
///
/// Rule YAML files frequently express single-element lists as bare strings:
///
/// ```yaml
/// cwe: "CWE-326"           # string form  -> vec!["CWE-326"]
/// owasp: ["A02:2021"]      # list form    -> vec!["A02:2021"]
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct StringOrVec(pub Vec<String>);

impl<'de> Deserialize<'de> for StringOrVec {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = StringOrVec;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string or a list of strings")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<StringOrVec, E> {
                Ok(StringOrVec(vec![v.to_owned()]))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<StringOrVec, E> {
                Ok(StringOrVec(vec![v]))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<StringOrVec, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element::<String>()? {
                    items.push(item);
                }
                Ok(StringOrVec(items))
            }
        }
        d.deserialize_any(V)
    }
}

impl StringOrVec {
    /// Returns the contents as a borrowed slice.
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    /// Returns `true` if the collection contains no strings.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Severity
// ---------------------------------------------------------------------------

/// Severity level assigned to a SAST rule.
///
/// Mirrors the `severity` field in Semgrep rule YAML.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    /// Definite security defect or crash; must be fixed before merge.
    Error,
    /// Probable issue that warrants review.
    Warning,
    /// Informational note; not necessarily a defect.
    Info,
}

// ---------------------------------------------------------------------------
// Confidence
// ---------------------------------------------------------------------------

/// Confidence level indicating how certain the rule author is that a match
/// represents a true positive.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Confidence {
    /// Rule has a low false-positive rate on the target codebase.
    High,
    /// Rule has a moderate false-positive rate.
    Medium,
    /// Rule is heuristic and may produce many false positives.
    Low,
    /// Unknown or unspecified confidence.
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// Category
// ---------------------------------------------------------------------------

/// Broad category for a SAST rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    /// Security-focused rule.
    Security,
    /// Correctness rule (logic errors, misuse of APIs).
    Correctness,
    /// Performance rule.
    Performance,
    /// Best-practice or style rule.
    #[serde(rename = "best-practice")]
    BestPractice,
    /// Maintainability rule.
    Maintainability,
    /// Portability rule.
    Portability,
    /// Uncategorised or unknown category.
    #[serde(other)]
    Other,
}

// ---------------------------------------------------------------------------
// RuleMetadata
// ---------------------------------------------------------------------------

/// Optional structured metadata attached to a SAST rule.
///
/// All fields are optional; rules that omit metadata entirely set this to
/// `None` in [`crate::scanner::sast::rule::ir::RuleIr`].
///
/// The `Default` implementation produces a `RuleMetadata` with all fields
/// set to `None`, representing a rule with no declared metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleMetadata {
    /// Free-text description of what the rule detects.
    pub description: Option<String>,

    /// Broad category of the rule (security, correctness, etc.).
    pub category: Option<Category>,

    /// Confidence level assigned by the rule author.
    pub confidence: Option<Confidence>,

    /// Affected technology stacks (e.g. `["rust", "tokio"]`).
    pub technology: Option<Vec<String>>,

    /// CWE identifiers (e.g. `["CWE-327"]`).
    pub cwe: Option<Vec<String>>,

    /// OWASP identifiers (e.g. `["A02:2021"]`).
    pub owasp: Option<Vec<String>>,

    /// External references (URLs to advisories, documentation, etc.).
    pub references: Option<Vec<String>>,

    /// SPDX license expression for the rule source.
    pub license: Option<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_equality_same_variant() {
        assert_eq!(Severity::Warning, Severity::Warning);
    }

    #[test]
    fn test_severity_inequality_different_variants() {
        assert_ne!(Severity::Error, Severity::Info);
    }

    #[test]
    fn test_severity_clone_produces_equal_value() {
        let s = Severity::Error;
        assert_eq!(s.clone(), Severity::Error);
    }

    #[test]
    fn test_confidence_unknown_variant_exists() {
        let c = Confidence::Unknown;
        assert_eq!(c, Confidence::Unknown);
    }

    #[test]
    fn test_rule_metadata_all_none_is_default_constructible() {
        let m = RuleMetadata {
            description: None,
            category: None,
            confidence: None,
            technology: None,
            cwe: None,
            owasp: None,
            references: None,
            license: None,
        };
        assert!(m.description.is_none());
    }

    #[test]
    fn test_rule_metadata_with_fields_is_accessible() {
        let m = RuleMetadata {
            description: Some("detects weak RSA keys".to_string()),
            category: Some(Category::Security),
            confidence: Some(Confidence::High),
            technology: Some(vec!["rust".to_string()]),
            cwe: Some(vec!["CWE-327".to_string()]),
            owasp: Some(vec!["A02:2021".to_string()]),
            references: None,
            license: Some("Apache-2.0".to_string()),
        };
        assert_eq!(m.description.as_deref(), Some("detects weak RSA keys"));
        assert_eq!(m.category, Some(Category::Security));
        assert_eq!(m.confidence, Some(Confidence::High));
    }
}
