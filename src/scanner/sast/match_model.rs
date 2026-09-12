//! Match model types for the SAST scanner output.
//!
//! This module defines the neutral [`SastMatch`] type that all consumers of
//! the SAST engine receive, along with the supporting types [`Position`],
//! [`MatchSnippet`], and [`MetavarBinding`].
//!
//! All types are serialisable; downstream code can embed a `SastMatch`
//! directly in a report without further transformation.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::scanner::sast::rule::metadata::{Confidence, RuleMetadata, Severity};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Number of source-file lines to include on each side of the matched range
/// when building a [`MatchSnippet`].
const CONTEXT_LINES: usize = 3;

// ---------------------------------------------------------------------------
// Position
// ---------------------------------------------------------------------------

/// Byte-addressable position within a source file.
///
/// `line` is 1-based. `col` is 0-based and counts bytes from the start of
/// the line (i.e. it is a byte-column, not a Unicode-scalar-column). `byte`
/// is the 0-based byte offset from the start of the file.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::match_model::Position;
///
/// let src = "fn main() {\n    let x = 1;\n}";
/// // "    let x" starts at byte 16 (0-based), which is line 2, col 4.
/// let pos = Position::from_offset(src, 16);
/// assert_eq!(pos.line, 2);
/// assert_eq!(pos.col, 4);
/// assert_eq!(pos.byte, 16);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    /// 1-based line number.
    pub line: u32,
    /// 0-based byte-column offset from the start of the line.
    pub col: u32,
    /// 0-based byte offset from the start of the file.
    pub byte: usize,
}

impl Position {
    /// Compute a `Position` from a UTF-8 source string and a byte offset.
    ///
    /// The byte offset is clamped to `src.len()` when out of bounds so that
    /// callers do not need to guard against off-by-one ranges produced by
    /// tree-sitter or regex engines.
    ///
    /// # Arguments
    ///
    /// * `src` - Full source file content as a UTF-8 string.
    /// * `byte` - Byte offset into `src`. Clamped to `src.len()` if larger.
    ///
    /// # Returns
    ///
    /// A `Position` with `line` and `col` derived from the offset.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::match_model::Position;
    ///
    /// let pos = Position::from_offset("hello\nworld", 6);
    /// assert_eq!(pos.line, 2);
    /// assert_eq!(pos.col, 0);
    /// assert_eq!(pos.byte, 6);
    /// ```
    pub fn from_offset(src: &str, byte: usize) -> Self {
        let clamped = byte.min(src.len());
        let prefix = &src[..clamped];
        let line = prefix.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
        let col = match prefix.rfind('\n') {
            Some(last_nl) => (clamped - last_nl - 1) as u32,
            None => clamped as u32,
        };
        Self {
            line,
            col,
            byte: clamped,
        }
    }
}

impl PartialOrd for Position {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Position {
    /// Positions are ordered by their byte offset alone.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.byte.cmp(&other.byte)
    }
}

// ---------------------------------------------------------------------------
// MatchSnippet
// ---------------------------------------------------------------------------

/// Matched source text with surrounding context lines.
///
/// The `text` field contains exactly the bytes in the matched range. The
/// `context_before` and `context_after` vectors each contain up to
/// [`CONTEXT_LINES`] (3) lines surrounding the match for human-readable
/// reporting.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::match_model::MatchSnippet;
///
/// let src = "line one\nline two\nline three\n";
/// // "line two" occupies bytes 9..17.
/// let snippet = MatchSnippet::from_source(src, 9, 17);
/// assert_eq!(snippet.text, "line two");
/// assert_eq!(snippet.context_before, vec!["line one"]);
/// assert!(snippet.context_after.contains(&"line three".to_string()));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSnippet {
    /// The exact source text of the matched byte range.
    pub text: String,
    /// Lines immediately before the match, oldest first (up to 3 lines).
    pub context_before: Vec<String>,
    /// Lines immediately after the match, earliest first (up to 3 lines).
    pub context_after: Vec<String>,
}

impl MatchSnippet {
    /// Construct a `MatchSnippet` from a source string and a byte range.
    ///
    /// # Arguments
    ///
    /// * `src` - Full source file content.
    /// * `start` - Inclusive start byte offset of the match.
    /// * `end` - Exclusive end byte offset of the match.
    ///
    /// Both offsets are clamped to `src.len()` before slicing.
    ///
    /// # Returns
    ///
    /// A `MatchSnippet` with the matched text and up to 3 context lines on
    /// each side.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::match_model::MatchSnippet;
    ///
    /// let src = "a\nb\nc\nd\ne\nf\ng\n";
    /// // "d" is the 4th line; with 3 context lines we get b,c before and e,f,g after.
    /// let snippet = MatchSnippet::from_source(src, 6, 7);
    /// assert_eq!(snippet.text, "d");
    /// assert_eq!(snippet.context_before.len(), 3);
    /// assert_eq!(snippet.context_after.len(), 3);
    /// ```
    pub fn from_source(src: &str, start: usize, end: usize) -> Self {
        let clamped_start = start.min(src.len());
        let clamped_end = end.min(src.len());
        let text = src[clamped_start..clamped_end].to_string();

        let lines: Vec<&str> = src.lines().collect();

        // 0-based line index of the first matched line.
        let match_first_line = src[..clamped_start].bytes().filter(|&b| b == b'\n').count();
        // 0-based line index of the last matched line.
        let match_last_line = if clamped_end == 0 {
            0
        } else {
            src[..clamped_end].bytes().filter(|&b| b == b'\n').count()
        };

        let before_start = match_first_line.saturating_sub(CONTEXT_LINES);
        let after_end = (match_last_line + CONTEXT_LINES + 1).min(lines.len());

        let context_before: Vec<String> = lines[before_start..match_first_line]
            .iter()
            .map(|l| l.to_string())
            .collect();

        let after_start = (match_last_line + 1).min(lines.len());
        let context_after: Vec<String> = lines[after_start..after_end]
            .iter()
            .map(|l| l.to_string())
            .collect();

        Self {
            text,
            context_before,
            context_after,
        }
    }
}

// ---------------------------------------------------------------------------
// MetavarBinding
// ---------------------------------------------------------------------------

/// A metavariable binding produced by the matching engine.
///
/// Stores the source text the metavariable was bound to together with the
/// byte positions of that text within the source file.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::match_model::{MetavarBinding, Position};
///
/// let binding = MetavarBinding {
///     text: "my_value".to_string(),
///     start: Position { line: 3, col: 8, byte: 42 },
///     end:   Position { line: 3, col: 16, byte: 50 },
/// };
/// assert_eq!(binding.text, "my_value");
/// assert_eq!(binding.start.line, 3);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetavarBinding {
    /// Source text the metavariable was bound to.
    pub text: String,
    /// Position of the first byte of the binding.
    pub start: Position,
    /// Position of the first byte past the end of the binding.
    pub end: Position,
}

// ---------------------------------------------------------------------------
// SastMatch
// ---------------------------------------------------------------------------

/// A fully-populated SAST match result.
///
/// `SastMatch` is the neutral output type returned by the SAST engine for
/// every match. It carries sufficient information to produce SARIF 2.1.0 and
/// CycloneDX 1.7 output without further lookups.
///
/// # Field notes
///
/// - `path` is always repo-relative; no absolute path appears in a serialised
///   `SastMatch`.
/// - `fingerprint` is stable across scan-root relocations because it is
///   computed from the repo-relative path, `rule_id`, and the matched snippet
///   text. See [`crate::scanner::sast::fingerprint`].
/// - `fix` is recorded but **never auto-applied**; callers that want to apply
///   fixes must do so explicitly.
/// - `ruleset_id` is an empty string in Phase 5 (no ruleset loading yet).
///   Phase 6 populates this field when rules come from named rulesets.
/// - `confidence` defaults to [`Confidence::Unknown`] when the rule's
///   metadata does not specify a confidence level.
///
/// # Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use std::collections::BTreeMap;
/// use xzardgz::scanner::sast::match_model::{SastMatch, Position, MatchSnippet};
/// use xzardgz::scanner::sast::rule::metadata::{Confidence, RuleMetadata, Severity};
///
/// let m = SastMatch {
///     rule_id: "rust-md5-usage".to_string(),
///     ruleset_id: String::new(),
///     message: "Avoid MD5 for security-sensitive hashing.".to_string(),
///     severity: Severity::Warning,
///     confidence: Confidence::High,
///     path: PathBuf::from("src/main.rs"),
///     start: Position { line: 10, col: 4, byte: 200 },
///     end:   Position { line: 10, col: 24, byte: 220 },
///     snippet: MatchSnippet {
///         text: "Md5::new()".to_string(),
///         context_before: vec![],
///         context_after: vec![],
///     },
///     metavariables: BTreeMap::new(),
///     metadata: RuleMetadata::default(),
///     fingerprint: "abc123_0".to_string(),
///     fix: None,
/// };
/// assert_eq!(m.start.line, 10);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SastMatch {
    /// Namespaced rule identifier (`"<ruleset_id>::<rule_id>"` or just the
    /// rule's own id when `ruleset_id` is empty).
    pub rule_id: String,

    /// Identifier of the ruleset that supplied this rule, or an empty string
    /// when the rule was not loaded from a named ruleset.
    pub ruleset_id: String,

    /// Human-readable message emitted by the rule.
    pub message: String,

    /// Severity level of this match.
    pub severity: Severity,

    /// Confidence level of this match (`Unknown` when not specified by the rule).
    pub confidence: Confidence,

    /// Repo-relative path of the file where the match occurred.
    pub path: PathBuf,

    /// Position of the first byte of the match.
    pub start: Position,

    /// Position of the first byte past the end of the match.
    pub end: Position,

    /// Matched text with surrounding context lines.
    pub snippet: MatchSnippet,

    /// Metavariable bindings produced by this match, keyed by `$NAME`.
    pub metavariables: BTreeMap<String, MetavarBinding>,

    /// Structured metadata from the rule (CWE, OWASP, licence, etc.).
    pub metadata: RuleMetadata,

    /// Stable content-addressed fingerprint for deduplication and change
    /// tracking across scan runs.
    pub fingerprint: String,

    /// Optional autofix template from the rule.
    ///
    /// This value is **recorded only** and is never auto-applied by the
    /// engine. Callers must apply fixes explicitly after reviewing the match.
    pub fix: Option<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Position tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_position_from_offset_start_of_file_is_line_one_col_zero() {
        let pos = Position::from_offset("hello", 0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.col, 0);
        assert_eq!(pos.byte, 0);
    }

    #[test]
    fn test_position_from_offset_mid_first_line_has_correct_col() {
        let pos = Position::from_offset("hello world", 6);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.col, 6);
        assert_eq!(pos.byte, 6);
    }

    #[test]
    fn test_position_from_offset_start_of_second_line_has_col_zero() {
        let pos = Position::from_offset("hello\nworld", 6);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.col, 0);
        assert_eq!(pos.byte, 6);
    }

    #[test]
    fn test_position_from_offset_mid_second_line_has_correct_col() {
        let pos = Position::from_offset("hello\nworld", 8);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.col, 2);
        assert_eq!(pos.byte, 8);
    }

    #[test]
    fn test_position_from_offset_clamped_to_src_len() {
        let src = "abc";
        let pos = Position::from_offset(src, 999);
        assert_eq!(pos.byte, 3);
    }

    #[test]
    fn test_position_from_offset_empty_source_returns_line_one_col_zero() {
        let pos = Position::from_offset("", 0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.col, 0);
        assert_eq!(pos.byte, 0);
    }

    #[test]
    fn test_position_ord_compares_by_byte() {
        let earlier = Position {
            line: 1,
            col: 5,
            byte: 5,
        };
        let later = Position {
            line: 2,
            col: 0,
            byte: 10,
        };
        assert!(earlier < later);
    }

    #[test]
    fn test_position_equal_positions_compare_equal() {
        let p1 = Position {
            line: 3,
            col: 7,
            byte: 42,
        };
        let p2 = p1.clone();
        assert_eq!(p1, p2);
        assert_eq!(p1.cmp(&p2), std::cmp::Ordering::Equal);
    }

    // -----------------------------------------------------------------------
    // MatchSnippet tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_match_snippet_from_source_text_is_correct_slice() {
        let src = "line one\nline two\nline three\n";
        // "line two" is at bytes 9..17.
        let snippet = MatchSnippet::from_source(src, 9, 17);
        assert_eq!(snippet.text, "line two");
    }

    #[test]
    fn test_match_snippet_context_before_contains_preceding_lines() {
        let src = "line one\nline two\nline three\n";
        let snippet = MatchSnippet::from_source(src, 9, 17);
        assert!(snippet.context_before.contains(&"line one".to_string()));
    }

    #[test]
    fn test_match_snippet_context_after_contains_following_lines() {
        let src = "line one\nline two\nline three\n";
        let snippet = MatchSnippet::from_source(src, 9, 17);
        assert!(snippet.context_after.contains(&"line three".to_string()));
    }

    #[test]
    fn test_match_snippet_context_capped_at_three_lines() {
        let src = "a\nb\nc\nd\ne\nf\ng\n";
        // "d" is line 4 (bytes 6..7).
        let snippet = MatchSnippet::from_source(src, 6, 7);
        assert!(snippet.context_before.len() <= 3);
        assert!(snippet.context_after.len() <= 3);
    }

    #[test]
    fn test_match_snippet_first_line_has_no_context_before() {
        let src = "first\nsecond\n";
        let snippet = MatchSnippet::from_source(src, 0, 5);
        assert!(snippet.context_before.is_empty());
    }

    #[test]
    fn test_match_snippet_last_line_has_no_context_after() {
        let src = "first\nsecond";
        let snippet = MatchSnippet::from_source(src, 6, 12);
        assert!(snippet.context_after.is_empty());
    }

    #[test]
    fn test_match_snippet_empty_range_produces_empty_text() {
        let src = "hello";
        let snippet = MatchSnippet::from_source(src, 2, 2);
        assert_eq!(snippet.text, "");
    }

    // -----------------------------------------------------------------------
    // MetavarBinding tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_metavar_binding_fields_accessible() {
        let binding = MetavarBinding {
            text: "my_val".to_string(),
            start: Position {
                line: 1,
                col: 0,
                byte: 0,
            },
            end: Position {
                line: 1,
                col: 6,
                byte: 6,
            },
        };
        assert_eq!(binding.text, "my_val");
        assert_eq!(binding.start.byte, 0);
        assert_eq!(binding.end.byte, 6);
    }

    #[test]
    fn test_metavar_binding_clone_is_equal() {
        let b = MetavarBinding {
            text: "x".to_string(),
            start: Position {
                line: 1,
                col: 0,
                byte: 0,
            },
            end: Position {
                line: 1,
                col: 1,
                byte: 1,
            },
        };
        assert_eq!(b.clone(), b);
    }

    // -----------------------------------------------------------------------
    // SastMatch tests
    // -----------------------------------------------------------------------

    fn make_test_match() -> SastMatch {
        SastMatch {
            rule_id: "test-rule".to_string(),
            ruleset_id: String::new(),
            message: "Test message.".to_string(),
            severity: Severity::Warning,
            confidence: Confidence::High,
            path: PathBuf::from("src/main.rs"),
            start: Position {
                line: 1,
                col: 0,
                byte: 0,
            },
            end: Position {
                line: 1,
                col: 10,
                byte: 10,
            },
            snippet: MatchSnippet {
                text: "let x = 1;".to_string(),
                context_before: vec![],
                context_after: vec![],
            },
            metavariables: BTreeMap::new(),
            metadata: RuleMetadata::default(),
            fingerprint: "aabbcc_0".to_string(),
            fix: None,
        }
    }

    #[test]
    fn test_sast_match_fields_are_accessible() {
        let m = make_test_match();
        assert_eq!(m.rule_id, "test-rule");
        assert_eq!(m.severity, Severity::Warning);
        assert_eq!(m.start.line, 1);
        assert_eq!(m.fingerprint, "aabbcc_0");
        assert!(m.fix.is_none());
    }

    #[test]
    fn test_sast_match_with_fix_records_fix_text() {
        let mut m = make_test_match();
        m.fix = Some("RsaPrivateKey::new(&mut rng, 2048)".to_string());
        assert_eq!(m.fix.as_deref(), Some("RsaPrivateKey::new(&mut rng, 2048)"));
    }

    #[test]
    fn test_sast_match_clone_is_equal() {
        let m = make_test_match();
        assert_eq!(m.clone(), m);
    }

    #[test]
    fn test_sast_match_serialization_roundtrip_preserves_fields() {
        let m = make_test_match();
        let json = serde_json::to_string(&m).expect("serialization must succeed");
        let restored: SastMatch =
            serde_json::from_str(&json).expect("deserialization must succeed");
        assert_eq!(m, restored);
    }

    #[test]
    fn test_sast_match_empty_ruleset_id_means_unnamespaced_rule_id() {
        let m = make_test_match();
        assert!(m.ruleset_id.is_empty());
        assert_eq!(m.rule_id, "test-rule");
    }
}
