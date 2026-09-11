//! Intermediate representation (IR) for compiled SAST rules.
//!
//! The IR is produced by the rule parser from the raw YAML schema and consumed
//! by the matching engine. It is intentionally kept separate from the serde
//! schema types so that the engine can make correctness assumptions that the
//! raw YAML surface does not provide.
//!
//! # Formula tree
//!
//! Every rule's matching logic is expressed as a [`Formula`] tree:
//!
//! ```text
//! Formula
//!   Leaf(Leaf::Regex("..."))       -- pattern-regex leaf
//!   Leaf(Leaf::Pattern("..."))     -- pattern leaf
//!   And { conjuncts, negations, conditions, focus }
//!   Or([Formula, ...])
//!   Inside(Box<Formula>)
//! ```
//!
//! The `And` node carries negations, metavariable conditions, and focus
//! metavariables as siblings rather than nested sub-nodes.

use serde::{Deserialize, Serialize};

use super::metadata::{RuleMetadata, Severity};
use crate::scanner::sast::error::SkipReason;

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

/// Identifier for a named metavariable (e.g. `$FOO`, `$VALUE`).
///
/// The `$` prefix is included in the stored string.
pub type MetavarId = String;

// ---------------------------------------------------------------------------
// Leaf
// ---------------------------------------------------------------------------

/// A terminal pattern node in the formula tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Leaf {
    /// An AST-based `pattern:` expression.
    Pattern(String),
    /// A raw regex `pattern-regex:` expression.
    Regex(String),
}

// ---------------------------------------------------------------------------
// Condition
// ---------------------------------------------------------------------------

/// A constraint on a named metavariable used inside a `patterns:` block.
///
/// Corresponds to `metavariable-regex`, `metavariable-pattern`, and
/// `metavariable-comparison` keys in the Semgrep YAML schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    /// Metavariable must match (or not match) a regular expression.
    MetavarRegex {
        /// The metavariable whose bound text is tested (e.g. `"$X"`).
        metavar: MetavarId,
        /// The regular expression.
        regex: String,
        /// When `true`, the condition is negated: the metavariable must NOT match.
        not: bool,
    },
    /// Metavariable must match a structural pattern, optionally in another language.
    MetavarPattern {
        /// The metavariable whose bound code is re-parsed and tested.
        metavar: MetavarId,
        /// The structural pattern the bound code must match.
        pattern: String,
        /// Optional language override for re-parsing the bound code.
        language: Option<String>,
    },
    /// Metavariable must satisfy a numeric comparison expression.
    MetavarComparison {
        /// The metavariable whose numeric value is tested.
        metavar: MetavarId,
        /// The comparison expression (e.g. `"$BITS < 2048"`).
        comparison: String,
    },
}

// ---------------------------------------------------------------------------
// Formula
// ---------------------------------------------------------------------------

/// A node in the compiled formula tree for a SAST rule.
///
/// The tree is produced by the rule parser and consumed by the matching
/// engine. Each variant corresponds to one or more Semgrep pattern
/// combinators.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Formula {
    /// A terminal pattern (AST-based or regex).
    Leaf(Leaf),

    /// Conjunction: all conjuncts must match; none of the negations may match;
    /// all conditions must hold; focus restricts the reported range to the
    /// listed metavariables.
    And {
        /// Positive sub-patterns that must all match.
        conjuncts: Vec<Formula>,
        /// Sub-patterns that must not match (`pattern-not*`).
        negations: Vec<Formula>,
        /// Metavariable constraints (`metavariable-*`).
        conditions: Vec<Condition>,
        /// Metavariables to focus the reported match on.
        focus: Vec<MetavarId>,
    },

    /// Disjunction: at least one alternative must match (`pattern-either`).
    Or(Vec<Formula>),

    /// Containment: the inner formula must match somewhere inside the range
    /// matched by the enclosing formula (`pattern-inside`).
    Inside(Box<Formula>),
}

// ---------------------------------------------------------------------------
// CompileOutcome
// ---------------------------------------------------------------------------

/// The outcome of compiling a rule schema into a [`RuleIr`].
///
/// Callers should pattern-match on this type to distinguish successfully
/// compiled rules from those skipped due to unsupported constructs.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum CompileOutcome {
    /// The rule was compiled successfully and is ready for evaluation.
    Compiled(RuleIr),
    /// The rule used unsupported constructs and was not compiled.
    Skipped {
        /// The `id` field of the rule that was skipped.
        rule_id: String,
        /// One or more reasons explaining which constructs blocked compilation.
        reasons: Vec<SkipReason>,
    },
}

// ---------------------------------------------------------------------------
// RuleIr
// ---------------------------------------------------------------------------

/// Compiled intermediate representation of a single SAST rule.
///
/// Instances are produced by the rule parser and handed to the matching
/// engine for evaluation against source files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleIr {
    /// Unique rule identifier (matches `^[a-zA-Z0-9._-]+$`).
    pub id: String,
    /// Human-readable message emitted when the rule matches.
    pub message: String,
    /// Language identifiers this rule applies to (e.g. `["rust"]`, `["regex"]`).
    pub languages: Vec<String>,
    /// Severity level of a match produced by this rule.
    pub severity: Severity,
    /// Optional structured metadata (CWE, OWASP, confidence, etc.).
    pub metadata: Option<RuleMetadata>,
    /// The compiled formula tree describing the matching logic.
    pub formula: Formula,
    /// Optional autofix template to apply at match sites.
    pub fix: Option<String>,
}

impl RuleIr {
    /// Returns `true` if this rule applies to Rust source files.
    ///
    /// The comparison is case-insensitive so both `"rust"` and `"Rust"` are accepted.
    pub fn applies_to_rust(&self) -> bool {
        self.languages
            .iter()
            .any(|l| l.eq_ignore_ascii_case("rust"))
    }

    /// Returns `true` if this rule should be evaluated in regex mode.
    ///
    /// Regex mode is used when `languages` contains `"regex"` or `"generic"`.
    /// No AST parsing is performed; the `pattern-regex` is applied directly
    /// over raw file bytes.
    pub fn applies_to_regex_mode(&self) -> bool {
        self.languages
            .iter()
            .any(|l| l == "regex" || l == "generic")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::sast::rule::metadata::Severity;

    fn make_simple_rule(languages: Vec<&str>) -> RuleIr {
        RuleIr {
            id: "test-rule".to_string(),
            message: "test message".to_string(),
            languages: languages.into_iter().map(str::to_string).collect(),
            severity: Severity::Warning,
            metadata: None,
            formula: Formula::Leaf(Leaf::Pattern("$X".to_string())),
            fix: None,
        }
    }

    #[test]
    fn test_applies_to_regex_mode_with_regex_language_returns_true() {
        let rule = make_simple_rule(vec!["regex"]);
        assert!(rule.applies_to_regex_mode());
    }

    #[test]
    fn test_applies_to_regex_mode_with_generic_language_returns_true() {
        let rule = make_simple_rule(vec!["generic"]);
        assert!(rule.applies_to_regex_mode());
    }

    #[test]
    fn test_applies_to_regex_mode_with_rust_language_returns_false() {
        let rule = make_simple_rule(vec!["rust"]);
        assert!(!rule.applies_to_regex_mode());
    }

    #[test]
    fn test_applies_to_regex_mode_with_mixed_languages_containing_regex_returns_true() {
        let rule = make_simple_rule(vec!["rust", "regex"]);
        assert!(rule.applies_to_regex_mode());
    }

    #[test]
    fn test_applies_to_regex_mode_with_empty_languages_returns_false() {
        let rule = make_simple_rule(vec![]);
        assert!(!rule.applies_to_regex_mode());
    }

    #[test]
    fn test_formula_leaf_regex_equality() {
        let f1 = Formula::Leaf(Leaf::Regex("foo".to_string()));
        let f2 = Formula::Leaf(Leaf::Regex("foo".to_string()));
        assert_eq!(f1, f2);
    }

    #[test]
    fn test_formula_and_carries_negations() {
        let f = Formula::And {
            conjuncts: vec![Formula::Leaf(Leaf::Pattern("$X".to_string()))],
            negations: vec![Formula::Leaf(Leaf::Pattern("$Y".to_string()))],
            conditions: vec![],
            focus: vec![],
        };
        if let Formula::And { negations, .. } = f {
            assert_eq!(negations.len(), 1);
        } else {
            panic!("expected And variant");
        }
    }

    #[test]
    fn test_formula_or_contains_expected_number_of_alternatives() {
        let f = Formula::Or(vec![
            Formula::Leaf(Leaf::Regex("foo".to_string())),
            Formula::Leaf(Leaf::Regex("bar".to_string())),
        ]);
        if let Formula::Or(alts) = f {
            assert_eq!(alts.len(), 2);
        } else {
            panic!("expected Or variant");
        }
    }

    #[test]
    fn test_rule_ir_clone_produces_equal_value() {
        let rule = make_simple_rule(vec!["rust"]);
        assert_eq!(rule.clone(), rule);
    }
}
