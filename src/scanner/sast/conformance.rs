//! Conformance testing harness for the SAST engine.
//!
//! This module provides types and functions for running structured conformance
//! checks against SAST rules. Each rule is evaluated against a fixture file
//! annotated with `// ruleid:`, `// ok:`, and `// todoruleid:` markers that
//! declare the expected scan results on specific lines.
//!
//! # Annotation format
//!
//! Annotations are line comments placed either directly above or inline with
//! the code they annotate.
//!
//! | Kind       | Syntax                     | Meaning                                            |
//! | ---------- | -------------------------- | -------------------------------------------------- |
//! | Expected   | `// ruleid: <rule-id>`     | The next (above) or same (inline) line must match  |
//! | Safe       | `// ok: <reason>`          | The next or same line must NOT match any rule      |
//! | Deferred   | `// todoruleid: <rule-id>` | Match expected but not yet enforced                |
//!
//! Conformance is evaluated per rule. The harness filters annotations by rule
//! id and reports missed matches, false positives, or a clean pass.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::scanner::sast::engine::RegexModeScanner;
use crate::scanner::sast::rule::ir::RuleIr;

// ---------------------------------------------------------------------------
// ConformanceError
// ---------------------------------------------------------------------------

/// Errors produced by the conformance harness.
#[derive(Error, Debug)]
pub enum ConformanceError {
    /// An I/O error occurred while reading a file.
    #[error("I/O error: {0}")]
    Io(String),

    /// A JSON parse error occurred while loading a baseline.
    #[error("JSON parse error: {0}")]
    Json(String),

    /// An error occurred during in-memory scanning.
    #[error("scan error: {0}")]
    Scan(String),

    /// The rule's declared languages are not supported by the in-memory scanner.
    #[error("unsupported language(s): {0:?}")]
    UnsupportedLanguage(Vec<String>),
}

// ---------------------------------------------------------------------------
// AnnotationKind
// ---------------------------------------------------------------------------

/// The semantic kind of a conformance annotation found in a fixture file.
///
/// Annotations are extracted from comments in the form `// ruleid: <id>`,
/// `// ok: <reason>`, or `// todoruleid: <id>`.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationKind {
    /// The annotated line is expected to be matched by the named rule.
    RuleId(String),
    /// The annotated line is expected NOT to be matched by any rule.
    Ok,
    /// A deferred match expectation. Not enforced; treated as absent for
    /// the purposes of pass/fail checking.
    TodoRuleId(String),
}

// ---------------------------------------------------------------------------
// Annotation
// ---------------------------------------------------------------------------

/// A single conformance annotation extracted from a fixture file.
///
/// Annotations appear as `// ruleid:`, `// ok:`, or `// todoruleid:` comments.
/// An annotation placed on its own line (above-style) refers to the following
/// line; an annotation that appears inline after code refers to the same line.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// The 1-based line number on which the annotation comment appears.
    pub annotation_line: u32,
    /// The 1-based line number to which this annotation applies.
    ///
    /// For above-style annotations (the comment is the only content on the
    /// line) this equals `annotation_line + 1`. For inline annotations it
    /// equals `annotation_line`.
    pub target_line: u32,
    /// The semantic kind of this annotation.
    pub kind: AnnotationKind,
}

// ---------------------------------------------------------------------------
// RuleConformanceResult
// ---------------------------------------------------------------------------

/// The outcome of evaluating a single rule against a conformance fixture.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleConformanceResult {
    /// All expected matches were found and no unexpected matches occurred.
    Pass,
    /// One or more expected matches were absent or unexpected matches appeared.
    Fail {
        /// Lines expected to match (via `ruleid:` annotation) but not matched.
        missed_lines: Vec<u32>,
        /// Lines expected NOT to match (via `ok:` annotation) but matched.
        false_positive_lines: Vec<u32>,
    },
    /// The fixture had no annotations applicable to this rule; check skipped.
    Skipped(String),
}

impl RuleConformanceResult {
    /// Returns `true` when the rule passed all conformance checks.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::conformance::RuleConformanceResult;
    ///
    /// assert!(RuleConformanceResult::Pass.is_pass());
    /// assert!(!RuleConformanceResult::Fail {
    ///     missed_lines: vec![1],
    ///     false_positive_lines: vec![],
    /// }
    /// .is_pass());
    /// ```
    #[must_use]
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }

    /// Returns `true` when the rule failed at least one conformance check.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::conformance::RuleConformanceResult;
    ///
    /// assert!(RuleConformanceResult::Fail {
    ///     missed_lines: vec![2],
    ///     false_positive_lines: vec![],
    /// }
    /// .is_fail());
    /// assert!(!RuleConformanceResult::Pass.is_fail());
    /// ```
    #[must_use]
    pub fn is_fail(&self) -> bool {
        matches!(self, Self::Fail { .. })
    }

    /// Returns `true` when the fixture had no applicable annotations for this rule.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::conformance::RuleConformanceResult;
    ///
    /// assert!(RuleConformanceResult::Skipped("no fixture".to_string()).is_skipped());
    /// assert!(!RuleConformanceResult::Pass.is_skipped());
    /// ```
    #[must_use]
    pub fn is_skipped(&self) -> bool {
        matches!(self, Self::Skipped(_))
    }

    /// Returns `true` when this result represents a regression from a prior passing run.
    ///
    /// A result is a regression from pass when the rule is currently failing.
    /// This method is used by [`ConformanceBaseline::detect_regressions`] to
    /// identify rules that were previously `"pass"` in the baseline but have
    /// since degraded to [`RuleConformanceResult::Fail`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::conformance::RuleConformanceResult;
    ///
    /// let fail = RuleConformanceResult::Fail {
    ///     missed_lines: vec![3],
    ///     false_positive_lines: vec![],
    /// };
    /// assert!(fail.is_regression_from_pass());
    /// assert!(!RuleConformanceResult::Pass.is_regression_from_pass());
    /// assert!(!RuleConformanceResult::Skipped("n/a".to_string()).is_regression_from_pass());
    /// ```
    #[must_use]
    pub fn is_regression_from_pass(&self) -> bool {
        self.is_fail()
    }
}

// ---------------------------------------------------------------------------
// ConformanceBaseline
// ---------------------------------------------------------------------------

/// A snapshot of conformance results used to detect regressions across runs.
///
/// The baseline is a JSON document with the following structure:
///
/// ```json
/// {
///   "version": "1",
///   "generated_at": "2026-11-25",
///   "entries": {
///     "rust-md5-usage": "pass",
///     "rust-sha1-usage": "pass",
///     "rust-unsafe-ffi-call": "skipped(no fixture)"
///   }
/// }
/// ```
///
/// Entry values are freeform strings, but the regression detector only acts
/// on entries whose value is exactly `"pass"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceBaseline {
    /// Schema version of the baseline format (currently `"1"`).
    pub version: String,
    /// ISO-8601 date string of when the baseline was generated.
    pub generated_at: String,
    /// Map of rule id to its recorded conformance status string.
    pub entries: BTreeMap<String, String>,
}

impl ConformanceBaseline {
    /// Load a [`ConformanceBaseline`] from a JSON file on disk.
    ///
    /// # Arguments
    ///
    /// * `path` - Filesystem path to the JSON baseline file.
    ///
    /// # Errors
    ///
    /// Returns [`ConformanceError::Io`] if the file cannot be read.
    /// Returns [`ConformanceError::Json`] if the contents are not valid JSON
    /// conforming to the baseline schema.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use xzardgz::scanner::sast::conformance::ConformanceBaseline;
    ///
    /// let baseline = ConformanceBaseline::from_path(
    ///     Path::new("testdata/sast/conformance_baseline.json"),
    /// )
    /// .expect("baseline must be readable");
    /// assert_eq!(baseline.version, "1");
    /// ```
    pub fn from_path(path: &Path) -> Result<Self, ConformanceError> {
        let contents =
            std::fs::read_to_string(path).map_err(|e| ConformanceError::Io(e.to_string()))?;
        Self::from_json_str(&contents)
    }

    /// Parse a [`ConformanceBaseline`] from a JSON string.
    ///
    /// # Arguments
    ///
    /// * `s` - JSON string with the baseline format.
    ///
    /// # Errors
    ///
    /// Returns [`ConformanceError::Json`] if `s` is not valid JSON or does not
    /// conform to the expected schema.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::conformance::ConformanceBaseline;
    ///
    /// let json = r#"{"version":"1","generated_at":"2026-11-25","entries":{"my-rule":"pass"}}"#;
    /// let baseline = ConformanceBaseline::from_json_str(json).unwrap();
    /// assert_eq!(baseline.version, "1");
    /// assert_eq!(baseline.entries.get("my-rule"), Some(&"pass".to_string()));
    /// ```
    pub fn from_json_str(s: &str) -> Result<Self, ConformanceError> {
        serde_json::from_str(s).map_err(|e| ConformanceError::Json(e.to_string()))
    }

    /// Compare current results against the baseline and return any regressions.
    ///
    /// A regression is a rule whose baseline entry is exactly `"pass"` but
    /// whose current result is [`RuleConformanceResult::Fail`]. Rules not
    /// present in the baseline, or whose baseline entry is not `"pass"`, are
    /// never flagged as regressions.
    ///
    /// # Arguments
    ///
    /// * `current` - Slice of `(rule_id, result)` pairs from the current run.
    ///
    /// # Returns
    ///
    /// A list of rule ids that regressed from `"pass"` to `Fail`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::conformance::{ConformanceBaseline, RuleConformanceResult};
    ///
    /// let json = r#"{"version":"1","generated_at":"2026-11-25","entries":{"my-rule":"pass"}}"#;
    /// let baseline = ConformanceBaseline::from_json_str(json).unwrap();
    /// let current = vec![(
    ///     "my-rule".to_string(),
    ///     RuleConformanceResult::Fail {
    ///         missed_lines: vec![2],
    ///         false_positive_lines: vec![],
    ///     },
    /// )];
    /// let regressions = baseline.detect_regressions(&current);
    /// assert!(regressions.contains(&"my-rule".to_string()));
    /// ```
    pub fn detect_regressions(&self, current: &[(String, RuleConformanceResult)]) -> Vec<String> {
        current
            .iter()
            .filter_map(|(rule_id, result)| {
                let baseline_entry = self.entries.get(rule_id.as_str())?;
                if baseline_entry == "pass" && result.is_regression_from_pass() {
                    Some(rule_id.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// parse_annotations
// ---------------------------------------------------------------------------

/// Parse conformance annotations from a fixture source string.
///
/// Scans `src` line-by-line for comment markers (`//` or `#`) followed by one
/// of the recognised annotation prefixes (`ruleid:`, `ok:`, `todoruleid:`).
///
/// The annotation `target_line` is determined by placement:
///
/// - **Above annotation**: the comment is the only content on the line.
///   `target_line` is `annotation_line + 1`.
/// - **Inline annotation**: code precedes the comment on the same line.
///   `target_line` is `annotation_line`.
///
/// # Arguments
///
/// * `src` - Source text of the fixture file.
///
/// # Returns
///
/// Annotations in source order. Returns an empty vec when no annotations are
/// present.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::conformance::{parse_annotations, AnnotationKind};
///
/// let src = "// ruleid: my-rule\nlet x = danger();\n";
/// let annotations = parse_annotations(src);
/// assert_eq!(annotations.len(), 1);
/// assert_eq!(annotations[0].target_line, 2);
/// assert!(
///     matches!(&annotations[0].kind, AnnotationKind::RuleId(id) if id == "my-rule")
/// );
/// ```
pub fn parse_annotations(src: &str) -> Vec<Annotation> {
    let mut annotations = Vec::new();

    for (idx, line) in src.lines().enumerate() {
        let line_number = (idx as u32) + 1;
        let trimmed = line.trim();

        // A pure-comment line has only a comment marker (and optional whitespace)
        // before the annotation text. Inline comments have code before the marker.
        let is_pure_comment = trimmed.starts_with("//") || trimmed.starts_with('#');

        // Extract the text that follows the first comment marker.
        let comment_content = if let Some(pos) = line.find("//") {
            line[pos + 2..].trim()
        } else if let Some(pos) = line.find('#') {
            line[pos + 1..].trim()
        } else {
            continue;
        };

        // Above-style: target is the following line.
        // Inline-style: target is the same line.
        let (annotation_line, target_line) = if is_pure_comment {
            (line_number, line_number + 1)
        } else {
            (line_number, line_number)
        };

        let kind = if let Some(rest) = comment_content.strip_prefix("ruleid:") {
            AnnotationKind::RuleId(rest.trim().to_string())
        } else if comment_content.starts_with("ok:") {
            AnnotationKind::Ok
        } else if let Some(rest) = comment_content.strip_prefix("todoruleid:") {
            AnnotationKind::TodoRuleId(rest.trim().to_string())
        } else {
            continue;
        };

        annotations.push(Annotation {
            annotation_line,
            target_line,
            kind,
        });
    }

    annotations
}

// ---------------------------------------------------------------------------
// scan_in_memory
// ---------------------------------------------------------------------------

/// Run a rule against an in-memory source string and return matched line numbers.
///
/// For rules with `languages: [regex]` or `languages: [generic]`, the
/// `pattern-regex` is applied directly over the raw source bytes using
/// [`RegexModeScanner`]. For rules with `languages: [rust]`, the source is
/// parsed with tree-sitter and evaluated with the formula engine.
///
/// Line numbers in the returned vec are 1-based and may contain duplicates when
/// multiple patterns within a rule match on the same line.
///
/// # Arguments
///
/// * `rule` - Compiled rule IR to evaluate.
/// * `src` - Source text to scan.
///
/// # Errors
///
/// Returns [`ConformanceError::Scan`] when regex compilation or AST evaluation
/// fails. Returns [`ConformanceError::UnsupportedLanguage`] when the rule's
/// declared language set contains no language supported by the in-memory scanner.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::conformance::scan_in_memory;
/// use xzardgz::scanner::sast::rule::ir::{Formula, Leaf, RuleIr};
/// use xzardgz::scanner::sast::rule::metadata::Severity;
///
/// let rule = RuleIr {
///     id: "test".to_string(),
///     message: "found".to_string(),
///     languages: vec!["regex".to_string()],
///     severity: Severity::Warning,
///     metadata: None,
///     formula: Formula::Leaf(Leaf::Regex("DANGER".to_string())),
///     fix: None,
/// };
/// let lines = scan_in_memory(&rule, "DANGER\nsafe\n").unwrap();
/// assert_eq!(lines, vec![1]);
/// ```
pub fn scan_in_memory(rule: &RuleIr, src: &str) -> Result<Vec<u32>, ConformanceError> {
    if rule.applies_to_regex_mode() {
        scan_in_memory_regex(rule, src)
    } else if rule.applies_to_rust() {
        scan_in_memory_ast(rule, src)
    } else {
        Err(ConformanceError::UnsupportedLanguage(
            rule.languages.clone(),
        ))
    }
}

/// Apply the regex-mode scanner to `src` and return 1-based line numbers.
///
/// Only called when `rule.applies_to_regex_mode()` is `true`.
fn scan_in_memory_regex(rule: &RuleIr, src: &str) -> Result<Vec<u32>, ConformanceError> {
    let scanner = RegexModeScanner::new(rule).map_err(|e| ConformanceError::Scan(e.to_string()))?;
    let matches = scanner
        .scan_bytes(src.as_bytes(), usize::MAX)
        .map_err(|e| ConformanceError::Scan(e.to_string()))?;
    Ok(matches.iter().map(|m| m.line_start as u32).collect())
}

/// Apply the AST formula engine to `src` and return 1-based line numbers.
///
/// Builds an in-memory [`CachedRoot`] by parsing `src` with tree-sitter, then
/// delegates to [`scan_rule`]. The byte offsets of each match are converted to
/// 1-based line numbers using a precomputed line-starts table.
///
/// Only called when `rule.applies_to_rust()` is `true`.
///
/// [`CachedRoot`]: crate::scanner::sast::ast::parse::CachedRoot
/// [`scan_rule`]: crate::scanner::sast::engine::formula::scan_rule
fn scan_in_memory_ast(rule: &RuleIr, src: &str) -> Result<Vec<u32>, ConformanceError> {
    use ast_grep_core::tree_sitter::LanguageExt;
    use ast_grep_language::SupportLang;

    use crate::scanner::sast::ast::diagnostics::ErrorNodeDensity;
    use crate::scanner::sast::ast::parse::CachedRoot;
    use crate::scanner::sast::config::SastEngineConfig;
    use crate::scanner::sast::engine::formula::scan_rule;
    use crate::scanner::sast::engine::pattern::PatternCompiler;

    // Build a table of byte offsets at which each line begins so that
    // match byte offsets can be converted to 1-based line numbers in O(log n).
    let mut line_starts: Vec<usize> = vec![0];
    for (i, b) in src.bytes().enumerate() {
        if b == b'\n' {
            line_starts.push(i + 1);
        }
    }

    let root = SupportLang::Rust.ast_grep(src);
    let density = ErrorNodeDensity::from_root(&root);
    let cached = CachedRoot { root, density };
    let compiler = PatternCompiler::new();
    let config = SastEngineConfig::new();

    let (matches, _) = scan_rule(rule, &cached, &compiler, &config)
        .map_err(|e| ConformanceError::Scan(e.to_string()))?;

    // partition_point(|&s| s <= byte_offset) yields the 1-based line number.
    let lines: Vec<u32> = matches
        .iter()
        .map(|m| line_starts.partition_point(|&s| s <= m.start) as u32)
        .collect();

    Ok(lines)
}

// ---------------------------------------------------------------------------
// check_conformance
// ---------------------------------------------------------------------------

/// Compare scanner results against fixture annotations for a specific rule.
///
/// Filters `annotations` for entries relevant to `rule_id`:
///
/// - `RuleId(rule_id)` annotations declare expected matches on their target line.
/// - `Ok` annotations declare expected non-matches on their target line. These
///   are global (not per-rule) and apply to any rule being checked.
/// - `TodoRuleId(rule_id)` annotations are silently ignored.
///
/// Returns [`RuleConformanceResult::Skipped`] when the fixture contains no
/// `RuleId(rule_id)` and no `Ok` annotations (the fixture does not test this
/// rule at all).
///
/// # Arguments
///
/// * `rule_id` - Identifier of the rule being checked.
/// * `annotations` - All annotations parsed from the fixture.
/// * `matched_lines` - 1-based line numbers returned by the scanner.
///
/// # Returns
///
/// - [`RuleConformanceResult::Pass`] when all expected matches are found and no
///   `ok:` line was matched.
/// - [`RuleConformanceResult::Fail`] when at least one expected match is absent
///   or at least one `ok:` line was matched.
/// - [`RuleConformanceResult::Skipped`] when no applicable annotations exist.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::conformance::{check_conformance, Annotation, AnnotationKind};
///
/// let annotations = vec![Annotation {
///     annotation_line: 1,
///     target_line: 2,
///     kind: AnnotationKind::RuleId("my-rule".to_string()),
/// }];
/// let result = check_conformance("my-rule", &annotations, &[2]);
/// assert!(result.is_pass());
/// ```
pub fn check_conformance(
    rule_id: &str,
    annotations: &[Annotation],
    matched_lines: &[u32],
) -> RuleConformanceResult {
    // Collect RuleId annotations for this specific rule.
    let rule_annotations: Vec<&Annotation> = annotations
        .iter()
        .filter(|a| matches!(&a.kind, AnnotationKind::RuleId(id) if id == rule_id))
        .collect();

    // Collect Ok annotations (global -- every rule is bound by them).
    let ok_annotations: Vec<&Annotation> = annotations
        .iter()
        .filter(|a| matches!(a.kind, AnnotationKind::Ok))
        .collect();

    // No RuleId or Ok annotations means this fixture does not test this rule.
    if rule_annotations.is_empty() && ok_annotations.is_empty() {
        return RuleConformanceResult::Skipped(format!("no annotations for rule '{rule_id}'"));
    }

    let matched_set: std::collections::HashSet<u32> = matched_lines.iter().copied().collect();

    // Lines expected to match but absent from matched_lines.
    let mut missed_lines: Vec<u32> = rule_annotations
        .iter()
        .filter(|a| !matched_set.contains(&a.target_line))
        .map(|a| a.target_line)
        .collect();
    missed_lines.sort_unstable();

    // Ok-annotated lines that were matched (false positives).
    let mut false_positive_lines: Vec<u32> = ok_annotations
        .iter()
        .filter(|a| matched_set.contains(&a.target_line))
        .map(|a| a.target_line)
        .collect();
    false_positive_lines.sort_unstable();

    if missed_lines.is_empty() && false_positive_lines.is_empty() {
        RuleConformanceResult::Pass
    } else {
        RuleConformanceResult::Fail {
            missed_lines,
            false_positive_lines,
        }
    }
}

// ---------------------------------------------------------------------------
// run_rule_conformance
// ---------------------------------------------------------------------------

/// Run a full conformance check for one rule against an in-memory fixture source.
///
/// Orchestrates annotation parsing, in-memory scanning, and result comparison
/// via [`parse_annotations`], [`scan_in_memory`], and [`check_conformance`].
///
/// When [`scan_in_memory`] returns an error (unsupported language, regex
/// compile failure, etc.) the error is recorded as a
/// [`RuleConformanceResult::Skipped`] with the error message rather than
/// propagating.
///
/// # Arguments
///
/// * `rule` - Compiled rule IR to evaluate.
/// * `fixture_src` - Source text of the conformance fixture.
///
/// # Returns
///
/// A [`RuleConformanceResult`] describing whether the rule passes, fails, or
/// is skipped for this fixture.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::conformance::run_rule_conformance;
/// use xzardgz::scanner::sast::rule::ir::{Formula, Leaf, RuleIr};
/// use xzardgz::scanner::sast::rule::metadata::Severity;
///
/// let rule = RuleIr {
///     id: "example".to_string(),
///     message: "found it".to_string(),
///     languages: vec!["regex".to_string()],
///     severity: Severity::Warning,
///     metadata: None,
///     formula: Formula::Leaf(Leaf::Regex("SECRET".to_string())),
///     fix: None,
/// };
/// let fixture =
///     "// ruleid: example\nSECRET = \"abc\";\n// ok: safe value\nPUBLIC = \"xyz\";\n";
/// let result = run_rule_conformance(&rule, fixture);
/// assert!(result.is_pass(), "expected Pass, got {result:?}");
/// ```
pub fn run_rule_conformance(rule: &RuleIr, fixture_src: &str) -> RuleConformanceResult {
    let annotations = parse_annotations(fixture_src);

    let matched_lines = match scan_in_memory(rule, fixture_src) {
        Ok(lines) => lines,
        Err(e) => {
            return RuleConformanceResult::Skipped(format!(
                "scan error for rule '{}': {e}",
                rule.id
            ));
        }
    };

    check_conformance(&rule.id, &annotations, &matched_lines)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::sast::rule::ir::{Formula, Leaf, RuleIr};
    use crate::scanner::sast::rule::metadata::Severity;

    // Helper: build a minimal regex-mode rule for unit testing.
    fn make_regex_rule(id: &str, pattern: &str) -> RuleIr {
        RuleIr {
            id: id.to_string(),
            message: "test".to_string(),
            languages: vec!["regex".to_string()],
            severity: Severity::Warning,
            metadata: None,
            formula: Formula::Leaf(Leaf::Regex(pattern.to_string())),
            fix: None,
        }
    }

    // ---------------------------------------------------------------------------
    // AnnotationKind
    // ---------------------------------------------------------------------------

    #[test]
    fn test_annotation_kind_ruleid_values_equal_when_id_matches() {
        assert_eq!(
            AnnotationKind::RuleId("my-rule".to_string()),
            AnnotationKind::RuleId("my-rule".to_string())
        );
    }

    #[test]
    fn test_annotation_kind_ruleid_values_differ_when_id_differs() {
        assert_ne!(
            AnnotationKind::RuleId("rule-a".to_string()),
            AnnotationKind::RuleId("rule-b".to_string())
        );
    }

    #[test]
    fn test_annotation_kind_ok_equals_ok() {
        assert_eq!(AnnotationKind::Ok, AnnotationKind::Ok);
    }

    // ---------------------------------------------------------------------------
    // parse_annotations
    // ---------------------------------------------------------------------------

    #[test]
    fn test_parse_annotations_above_ruleid_target_is_next_line() {
        let src = "// ruleid: my-rule\nlet x = 1;\n";
        let annotations = parse_annotations(src);
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].annotation_line, 1);
        assert_eq!(annotations[0].target_line, 2);
        assert!(
            matches!(&annotations[0].kind, AnnotationKind::RuleId(id) if id == "my-rule"),
            "expected RuleId('my-rule')"
        );
    }

    #[test]
    fn test_parse_annotations_inline_ruleid_target_is_same_line() {
        let src = "let x = 1; // ruleid: my-rule\n";
        let annotations = parse_annotations(src);
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].annotation_line, 1);
        assert_eq!(annotations[0].target_line, 1);
    }

    #[test]
    fn test_parse_annotations_ok_above_target_is_next_line() {
        let src = "// ok: safe alternative\nlet y = 2;\n";
        let annotations = parse_annotations(src);
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].target_line, 2);
        assert!(matches!(annotations[0].kind, AnnotationKind::Ok));
    }

    #[test]
    fn test_parse_annotations_todoruleid_above_recognized() {
        let src = "// todoruleid: my-rule\nsome_code();\n";
        let annotations = parse_annotations(src);
        assert_eq!(annotations.len(), 1);
        assert!(
            matches!(&annotations[0].kind, AnnotationKind::TodoRuleId(id) if id == "my-rule"),
            "expected TodoRuleId('my-rule')"
        );
    }

    #[test]
    fn test_parse_annotations_empty_source_returns_empty_vec() {
        assert!(parse_annotations("").is_empty());
    }

    #[test]
    fn test_parse_annotations_source_with_no_annotations_returns_empty_vec() {
        let src = "fn main() {\n    println!(\"hello\");\n}\n";
        assert!(parse_annotations(src).is_empty());
    }

    #[test]
    fn test_parse_annotations_multiple_annotations_returned_in_source_order() {
        let src = "// ruleid: r1\nline1;\n// ok: comment\nline2;\n";
        let annotations = parse_annotations(src);
        assert_eq!(annotations.len(), 2);
        assert!(matches!(&annotations[0].kind, AnnotationKind::RuleId(id) if id == "r1"));
        assert!(matches!(annotations[1].kind, AnnotationKind::Ok));
    }

    #[test]
    fn test_parse_annotations_annotation_on_last_line_target_is_line_plus_one() {
        let src = "code;\n// ruleid: my-rule";
        let annotations = parse_annotations(src);
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].annotation_line, 2);
        assert_eq!(annotations[0].target_line, 3);
    }

    // ---------------------------------------------------------------------------
    // check_conformance
    // ---------------------------------------------------------------------------

    #[test]
    fn test_check_conformance_all_ruleid_matched_returns_pass() {
        let annotations = vec![Annotation {
            annotation_line: 1,
            target_line: 2,
            kind: AnnotationKind::RuleId("my-rule".to_string()),
        }];
        let result = check_conformance("my-rule", &annotations, &[2]);
        assert!(result.is_pass(), "expected Pass, got {result:?}");
    }

    #[test]
    fn test_check_conformance_missed_match_returns_fail_with_missed_line() {
        let annotations = vec![Annotation {
            annotation_line: 1,
            target_line: 2,
            kind: AnnotationKind::RuleId("my-rule".to_string()),
        }];
        let result = check_conformance("my-rule", &annotations, &[]);
        assert!(result.is_fail(), "expected Fail, got {result:?}");
        if let RuleConformanceResult::Fail { missed_lines, .. } = result {
            assert!(missed_lines.contains(&2));
        }
    }

    #[test]
    fn test_check_conformance_false_positive_returns_fail_with_fp_line() {
        let annotations = vec![Annotation {
            annotation_line: 2,
            target_line: 3,
            kind: AnnotationKind::Ok,
        }];
        let result = check_conformance("my-rule", &annotations, &[3]);
        assert!(result.is_fail(), "expected Fail, got {result:?}");
        if let RuleConformanceResult::Fail {
            false_positive_lines,
            ..
        } = result
        {
            assert!(false_positive_lines.contains(&3));
        }
    }

    #[test]
    fn test_check_conformance_todoruleid_only_returns_skipped() {
        let annotations = vec![Annotation {
            annotation_line: 1,
            target_line: 2,
            kind: AnnotationKind::TodoRuleId("my-rule".to_string()),
        }];
        let result = check_conformance("my-rule", &annotations, &[]);
        assert!(result.is_skipped(), "expected Skipped, got {result:?}");
    }

    #[test]
    fn test_check_conformance_no_applicable_annotations_returns_skipped() {
        let annotations = vec![Annotation {
            annotation_line: 1,
            target_line: 2,
            kind: AnnotationKind::RuleId("other-rule".to_string()),
        }];
        let result = check_conformance("my-rule", &annotations, &[2]);
        assert!(result.is_skipped(), "expected Skipped, got {result:?}");
    }

    #[test]
    fn test_check_conformance_ok_with_no_match_returns_pass() {
        let annotations = vec![Annotation {
            annotation_line: 1,
            target_line: 2,
            kind: AnnotationKind::Ok,
        }];
        let result = check_conformance("my-rule", &annotations, &[]);
        assert!(result.is_pass(), "expected Pass, got {result:?}");
    }

    #[test]
    fn test_check_conformance_missed_and_false_positive_both_reported_in_fail() {
        let annotations = vec![
            Annotation {
                annotation_line: 1,
                target_line: 2,
                kind: AnnotationKind::RuleId("my-rule".to_string()),
            },
            Annotation {
                annotation_line: 3,
                target_line: 4,
                kind: AnnotationKind::Ok,
            },
        ];
        // Line 2 not matched (missed), line 4 matched (false positive).
        let result = check_conformance("my-rule", &annotations, &[4]);
        assert!(result.is_fail(), "expected Fail, got {result:?}");
        if let RuleConformanceResult::Fail {
            missed_lines,
            false_positive_lines,
        } = result
        {
            assert!(missed_lines.contains(&2), "line 2 must be missed");
            assert!(
                false_positive_lines.contains(&4),
                "line 4 must be a false positive"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // RuleConformanceResult
    // ---------------------------------------------------------------------------

    #[test]
    fn test_rule_conformance_result_pass_is_pass_returns_true() {
        assert!(RuleConformanceResult::Pass.is_pass());
    }

    #[test]
    fn test_rule_conformance_result_fail_is_pass_returns_false() {
        let r = RuleConformanceResult::Fail {
            missed_lines: vec![],
            false_positive_lines: vec![1],
        };
        assert!(!r.is_pass());
    }

    #[test]
    fn test_rule_conformance_result_fail_is_fail_returns_true() {
        let r = RuleConformanceResult::Fail {
            missed_lines: vec![2],
            false_positive_lines: vec![],
        };
        assert!(r.is_fail());
    }

    #[test]
    fn test_rule_conformance_result_pass_is_fail_returns_false() {
        assert!(!RuleConformanceResult::Pass.is_fail());
    }

    #[test]
    fn test_rule_conformance_result_skipped_is_skipped_returns_true() {
        assert!(RuleConformanceResult::Skipped("reason".to_string()).is_skipped());
    }

    #[test]
    fn test_rule_conformance_result_pass_is_skipped_returns_false() {
        assert!(!RuleConformanceResult::Pass.is_skipped());
    }

    #[test]
    fn test_rule_conformance_result_fail_is_regression_from_pass_returns_true() {
        let r = RuleConformanceResult::Fail {
            missed_lines: vec![2],
            false_positive_lines: vec![],
        };
        assert!(r.is_regression_from_pass());
    }

    #[test]
    fn test_rule_conformance_result_pass_is_not_regression_from_pass() {
        assert!(!RuleConformanceResult::Pass.is_regression_from_pass());
    }

    #[test]
    fn test_rule_conformance_result_skipped_is_not_regression_from_pass() {
        assert!(!RuleConformanceResult::Skipped("n/a".to_string()).is_regression_from_pass());
    }

    // ---------------------------------------------------------------------------
    // scan_in_memory
    // ---------------------------------------------------------------------------

    #[test]
    fn test_scan_in_memory_regex_finds_match_on_correct_line() {
        let rule = make_regex_rule("test", "DANGER");
        let src = "safe\nDANGER\nsafe\n";
        let lines = scan_in_memory(&rule, src).unwrap();
        assert_eq!(lines, vec![2], "DANGER is on line 2");
    }

    #[test]
    fn test_scan_in_memory_regex_no_match_returns_empty_vec() {
        let rule = make_regex_rule("test", "DANGER");
        let src = "safe\nnothing\n";
        let lines = scan_in_memory(&rule, src).unwrap();
        assert!(lines.is_empty());
    }

    #[test]
    fn test_scan_in_memory_regex_multiple_lines_all_reported() {
        let rule = make_regex_rule("test", "HIT");
        let src = "HIT\nsafe\nHIT\n";
        let lines = scan_in_memory(&rule, src).unwrap();
        assert!(lines.contains(&1), "line 1 must be matched");
        assert!(lines.contains(&3), "line 3 must be matched");
    }

    #[test]
    fn test_scan_in_memory_unsupported_language_returns_error() {
        let rule = RuleIr {
            id: "go-rule".to_string(),
            message: "test".to_string(),
            languages: vec!["go".to_string()],
            severity: Severity::Warning,
            metadata: None,
            formula: Formula::Leaf(Leaf::Regex("pattern".to_string())),
            fix: None,
        };
        let result = scan_in_memory(&rule, "code");
        assert!(
            matches!(result, Err(ConformanceError::UnsupportedLanguage(_))),
            "go language must return UnsupportedLanguage error"
        );
    }

    // ---------------------------------------------------------------------------
    // run_rule_conformance
    // ---------------------------------------------------------------------------

    #[test]
    fn test_run_rule_conformance_pass_fixture_returns_pass() {
        let rule = make_regex_rule("test-rule", "DANGER");
        let fixture = "// ruleid: test-rule\nDANGER\n// ok: safe\nSAFE\n";
        let result = run_rule_conformance(&rule, fixture);
        assert!(result.is_pass(), "expected Pass, got {result:?}");
    }

    #[test]
    fn test_run_rule_conformance_missed_match_returns_fail() {
        let rule = make_regex_rule("test-rule", "DANGER");
        let fixture = "// ruleid: test-rule\nSAFE\n";
        let result = run_rule_conformance(&rule, fixture);
        assert!(
            result.is_fail(),
            "expected Fail (missed match), got {result:?}"
        );
    }

    #[test]
    fn test_run_rule_conformance_false_positive_returns_fail() {
        let rule = make_regex_rule("test-rule", "DANGER");
        let fixture = "// ok: should not match\nDANGER\n";
        let result = run_rule_conformance(&rule, fixture);
        assert!(
            result.is_fail(),
            "expected Fail (false positive), got {result:?}"
        );
    }

    #[test]
    fn test_run_rule_conformance_no_annotations_returns_skipped() {
        let rule = make_regex_rule("test-rule", "DANGER");
        let fixture = "DANGER\nsafe\n";
        let result = run_rule_conformance(&rule, fixture);
        assert!(result.is_skipped(), "expected Skipped, got {result:?}");
    }

    // ---------------------------------------------------------------------------
    // ConformanceBaseline
    // ---------------------------------------------------------------------------

    #[test]
    fn test_conformance_baseline_from_json_str_valid_parses_successfully() {
        let json =
            r#"{"version":"1","generated_at":"2026-11-25","entries":{"rust-md5-usage":"pass"}}"#;
        let baseline = ConformanceBaseline::from_json_str(json).unwrap();
        assert_eq!(baseline.version, "1");
        assert_eq!(
            baseline.entries.get("rust-md5-usage"),
            Some(&"pass".to_string())
        );
    }

    #[test]
    fn test_conformance_baseline_from_json_str_invalid_json_returns_error() {
        let result = ConformanceBaseline::from_json_str("not valid json");
        assert!(result.is_err(), "invalid JSON must return Err");
        assert!(matches!(result, Err(ConformanceError::Json(_))));
    }

    #[test]
    fn test_conformance_baseline_from_path_missing_file_returns_io_error() {
        let result = ConformanceBaseline::from_path(Path::new("/nonexistent/path/baseline.json"));
        assert!(result.is_err(), "missing file must return Err");
        assert!(matches!(result, Err(ConformanceError::Io(_))));
    }

    #[test]
    fn test_conformance_baseline_detect_regressions_finds_pass_to_fail() {
        let json = r#"{"version":"1","generated_at":"2026-01-01","entries":{"my-rule":"pass"}}"#;
        let baseline = ConformanceBaseline::from_json_str(json).unwrap();
        let current = vec![(
            "my-rule".to_string(),
            RuleConformanceResult::Fail {
                missed_lines: vec![5],
                false_positive_lines: vec![],
            },
        )];
        let regressions = baseline.detect_regressions(&current);
        assert!(
            regressions.contains(&"my-rule".to_string()),
            "pass-to-fail must be detected as a regression"
        );
    }

    #[test]
    fn test_conformance_baseline_detect_regressions_ignores_pass_to_pass() {
        let json = r#"{"version":"1","generated_at":"2026-01-01","entries":{"my-rule":"pass"}}"#;
        let baseline = ConformanceBaseline::from_json_str(json).unwrap();
        let current = vec![("my-rule".to_string(), RuleConformanceResult::Pass)];
        let regressions = baseline.detect_regressions(&current);
        assert!(
            regressions.is_empty(),
            "pass-to-pass must not be a regression"
        );
    }

    #[test]
    fn test_conformance_baseline_detect_regressions_ignores_new_failures_not_in_baseline() {
        let json = r#"{"version":"1","generated_at":"2026-01-01","entries":{}}"#;
        let baseline = ConformanceBaseline::from_json_str(json).unwrap();
        let current = vec![(
            "new-rule".to_string(),
            RuleConformanceResult::Fail {
                missed_lines: vec![3],
                false_positive_lines: vec![],
            },
        )];
        let regressions = baseline.detect_regressions(&current);
        assert!(
            regressions.is_empty(),
            "new failure not in baseline must not be a regression"
        );
    }

    #[test]
    fn test_conformance_baseline_detect_regressions_ignores_skipped_baseline_entries() {
        let json = r#"{"version":"1","generated_at":"2026-01-01","entries":{"my-rule":"skipped(no fixture)"}}"#;
        let baseline = ConformanceBaseline::from_json_str(json).unwrap();
        let current = vec![(
            "my-rule".to_string(),
            RuleConformanceResult::Fail {
                missed_lines: vec![2],
                false_positive_lines: vec![],
            },
        )];
        let regressions = baseline.detect_regressions(&current);
        assert!(
            regressions.is_empty(),
            "baseline entry that was not 'pass' must not trigger a regression"
        );
    }
}
