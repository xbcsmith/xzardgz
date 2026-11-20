//! Markdown report builder for the SAST scan plugin.
//!
//! [`build_sast_markdown_report`] renders a [`SastScanResult`] and its
//! accompanying [`ScoringResult`] as a structured Markdown document suitable
//! for human review and CI artifact storage.

use std::fmt::Write as FmtWrite;

use crate::plugins::sast_scan::scoring::SastScanResult;
use crate::scanner::sast::rule::metadata::{Confidence, Severity};
use crate::scanner::scoring::ScoringResult;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn severity_label(s: &Severity) -> &'static str {
    match s {
        Severity::Error => "ERROR",
        Severity::Warning => "WARNING",
        Severity::Info => "INFO",
    }
}

fn confidence_label(c: &Confidence) -> &'static str {
    match c {
        Confidence::High => "HIGH",
        Confidence::Medium => "MEDIUM",
        Confidence::Low => "LOW",
        Confidence::Unknown => "UNKNOWN",
    }
}

fn severity_order(s: &Severity) -> u8 {
    match s {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    }
}

// ---------------------------------------------------------------------------
// build_sast_markdown_report
// ---------------------------------------------------------------------------

/// Renders a SAST scan result and its confidence scoring into a Markdown
/// document.
///
/// The document structure:
///
/// 1. **Header block** — scan root, duration, match count, confidence scores.
/// 2. **Summary table** — one row per match sorted by severity (Error,
///    Warning, Info), showing file, rule ID, severity, confidence, and
///    message.
/// 3. **Findings** — detailed per-match blocks grouped by severity, each
///    containing file location, message, snippet, and metavariable bindings.
///
/// # Arguments
///
/// * `result`         - The completed SAST scan result with all matches.
/// * `scoring_result` - The confidence scoring result for this scan.
///
/// # Returns
///
/// A `String` containing the full Markdown document.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use xzardgz::plugins::sast_scan::scoring::SastScanResult;
/// use xzardgz::plugins::sast_scan::report::build_sast_markdown_report;
/// use xzardgz::scanner::scoring::{ConfidenceScorer, ScoringInput};
///
/// let result = SastScanResult {
///     matches: vec![],
///     ruleset_ids: vec!["builtin:crypto".to_string()],
///     scan_root: PathBuf::from("/repo"),
///     duration_ms: 10,
/// };
/// let scorer = ConfidenceScorer::with_defaults();
/// let scoring_result = scorer.score(&result, None);
/// let report = build_sast_markdown_report(&result, &scoring_result);
/// assert!(report.contains("SAST Scan Report"));
/// assert!(report.contains("## Summary"));
/// ```
pub fn build_sast_markdown_report(
    result: &SastScanResult,
    scoring_result: &ScoringResult,
) -> String {
    let mut doc = String::new();

    // H1 title
    // SAFETY: writing to a String cannot fail.
    writeln!(doc, "# SAST Scan Report").unwrap();
    writeln!(doc).unwrap();

    // Header metadata block
    writeln!(doc, "**Scan root:** `{}`", result.scan_root.display()).unwrap();
    writeln!(doc, "**Duration:** {}ms", result.duration_ms).unwrap();
    writeln!(doc, "**Total matches:** {}", result.matches.len()).unwrap();
    writeln!(
        doc,
        "**Static confidence:** {:.2}",
        scoring_result.static_score
    )
    .unwrap();
    writeln!(
        doc,
        "**Blended confidence:** {:.2}",
        scoring_result.blended_score
    )
    .unwrap();
    writeln!(doc).unwrap();

    // Summary table
    writeln!(doc, "## Summary").unwrap();
    writeln!(doc).unwrap();

    if result.matches.is_empty() {
        writeln!(doc, "*No matches found.*").unwrap();
        writeln!(doc).unwrap();
    } else {
        writeln!(doc, "| File | Rule ID | Severity | Confidence | Message |").unwrap();
        writeln!(doc, "|------|---------|----------|------------|---------|").unwrap();

        // Sort matches by severity for the summary table (Error first).
        let mut sorted: Vec<_> = result.matches.iter().collect();
        sorted.sort_by_key(|m| severity_order(&m.severity));

        for m in &sorted {
            let path_str = m.path.display().to_string().replace('|', "\\|");
            let rule_id = m.rule_id.replace('|', "\\|");
            let message = m.message.replace('|', "\\|");
            writeln!(
                doc,
                "| `{}` | `{}` | {} | {} | {} |",
                path_str,
                rule_id,
                severity_label(&m.severity),
                confidence_label(&m.confidence),
                message,
            )
            .unwrap();
        }
        writeln!(doc).unwrap();
    }

    // Detailed findings section
    writeln!(doc, "## Findings").unwrap();
    writeln!(doc).unwrap();

    if result.matches.is_empty() {
        writeln!(doc, "*No findings to display.*").unwrap();
        writeln!(doc).unwrap();
        return doc;
    }

    // Group and sort by severity: Error -> Warning -> Info.
    let mut sorted: Vec<_> = result.matches.iter().collect();
    sorted.sort_by_key(|m| severity_order(&m.severity));

    for m in &sorted {
        writeln!(doc, "### `{}` — {}", m.rule_id, severity_label(&m.severity)).unwrap();
        writeln!(doc).unwrap();

        writeln!(doc, "**File:** `{}:{}`", m.path.display(), m.start.line).unwrap();
        writeln!(doc, "**Message:** {}", m.message).unwrap();

        if !m.snippet.text.is_empty() {
            writeln!(doc, "**Snippet:**").unwrap();
            writeln!(doc, "```").unwrap();
            writeln!(doc, "{}", m.snippet.text).unwrap();
            writeln!(doc, "```").unwrap();
        }

        if !m.metavariables.is_empty() {
            let pairs: Vec<String> = m
                .metavariables
                .iter()
                .map(|(k, v)| format!("{} = `{}`", k, v.text))
                .collect();
            writeln!(doc, "**Metavariables:** {}", pairs.join(", ")).unwrap();
        }

        writeln!(doc).unwrap();
        writeln!(doc, "---").unwrap();
        writeln!(doc).unwrap();
    }

    doc
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::plugins::sast_scan::scoring::SastScanResult;
    use crate::scanner::sast::match_model::{MatchSnippet, MetavarBinding, Position, SastMatch};
    use crate::scanner::sast::rule::metadata::{Confidence, RuleMetadata, Severity};
    use crate::scanner::scoring::{ConfidenceScorer, ScoringResult};

    fn zero_scoring_result() -> ScoringResult {
        ScoringResult {
            static_score: 1.0,
            ai_score: None,
            blended_score: 1.0,
            violation_reasons: vec![],
        }
    }

    fn make_match(rule_id: &str, severity: Severity) -> SastMatch {
        SastMatch {
            rule_id: rule_id.to_string(),
            ruleset_id: "builtin:crypto".to_string(),
            message: format!("{} usage detected", rule_id),
            severity,
            confidence: Confidence::High,
            path: PathBuf::from("src/crypto.rs"),
            start: Position {
                line: 5,
                col: 0,
                byte: 0,
            },
            end: Position {
                line: 5,
                col: 10,
                byte: 10,
            },
            snippet: MatchSnippet {
                text: "Md5::new()".to_string(),
                context_before: vec![],
                context_after: vec![],
            },
            metavariables: BTreeMap::new(),
            metadata: RuleMetadata::default(),
            fingerprint: "fp_test_0".to_string(),
            fix: None,
        }
    }

    fn make_result_with_match(m: SastMatch) -> SastScanResult {
        SastScanResult {
            matches: vec![m],
            ruleset_ids: vec!["builtin:crypto".to_string()],
            scan_root: PathBuf::from("/workspace/repo"),
            duration_ms: 55,
        }
    }

    #[test]
    fn test_build_sast_markdown_report_contains_rule_id() {
        let m = make_match("rust-md5-usage", Severity::Warning);
        let result = make_result_with_match(m);
        let scorer = ConfidenceScorer::with_defaults();
        let scoring_result = scorer.score(&result, None);
        let report = build_sast_markdown_report(&result, &scoring_result);
        assert!(
            report.contains("rust-md5-usage"),
            "report must contain the rule id"
        );
    }

    #[test]
    fn test_build_sast_markdown_report_with_zero_matches_has_summary() {
        let result = SastScanResult {
            matches: vec![],
            ruleset_ids: vec!["builtin:crypto".to_string()],
            scan_root: PathBuf::from("/workspace/repo"),
            duration_ms: 10,
        };
        let scoring_result = zero_scoring_result();
        let report = build_sast_markdown_report(&result, &scoring_result);
        assert!(
            report.contains("## Summary"),
            "report must contain Summary section even with zero matches"
        );
        assert!(
            report.contains("No matches found"),
            "report must indicate no matches"
        );
    }

    #[test]
    fn test_build_sast_markdown_report_contains_scan_root() {
        let m = make_match("rust-sha1-usage", Severity::Warning);
        let result = SastScanResult {
            matches: vec![m],
            ruleset_ids: vec!["builtin:crypto".to_string()],
            scan_root: PathBuf::from("/my/custom/scan/root"),
            duration_ms: 20,
        };
        let scoring_result = zero_scoring_result();
        let report = build_sast_markdown_report(&result, &scoring_result);
        assert!(
            report.contains("/my/custom/scan/root"),
            "report must contain the scan root path"
        );
    }

    #[test]
    fn test_build_sast_markdown_report_metavariables_appear_when_present() {
        let mut m = make_match("rust-md5-usage", Severity::Warning);
        m.metavariables.insert(
            "$HASH".to_string(),
            MetavarBinding {
                text: "md5_instance".to_string(),
                start: Position {
                    line: 5,
                    col: 0,
                    byte: 0,
                },
                end: Position {
                    line: 5,
                    col: 11,
                    byte: 11,
                },
            },
        );
        let result = make_result_with_match(m);
        let scoring_result = zero_scoring_result();
        let report = build_sast_markdown_report(&result, &scoring_result);
        assert!(
            report.contains("$HASH"),
            "report must contain metavariable binding"
        );
    }

    #[test]
    fn test_build_sast_markdown_report_error_severity_appears_before_warning() {
        let m_warn = make_match("rust-sha1-usage", Severity::Warning);
        let m_err = make_match("rust-weak-cipher", Severity::Error);
        let result = SastScanResult {
            matches: vec![m_warn, m_err],
            ruleset_ids: vec!["builtin:crypto".to_string()],
            scan_root: PathBuf::from("/repo"),
            duration_ms: 30,
        };
        let scoring_result = zero_scoring_result();
        let report = build_sast_markdown_report(&result, &scoring_result);

        let err_pos = report
            .find("rust-weak-cipher")
            .expect("error rule must appear");
        let warn_pos = report
            .find("rust-sha1-usage")
            .expect("warning rule must appear");
        assert!(
            err_pos < warn_pos,
            "ERROR findings must appear before WARNING findings in the report"
        );
    }
}
