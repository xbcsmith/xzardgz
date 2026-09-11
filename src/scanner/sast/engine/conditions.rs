//! Metavariable condition evaluation for SAST rules.
//!
//! This module implements the three metavariable condition types defined in
//! [`Condition`]:
//!
//! - [`eval_metavar_regex`]: checks whether a bound metavariable matches a regex
//! - [`eval_metavar_pattern`]: checks whether a bound metavariable matches an
//!   AST pattern (re-parsing the bound text)
//! - [`eval_metavar_comparison`]: evaluates a numeric comparison expression
//!
//! The public entry point is [`apply_conditions`], which applies a sequence of
//! conditions to a set of [`RangeWithMetavars`] values and returns only those
//! ranges that satisfy all conditions.  [`apply_focus`] then narrows each
//! surviving range to the byte extent of one or more named focus metavariables.
//!
//! # Error handling policy
//!
//! | Error variant | Effect |
//! |---|---|
//! | `UnsupportedComparison` | Bubble up; abort rule evaluation |
//! | `RecursionLimitExceeded` | Bubble up; abort rule evaluation |
//! | `RegexCompile` | Bubble up; abort rule evaluation |
//! | `UnboundMetavar` | Treat as `false`; drop range, continue |
//! | `TypeMismatch` | Treat as `false`; drop range, continue |

use ast_grep_core::MatchStrictness;
use ast_grep_core::tree_sitter::LanguageExt;
use ast_grep_language::SupportLang;
use regex::RegexBuilder;
use std::str::FromStr;
use thiserror::Error;

use crate::scanner::sast::engine::compare::{CompareError, CompareOptions, eval_comparison};
use crate::scanner::sast::engine::pattern::PatternCompiler;
use crate::scanner::sast::engine::range::{MetavarBindings, RangeWithMetavars};
use crate::scanner::sast::rule::ir::{Condition, MetavarId};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum nesting depth allowed for recursive `metavariable-pattern` evaluation.
///
/// If [`eval_metavar_pattern`] is called with `depth >= MAX_RECURSION_DEPTH`,
/// it returns `Err(ConditionError::RecursionLimitExceeded)` immediately,
/// preventing unbounded recursion.
const MAX_RECURSION_DEPTH: usize = 10;

// ---------------------------------------------------------------------------
// ConditionError
// ---------------------------------------------------------------------------

/// Error returned when evaluating a metavariable condition.
///
/// Variants are divided into two categories based on their effect on the
/// containing rule evaluation:
///
/// **Rule-level errors** — bubble up and abort the entire rule:
/// - [`ConditionError::UnsupportedComparison`]
/// - [`ConditionError::RecursionLimitExceeded`]
/// - [`ConditionError::RegexCompile`]
///
/// **Range-level errors** — filter out the current range, continue others:
/// - [`ConditionError::UnboundMetavar`]
/// - [`ConditionError::TypeMismatch`]
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConditionError {
    /// The comparison expression uses constructs outside the closed grammar.
    ///
    /// Causes the entire rule evaluation to be skipped (return empty results).
    #[error("unsupported comparison expression: {0}")]
    UnsupportedComparison(String),

    /// Recursion depth limit exceeded in metavariable-pattern evaluation.
    ///
    /// Triggered when the nesting depth reaches [`MAX_RECURSION_DEPTH`] (10).
    #[error("recursion depth limit exceeded in metavariable-pattern evaluation")]
    RecursionLimitExceeded,

    /// Regex compilation failed (e.g. invalid syntax or automaton exceeds size limit).
    #[error("regex compilation failed: {0}")]
    RegexCompile(String),

    /// The referenced metavariable had no binding for this range.
    ///
    /// The condition evaluates to `false` for this range; the range is filtered
    /// out but processing of other ranges continues.
    #[error("metavariable '{0}' is not bound in this match")]
    UnboundMetavar(String),

    /// The bound text could not be parsed as a number for comparison.
    ///
    /// The condition evaluates to `false` for this range; the range is filtered
    /// out but processing of other ranges continues.
    #[error("type mismatch in comparison: {0}")]
    TypeMismatch(String),
}

// ---------------------------------------------------------------------------
// eval_metavar_regex
// ---------------------------------------------------------------------------

/// Evaluate a `metavariable-regex` condition against a set of bindings.
///
/// The condition is satisfied when the bound text of `metavar` fully matches
/// the anchored regex `^(?:regex_str)$` (when `not` is false), or when it
/// does NOT match (when `not` is true).
///
/// Both NFA size and DFA size limits are set to 10 MiB to bound ReDoS risk,
/// matching the limits used by `RegexModeScanner`.
///
/// # Arguments
///
/// * `metavar` - The metavariable name whose bound text is tested (e.g. `"$X"`).
/// * `regex_str` - The regular expression (unanchored; anchors are added internally).
/// * `not` - When `true`, the result is inverted: the bound text must NOT match.
/// * `bindings` - Metavariable bindings from the current pattern match.
///
/// # Returns
///
/// `Ok(true)` if the condition is satisfied; `Ok(false)` otherwise.
///
/// # Errors
///
/// - [`ConditionError::RegexCompile`] if `regex_str` is syntactically invalid
///   or the compiled automaton exceeds the 10 MiB size limit.
/// - [`ConditionError::UnboundMetavar`] if `metavar` is absent from `bindings`.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use xzardgz::scanner::sast::engine::conditions::eval_metavar_regex;
/// use xzardgz::scanner::sast::engine::range::MetavarValue;
///
/// let mut bindings = BTreeMap::new();
/// bindings.insert("$X".to_string(), MetavarValue { text: "foo".to_string(), start: 0, end: 3 });
///
/// assert_eq!(eval_metavar_regex("$X", "foo", false, &bindings).unwrap(), true);
/// assert_eq!(eval_metavar_regex("$X", "foo", true, &bindings).unwrap(), false);
/// assert_eq!(eval_metavar_regex("$X", "fo+", false, &bindings).unwrap(), true);
/// ```
pub fn eval_metavar_regex(
    metavar: &str,
    regex_str: &str,
    not: bool,
    bindings: &MetavarBindings,
) -> Result<bool, ConditionError> {
    let anchored = format!("^(?:{regex_str})$");
    let re = RegexBuilder::new(&anchored)
        .size_limit(10_485_760)
        .dfa_size_limit(10_485_760)
        .build()
        .map_err(|e| ConditionError::RegexCompile(e.to_string()))?;
    let binding = bindings
        .get(metavar)
        .ok_or_else(|| ConditionError::UnboundMetavar(metavar.to_string()))?;
    let matched = re.is_match(&binding.text);
    Ok(matched ^ not)
}

// ---------------------------------------------------------------------------
// eval_metavar_pattern
// ---------------------------------------------------------------------------

/// Evaluate a `metavariable-pattern` condition against a set of bindings.
///
/// Re-parses the bound text of `metavar` as a source file in the given
/// `language` (defaulting to Rust) and checks whether `pattern_str` matches
/// anywhere in that re-parsed tree.
///
/// The `depth` parameter guards against unbounded recursion when conditions
/// are nested.  When `depth` reaches [`MAX_RECURSION_DEPTH`] (10) this
/// function returns `Err(ConditionError::RecursionLimitExceeded)` immediately.
///
/// # Arguments
///
/// * `metavar` - The metavariable name whose bound code is re-parsed (e.g. `"$X"`).
/// * `pattern_str` - The structural pattern the re-parsed code must match.
/// * `language` - Optional language override for re-parsing (defaults to `"rust"`).
/// * `bindings` - Metavariable bindings from the current pattern match.
/// * `compiler` - Shared compiled-pattern cache.
/// * `depth` - Current recursion depth; pass `0` for top-level calls.
///
/// # Returns
///
/// `Ok(true)` if the bound code contains at least one match for `pattern_str`;
/// `Ok(false)` otherwise.
///
/// # Errors
///
/// - [`ConditionError::RecursionLimitExceeded`] if `depth >= MAX_RECURSION_DEPTH`.
/// - [`ConditionError::UnboundMetavar`] if `metavar` is absent from `bindings`.
/// - [`ConditionError::RegexCompile`] if `pattern_str` fails to compile.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use xzardgz::scanner::sast::engine::conditions::eval_metavar_pattern;
/// use xzardgz::scanner::sast::engine::pattern::PatternCompiler;
/// use xzardgz::scanner::sast::engine::range::MetavarValue;
///
/// let compiler = PatternCompiler::new();
/// let mut bindings = BTreeMap::new();
/// bindings.insert(
///     "$F".to_string(),
///     MetavarValue { text: "fn foo() {}".to_string(), start: 0, end: 11 },
/// );
/// let result = eval_metavar_pattern("$F", "fn $NAME() {}", None, &bindings, &compiler, 0);
/// assert_eq!(result.unwrap(), true);
/// ```
pub fn eval_metavar_pattern(
    metavar: &str,
    pattern_str: &str,
    language: Option<&str>,
    bindings: &MetavarBindings,
    compiler: &PatternCompiler,
    depth: usize,
) -> Result<bool, ConditionError> {
    if depth >= MAX_RECURSION_DEPTH {
        return Err(ConditionError::RecursionLimitExceeded);
    }
    let binding = bindings
        .get(metavar)
        .ok_or_else(|| ConditionError::UnboundMetavar(metavar.to_string()))?;
    let lang = language
        .and_then(|s| SupportLang::from_str(s).ok())
        .unwrap_or(SupportLang::Rust);
    let root = lang.ast_grep(&binding.text);
    let compiled = compiler
        .compile(pattern_str, lang, MatchStrictness::Relaxed)
        .map_err(|e| ConditionError::RegexCompile(e.to_string()))?;
    Ok(root.root().find_all(&*compiled).next().is_some())
}

// ---------------------------------------------------------------------------
// eval_metavar_comparison
// ---------------------------------------------------------------------------

/// Evaluate a `metavariable-comparison` condition against a set of bindings.
///
/// Delegates to [`eval_comparison`] from the comparison expression evaluator.
/// The `_metavar` argument is carried for API consistency with
/// [`Condition::MetavarComparison`]; the `comparison` expression string
/// directly references metavariable names (e.g. `"$BITS < 2048"`).
///
/// # Arguments
///
/// * `_metavar` - The primary metavariable (API consistency; not used internally).
/// * `comparison` - The comparison expression (e.g. `"$BITS < 2048"`).
/// * `strip` - When `true`, trailing non-numeric characters are stripped from
///   bound text before numeric parsing (e.g. `"1024k"` becomes `"1024"`).
/// * `base` - Optional numeric base (e.g. `16` for hex). `None` = decimal.
/// * `bindings` - Metavariable bindings from the current pattern match.
///
/// # Returns
///
/// `Ok(true)` if the comparison evaluates to `true`; `Ok(false)` otherwise.
///
/// # Errors
///
/// - [`ConditionError::UnsupportedComparison`] if `comparison` uses constructs
///   outside the supported grammar (e.g. `+`, `/`, parentheses).
/// - [`ConditionError::UnboundMetavar`] if a metavariable referenced in the
///   expression is absent from `bindings`.
/// - [`ConditionError::TypeMismatch`] if bound text cannot be parsed as a number.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use xzardgz::scanner::sast::engine::conditions::eval_metavar_comparison;
/// use xzardgz::scanner::sast::engine::range::MetavarValue;
///
/// let mut bindings = BTreeMap::new();
/// bindings.insert(
///     "$BITS".to_string(),
///     MetavarValue { text: "1024".to_string(), start: 0, end: 4 },
/// );
/// assert_eq!(
///     eval_metavar_comparison("$BITS", "$BITS < 2048", false, None, &bindings).unwrap(),
///     true,
/// );
/// ```
pub fn eval_metavar_comparison(
    _metavar: &str,
    comparison: &str,
    strip: bool,
    base: Option<u32>,
    bindings: &MetavarBindings,
) -> Result<bool, ConditionError> {
    let options = CompareOptions { strip, base };
    eval_comparison(comparison, bindings, &options).map_err(|e| match e {
        CompareError::UnsupportedConstruct { detail } => {
            ConditionError::UnsupportedComparison(detail)
        }
        CompareError::UnboundMetavar(m) => ConditionError::UnboundMetavar(m),
        CompareError::TypeMismatch(m) => ConditionError::TypeMismatch(m),
    })
}

// ---------------------------------------------------------------------------
// apply_conditions
// ---------------------------------------------------------------------------

/// Apply all conditions in order to a set of ranges, returning only passing ranges.
///
/// For each range, every condition is evaluated in sequence.  A range is kept
/// only if ALL conditions pass.  The error handling policy is:
///
/// - [`ConditionError::UnsupportedComparison`], [`ConditionError::RecursionLimitExceeded`],
///   and [`ConditionError::RegexCompile`]: bubble up immediately, aborting the
///   entire rule evaluation.
/// - [`ConditionError::UnboundMetavar`] and [`ConditionError::TypeMismatch`]:
///   treated as `false` for the current range (range is filtered out) and
///   processing continues with the next range.
///
/// # Arguments
///
/// * `ranges` - The candidate ranges to filter.
/// * `conditions` - Ordered list of conditions; evaluated left-to-right with
///   short-circuit on first `false`.
/// * `compiler` - Shared compiled-pattern cache (used by `metavariable-pattern`).
/// * `depth` - Current recursion depth forwarded to [`eval_metavar_pattern`].
///
/// # Returns
///
/// The subset of `ranges` for which all conditions pass, in original order.
///
/// # Errors
///
/// Returns `Err` when any condition produces a rule-level error:
/// `UnsupportedComparison`, `RecursionLimitExceeded`, or `RegexCompile`.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use xzardgz::scanner::sast::engine::conditions::apply_conditions;
/// use xzardgz::scanner::sast::engine::pattern::PatternCompiler;
/// use xzardgz::scanner::sast::engine::range::{MetavarValue, RangeWithMetavars};
/// use xzardgz::scanner::sast::rule::ir::Condition;
///
/// let compiler = PatternCompiler::new();
/// let mut bindings = BTreeMap::new();
/// bindings.insert(
///     "$X".to_string(),
///     MetavarValue { text: "foo".to_string(), start: 0, end: 3 },
/// );
/// let ranges = vec![RangeWithMetavars::new(0, 10, bindings)];
/// let conditions = vec![Condition::MetavarRegex {
///     metavar: "$X".to_string(),
///     regex: "foo".to_string(),
///     not: false,
/// }];
/// let result = apply_conditions(ranges, &conditions, &compiler, 0).unwrap();
/// assert_eq!(result.len(), 1);
/// ```
pub fn apply_conditions(
    ranges: Vec<RangeWithMetavars>,
    conditions: &[Condition],
    compiler: &PatternCompiler,
    depth: usize,
) -> Result<Vec<RangeWithMetavars>, ConditionError> {
    let mut passing = Vec::with_capacity(ranges.len());
    for range in ranges {
        let mut keep = true;
        'conditions: for condition in conditions {
            let result = match condition {
                Condition::MetavarRegex {
                    metavar,
                    regex,
                    not,
                } => eval_metavar_regex(metavar, regex, *not, &range.bindings),
                Condition::MetavarPattern {
                    metavar,
                    pattern,
                    language,
                } => eval_metavar_pattern(
                    metavar,
                    pattern,
                    language.as_deref(),
                    &range.bindings,
                    compiler,
                    depth + 1,
                ),
                Condition::MetavarComparison {
                    metavar,
                    comparison,
                    strip,
                    base,
                } => eval_metavar_comparison(metavar, comparison, *strip, *base, &range.bindings),
            };
            match result {
                Ok(true) => {}
                Ok(false) => {
                    keep = false;
                    break 'conditions;
                }
                Err(ConditionError::UnboundMetavar(_)) | Err(ConditionError::TypeMismatch(_)) => {
                    keep = false;
                    break 'conditions;
                }
                Err(e) => return Err(e),
            }
        }
        if keep {
            passing.push(range);
        }
    }
    Ok(passing)
}

// ---------------------------------------------------------------------------
// apply_focus
// ---------------------------------------------------------------------------

/// Narrow each range to the byte range of the focused metavariable(s).
///
/// For each range, every variable in `focus_vars` is looked up in the
/// bindings.  If any variable is not bound, the range is dropped.  If
/// multiple focus variables are specified, their byte ranges are intersected:
/// the focused range is `[max(starts), min(ends))`.  A non-positive
/// intersection (start > end) also drops the range.
///
/// Ranges are sorted by `(start, end)` in the returned `Vec`.
///
/// If `focus_vars` is empty, the original `ranges` vector is returned unchanged.
///
/// # Arguments
///
/// * `ranges` - The ranges to narrow.
/// * `focus_vars` - Metavariable names to focus on (e.g. `["$X", "$Y"]`).
///
/// # Returns
///
/// Sorted `Vec<RangeWithMetavars>` with each range narrowed to the intersection
/// of the specified focus metavariables' byte extents.  Ranges whose focus
/// bindings are absent or whose intersection is empty are dropped.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use xzardgz::scanner::sast::engine::conditions::apply_focus;
/// use xzardgz::scanner::sast::engine::range::{MetavarValue, RangeWithMetavars};
///
/// let mut bindings = BTreeMap::new();
/// bindings.insert("$X".to_string(), MetavarValue { text: "foo".to_string(), start: 5, end: 8 });
/// let ranges = vec![RangeWithMetavars::new(0, 20, bindings)];
/// let result = apply_focus(ranges, &["$X".to_string()]);
/// assert_eq!(result.len(), 1);
/// assert_eq!(result[0].start, 5);
/// assert_eq!(result[0].end, 8);
/// ```
pub fn apply_focus(
    ranges: Vec<RangeWithMetavars>,
    focus_vars: &[MetavarId],
) -> Vec<RangeWithMetavars> {
    if focus_vars.is_empty() {
        return ranges;
    }
    let mut result: Vec<RangeWithMetavars> = Vec::with_capacity(ranges.len());
    for range in ranges {
        let mut focused_start: usize = 0;
        let mut focused_end: usize = usize::MAX;
        let mut all_bound = true;
        for var in focus_vars {
            match range.bindings.get(var) {
                Some(binding) => {
                    focused_start = focused_start.max(binding.start);
                    focused_end = focused_end.min(binding.end);
                }
                None => {
                    all_bound = false;
                    break;
                }
            }
        }
        if all_bound && focused_start <= focused_end {
            result.push(RangeWithMetavars::new(
                focused_start,
                focused_end,
                range.bindings,
            ));
        }
    }
    result.sort();
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::MAX_RECURSION_DEPTH;
    use super::*;

    use crate::scanner::sast::engine::pattern::PatternCompiler;
    use crate::scanner::sast::engine::range::{MetavarValue, RangeWithMetavars};
    use crate::scanner::sast::rule::ir::Condition;

    // -----------------------------------------------------------------------
    // Test helper
    // -----------------------------------------------------------------------

    fn make_bindings(pairs: &[(&str, &str, usize, usize)]) -> MetavarBindings {
        pairs
            .iter()
            .map(|(k, v, s, e)| {
                (
                    k.to_string(),
                    MetavarValue {
                        text: v.to_string(),
                        start: *s,
                        end: *e,
                    },
                )
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // eval_metavar_regex
    // -----------------------------------------------------------------------

    #[test]
    fn test_eval_metavar_regex_anchored_match_succeeds() {
        let bindings = make_bindings(&[("$X", "foo", 0, 3)]);
        // "^(?:foo)$" must fully match the bound text "foo".
        assert!(
            eval_metavar_regex("$X", "foo", false, &bindings).unwrap(),
            "exact match must succeed"
        );
    }

    #[test]
    fn test_eval_metavar_regex_unanchored_substring_does_not_match() {
        let bindings = make_bindings(&[("$X", "foobar", 0, 6)]);
        // The anchored form "^(?:foo)$" does not match "foobar" because the
        // string has trailing characters that prevent a full match.
        assert!(
            !eval_metavar_regex("$X", "foo", false, &bindings).unwrap(),
            "partial substring must not match because pattern is anchored"
        );
    }

    #[test]
    fn test_eval_metavar_regex_not_flag_inverts_result() {
        let bindings = make_bindings(&[("$X", "foo", 0, 3)]);
        // "foo" would match "^(?:foo)$", but not=true inverts the result.
        assert!(
            !eval_metavar_regex("$X", "foo", true, &bindings).unwrap(),
            "not=true must invert a positive match to false"
        );
    }

    #[test]
    fn test_eval_metavar_regex_invalid_regex_returns_error() {
        let bindings = make_bindings(&[("$X", "foo", 0, 3)]);
        // "[" is an unclosed character class — invalid regex syntax.
        let result = eval_metavar_regex("$X", "[", false, &bindings);
        assert!(
            matches!(result, Err(ConditionError::RegexCompile(_))),
            "invalid regex syntax must return RegexCompile error, got {result:?}"
        );
    }

    #[test]
    fn test_eval_metavar_regex_oversized_regex_returns_error() {
        let bindings = make_bindings(&[("$X", "test0", 0, 5)]);
        // Build a very large alternation to exceed the 10 MiB NFA size limit.
        let large_regex = (0..100_000)
            .map(|i| format!("(?:test{i})"))
            .collect::<Vec<_>>()
            .join("|");
        let result = eval_metavar_regex("$X", &large_regex, false, &bindings);
        assert!(
            matches!(result, Err(ConditionError::RegexCompile(_))),
            "oversized regex must return RegexCompile error due to size_limit, got {result:?}"
        );
    }

    #[test]
    fn test_eval_metavar_regex_unbound_metavar_returns_error() {
        let bindings = make_bindings(&[]);
        let result = eval_metavar_regex("$MISSING", "foo", false, &bindings);
        assert!(
            matches!(result, Err(ConditionError::UnboundMetavar(ref m)) if m == "$MISSING"),
            "missing binding must return UnboundMetavar error, got {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // eval_metavar_pattern
    // -----------------------------------------------------------------------

    #[test]
    fn test_eval_metavar_pattern_nested_match_succeeds() {
        let compiler = PatternCompiler::new();
        // Bind $F to a function declaration; the pattern "fn $NAME() {}" must match.
        let bindings = make_bindings(&[("$F", "fn foo() {}", 0, 11)]);
        let result = eval_metavar_pattern("$F", "fn $NAME() {}", None, &bindings, &compiler, 0);
        assert!(
            result.unwrap(),
            "pattern must match re-parsed bound function text"
        );
    }

    #[test]
    fn test_eval_metavar_pattern_language_switch_works() {
        let compiler = PatternCompiler::new();
        let bindings = make_bindings(&[("$F", "fn foo() {}", 0, 11)]);
        // Explicit language: Some("rust") must behave identically to None.
        let result =
            eval_metavar_pattern("$F", "fn $NAME() {}", Some("rust"), &bindings, &compiler, 0);
        assert!(
            result.unwrap(),
            "explicit rust language switch must produce the same result as the default"
        );
    }

    #[test]
    fn test_eval_metavar_pattern_depth_limit_exceeded_returns_error() {
        let compiler = PatternCompiler::new();
        let bindings = make_bindings(&[("$F", "fn foo() {}", 0, 11)]);
        // depth == MAX_RECURSION_DEPTH must trigger the recursion guard.
        let result = eval_metavar_pattern(
            "$F",
            "fn $NAME() {}",
            None,
            &bindings,
            &compiler,
            MAX_RECURSION_DEPTH,
        );
        assert!(
            matches!(result, Err(ConditionError::RecursionLimitExceeded)),
            "depth == MAX_RECURSION_DEPTH must return RecursionLimitExceeded, got {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // eval_metavar_comparison
    // -----------------------------------------------------------------------

    #[test]
    fn test_eval_metavar_comparison_less_than_true() {
        let bindings = make_bindings(&[("$BITS", "1024", 0, 4)]);
        let result = eval_metavar_comparison("$BITS", "$BITS < 2048", false, None, &bindings);
        assert!(result.unwrap(), "$BITS=1024 must satisfy $BITS < 2048");
    }

    #[test]
    fn test_eval_metavar_comparison_unsupported_returns_error() {
        let bindings = make_bindings(&[("$BITS", "1024", 0, 4)]);
        // The "+" operator is outside the supported grammar.
        let result = eval_metavar_comparison("$BITS", "$BITS + 1 < 2048", false, None, &bindings);
        assert!(
            matches!(result, Err(ConditionError::UnsupportedComparison(_))),
            "unsupported operator must produce UnsupportedComparison, got {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // apply_conditions
    // -----------------------------------------------------------------------

    #[test]
    fn test_apply_conditions_all_pass_returns_all_ranges() {
        let compiler = PatternCompiler::new();
        let bindings = make_bindings(&[("$X", "foo", 0, 3)]);
        let ranges = vec![RangeWithMetavars::new(0, 10, bindings)];
        let conditions = vec![Condition::MetavarRegex {
            metavar: "$X".to_string(),
            regex: "foo".to_string(),
            not: false,
        }];
        let result = apply_conditions(ranges, &conditions, &compiler, 0).unwrap();
        assert_eq!(
            result.len(),
            1,
            "all-passing conditions must keep all ranges"
        );
    }

    #[test]
    fn test_apply_conditions_one_fail_removes_range() {
        let compiler = PatternCompiler::new();
        // Binding has "bar" but condition expects "foo".
        let bindings = make_bindings(&[("$X", "bar", 0, 3)]);
        let ranges = vec![RangeWithMetavars::new(0, 10, bindings)];
        let conditions = vec![Condition::MetavarRegex {
            metavar: "$X".to_string(),
            regex: "foo".to_string(),
            not: false,
        }];
        let result = apply_conditions(ranges, &conditions, &compiler, 0).unwrap();
        assert!(result.is_empty(), "failing condition must remove the range");
    }

    #[test]
    fn test_apply_conditions_unbound_metavar_removes_range() {
        let compiler = PatternCompiler::new();
        let bindings = make_bindings(&[("$X", "foo", 0, 3)]);
        let ranges = vec![RangeWithMetavars::new(0, 10, bindings)];
        // Condition references $UNBOUND which has no binding.
        let conditions = vec![Condition::MetavarRegex {
            metavar: "$UNBOUND".to_string(),
            regex: "foo".to_string(),
            not: false,
        }];
        let result = apply_conditions(ranges, &conditions, &compiler, 0).unwrap();
        assert!(
            result.is_empty(),
            "UnboundMetavar must remove the range but not abort rule evaluation"
        );
    }

    #[test]
    fn test_apply_conditions_unsupported_comparison_bubbles_up() {
        let compiler = PatternCompiler::new();
        let bindings = make_bindings(&[("$BITS", "1024", 0, 4)]);
        let ranges = vec![RangeWithMetavars::new(0, 10, bindings)];
        // "$BITS + 1 < 2048" uses the "+" operator which is unsupported.
        let conditions = vec![Condition::MetavarComparison {
            metavar: "$BITS".to_string(),
            comparison: "$BITS + 1 < 2048".to_string(),
            strip: false,
            base: None,
        }];
        let result = apply_conditions(ranges, &conditions, &compiler, 0);
        assert!(
            matches!(result, Err(ConditionError::UnsupportedComparison(_))),
            "UnsupportedComparison must bubble up from apply_conditions, got {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // apply_focus
    // -----------------------------------------------------------------------

    #[test]
    fn test_apply_focus_single_variable_narrows_range() {
        // Range covers [5, 20); $BITS is bound at [10, 14).
        let bindings = make_bindings(&[("$BITS", "1024", 10, 14)]);
        let ranges = vec![RangeWithMetavars::new(5, 20, bindings)];
        let result = apply_focus(ranges, &["$BITS".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].start, 10, "start must be narrowed to $BITS.start");
        assert_eq!(result[0].end, 14, "end must be narrowed to $BITS.end");
    }

    #[test]
    fn test_apply_focus_multiple_intersecting_focuses() {
        // $A covers [5, 15), $B covers [10, 20): intersection is [10, 15).
        let bindings = make_bindings(&[("$A", "text_a", 5, 15), ("$B", "text_b", 10, 20)]);
        let ranges = vec![RangeWithMetavars::new(0, 30, bindings)];
        let result = apply_focus(ranges, &["$A".to_string(), "$B".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].start, 10, "start must be max(5, 10) = 10");
        assert_eq!(result[0].end, 15, "end must be min(15, 20) = 15");
    }

    #[test]
    fn test_apply_focus_non_bound_focus_variable_drops_range() {
        let bindings = make_bindings(&[("$FOO", "foo", 0, 3)]);
        let ranges = vec![RangeWithMetavars::new(0, 10, bindings)];
        // $UNBOUND is not in bindings; the range must be dropped.
        let result = apply_focus(ranges, &["$UNBOUND".to_string()]);
        assert!(
            result.is_empty(),
            "missing focus binding must cause the range to be dropped"
        );
    }

    #[test]
    fn test_apply_focus_empty_focus_vars_returns_unchanged() {
        let bindings = make_bindings(&[("$FOO", "foo", 0, 3)]);
        let range = RangeWithMetavars::new(0, 10, bindings);
        let original_start = range.start;
        let original_end = range.end;
        let result = apply_focus(vec![range], &[]);
        assert_eq!(
            result.len(),
            1,
            "empty focus_vars must return ranges unchanged"
        );
        assert_eq!(result[0].start, original_start);
        assert_eq!(result[0].end, original_end);
    }

    #[test]
    fn test_apply_focus_non_overlapping_focus_vars_drops_range() {
        // $A covers [5, 10), $B covers [15, 20): no overlap so range is dropped.
        let bindings = make_bindings(&[("$A", "text_a", 5, 10), ("$B", "text_b", 15, 20)]);
        let ranges = vec![RangeWithMetavars::new(0, 30, bindings)];
        let result = apply_focus(ranges, &["$A".to_string(), "$B".to_string()]);
        assert!(
            result.is_empty(),
            "non-overlapping focus variables must cause the range to be dropped"
        );
    }
}
