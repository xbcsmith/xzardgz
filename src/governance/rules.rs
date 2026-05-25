//! Core data model for governance rules, violations, and results.
//!
//! This module defines the fundamental types used throughout the governance
//! system:
//!
//! - [`EnforcementLevel`] — what happens when a rule is violated.
//! - [`RuleSource`] — where a rule originated.
//! - [`GovernanceRule`] — a single governance policy.
//! - [`GovernanceViolation`] — a concrete rule violation with context.
//! - [`GovernanceResult`] — the aggregate outcome of one or more checks.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::diagnostics::{Diagnostic, DiagnosticCategory, DiagnosticLevel, Diagnostics};

// ---------------------------------------------------------------------------
// EnforcementLevel
// ---------------------------------------------------------------------------

/// Enforcement level determines what happens when a governance rule is violated.
///
/// - `Required` violations stop execution when `fail_on_violation` is set.
/// - `Recommended` violations produce warning diagnostics but do not stop execution.
/// - `Optional` violations produce informational diagnostics only.
///
/// # Examples
///
/// ```
/// use xzardgz::governance::EnforcementLevel;
///
/// assert_ne!(EnforcementLevel::Required, EnforcementLevel::Recommended);
/// assert_eq!(EnforcementLevel::Optional, EnforcementLevel::Optional);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnforcementLevel {
    /// Stops execution when `fail_on_violation` is true.
    Required,
    /// Produces a warning diagnostic; does not stop execution.
    Recommended,
    /// Produces an info diagnostic only.
    Optional,
}

// ---------------------------------------------------------------------------
// RuleSource
// ---------------------------------------------------------------------------

/// Describes where a governance rule was loaded from.
///
/// # Examples
///
/// ```
/// use xzardgz::governance::RuleSource;
/// use std::path::PathBuf;
///
/// let src = RuleSource::RepositoryFile { path: PathBuf::from("AGENTS.md") };
/// let _ = src.clone();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleSource {
    /// The rule is compiled into the binary as a default.
    Embedded,
    /// The rule was loaded from a governance file in the repository.
    RepositoryFile {
        /// Path to the file from which the rule was loaded.
        path: std::path::PathBuf,
    },
    /// The rule was derived or synthesised at runtime.
    Derived,
}

// ---------------------------------------------------------------------------
// GovernanceRule
// ---------------------------------------------------------------------------

/// A single governance policy that can be applied to pipeline inputs.
///
/// Each rule has a unique `id`, a human-readable `description`, an
/// [`EnforcementLevel`] controlling what happens on violation, and a
/// [`RuleSource`] recording where it came from.
///
/// # Examples
///
/// ```
/// use xzardgz::governance::{GovernanceRule, EnforcementLevel, RuleSource};
///
/// let rule = GovernanceRule {
///     id: "governance.path.no_traversal".to_string(),
///     description: "File paths must not contain path traversal sequences".to_string(),
///     enforcement: EnforcementLevel::Required,
///     source: RuleSource::Embedded,
/// };
///
/// assert_eq!(rule.id, "governance.path.no_traversal");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceRule {
    /// Unique rule identifier in reverse-DNS style, e.g. `governance.path.no_traversal`.
    pub id: String,
    /// Human-readable description of what the rule enforces.
    pub description: String,
    /// How violations of this rule are handled.
    pub enforcement: EnforcementLevel,
    /// Where the rule was loaded from.
    pub source: RuleSource,
}

// ---------------------------------------------------------------------------
// GovernanceViolation
// ---------------------------------------------------------------------------

/// A concrete instance of a governance rule being violated.
///
/// Each violation records the offending value and a descriptive message so
/// that reports and diagnostics are self-contained.
///
/// # Examples
///
/// ```
/// use xzardgz::governance::{GovernanceRule, GovernanceViolation, EnforcementLevel, RuleSource};
///
/// let rule = GovernanceRule {
///     id: "governance.path.no_traversal".to_string(),
///     description: "File paths must not contain path traversal sequences".to_string(),
///     enforcement: EnforcementLevel::Required,
///     source: RuleSource::Embedded,
/// };
///
/// let violation = GovernanceViolation::new(&rule, "../etc/passwd", "path traversal detected");
/// assert_eq!(violation.rule_id, "governance.path.no_traversal");
/// assert!(violation.is_blocking());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceViolation {
    /// ID of the rule that was violated.
    pub rule_id: String,
    /// Description of the violated rule.
    pub rule_description: String,
    /// Enforcement level of the violated rule.
    pub enforcement: EnforcementLevel,
    /// The actual value that caused the violation.
    pub value: String,
    /// Human-readable message explaining the violation.
    pub message: String,
}

impl GovernanceViolation {
    /// Creates a new violation from a rule, the offending value, and a message.
    ///
    /// # Arguments
    ///
    /// * `rule` - The [`GovernanceRule`] that was violated.
    /// * `value` - The concrete value (path, name, etc.) that triggered the violation.
    /// * `message` - A human-readable explanation of why the value violates the rule.
    ///
    /// # Returns
    ///
    /// A fully populated [`GovernanceViolation`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceRule, GovernanceViolation, EnforcementLevel, RuleSource};
    ///
    /// let rule = GovernanceRule {
    ///     id: "governance.plugin.valid_name".to_string(),
    ///     description: "Plugin names must be valid identifiers".to_string(),
    ///     enforcement: EnforcementLevel::Required,
    ///     source: RuleSource::Embedded,
    /// };
    ///
    /// let v = GovernanceViolation::new(&rule, "BadPlugin", "must start with lowercase letter");
    /// assert_eq!(v.value, "BadPlugin");
    /// assert!(v.is_blocking());
    /// ```
    pub fn new(
        rule: &GovernanceRule,
        value: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            rule_id: rule.id.clone(),
            rule_description: rule.description.clone(),
            enforcement: rule.enforcement.clone(),
            value: value.into(),
            message: message.into(),
        }
    }

    /// Returns `true` when this violation requires blocking execution.
    ///
    /// Only [`EnforcementLevel::Required`] violations are blocking.
    /// `Recommended` and `Optional` violations produce diagnostics but do not
    /// stop the pipeline.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceRule, GovernanceViolation, EnforcementLevel, RuleSource};
    ///
    /// let required_rule = GovernanceRule {
    ///     id: "r1".to_string(),
    ///     description: "required rule".to_string(),
    ///     enforcement: EnforcementLevel::Required,
    ///     source: RuleSource::Embedded,
    /// };
    /// let recommended_rule = GovernanceRule {
    ///     id: "r2".to_string(),
    ///     description: "recommended rule".to_string(),
    ///     enforcement: EnforcementLevel::Recommended,
    ///     source: RuleSource::Embedded,
    /// };
    ///
    /// let blocking = GovernanceViolation::new(&required_rule, "v", "msg");
    /// let non_blocking = GovernanceViolation::new(&recommended_rule, "v", "msg");
    ///
    /// assert!(blocking.is_blocking());
    /// assert!(!non_blocking.is_blocking());
    /// ```
    pub fn is_blocking(&self) -> bool {
        self.enforcement == EnforcementLevel::Required
    }
}

// ---------------------------------------------------------------------------
// GovernanceResult
// ---------------------------------------------------------------------------

/// The aggregate result of one or more governance checks.
///
/// A result collects all [`GovernanceViolation`] instances produced during a
/// check and provides helpers for querying and converting them.  An empty
/// result (no violations) means all rules passed.
///
/// # Examples
///
/// ```
/// use xzardgz::governance::GovernanceResult;
///
/// let result = GovernanceResult::new();
/// assert!(result.is_ok());
/// assert!(result.is_empty());
/// assert_eq!(result.len(), 0);
/// ```
#[derive(Debug, Clone, Default)]
pub struct GovernanceResult {
    /// All violations collected during the check.
    pub violations: Vec<GovernanceViolation>,
}

impl GovernanceResult {
    /// Creates a new, empty [`GovernanceResult`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::GovernanceResult;
    ///
    /// let r = GovernanceResult::new();
    /// assert!(r.is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if any collected violation has [`EnforcementLevel::Required`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceRule, GovernanceViolation, GovernanceResult, EnforcementLevel, RuleSource};
    ///
    /// let rule = GovernanceRule {
    ///     id: "r".to_string(),
    ///     description: "d".to_string(),
    ///     enforcement: EnforcementLevel::Required,
    ///     source: RuleSource::Embedded,
    /// };
    /// let mut result = GovernanceResult::new();
    /// result.violations.push(GovernanceViolation::new(&rule, "v", "msg"));
    /// assert!(result.has_blocking_violations());
    /// ```
    pub fn has_blocking_violations(&self) -> bool {
        self.violations.iter().any(|v| v.is_blocking())
    }

    /// Returns references to all violations that have [`EnforcementLevel::Required`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceRule, GovernanceViolation, GovernanceResult, EnforcementLevel, RuleSource};
    ///
    /// let rule = GovernanceRule {
    ///     id: "r".to_string(),
    ///     description: "d".to_string(),
    ///     enforcement: EnforcementLevel::Required,
    ///     source: RuleSource::Embedded,
    /// };
    /// let mut result = GovernanceResult::new();
    /// result.violations.push(GovernanceViolation::new(&rule, "v", "msg"));
    /// assert_eq!(result.blocking_violations().len(), 1);
    /// ```
    pub fn blocking_violations(&self) -> Vec<&GovernanceViolation> {
        self.violations.iter().filter(|v| v.is_blocking()).collect()
    }

    /// Returns references to violations that do NOT have [`EnforcementLevel::Required`].
    ///
    /// These are `Recommended` and `Optional` violations that become diagnostics
    /// rather than errors.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceRule, GovernanceViolation, GovernanceResult, EnforcementLevel, RuleSource};
    ///
    /// let rule = GovernanceRule {
    ///     id: "r".to_string(),
    ///     description: "d".to_string(),
    ///     enforcement: EnforcementLevel::Recommended,
    ///     source: RuleSource::Embedded,
    /// };
    /// let mut result = GovernanceResult::new();
    /// result.violations.push(GovernanceViolation::new(&rule, "v", "msg"));
    /// assert_eq!(result.non_blocking_violations().len(), 1);
    /// assert_eq!(result.blocking_violations().len(), 0);
    /// ```
    pub fn non_blocking_violations(&self) -> Vec<&GovernanceViolation> {
        self.violations
            .iter()
            .filter(|v| !v.is_blocking())
            .collect()
    }

    /// Moves all violations from `other` into this result.
    ///
    /// Used by the high-level checker to aggregate results from multiple
    /// individual validations before a single `handle_result` call.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceRule, GovernanceViolation, GovernanceResult, EnforcementLevel, RuleSource};
    ///
    /// let rule = GovernanceRule {
    ///     id: "r".to_string(),
    ///     description: "d".to_string(),
    ///     enforcement: EnforcementLevel::Required,
    ///     source: RuleSource::Embedded,
    /// };
    ///
    /// let mut a = GovernanceResult::new();
    /// a.violations.push(GovernanceViolation::new(&rule, "v1", "m1"));
    ///
    /// let mut b = GovernanceResult::new();
    /// b.violations.push(GovernanceViolation::new(&rule, "v2", "m2"));
    ///
    /// a.merge(b);
    /// assert_eq!(a.len(), 2);
    /// ```
    pub fn merge(&mut self, other: GovernanceResult) {
        self.violations.extend(other.violations);
    }

    /// Converts all violations into a [`Diagnostics`] collection.
    ///
    /// Conversion rules:
    /// - `Required` and `Recommended` violations become [`DiagnosticLevel::Warning`].
    /// - `Optional` violations become [`DiagnosticLevel::Info`].
    ///
    /// Every diagnostic entry uses:
    /// - `message` = `"[<rule_id>] <violation message>"`
    /// - `context` = `Some(<offending value>)`
    /// - `category` = [`DiagnosticCategory::Governance`]
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceRule, GovernanceViolation, GovernanceResult, EnforcementLevel, RuleSource};
    ///
    /// let rule = GovernanceRule {
    ///     id: "gov.test".to_string(),
    ///     description: "Test rule".to_string(),
    ///     enforcement: EnforcementLevel::Required,
    ///     source: RuleSource::Embedded,
    /// };
    /// let mut result = GovernanceResult::new();
    /// result.violations.push(GovernanceViolation::new(&rule, "bad_val", "reason"));
    ///
    /// let diags = result.to_diagnostics();
    /// assert_eq!(diags.len(), 1);
    /// assert!(diags.entries[0].message.contains("gov.test"));
    /// ```
    pub fn to_diagnostics(&self) -> Diagnostics {
        let mut diags = Diagnostics::new();
        for v in &self.violations {
            let level = match v.enforcement {
                EnforcementLevel::Required | EnforcementLevel::Recommended => {
                    DiagnosticLevel::Warning
                }
                EnforcementLevel::Optional => DiagnosticLevel::Info,
            };
            let message = format!("[{}] {}", v.rule_id, v.message);
            let d = Diagnostic {
                level,
                category: DiagnosticCategory::Governance,
                message,
                context: Some(v.value.clone()),
                timestamp: Utc::now(),
            };
            diags.push(d);
        }
        diags
    }

    /// Returns `true` when there are no violations.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::GovernanceResult;
    ///
    /// let r = GovernanceResult::new();
    /// assert!(r.is_ok());
    /// ```
    pub fn is_ok(&self) -> bool {
        self.violations.is_empty()
    }

    /// Returns the total number of violations collected.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::GovernanceResult;
    ///
    /// let r = GovernanceResult::new();
    /// assert_eq!(r.len(), 0);
    /// ```
    pub fn len(&self) -> usize {
        self.violations.len()
    }

    /// Returns `true` when there are no violations.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::GovernanceResult;
    ///
    /// let r = GovernanceResult::new();
    /// assert!(r.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.violations.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{DiagnosticCategory, DiagnosticLevel};

    fn make_rule(id: &str, enforcement: EnforcementLevel) -> GovernanceRule {
        GovernanceRule {
            id: id.to_string(),
            description: format!("Test rule for {}", id),
            enforcement,
            source: RuleSource::Embedded,
        }
    }

    // ------------------------------------------------------------------
    // GovernanceViolation::new
    // ------------------------------------------------------------------

    #[test]
    fn test_governance_violation_new_creates_with_correct_fields() {
        let rule = make_rule("gov.test", EnforcementLevel::Required);
        let v = GovernanceViolation::new(&rule, "bad_value", "something is wrong");
        assert_eq!(v.rule_id, "gov.test");
        assert_eq!(v.rule_description, "Test rule for gov.test");
        assert_eq!(v.enforcement, EnforcementLevel::Required);
        assert_eq!(v.value, "bad_value");
        assert_eq!(v.message, "something is wrong");
    }

    #[test]
    fn test_governance_violation_new_accepts_string_and_str_for_value_and_message() {
        let rule = make_rule("gov.r", EnforcementLevel::Recommended);
        let v = GovernanceViolation::new(&rule, "value".to_string(), "msg".to_string());
        assert_eq!(v.value, "value");
        assert_eq!(v.message, "msg");
    }

    // ------------------------------------------------------------------
    // GovernanceViolation::is_blocking
    // ------------------------------------------------------------------

    #[test]
    fn test_governance_violation_is_blocking_returns_true_for_required() {
        let rule = make_rule("r", EnforcementLevel::Required);
        let v = GovernanceViolation::new(&rule, "v", "m");
        assert!(v.is_blocking());
    }

    #[test]
    fn test_governance_violation_is_blocking_returns_false_for_recommended() {
        let rule = make_rule("r", EnforcementLevel::Recommended);
        let v = GovernanceViolation::new(&rule, "v", "m");
        assert!(!v.is_blocking());
    }

    #[test]
    fn test_governance_violation_is_blocking_returns_false_for_optional() {
        let rule = make_rule("r", EnforcementLevel::Optional);
        let v = GovernanceViolation::new(&rule, "v", "m");
        assert!(!v.is_blocking());
    }

    // ------------------------------------------------------------------
    // GovernanceResult::new / is_ok / is_empty / len
    // ------------------------------------------------------------------

    #[test]
    fn test_governance_result_new_creates_empty_result() {
        let r = GovernanceResult::new();
        assert!(r.is_ok());
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
        assert!(r.violations.is_empty());
    }

    #[test]
    fn test_governance_result_len_returns_violation_count() {
        let rule = make_rule("r", EnforcementLevel::Required);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&rule, "v1", "m1"));
        result
            .violations
            .push(GovernanceViolation::new(&rule, "v2", "m2"));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_governance_result_is_empty_returns_true_for_new_result() {
        assert!(GovernanceResult::new().is_empty());
    }

    #[test]
    fn test_governance_result_is_empty_returns_false_when_has_violations() {
        let rule = make_rule("r", EnforcementLevel::Required);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&rule, "v", "m"));
        assert!(!result.is_empty());
    }

    #[test]
    fn test_governance_result_is_ok_returns_true_when_no_violations() {
        let r = GovernanceResult::new();
        assert!(r.is_ok());
    }

    #[test]
    fn test_governance_result_is_ok_returns_false_when_has_violations() {
        let rule = make_rule("r", EnforcementLevel::Required);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&rule, "v", "m"));
        assert!(!result.is_ok());
    }

    // ------------------------------------------------------------------
    // GovernanceResult::has_blocking_violations
    // ------------------------------------------------------------------

    #[test]
    fn test_governance_result_has_blocking_violations_returns_true_when_required_present() {
        let rule = make_rule("r", EnforcementLevel::Required);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&rule, "v", "m"));
        assert!(result.has_blocking_violations());
    }

    #[test]
    fn test_governance_result_has_blocking_violations_returns_false_when_only_recommended() {
        let rule = make_rule("r", EnforcementLevel::Recommended);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&rule, "v", "m"));
        assert!(!result.has_blocking_violations());
    }

    #[test]
    fn test_governance_result_has_blocking_violations_returns_false_when_only_optional() {
        let rule = make_rule("r", EnforcementLevel::Optional);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&rule, "v", "m"));
        assert!(!result.has_blocking_violations());
    }

    #[test]
    fn test_governance_result_has_blocking_violations_returns_false_when_empty() {
        let result = GovernanceResult::new();
        assert!(!result.has_blocking_violations());
    }

    // ------------------------------------------------------------------
    // GovernanceResult::blocking_violations / non_blocking_violations
    // ------------------------------------------------------------------

    #[test]
    fn test_governance_result_blocking_violations_filters_required_only() {
        let req = make_rule("req", EnforcementLevel::Required);
        let rec = make_rule("rec", EnforcementLevel::Recommended);
        let opt = make_rule("opt", EnforcementLevel::Optional);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&req, "v1", "m1"));
        result
            .violations
            .push(GovernanceViolation::new(&rec, "v2", "m2"));
        result
            .violations
            .push(GovernanceViolation::new(&opt, "v3", "m3"));

        let blocking = result.blocking_violations();
        assert_eq!(blocking.len(), 1);
        assert_eq!(blocking[0].rule_id, "req");
    }

    #[test]
    fn test_governance_result_non_blocking_violations_excludes_required() {
        let req = make_rule("req", EnforcementLevel::Required);
        let rec = make_rule("rec", EnforcementLevel::Recommended);
        let opt = make_rule("opt", EnforcementLevel::Optional);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&req, "v1", "m1"));
        result
            .violations
            .push(GovernanceViolation::new(&rec, "v2", "m2"));
        result
            .violations
            .push(GovernanceViolation::new(&opt, "v3", "m3"));

        let non_blocking = result.non_blocking_violations();
        assert_eq!(non_blocking.len(), 2);
        let ids: Vec<&str> = non_blocking.iter().map(|v| v.rule_id.as_str()).collect();
        assert!(ids.contains(&"rec"));
        assert!(ids.contains(&"opt"));
    }

    #[test]
    fn test_governance_result_blocking_violations_returns_empty_when_no_required() {
        let rule = make_rule("r", EnforcementLevel::Recommended);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&rule, "v", "m"));
        assert!(result.blocking_violations().is_empty());
    }

    // ------------------------------------------------------------------
    // GovernanceResult::merge
    // ------------------------------------------------------------------

    #[test]
    fn test_governance_result_merge_combines_all_violations() {
        let rule = make_rule("r", EnforcementLevel::Required);
        let mut a = GovernanceResult::new();
        a.violations
            .push(GovernanceViolation::new(&rule, "v1", "m1"));

        let mut b = GovernanceResult::new();
        b.violations
            .push(GovernanceViolation::new(&rule, "v2", "m2"));
        b.violations
            .push(GovernanceViolation::new(&rule, "v3", "m3"));

        a.merge(b);
        assert_eq!(a.len(), 3);
        assert_eq!(a.violations[0].value, "v1");
        assert_eq!(a.violations[1].value, "v2");
        assert_eq!(a.violations[2].value, "v3");
    }

    #[test]
    fn test_governance_result_merge_with_empty_other_leaves_unchanged() {
        let rule = make_rule("r", EnforcementLevel::Required);
        let mut a = GovernanceResult::new();
        a.violations.push(GovernanceViolation::new(&rule, "v", "m"));
        a.merge(GovernanceResult::new());
        assert_eq!(a.len(), 1);
    }

    // ------------------------------------------------------------------
    // GovernanceResult::to_diagnostics
    // ------------------------------------------------------------------

    #[test]
    fn test_governance_result_to_diagnostics_required_produces_warning() {
        let rule = make_rule("gov.required", EnforcementLevel::Required);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&rule, "v", "msg"));

        let diags = result.to_diagnostics();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags.entries[0].level, DiagnosticLevel::Warning);
        assert_eq!(diags.entries[0].category, DiagnosticCategory::Governance);
    }

    #[test]
    fn test_governance_result_to_diagnostics_recommended_produces_warning() {
        let rule = make_rule("gov.recommended", EnforcementLevel::Recommended);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&rule, "v", "msg"));

        let diags = result.to_diagnostics();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags.entries[0].level, DiagnosticLevel::Warning);
    }

    #[test]
    fn test_governance_result_to_diagnostics_optional_produces_info() {
        let rule = make_rule("gov.optional", EnforcementLevel::Optional);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&rule, "v", "msg"));

        let diags = result.to_diagnostics();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags.entries[0].level, DiagnosticLevel::Info);
    }

    #[test]
    fn test_governance_result_to_diagnostics_message_includes_rule_id_and_message() {
        let rule = make_rule("gov.my_rule", EnforcementLevel::Required);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&rule, "v", "the violation reason"));

        let diags = result.to_diagnostics();
        let msg = &diags.entries[0].message;
        assert!(
            msg.contains("gov.my_rule"),
            "message should contain rule id, got: {}",
            msg
        );
        assert!(
            msg.contains("the violation reason"),
            "message should contain violation text, got: {}",
            msg
        );
    }

    #[test]
    fn test_governance_result_to_diagnostics_context_is_offending_value() {
        let rule = make_rule("gov.r", EnforcementLevel::Required);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&rule, "the_bad_value", "reason"));

        let diags = result.to_diagnostics();
        assert_eq!(diags.entries[0].context, Some("the_bad_value".to_string()));
    }

    #[test]
    fn test_governance_result_to_diagnostics_empty_result_produces_empty_diagnostics() {
        let result = GovernanceResult::new();
        let diags = result.to_diagnostics();
        assert!(diags.is_empty());
    }

    #[test]
    fn test_governance_result_to_diagnostics_multiple_violations_all_converted() {
        let req = make_rule("gov.req", EnforcementLevel::Required);
        let opt = make_rule("gov.opt", EnforcementLevel::Optional);
        let mut result = GovernanceResult::new();
        result
            .violations
            .push(GovernanceViolation::new(&req, "v1", "m1"));
        result
            .violations
            .push(GovernanceViolation::new(&opt, "v2", "m2"));

        let diags = result.to_diagnostics();
        assert_eq!(diags.len(), 2);
        assert_eq!(diags.entries[0].level, DiagnosticLevel::Warning);
        assert_eq!(diags.entries[1].level, DiagnosticLevel::Info);
    }

    // ------------------------------------------------------------------
    // EnforcementLevel derived traits
    // ------------------------------------------------------------------

    #[test]
    fn test_enforcement_level_clone_and_eq_work() {
        let level = EnforcementLevel::Required;
        let cloned = level.clone();
        assert_eq!(level, cloned);
    }

    #[test]
    fn test_enforcement_level_variants_are_not_equal_to_each_other() {
        assert_ne!(EnforcementLevel::Required, EnforcementLevel::Recommended);
        assert_ne!(EnforcementLevel::Required, EnforcementLevel::Optional);
        assert_ne!(EnforcementLevel::Recommended, EnforcementLevel::Optional);
    }

    // ------------------------------------------------------------------
    // RuleSource clone
    // ------------------------------------------------------------------

    #[test]
    fn test_rule_source_embedded_clone_works() {
        let src = RuleSource::Embedded;
        let _ = src.clone();
    }

    #[test]
    fn test_rule_source_repository_file_clone_works() {
        let src = RuleSource::RepositoryFile {
            path: std::path::PathBuf::from("rules.yaml"),
        };
        let cloned = src.clone();
        if let RuleSource::RepositoryFile { path } = cloned {
            assert_eq!(path, std::path::PathBuf::from("rules.yaml"));
        } else {
            panic!("unexpected variant after clone");
        }
    }
}
