//! Serde types mirroring the Semgrep YAML rule schema surface.
//!
//! All constructs are modelled here, including those the engine cannot yet
//! evaluate (taint propagators, metavariable-analysis, deep expressions).
//! The compatibility gate in [`rule::compat`] inspects these fields to
//! determine which rules to skip before compilation.
//!
//! [`rule::compat`]: crate::scanner::sast::rule::compat

use crate::scanner::sast::rule::metadata::{RuleMetadata, Severity, StringOrVec};
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;

// ---------------------------------------------------------------------------
// Top-level file and rule types
// ---------------------------------------------------------------------------

/// Top-level structure of a Semgrep-dialect YAML rule file.
///
/// A single YAML file may contain multiple rules under the `rules:` key.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::rule::schema::RuleFile;
///
/// let yaml = r#"
/// rules:
///   - id: my-rule
///     message: "Found an issue"
///     languages: [rust]
///     severity: WARNING
///     pattern: "foo()"
/// "#;
/// let file: RuleFile = serde_yaml::from_str(yaml).expect("valid YAML");
/// assert_eq!(file.rules.len(), 1);
/// assert_eq!(file.rules[0].id, "my-rule");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleFile {
    /// The list of rules contained in this file.
    pub rules: Vec<RuleSchema>,
}

/// A single rule in Semgrep YAML dialect.
///
/// Exactly one of `pattern`, `patterns`, `pattern_either`, or `pattern_regex`
/// should be present at the rule root. The compatibility gate enforces this
/// constraint during compilation; at the schema level all fields are optional
/// so that invalid YAML still deserializes for diagnostic purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSchema {
    /// Unique rule identifier. Must match `^[a-zA-Z0-9._/-]+$`.
    pub id: String,

    /// Human-readable message shown when the rule fires.
    pub message: String,

    /// Language names this rule applies to (e.g. `["rust"]`).
    pub languages: Vec<String>,

    /// Severity of findings produced by this rule.
    pub severity: Severity,

    /// Optional structured metadata block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<RuleMetadata>,

    // --- Formula fields (exactly one should be set at the rule root) --------
    /// A single structural pattern (e.g. `RsaPrivateKey::new(&mut $RNG, $BITS)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    /// A conjunction or disjunction of pattern terms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patterns: Option<Vec<PatternTerm>>,

    /// A disjunction of pattern alternatives.
    #[serde(
        rename = "pattern-either",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub pattern_either: Option<Vec<PatternTerm>>,

    /// A regex pattern applied to the raw source text.
    #[serde(
        rename = "pattern-regex",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub pattern_regex: Option<String>,

    // --- Engine mode --------------------------------------------------------
    /// Engine execution mode. Supported values: `search` (default), `taint`.
    ///
    /// Rules with `mode: taint` are gated by the compatibility checker and
    /// produce a [`SkipReason::TaintMode`] outcome rather than a compiled IR.
    ///
    /// [`SkipReason::TaintMode`]: crate::scanner::sast::error::SkipReason::TaintMode
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    // --- Fix fields ---------------------------------------------------------
    /// Suggested source-level fix string applied to matched code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,

    /// Regex-based fix specification (Semgrep `fix-regex` block).
    ///
    /// Stored as a raw YAML value; content is not validated by this engine.
    #[serde(rename = "fix-regex", default, skip_serializing_if = "Option::is_none")]
    pub fix_regex: Option<YamlValue>,

    // --- Gated constructs ---------------------------------------------------
    /// Taint propagation rules (Semgrep Pro feature).
    ///
    /// Stored as a raw YAML value; presence triggers a [`SkipReason::PatternPropagators`]
    /// outcome from the compatibility gate.
    ///
    /// [`SkipReason::PatternPropagators`]: crate::scanner::sast::error::SkipReason::PatternPropagators
    #[serde(
        rename = "pattern-propagators",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub pattern_propagators: Option<YamlValue>,

    /// Per-rule engine options block (preserved verbatim, not evaluated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<YamlValue>,
}

// ---------------------------------------------------------------------------
// Metavariable condition types
// ---------------------------------------------------------------------------

/// Condition that tests a metavariable against a regular expression.
///
/// Corresponds to the `metavariable-regex:` key within a `patterns:` list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetavarRegexCondition {
    /// The metavariable to test (e.g. `"$X"`).
    pub metavariable: String,
    /// The regular expression the metavariable text must match.
    pub regex: String,
    /// When `true`, the condition is negated: the metavariable must NOT match.
    #[serde(default)]
    pub not: bool,
}

/// Condition that tests a metavariable against a structural or regex pattern.
///
/// Corresponds to the `metavariable-pattern:` key within a `patterns:` list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetavarPatternCondition {
    /// The metavariable whose bound code is to be tested.
    pub metavariable: String,

    /// Structural pattern the metavariable must match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    /// Regex pattern the metavariable text must match.
    #[serde(
        rename = "pattern-regex",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub pattern_regex: Option<String>,

    /// Optional language override for the sub-pattern analysis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Condition that compares a metavariable against an expression.
///
/// Corresponds to the `metavariable-comparison:` key within a `patterns:` list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetavarComparisonCondition {
    /// The metavariable whose numeric value is tested.
    pub metavariable: String,
    /// Comparison expression (e.g. `"$BITS < 2048"`).
    pub comparison: String,
    /// Optional numeric base for the metavariable text (e.g. `16` for hex).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<u32>,
    /// When `true`, strip non-numeric characters before comparison.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip: Option<bool>,
}

// ---------------------------------------------------------------------------
// PatternTerm
// ---------------------------------------------------------------------------

/// A single term within a `patterns:` or `pattern-either:` list.
///
/// Each term carries exactly one positive construct (a `pattern`, a
/// `pattern-inside`, an inner `pattern-either`, or a `pattern-regex`) and
/// optionally negation clauses, metavariable conditions, and focus directives.
///
/// The compatibility gate inspects [`metavariable_analysis`] and
/// [`pattern_propagators`] to detect unsupported constructs.
///
/// [`metavariable_analysis`]: PatternTerm::metavariable_analysis
/// [`pattern_propagators`]: PatternTerm::pattern_propagators
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatternTerm {
    // --- Positive constructs ------------------------------------------------
    /// A structural pattern that must match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    /// The match must be structurally inside a region matching this pattern.
    #[serde(
        rename = "pattern-inside",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub pattern_inside: Option<String>,

    /// A nested disjunction of pattern alternatives.
    #[serde(
        rename = "pattern-either",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub pattern_either: Option<Vec<PatternTerm>>,

    /// A regex that must match the raw source text.
    #[serde(
        rename = "pattern-regex",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub pattern_regex: Option<String>,

    // --- Negative constructs ------------------------------------------------
    /// A structural pattern that must NOT match at the same location.
    #[serde(
        rename = "pattern-not",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub pattern_not: Option<String>,

    /// The match must NOT be structurally inside a region matching this pattern.
    #[serde(
        rename = "pattern-not-inside",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub pattern_not_inside: Option<String>,

    // --- Metavariable conditions --------------------------------------------
    /// Regex condition on a bound metavariable.
    #[serde(
        rename = "metavariable-regex",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub metavariable_regex: Option<MetavarRegexCondition>,

    /// Structural or regex condition on a bound metavariable.
    #[serde(
        rename = "metavariable-pattern",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub metavariable_pattern: Option<MetavarPatternCondition>,

    /// Numeric comparison condition on a bound metavariable.
    #[serde(
        rename = "metavariable-comparison",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub metavariable_comparison: Option<MetavarComparisonCondition>,

    /// Dataflow analysis condition on a bound metavariable (gated construct).
    ///
    /// Presence triggers [`SkipReason::MetavarAnalysis`].
    ///
    /// [`SkipReason::MetavarAnalysis`]: crate::scanner::sast::error::SkipReason::MetavarAnalysis
    #[serde(
        rename = "metavariable-analysis",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub metavariable_analysis: Option<YamlValue>,

    // --- Focus --------------------------------------------------------------
    /// Metavariable(s) to focus the reported finding range on.
    ///
    /// Accepts either a bare string or a YAML sequence.
    #[serde(
        rename = "focus-metavariable",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub focus_metavariable: Option<StringOrVec>,

    // --- Gated constructs ---------------------------------------------------
    /// Taint propagation rules nested inside a pattern term (gated construct).
    ///
    /// Presence triggers [`SkipReason::PatternPropagators`].
    ///
    /// [`SkipReason::PatternPropagators`]: crate::scanner::sast::error::SkipReason::PatternPropagators
    #[serde(
        rename = "pattern-propagators",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub pattern_propagators: Option<YamlValue>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid rule YAML used across several tests.
    fn minimal_rule_yaml() -> &'static str {
        r#"
rules:
  - id: test-rule
    message: "Test message"
    languages: [rust]
    severity: WARNING
    pattern: "foo()"
"#
    }

    #[test]
    fn test_rule_file_parses_minimal_pattern_rule() {
        // SAFETY: static YAML is valid; failure indicates a deserialization bug.
        let file: RuleFile = serde_yaml::from_str(minimal_rule_yaml()).unwrap();
        assert_eq!(file.rules.len(), 1);
        let rule = &file.rules[0];
        assert_eq!(rule.id, "test-rule");
        assert_eq!(rule.message, "Test message");
        assert_eq!(rule.languages, vec!["rust"]);
        assert_eq!(rule.pattern.as_deref(), Some("foo()"));
        assert!(rule.patterns.is_none());
    }

    #[test]
    fn test_rule_file_parses_patterns_with_pattern_not() {
        let yaml = r#"
rules:
  - id: compound-rule
    message: "Compound rule"
    languages: [rust]
    severity: ERROR
    patterns:
      - pattern: "foo($X)"
      - pattern-not: "foo(0)"
"#;
        // SAFETY: static YAML is valid; failure indicates a deserialization bug.
        let file: RuleFile = serde_yaml::from_str(yaml).unwrap();
        let rule = &file.rules[0];
        // SAFETY: YAML contains patterns; None would be a parse bug.
        let terms = rule.patterns.as_ref().unwrap();
        assert_eq!(terms.len(), 2);
        assert!(
            terms[0].pattern.is_some(),
            "first term should be a positive pattern"
        );
        assert!(
            terms[1].pattern_not.is_some(),
            "second term should be pattern-not"
        );
    }

    #[test]
    fn test_rule_file_parses_pattern_regex_rule() {
        let yaml = r#"
rules:
  - id: regex-rule
    message: "Regex rule"
    languages: [generic]
    severity: INFO
    pattern-regex: 'secret_key\s*=\s*.*'
"#;
        // SAFETY: static YAML is valid; failure indicates a deserialization bug.
        let file: RuleFile = serde_yaml::from_str(yaml).unwrap();
        let rule = &file.rules[0];
        assert!(
            rule.pattern_regex.is_some(),
            "pattern-regex should be parsed"
        );
        assert!(rule.pattern.is_none(), "pattern should be absent");
    }

    #[test]
    fn test_rule_file_parses_mode_taint_rule() {
        // Taint-mode rules parse at the schema level; rejection happens in compat.
        let yaml = r#"
rules:
  - id: taint-rule
    message: "Taint rule"
    languages: [python]
    severity: ERROR
    mode: taint
    pattern: "sink($X)"
"#;
        // SAFETY: static YAML is valid; failure indicates a deserialization bug.
        let file: RuleFile = serde_yaml::from_str(yaml).unwrap();
        let rule = &file.rules[0];
        assert_eq!(rule.mode.as_deref(), Some("taint"));
    }

    #[test]
    fn test_pattern_term_parses_metavariable_regex() {
        let yaml = "metavariable-regex:\n  metavariable: \"$X\"\n  regex: \"^foo.*$\"";
        // SAFETY: static YAML is valid; failure indicates a deserialization bug.
        let term: PatternTerm = serde_yaml::from_str(yaml).unwrap();
        // SAFETY: YAML contains metavariable-regex; None would be a parse bug.
        let cond = term.metavariable_regex.unwrap();
        assert_eq!(cond.metavariable, "$X");
        assert_eq!(cond.regex, "^foo.*$");
        assert!(!cond.not, "not should default to false");
    }

    #[test]
    fn test_pattern_term_parses_focus_metavariable_single() {
        let yaml = "focus-metavariable: \"$X\"";
        // SAFETY: static YAML is valid; failure indicates a deserialization bug.
        let term: PatternTerm = serde_yaml::from_str(yaml).unwrap();
        // SAFETY: YAML contains focus-metavariable; None would be a parse bug.
        let focus = term.focus_metavariable.unwrap();
        assert_eq!(focus.as_slice(), &["$X"]);
    }

    #[test]
    fn test_pattern_term_parses_focus_metavariable_list() {
        let yaml = "focus-metavariable:\n  - \"$X\"\n  - \"$Y\"";
        // SAFETY: static YAML is valid; failure indicates a deserialization bug.
        let term: PatternTerm = serde_yaml::from_str(yaml).unwrap();
        // SAFETY: YAML contains focus-metavariable; None would be a parse bug.
        let focus = term.focus_metavariable.unwrap();
        assert_eq!(focus.as_slice(), &["$X", "$Y"]);
    }
}
