//! Confidence scoring for the SAST scan plugin.
//!
//! [`SastScanResult`] aggregates a completed SAST scan and implements
//! [`crate::scanner::scoring::ScoringInput`] so the scan result can be fed to
//! [`crate::scanner::scoring::ConfidenceScorer`].
//!
//! # Signal Catalog
//!
//! One signal is emitted per [`crate::scanner::sast::match_model::SastMatch`]:
//!
//! - `AbsoluteViolation` when `severity == Error` AND `metadata.cwe` contains
//!   `CWE-326`, `CWE-327`, or `CWE-328`. No further signal is emitted for that
//!   match.
//! - `Positive` (weight `0.3`) for `confidence == High` matches that did not
//!   trigger a violation.
//! - `Negative` (weight `0.2`) for `confidence == Low` matches.
//! - `confidence == Medium` and `confidence == Unknown` produce no signal.
//!
//! `review_violations` mode is `false`; `AbsoluteViolation` always floors the
//! static score to `0.0` for this plugin.

use std::path::PathBuf;

use crate::scanner::sast::match_model::SastMatch;
use crate::scanner::sast::rule::metadata::{Confidence, Severity};
use crate::scanner::scoring::{ScoringInput, ScoringSignal};

/// CWE identifiers that trigger an [`ScoringSignal::AbsoluteViolation`] when
/// found on an `Error`-severity match.
const VIOLATION_CWES: &[&str] = &["CWE-326", "CWE-327", "CWE-328"];

// ---------------------------------------------------------------------------
// SastScanResult
// ---------------------------------------------------------------------------

/// Aggregated SAST scan result used as input to the confidence scorer.
///
/// Wraps the matches from a completed scan together with the ruleset
/// identifiers, the scan root path, and the total scan duration. The
/// confidence scorer consumes this via
/// [`ScoringInput`][crate::scanner::scoring::ScoringInput].
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use xzardgz::plugins::sast_scan::scoring::SastScanResult;
/// use xzardgz::scanner::scoring::ScoringInput;
///
/// let result = SastScanResult {
///     matches: vec![],
///     ruleset_ids: vec!["builtin:crypto".to_string()],
///     scan_root: PathBuf::from("/repo"),
///     duration_ms: 42,
/// };
/// assert_eq!(result.plugin_name(), "sast_scan");
/// assert!(result.signals().is_empty());
/// ```
pub struct SastScanResult {
    /// All matches returned by the SAST engine after severity filtering and
    /// the `max_findings` cap has been applied.
    pub matches: Vec<SastMatch>,
    /// Deduplicated ruleset identifiers that were active for this scan.
    pub ruleset_ids: Vec<String>,
    /// Root path that was scanned (repo-relative).
    pub scan_root: PathBuf,
    /// Total scan duration reported by the SAST engine in milliseconds.
    pub duration_ms: u64,
}

impl ScoringInput for SastScanResult {
    /// Returns one scoring signal per match in `self.matches`.
    ///
    /// Signals are emitted in the order the matches appear in the slice.
    /// For each match:
    ///
    /// 1. If `severity == Error` AND `metadata.cwe` contains `CWE-326`,
    ///    `CWE-327`, or `CWE-328`, emit
    ///    [`ScoringSignal::AbsoluteViolation`][crate::scanner::scoring::ScoringSignal::AbsoluteViolation]
    ///    with reason `"SAST [{rule_id}]: {cwes}"` and skip to the next match.
    /// 2. Otherwise, emit a signal based on confidence:
    ///    - [`Confidence::High`][crate::scanner::sast::rule::metadata::Confidence::High]:
    ///      `Positive { weight: 0.3 }`
    ///    - [`Confidence::Low`][crate::scanner::sast::rule::metadata::Confidence::Low]:
    ///      `Negative { weight: 0.2 }`
    ///    - `Medium` / `Unknown`: no signal.
    ///
    /// # Returns
    ///
    /// A `Vec<ScoringSignal>` with at most one entry per match.
    fn signals(&self) -> Vec<ScoringSignal> {
        let mut signals = Vec::new();

        for m in &self.matches {
            let is_violation = match &m.metadata.cwe {
                Some(cwes) => {
                    m.severity == Severity::Error
                        && cwes.iter().any(|c| VIOLATION_CWES.contains(&c.as_str()))
                }
                None => false,
            };

            if is_violation {
                let cwe_str = m
                    .metadata
                    .cwe
                    .as_deref()
                    .map(|cwes| cwes.join(", "))
                    .unwrap_or_default();
                signals.push(ScoringSignal::AbsoluteViolation {
                    reason: format!("SAST [{}]: {}", m.rule_id, cwe_str),
                });
                continue;
            }

            match m.confidence {
                Confidence::High => {
                    signals.push(ScoringSignal::Positive {
                        label: format!("high-confidence sast match: {}", m.rule_id),
                        weight: 0.3,
                    });
                }
                Confidence::Low => {
                    signals.push(ScoringSignal::Negative {
                        label: format!("low-confidence sast match: {}", m.rule_id),
                        weight: 0.2,
                    });
                }
                Confidence::Medium | Confidence::Unknown => {}
            }
        }

        signals
    }

    /// Returns a compact human-readable context string for AI prompt
    /// construction.
    ///
    /// Format:
    ///
    /// ```text
    /// SAST scan completed in {ms}ms. {total} matches: {error_count} errors,
    /// {warning_count} warnings. Rulesets: {rulesets}. Root: {scan_root}.
    /// ```
    ///
    /// # Returns
    ///
    /// A single-line summary string suitable for embedding in a prompt.
    fn context_for_ai(&self) -> String {
        let total = self.matches.len();
        let error_count = self
            .matches
            .iter()
            .filter(|m| m.severity == Severity::Error)
            .count();
        let warning_count = self
            .matches
            .iter()
            .filter(|m| m.severity == Severity::Warning)
            .count();
        let rulesets = self.ruleset_ids.join(", ");
        format!(
            "SAST scan completed in {}ms. {} matches: {} errors, {} warnings. \
             Rulesets: {}. Root: {}.",
            self.duration_ms,
            total,
            error_count,
            warning_count,
            rulesets,
            self.scan_root.display(),
        )
    }

    /// Returns `"sast_scan"`.
    fn plugin_name(&self) -> &str {
        "sast_scan"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::scanner::sast::match_model::{MatchSnippet, Position, SastMatch};
    use crate::scanner::sast::rule::metadata::{Confidence, RuleMetadata, Severity};
    use crate::scanner::scoring::{ConfidenceScorer, ScoringConfig, ScoringSignal};

    // ------------------------------------------------------------------
    // Helper constructors
    // ------------------------------------------------------------------

    fn make_match(
        rule_id: &str,
        severity: Severity,
        confidence: Confidence,
        cwe: Option<Vec<String>>,
    ) -> SastMatch {
        let metadata = RuleMetadata {
            cwe,
            ..RuleMetadata::default()
        };
        SastMatch {
            rule_id: rule_id.to_string(),
            ruleset_id: String::new(),
            message: "test message".to_string(),
            severity,
            confidence,
            path: PathBuf::from("src/lib.rs"),
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
                text: "Md5::new()".to_string(),
                context_before: vec![],
                context_after: vec![],
            },
            metavariables: BTreeMap::new(),
            metadata,
            fingerprint: "fp_test_0".to_string(),
            fix: None,
        }
    }

    fn make_result_with_match(m: SastMatch) -> SastScanResult {
        SastScanResult {
            matches: vec![m],
            ruleset_ids: vec!["builtin:crypto".to_string()],
            scan_root: PathBuf::from("/repo"),
            duration_ms: 100,
        }
    }

    // ------------------------------------------------------------------
    // ScoringInput::signals tests
    // ------------------------------------------------------------------

    #[test]
    fn test_sast_scan_result_high_confidence_emits_positive_signal() {
        let m = make_match("rust-md5-usage", Severity::Warning, Confidence::High, None);
        let result = make_result_with_match(m);
        let signals = result.signals();
        assert_eq!(signals.len(), 1);
        assert!(
            matches!(signals[0], ScoringSignal::Positive { .. }),
            "High confidence must emit Positive signal"
        );
    }

    #[test]
    fn test_sast_scan_result_low_confidence_emits_negative_signal() {
        let m = make_match("rust-md5-usage", Severity::Warning, Confidence::Low, None);
        let result = make_result_with_match(m);
        let signals = result.signals();
        assert_eq!(signals.len(), 1);
        assert!(
            matches!(signals[0], ScoringSignal::Negative { .. }),
            "Low confidence must emit Negative signal"
        );
    }

    #[test]
    fn test_sast_scan_result_medium_confidence_emits_no_signal() {
        let m = make_match(
            "rust-md5-usage",
            Severity::Warning,
            Confidence::Medium,
            None,
        );
        let result = make_result_with_match(m);
        assert!(
            result.signals().is_empty(),
            "Medium confidence must not emit any signal"
        );
    }

    #[test]
    fn test_sast_scan_result_cwe327_error_emits_absolute_violation() {
        let m = make_match(
            "rust-weak-crypto",
            Severity::Error,
            Confidence::High,
            Some(vec!["CWE-327".to_string()]),
        );
        let result = make_result_with_match(m);
        let signals = result.signals();
        assert_eq!(signals.len(), 1);
        assert!(
            signals[0].is_absolute_violation(),
            "CWE-327 Error must emit AbsoluteViolation"
        );
    }

    #[test]
    fn test_sast_scan_result_cwe327_warning_does_not_emit_absolute_violation() {
        // CWE-327 on a Warning-severity match must NOT trigger a violation;
        // the High confidence should produce a Positive signal instead.
        let m = make_match(
            "rust-weak-crypto",
            Severity::Warning,
            Confidence::High,
            Some(vec!["CWE-327".to_string()]),
        );
        let result = make_result_with_match(m);
        let signals = result.signals();
        assert_eq!(signals.len(), 1);
        assert!(
            matches!(signals[0], ScoringSignal::Positive { .. }),
            "CWE-327 Warning with High confidence must emit Positive, not AbsoluteViolation"
        );
    }

    #[test]
    fn test_sast_scan_result_plugin_name_is_sast_scan() {
        let result = SastScanResult {
            matches: vec![],
            ruleset_ids: vec![],
            scan_root: PathBuf::from("/repo"),
            duration_ms: 0,
        };
        assert_eq!(result.plugin_name(), "sast_scan");
    }

    #[test]
    fn test_sast_scan_result_context_for_ai_contains_match_count() {
        let m = make_match("rust-md5-usage", Severity::Warning, Confidence::High, None);
        let result = make_result_with_match(m);
        let ctx = result.context_for_ai();
        assert!(
            ctx.contains("1 matches"),
            "context must report total match count"
        );
    }

    // ------------------------------------------------------------------
    // Required scoring integration tests (per src/plugins/AGENTS.md)
    // ------------------------------------------------------------------

    #[test]
    fn test_absolute_violation_passes_filter_regardless_of_threshold() {
        // Construct a SastScanResult with one CWE-326 Error-severity match.
        // Set confidence_threshold effectively to 1.0 so nothing passes on
        // score alone. Assert the finding is retained because has_violations()
        // returns true.
        let m = make_match(
            "rust-weak-cipher",
            Severity::Error,
            Confidence::High,
            Some(vec!["CWE-326".to_string()]),
        );
        let result = make_result_with_match(m);

        let scorer = ConfidenceScorer::new(ScoringConfig {
            ai_confidence_weight: 0.0,
            ai_analysis_enabled: false,
            review_violations: false,
        });
        let scoring_result = scorer.score(&result, None);

        assert!(
            scoring_result.has_violations(),
            "CWE-326 Error match must register as a violation"
        );
        // The filter condition: even at threshold=1.0, has_violations() bypasses it.
        let passes_filter = scoring_result.has_violations() || scoring_result.blended_score >= 1.0;
        assert!(
            passes_filter,
            "AbsoluteViolation must bypass the confidence threshold filter"
        );
    }

    #[test]
    fn test_ai_confidence_weight_shifts_blended_score() {
        // A High-confidence match produces a Positive signal but static_score
        // stays 1.0 (the ceiling). To make the AI weight visible, we use an
        // ai_score lower than the static score (0.5) so the blend formula
        // produces different results at weight=0.0 vs weight=1.0.
        //
        // weight=0.0: blended = 1.0*(1-0) + 0.5*0   = 1.0
        // weight=1.0: blended = 1.0*(1-1) + 0.5*1   = 0.5
        // 1.0 != 0.5 => scores differ, demonstrating the weight effect.
        let m = make_match("rust-md5-usage", Severity::Warning, Confidence::High, None);
        let result = make_result_with_match(m);

        let scorer_zero = ConfidenceScorer::new(ScoringConfig {
            ai_confidence_weight: 0.0,
            ai_analysis_enabled: true,
            review_violations: false,
        });
        // ai_score=Some(0.5) is below static=1.0 so the weights produce a
        // visible difference.
        let result_zero = scorer_zero.score(&result, Some(0.5));

        let scorer_one = ConfidenceScorer::new(ScoringConfig {
            ai_confidence_weight: 1.0,
            ai_analysis_enabled: true,
            review_violations: false,
        });
        let result_one = scorer_one.score(&result, Some(0.5));

        assert!(
            (result_zero.blended_score - result_one.blended_score).abs() > f64::EPSILON,
            "blended scores must differ when ai_confidence_weight differs: {} vs {}",
            result_zero.blended_score,
            result_one.blended_score,
        );
    }

    #[test]
    fn test_ai_analysis_disabled_yields_none_ai_score_and_blended_equals_static() {
        // A Low-confidence match produces a Negative signal, reducing static
        // below 1.0. With ai_analysis_enabled=false, even though ai_score is
        // provided, it must be ignored: blended must equal static.
        let m = make_match("rust-md5-usage", Severity::Warning, Confidence::Low, None);
        let result = make_result_with_match(m);

        let scorer = ConfidenceScorer::new(ScoringConfig {
            ai_confidence_weight: 0.5,
            ai_analysis_enabled: false,
            review_violations: false,
        });
        // Pass a non-None ai_score to verify it is suppressed when AI is off.
        let scoring_result = scorer.score(&result, Some(1.0));

        assert!(
            scoring_result.ai_score.is_none(),
            "ai_score must be None when ai_analysis_enabled is false"
        );
        assert!(
            (scoring_result.blended_score - scoring_result.static_score).abs() < f64::EPSILON,
            "blended_score must equal static_score when AI is disabled"
        );
    }
}
