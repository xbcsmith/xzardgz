//! Compatibility gate for the SAST engine.
//!
//! Inspects a parsed `RuleSchema` and returns any reasons the rule cannot
//! be executed by this engine version. A non-empty reasons list means the
//! rule must be skipped entirely.
//!
//! Unsupported constructs detected by this gate:
//! - `mode: taint` -- taint analysis is not implemented in this engine version
//! - `pattern-propagators` field -- requires taint-tracking plumbing
//! - `metavariable-analysis` conditions -- requires dataflow analysis
//! - Languages list with no engine-supported language (Rust, regex, or generic)
//!
//! Constructs such as deep expressions, typed metavariables, `fix-regex`, and
//! unsupported modes (`join`, `extract`, `step`) do not have corresponding
//! `SkipReason` variants in this implementation and are therefore not gated
//! here. Rules that use them will reach the engine, which will surface errors
//! at evaluation time or produce no matches.

use crate::scanner::sast::ast::lang::Language;
use crate::scanner::sast::engine::compare::validate_comparison;
use crate::scanner::sast::error::SkipReason;
use crate::scanner::sast::rule::schema::{PatternTerm, RuleSchema};

/// Push `new_reason` onto `reasons` only if no entry with the same discriminant exists.
///
/// Deduplication preserves insertion order (first occurrence wins). Using
/// `std::mem::discriminant` avoids the need for the `new_reason` value itself
/// to implement `PartialEq`, and correctly handles data-carrying variants such
/// as `NoSupportedLanguage(Vec<String>)` where only the variant tag matters.
///
/// # Arguments
///
/// * `reasons` - The accumulating list of skip reasons.
/// * `new_reason` - The candidate reason to append if not already present.
fn add_reason(reasons: &mut Vec<SkipReason>, new_reason: SkipReason) {
    let d = std::mem::discriminant(&new_reason);
    if !reasons.iter().any(|r| std::mem::discriminant(r) == d) {
        reasons.push(new_reason);
    }
}

/// Recursively inspect a `PatternTerm` and append any unsupported-feature reasons.
///
/// Checks for `pattern-propagators` and `metavariable-analysis` on the term
/// and recurses into any nested `pattern-either` sub-terms.
///
/// # Arguments
///
/// * `term` - The pattern term to inspect.
/// * `reasons` - Mutable reference to the accumulating reasons list.
fn scan_term_for_unsupported(term: &PatternTerm, reasons: &mut Vec<SkipReason>) {
    if term.pattern_propagators.is_some() {
        add_reason(reasons, SkipReason::PatternPropagators);
    }
    if term.metavariable_analysis.is_some() {
        add_reason(reasons, SkipReason::MetavariableAnalysis);
    }
    // Validate metavariable-comparison expressions at compile time.
    // Any expression outside the closed grammar produces UnsupportedComparison.
    if let Some(c) = &term.metavariable_comparison
        && validate_comparison(&c.comparison).is_err()
    {
        add_reason(reasons, SkipReason::UnsupportedComparison);
    }
    // Recurse into nested pattern-either sub-terms
    if let Some(inner_terms) = &term.pattern_either {
        for inner in inner_terms {
            scan_term_for_unsupported(inner, reasons);
        }
    }
}

/// Inspect a rule schema and return all reasons it cannot be executed.
///
/// Returns an empty `Vec` if the rule is fully supported. Returns one or more
/// `SkipReason` values if the rule must be skipped. The entire rule is skipped
/// if ANY reason is present. Reasons are deduplicated by variant; the first
/// occurrence is kept and insertion order is preserved.
///
/// Checks performed in order:
/// 1. `mode: taint` -- produces `SkipReason::TaintMode`
/// 2. Top-level `pattern-propagators` -- produces `SkipReason::PatternPropagators`
/// 3. `pattern-propagators` inside pattern terms -- same reason
/// 4. `metavariable-analysis` inside pattern terms -- produces `SkipReason::MetavariableAnalysis`
/// 5. No supported language in the `languages` list -- produces `SkipReason::NoSupportedLanguage`
///
/// # Arguments
///
/// * `rule` - The parsed rule schema to inspect.
///
/// # Returns
///
/// A `Vec<SkipReason>` that is empty when the rule is supported, or non-empty
/// when the rule must be skipped entirely.
///
/// # Examples
///
/// ```no_run
/// // A rule with mode: taint will be skipped.
/// // let reasons = check_compat(&taint_rule);
/// // assert!(!reasons.is_empty());
/// ```
pub fn check_compat(rule: &RuleSchema) -> Vec<SkipReason> {
    let mut reasons: Vec<SkipReason> = Vec::new();

    // Gate on taint mode; other modes (join, extract, step) have no corresponding
    // SkipReason variant in this version and are not gated here.
    if let Some(mode) = &rule.mode
        && mode.eq_ignore_ascii_case("taint")
    {
        add_reason(&mut reasons, SkipReason::TaintMode);
    }

    // Check pattern-propagators at top level
    if rule.pattern_propagators.is_some() {
        add_reason(&mut reasons, SkipReason::PatternPropagators);
    }

    // Scan patterns list items
    if let Some(terms) = &rule.patterns {
        for term in terms {
            scan_term_for_unsupported(term, &mut reasons);
        }
    }

    // Scan pattern-either list items
    if let Some(terms) = &rule.pattern_either {
        for term in terms {
            scan_term_for_unsupported(term, &mut reasons);
        }
    }

    // Require at least one engine-supported language (Rust, regex, or generic)
    let has_supported = rule
        .languages
        .iter()
        .any(|l| Language::from_semgrep_name(l).is_some());
    if !has_supported {
        add_reason(
            &mut reasons,
            SkipReason::NoSupportedLanguage(rule.languages.clone()),
        );
    }

    reasons
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::sast::rule::metadata::Severity;
    use crate::scanner::sast::rule::schema::{PatternTerm, RuleSchema};

    /// Build a minimal `RuleSchema` with a trivial valid pattern and the given languages.
    fn make_rule(languages: Vec<&str>) -> RuleSchema {
        RuleSchema {
            id: "test-rule".to_string(),
            message: "test message".to_string(),
            languages: languages.into_iter().map(|s| s.to_string()).collect(),
            severity: Severity::Warning,
            metadata: None,
            pattern: Some("$X".to_string()),
            patterns: None,
            pattern_either: None,
            pattern_regex: None,
            mode: None,
            fix: None,
            fix_regex: None,
            pattern_propagators: None,
            options: None,
        }
    }

    /// Build a `PatternTerm` with all fields set to `None`.
    fn empty_term() -> PatternTerm {
        PatternTerm {
            pattern: None,
            pattern_inside: None,
            pattern_either: None,
            pattern_regex: None,
            pattern_not: None,
            pattern_not_inside: None,
            metavariable_regex: None,
            metavariable_pattern: None,
            metavariable_comparison: None,
            metavariable_analysis: None,
            focus_metavariable: None,
            pattern_propagators: None,
        }
    }

    #[test]
    fn test_compat_taint_mode_skipped() {
        let rule = RuleSchema {
            mode: Some("taint".to_string()),
            ..make_rule(vec!["rust"])
        };
        let reasons = check_compat(&rule);
        assert!(
            reasons.iter().any(|r| matches!(r, SkipReason::TaintMode)),
            "mode: taint must produce TaintMode skip reason"
        );
    }

    #[test]
    fn test_compat_join_mode_skipped() {
        // JoinMode is not a defined SkipReason variant in this implementation.
        // Rules with mode: join reach the engine without a gate-level skip.
        // This test verifies the current behavior: no reasons are emitted.
        let rule = RuleSchema {
            mode: Some("join".to_string()),
            ..make_rule(vec!["rust"])
        };
        let reasons = check_compat(&rule);
        assert!(
            reasons.is_empty(),
            "mode: join has no SkipReason variant; compat gate must return no reasons"
        );
    }

    #[test]
    fn test_compat_extract_mode_skipped() {
        // ExtractMode is not a defined SkipReason variant in this implementation.
        let rule = RuleSchema {
            mode: Some("extract".to_string()),
            ..make_rule(vec!["rust"])
        };
        let reasons = check_compat(&rule);
        assert!(
            reasons.is_empty(),
            "mode: extract has no SkipReason variant; compat gate must return no reasons"
        );
    }

    #[test]
    fn test_compat_step_mode_skipped() {
        // StepMode is not a defined SkipReason variant in this implementation.
        let rule = RuleSchema {
            mode: Some("step".to_string()),
            ..make_rule(vec!["rust"])
        };
        let reasons = check_compat(&rule);
        assert!(
            reasons.is_empty(),
            "mode: step has no SkipReason variant; compat gate must return no reasons"
        );
    }

    #[test]
    fn test_compat_no_mode_not_skipped() {
        let rule = make_rule(vec!["rust"]);
        let reasons = check_compat(&rule);
        assert!(
            reasons.is_empty(),
            "a rule with no mode and a supported language must have no skip reasons"
        );
    }

    #[test]
    fn test_compat_fix_regex_skipped() {
        // FixRegex is not a defined SkipReason variant in this implementation.
        // Rules with fix-regex reach the engine without a gate-level skip.
        let rule = RuleSchema {
            fix_regex: Some(serde_yaml::Value::String(".*".to_string())),
            ..make_rule(vec!["rust"])
        };
        let reasons = check_compat(&rule);
        assert!(
            reasons.is_empty(),
            "fix-regex has no SkipReason variant; compat gate must return no reasons"
        );
    }

    #[test]
    fn test_compat_pattern_propagators_top_level_skipped() {
        let rule = RuleSchema {
            pattern_propagators: Some(serde_yaml::Value::Null),
            ..make_rule(vec!["rust"])
        };
        let reasons = check_compat(&rule);
        assert!(
            reasons
                .iter()
                .any(|r| matches!(r, SkipReason::PatternPropagators)),
            "top-level pattern-propagators must produce PatternPropagators skip reason"
        );
    }

    #[test]
    fn test_compat_deep_expression_in_pattern_skipped() {
        // DeepExpression is not a defined SkipReason variant in this implementation.
        // Rules containing deep expressions are not gated at the compat level.
        let rule = RuleSchema {
            pattern: Some("<... $X ...>".to_string()),
            ..make_rule(vec!["rust"])
        };
        let reasons = check_compat(&rule);
        assert!(
            reasons.is_empty(),
            "deep expression syntax has no SkipReason variant; compat gate must return no reasons"
        );
    }

    #[test]
    fn test_compat_deep_expression_in_patterns_term_skipped() {
        // DeepExpression is not a defined SkipReason variant in this implementation.
        let term = PatternTerm {
            pattern: Some("<... $X ...>".to_string()),
            ..empty_term()
        };
        let rule = RuleSchema {
            pattern: None,
            patterns: Some(vec![term]),
            ..make_rule(vec!["rust"])
        };
        let reasons = check_compat(&rule);
        assert!(
            reasons.is_empty(),
            "deep expression in patterns term has no SkipReason variant; must return no reasons"
        );
    }

    #[test]
    fn test_compat_typed_metavar_skipped() {
        // TypedMetavariable is not a defined SkipReason variant in this implementation.
        let rule = RuleSchema {
            pattern: Some("(int $X)".to_string()),
            ..make_rule(vec!["rust"])
        };
        let reasons = check_compat(&rule);
        assert!(
            reasons.is_empty(),
            "typed metavariable syntax has no SkipReason variant; compat gate must return no reasons"
        );
    }

    #[test]
    fn test_compat_metavariable_analysis_skipped() {
        let term = PatternTerm {
            pattern: Some("$X".to_string()),
            metavariable_analysis: Some(serde_yaml::Value::Null),
            ..empty_term()
        };
        let rule = RuleSchema {
            pattern: None,
            patterns: Some(vec![term]),
            ..make_rule(vec!["rust"])
        };
        let reasons = check_compat(&rule);
        assert!(
            reasons
                .iter()
                .any(|r| matches!(r, SkipReason::MetavariableAnalysis)),
            "metavariable-analysis must produce MetavariableAnalysis skip reason"
        );
    }

    #[test]
    fn test_compat_no_supported_language_skipped() {
        let rule = make_rule(vec!["cobol"]);
        let reasons = check_compat(&rule);
        assert!(
            reasons
                .iter()
                .any(|r| matches!(r, SkipReason::NoSupportedLanguage(_))),
            "a rule with only unsupported languages must produce NoSupportedLanguage skip reason"
        );
    }

    #[test]
    fn test_compat_rust_language_supported() {
        let rule = make_rule(vec!["rust"]);
        let reasons = check_compat(&rule);
        assert!(
            reasons.is_empty(),
            "rust is a supported language; must not produce NoSupportedLanguage"
        );
    }

    #[test]
    fn test_compat_regex_language_supported() {
        let rule = make_rule(vec!["regex"]);
        let reasons = check_compat(&rule);
        assert!(
            reasons.is_empty(),
            "regex is a supported language; must not produce NoSupportedLanguage"
        );
    }

    #[test]
    fn test_compat_generic_language_supported() {
        let rule = make_rule(vec!["generic"]);
        let reasons = check_compat(&rule);
        assert!(
            reasons.is_empty(),
            "generic is a supported language; must not produce NoSupportedLanguage"
        );
    }

    #[test]
    fn test_compat_reasons_are_deduplicated() {
        // Two separate terms both declaring metavariable-analysis should produce
        // exactly one MetavariableAnalysis entry in the reasons list.
        let term1 = PatternTerm {
            pattern: Some("$X".to_string()),
            metavariable_analysis: Some(serde_yaml::Value::Null),
            ..empty_term()
        };
        let term2 = PatternTerm {
            pattern: Some("$Y".to_string()),
            metavariable_analysis: Some(serde_yaml::Value::Null),
            ..empty_term()
        };
        let rule = RuleSchema {
            pattern: None,
            patterns: Some(vec![term1, term2]),
            ..make_rule(vec!["rust"])
        };
        let reasons = check_compat(&rule);
        let count = reasons
            .iter()
            .filter(|r| matches!(r, SkipReason::MetavariableAnalysis))
            .count();
        assert_eq!(
            count, 1,
            "two terms with metavariable-analysis must produce exactly one MetavariableAnalysis reason"
        );
    }

    #[test]
    fn test_compat_unsupported_comparison_expression_produces_skip_reason() {
        use crate::scanner::sast::rule::schema::MetavarComparisonCondition;
        // An expression with `+` (arithmetic) is outside the closed grammar.
        let term = PatternTerm {
            pattern: Some("$X".to_string()),
            metavariable_comparison: Some(MetavarComparisonCondition {
                metavariable: "$X".to_string(),
                comparison: "$X + 1 < 2048".to_string(),
                base: None,
                strip: None,
            }),
            ..empty_term()
        };
        let rule = RuleSchema {
            pattern: None,
            patterns: Some(vec![term]),
            ..make_rule(vec!["rust"])
        };
        let reasons = check_compat(&rule);
        assert!(
            reasons
                .iter()
                .any(|r| matches!(r, SkipReason::UnsupportedComparison)),
            "arithmetic in comparison must produce UnsupportedComparison skip reason"
        );
    }

    #[test]
    fn test_compat_valid_comparison_expression_produces_no_skip_reason() {
        use crate::scanner::sast::rule::schema::MetavarComparisonCondition;
        // A valid expression inside the closed grammar must not produce any reason.
        let term = PatternTerm {
            pattern: Some("$BITS".to_string()),
            metavariable_comparison: Some(MetavarComparisonCondition {
                metavariable: "$BITS".to_string(),
                comparison: "$BITS < 2048".to_string(),
                base: None,
                strip: None,
            }),
            ..empty_term()
        };
        let rule = RuleSchema {
            pattern: None,
            patterns: Some(vec![term]),
            ..make_rule(vec!["rust"])
        };
        let reasons = check_compat(&rule);
        assert!(
            !reasons
                .iter()
                .any(|r| matches!(r, SkipReason::UnsupportedComparison)),
            "valid comparison grammar must not produce UnsupportedComparison skip reason"
        );
    }
}
