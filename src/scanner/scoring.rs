//! Baseline-fold confidence scoring for scan findings.
//!
//! This module provides the redesigned scoring core built around three
//! cooperating types:
//!
//! - [`ScoringSignal`]: an enum whose `Positive`, `Negative`, and
//!   `AbsoluteViolation` variants compose to produce a static confidence score
//!   via a sequential baseline fold.
//! - [`ScoringInput`]: a trait that findings or other scoreable items implement
//!   to expose their ordered signal list, their AI context string, and the name
//!   of the plugin that produced them.
//! - [`ConfidenceScorer`]: the engine that runs the baseline fold, handles
//!   `AbsoluteViolation` short-circuits, and blends the static result with an
//!   optional AI-reported confidence using configurable weights.
//!
//! ## Baseline-fold algorithm
//!
//! The fold starts at `1.0` (full confidence) and processes each signal in
//! insertion order:
//!
//! - `Negative { weight }`: proportionally deducts from the current score.
//!   `score = score * (1.0 - w)` where `w = weight.clamp(0.0, 1.0)`.
//! - `Positive { weight }`: proportionally recovers toward the ceiling.
//!   `score = score + (1.0 - score) * w` where `w = weight.clamp(0.0, 1.0)`.
//! - `AbsoluteViolation`: immediately short-circuits to [`VIOLATION_FLOOR`]
//!   (`0.0`) unless `review_violations` mode is active, in which case the
//!   violation is rewritten to a heavily-weighted `Negative` of weight
//!   [`VIOLATION_REVIEW_WEIGHT`] and folded normally so an AI leg can still
//!   contribute.
//!
//! ## AI blend
//!
//! After the static fold, if `ai_analysis_enabled` is `true` and a valid AI
//! score is available:
//!
//! ```text
//! blended = static_score * (1.0 - w) + ai_score * w
//! ```
//!
//! where `w = ai_confidence_weight.clamp(0.0, 1.0)`. Any AI-side failure
//! (missing score, parse error, provider outage represented as `None`) leaves
//! the blended result equal to the static score unchanged.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Floor value applied when an [`ScoringSignal::AbsoluteViolation`] is
/// encountered and `review_violations` mode is inactive.
///
/// The value `0.0` represents zero confidence in the finding's legitimacy
/// from a static-analysis perspective.
pub const VIOLATION_FLOOR: f64 = 0.0;

/// Effective `Negative` weight applied to an [`ScoringSignal::AbsoluteViolation`]
/// signal when `review_violations` mode is active.
///
/// This is intentionally high (`0.9`) so the static score is heavily penalised
/// while still allowing an AI leg to move the blended result above `0.0`.
pub const VIOLATION_REVIEW_WEIGHT: f64 = 0.9;

// ---------------------------------------------------------------------------
// ScoringSignal
// ---------------------------------------------------------------------------

/// A signal that contributes to a confidence score via the baseline fold.
///
/// Signals are processed in order by [`ConfidenceScorer::score_static`]. The
/// fold starts at `1.0` and each signal moves it proportionally toward or
/// away from that ceiling.
///
/// # Variants
///
/// - [`Positive`][Self::Positive]: increases the score proportionally toward
///   the ceiling.
/// - [`Negative`][Self::Negative]: decreases the score proportionally.
/// - [`AbsoluteViolation`][Self::AbsoluteViolation]: immediately floors the
///   score to [`VIOLATION_FLOOR`] (or folds as a heavy `Negative` when
///   `review_violations` is active).
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::scoring::ScoringSignal;
///
/// let pos = ScoringSignal::Positive { label: "pattern_match".into(), weight: 0.4 };
/// let neg = ScoringSignal::Negative { label: "severity_high".into(), weight: 0.5 };
/// let vio = ScoringSignal::AbsoluteViolation {
///     reason: "hardcoded credential detected".into(),
/// };
///
/// assert!(!pos.is_absolute_violation());
/// assert!(!neg.is_absolute_violation());
/// assert!(vio.is_absolute_violation());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScoringSignal {
    /// Proportionally increases the current score toward the ceiling.
    ///
    /// `new_score = score + (1.0 - score) * weight.clamp(0.0, 1.0)`
    Positive {
        /// Human-readable name for this signal.
        label: String,
        /// Proportional recovery strength; clamped to `[0.0, 1.0]` during scoring.
        weight: f64,
    },
    /// Proportionally decreases the current score.
    ///
    /// `new_score = score * (1.0 - weight.clamp(0.0, 1.0))`
    Negative {
        /// Human-readable name for this signal.
        label: String,
        /// Proportional deduction strength; clamped to `[0.0, 1.0]` during scoring.
        weight: f64,
    },
    /// Hard violation that immediately floors the score to [`VIOLATION_FLOOR`].
    ///
    /// When `review_violations` mode is active in [`ScoringConfig`], this is
    /// rewritten to a heavily-weighted [`Negative`][Self::Negative] of weight
    /// [`VIOLATION_REVIEW_WEIGHT`] so the fold continues and an AI leg can
    /// still weigh in.
    AbsoluteViolation {
        /// Human-readable description of the deterministic rule breach.
        reason: String,
    },
}

impl ScoringSignal {
    /// Returns the display label or reason string for this signal.
    ///
    /// For [`Positive`][Self::Positive] and [`Negative`][Self::Negative]
    /// variants this returns the `label` field.  For
    /// [`AbsoluteViolation`][Self::AbsoluteViolation] it returns the `reason`.
    ///
    /// # Returns
    ///
    /// A string slice identifying this signal.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::scoring::ScoringSignal;
    ///
    /// let sig = ScoringSignal::Positive { label: "pattern_match".into(), weight: 0.3 };
    /// assert_eq!(sig.display_label(), "pattern_match");
    ///
    /// let vio = ScoringSignal::AbsoluteViolation { reason: "hardcoded credential".into() };
    /// assert_eq!(vio.display_label(), "hardcoded credential");
    /// ```
    pub fn display_label(&self) -> &str {
        match self {
            ScoringSignal::Positive { label, .. } => label,
            ScoringSignal::Negative { label, .. } => label,
            ScoringSignal::AbsoluteViolation { reason } => reason,
        }
    }

    /// Returns `true` if this signal is an [`AbsoluteViolation`][Self::AbsoluteViolation].
    ///
    /// # Returns
    ///
    /// `true` when the variant is `AbsoluteViolation`, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::scoring::ScoringSignal;
    ///
    /// let vio = ScoringSignal::AbsoluteViolation { reason: "credential leaked".into() };
    /// assert!(vio.is_absolute_violation());
    ///
    /// let neg = ScoringSignal::Negative { label: "severity".into(), weight: 0.3 };
    /// assert!(!neg.is_absolute_violation());
    /// ```
    pub fn is_absolute_violation(&self) -> bool {
        matches!(self, ScoringSignal::AbsoluteViolation { .. })
    }
}

// ---------------------------------------------------------------------------
// ScoringInput
// ---------------------------------------------------------------------------

/// A trait for items that expose scoring signals to [`ConfidenceScorer`].
///
/// Implement this trait for finding types that participate in the confidence
/// scoring pipeline.  The scorer calls [`signals`][Self::signals] to obtain
/// the ordered list of signals for the baseline fold, and
/// [`context_for_ai`][Self::context_for_ai] when constructing an AI prompt.
///
/// Both [`ai_confidence_weight`][crate::scanner::scoring::ScoringConfig::ai_confidence_weight]
/// and [`ai_analysis_enabled`][crate::scanner::scoring::ScoringConfig::ai_analysis_enabled]
/// must be exposed by every consumer of this trait from its first version.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::scoring::{ScoringInput, ScoringSignal};
///
/// struct MyFinding {
///     severity_weight: f64,
/// }
///
/// impl ScoringInput for MyFinding {
///     fn signals(&self) -> Vec<ScoringSignal> {
///         vec![ScoringSignal::Negative {
///             label: "severity".into(),
///             weight: self.severity_weight,
///         }]
///     }
///
///     fn context_for_ai(&self) -> String {
///         "Unsanitized user input passed to SQL query.".to_string()
///     }
///
///     fn plugin_name(&self) -> &str {
///         "my_plugin"
///     }
/// }
///
/// let finding = MyFinding { severity_weight: 0.4 };
/// assert_eq!(finding.plugin_name(), "my_plugin");
/// assert_eq!(finding.signals().len(), 1);
/// ```
pub trait ScoringInput {
    /// Returns the ordered list of scoring signals for this item.
    ///
    /// Signals are processed left-to-right by [`ConfidenceScorer::score_static`].
    fn signals(&self) -> Vec<ScoringSignal>;

    /// Returns a human-readable context string for inclusion in an AI prompt.
    ///
    /// The string should describe the finding in enough detail for an AI to
    /// estimate its confidence independently of the static signals.
    fn context_for_ai(&self) -> String;

    /// Returns the name of the plugin that produced this scoreable item.
    fn plugin_name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// ScoringConfig
// ---------------------------------------------------------------------------

/// Configuration for [`ConfidenceScorer`].
///
/// Controls how the AI confidence leg is weighted against the static signal
/// fold and whether `AbsoluteViolation` signals are hard-floored or blended.
/// Both `ai_confidence_weight` and `ai_analysis_enabled` are first-class
/// configuration fields introduced here for every consumer to expose.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::scoring::ScoringConfig;
///
/// let cfg = ScoringConfig::default();
/// assert!((cfg.ai_confidence_weight - 0.5).abs() < f64::EPSILON);
/// assert!(cfg.ai_analysis_enabled);
/// assert!(!cfg.review_violations);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConfig {
    /// Weight of the AI score in the final blend, in `[0.0, 1.0]`.
    ///
    /// Clamped to `[0.0, 1.0]` at blend time.  At `0.0` the AI score has no
    /// effect on the blended result; at `1.0` only the AI score matters.
    ///
    /// Default: `0.5`.
    pub ai_confidence_weight: f64,

    /// Master switch for the AI blend leg.
    ///
    /// When `false`, the AI blend is skipped entirely: the final blended score
    /// equals the static score regardless of any provided AI input.
    ///
    /// Default: `true`.
    pub ai_analysis_enabled: bool,

    /// When `true`, [`ScoringSignal::AbsoluteViolation`] signals are treated
    /// as heavily-weighted [`ScoringSignal::Negative`] signals of weight
    /// [`VIOLATION_REVIEW_WEIGHT`] instead of immediately flooring the score.
    ///
    /// This allows the AI leg to contribute even when a deterministic rule
    /// breach is present.
    ///
    /// Default: `false`.
    pub review_violations: bool,
}

impl Default for ScoringConfig {
    /// Returns a [`ScoringConfig`] with default values.
    ///
    /// - `ai_confidence_weight`: `0.5`
    /// - `ai_analysis_enabled`: `true`
    /// - `review_violations`: `false`
    fn default() -> Self {
        Self {
            ai_confidence_weight: 0.5,
            ai_analysis_enabled: true,
            review_violations: false,
        }
    }
}

// ---------------------------------------------------------------------------
// ScoringResult
// ---------------------------------------------------------------------------

/// The output of a full [`ConfidenceScorer::score`] run.
///
/// Records the pure static score, the raw AI score (if used), the blended
/// final score, and any absolute violation reasons encountered during the
/// static fold.  This provides a complete audit trail for every scoring
/// decision.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::scoring::ScoringResult;
///
/// let result = ScoringResult {
///     static_score: 0.7,
///     ai_score: Some(0.85),
///     blended_score: 0.775,
///     violation_reasons: vec![],
/// };
/// assert!(!result.has_violations());
/// assert!((result.blended_score - 0.775).abs() < 1e-9);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoringResult {
    /// Score derived exclusively from the static baseline fold.
    ///
    /// Always in `[0.0, 1.0]`.
    pub static_score: f64,

    /// Raw confidence reported by the AI provider, if AI analysis was active
    /// and a valid score was available.
    ///
    /// `None` when `ai_analysis_enabled` is `false`, when the AI is not
    /// configured, or when the AI call fails (provider error, malformed JSON,
    /// etc.).  Any of these conditions cause the blended score to equal the
    /// static score.
    pub ai_score: Option<f64>,

    /// The final blended confidence score combining the static and AI legs.
    ///
    /// Equals `static_score` when no AI score was available or when
    /// `ai_analysis_enabled` is `false`.  Always in `[0.0, 1.0]`.
    pub blended_score: f64,

    /// Reasons collected from every [`ScoringSignal::AbsoluteViolation`]
    /// signal encountered during the static fold.
    ///
    /// Empty when no violations were present.  In non-`review_violations`
    /// mode, a non-empty list also implies `static_score == VIOLATION_FLOOR`.
    pub violation_reasons: Vec<String>,
}

impl ScoringResult {
    /// Returns `true` if any [`ScoringSignal::AbsoluteViolation`] was
    /// encountered during the static fold.
    ///
    /// # Returns
    ///
    /// `true` when `violation_reasons` is non-empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::scoring::ScoringResult;
    ///
    /// let clean = ScoringResult {
    ///     static_score: 0.8,
    ///     ai_score: None,
    ///     blended_score: 0.8,
    ///     violation_reasons: vec![],
    /// };
    /// assert!(!clean.has_violations());
    ///
    /// let dirty = ScoringResult {
    ///     static_score: 0.0,
    ///     ai_score: None,
    ///     blended_score: 0.0,
    ///     violation_reasons: vec!["hardcoded credential detected".to_string()],
    /// };
    /// assert!(dirty.has_violations());
    /// ```
    pub fn has_violations(&self) -> bool {
        !self.violation_reasons.is_empty()
    }
}

// ---------------------------------------------------------------------------
// ConfidenceScorer
// ---------------------------------------------------------------------------

/// Computes confidence scores via the baseline-fold algorithm.
///
/// Construct with [`new`][Self::new] (custom config) or
/// [`with_defaults`][Self::with_defaults] (default config).  Then call:
///
/// - [`score_static`][Self::score_static]: pure static fold over a signal
///   slice; returns the clamped score and any violation reasons.
/// - [`blend`][Self::blend]: applies the AI weight formula to a pre-computed
///   static score.
/// - [`score`][Self::score]: full pipeline using a [`ScoringInput`]
///   implementation; returns a [`ScoringResult`] with all audit fields.
///
/// # Guarantees
///
/// - Never panics on empty or degenerate input.
/// - An AI-side failure (any `None` AI score) always produces a `blended_score`
///   equal to `static_score`.
/// - All returned scores are clamped to `[0.0, 1.0]`.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::scoring::{ConfidenceScorer, ScoringSignal};
///
/// let scorer = ConfidenceScorer::with_defaults();
/// let signals = vec![
///     ScoringSignal::Negative { label: "high_severity".into(), weight: 0.4 },
///     ScoringSignal::Positive { label: "pattern_confirmed".into(), weight: 0.2 },
/// ];
/// let (score, violations) = scorer.score_static(&signals);
/// // After Negative(0.4): 1.0 * 0.6 = 0.6
/// // After Positive(0.2): 0.6 + (1.0 - 0.6) * 0.2 = 0.6 + 0.08 = 0.68
/// assert!((score - 0.68).abs() < 1e-9);
/// assert!(violations.is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct ConfidenceScorer {
    /// Scoring configuration controlling AI weight, enable flag, and
    /// violation review mode.
    pub config: ScoringConfig,
}

impl ConfidenceScorer {
    /// Creates a new [`ConfidenceScorer`] with the supplied configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Scoring configuration to use for all scoring operations.
    ///
    /// # Returns
    ///
    /// A `ConfidenceScorer` using the given `config`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::scoring::{ConfidenceScorer, ScoringConfig};
    ///
    /// let cfg = ScoringConfig { ai_confidence_weight: 0.3, ..ScoringConfig::default() };
    /// let scorer = ConfidenceScorer::new(cfg);
    /// assert!((scorer.config.ai_confidence_weight - 0.3).abs() < f64::EPSILON);
    /// ```
    pub fn new(config: ScoringConfig) -> Self {
        Self { config }
    }

    /// Creates a [`ConfidenceScorer`] with default configuration.
    ///
    /// Equivalent to `ConfidenceScorer::new(ScoringConfig::default())`.
    ///
    /// # Returns
    ///
    /// A `ConfidenceScorer` with `ai_confidence_weight = 0.5`,
    /// `ai_analysis_enabled = true`, and `review_violations = false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::scoring::ConfidenceScorer;
    ///
    /// let scorer = ConfidenceScorer::with_defaults();
    /// assert!((scorer.config.ai_confidence_weight - 0.5).abs() < f64::EPSILON);
    /// assert!(scorer.config.ai_analysis_enabled);
    /// ```
    pub fn with_defaults() -> Self {
        Self::new(ScoringConfig::default())
    }

    /// Runs the baseline fold on a slice of [`ScoringSignal`]s.
    ///
    /// The fold starts at `1.0` and processes signals in order:
    ///
    /// - `Negative { weight }`: `score *= 1.0 - weight.clamp(0.0, 1.0)`
    /// - `Positive { weight }`: `score += (1.0 - score) * weight.clamp(0.0, 1.0)`
    /// - `AbsoluteViolation { reason }`:
    ///   - `review_violations = false` (default): immediately returns
    ///     `(VIOLATION_FLOOR, reasons_so_far)`.
    ///   - `review_violations = true`: folds as `Negative { weight:
    ///     VIOLATION_REVIEW_WEIGHT }` and records the reason; fold continues.
    ///
    /// The result score is clamped to `[0.0, 1.0]` before returning.
    ///
    /// # Arguments
    ///
    /// * `signals` - Ordered slice of signals to fold.
    ///
    /// # Returns
    ///
    /// A tuple `(static_score, violation_reasons)` where:
    /// - `static_score` is in `[0.0, 1.0]`.
    /// - `violation_reasons` lists every `AbsoluteViolation` reason encountered.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::scoring::{
    ///     ConfidenceScorer, ScoringSignal, VIOLATION_FLOOR,
    /// };
    ///
    /// let scorer = ConfidenceScorer::with_defaults();
    ///
    /// // Empty signals return full confidence with no violations.
    /// let (score, violations) = scorer.score_static(&[]);
    /// assert!((score - 1.0).abs() < f64::EPSILON);
    /// assert!(violations.is_empty());
    ///
    /// // A single AbsoluteViolation floors the score immediately.
    /// let signals = vec![ScoringSignal::AbsoluteViolation { reason: "cred_leak".into() }];
    /// let (score, reasons) = scorer.score_static(&signals);
    /// assert!((score - VIOLATION_FLOOR).abs() < f64::EPSILON);
    /// assert_eq!(reasons, vec!["cred_leak"]);
    /// ```
    pub fn score_static(&self, signals: &[ScoringSignal]) -> (f64, Vec<String>) {
        let mut score: f64 = 1.0;
        let mut violations: Vec<String> = Vec::new();

        for signal in signals {
            match signal {
                ScoringSignal::Positive { weight, .. } => {
                    let w = weight.clamp(0.0, 1.0);
                    score += (1.0 - score) * w;
                }
                ScoringSignal::Negative { weight, .. } => {
                    let w = weight.clamp(0.0, 1.0);
                    score *= 1.0 - w;
                }
                ScoringSignal::AbsoluteViolation { reason } => {
                    violations.push(reason.clone());
                    if self.config.review_violations {
                        score *= 1.0 - VIOLATION_REVIEW_WEIGHT;
                    } else {
                        return (VIOLATION_FLOOR, violations);
                    }
                }
            }
        }

        (score.clamp(0.0, 1.0), violations)
    }

    /// Blends a static score with an optional AI score.
    ///
    /// When `ai_analysis_enabled` is `false` or `ai_score` is `None`,
    /// returns `static_score` unchanged.  This guarantees that any AI-side
    /// failure (provider outage, malformed JSON, missing score) leaves the
    /// result equal to what a static-only run produces.
    ///
    /// When blending is active:
    ///
    /// ```text
    /// blended = static_score * (1.0 - w) + ai_score * w
    /// ```
    ///
    /// where `w = ai_confidence_weight.clamp(0.0, 1.0)`.
    ///
    /// The result is clamped to `[0.0, 1.0]`.
    ///
    /// # Arguments
    ///
    /// * `static_score` - Pre-computed static fold score in `[0.0, 1.0]`.
    /// * `ai_score`     - AI-reported confidence, or `None` on any failure.
    ///
    /// # Returns
    ///
    /// Blended score in `[0.0, 1.0]`, or `static_score` when AI is
    /// unavailable or disabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::scoring::ConfidenceScorer;
    ///
    /// let scorer = ConfidenceScorer::with_defaults();
    ///
    /// // Provider error: ai_score is None; returns static_score unchanged.
    /// assert!((scorer.blend(0.6, None) - 0.6).abs() < f64::EPSILON);
    ///
    /// // w=0.5, static=0.6, ai=1.0 => 0.6 * 0.5 + 1.0 * 0.5 = 0.8
    /// assert!((scorer.blend(0.6, Some(1.0)) - 0.8).abs() < 1e-9);
    /// ```
    pub fn blend(&self, static_score: f64, ai_score: Option<f64>) -> f64 {
        if !self.config.ai_analysis_enabled {
            return static_score;
        }
        let Some(ai) = ai_score else {
            return static_score;
        };
        let w = self.config.ai_confidence_weight.clamp(0.0, 1.0);
        (static_score * (1.0 - w) + ai * w).clamp(0.0, 1.0)
    }

    /// Runs the full scoring pipeline using a [`ScoringInput`] implementation.
    ///
    /// Steps:
    /// 1. Calls [`score_static`][Self::score_static] on the signals from
    ///    `input.signals()`.
    /// 2. Calls [`blend`][Self::blend] with the static result and `ai_score`.
    /// 3. Returns a [`ScoringResult`] with the static score, the AI score
    ///    (when AI analysis is active and a score was provided), the blended
    ///    score, and any violation reasons.
    ///
    /// # Arguments
    ///
    /// * `input`    - The item to score, implementing [`ScoringInput`].
    /// * `ai_score` - AI-reported confidence in `[0.0, 1.0]`, or `None` on
    ///   any provider failure or when AI analysis is disabled upstream.
    ///
    /// # Returns
    ///
    /// A [`ScoringResult`] containing all scoring audit fields.  The
    /// `blended_score` always equals `static_score` when `ai_score` is `None`
    /// or `ai_analysis_enabled` is `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::scoring::{
    ///     ConfidenceScorer, ScoringInput, ScoringSignal,
    /// };
    ///
    /// struct TestInput;
    ///
    /// impl ScoringInput for TestInput {
    ///     fn signals(&self) -> Vec<ScoringSignal> {
    ///         vec![ScoringSignal::Negative { label: "severity_high".into(), weight: 0.4 }]
    ///     }
    ///     fn context_for_ai(&self) -> String { "test context".to_string() }
    ///     fn plugin_name(&self) -> &str { "test_plugin" }
    /// }
    ///
    /// let scorer = ConfidenceScorer::with_defaults();
    ///
    /// // Provider error: ai_score is None; blended == static.
    /// let result = scorer.score(&TestInput, None);
    /// assert!((result.blended_score - result.static_score).abs() < f64::EPSILON);
    /// assert!(!result.has_violations());
    /// ```
    pub fn score(&self, input: &dyn ScoringInput, ai_score: Option<f64>) -> ScoringResult {
        let signals = input.signals();
        let (static_score, violation_reasons) = self.score_static(&signals);
        let blended_score = self.blend(static_score, ai_score);
        // Only surface the AI score in the result when AI analysis was active.
        // When ai_analysis_enabled = false, the blend is skipped and no AI
        // score should appear in the audit record.
        let effective_ai = if self.config.ai_analysis_enabled {
            ai_score
        } else {
            None
        };
        ScoringResult {
            static_score,
            ai_score: effective_ai,
            blended_score,
            violation_reasons,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Test helpers
    // ------------------------------------------------------------------

    /// A minimal [`ScoringInput`] implementation for tests.
    struct TestInput {
        signals: Vec<ScoringSignal>,
    }

    impl TestInput {
        fn new(signals: Vec<ScoringSignal>) -> Self {
            Self { signals }
        }
    }

    impl ScoringInput for TestInput {
        fn signals(&self) -> Vec<ScoringSignal> {
            self.signals.clone()
        }

        fn context_for_ai(&self) -> String {
            "test context".to_string()
        }

        fn plugin_name(&self) -> &str {
            "test_plugin"
        }
    }

    fn default_scorer() -> ConfidenceScorer {
        ConfidenceScorer::with_defaults()
    }

    // ------------------------------------------------------------------
    // ScoringSignal
    // ------------------------------------------------------------------

    #[test]
    fn test_scoring_signal_display_label_positive_returns_label() {
        let sig = ScoringSignal::Positive {
            label: "pattern_match".into(),
            weight: 0.3,
        };
        assert_eq!(sig.display_label(), "pattern_match");
    }

    #[test]
    fn test_scoring_signal_display_label_negative_returns_label() {
        let sig = ScoringSignal::Negative {
            label: "severity_high".into(),
            weight: 0.5,
        };
        assert_eq!(sig.display_label(), "severity_high");
    }

    #[test]
    fn test_scoring_signal_display_label_absolute_violation_returns_reason() {
        let sig = ScoringSignal::AbsoluteViolation {
            reason: "hardcoded credential".into(),
        };
        assert_eq!(sig.display_label(), "hardcoded credential");
    }

    #[test]
    fn test_scoring_signal_is_absolute_violation_returns_true_for_violation() {
        let vio = ScoringSignal::AbsoluteViolation {
            reason: "credential leaked".into(),
        };
        assert!(vio.is_absolute_violation());
    }

    #[test]
    fn test_scoring_signal_is_absolute_violation_returns_false_for_positive() {
        let sig = ScoringSignal::Positive {
            label: "match".into(),
            weight: 0.2,
        };
        assert!(!sig.is_absolute_violation());
    }

    #[test]
    fn test_scoring_signal_is_absolute_violation_returns_false_for_negative() {
        let sig = ScoringSignal::Negative {
            label: "severity".into(),
            weight: 0.4,
        };
        assert!(!sig.is_absolute_violation());
    }

    // ------------------------------------------------------------------
    // ScoringConfig
    // ------------------------------------------------------------------

    #[test]
    fn test_scoring_config_default_ai_weight_is_half() {
        let cfg = ScoringConfig::default();
        assert!((cfg.ai_confidence_weight - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_scoring_config_default_ai_analysis_enabled_is_true() {
        let cfg = ScoringConfig::default();
        assert!(cfg.ai_analysis_enabled);
    }

    #[test]
    fn test_scoring_config_default_review_violations_is_false() {
        let cfg = ScoringConfig::default();
        assert!(!cfg.review_violations);
    }

    // ------------------------------------------------------------------
    // ScoringResult
    // ------------------------------------------------------------------

    #[test]
    fn test_scoring_result_has_violations_with_empty_reasons_returns_false() {
        let result = ScoringResult {
            static_score: 0.8,
            ai_score: None,
            blended_score: 0.8,
            violation_reasons: vec![],
        };
        assert!(!result.has_violations());
    }

    #[test]
    fn test_scoring_result_has_violations_with_non_empty_reasons_returns_true() {
        let result = ScoringResult {
            static_score: 0.0,
            ai_score: None,
            blended_score: 0.0,
            violation_reasons: vec!["credential detected".to_string()],
        };
        assert!(result.has_violations());
    }

    // ------------------------------------------------------------------
    // ConfidenceScorer construction
    // ------------------------------------------------------------------

    #[test]
    fn test_confidence_scorer_new_stores_config_fields() {
        let cfg = ScoringConfig {
            ai_confidence_weight: 0.3,
            ai_analysis_enabled: false,
            review_violations: true,
        };
        let scorer = ConfidenceScorer::new(cfg);
        assert!((scorer.config.ai_confidence_weight - 0.3).abs() < f64::EPSILON);
        assert!(!scorer.config.ai_analysis_enabled);
        assert!(scorer.config.review_violations);
    }

    #[test]
    fn test_confidence_scorer_with_defaults_uses_default_config() {
        let scorer = ConfidenceScorer::with_defaults();
        assert!((scorer.config.ai_confidence_weight - 0.5).abs() < f64::EPSILON);
        assert!(scorer.config.ai_analysis_enabled);
        assert!(!scorer.config.review_violations);
    }

    // ------------------------------------------------------------------
    // score_static: empty and zero-weight inputs
    // ------------------------------------------------------------------

    #[test]
    fn test_score_static_with_empty_signals_returns_one() {
        let (score, violations) = default_scorer().score_static(&[]);
        assert!((score - 1.0).abs() < f64::EPSILON);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_score_static_with_zero_weight_negative_has_no_effect() {
        let signals = vec![ScoringSignal::Negative {
            label: "no_op".into(),
            weight: 0.0,
        }];
        let (score, violations) = default_scorer().score_static(&signals);
        assert!((score - 1.0).abs() < f64::EPSILON);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_score_static_with_zero_weight_positive_has_no_effect() {
        // First reduce the score, then apply a zero-weight positive.
        let signals = vec![
            ScoringSignal::Negative {
                label: "reduce".into(),
                weight: 0.5,
            },
            ScoringSignal::Positive {
                label: "no_op".into(),
                weight: 0.0,
            },
        ];
        let (score, _) = default_scorer().score_static(&signals);
        // Only the Negative(0.5) applies: 1.0 * (1.0 - 0.5) = 0.5
        assert!((score - 0.5).abs() < 1e-9);
    }

    // ------------------------------------------------------------------
    // score_static: single signal arithmetic
    // ------------------------------------------------------------------

    #[test]
    fn test_score_static_with_single_negative_signal_reduces_score() {
        let signals = vec![ScoringSignal::Negative {
            label: "severity_high".into(),
            weight: 0.4,
        }];
        let (score, violations) = default_scorer().score_static(&signals);
        // 1.0 * (1.0 - 0.4) = 0.6
        assert!((score - 0.6).abs() < 1e-9);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_score_static_with_single_positive_at_ceiling_has_no_effect() {
        // Starting at 1.0, a Positive cannot go higher.
        let signals = vec![ScoringSignal::Positive {
            label: "pattern_confirmed".into(),
            weight: 0.5,
        }];
        let (score, violations) = default_scorer().score_static(&signals);
        // 1.0 + (1.0 - 1.0) * 0.5 = 1.0
        assert!((score - 1.0).abs() < f64::EPSILON);
        assert!(violations.is_empty());
    }

    // ------------------------------------------------------------------
    // score_static: compound signals
    // ------------------------------------------------------------------

    #[test]
    fn test_score_static_with_negative_then_positive_recovers_partially() {
        let signals = vec![
            ScoringSignal::Negative {
                label: "sev".into(),
                weight: 0.5,
            },
            ScoringSignal::Positive {
                label: "confirmed".into(),
                weight: 0.4,
            },
        ];
        let (score, violations) = default_scorer().score_static(&signals);
        // After Negative(0.5): 1.0 * 0.5 = 0.5
        // After Positive(0.4): 0.5 + (1.0 - 0.5) * 0.4 = 0.5 + 0.2 = 0.7
        assert!((score - 0.7).abs() < 1e-9);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_score_static_with_multiple_negatives_compounds_reduction() {
        let signals = vec![
            ScoringSignal::Negative {
                label: "n1".into(),
                weight: 0.5,
            },
            ScoringSignal::Negative {
                label: "n2".into(),
                weight: 0.5,
            },
        ];
        let (score, violations) = default_scorer().score_static(&signals);
        // 1.0 * 0.5 * 0.5 = 0.25
        assert!((score - 0.25).abs() < 1e-9);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_score_static_with_multiple_positives_compounds_recovery() {
        // Reduce first so Positives have room to recover.
        let signals = vec![
            ScoringSignal::Negative {
                label: "base".into(),
                weight: 0.8,
            },
            ScoringSignal::Positive {
                label: "p1".into(),
                weight: 0.5,
            },
            ScoringSignal::Positive {
                label: "p2".into(),
                weight: 0.5,
            },
        ];
        let (score, _) = default_scorer().score_static(&signals);
        // After Negative(0.8): 1.0 * 0.2 = 0.2
        // After Positive(0.5): 0.2 + 0.8 * 0.5 = 0.2 + 0.4 = 0.6
        // After Positive(0.5): 0.6 + 0.4 * 0.5 = 0.6 + 0.2 = 0.8
        assert!((score - 0.8).abs() < 1e-9);
    }

    // ------------------------------------------------------------------
    // score_static: AbsoluteViolation - floor mode (default)
    // ------------------------------------------------------------------

    #[test]
    fn test_score_static_with_absolute_violation_floors_to_violation_floor() {
        let signals = vec![ScoringSignal::AbsoluteViolation {
            reason: "hardcoded_api_key".into(),
        }];
        let (score, _) = default_scorer().score_static(&signals);
        assert!((score - VIOLATION_FLOOR).abs() < f64::EPSILON);
    }

    #[test]
    fn test_score_static_with_absolute_violation_records_violation_reason() {
        let signals = vec![ScoringSignal::AbsoluteViolation {
            reason: "hardcoded_api_key".into(),
        }];
        let (_, violations) = default_scorer().score_static(&signals);
        assert_eq!(violations, vec!["hardcoded_api_key"]);
    }

    #[test]
    fn test_score_static_with_absolute_violation_short_circuits_remaining_signals() {
        // A Positive after the violation should never be reached.
        let signals = vec![
            ScoringSignal::AbsoluteViolation {
                reason: "cred".into(),
            },
            ScoringSignal::Positive {
                label: "should_not_run".into(),
                weight: 1.0,
            },
        ];
        let (score, violations) = default_scorer().score_static(&signals);
        assert!((score - VIOLATION_FLOOR).abs() < f64::EPSILON);
        // Only one reason should be recorded; the Positive does not add one.
        assert_eq!(violations.len(), 1);
    }

    // ------------------------------------------------------------------
    // score_static: AbsoluteViolation - review_violations mode
    // ------------------------------------------------------------------

    #[test]
    fn test_score_static_review_violations_mode_folds_instead_of_flooring() {
        let cfg = ScoringConfig {
            review_violations: true,
            ..ScoringConfig::default()
        };
        let scorer = ConfidenceScorer::new(cfg);
        let signals = vec![ScoringSignal::AbsoluteViolation {
            reason: "cred".into(),
        }];
        let (score, _) = scorer.score_static(&signals);
        // Folded as Negative(VIOLATION_REVIEW_WEIGHT = 0.9):
        // 1.0 * (1.0 - 0.9) = 0.1
        assert!((score - 0.1).abs() < 1e-9);
    }

    #[test]
    fn test_score_static_review_violations_mode_records_reason() {
        let cfg = ScoringConfig {
            review_violations: true,
            ..ScoringConfig::default()
        };
        let scorer = ConfidenceScorer::new(cfg);
        let signals = vec![ScoringSignal::AbsoluteViolation {
            reason: "cred_leak".into(),
        }];
        let (_, violations) = scorer.score_static(&signals);
        assert_eq!(violations, vec!["cred_leak"]);
    }

    #[test]
    fn test_score_static_review_violations_multiple_violations_compound() {
        let cfg = ScoringConfig {
            review_violations: true,
            ..ScoringConfig::default()
        };
        let scorer = ConfidenceScorer::new(cfg);
        let signals = vec![
            ScoringSignal::AbsoluteViolation {
                reason: "v1".into(),
            },
            ScoringSignal::AbsoluteViolation {
                reason: "v2".into(),
            },
        ];
        let (score, violations) = scorer.score_static(&signals);
        // v1: 1.0 * 0.1 = 0.1
        // v2: 0.1 * 0.1 = 0.01
        assert!((score - 0.01).abs() < 1e-9);
        assert_eq!(violations, vec!["v1", "v2"]);
    }

    // ------------------------------------------------------------------
    // blend
    // ------------------------------------------------------------------

    #[test]
    fn test_blend_with_none_ai_score_returns_static_score() {
        let scorer = default_scorer();
        let result = scorer.blend(0.6, None);
        assert!((result - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn test_blend_with_ai_disabled_returns_static_score_ignoring_ai() {
        let cfg = ScoringConfig {
            ai_analysis_enabled: false,
            ..ScoringConfig::default()
        };
        let scorer = ConfidenceScorer::new(cfg);
        // Even though an AI score is provided, it must be ignored.
        let result = scorer.blend(0.6, Some(1.0));
        assert!((result - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn test_blend_with_ai_score_applies_default_weight() {
        let scorer = default_scorer();
        // w=0.5, static=0.6, ai=1.0 => 0.6 * 0.5 + 1.0 * 0.5 = 0.8
        let result = scorer.blend(0.6, Some(1.0));
        assert!((result - 0.8).abs() < 1e-9);
    }

    #[test]
    fn test_blend_weight_zero_uses_only_static_score() {
        let cfg = ScoringConfig {
            ai_confidence_weight: 0.0,
            ..ScoringConfig::default()
        };
        let scorer = ConfidenceScorer::new(cfg);
        let result = scorer.blend(0.6, Some(1.0));
        // 0.6 * 1.0 + 1.0 * 0.0 = 0.6
        assert!((result - 0.6).abs() < 1e-9);
    }

    #[test]
    fn test_blend_weight_one_uses_only_ai_score() {
        let cfg = ScoringConfig {
            ai_confidence_weight: 1.0,
            ..ScoringConfig::default()
        };
        let scorer = ConfidenceScorer::new(cfg);
        let result = scorer.blend(0.3, Some(0.9));
        // 0.3 * 0.0 + 0.9 * 1.0 = 0.9
        assert!((result - 0.9).abs() < 1e-9);
    }

    #[test]
    fn test_blend_clamps_result_within_unit_interval() {
        let cfg = ScoringConfig {
            ai_confidence_weight: 1.0,
            ..ScoringConfig::default()
        };
        let scorer = ConfidenceScorer::new(cfg);
        // ai_score out of range: result should still be clamped.
        let high = scorer.blend(0.0, Some(1.5));
        assert!((high - 1.0).abs() < f64::EPSILON);

        let low = scorer.blend(0.0, Some(-0.5));
        assert!((low - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_blend_overclamped_weight_is_clamped_to_one() {
        let cfg = ScoringConfig {
            // weight > 1.0 must be clamped at blend time.
            ai_confidence_weight: 2.0,
            ..ScoringConfig::default()
        };
        let scorer = ConfidenceScorer::new(cfg);
        // Effective w = 1.0 => result = ai_score
        let result = scorer.blend(0.3, Some(0.7));
        assert!((result - 0.7).abs() < 1e-9);
    }

    // ------------------------------------------------------------------
    // score (full pipeline via ScoringInput)
    // ------------------------------------------------------------------

    #[test]
    fn test_score_with_no_ai_blended_score_equals_static_score() {
        let input = TestInput::new(vec![ScoringSignal::Negative {
            label: "sev".into(),
            weight: 0.4,
        }]);
        let result = default_scorer().score(&input, None);
        assert!((result.blended_score - result.static_score).abs() < f64::EPSILON);
        assert!(result.ai_score.is_none());
    }

    #[test]
    fn test_score_with_ai_score_blends_correctly() {
        let input = TestInput::new(vec![ScoringSignal::Negative {
            label: "sev".into(),
            weight: 0.4,
        }]);
        // static = 1.0 * 0.6 = 0.6; ai = 1.0; w=0.5 => blended = 0.6*0.5 + 1.0*0.5 = 0.8
        let result = default_scorer().score(&input, Some(1.0));
        assert!((result.static_score - 0.6).abs() < 1e-9);
        assert!((result.blended_score - 0.8).abs() < 1e-9);
        assert_eq!(result.ai_score, Some(1.0));
    }

    #[test]
    fn test_score_with_absolute_violation_floors_static_score() {
        let input = TestInput::new(vec![ScoringSignal::AbsoluteViolation {
            reason: "hardcoded_credential".into(),
        }]);
        let result = default_scorer().score(&input, None);
        assert!((result.static_score - VIOLATION_FLOOR).abs() < f64::EPSILON);
        assert!((result.blended_score - VIOLATION_FLOOR).abs() < f64::EPSILON);
        assert!(result.has_violations());
        assert_eq!(result.violation_reasons, vec!["hardcoded_credential"]);
    }

    #[test]
    fn test_score_with_ai_score_none_simulates_provider_error_fallback() {
        // Simulates a provider outage or malformed JSON response (ai_score = None).
        // The blended score must equal the static score, matching a static-only run.
        let input = TestInput::new(vec![ScoringSignal::Negative {
            label: "severity".into(),
            weight: 0.3,
        }]);
        let static_result = default_scorer().score(&input, None);
        let error_result = default_scorer().score(&input, None);
        assert!((static_result.blended_score - error_result.blended_score).abs() < f64::EPSILON);
        assert!((error_result.blended_score - error_result.static_score).abs() < f64::EPSILON);
    }

    #[test]
    fn test_score_empty_signals_returns_full_confidence_with_no_violations() {
        let input = TestInput::new(vec![]);
        let result = default_scorer().score(&input, None);
        assert!((result.static_score - 1.0).abs() < f64::EPSILON);
        assert!((result.blended_score - 1.0).abs() < f64::EPSILON);
        assert!(!result.has_violations());
    }

    #[test]
    fn test_score_with_violation_in_review_mode_ai_can_influence_blended_score() {
        let cfg = ScoringConfig {
            review_violations: true,
            ai_confidence_weight: 0.5,
            ai_analysis_enabled: true,
        };
        let scorer = ConfidenceScorer::new(cfg);
        let input = TestInput::new(vec![ScoringSignal::AbsoluteViolation {
            reason: "cred".into(),
        }]);
        // static = 1.0 * 0.1 = 0.1; ai = 0.8; w=0.5 => blended = 0.1*0.5 + 0.8*0.5 = 0.45
        let result = scorer.score(&input, Some(0.8));
        assert!((result.static_score - 0.1).abs() < 1e-9);
        assert!((result.blended_score - 0.45).abs() < 1e-9);
        assert!(result.has_violations());
    }

    #[test]
    fn test_score_ai_disabled_stores_no_ai_score_in_result() {
        let cfg = ScoringConfig {
            ai_analysis_enabled: false,
            ..ScoringConfig::default()
        };
        let scorer = ConfidenceScorer::new(cfg);
        let input = TestInput::new(vec![]);
        let result = scorer.score(&input, Some(0.9));
        // AI disabled: ai_score field in result must be None.
        assert!(result.ai_score.is_none());
        // Blended equals static since AI was disabled.
        assert!((result.blended_score - result.static_score).abs() < f64::EPSILON);
    }

    #[test]
    fn test_score_configurable_ai_weight_shifts_blended_result() {
        // Verify that a higher ai_confidence_weight produces a result closer to ai_score.
        let low_weight_cfg = ScoringConfig {
            ai_confidence_weight: 0.1,
            ..ScoringConfig::default()
        };
        let high_weight_cfg = ScoringConfig {
            ai_confidence_weight: 0.9,
            ..ScoringConfig::default()
        };
        let input = TestInput::new(vec![ScoringSignal::Negative {
            label: "sev".into(),
            weight: 0.6,
        }]);
        let ai = Some(1.0);
        // static = 1.0 * 0.4 = 0.4; ai = 1.0
        let low = ConfidenceScorer::new(low_weight_cfg).score(&input, ai);
        let high = ConfidenceScorer::new(high_weight_cfg).score(&input, ai);
        // Higher weight => blended closer to ai_score (1.0) => higher result.
        assert!(high.blended_score > low.blended_score);
    }
}
