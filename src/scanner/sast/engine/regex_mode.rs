//! Regex-mode scanner for rules with `languages: [regex]` or `languages: [generic]`.
//!
//! Applies `pattern-regex` directly over raw file bytes without any AST parsing.
//! Named capture groups in the regex are bound as metavariables (`$NAME`).
//!
//! All regexes are compiled with 10 MiB size limits on both the NFA and DFA
//! to bound ReDoS risk.

use regex::RegexBuilder;
use std::collections::BTreeMap;

use crate::scanner::sast::error::SastError;
use crate::scanner::sast::rule::ir::{Formula, Leaf, RuleIr};

/// NFA and DFA cache size limit applied to every compiled regex (10 MiB).
const REGEX_SIZE_LIMIT: usize = 10_485_760;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single regex-mode match result.
///
/// Byte offsets and line numbers refer to the raw file content as supplied to
/// [`RegexModeScanner::scan_bytes`]. Line numbers are 1-based.
#[derive(Debug, Clone, PartialEq)]
pub struct RegexMatch {
    /// Byte offset of the start of the match in the file.
    pub byte_start: usize,
    /// Byte offset of the end of the match in the file (exclusive).
    pub byte_end: usize,
    /// 1-based line number of the match start.
    pub line_start: usize,
    /// 1-based line number of the match end.
    pub line_end: usize,
    /// The matched text snippet.
    pub snippet: String,
    /// Named capture groups bound as `$NAME` metavariables (NAME is uppercased).
    pub metavariables: BTreeMap<String, String>,
}

/// Regex-mode scanner for a single SAST rule.
///
/// Constructed from a [`RuleIr`] whose formula contains a [`Leaf::Regex`]
/// node. The compiled regex is reused across multiple calls to
/// [`RegexModeScanner::scan_bytes`].
pub struct RegexModeScanner {
    rule_id: String,
    compiled: regex::Regex,
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Walk a [`Formula`] tree and return the first `pattern-regex` string found.
///
/// Search order:
/// - [`Formula::Leaf`]: returns the pattern if it is a [`Leaf::Regex`].
/// - [`Formula::And`]: searches `conjuncts` recursively (depth-first).
/// - [`Formula::Or`]: searches only the first alternative.
/// - [`Formula::Inside`]: recurses into the inner formula.
/// - [`Leaf::Pattern`]: not a regex leaf; returns `None`.
fn extract_regex_pattern(formula: &Formula) -> Option<&str> {
    match formula {
        Formula::Leaf(Leaf::Regex(p)) => Some(p.as_str()),
        Formula::Leaf(Leaf::Pattern(_)) => None,
        Formula::And { conjuncts, .. } => conjuncts.iter().find_map(|f| extract_regex_pattern(f)),
        Formula::Or(alts) => alts.first().and_then(|f| extract_regex_pattern(f)),
        Formula::Inside(inner) => extract_regex_pattern(inner),
    }
}

/// Return the 1-based line number for a byte offset given a table of
/// line-start byte positions.
///
/// `line_starts` must be sorted in ascending order with `line_starts[0] == 0`.
fn byte_to_line(line_starts: &[usize], byte_offset: usize) -> usize {
    // partition_point returns the number of elements satisfying the predicate,
    // which equals the 1-based line number.
    line_starts.partition_point(|&s| s <= byte_offset)
}

// ---------------------------------------------------------------------------
// RegexModeScanner impl
// ---------------------------------------------------------------------------

impl RegexModeScanner {
    /// Compile a regex-mode scanner from a [`RuleIr`].
    ///
    /// Walks the rule's formula tree to find the first [`Leaf::Regex`] node
    /// and compiles it with 10 MiB NFA and DFA size limits.
    ///
    /// # Errors
    ///
    /// Returns [`SastError::Internal`] if the formula contains no
    /// `pattern-regex` leaf.
    ///
    /// Returns [`SastError::RegexCompile`] if the regex string is syntactically
    /// invalid or would exceed the 10 MiB compiled-size limit.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::engine::RegexModeScanner;
    /// use xzardgz::scanner::sast::rule::ir::{Formula, Leaf, RuleIr};
    /// use xzardgz::scanner::sast::rule::metadata::Severity;
    ///
    /// let rule = RuleIr {
    ///     id: "example".to_string(),
    ///     message: "found pattern".to_string(),
    ///     languages: vec!["regex".to_string()],
    ///     severity: Severity::Warning,
    ///     metadata: None,
    ///     formula: Formula::Leaf(Leaf::Regex(r"secret\s*=\s*\S+".to_string())),
    ///     fix: None,
    /// };
    ///
    /// let scanner = RegexModeScanner::new(&rule).unwrap();
    /// assert_eq!(scanner.rule_id(), "example");
    /// ```
    pub fn new(rule: &RuleIr) -> Result<Self, SastError> {
        let pattern = extract_regex_pattern(&rule.formula)
            .ok_or_else(|| SastError::Internal("no pattern-regex leaf in formula".to_string()))?;

        let compiled = RegexBuilder::new(pattern)
            .size_limit(REGEX_SIZE_LIMIT)
            .dfa_size_limit(REGEX_SIZE_LIMIT)
            .build()
            .map_err(|e| SastError::RegexCompile {
                rule_id: rule.id.clone(),
                cause: e.to_string(),
            })?;

        Ok(Self {
            rule_id: rule.id.clone(),
            compiled,
        })
    }

    /// Scan raw file content for matches.
    ///
    /// `content` is treated as raw bytes. Invalid UTF-8 sequences are replaced
    /// with the Unicode replacement character (U+FFFD) via
    /// [`String::from_utf8_lossy`] before regex matching; callers should
    /// prefer valid UTF-8 input for accurate byte-offset reporting.
    ///
    /// At most `max_matches` results are returned. Pass `usize::MAX` to
    /// collect all matches.
    ///
    /// Named capture groups in the compiled regex are exposed as
    /// `$NAME` metavariables (the group name is uppercased).
    ///
    /// # Errors
    ///
    /// Currently infallible; the `Result` return type is reserved for future
    /// timeout or resource-limit enforcement.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::engine::RegexModeScanner;
    /// use xzardgz::scanner::sast::rule::ir::{Formula, Leaf, RuleIr};
    /// use xzardgz::scanner::sast::rule::metadata::Severity;
    ///
    /// let rule = RuleIr {
    ///     id: "tok".to_string(),
    ///     message: "token".to_string(),
    ///     languages: vec!["regex".to_string()],
    ///     severity: Severity::Info,
    ///     metadata: None,
    ///     formula: Formula::Leaf(Leaf::Regex(r"(?P<key>\w+)=(?P<val>\d+)".to_string())),
    ///     fix: None,
    /// };
    ///
    /// let scanner = RegexModeScanner::new(&rule).unwrap();
    /// let matches = scanner.scan_bytes(b"x=1 y=2", 10).unwrap();
    /// assert_eq!(matches.len(), 2);
    /// assert_eq!(matches[0].metavariables["$KEY"], "x");
    /// ```
    pub fn scan_bytes(
        &self,
        content: &[u8],
        max_matches: usize,
    ) -> Result<Vec<RegexMatch>, SastError> {
        let text = String::from_utf8_lossy(content);

        // Build a table of byte offsets at which each line starts so that
        // byte offsets can be converted to 1-based line numbers in O(log n).
        let mut line_starts: Vec<usize> = vec![0];
        for (i, &b) in content.iter().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }

        let mut results = Vec::new();
        for caps in self.compiled.captures_iter(text.as_ref()).take(max_matches) {
            // Group 0 (the full match) is always present when captures_iter
            // yields a value; unwrap is safe here.
            let full = caps.get(0).unwrap(); // SAFETY: group 0 always present
            let byte_start = full.start();
            let byte_end = full.end();
            let snippet = text[byte_start..byte_end].to_string();

            let line_start = byte_to_line(&line_starts, byte_start);
            // saturating_sub(1) avoids an off-by-one when byte_end falls
            // exactly on a newline or when the match is zero-length.
            let line_end = byte_to_line(&line_starts, byte_end.saturating_sub(1));

            // Collect named capture groups as $NAME metavariables.
            let mut metavariables = BTreeMap::new();
            for name in self.compiled.capture_names().flatten() {
                if let Some(m) = caps.name(name) {
                    metavariables.insert(
                        format!("${}", name.to_ascii_uppercase()),
                        m.as_str().to_string(),
                    );
                }
            }

            results.push(RegexMatch {
                byte_start,
                byte_end,
                line_start,
                line_end,
                snippet,
                metavariables,
            });
        }

        Ok(results)
    }

    /// Returns the rule id this scanner was built for.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::engine::RegexModeScanner;
    /// use xzardgz::scanner::sast::rule::ir::{Formula, Leaf, RuleIr};
    /// use xzardgz::scanner::sast::rule::metadata::Severity;
    ///
    /// let rule = RuleIr {
    ///     id: "my-rule".to_string(),
    ///     message: "msg".to_string(),
    ///     languages: vec!["regex".to_string()],
    ///     severity: Severity::Info,
    ///     metadata: None,
    ///     formula: Formula::Leaf(Leaf::Regex(r"\d+".to_string())),
    ///     fix: None,
    /// };
    /// let scanner = RegexModeScanner::new(&rule).unwrap();
    /// assert_eq!(scanner.rule_id(), "my-rule");
    /// ```
    pub fn rule_id(&self) -> &str {
        &self.rule_id
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::sast::rule::ir::{Formula, Leaf, RuleIr};
    use crate::scanner::sast::rule::metadata::Severity;

    // ------------------------------------------------------------------
    // Test helpers
    // ------------------------------------------------------------------

    fn make_regex_rule(pattern: &str) -> RuleIr {
        RuleIr {
            id: "test-rule".to_string(),
            message: "test".to_string(),
            languages: vec!["regex".to_string()],
            severity: Severity::Warning,
            metadata: None,
            formula: Formula::Leaf(Leaf::Regex(pattern.to_string())),
            fix: None,
        }
    }

    fn make_pattern_rule(pattern: &str) -> RuleIr {
        RuleIr {
            id: "test-pattern-rule".to_string(),
            message: "test".to_string(),
            languages: vec!["rust".to_string()],
            severity: Severity::Warning,
            metadata: None,
            formula: Formula::Leaf(Leaf::Pattern(pattern.to_string())),
            fix: None,
        }
    }

    // ------------------------------------------------------------------
    // RegexModeScanner::new
    // ------------------------------------------------------------------

    #[test]
    fn test_regex_mode_scanner_new_with_valid_regex_succeeds() {
        let rule = make_regex_rule(r"\bsecret\s*=\s*\S+");
        let result = RegexModeScanner::new(&rule);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().rule_id(), "test-rule");
    }

    #[test]
    fn test_regex_mode_scanner_new_no_regex_leaf_returns_error() {
        // A rule with only a Pattern leaf has no Leaf::Regex; new() must fail.
        let rule = make_pattern_rule("$X.unwrap()");
        let result = RegexModeScanner::new(&rule);
        assert!(result.is_err());
        assert!(matches!(result, Err(SastError::Internal(_))));
    }

    #[test]
    fn test_regex_mode_scanner_invalid_regex_returns_error() {
        // An unclosed group is a compile-time syntax error.
        let rule = make_regex_rule(r"(unclosed");
        let result = RegexModeScanner::new(&rule);
        assert!(result.is_err());
        assert!(matches!(result, Err(SastError::RegexCompile { .. })));
    }

    #[test]
    fn test_regex_mode_scanner_oversized_regex_returns_error() {
        // A 2 million character literal forces the Thompson NFA to allocate
        // approximately 2 million consecutive ByteRange states with no prefix
        // sharing. At any reasonable NFA state size this far exceeds the
        // 10 MiB (10_485_760 byte) size limit set by REGEX_SIZE_LIMIT.
        let pattern = "a".repeat(2_000_000);
        let rule = make_regex_rule(&pattern);
        let result = RegexModeScanner::new(&rule);
        assert!(
            result.is_err(),
            "expected RegexCompile error for oversized pattern"
        );
        assert!(matches!(result, Err(SastError::RegexCompile { .. })));
    }

    // ------------------------------------------------------------------
    // RegexModeScanner::scan_bytes -- basic matching
    // ------------------------------------------------------------------

    #[test]
    fn test_scan_bytes_finds_simple_match() {
        let rule = make_regex_rule(r"secret");
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let content = b"password=x\nsecret=abc\nend";
        let matches = scanner.scan_bytes(content, 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].snippet, "secret");
    }

    #[test]
    fn test_scan_bytes_no_match_returns_empty() {
        let rule = make_regex_rule(r"NOTPRESENT");
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let matches = scanner.scan_bytes(b"nothing here", 10).unwrap();
        assert!(matches.is_empty());
    }

    // ------------------------------------------------------------------
    // Named capture groups -> metavariables
    // ------------------------------------------------------------------

    #[test]
    fn test_scan_bytes_named_capture_group_becomes_metavar() {
        let rule = make_regex_rule(r"(?P<token>[a-z]+)=(?P<value>[0-9]+)");
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let content = b"key=42";
        let matches = scanner.scan_bytes(content, 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].metavariables.get("$TOKEN"),
            Some(&"key".to_string())
        );
        assert_eq!(
            matches[0].metavariables.get("$VALUE"),
            Some(&"42".to_string())
        );
    }

    #[test]
    fn test_scan_bytes_multiple_named_groups_all_bound() {
        let rule = make_regex_rule(r"(?P<user>\w+):(?P<pass>\w+)@(?P<host>\w+)");
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let matches = scanner.scan_bytes(b"admin:hunter2@db", 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].metavariables.get("$USER"),
            Some(&"admin".to_string())
        );
        assert_eq!(
            matches[0].metavariables.get("$PASS"),
            Some(&"hunter2".to_string())
        );
        assert_eq!(
            matches[0].metavariables.get("$HOST"),
            Some(&"db".to_string())
        );
    }

    #[test]
    fn test_scan_bytes_group_name_is_uppercased_in_metavar_key() {
        let rule = make_regex_rule(r"(?P<mixedCase>\w+)");
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let matches = scanner.scan_bytes(b"hello", 10).unwrap();
        assert_eq!(matches.len(), 1);
        // The key must be $MIXEDCASE regardless of the group name casing.
        assert!(matches[0].metavariables.contains_key("$MIXEDCASE"));
    }

    // ------------------------------------------------------------------
    // max_matches limit
    // ------------------------------------------------------------------

    #[test]
    fn test_scan_bytes_respects_max_matches_limit() {
        let rule = make_regex_rule(r"x");
        let scanner = RegexModeScanner::new(&rule).unwrap();
        // Content has 8 'x' occurrences but limit is 3.
        let content = b"x x x x x x x x";
        let matches = scanner.scan_bytes(content, 3).unwrap();
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_scan_bytes_max_matches_zero_returns_empty() {
        let rule = make_regex_rule(r"x");
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let matches = scanner.scan_bytes(b"x x x", 0).unwrap();
        assert!(matches.is_empty());
    }

    // ------------------------------------------------------------------
    // Invalid UTF-8
    // ------------------------------------------------------------------

    #[test]
    fn test_scan_bytes_invalid_utf8_returns_empty() {
        // Bytes 0xFF 0xFE 0xFD are never valid UTF-8.
        // from_utf8_lossy replaces them with U+FFFD; the pattern "hello"
        // is not present in the replacement output, so no matches are found.
        let rule = make_regex_rule(r"hello");
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let content: &[u8] = &[0xFF, 0xFE, 0xFD];
        let matches = scanner.scan_bytes(content, 10).unwrap();
        assert!(matches.is_empty());
    }

    // ------------------------------------------------------------------
    // Line number calculation
    // ------------------------------------------------------------------

    #[test]
    fn test_scan_bytes_line_numbers_are_correct() {
        // "line one\n"   -> bytes  0.. 9, line 1
        // "pattern here" -> bytes  9..21, line 2
        // "\nline three" -> bytes 21..32, line 3
        let rule = make_regex_rule(r"pattern");
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let content = b"line one\npattern here\nline three\n";
        let matches = scanner.scan_bytes(content, 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_start, 2, "match should start on line 2");
        assert_eq!(matches[0].line_end, 2, "match should end on line 2");
    }

    #[test]
    fn test_scan_bytes_match_spanning_two_lines_has_correct_line_end() {
        // "foo\nbar" spans lines 1 and 2.
        let rule = make_regex_rule(r"foo\nbar");
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let content = b"foo\nbar baz";
        let matches = scanner.scan_bytes(content, 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_start, 1);
        assert_eq!(matches[0].line_end, 2);
    }

    #[test]
    fn test_scan_bytes_first_line_match_has_line_number_one() {
        let rule = make_regex_rule(r"start");
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let content = b"start of file\nsecond line";
        let matches = scanner.scan_bytes(content, 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_start, 1);
        assert_eq!(matches[0].line_end, 1);
    }

    // ------------------------------------------------------------------
    // Byte offset fields
    // ------------------------------------------------------------------

    #[test]
    fn test_scan_bytes_byte_offsets_are_correct() {
        let rule = make_regex_rule(r"target");
        let scanner = RegexModeScanner::new(&rule).unwrap();
        // "prefix_target_suffix" -> "target" starts at byte 7
        let content = b"prefix_target_suffix";
        let matches = scanner.scan_bytes(content, 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].byte_start, 7);
        assert_eq!(matches[0].byte_end, 13);
        assert_eq!(&content[7..13], b"target");
    }

    // ------------------------------------------------------------------
    // Snippet content
    // ------------------------------------------------------------------

    #[test]
    fn test_scan_bytes_snippet_equals_matched_text() {
        let rule = make_regex_rule(r"\d+\.\d+");
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let matches = scanner.scan_bytes(b"version 3.14 released", 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].snippet, "3.14");
    }

    // ------------------------------------------------------------------
    // Formula tree traversal
    // ------------------------------------------------------------------

    #[test]
    fn test_regex_mode_scanner_new_extracts_regex_from_and_conjunct() {
        // The Regex leaf is inside the And's conjuncts, not at the top level.
        let rule = RuleIr {
            id: "and-rule".to_string(),
            message: "test".to_string(),
            languages: vec!["regex".to_string()],
            severity: Severity::Warning,
            metadata: None,
            formula: Formula::And {
                conjuncts: vec![Formula::Leaf(Leaf::Regex(r"needle".to_string()))],
                negations: vec![],
                conditions: vec![],
                focus: vec![],
            },
            fix: None,
        };
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let matches = scanner.scan_bytes(b"find the needle here", 10).unwrap();
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_regex_mode_scanner_new_extracts_regex_from_or_first_element() {
        // Only the first alternative of an Or is searched for a Regex leaf.
        let rule = RuleIr {
            id: "or-rule".to_string(),
            message: "test".to_string(),
            languages: vec!["regex".to_string()],
            severity: Severity::Warning,
            metadata: None,
            formula: Formula::Or(vec![
                Formula::Leaf(Leaf::Regex(r"first".to_string())),
                Formula::Leaf(Leaf::Regex(r"second".to_string())),
            ]),
            fix: None,
        };
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let matches = scanner.scan_bytes(b"first or second", 10).unwrap();
        // Only the regex from the first alternative is used.
        assert!(matches.iter().any(|m| m.snippet == "first"));
    }

    #[test]
    fn test_regex_mode_scanner_new_extracts_regex_from_inside() {
        let rule = RuleIr {
            id: "inside-rule".to_string(),
            message: "test".to_string(),
            languages: vec!["regex".to_string()],
            severity: Severity::Warning,
            metadata: None,
            formula: Formula::Inside(Box::new(Formula::Leaf(Leaf::Regex(r"inside".to_string())))),
            fix: None,
        };
        let scanner = RegexModeScanner::new(&rule).unwrap();
        let matches = scanner.scan_bytes(b"look inside here", 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].snippet, "inside");
    }

    // ------------------------------------------------------------------
    // Helper: byte_to_line
    // ------------------------------------------------------------------

    #[test]
    fn test_byte_to_line_offset_zero_returns_line_one() {
        let line_starts = vec![0usize, 10, 20];
        assert_eq!(byte_to_line(&line_starts, 0), 1);
    }

    #[test]
    fn test_byte_to_line_mid_first_line_returns_line_one() {
        let line_starts = vec![0usize, 10, 20];
        assert_eq!(byte_to_line(&line_starts, 5), 1);
    }

    #[test]
    fn test_byte_to_line_start_of_second_line_returns_line_two() {
        let line_starts = vec![0usize, 10, 20];
        assert_eq!(byte_to_line(&line_starts, 10), 2);
    }

    #[test]
    fn test_byte_to_line_past_last_known_line_start_returns_last_line() {
        let line_starts = vec![0usize, 10, 20];
        assert_eq!(byte_to_line(&line_starts, 25), 3);
    }
}
