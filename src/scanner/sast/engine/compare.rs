//! Closed-grammar comparison expression evaluator for `metavariable-comparison` conditions.
//!
//! This module implements a strict subset of Python-like boolean/comparison
//! expressions used by Semgrep's `metavariable-comparison` condition key.
//! Only the constructs listed in the supported grammar below are accepted; all
//! others produce [`CompareError::UnsupportedConstruct`] at tokenisation or
//! parse time.  The parent rule is skipped rather than silently accepting
//! unsafe expressions or panicking.
//!
//! # Supported Grammar
//!
//! ```text
//! expr     := or_expr EOF
//! or_expr  := and_expr ('or' and_expr)*
//! and_expr := not_expr ('and' not_expr)*
//! not_expr := 'not' not_expr | cmp_expr
//! cmp_expr := value (cmp_op value)?
//! cmp_op   := '<' | '<=' | '>' | '>=' | '==' | '!='
//! value    := METAVAR | NUMBER
//! METAVAR  := '$' [A-Z_][A-Z0-9_]*
//! NUMBER   := decimal integer or float literal
//! ```
//!
//! The keywords `and`, `or`, and `not` are lowercase only.  Metavariable
//! names must start with an uppercase letter or underscore and contain only
//! uppercase letters, digits, and underscores after the `$` sigil.
//!
//! # Unsupported Constructs
//!
//! The following produce [`CompareError::UnsupportedConstruct`] and cause the
//! parent rule to be skipped rather than aborting the scan:
//!
//! - Arithmetic operators (`+`, `-`, `*`, `/`)
//! - Grouping (`(`, `)`, `[`, `]`, `{`, `}`)
//! - String literals (any quote character)
//! - Function calls and arbitrary identifiers
//! - Any character not present in the grammar above
//!
//! # Metavariable Resolution
//!
//! Metavariable names are looked up in the supplied [`MetavarBindings`] map.
//! The bound text is converted to `f64` for comparison according to
//! [`CompareOptions`]:
//!
//! - If `strip` is `true`, trailing alphabetic characters and ASCII whitespace
//!   are removed from the bound text before parsing (e.g. `"1024k"` becomes
//!   `"1024"`).
//! - If `base` is `Some(b)`, the (optionally stripped) text is parsed as a
//!   signed integer in base `b` (e.g. `base: Some(16)` treats `"1000"` as
//!   `0x1000 = 4096`).
//! - Otherwise the text is parsed as a decimal integer or float.

use std::fmt;

use crate::scanner::sast::engine::range::{MetavarBindings, MetavarValue};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Options controlling how metavariable text is parsed in comparisons.
///
/// These options mirror the `strip` and `base` keys in a Semgrep
/// `metavariable-comparison` condition.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::engine::compare::CompareOptions;
///
/// let defaults = CompareOptions::default();
/// assert!(!defaults.strip);
/// assert!(defaults.base.is_none());
///
/// let hex = CompareOptions { strip: false, base: Some(16) };
/// assert_eq!(hex.base, Some(16));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareOptions {
    /// When `true`, trailing alphabetic characters and ASCII whitespace are
    /// stripped from the bound text before parsing.  For example `"1024k"`
    /// becomes `"1024"`.
    pub strip: bool,
    /// Optional numeric base for parsing metavariable text (e.g. `16` for
    /// hexadecimal).  When `None`, decimal (base 10) is used.
    pub base: Option<u32>,
}

impl Default for CompareOptions {
    /// Returns `CompareOptions` with `strip: false` and `base: None`.
    fn default() -> Self {
        Self {
            strip: false,
            base: None,
        }
    }
}

/// Error from the comparison expression evaluator.
///
/// Each variant represents a distinct failure mode.  Callers should treat
/// [`CompareError::UnsupportedConstruct`] as a signal to skip the containing
/// rule rather than as a hard error; the expression is syntactically outside
/// the supported grammar.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::engine::compare::{CompareError, validate_comparison};
///
/// let err = validate_comparison("$X + $Y < 10").unwrap_err();
/// assert!(matches!(err, CompareError::UnsupportedConstruct { .. }));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareError {
    /// The expression uses constructs outside the supported grammar.
    ///
    /// The `detail` field contains a human-readable description of the
    /// unsupported construct.  This variant causes the parent rule to be
    /// skipped; it is never a fatal engine error.
    UnsupportedConstruct {
        /// Human-readable description of the unsupported construct.
        detail: String,
    },
    /// A referenced metavariable was not bound in the current match.
    UnboundMetavar(String),
    /// The bound metavariable text could not be parsed as a number.
    TypeMismatch(String),
}

impl fmt::Display for CompareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedConstruct { detail } => {
                write!(
                    f,
                    "unsupported construct in comparison expression: {detail}"
                )
            }
            Self::UnboundMetavar(name) => {
                write!(f, "metavar '{name}' is not bound in this match")
            }
            Self::TypeMismatch(msg) => {
                write!(f, "type mismatch: {msg}")
            }
        }
    }
}

impl std::error::Error for CompareError {}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Evaluate a comparison expression against metavariable bindings.
///
/// Parses and evaluates `expr` within the supported grammar (see module
/// docs).  Metavariable references are resolved from `bindings` and converted
/// to `f64` values according to `options`.
///
/// # Arguments
///
/// * `expr` - The comparison expression string to evaluate.
/// * `bindings` - Metavariable bindings from the current pattern match.
/// * `options` - Options controlling how metavariable text is parsed.
///
/// # Returns
///
/// `Ok(true)` if the expression evaluates to `true`; `Ok(false)` otherwise.
///
/// # Errors
///
/// - [`CompareError::UnsupportedConstruct`] if `expr` contains syntax outside
///   the supported grammar.
/// - [`CompareError::UnboundMetavar`] if a referenced metavariable is absent
///   from `bindings`.
/// - [`CompareError::TypeMismatch`] if a bound metavariable text cannot be
///   parsed as a number under the current `options`.
///
/// # Panics
///
/// Never panics on any input.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use xzardgz::scanner::sast::engine::compare::{CompareOptions, eval_comparison};
/// use xzardgz::scanner::sast::engine::range::MetavarValue;
///
/// let mut bindings = BTreeMap::new();
/// bindings.insert(
///     "$BITS".to_string(),
///     MetavarValue { text: "1024".to_string(), start: 0, end: 4 },
/// );
/// let options = CompareOptions::default();
/// assert!(eval_comparison("$BITS < 2048", &bindings, &options).unwrap());
/// ```
pub fn eval_comparison(
    expr: &str,
    bindings: &MetavarBindings,
    options: &CompareOptions,
) -> Result<bool, CompareError> {
    let tokens = tokenize(expr)?;
    let mut parser = Parser::for_eval(&tokens, bindings, options);
    parser.parse_expr()
}

/// Validate a comparison expression string without evaluating it.
///
/// Parses `expr` according to the supported grammar, treating every
/// `$METAVAR` reference as a placeholder with value `0.0`.  No bindings are
/// required.
///
/// Intended for use at rule compile time by the compatibility gate to catch
/// unsupported syntax before any match is attempted.
///
/// # Arguments
///
/// * `expr` - The comparison expression string to validate.
///
/// # Returns
///
/// `Ok(())` if the expression conforms to the supported grammar.
///
/// # Errors
///
/// [`CompareError::UnsupportedConstruct`] if `expr` uses syntax outside the
/// supported grammar.
///
/// # Panics
///
/// Never panics on any input.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::engine::compare::validate_comparison;
///
/// assert!(validate_comparison("$BITS < 2048").is_ok());
/// assert!(validate_comparison("$A < 5 and $B > 3 or not $C == 0").is_ok());
/// assert!(validate_comparison("$X + $Y < 10").is_err());
/// assert!(validate_comparison("($X < 2048)").is_err());
/// ```
pub fn validate_comparison(expr: &str) -> Result<(), CompareError> {
    let tokens = tokenize(expr)?;
    let mut parser = Parser::for_validate(&tokens);
    parser.parse_expr().map(|_| ())
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

// Tokens produced by the comparison expression lexer.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    // Metavariable name including the '$' prefix, e.g. "$BITS".
    MetaVar(String),
    // Numeric literal parsed as f64.
    Number(f64),
    // Keyword 'and'.
    And,
    // Keyword 'or'.
    Or,
    // Keyword 'not'.
    Not,
    // Operator '<'.
    Lt,
    // Operator '<='.
    Lte,
    // Operator '>'.
    Gt,
    // Operator '>='.
    Gte,
    // Operator '=='.
    Eq,
    // Operator '!='.
    Neq,
}

// Convert the expression string into a flat list of tokens.
//
// Returns `CompareError::UnsupportedConstruct` on the first unsupported
// character or construct encountered.
fn tokenize(expr: &str) -> Result<Vec<Token>, CompareError> {
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0usize;
    let mut tokens: Vec<Token> = Vec::new();

    while i < chars.len() {
        let c = chars[i];

        // Skip ASCII whitespace.
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        // Metavariable: '$' [A-Z_][A-Z0-9_]*
        if c == '$' {
            i += 1;
            if i >= chars.len() {
                return Err(CompareError::UnsupportedConstruct {
                    detail: "bare '$' at end of expression".to_owned(),
                });
            }
            let first = chars[i];
            if !first.is_ascii_uppercase() && first != '_' {
                return Err(CompareError::UnsupportedConstruct {
                    detail: format!("metavar name must start with [A-Z_] after '$', got '{first}'"),
                });
            }
            let name_start = i;
            while i < chars.len()
                && (chars[i].is_ascii_uppercase() || chars[i].is_ascii_digit() || chars[i] == '_')
            {
                i += 1;
            }
            let name: String = std::iter::once('$')
                .chain(chars[name_start..i].iter().copied())
                .collect();
            tokens.push(Token::MetaVar(name));
            continue;
        }

        // Numeric literal: [0-9]+ ('.' [0-9]+)?
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < chars.len() && chars[i] == '.' {
                i += 1;
                if i >= chars.len() || !chars[i].is_ascii_digit() {
                    return Err(CompareError::UnsupportedConstruct {
                        detail: "numeric literal: digits expected after decimal point".to_owned(),
                    });
                }
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let num_str: String = chars[start..i].iter().collect();
            // SAFETY: The character sequence was validated to be ASCII digits
            // with at most one decimal point, so parse::<f64>() cannot fail.
            let num: f64 = num_str
                .parse()
                .map_err(|_| CompareError::UnsupportedConstruct {
                    detail: format!("internal: failed to parse validated numeral '{num_str}'"),
                })?;
            tokens.push(Token::Number(num));
            continue;
        }

        // Lowercase identifiers: only 'and', 'or', 'not' are allowed.
        if c.is_ascii_lowercase() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_alphabetic() {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            match word.as_str() {
                "and" => tokens.push(Token::And),
                "or" => tokens.push(Token::Or),
                "not" => tokens.push(Token::Not),
                other => {
                    return Err(CompareError::UnsupportedConstruct {
                        detail: format!(
                            "identifier '{other}' is not allowed; \
                             only 'and', 'or', 'not' are supported"
                        ),
                    });
                }
            }
            continue;
        }

        // Comparison operators and everything else.
        match c {
            '<' => {
                i += 1;
                if i < chars.len() && chars[i] == '=' {
                    i += 1;
                    tokens.push(Token::Lte);
                } else {
                    tokens.push(Token::Lt);
                }
            }
            '>' => {
                i += 1;
                if i < chars.len() && chars[i] == '=' {
                    i += 1;
                    tokens.push(Token::Gte);
                } else {
                    tokens.push(Token::Gt);
                }
            }
            '=' => {
                i += 1;
                if i < chars.len() && chars[i] == '=' {
                    i += 1;
                    tokens.push(Token::Eq);
                } else {
                    return Err(CompareError::UnsupportedConstruct {
                        detail: "single '=' is not valid; use '==' for equality".to_owned(),
                    });
                }
            }
            '!' => {
                i += 1;
                if i < chars.len() && chars[i] == '=' {
                    i += 1;
                    tokens.push(Token::Neq);
                } else {
                    return Err(CompareError::UnsupportedConstruct {
                        detail: "bare '!' is not supported; use '!=' for not-equal".to_owned(),
                    });
                }
            }
            other => {
                return Err(CompareError::UnsupportedConstruct {
                    detail: format!("character '{other}' is not part of the supported grammar"),
                });
            }
        }
    }

    Ok(tokens)
}

// ---------------------------------------------------------------------------
// Comparison operator
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum CmpOp {
    Lt,
    Lte,
    Gt,
    Gte,
    Eq,
    Neq,
}

// ---------------------------------------------------------------------------
// Recursive-descent parser / evaluator
// ---------------------------------------------------------------------------

// Fused parse-and-evaluate parser for comparison expressions.
//
// Two modes are supported, set at construction time:
//
// - Evaluation mode (Parser::for_eval): metavariable names are looked up in
//   the supplied MetavarBindings and resolved to f64 values.
// - Validation mode (Parser::for_validate): every metavariable is treated as
//   0.0 without consulting any bindings map.
struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    // Whether the parser is operating in validation mode.
    validation: bool,
    // Metavariable bindings; always Some in evaluation mode.
    bindings: Option<&'a MetavarBindings>,
    // Comparison options; always Some in evaluation mode.
    options: Option<&'a CompareOptions>,
}

impl<'a> Parser<'a> {
    // Create a parser in evaluation mode.
    fn for_eval(
        tokens: &'a [Token],
        bindings: &'a MetavarBindings,
        options: &'a CompareOptions,
    ) -> Self {
        Self {
            tokens,
            pos: 0,
            validation: false,
            bindings: Some(bindings),
            options: Some(options),
        }
    }

    // Create a parser in validation mode (no bindings needed).
    fn for_validate(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            pos: 0,
            validation: true,
            bindings: None,
            options: None,
        }
    }

    // Return a reference to the current token without consuming it.
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    // Advance past the current token.
    fn advance(&mut self) {
        self.pos += 1;
    }

    // expr := or_expr EOF
    fn parse_expr(&mut self) -> Result<bool, CompareError> {
        let result = self.parse_or_expr()?;
        if let Some(tok) = self.peek() {
            let detail = format!("unexpected token {:?} after expression", tok);
            return Err(CompareError::UnsupportedConstruct { detail });
        }
        Ok(result)
    }

    // or_expr := and_expr ('or' and_expr)*
    fn parse_or_expr(&mut self) -> Result<bool, CompareError> {
        let mut lhs = self.parse_and_expr()?;
        while matches!(self.peek(), Some(Token::Or)) {
            self.advance();
            let rhs = self.parse_and_expr()?;
            lhs = lhs || rhs;
        }
        Ok(lhs)
    }

    // and_expr := not_expr ('and' not_expr)*
    fn parse_and_expr(&mut self) -> Result<bool, CompareError> {
        let mut lhs = self.parse_not_expr()?;
        while matches!(self.peek(), Some(Token::And)) {
            self.advance();
            let rhs = self.parse_not_expr()?;
            lhs = lhs && rhs;
        }
        Ok(lhs)
    }

    // not_expr := 'not' not_expr | cmp_expr
    fn parse_not_expr(&mut self) -> Result<bool, CompareError> {
        if matches!(self.peek(), Some(Token::Not)) {
            self.advance();
            let inner = self.parse_not_expr()?;
            return Ok(!inner);
        }
        self.parse_cmp_expr()
    }

    // cmp_expr := value (cmp_op value)?
    fn parse_cmp_expr(&mut self) -> Result<bool, CompareError> {
        let lhs = self.parse_value()?;

        let op = match self.peek() {
            Some(Token::Lt) => CmpOp::Lt,
            Some(Token::Lte) => CmpOp::Lte,
            Some(Token::Gt) => CmpOp::Gt,
            Some(Token::Gte) => CmpOp::Gte,
            Some(Token::Eq) => CmpOp::Eq,
            Some(Token::Neq) => CmpOp::Neq,
            // No comparison operator: treat the value as truthy if non-zero.
            _ => return Ok(lhs != 0.0),
        };

        self.advance();
        let rhs = self.parse_value()?;
        Ok(apply_cmp(lhs, op, rhs))
    }

    // value := METAVAR | NUMBER
    fn parse_value(&mut self) -> Result<f64, CompareError> {
        let next = self.peek().cloned();
        match next {
            Some(Token::Number(n)) => {
                self.advance();
                Ok(n)
            }
            Some(Token::MetaVar(name)) => {
                self.advance();
                if self.validation {
                    // Validation mode: return dummy 0.0 without consulting bindings.
                    return Ok(0.0);
                }
                // SAFETY: `self.validation` is false only when the parser was
                // constructed via `Parser::for_eval`, which always sets both
                // `bindings` and `options` to `Some`. The flag and the fields
                // are always set together; neither unwrap() can fail here.
                let bindings = self.bindings.unwrap();
                let options = self.options.unwrap();
                let mv: &MetavarValue = bindings
                    .get(&name)
                    .ok_or_else(|| CompareError::UnboundMetavar(name.clone()))?;
                resolve_to_f64(&mv.text, options, &name)
            }
            Some(tok) => Err(CompareError::UnsupportedConstruct {
                detail: format!("expected value (metavar or number), got {:?}", tok),
            }),
            None => Err(CompareError::UnsupportedConstruct {
                detail: "expected value (metavar or number) but expression ended".to_owned(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

// Apply a binary comparison operator to two f64 operands.
fn apply_cmp(lhs: f64, op: CmpOp, rhs: f64) -> bool {
    match op {
        CmpOp::Lt => lhs < rhs,
        CmpOp::Lte => lhs <= rhs,
        CmpOp::Gt => lhs > rhs,
        CmpOp::Gte => lhs >= rhs,
        CmpOp::Eq => lhs == rhs,
        CmpOp::Neq => lhs != rhs,
    }
}

// Convert metavar bound text to f64 according to the supplied options.
//
// 1. If options.strip is true, trailing alphabetic characters and ASCII
//    whitespace are removed from `text` before parsing.
// 2. If options.base is Some(b), the (stripped) text is parsed as a signed
//    integer in base b.
// 3. Otherwise the text is parsed as a decimal float.
fn resolve_to_f64(text: &str, options: &CompareOptions, name: &str) -> Result<f64, CompareError> {
    let stripped;
    let working: &str = if options.strip {
        stripped = text.trim_end_matches(|c: char| c.is_alphabetic() || c.is_ascii_whitespace());
        stripped
    } else {
        text
    };

    if let Some(base) = options.base {
        let trimmed = working.trim();
        i64::from_str_radix(trimmed, base)
            .map(|n| n as f64)
            .map_err(|_| {
                CompareError::TypeMismatch(format!(
                    "metavar {name} bound to '{text}': \
                     cannot parse '{trimmed}' as an integer in base {base}"
                ))
            })
    } else {
        let trimmed = working.trim();
        trimmed.parse::<f64>().map_err(|_| {
            CompareError::TypeMismatch(format!(
                "metavar {name} bound to '{text}': cannot parse '{trimmed}' as a number"
            ))
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::sast::engine::range::{MetavarBindings, MetavarValue};

    // Build a MetavarValue with the text as content and dummy byte offsets.
    fn mv(text: &str) -> MetavarValue {
        MetavarValue {
            text: text.to_owned(),
            start: 0,
            end: text.len(),
        }
    }

    // Build a MetavarBindings map from (name, text) pairs.
    fn make_bindings(pairs: &[(&str, &str)]) -> MetavarBindings {
        pairs.iter().map(|(k, v)| (k.to_string(), mv(v))).collect()
    }

    // Return CompareOptions with no stripping and decimal (base 10) parsing.
    fn default_opts() -> CompareOptions {
        CompareOptions {
            strip: false,
            base: None,
        }
    }

    #[test]
    fn test_eval_comparison_less_than_with_true_operands_returns_true() {
        let bindings = make_bindings(&[("$BITS", "1024")]);
        let result = eval_comparison("$BITS < 2048", &bindings, &default_opts()).unwrap();
        assert!(result);
    }

    #[test]
    fn test_eval_comparison_less_than_with_false_operands_returns_false() {
        let bindings = make_bindings(&[("$BITS", "1024")]);
        let result = eval_comparison("$BITS < 1024", &bindings, &default_opts()).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_eval_comparison_less_than_equal_boundary_returns_true() {
        let bindings = make_bindings(&[("$BITS", "2048")]);
        let result = eval_comparison("$BITS <= 2048", &bindings, &default_opts()).unwrap();
        assert!(result);
    }

    #[test]
    fn test_eval_comparison_greater_than_returns_true() {
        let bindings = make_bindings(&[("$BITS", "1024")]);
        let result = eval_comparison("$BITS > 512", &bindings, &default_opts()).unwrap();
        assert!(result);
    }

    #[test]
    fn test_eval_comparison_greater_than_equal_boundary_returns_true() {
        let bindings = make_bindings(&[("$BITS", "2048")]);
        let result = eval_comparison("$BITS >= 2048", &bindings, &default_opts()).unwrap();
        assert!(result);
    }

    #[test]
    fn test_eval_comparison_equal_returns_true() {
        let bindings = make_bindings(&[("$BITS", "1024")]);
        let result = eval_comparison("$BITS == 1024", &bindings, &default_opts()).unwrap();
        assert!(result);
    }

    #[test]
    fn test_eval_comparison_not_equal_returns_true() {
        let bindings = make_bindings(&[("$BITS", "1024")]);
        let result = eval_comparison("$BITS != 2048", &bindings, &default_opts()).unwrap();
        assert!(result);
    }

    #[test]
    fn test_eval_comparison_and_precedence_over_or() {
        // Expression: "$A < 5 and $B < 5 or $C < 5"
        //
        // With and-higher-than-or precedence this parses as:
        //   ($A < 5 and $B < 5) or $C < 5
        //
        // Bindings chosen so the two interpretations produce different results:
        //   $A = 10 (fails < 5), $B = 3 (passes < 5), $C = 3 (passes < 5)
        //
        //   and-higher (correct): (false and true) or true = false or true = true
        //   or-higher  (wrong):   false and (true or true)  = false and true = false
        //
        // Asserting true verifies that 'and' binds tighter than 'or'.
        let bindings = make_bindings(&[("$A", "10"), ("$B", "3"), ("$C", "3")]);
        let result =
            eval_comparison("$A < 5 and $B < 5 or $C < 5", &bindings, &default_opts()).unwrap();
        assert!(result, "'and' must bind tighter than 'or'");
    }

    #[test]
    fn test_eval_comparison_strip_option_removes_suffix() {
        // "1024k" with strip=true strips the trailing 'k', leaving 1024.
        let bindings = make_bindings(&[("$X", "1024k")]);
        let opts = CompareOptions {
            strip: true,
            base: None,
        };
        let result = eval_comparison("$X < 2048", &bindings, &opts).unwrap();
        assert!(
            result,
            "strip=true must remove trailing 'k', leaving 1024 < 2048"
        );
    }

    #[test]
    fn test_eval_comparison_base_option_parses_hex() {
        // "1000" in base 16 equals 0x1000 = 4096; 4096 < 4097 is true.
        let bindings = make_bindings(&[("$X", "1000")]);
        let opts = CompareOptions {
            strip: false,
            base: Some(16),
        };
        let result = eval_comparison("$X < 4097", &bindings, &opts).unwrap();
        assert!(result, "base=16 must parse '1000' as 0x1000 = 4096 < 4097");
    }

    #[test]
    fn test_eval_comparison_type_mismatch_returns_error() {
        let bindings = make_bindings(&[("$X", "abc")]);
        let err = eval_comparison("$X < 1024", &bindings, &default_opts()).unwrap_err();
        assert!(
            matches!(err, CompareError::TypeMismatch(_)),
            "non-numeric bound text must produce TypeMismatch, got: {err:?}"
        );
    }

    #[test]
    fn test_eval_comparison_unsupported_construct_division_returns_error() {
        let bindings = make_bindings(&[("$X", "1")]);
        let err = eval_comparison("$X / 0 == 0", &bindings, &default_opts()).unwrap_err();
        assert!(
            matches!(err, CompareError::UnsupportedConstruct { .. }),
            "division must produce UnsupportedConstruct, got: {err:?}"
        );
    }

    #[test]
    fn test_eval_comparison_unsupported_construct_plus_returns_error() {
        let bindings = make_bindings(&[("$X", "1")]);
        let err = eval_comparison("$X + 1 < 2048", &bindings, &default_opts()).unwrap_err();
        assert!(
            matches!(err, CompareError::UnsupportedConstruct { .. }),
            "addition must produce UnsupportedConstruct, got: {err:?}"
        );
    }

    #[test]
    fn test_eval_comparison_unsupported_construct_parens_returns_error() {
        let bindings = make_bindings(&[("$X", "1")]);
        let err = eval_comparison("($X < 2048)", &bindings, &default_opts()).unwrap_err();
        assert!(
            matches!(err, CompareError::UnsupportedConstruct { .. }),
            "parentheses must produce UnsupportedConstruct, got: {err:?}"
        );
    }

    #[test]
    fn test_validate_comparison_valid_expression_returns_ok() {
        assert!(validate_comparison("$BITS < 2048").is_ok());
    }

    #[test]
    fn test_validate_comparison_unsupported_expression_returns_error() {
        let err = validate_comparison("$X + $Y < 10").unwrap_err();
        assert!(
            matches!(err, CompareError::UnsupportedConstruct { .. }),
            "arithmetic must produce UnsupportedConstruct, got: {err:?}"
        );
    }
}
