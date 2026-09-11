//! Rule schema compiler: `RuleSchema` -> `CompileOutcome`.
//!
//! Enforces structural invariants on the rule before compiling to IR:
//! 1. Rule id matches `^[a-zA-Z0-9._-]+$`
//! 2. Exactly one formula root: `pattern`, `patterns`, `pattern-either`, or `pattern-regex`
//! 3. `pattern-not*` only inside `patterns:` or `pattern-either:` (enforced by structure)
//! 4. `patterns:` must have at least one positive term
//! 5. `metavariable-*` and `focus-metavariable` only under `patterns:` or `pattern-either:`
//!
//! Also calls the compatibility gate first; returns `Skipped` if any reasons are found.

use crate::scanner::sast::error::RuleParseError;
use crate::scanner::sast::rule::compat;
use crate::scanner::sast::rule::ir::{CompileOutcome, Condition, Formula, Leaf, RuleIr};
use crate::scanner::sast::rule::schema::{PatternTerm, RuleFile, RuleSchema};

/// Return `true` if `id` is a valid rule identifier.
///
/// A valid identifier is non-empty and contains only ASCII alphanumeric
/// characters, dots (`.`), underscores (`_`), or hyphens (`-`). This is
/// equivalent to matching `^[a-zA-Z0-9._-]+$` without requiring the `regex`
/// crate.
///
/// # Arguments
///
/// * `id` - The candidate rule identifier.
///
/// # Returns
///
/// `true` when the identifier is valid.
///
/// # Examples
///
/// ```ignore
/// assert!(is_valid_rule_id("my-rule.v2_alpha"));
/// assert!(!is_valid_rule_id("invalid rule"));
/// assert!(!is_valid_rule_id(""));
/// ```
fn is_valid_rule_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' || c == '/')
}

/// Compile a single `PatternTerm` to a `Formula`.
///
/// Used for items inside `pattern-either` lists and for nested `pattern-either`
/// sub-terms. Only positive constructs are handled here; negation and conditions
/// belong in `compile_patterns`.
///
/// # Arguments
///
/// * `rule_id` - The enclosing rule identifier, used in error messages.
/// * `term` - The pattern term to compile.
///
/// # Returns
///
/// `Ok(Formula)` when the term contains a recognised positive construct.
///
/// # Errors
///
/// Returns `RuleParseError::Invariant` when the term has no positive construct.
///
/// # Examples
///
/// ```no_run
/// # use xzardgz::scanner::sast::rule::parse::compile_rule;
/// # use xzardgz::scanner::sast::rule::schema::PatternTerm;
/// // compile_term is private; exercise it through compile_rule.
/// ```
fn compile_term(rule_id: &str, term: &PatternTerm) -> Result<Formula, RuleParseError> {
    if let Some(s) = &term.pattern {
        return Ok(Formula::Leaf(Leaf::Pattern(s.clone())));
    }
    if let Some(s) = &term.pattern_inside {
        return Ok(Formula::Inside(Box::new(Formula::Leaf(Leaf::Pattern(
            s.clone(),
        )))));
    }
    if let Some(s) = &term.pattern_regex {
        return Ok(Formula::Leaf(Leaf::Regex(s.clone())));
    }
    if let Some(inner_terms) = &term.pattern_either {
        let branches = inner_terms
            .iter()
            .map(|t| compile_term(rule_id, t))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Formula::Or(branches));
    }
    Err(RuleParseError::Invariant {
        rule_id: rule_id.to_string(),
        reason: "pattern term has no positive construct (pattern, pattern-inside, \
             pattern-regex, or pattern-either)"
            .to_string(),
    })
}

/// Compile a `patterns` list into a `Formula::And`.
///
/// Distributes each `PatternTerm` into the appropriate bucket. A single term
/// may contribute to multiple buckets simultaneously (for example, a term can
/// have a positive `pattern` field and a `metavariable-regex` condition):
///
/// - **conjuncts**: terms with `pattern`, `pattern-inside`, `pattern-regex`,
///   or `pattern-either`
/// - **negations**: terms with `pattern-not` or `pattern-not-inside`
/// - **conditions**: terms with `metavariable-regex`, `metavariable-pattern`,
///   or `metavariable-comparison`
/// - **focus**: metavariable identifiers from `focus-metavariable`
///
/// # Arguments
///
/// * `rule_id` - The enclosing rule identifier, used in error messages.
/// * `terms` - The slice of pattern terms from the `patterns` list.
///
/// # Returns
///
/// `Ok(Formula::And { .. })` when the list is valid.
///
/// # Errors
///
/// Returns `RuleParseError::Invariant` when the list is empty or contains no
/// positive term.
fn compile_patterns(rule_id: &str, terms: &[PatternTerm]) -> Result<Formula, RuleParseError> {
    if terms.is_empty() {
        return Err(RuleParseError::Invariant {
            rule_id: rule_id.to_string(),
            reason: "patterns list must not be empty".to_string(),
        });
    }

    let mut conjuncts: Vec<Formula> = Vec::new();
    let mut negations: Vec<Formula> = Vec::new();
    let mut conditions: Vec<Condition> = Vec::new();
    let mut focus: Vec<String> = Vec::new();

    for term in terms {
        // Positive conjunct: at most one positive construct per term
        if let Some(s) = &term.pattern {
            conjuncts.push(Formula::Leaf(Leaf::Pattern(s.clone())));
        } else if let Some(s) = &term.pattern_inside {
            conjuncts.push(Formula::Inside(Box::new(Formula::Leaf(Leaf::Pattern(
                s.clone(),
            )))));
        } else if let Some(s) = &term.pattern_regex {
            conjuncts.push(Formula::Leaf(Leaf::Regex(s.clone())));
        } else if let Some(inner_terms) = &term.pattern_either {
            let branches = inner_terms
                .iter()
                .map(|t| compile_term(rule_id, t))
                .collect::<Result<Vec<_>, _>>()?;
            conjuncts.push(Formula::Or(branches));
        }

        // Negations (do not conflict with positive constructs above)
        if let Some(s) = &term.pattern_not {
            negations.push(Formula::Leaf(Leaf::Pattern(s.clone())));
        }
        if let Some(s) = &term.pattern_not_inside {
            negations.push(Formula::Inside(Box::new(Formula::Leaf(Leaf::Pattern(
                s.clone(),
            )))));
        }

        // Metavariable conditions (may coexist with a positive construct)
        if let Some(c) = &term.metavariable_regex {
            conditions.push(Condition::MetavarRegex {
                metavar: c.metavariable.clone(),
                regex: c.regex.clone(),
                not: c.not,
            });
        }
        if let Some(c) = &term.metavariable_pattern {
            // Prefer the pattern field; fall back to pattern_regex; default to empty.
            let pattern = c
                .pattern
                .clone()
                .or_else(|| c.pattern_regex.clone())
                .unwrap_or_default();
            conditions.push(Condition::MetavarPattern {
                metavar: c.metavariable.clone(),
                pattern,
                language: c.language.clone(),
            });
        }
        if let Some(c) = &term.metavariable_comparison {
            conditions.push(Condition::MetavarComparison {
                metavar: c.metavariable.clone(),
                comparison: c.comparison.clone(),
                strip: c.strip.unwrap_or(false),
                base: c.base,
            });
        }

        // Focus metavariable identifiers
        if let Some(f) = &term.focus_metavariable {
            focus.extend(f.as_slice().iter().cloned());
        }
    }

    if conjuncts.is_empty() {
        return Err(RuleParseError::Invariant {
            rule_id: rule_id.to_string(),
            reason: "patterns must contain at least one positive term (pattern, pattern-inside, \
                 pattern-regex, or pattern-either)"
                .to_string(),
        });
    }

    Ok(Formula::And {
        conjuncts,
        negations,
        conditions,
        focus,
    })
}

/// Compile a parsed `RuleSchema` into a `CompileOutcome`.
///
/// Checks the compatibility gate first; if the rule is unsupported, returns
/// `Ok(CompileOutcome::Skipped)`. If the rule has structural errors, returns
/// `Err(RuleParseError)`. If the rule is fully valid and supported, returns
/// `Ok(CompileOutcome::Compiled(RuleIr))`.
///
/// Invariants enforced (in order):
/// 1. Compatibility gate passes (no unsupported constructs)
/// 2. Rule id matches `^[a-zA-Z0-9._-]+$`
/// 3. Exactly one formula root is present
/// 4. `patterns` list (when used) has at least one positive term
///
/// # Arguments
///
/// * `schema` - The parsed rule schema to compile.
///
/// # Returns
///
/// `Ok(CompileOutcome)` on success. The outcome may be either `Compiled` or
/// `Skipped` depending on the compatibility gate result.
///
/// # Errors
///
/// Returns `RuleParseError` when the rule has a structural or schema violation.
///
/// # Examples
///
/// ```no_run
/// # use xzardgz::scanner::sast::rule::parse::compile_rule;
/// # use xzardgz::scanner::sast::rule::ir::CompileOutcome;
/// # use xzardgz::scanner::sast::rule::schema::RuleSchema;
/// # let schema: RuleSchema = unimplemented!();
/// let outcome = compile_rule(&schema).expect("compile must succeed");
/// assert!(matches!(outcome, CompileOutcome::Compiled(_)));
/// ```
pub fn compile_rule(schema: &RuleSchema) -> Result<CompileOutcome, RuleParseError> {
    // Step 1: compatibility gate
    let skip_reasons = compat::check_compat(schema);
    if !skip_reasons.is_empty() {
        return Ok(CompileOutcome::Skipped {
            rule_id: schema.id.clone(),
            reasons: skip_reasons,
        });
    }

    // Step 2: validate rule id
    if !is_valid_rule_id(&schema.id) {
        return Err(RuleParseError::InvalidId {
            id: schema.id.clone(),
            reason: "rule id must contain only ASCII alphanumeric characters, \
                     dots, underscores, or hyphens"
                .to_string(),
        });
    }

    // Step 3: exactly one formula root
    let root_count = [
        schema.pattern.is_some(),
        schema.patterns.is_some(),
        schema.pattern_either.is_some(),
        schema.pattern_regex.is_some(),
    ]
    .iter()
    .filter(|&&b| b)
    .count();

    if root_count != 1 {
        return Err(RuleParseError::Schema {
            rule_id: schema.id.clone(),
            reason: format!(
                "exactly one formula root required (pattern, patterns, pattern-either, \
                 or pattern-regex); found {root_count}"
            ),
        });
    }

    // Step 4: compile formula
    let formula = if let Some(s) = &schema.pattern {
        Formula::Leaf(Leaf::Pattern(s.clone()))
    } else if let Some(s) = &schema.pattern_regex {
        Formula::Leaf(Leaf::Regex(s.clone()))
    } else if let Some(terms) = &schema.patterns {
        compile_patterns(&schema.id, terms)?
    } else if let Some(terms) = &schema.pattern_either {
        let branches = terms
            .iter()
            .map(|t| compile_term(&schema.id, t))
            .collect::<Result<Vec<_>, _>>()?;
        Formula::Or(branches)
    } else {
        // Unreachable: root_count == 1 guarantees one arm is taken above.
        return Err(RuleParseError::Schema {
            rule_id: schema.id.clone(),
            reason: "internal: formula root count mismatch".to_string(),
        });
    };

    // Step 5: assemble and return the IR node
    Ok(CompileOutcome::Compiled(RuleIr {
        id: schema.id.clone(),
        message: schema.message.clone(),
        languages: schema.languages.clone(),
        severity: schema.severity.clone(),
        metadata: schema.metadata.clone(),
        formula,
        fix: schema.fix.clone(),
    }))
}

/// Compile all rules from a `RuleFile`.
///
/// Each rule is compiled independently. A parse error from one rule does not
/// prevent other rules from being compiled.
///
/// # Arguments
///
/// * `file` - The parsed rule file containing one or more rule schemas.
///
/// # Returns
///
/// A `Vec` with one entry per rule. Each entry is `Ok(CompileOutcome)` on
/// success or `Err(RuleParseError)` when a structural invariant is violated.
///
/// # Examples
///
/// ```no_run
/// # use xzardgz::scanner::sast::rule::parse::compile_rule_file;
/// # use xzardgz::scanner::sast::rule::schema::RuleFile;
/// # let rule_file: RuleFile = unimplemented!();
/// let outcomes = compile_rule_file(&rule_file);
/// for result in &outcomes {
///     match result {
///         Ok(outcome) => { /* handle outcome */ }
///         Err(e) => { /* handle parse error */ }
///     }
/// }
/// ```
pub fn compile_rule_file(file: &RuleFile) -> Vec<Result<CompileOutcome, RuleParseError>> {
    file.rules.iter().map(compile_rule).collect()
}

/// Parse a YAML string as a semgrep rule file and compile all rules.
///
/// Combines YAML parsing with rule compilation in a single step. A failure to
/// parse the YAML returns `Err`; individual rule compile failures appear as
/// `Err` entries inside the returned `Vec`.
///
/// # Arguments
///
/// * `yaml` - A YAML string conforming to the semgrep rule file format.
///
/// # Returns
///
/// `Ok(Vec<Result<CompileOutcome, RuleParseError>>)` when the YAML is
/// structurally valid. Each inner `Result` corresponds to one rule.
///
/// # Errors
///
/// Returns `RuleParseError::Yaml` when the YAML is malformed or cannot be
/// deserialized into a `RuleFile`.
///
/// # Examples
///
/// ```no_run
/// # use xzardgz::scanner::sast::rule::parse::parse_and_compile;
/// let yaml = "rules:\n  - id: my-rule\n  ...";
/// let results = parse_and_compile(yaml).expect("yaml must be valid");
/// ```
pub fn parse_and_compile(
    yaml: &str,
) -> Result<Vec<Result<CompileOutcome, RuleParseError>>, RuleParseError> {
    let file: RuleFile =
        serde_yaml::from_str(yaml).map_err(|e| RuleParseError::Yaml(e.to_string()))?;
    Ok(compile_rule_file(&file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::sast::error::{RuleParseError, SkipReason};
    use crate::scanner::sast::rule::ir::{CompileOutcome, Formula, Leaf};
    use crate::scanner::sast::rule::metadata::{Severity, StringOrVec};
    use crate::scanner::sast::rule::schema::{MetavarRegexCondition, PatternTerm, RuleSchema};

    /// Build a base `RuleSchema` with no formula root set.
    ///
    /// Individual tests add exactly the formula fields they need.
    fn make_rule_schema(id: &str, languages: Vec<&str>) -> RuleSchema {
        RuleSchema {
            id: id.to_string(),
            message: "test message".to_string(),
            languages: languages.into_iter().map(|s| s.to_string()).collect(),
            severity: Severity::Warning,
            metadata: None,
            pattern: None,
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
    fn test_compile_rule_simple_pattern() {
        let schema = RuleSchema {
            pattern: Some("fn $F() {}".to_string()),
            ..make_rule_schema("test-rule", vec!["rust"])
        };
        let result = compile_rule(&schema).expect("compile_rule must succeed");
        match result {
            CompileOutcome::Compiled(ir) => {
                assert_eq!(ir.id, "test-rule");
                assert!(
                    matches!(ir.formula, Formula::Leaf(Leaf::Pattern(_))),
                    "expected Leaf::Pattern formula"
                );
            }
            CompileOutcome::Skipped { .. } => panic!("expected Compiled, got Skipped"),
        }
    }

    #[test]
    fn test_compile_rule_pattern_regex() {
        let schema = RuleSchema {
            pattern_regex: Some(".*secret.*".to_string()),
            ..make_rule_schema("regex-rule", vec!["rust"])
        };
        let result = compile_rule(&schema).expect("compile_rule must succeed");
        match result {
            CompileOutcome::Compiled(ir) => {
                assert!(
                    matches!(ir.formula, Formula::Leaf(Leaf::Regex(_))),
                    "expected Leaf::Regex formula"
                );
            }
            CompileOutcome::Skipped { .. } => panic!("expected Compiled, got Skipped"),
        }
    }

    #[test]
    fn test_compile_rule_patterns_with_pattern_not() {
        let positive = PatternTerm {
            pattern: Some("fn $F() {}".to_string()),
            ..empty_term()
        };
        let negative = PatternTerm {
            pattern_not: Some("fn test_$F() {}".to_string()),
            ..empty_term()
        };
        let schema = RuleSchema {
            patterns: Some(vec![positive, negative]),
            ..make_rule_schema("patterns-rule", vec!["rust"])
        };
        let result = compile_rule(&schema).expect("compile_rule must succeed");
        match result {
            CompileOutcome::Compiled(ir) => match ir.formula {
                Formula::And {
                    conjuncts,
                    negations,
                    conditions,
                    focus,
                } => {
                    assert_eq!(conjuncts.len(), 1, "one positive conjunct expected");
                    assert_eq!(negations.len(), 1, "one negation expected");
                    assert!(conditions.is_empty(), "no conditions expected");
                    assert!(focus.is_empty(), "no focus expected");
                }
                other => panic!("expected Formula::And, got {other:?}"),
            },
            CompileOutcome::Skipped { .. } => panic!("expected Compiled, got Skipped"),
        }
    }

    #[test]
    fn test_compile_rule_pattern_either() {
        let branch_a = PatternTerm {
            pattern: Some("$X.unwrap()".to_string()),
            ..empty_term()
        };
        let branch_b = PatternTerm {
            pattern: Some("$X.expect($MSG)".to_string()),
            ..empty_term()
        };
        let schema = RuleSchema {
            pattern_either: Some(vec![branch_a, branch_b]),
            ..make_rule_schema("either-rule", vec!["rust"])
        };
        let result = compile_rule(&schema).expect("compile_rule must succeed");
        match result {
            CompileOutcome::Compiled(ir) => {
                assert!(
                    matches!(ir.formula, Formula::Or(_)),
                    "expected Formula::Or for pattern-either"
                );
                if let Formula::Or(branches) = ir.formula {
                    assert_eq!(branches.len(), 2);
                }
            }
            CompileOutcome::Skipped { .. } => panic!("expected Compiled, got Skipped"),
        }
    }

    #[test]
    fn test_compile_rule_no_formula_root_returns_error() {
        // make_rule_schema leaves all formula fields as None
        let schema = make_rule_schema("no-root-rule", vec!["rust"]);
        let result = compile_rule(&schema);
        assert!(
            matches!(result, Err(RuleParseError::Schema { .. })),
            "expected Schema error when no formula root is present"
        );
    }

    #[test]
    fn test_compile_rule_multiple_formula_roots_returns_error() {
        let schema = RuleSchema {
            pattern: Some("$X".to_string()),
            pattern_regex: Some(".*".to_string()),
            ..make_rule_schema("multi-root-rule", vec!["rust"])
        };
        let result = compile_rule(&schema);
        assert!(
            matches!(result, Err(RuleParseError::Schema { .. })),
            "expected Schema error when multiple formula roots are present"
        );
    }

    #[test]
    fn test_compile_rule_invalid_id_returns_error() {
        let schema = RuleSchema {
            pattern: Some("$X".to_string()),
            ..make_rule_schema("invalid rule id", vec!["rust"])
        };
        let result = compile_rule(&schema);
        assert!(
            matches!(result, Err(RuleParseError::InvalidId { .. })),
            "expected InvalidId error for id containing spaces"
        );
    }

    #[test]
    fn test_compile_rule_empty_id_returns_error() {
        let schema = RuleSchema {
            pattern: Some("$X".to_string()),
            ..make_rule_schema("", vec!["rust"])
        };
        let result = compile_rule(&schema);
        assert!(
            matches!(result, Err(RuleParseError::InvalidId { .. })),
            "expected InvalidId error for empty id"
        );
    }

    #[test]
    fn test_compile_rule_taint_mode_skipped() {
        let schema = RuleSchema {
            mode: Some("taint".to_string()),
            pattern: Some("$X".to_string()),
            ..make_rule_schema("taint-rule", vec!["rust"])
        };
        let result = compile_rule(&schema).expect("compile_rule must return Ok for skipped rules");
        assert!(
            matches!(result, CompileOutcome::Skipped { .. }),
            "taint mode must produce a Skipped outcome"
        );
        if let CompileOutcome::Skipped { reasons, .. } = result {
            assert!(
                reasons.iter().any(|r| matches!(r, SkipReason::TaintMode)),
                "TaintMode must appear in skip reasons"
            );
        }
    }

    #[test]
    fn test_compile_rule_empty_patterns_list_returns_error() {
        let schema = RuleSchema {
            patterns: Some(vec![]),
            ..make_rule_schema("empty-patterns-rule", vec!["rust"])
        };
        let result = compile_rule(&schema);
        assert!(
            matches!(result, Err(RuleParseError::Invariant { .. })),
            "expected Invariant error for empty patterns list"
        );
    }

    #[test]
    fn test_compile_rule_patterns_no_positive_term_returns_error() {
        // A patterns list containing only a pattern-not and no positive term
        let negative_only = PatternTerm {
            pattern_not: Some("fn test_$F() {}".to_string()),
            ..empty_term()
        };
        let schema = RuleSchema {
            patterns: Some(vec![negative_only]),
            ..make_rule_schema("no-positive-rule", vec!["rust"])
        };
        let result = compile_rule(&schema);
        assert!(
            matches!(result, Err(RuleParseError::Invariant { .. })),
            "expected Invariant error when patterns contains no positive term"
        );
    }

    #[test]
    fn test_compile_rule_focus_metavariable_collected() {
        let term = PatternTerm {
            pattern: Some("$X + $Y".to_string()),
            focus_metavariable: Some(StringOrVec(vec!["$X".to_string()])),
            ..empty_term()
        };
        let schema = RuleSchema {
            patterns: Some(vec![term]),
            ..make_rule_schema("focus-rule", vec!["rust"])
        };
        let result = compile_rule(&schema).expect("compile_rule must succeed");
        match result {
            CompileOutcome::Compiled(ir) => match ir.formula {
                Formula::And { focus, .. } => {
                    assert_eq!(
                        focus,
                        vec!["$X".to_string()],
                        "focus metavar must be collected"
                    );
                }
                other => panic!("expected Formula::And, got {other:?}"),
            },
            CompileOutcome::Skipped { .. } => panic!("expected Compiled, got Skipped"),
        }
    }

    #[test]
    fn test_compile_rule_conditions_collected() {
        let term = PatternTerm {
            pattern: Some("$X".to_string()),
            metavariable_regex: Some(MetavarRegexCondition {
                metavariable: "$X".to_string(),
                regex: ".*secret.*".to_string(),
                not: false,
            }),
            ..empty_term()
        };
        let schema = RuleSchema {
            patterns: Some(vec![term]),
            ..make_rule_schema("condition-rule", vec!["rust"])
        };
        let result = compile_rule(&schema).expect("compile_rule must succeed");
        match result {
            CompileOutcome::Compiled(ir) => match ir.formula {
                Formula::And {
                    conjuncts,
                    conditions,
                    ..
                } => {
                    assert_eq!(conjuncts.len(), 1, "one positive conjunct expected");
                    assert_eq!(conditions.len(), 1, "one metavar-regex condition expected");
                }
                other => panic!("expected Formula::And, got {other:?}"),
            },
            CompileOutcome::Skipped { .. } => panic!("expected Compiled, got Skipped"),
        }
    }

    #[test]
    fn test_parse_and_compile_valid_yaml() {
        let yaml = r#"
rules:
  - id: test-rule
    message: "test message"
    languages:
      - rust
    severity: WARNING
    pattern: "$X"
"#;
        let result = parse_and_compile(yaml).expect("valid YAML must parse successfully");
        assert_eq!(result.len(), 1, "one rule expected");
        assert!(
            result[0].is_ok(),
            "the single rule must compile without error"
        );
        match result[0].as_ref().unwrap() {
            CompileOutcome::Compiled(ir) => {
                assert_eq!(ir.id, "test-rule");
            }
            CompileOutcome::Skipped { .. } => panic!("expected Compiled, got Skipped"),
        }
    }

    #[test]
    fn test_parse_and_compile_invalid_yaml() {
        let yaml = ": {{this is not valid yaml";
        let result = parse_and_compile(yaml);
        assert!(
            matches!(result, Err(RuleParseError::Yaml(_))),
            "malformed YAML must return RuleParseError::Yaml"
        );
    }
}
