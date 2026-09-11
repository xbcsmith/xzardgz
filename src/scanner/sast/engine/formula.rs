//! Formula evaluator for the AST-mode SAST engine (Phase 2 + Phase 3).
//!
//! This module provides [`eval_formula`] and [`scan_rule`], the primary entry
//! points for evaluating a compiled [`Formula`] tree against a parsed source
//! file.
//!
//! # Formula semantics
//!
//! | Variant | Behaviour |
//! |---------|-----------|
//! | `Leaf(Pattern(s))` | AST pattern match via [`PatternCompiler`] |
//! | `Leaf(Regex(_))`  | Returns empty set (regex mode is handled by [`RegexModeScanner`]) |
//! | `And { conjuncts, negations, conditions, focus }` | Intersection of conjuncts minus negations, then conditions filter, then focus narrows |
//! | `Or(children)` | Union of all children |
//! | `Inside(inner)` | Containment filter when used inside `And`; standalone evaluates inner |
//!
//! # Evaluation order for `And`
//!
//! 1. Intersect regular conjuncts.
//! 2. Apply `Inside` containment filters.
//! 3. Subtract negations (`pattern-not*`).
//! 4. Apply metavariable conditions (`metavariable-regex`, `metavariable-pattern`,
//!    `metavariable-comparison`).
//! 5. Apply `focus-metavariable` narrowing.
//!
//! If any condition returns an unsupported-construct or recursion-limit error,
//! the entire rule evaluation returns an empty set (the rule is effectively skipped
//! for this file without aborting the scan).
//!
//! # Timeout and truncation
//!
//! A deadline is computed from [`SastEngineConfig::rule_timeout_ms`] at the
//! start of each top-level [`eval_formula`] call. The deadline is checked
//! before every recursive call and before every pattern-match iteration. When
//! the deadline is exceeded a [`TruncationReason::Timeout`] is returned
//! alongside any results collected so far.
//!
//! The total number of result ranges is capped at
//! [`SastEngineConfig::max_matches_per_file`]. When the cap is hit a
//! [`TruncationReason::MaxMatchesReached`] is returned.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ast_grep_core::meta_var::MetaVariable;
use ast_grep_core::{MatchStrictness, Pattern};
use ast_grep_language::SupportLang;

use crate::scanner::sast::ast::parse::CachedRoot;
use crate::scanner::sast::config::SastEngineConfig;
use crate::scanner::sast::engine::conditions::{ConditionError, apply_conditions, apply_focus};
use crate::scanner::sast::engine::pattern::PatternCompiler;
use crate::scanner::sast::engine::range::{
    MetavarBindings, MetavarValue, RangeWithMetavars, intersect, subtract, union,
};
use crate::scanner::sast::error::SastError;
use crate::scanner::sast::rule::ir::{Condition, Formula, Leaf, MetavarId, RuleIr};

// ---------------------------------------------------------------------------
// TruncationReason
// ---------------------------------------------------------------------------

/// Reason a match result set was truncated before all matches were found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TruncationReason {
    /// The per-file match limit was reached.
    MaxMatchesReached {
        /// The limit that was applied.
        limit: usize,
    },
    /// The per-rule timeout elapsed.
    Timeout {
        /// Milliseconds elapsed when the timeout was detected.
        elapsed_ms: u64,
    },
}

// ---------------------------------------------------------------------------
// Internal evaluation context
// ---------------------------------------------------------------------------

/// Internal context passed through recursive formula evaluation.
///
/// Carries all mutable bookkeeping state so that the recursive evaluator does
/// not need to return it alongside results.
struct EvalContext<'a> {
    /// Pattern compiler shared across all leaves in this evaluation.
    compiler: &'a PatternCompiler,
    /// Engine configuration for timeout and match-cap limits.
    config: &'a SastEngineConfig,
    /// Rule identifier used in error messages.
    rule_id: &'a str,
    /// Absolute time after which evaluation must stop.
    deadline: Instant,
    /// Wall-clock time at the start of this evaluation, for elapsed-ms reporting.
    start_time: Instant,
    /// Running total of result ranges accumulated so far across all leaves.
    matches_found: usize,
}

impl EvalContext<'_> {
    /// Returns `true` if the wall clock has passed the evaluation deadline.
    fn is_timed_out(&self) -> bool {
        Instant::now() > self.deadline
    }

    /// Returns `true` if the number of accumulated matches has reached the cap.
    fn is_match_cap_reached(&self) -> bool {
        self.matches_found >= self.config.max_matches_per_file
    }

    /// Returns milliseconds elapsed since the evaluation started.
    fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Evaluate a compiled formula against a parsed source file root.
///
/// Traverses the formula tree recursively, applying:
/// - `Leaf(Pattern)`: pattern matching via `PatternCompiler`
/// - `Leaf(Regex)`: not supported in AST mode; returns empty set
/// - `And { conjuncts, negations, .. }`: intersection of conjuncts minus negations
/// - `Or(children)`: union of all children
/// - `Inside(inner)`: containment filter (see semantics below)
///
/// Conditions (`metavariable-*`) and focus are NOT evaluated in Phase 2;
/// they are deferred to Phase 3. The raw metavar bindings from matching are
/// preserved in `RangeWithMetavars::bindings` for Phase 3 to consume.
///
/// # Inside semantics
///
/// When a conjunct in `And` is `Formula::Inside(inner)`, the `inner`
/// formula is evaluated to produce "container" ranges. Candidate ranges
/// from other conjuncts are then filtered to keep only those fully
/// contained within at least one container range. Bindings from the
/// container match are NOT merged (they are from a different node).
///
/// # Timeout and truncation
///
/// A `std::time::Instant` deadline is computed from
/// `config.rule_timeout_ms` at the start of each `eval_formula` call.
/// Before every recursive call and before every pattern-match iteration,
/// the current time is compared against the deadline. If exceeded, a
/// `TruncationReason::Timeout` is returned immediately.
///
/// The total number of result ranges produced is capped at
/// `config.max_matches_per_file`. When the cap is hit,
/// `TruncationReason::MaxMatchesReached` is returned alongside the
/// capped results.
///
/// # Arguments
///
/// * `formula` - The compiled formula to evaluate.
/// * `cached_root` - The parsed source file root (borrowed from cache).
/// * `compiler` - Pattern compiler for `Leaf::Pattern` nodes.
/// * `config` - Engine configuration (timeout, max matches).
/// * `rule_id` - Rule identifier for error reporting.
///
/// # Returns
///
/// A tuple of:
/// - `Vec<RangeWithMetavars>`: matched ranges (possibly capped)
/// - `Option<TruncationReason>`: `Some` if truncation occurred
///
/// # Errors
///
/// Returns [`SastError`] on pattern compilation failure.
///
/// # Examples
///
/// ```
/// use ast_grep_core::tree_sitter::LanguageExt;
/// use ast_grep_language::SupportLang;
/// use xzardgz::scanner::sast::ast::diagnostics::ErrorNodeDensity;
/// use xzardgz::scanner::sast::ast::parse::CachedRoot;
/// use xzardgz::scanner::sast::config::SastEngineConfig;
/// use xzardgz::scanner::sast::engine::formula::eval_formula;
/// use xzardgz::scanner::sast::engine::pattern::PatternCompiler;
/// use xzardgz::scanner::sast::rule::ir::{Formula, Leaf};
///
/// let root = SupportLang::Rust.ast_grep("fn foo() {}");
/// let density = ErrorNodeDensity::from_root(&root);
/// let cached = CachedRoot { root, density };
/// let compiler = PatternCompiler::new();
/// let config = SastEngineConfig::new();
/// let formula = Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string()));
/// let (matches, reason) = eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();
/// assert!(!matches.is_empty());
/// assert!(reason.is_none());
/// ```
pub fn eval_formula(
    formula: &Formula,
    cached_root: &CachedRoot,
    compiler: &PatternCompiler,
    config: &SastEngineConfig,
    rule_id: &str,
) -> Result<(Vec<RangeWithMetavars>, Option<TruncationReason>), SastError> {
    let now = Instant::now();
    let deadline = now + Duration::from_millis(config.rule_timeout_ms);

    let mut ctx = EvalContext {
        compiler,
        config,
        rule_id,
        deadline,
        start_time: now,
        matches_found: 0,
    };

    eval_recursive(formula, cached_root, &mut ctx)
}

/// Convenience wrapper: evaluate a complete rule's formula and return matches.
///
/// This is the primary entry point for Phase 2 engine integration tests.
///
/// # Arguments
///
/// * `rule` - The compiled rule to evaluate.
/// * `cached_root` - The parsed source file.
/// * `compiler` - Shared pattern compiler.
/// * `config` - Engine configuration.
///
/// # Returns
///
/// Same as [`eval_formula`].
///
/// # Errors
///
/// Propagates [`SastError`] from [`eval_formula`].
///
/// # Examples
///
/// ```
/// use ast_grep_core::tree_sitter::LanguageExt;
/// use ast_grep_language::SupportLang;
/// use xzardgz::scanner::sast::ast::diagnostics::ErrorNodeDensity;
/// use xzardgz::scanner::sast::ast::parse::CachedRoot;
/// use xzardgz::scanner::sast::config::SastEngineConfig;
/// use xzardgz::scanner::sast::engine::formula::scan_rule;
/// use xzardgz::scanner::sast::engine::pattern::PatternCompiler;
/// use xzardgz::scanner::sast::rule::ir::{Formula, Leaf, RuleIr};
/// use xzardgz::scanner::sast::rule::metadata::Severity;
///
/// let root = SupportLang::Rust.ast_grep("fn foo() {}");
/// let density = ErrorNodeDensity::from_root(&root);
/// let cached = CachedRoot { root, density };
/// let compiler = PatternCompiler::new();
/// let config = SastEngineConfig::new();
/// let rule = RuleIr {
///     id: "test-rule".to_string(),
///     message: "test".to_string(),
///     languages: vec!["rust".to_string()],
///     severity: Severity::Warning,
///     metadata: None,
///     formula: Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string())),
///     fix: None,
/// };
/// let (matches, reason) = scan_rule(&rule, &cached, &compiler, &config).unwrap();
/// assert!(!matches.is_empty());
/// ```
pub fn scan_rule(
    rule: &RuleIr,
    cached_root: &CachedRoot,
    compiler: &PatternCompiler,
    config: &SastEngineConfig,
) -> Result<(Vec<RangeWithMetavars>, Option<TruncationReason>), SastError> {
    eval_formula(&rule.formula, cached_root, compiler, config, &rule.id)
}

// ---------------------------------------------------------------------------
// Recursive evaluator
// ---------------------------------------------------------------------------

/// Recursively evaluate a formula node, updating `ctx` in place.
///
/// # Errors
///
/// Returns [`SastError`] on pattern compilation failure.
fn eval_recursive(
    formula: &Formula,
    root: &CachedRoot,
    ctx: &mut EvalContext<'_>,
) -> Result<(Vec<RangeWithMetavars>, Option<TruncationReason>), SastError> {
    match formula {
        // ---------------------------------------------------------------
        // Leaf: AST pattern
        // ---------------------------------------------------------------
        Formula::Leaf(Leaf::Pattern(pattern_text)) => eval_leaf_pattern(pattern_text, root, ctx),

        // ---------------------------------------------------------------
        // Leaf: regex (handled by RegexModeScanner, not here)
        // ---------------------------------------------------------------
        Formula::Leaf(Leaf::Regex(_)) => Ok((vec![], None)),

        // ---------------------------------------------------------------
        // And: intersection of conjuncts minus negations, then conditions, then focus
        // ---------------------------------------------------------------
        Formula::And {
            conjuncts,
            negations,
            conditions,
            focus,
        } => eval_and(conjuncts, negations, conditions, focus, root, ctx),

        // ---------------------------------------------------------------
        // Or: union of alternatives
        // ---------------------------------------------------------------
        Formula::Or(children) => eval_or(children, root, ctx),

        // ---------------------------------------------------------------
        // Inside: standalone (not as a conjunct in And) - evaluate inner
        // ---------------------------------------------------------------
        Formula::Inside(inner) => {
            // When used as a top-level formula, Inside just forwards to the
            // inner formula. The containment filtering semantics apply only
            // when Inside appears as a conjunct inside an And node.
            eval_recursive(inner, root, ctx)
        }
    }
}

// ---------------------------------------------------------------------------
// Leaf pattern evaluation
// ---------------------------------------------------------------------------

/// Evaluate a `Leaf::Pattern` node by running the compiled pattern against
/// every node in the tree via DFS.
fn eval_leaf_pattern(
    pattern_text: &str,
    root: &CachedRoot,
    ctx: &mut EvalContext<'_>,
) -> Result<(Vec<RangeWithMetavars>, Option<TruncationReason>), SastError> {
    // Pre-iteration timeout and cap checks.
    if ctx.is_timed_out() {
        return Ok((
            vec![],
            Some(TruncationReason::Timeout {
                elapsed_ms: ctx.elapsed_ms(),
            }),
        ));
    }
    if ctx.is_match_cap_reached() {
        return Ok((
            vec![],
            Some(TruncationReason::MaxMatchesReached {
                limit: ctx.config.max_matches_per_file,
            }),
        ));
    }

    let compiled: Arc<Pattern> =
        ctx.compiler
            .compile(pattern_text, SupportLang::Rust, MatchStrictness::Relaxed)?;

    let mut results: Vec<RangeWithMetavars> = Vec::new();
    let mut truncation: Option<TruncationReason> = None;

    for node_match in root.root.root().find_all(&*compiled) {
        // Per-iteration timeout check.
        if ctx.is_timed_out() {
            truncation = Some(TruncationReason::Timeout {
                elapsed_ms: ctx.elapsed_ms(),
            });
            break;
        }
        // Per-iteration cap check.
        if ctx.is_match_cap_reached() {
            truncation = Some(TruncationReason::MaxMatchesReached {
                limit: ctx.config.max_matches_per_file,
            });
            break;
        }

        let node = node_match.get_node();
        let range = node.range();
        let start = range.start;
        let end = range.end;

        // Collect metavariable bindings as owned values to avoid borrow
        // conflicts with the CachedRoot. The NodeMatch and its MetaVarEnv
        // borrow from `root`, so all data must be extracted into owned values
        // before the next iteration.
        let env = node_match.get_env();
        let mut bindings: MetavarBindings = MetavarBindings::new();
        for mv in env.get_matched_variables() {
            if let MetaVariable::Capture(name, _) = mv
                && let Some(bound_node) = env.get_match(&name)
            {
                let bound_range = bound_node.range();
                let text = bound_node.text().to_string();
                // Store with the conventional $ prefix.
                bindings.insert(
                    format!("${name}"),
                    MetavarValue {
                        text,
                        start: bound_range.start,
                        end: bound_range.end,
                    },
                );
            }
        }

        results.push(RangeWithMetavars::new(start, end, bindings));
        ctx.matches_found += 1;
    }

    Ok((results, truncation))
}

// ---------------------------------------------------------------------------
// And evaluation
// ---------------------------------------------------------------------------

/// Evaluate an `And` node: intersect conjuncts, apply Inside filters, subtract
/// negations, apply metavariable conditions, then apply focus-metavariable narrowing.
///
/// Evaluation order:
/// 1. Intersect regular conjuncts.
/// 2. Apply `Inside` containment filters.
/// 3. Subtract negations (`pattern-not*`).
/// 4. Apply metavariable conditions (`metavariable-regex`, `metavariable-pattern`,
///    `metavariable-comparison`).
/// 5. Apply `focus-metavariable` narrowing.
///
/// When a condition returns an unsupported-construct or recursion-limit error,
/// the evaluation returns an empty set (the rule is effectively skipped for this
/// file without aborting the broader scan).
fn eval_and(
    conjuncts: &[Formula],
    negations: &[Formula],
    conditions: &[Condition],
    focus: &[MetavarId],
    root: &CachedRoot,
    ctx: &mut EvalContext<'_>,
) -> Result<(Vec<RangeWithMetavars>, Option<TruncationReason>), SastError> {
    // Split conjuncts into regular formulas and Inside containers.
    let mut regular_conjuncts: Vec<&Formula> = Vec::new();
    let mut inside_formulas: Vec<&Formula> = Vec::new();

    for conjunct in conjuncts {
        match conjunct {
            Formula::Inside(inner) => inside_formulas.push(inner.as_ref()),
            other => regular_conjuncts.push(other),
        }
    }

    // If there are no regular conjuncts, there is no positive anchor to match;
    // return empty (cannot determine containment without a candidate set).
    if regular_conjuncts.is_empty() {
        return Ok((vec![], None));
    }

    let mut truncation: Option<TruncationReason> = None;

    // --- Evaluate and intersect regular conjuncts ---

    // Timeout guard before first conjunct evaluation.
    if ctx.is_timed_out() {
        return Ok((
            vec![],
            Some(TruncationReason::Timeout {
                elapsed_ms: ctx.elapsed_ms(),
            }),
        ));
    }

    let (first_ranges, first_trunc) = eval_recursive(regular_conjuncts[0], root, ctx)?;
    let mut current_ranges = first_ranges;
    if first_trunc.is_some() {
        truncation = first_trunc;
    }

    for conjunct in &regular_conjuncts[1..] {
        // Early exit on truncation or empty intersection so far.
        if current_ranges.is_empty() {
            break;
        }
        if ctx.is_timed_out() {
            truncation = Some(TruncationReason::Timeout {
                elapsed_ms: ctx.elapsed_ms(),
            });
            return Ok((current_ranges, truncation));
        }

        let (next_ranges, next_trunc) = eval_recursive(conjunct, root, ctx)?;
        if next_trunc.is_some() && truncation.is_none() {
            truncation = next_trunc;
        }

        // Cross-product intersection: keep pairs that overlap.
        let mut intersected: Vec<RangeWithMetavars> = Vec::new();
        for a in &current_ranges {
            for b in &next_ranges {
                if let Some(merged) = intersect(a, b) {
                    intersected.push(merged);
                }
            }
        }
        current_ranges = intersected;
    }

    // --- Apply Inside containment filters ---

    for inside_formula in inside_formulas {
        if current_ranges.is_empty() {
            break;
        }
        if ctx.is_timed_out() {
            truncation = Some(TruncationReason::Timeout {
                elapsed_ms: ctx.elapsed_ms(),
            });
            return Ok((current_ranges, truncation));
        }

        let (container_ranges, container_trunc) = eval_recursive(inside_formula, root, ctx)?;
        if container_trunc.is_some() && truncation.is_none() {
            truncation = container_trunc;
        }

        // Keep only candidates that are fully contained within at least one container.
        current_ranges.retain(|candidate| {
            container_ranges
                .iter()
                .any(|container| container.contains(candidate))
        });
    }

    // --- Subtract negations ---

    if !negations.is_empty() {
        let mut neg_sets: Vec<Vec<RangeWithMetavars>> = Vec::new();

        for negation in negations {
            if ctx.is_timed_out() {
                truncation = Some(TruncationReason::Timeout {
                    elapsed_ms: ctx.elapsed_ms(),
                });
                return Ok((current_ranges, truncation));
            }

            let (neg_ranges, neg_trunc) = eval_recursive(negation, root, ctx)?;
            if neg_trunc.is_some() && truncation.is_none() {
                truncation = neg_trunc;
            }
            neg_sets.push(neg_ranges);
        }

        let all_negations = union(neg_sets);
        current_ranges = subtract(current_ranges, &all_negations);
    }

    // --- Apply metavariable conditions ---
    //
    // Conditions are evaluated after negation subtraction. Each range is tested
    // against all conditions in order; ranges that fail any condition are dropped.
    // Unsupported-comparison or recursion-limit errors skip the entire rule
    // evaluation for this file (return empty, no abort).
    if !conditions.is_empty() && !current_ranges.is_empty() {
        current_ranges = match apply_conditions(current_ranges, conditions, ctx.compiler, 0) {
            Ok(filtered) => filtered,
            Err(ConditionError::UnsupportedComparison(_))
            | Err(ConditionError::RecursionLimitExceeded) => {
                // Skip this rule evaluation: unsupported construct or depth exceeded.
                return Ok((vec![], truncation));
            }
            Err(ConditionError::RegexCompile(cause)) => {
                return Err(SastError::RegexCompile {
                    rule_id: ctx.rule_id.to_string(),
                    cause,
                });
            }
            Err(ConditionError::UnboundMetavar(_)) | Err(ConditionError::TypeMismatch(_)) => {
                // These errors should never bubble up from apply_conditions
                // (they are handled per-range inside it). Treat as empty.
                vec![]
            }
        };
    }

    // --- Apply focus-metavariable narrowing ---
    //
    // Focus is applied last. Each surviving range is narrowed to the byte range
    // of the focused metavariable(s). Ranges with missing or non-overlapping
    // focus bindings are dropped.
    if !focus.is_empty() && !current_ranges.is_empty() {
        current_ranges = apply_focus(current_ranges, focus);
    }

    Ok((current_ranges, truncation))
}

// ---------------------------------------------------------------------------
// Or evaluation
// ---------------------------------------------------------------------------

/// Evaluate an `Or` node by evaluating each child and unioning the results.
fn eval_or(
    children: &[Formula],
    root: &CachedRoot,
    ctx: &mut EvalContext<'_>,
) -> Result<(Vec<RangeWithMetavars>, Option<TruncationReason>), SastError> {
    if children.is_empty() {
        return Ok((vec![], None));
    }

    let mut all_sets: Vec<Vec<RangeWithMetavars>> = Vec::new();
    let mut first_truncation: Option<TruncationReason> = None;

    for child in children {
        if ctx.is_timed_out() {
            let trunc = TruncationReason::Timeout {
                elapsed_ms: ctx.elapsed_ms(),
            };
            if first_truncation.is_none() {
                first_truncation = Some(trunc);
            }
            break;
        }

        let (child_ranges, child_trunc) = eval_recursive(child, root, ctx)?;
        if child_trunc.is_some() && first_truncation.is_none() {
            first_truncation = child_trunc;
        }
        all_sets.push(child_ranges);
    }

    let merged = union(all_sets);
    Ok((merged, first_truncation))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ast_grep_core::tree_sitter::LanguageExt;

    use crate::scanner::sast::ast::diagnostics::ErrorNodeDensity;
    use crate::scanner::sast::ast::parse::CachedRoot;
    use crate::scanner::sast::config::SastEngineConfig;
    use crate::scanner::sast::rule::ir::{Condition, Formula, Leaf};

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_cached_root(src: &str) -> CachedRoot {
        let root = SupportLang::Rust.ast_grep(src);
        let density = ErrorNodeDensity::from_root(&root);
        CachedRoot { root, density }
    }

    fn default_compiler() -> PatternCompiler {
        PatternCompiler::new()
    }

    fn default_config() -> SastEngineConfig {
        SastEngineConfig::new()
    }

    // -----------------------------------------------------------------------
    // Leaf::Pattern tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_eval_leaf_pattern_finds_simple_match() {
        let cached = make_cached_root("fn foo() {}");
        let compiler = default_compiler();
        let config = default_config();
        let formula = Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string()));

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        assert!(!matches.is_empty(), "pattern must match fn declaration");
        assert!(reason.is_none(), "no truncation expected for small input");
    }

    #[test]
    fn test_eval_leaf_pattern_no_match_returns_empty() {
        let cached = make_cached_root("let x = 42;");
        let compiler = default_compiler();
        let config = default_config();
        let formula = Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string()));

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        assert!(
            matches.is_empty(),
            "pattern must not match unrelated source"
        );
        assert!(reason.is_none());
    }

    // -----------------------------------------------------------------------
    // Leaf::Regex tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_eval_leaf_regex_returns_empty() {
        let cached = make_cached_root("fn foo() {}");
        let compiler = default_compiler();
        let config = default_config();
        let formula = Formula::Leaf(Leaf::Regex("fn \\w+".to_string()));

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        assert!(
            matches.is_empty(),
            "Leaf::Regex must return empty in AST mode"
        );
        assert!(reason.is_none());
    }

    // -----------------------------------------------------------------------
    // Or tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_eval_or_finds_either_alternative() {
        // Source contains Md5::new() but not Sha1::new().
        let cached = make_cached_root("let h = Md5::new();");
        let compiler = default_compiler();
        let config = default_config();
        let formula = Formula::Or(vec![
            Formula::Leaf(Leaf::Pattern("Md5::new()".to_string())),
            Formula::Leaf(Leaf::Pattern("Sha1::new()".to_string())),
        ]);

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        assert!(
            !matches.is_empty(),
            "Or must find the Md5::new() alternative"
        );
        assert!(reason.is_none());
    }

    #[test]
    fn test_eval_or_empty_formula_returns_empty() {
        let cached = make_cached_root("fn foo() {}");
        let compiler = default_compiler();
        let config = default_config();
        let formula = Formula::Or(vec![]);

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        assert!(matches.is_empty(), "Or([]) must return empty");
        assert!(reason.is_none());
    }

    // -----------------------------------------------------------------------
    // And tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_eval_and_single_conjunct_returns_matches() {
        let cached = make_cached_root("fn foo() {}");
        let compiler = default_compiler();
        let config = default_config();
        let formula = Formula::And {
            conjuncts: vec![Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string()))],
            negations: vec![],
            conditions: vec![],
            focus: vec![],
        };

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        assert!(!matches.is_empty(), "And with one conjunct must match");
        assert!(reason.is_none());
    }

    #[test]
    fn test_eval_and_with_negation_removes_match() {
        // Both the positive and the negation match fn foo() {}.
        let cached = make_cached_root("fn foo() {}");
        let compiler = default_compiler();
        let config = default_config();
        let formula = Formula::And {
            conjuncts: vec![Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string()))],
            negations: vec![Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string()))],
            conditions: vec![],
            focus: vec![],
        };

        let (matches, _) = eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        assert!(
            matches.is_empty(),
            "negation that matches the same range must remove it"
        );
    }

    #[test]
    fn test_eval_and_two_conjuncts_intersects() {
        // Both patterns match the same fn declaration node.
        let cached = make_cached_root("fn foo() {}");
        let compiler = default_compiler();
        let config = default_config();
        let formula = Formula::And {
            conjuncts: vec![
                Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string())),
                Formula::Leaf(Leaf::Pattern("fn foo() {}".to_string())),
            ],
            negations: vec![],
            conditions: vec![],
            focus: vec![],
        };

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        assert!(
            !matches.is_empty(),
            "two overlapping conjuncts must produce a result"
        );
        assert!(reason.is_none());
    }

    #[test]
    fn test_eval_and_inside_containment_filters() {
        // Source: a function containing let declarations.
        // Note: single-capture metavars ($X) cannot match multi-token macro arguments,
        // so this test uses `let $NAME = $VAL` (which reliably matches a single binding)
        // rather than a macro invocation pattern.
        let src = "fn my_fn() { let x = 1; let y = 2; }";
        let cached = make_cached_root(src);
        let compiler = default_compiler();
        let config = default_config();

        // Pattern: find let declarations inside fn my_fn.
        // The Inside container uses Semgrep `...` ellipsis, which PatternCompiler
        // rewrites to `$$$` (ast-grep multi-metavar) before compilation.
        let formula = Formula::And {
            conjuncts: vec![
                Formula::Leaf(Leaf::Pattern("let $NAME = $VAL".to_string())),
                Formula::Inside(Box::new(Formula::Leaf(Leaf::Pattern(
                    "fn my_fn() { ... }".to_string(),
                )))),
            ],
            negations: vec![],
            conditions: vec![],
            focus: vec![],
        };

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        assert!(
            !matches.is_empty(),
            "let declarations inside my_fn should match; got empty set"
        );
        assert!(reason.is_none());
    }

    // -----------------------------------------------------------------------
    // Truncation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_eval_formula_truncates_at_max_matches() {
        let mut config = SastEngineConfig::new();
        config.max_matches_per_file = 2;

        // Five distinct function declarations.
        let src = "fn a(){} fn b(){} fn c(){} fn d(){} fn e(){}";
        let cached = make_cached_root(src);
        let compiler = default_compiler();
        let formula = Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string()));

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        assert!(
            matches.len() <= 2,
            "must not exceed max_matches_per_file; got {}",
            matches.len()
        );
        assert!(reason.is_some(), "truncation reason must be present");
        assert!(
            matches!(
                reason.unwrap(),
                TruncationReason::MaxMatchesReached { limit: 2 }
            ),
            "truncation reason must be MaxMatchesReached with limit 2"
        );
    }

    #[test]
    fn test_eval_formula_timeout_returns_truncation_reason() {
        let mut config = SastEngineConfig::new();
        // Setting timeout to 0 ms means the deadline is set to `now`, so any
        // work performed after construction will detect the timeout on the
        // first check. The exact outcome (empty + Timeout vs. some matches +
        // Timeout) is racy, so this test only verifies no panic occurs and that
        // either no truncation was reported (extremely fast machines) or a
        // Timeout reason is present.
        config.rule_timeout_ms = 0;

        let cached = make_cached_root("fn foo() {} fn bar() {} fn baz() {}");
        let compiler = default_compiler();
        let formula = Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string()));

        let result = eval_formula(&formula, &cached, &compiler, &config, "test");
        // Must not return Err - timeout is a soft truncation, not an error.
        assert!(
            result.is_ok(),
            "timeout must not produce Err, got: {result:?}"
        );

        let (matches, reason) = result.unwrap();
        // Either the engine found all matches before detecting the timeout (fast
        // machine) or it stopped early with a Timeout reason. Both are valid.
        if let Some(r) = reason {
            assert!(
                matches!(r, TruncationReason::Timeout { .. }),
                "if truncation is present it must be Timeout, not MaxMatchesReached"
            );
        }
        // Ensure the returned data is self-consistent (no panic, valid vec).
        let _ = matches;
    }

    // -----------------------------------------------------------------------
    // scan_rule convenience wrapper
    // -----------------------------------------------------------------------

    #[test]
    fn test_scan_rule_delegates_to_eval_formula() {
        use crate::scanner::sast::rule::ir::RuleIr;
        use crate::scanner::sast::rule::metadata::Severity;

        let cached = make_cached_root("fn foo() {}");
        let compiler = default_compiler();
        let config = default_config();

        let rule = RuleIr {
            id: "test-rule".to_string(),
            message: "test message".to_string(),
            languages: vec!["rust".to_string()],
            severity: Severity::Warning,
            metadata: None,
            formula: Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string())),
            fix: None,
        };

        let (matches, reason) = scan_rule(&rule, &cached, &compiler, &config).unwrap();
        assert!(
            !matches.is_empty(),
            "scan_rule must find the fn declaration"
        );
        assert!(reason.is_none());
    }

    // -----------------------------------------------------------------------
    // Edge-case tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_eval_and_only_inside_conjuncts_returns_empty() {
        // And with only Inside conjuncts and no regular conjuncts must return empty.
        let cached = make_cached_root("fn foo() { let x = 1; }");
        let compiler = default_compiler();
        let config = default_config();
        let formula = Formula::And {
            conjuncts: vec![Formula::Inside(Box::new(Formula::Leaf(Leaf::Pattern(
                "fn foo() { $$$BODY }".to_string(),
            ))))],
            negations: vec![],
            conditions: vec![],
            focus: vec![],
        };

        let (matches, _) = eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        assert!(
            matches.is_empty(),
            "And with only Inside conjuncts must return empty (no positive anchor)"
        );
    }

    #[test]
    fn test_eval_leaf_pattern_captures_metavar_binding() {
        let cached = make_cached_root("fn my_function() {}");
        let compiler = default_compiler();
        let config = default_config();
        let formula = Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string()));

        let (matches, _) = eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        assert!(!matches.is_empty());
        // The binding for $F should contain the function name.
        let bindings = &matches[0].bindings;
        assert!(
            bindings.contains_key("$F"),
            "metavar $F must be bound; bindings: {bindings:?}"
        );
        assert_eq!(
            bindings.get("$F").map(|v| v.text.as_str()),
            Some("my_function"),
            "metavar $F must be bound to the function name"
        );
    }

    #[test]
    fn test_eval_or_both_alternatives_match_deduplicates_by_range() {
        // Both alternatives match the same source text at the same byte range
        // but produce different metavar bindings ($A vs $B). The `union` function
        // in range.rs deduplicates by (start, end, bindings), so entries with
        // different bindings are kept as distinct results. Verify Or finds at
        // least one match from the alternatives.
        let cached = make_cached_root("fn foo() {}");
        let compiler = default_compiler();
        let config = default_config();
        let formula = Formula::Or(vec![
            Formula::Leaf(Leaf::Pattern("fn $A() {}".to_string())),
            Formula::Leaf(Leaf::Pattern("fn $B() {}".to_string())),
        ]);

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        // Both alternatives match; union keeps them if bindings differ.
        // At minimum one match must be found.
        assert!(
            !matches.is_empty(),
            "Or must find at least one match; got 0"
        );
        assert!(reason.is_none());
    }

    #[test]
    fn test_eval_inside_standalone_forwards_to_inner() {
        let cached = make_cached_root("fn foo() {}");
        let compiler = default_compiler();
        let config = default_config();
        // Standalone Inside forwards evaluation to the inner formula.
        let formula = Formula::Inside(Box::new(Formula::Leaf(Leaf::Pattern(
            "fn $F() {}".to_string(),
        ))));

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();

        assert!(
            !matches.is_empty(),
            "standalone Inside must forward to inner formula"
        );
        assert!(reason.is_none());
    }

    // Phase 3: In Phase 3, conditions and focus ARE evaluated.
    // The regex "foo" (anchored) matches the binding $F = "foo", so the range survives.
    // The focus on $F narrows the reported range to the byte range of `foo` in the source.
    #[test]
    fn test_eval_and_metavar_regex_condition_filters_non_matching_binding() {
        let source = "fn foo() {} fn bar() {}".to_string();
        let cached = make_cached_root(&source);
        let compiler = default_compiler();
        let config = default_config();
        // Pattern matches both `fn foo() {}` and `fn bar() {}`.
        // The regex condition `$F =~ ^foo$` keeps only the `foo` match.
        let formula = Formula::And {
            conjuncts: vec![Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string()))],
            negations: vec![],
            conditions: vec![Condition::MetavarRegex {
                metavar: "$F".to_string(),
                regex: "foo".to_string(),
                not: false,
            }],
            focus: vec![],
        };

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();
        assert!(reason.is_none());
        // Only the `foo` function should remain after the regex condition.
        assert_eq!(
            matches.len(),
            1,
            "regex condition must filter out the bar match"
        );
        let binding = matches[0].bindings.get("$F").expect("$F must be bound");
        assert_eq!(binding.text, "foo");
    }

    #[test]
    fn test_eval_and_metavar_regex_condition_not_flag_inverts_filter() {
        let source = "fn foo() {} fn bar() {}".to_string();
        let cached = make_cached_root(&source);
        let compiler = default_compiler();
        let config = default_config();
        // With not=true, the regex condition keeps only the functions whose name does NOT match "foo".
        let formula = Formula::And {
            conjuncts: vec![Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string()))],
            negations: vec![],
            conditions: vec![Condition::MetavarRegex {
                metavar: "$F".to_string(),
                regex: "foo".to_string(),
                not: true,
            }],
            focus: vec![],
        };

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();
        assert!(reason.is_none());
        // Only the `bar` function should remain.
        assert_eq!(
            matches.len(),
            1,
            "inverted regex condition must keep only non-foo match"
        );
        let binding = matches[0].bindings.get("$F").expect("$F must be bound");
        assert_eq!(binding.text, "bar");
    }

    #[test]
    fn test_eval_and_focus_metavariable_narrows_range() {
        // The source has `fn foo() {}`. Matching `fn $F() {}` binds $F to "foo".
        // After focus on $F the reported range should span only "foo".
        let source = "fn foo() {}".to_string();
        let foo_start = source.find("foo").expect("'foo' must be in source");
        let foo_end = foo_start + 3;
        let cached = make_cached_root(&source);
        let compiler = default_compiler();
        let config = default_config();
        let formula = Formula::And {
            conjuncts: vec![Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string()))],
            negations: vec![],
            conditions: vec![],
            focus: vec!["$F".to_string()],
        };

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();
        assert!(reason.is_none());
        assert_eq!(matches.len(), 1, "focus must produce exactly one match");
        assert_eq!(
            matches[0].start, foo_start,
            "focus must narrow start to the bound metavar"
        );
        assert_eq!(
            matches[0].end, foo_end,
            "focus must narrow end to the bound metavar"
        );
    }

    #[test]
    fn test_eval_and_metavar_comparison_filters_weak_rsa_key() {
        // Worked-example integration test: rust-weak-rsa-key.
        // A call with 1024 bits must match; a call with 2048 bits must not.
        let source = "let key = RsaPrivateKey::new(&mut rng, 1024).unwrap();".to_string();
        let bits_start = source.find("1024").expect("'1024' must be in source");
        let bits_end = bits_start + 4;
        let cached = make_cached_root(&source);
        let compiler = default_compiler();
        let config = default_config();

        let formula = Formula::And {
            conjuncts: vec![Formula::Leaf(Leaf::Pattern(
                "RsaPrivateKey::new(&mut $RNG, $BITS)".to_string(),
            ))],
            negations: vec![],
            conditions: vec![Condition::MetavarComparison {
                metavar: "$BITS".to_string(),
                comparison: "$BITS < 2048".to_string(),
                strip: false,
                base: None,
            }],
            focus: vec!["$BITS".to_string()],
        };

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();
        assert!(reason.is_none());
        assert_eq!(matches.len(), 1, "weak RSA call must match");
        assert_eq!(
            matches[0].start, bits_start,
            "focus must narrow start to $BITS"
        );
        assert_eq!(matches[0].end, bits_end, "focus must narrow end to $BITS");
    }

    #[test]
    fn test_eval_and_metavar_comparison_does_not_match_compliant_rsa_key() {
        // A call with 2048 bits must NOT match: 2048 < 2048 is false.
        let source = "let key = RsaPrivateKey::new(&mut rng, 2048).unwrap();".to_string();
        let cached = make_cached_root(&source);
        let compiler = default_compiler();
        let config = default_config();

        let formula = Formula::And {
            conjuncts: vec![Formula::Leaf(Leaf::Pattern(
                "RsaPrivateKey::new(&mut $RNG, $BITS)".to_string(),
            ))],
            negations: vec![],
            conditions: vec![Condition::MetavarComparison {
                metavar: "$BITS".to_string(),
                comparison: "$BITS < 2048".to_string(),
                strip: false,
                base: None,
            }],
            focus: vec!["$BITS".to_string()],
        };

        let (matches, reason) =
            eval_formula(&formula, &cached, &compiler, &config, "test").unwrap();
        assert!(reason.is_none());
        assert!(
            matches.is_empty(),
            "compliant RSA key must not match (2048 < 2048 is false)"
        );
    }

    #[test]
    fn test_eval_and_unsupported_comparison_returns_empty_not_error() {
        // An unsupported comparison expression (arithmetic) must not panic or error;
        // it must return an empty match set (rule is effectively skipped).
        let cached = make_cached_root("fn foo() {}");
        let compiler = default_compiler();
        let config = default_config();
        let formula = Formula::And {
            conjuncts: vec![Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string()))],
            negations: vec![],
            conditions: vec![Condition::MetavarComparison {
                metavar: "$F".to_string(),
                comparison: "$F + 1 < 100".to_string(), // unsupported: arithmetic
                strip: false,
                base: None,
            }],
            focus: vec![],
        };

        let result = eval_formula(&formula, &cached, &compiler, &config, "test");
        assert!(
            result.is_ok(),
            "unsupported comparison must not produce Err"
        );
        let (matches, _) = result.unwrap();
        assert!(
            matches.is_empty(),
            "unsupported comparison must produce empty results (rule skipped)"
        );
    }
}
