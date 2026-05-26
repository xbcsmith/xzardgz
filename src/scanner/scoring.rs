//! Weighted confidence scoring for scan findings.
//!
//! [`ScoringSignal`] represents a named, weighted signal that contributes to
//! an overall confidence estimate.  [`ScoringInput`] collects multiple
//! signals, and [`ConfidenceScorer`] aggregates them into a single normalised
//! value in the range `[0.0, 1.0]` using a weighted average.
//!
//! A score of `0.0` means no confidence; `1.0` means full confidence.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ScoringSignal
// ---------------------------------------------------------------------------

/// A single named signal contributing to a confidence score.
///
/// A signal combines a human-readable name, a relative importance `weight`,
/// and a normalised `value` in `[0.0, 1.0]`.  Signals with a `weight` of
/// zero are present but do not alter the final score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringSignal {
    /// Human-readable name for this signal.
    pub name: String,
    /// Relative importance weight (positive, non-zero for meaningful signals).
    pub weight: f64,
    /// Normalised signal value in the range `[0.0, 1.0]`.
    pub value: f64,
}

impl ScoringSignal {
    /// Creates a new [`ScoringSignal`] with the given name, weight, and value.
    ///
    /// # Arguments
    ///
    /// * `name` - Human-readable name for this signal.
    /// * `weight` - Relative importance weight.  Use a positive value for
    ///   meaningful signals; zero-weight signals are ignored by the scorer.
    /// * `value` - Normalised signal strength.  Values outside `[0.0, 1.0]`
    ///   are accepted here and clamped during aggregation.
    ///
    /// # Returns
    ///
    /// A new [`ScoringSignal`] with the supplied fields set.
    pub fn new(name: impl Into<String>, weight: f64, value: f64) -> Self {
        Self {
            name: name.into(),
            weight,
            value,
        }
    }
}

// ---------------------------------------------------------------------------
// ScoringInput
// ---------------------------------------------------------------------------

/// A collection of scoring signals to aggregate.
///
/// Build an input by calling [`ScoringInput::new`] and then appending signals
/// with [`ScoringInput::add_signal`].  Pass the completed input to
/// [`ConfidenceScorer::score`] to obtain a single confidence value.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoringInput {
    /// Ordered list of signals.
    pub signals: Vec<ScoringSignal>,
}

impl ScoringInput {
    /// Creates an empty [`ScoringInput`] with no signals.
    ///
    /// # Returns
    ///
    /// A `ScoringInput` whose `signals` list is empty.
    pub fn new() -> Self {
        Self {
            signals: Vec::new(),
        }
    }

    /// Appends a signal to the input.
    ///
    /// Signals are stored in insertion order.  The order does not affect the
    /// final score, but is preserved in serialised output for reproducibility.
    ///
    /// # Arguments
    ///
    /// * `signal` - The [`ScoringSignal`] to append.
    pub fn add_signal(&mut self, signal: ScoringSignal) {
        self.signals.push(signal);
    }
}

// ---------------------------------------------------------------------------
// ConfidenceScorer
// ---------------------------------------------------------------------------

/// Aggregates scoring signals into a single confidence value.
///
/// The scorer computes a weighted average of all signal values and clamps the
/// result to `[0.0, 1.0]`.  An empty input or one whose total weight sums to
/// zero yields a score of `0.0`.
#[derive(Debug, Default)]
pub struct ConfidenceScorer;

impl ConfidenceScorer {
    /// Creates a new [`ConfidenceScorer`].
    ///
    /// `ConfidenceScorer` is stateless; all state lives in the
    /// [`ScoringInput`] passed to [`ConfidenceScorer::score`].
    ///
    /// # Returns
    ///
    /// A new `ConfidenceScorer` instance.
    pub fn new() -> Self {
        Self
    }

    /// Computes the weighted-average score, clamped to `[0.0, 1.0]`.
    ///
    /// The algorithm:
    /// 1. Sum all signal weights (`total_weight`).
    /// 2. If `total_weight` is zero, return `0.0`.
    /// 3. Compute `weighted_sum = sum(signal.weight * signal.value)`.
    /// 4. Divide `weighted_sum / total_weight` and clamp to `[0.0, 1.0]`.
    ///
    /// # Arguments
    ///
    /// * `input` - The collection of [`ScoringSignal`]s to aggregate.
    ///
    /// # Returns
    ///
    /// A `f64` in the range `[0.0, 1.0]` representing the aggregated
    /// confidence.  Returns `0.0` when there are no signals or the total
    /// weight is zero.
    pub fn score(&self, input: &ScoringInput) -> f64 {
        let total_weight: f64 = input.signals.iter().map(|s| s.weight).sum();
        if total_weight == 0.0 {
            return 0.0;
        }
        let weighted_sum: f64 = input.signals.iter().map(|s| s.weight * s.value).sum();
        let result = weighted_sum / total_weight;
        result.clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Convenience: finding confidence
// ---------------------------------------------------------------------------

/// Computes a final finding confidence by combining scanner-derived signals
/// with an AI-reported confidence value.
///
/// The AI confidence is treated as a signal with weight `2.0`, giving it
/// roughly double the influence of any single scanner signal (which each
/// carry weight `1.0` by default).  The result is clamped to `[0.0, 1.0]`.
///
/// # Arguments
///
/// * `ai_confidence`   - Confidence reported by the AI provider in `[0.0, 1.0]`.
/// * `scanner_signals` - Additional signals derived from static scanner analysis.
///
/// # Returns
///
/// A weighted-average confidence in `[0.0, 1.0]`.  Returns the clamped
/// `ai_confidence` when `scanner_signals` is empty.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::scoring::{ScoringSignal, compute_finding_confidence};
///
/// // Pure AI confidence with no scanner signals
/// let conf = compute_finding_confidence(0.8, &[]);
/// assert!((conf - 0.8).abs() < 1e-9);
///
/// // AI confidence blended with a scanner signal
/// let signals = [ScoringSignal::new("pattern_hit", 1.0, 1.0)];
/// let blended = compute_finding_confidence(0.6, &signals);
/// // weighted avg of (0.6 * 2.0 + 1.0 * 1.0) / 3.0 = 2.2 / 3.0 ≈ 0.733
/// assert!((blended - (2.2 / 3.0)).abs() < 1e-9);
/// ```
pub fn compute_finding_confidence(ai_confidence: f64, scanner_signals: &[ScoringSignal]) -> f64 {
    let mut input = ScoringInput::new();
    input.add_signal(ScoringSignal::new("ai_confidence", 2.0, ai_confidence));
    for signal in scanner_signals {
        input.add_signal(signal.clone());
    }
    ConfidenceScorer::new().score(&input)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scoring_signal_new_sets_fields() {
        let signal = ScoringSignal::new("keyword_match", 2.0, 0.75);
        assert_eq!(signal.name, "keyword_match");
        assert!((signal.weight - 2.0).abs() < f64::EPSILON);
        assert!((signal.value - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn test_scoring_input_add_signal_appends() {
        let mut input = ScoringInput::new();
        assert!(input.signals.is_empty());
        input.add_signal(ScoringSignal::new("a", 1.0, 0.5));
        assert_eq!(input.signals.len(), 1);
        input.add_signal(ScoringSignal::new("b", 1.0, 0.8));
        assert_eq!(input.signals.len(), 2);
    }

    #[test]
    fn test_confidence_scorer_empty_input_returns_zero() {
        let scorer = ConfidenceScorer::new();
        let input = ScoringInput::new();
        let score = scorer.score(&input);
        assert!((score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_confidence_scorer_single_signal_returns_its_value() {
        let scorer = ConfidenceScorer::new();
        let mut input = ScoringInput::new();
        input.add_signal(ScoringSignal::new("only", 3.0, 0.6));
        let score = scorer.score(&input);
        // weighted_sum = 3.0 * 0.6 = 1.8; total_weight = 3.0; result = 0.6
        assert!((score - 0.6).abs() < 1e-9);
    }

    #[test]
    fn test_confidence_scorer_two_equal_weight_signals_returns_average() {
        let scorer = ConfidenceScorer::new();
        let mut input = ScoringInput::new();
        input.add_signal(ScoringSignal::new("a", 1.0, 0.4));
        input.add_signal(ScoringSignal::new("b", 1.0, 0.8));
        let score = scorer.score(&input);
        // (1.0*0.4 + 1.0*0.8) / 2.0 = 1.2 / 2.0 = 0.6
        assert!((score - 0.6).abs() < 1e-9);
    }

    #[test]
    fn test_confidence_scorer_weighted_signals_returns_weighted_average() {
        let scorer = ConfidenceScorer::new();
        let mut input = ScoringInput::new();
        input.add_signal(ScoringSignal::new("high_weight", 2.0, 0.5));
        input.add_signal(ScoringSignal::new("low_weight", 1.0, 0.8));
        let score = scorer.score(&input);
        // (2.0*0.5 + 1.0*0.8) / 3.0 = (1.0 + 0.8) / 3.0 = 1.8 / 3.0 = 0.6
        assert!((score - 0.6).abs() < 1e-9);
    }

    #[test]
    fn test_confidence_scorer_clamps_above_one() {
        let scorer = ConfidenceScorer::new();
        let mut input = ScoringInput::new();
        input.add_signal(ScoringSignal::new("overload", 1.0, 1.5));
        let score = scorer.score(&input);
        assert!((score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_confidence_scorer_clamps_below_zero() {
        let scorer = ConfidenceScorer::new();
        let mut input = ScoringInput::new();
        input.add_signal(ScoringSignal::new("negative", 1.0, -0.5));
        let score = scorer.score(&input);
        assert!((score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_confidence_scorer_zero_weight_returns_zero() {
        let scorer = ConfidenceScorer::new();
        let mut input = ScoringInput::new();
        input.add_signal(ScoringSignal::new("no_weight", 0.0, 0.9));
        let score = scorer.score(&input);
        assert!((score - 0.0).abs() < f64::EPSILON);
    }

    // ------------------------------------------------------------------
    // compute_finding_confidence
    // ------------------------------------------------------------------

    #[test]
    fn test_compute_finding_confidence_no_scanner_signals_returns_ai_confidence() {
        let conf = compute_finding_confidence(0.8, &[]);
        assert!((conf - 0.8).abs() < 1e-9);
    }

    #[test]
    fn test_compute_finding_confidence_zero_ai_with_full_scanner_signal() {
        // ai=0.0 (weight 2.0), scanner=1.0 (weight 1.0) => (0.0*2+1.0*1)/3 = 0.333...
        let signals = [ScoringSignal::new("hit", 1.0, 1.0)];
        let conf = compute_finding_confidence(0.0, &signals);
        assert!((conf - (1.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn test_compute_finding_confidence_blends_ai_and_scanner_signals() {
        // ai=0.6 (weight 2.0), scanner=1.0 (weight 1.0) => (0.6*2+1.0*1)/3 = 2.2/3
        let signals = [ScoringSignal::new("pattern_hit", 1.0, 1.0)];
        let conf = compute_finding_confidence(0.6, &signals);
        assert!((conf - (2.2 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn test_compute_finding_confidence_clamps_above_one() {
        let conf = compute_finding_confidence(2.0, &[]);
        assert!((conf - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_finding_confidence_clamps_below_zero() {
        let conf = compute_finding_confidence(-1.0, &[]);
        assert!((conf - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_finding_confidence_multiple_scanner_signals_blends_correctly() {
        // ai=0.5 (w=2), s1=1.0 (w=1), s2=0.0 (w=1) => (0.5*2+1.0+0.0)/4 = 2.0/4 = 0.5
        let signals = [
            ScoringSignal::new("s1", 1.0, 1.0),
            ScoringSignal::new("s2", 1.0, 0.0),
        ];
        let conf = compute_finding_confidence(0.5, &signals);
        assert!((conf - 0.5).abs() < 1e-9);
    }
}
