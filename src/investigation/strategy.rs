//! Investigation strategy selection.
//!
//! Provides [`InvestigationStrategy`], which determines whether a plugin
//! investigates files in a single AI session or spreads work across multiple
//! batched sessions. The strategy is chosen based on the size of the
//! [`InvestigationScope`] and drives downstream plugin dispatch.

use crate::investigation::batch::{BatchConfig, compute_investigation_turns};
use crate::investigation::scope::{InvestigationScope, ScopeMetrics};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Turn budget constants
// ---------------------------------------------------------------------------

/// Minimum turn allowance for any investigation session.
const BASE_TURNS: u32 = 5;

/// Additional turns added per matched file in the scope.
const PER_FILE_TURNS: u32 = 2;

/// One additional turn is added per this many total repository files.
///
/// Represents the repository breadth: a large codebase with many files
/// requires broader exploration even if few files are directly flagged.
const BREADTH_DIVISOR: u64 = 100;

/// Hard cap on the turn budget regardless of scope size.
const MAX_TURNS: u32 = 200;

/// Default batch size used when `batch_count` is zero in
/// [`decide_investigation_strategy`].
const DEFAULT_BATCH_SIZE: usize = 10;

// ---------------------------------------------------------------------------
// compute_turn_budget
// ---------------------------------------------------------------------------

/// Computes the turn budget for an investigation session given repository metrics.
///
/// The budget follows an additive model:
///
/// | Term | Formula | Meaning |
/// |---|---|---|
/// | Base allowance | [`BASE_TURNS`] | Fixed minimum for any session |
/// | Per-file term | `matched_file_count * PER_FILE_TURNS` | Scales with flagged files |
/// | Breadth term | `total_files / BREADTH_DIVISOR` | Scales with repo size |
///
/// All intermediate values are computed with saturating arithmetic to avoid
/// overflow.  The final result is capped at [`MAX_TURNS`].
///
/// A large repository with many matched files always yields a materially
/// larger budget than a small one with few matches.
///
/// # Arguments
///
/// * `metrics` - Repository scope metrics (see [`ScopeMetrics`]).
///
/// # Returns
///
/// A `u32` turn budget in the range `[BASE_TURNS, MAX_TURNS]`.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::scope::ScopeMetrics;
/// use xzardgz::investigation::strategy::compute_turn_budget;
///
/// // Small scope: 5 matched files, 50 total.
/// let small = ScopeMetrics::new(50, 0, 5);
/// let budget = compute_turn_budget(&small);
/// assert!(budget >= 5);
///
/// // Large scope: 80 matched files, 2000 total.
/// let large = ScopeMetrics::new(2000, 0, 80);
/// let large_budget = compute_turn_budget(&large);
/// assert!(large_budget > budget);
/// ```
pub fn compute_turn_budget(metrics: &ScopeMetrics) -> u32 {
    let per_file = u32::try_from(
        metrics
            .matched_file_count
            .saturating_mul(PER_FILE_TURNS as u64),
    )
    .unwrap_or(u32::MAX);
    let breadth = u32::try_from(metrics.total_files / BREADTH_DIVISOR.max(1)).unwrap_or(u32::MAX);
    BASE_TURNS
        .saturating_add(per_file)
        .saturating_add(breadth)
        .min(MAX_TURNS)
}

// ---------------------------------------------------------------------------
// decide_investigation_strategy
// ---------------------------------------------------------------------------

/// Selects an [`InvestigationStrategy`] from repository metrics and thresholds.
///
/// The strategy is chosen as follows:
///
/// - If `metrics.matched_file_count > threshold_files` **or**
///   `metrics.total_size_bytes > threshold_bytes`, the scope is considered
///   too large for a single session and
///   [`InvestigationStrategy::BatchedSession`] is returned.
///   The batch size is computed as `ceil(matched_file_count / batch_count)`,
///   ensuring the files are spread across `batch_count` sessions.
/// - Otherwise, [`InvestigationStrategy::SingleSession`] is returned.
///
/// When `batch_count` is `0`, a default batch size of [`DEFAULT_BATCH_SIZE`]
/// is used and `max_batches` is set to `1`.
///
/// # Arguments
///
/// * `metrics` - Repository scope metrics (see [`ScopeMetrics`]).
/// * `threshold_files` - Matched-file count above which batching is triggered.
/// * `threshold_bytes` - Total repository byte count above which batching is triggered.
/// * `batch_count` - Desired number of batches; must be non-zero for meaningful batching.
///
/// # Returns
///
/// [`InvestigationStrategy::SingleSession`] when both thresholds are satisfied,
/// [`InvestigationStrategy::BatchedSession`] otherwise.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::scope::ScopeMetrics;
/// use xzardgz::investigation::strategy::{decide_investigation_strategy, InvestigationStrategy};
///
/// let small = ScopeMetrics::new(50, 1024, 5);
/// let strategy = decide_investigation_strategy(&small, 20, 1_000_000, 4);
/// assert!(matches!(strategy, InvestigationStrategy::SingleSession));
///
/// let large = ScopeMetrics::new(2000, 10_000_000, 80);
/// let strategy = decide_investigation_strategy(&large, 20, 1_000_000, 4);
/// assert!(matches!(strategy, InvestigationStrategy::BatchedSession(_)));
/// ```
pub fn decide_investigation_strategy(
    metrics: &ScopeMetrics,
    threshold_files: u64,
    threshold_bytes: u64,
    batch_count: usize,
) -> InvestigationStrategy {
    let needs_batching =
        metrics.matched_file_count > threshold_files || metrics.total_size_bytes > threshold_bytes;
    if needs_batching {
        if batch_count == 0 {
            return InvestigationStrategy::BatchedSession(BatchConfig::new(
                1,
                DEFAULT_BATCH_SIZE,
                1,
            ));
        }
        let batch_size = ((metrics.matched_file_count as usize).div_ceil(batch_count)).max(1);
        InvestigationStrategy::BatchedSession(BatchConfig::new(batch_count, batch_size, 1))
    } else {
        InvestigationStrategy::SingleSession
    }
}

// ---------------------------------------------------------------------------
// InvestigationStrategy
// ---------------------------------------------------------------------------

/// Strategy for how a plugin investigates files.
///
/// Select the appropriate variant based on the scope size and available
/// context window. Use [`InvestigationStrategy::default_for_scope`] to let
/// the library choose automatically.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
/// use xzardgz::investigation::strategy::InvestigationStrategy;
///
/// let mut scope = InvestigationScope::new();
/// for i in 0..5 {
///     scope.insert(FileMatchEntry::new(format!("f_{}.rs", i)));
/// }
/// let strategy = InvestigationStrategy::default_for_scope(&scope);
/// assert!(matches!(strategy, InvestigationStrategy::SingleSession));
/// assert!(!strategy.is_batched());
/// assert_eq!(strategy.estimated_turns(&scope), 1);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InvestigationStrategy {
    /// Analyze all files in a single AI session.
    ///
    /// Suitable for small scopes where the entire file list fits in context.
    SingleSession,
    /// Analyze files in multiple batches, each in its own AI session.
    ///
    /// Suitable for large repositories where a single session would exceed
    /// context limits. The contained [`BatchConfig`] controls chunk sizing
    /// and the maximum number of batches.
    BatchedSession(BatchConfig),
}

impl InvestigationStrategy {
    /// Selects a default strategy based on the size of the scope.
    ///
    /// Scopes with 20 or fewer files use [`InvestigationStrategy::SingleSession`].
    /// Larger scopes use [`InvestigationStrategy::BatchedSession`] with
    /// [`BatchConfig::default`].
    ///
    /// # Arguments
    ///
    /// * `scope` - The investigation scope to evaluate.
    ///
    /// # Returns
    ///
    /// [`InvestigationStrategy::SingleSession`] when `scope.len() <= 20`,
    /// [`InvestigationStrategy::BatchedSession`] otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
    /// use xzardgz::investigation::strategy::InvestigationStrategy;
    ///
    /// let mut small = InvestigationScope::new();
    /// for i in 0..20 {
    ///     small.insert(FileMatchEntry::new(format!("f_{}.rs", i)));
    /// }
    /// assert!(matches!(
    ///     InvestigationStrategy::default_for_scope(&small),
    ///     InvestigationStrategy::SingleSession
    /// ));
    ///
    /// let mut large = InvestigationScope::new();
    /// for i in 0..21 {
    ///     large.insert(FileMatchEntry::new(format!("g_{}.rs", i)));
    /// }
    /// assert!(matches!(
    ///     InvestigationStrategy::default_for_scope(&large),
    ///     InvestigationStrategy::BatchedSession(_)
    /// ));
    /// ```
    pub fn default_for_scope(scope: &InvestigationScope) -> InvestigationStrategy {
        if scope.len() <= 20 {
            InvestigationStrategy::SingleSession
        } else {
            InvestigationStrategy::BatchedSession(BatchConfig::default())
        }
    }

    /// Returns the [`BatchConfig`] if the strategy is [`InvestigationStrategy::BatchedSession`].
    ///
    /// # Returns
    ///
    /// `Some(&BatchConfig)` for `BatchedSession`, `None` for `SingleSession`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::batch::BatchConfig;
    /// use xzardgz::investigation::strategy::InvestigationStrategy;
    ///
    /// let single = InvestigationStrategy::SingleSession;
    /// assert!(single.batch_config().is_none());
    ///
    /// let batched = InvestigationStrategy::BatchedSession(BatchConfig::default());
    /// assert!(batched.batch_config().is_some());
    /// ```
    pub fn batch_config(&self) -> Option<&BatchConfig> {
        match self {
            InvestigationStrategy::SingleSession => None,
            InvestigationStrategy::BatchedSession(config) => Some(config),
        }
    }

    /// Returns `true` if the strategy uses batched sessions.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::batch::BatchConfig;
    /// use xzardgz::investigation::strategy::InvestigationStrategy;
    ///
    /// assert!(!InvestigationStrategy::SingleSession.is_batched());
    /// assert!(InvestigationStrategy::BatchedSession(BatchConfig::default()).is_batched());
    /// ```
    pub fn is_batched(&self) -> bool {
        matches!(self, InvestigationStrategy::BatchedSession(_))
    }

    /// Estimates the number of AI turns required for the given scope.
    ///
    /// For [`InvestigationStrategy::SingleSession`], always returns `1`.
    /// For [`InvestigationStrategy::BatchedSession`], delegates to
    /// [`compute_investigation_turns`].
    ///
    /// # Arguments
    ///
    /// * `scope` - The investigation scope to evaluate.
    ///
    /// # Returns
    ///
    /// The estimated number of AI turns.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
    /// use xzardgz::investigation::batch::BatchConfig;
    /// use xzardgz::investigation::strategy::InvestigationStrategy;
    ///
    /// let mut scope = InvestigationScope::new();
    /// for i in 0..3 {
    ///     scope.insert(FileMatchEntry::new(format!("f_{}.rs", i)));
    /// }
    ///
    /// let single = InvestigationStrategy::SingleSession;
    /// assert_eq!(single.estimated_turns(&scope), 1);
    ///
    /// let batched = InvestigationStrategy::BatchedSession(BatchConfig::new(10, 2, 1));
    /// assert_eq!(batched.estimated_turns(&scope), 2);
    /// ```
    pub fn estimated_turns(&self, scope: &InvestigationScope) -> usize {
        match self {
            InvestigationStrategy::SingleSession => 1,
            InvestigationStrategy::BatchedSession(config) => {
                compute_investigation_turns(scope, config)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::investigation::scope::FileMatchEntry;
    use crate::investigation::scope::ScopeMetrics;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn make_scope(n: usize) -> InvestigationScope {
        let mut scope = InvestigationScope::new();
        for i in 0..n {
            scope.insert(FileMatchEntry::new(format!("file_{i}.rs")));
        }
        scope
    }

    // ------------------------------------------------------------------
    // default_for_scope
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_strategy_default_for_scope_single_when_scope_is_empty() {
        let scope = InvestigationScope::new();
        let strategy = InvestigationStrategy::default_for_scope(&scope);
        assert!(matches!(strategy, InvestigationStrategy::SingleSession));
    }

    #[test]
    fn test_investigation_strategy_default_for_scope_single_when_scope_is_small() {
        let scope = make_scope(5);
        let strategy = InvestigationStrategy::default_for_scope(&scope);
        assert!(matches!(strategy, InvestigationStrategy::SingleSession));
    }

    #[test]
    fn test_investigation_strategy_default_for_scope_single_at_exact_boundary() {
        let scope = make_scope(20);
        let strategy = InvestigationStrategy::default_for_scope(&scope);
        assert!(matches!(strategy, InvestigationStrategy::SingleSession));
    }

    #[test]
    fn test_investigation_strategy_default_for_scope_batched_when_one_over_boundary() {
        let scope = make_scope(21);
        let strategy = InvestigationStrategy::default_for_scope(&scope);
        assert!(matches!(strategy, InvestigationStrategy::BatchedSession(_)));
    }

    #[test]
    fn test_investigation_strategy_default_for_scope_batched_when_large() {
        let scope = make_scope(100);
        let strategy = InvestigationStrategy::default_for_scope(&scope);
        assert!(matches!(strategy, InvestigationStrategy::BatchedSession(_)));
    }

    #[test]
    fn test_investigation_strategy_default_for_scope_batched_uses_default_config() {
        let scope = make_scope(50);
        let strategy = InvestigationStrategy::default_for_scope(&scope);
        if let InvestigationStrategy::BatchedSession(config) = strategy {
            assert_eq!(config.max_batches, 10);
            assert_eq!(config.batch_size, 20);
            assert_eq!(config.clean_verification_turns, 1);
        } else {
            panic!("expected BatchedSession");
        }
    }

    // ------------------------------------------------------------------
    // batch_config
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_strategy_batch_config_returns_none_for_single_session() {
        let strategy = InvestigationStrategy::SingleSession;
        assert!(strategy.batch_config().is_none());
    }

    #[test]
    fn test_investigation_strategy_batch_config_returns_some_for_batched_session() {
        let config = BatchConfig::new(5, 15, 2);
        let strategy = InvestigationStrategy::BatchedSession(config);
        let bc = strategy.batch_config();
        assert!(bc.is_some());
        let bc = bc.unwrap();
        assert_eq!(bc.max_batches, 5);
        assert_eq!(bc.batch_size, 15);
        assert_eq!(bc.clean_verification_turns, 2);
    }

    // ------------------------------------------------------------------
    // is_batched
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_strategy_is_batched_false_for_single_session() {
        assert!(!InvestigationStrategy::SingleSession.is_batched());
    }

    #[test]
    fn test_investigation_strategy_is_batched_true_for_batched_session() {
        let strategy = InvestigationStrategy::BatchedSession(BatchConfig::default());
        assert!(strategy.is_batched());
    }

    // ------------------------------------------------------------------
    // estimated_turns
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_strategy_estimated_turns_single_session_returns_one() {
        let scope = make_scope(100);
        let strategy = InvestigationStrategy::SingleSession;
        assert_eq!(strategy.estimated_turns(&scope), 1);
    }

    #[test]
    fn test_investigation_strategy_estimated_turns_single_session_empty_scope_returns_one() {
        let scope = InvestigationScope::new();
        let strategy = InvestigationStrategy::SingleSession;
        // Single session is always one turn regardless of scope size.
        assert_eq!(strategy.estimated_turns(&scope), 1);
    }

    #[test]
    fn test_investigation_strategy_estimated_turns_batched_session_computes_turns() {
        let scope = make_scope(3);
        let strategy = InvestigationStrategy::BatchedSession(BatchConfig::new(10, 2, 1));
        // ceil(3/2) = 2
        assert_eq!(strategy.estimated_turns(&scope), 2);
    }

    #[test]
    fn test_investigation_strategy_estimated_turns_batched_session_empty_scope_returns_zero() {
        let scope = InvestigationScope::new();
        let strategy = InvestigationStrategy::BatchedSession(BatchConfig::default());
        assert_eq!(strategy.estimated_turns(&scope), 0);
    }

    #[test]
    fn test_investigation_strategy_estimated_turns_batched_respects_max_batches_cap() {
        let scope = make_scope(200);
        let strategy = InvestigationStrategy::BatchedSession(BatchConfig::new(5, 10, 1));
        // 200/10 = 20 raw, capped to 5
        assert_eq!(strategy.estimated_turns(&scope), 5);
    }

    #[test]
    fn test_investigation_strategy_estimated_turns_batched_exact_fit() {
        let scope = make_scope(20);
        let strategy = InvestigationStrategy::BatchedSession(BatchConfig::new(10, 20, 1));
        assert_eq!(strategy.estimated_turns(&scope), 1);
    }

    // ------------------------------------------------------------------
    // compute_turn_budget
    // ------------------------------------------------------------------

    #[test]
    fn test_compute_turn_budget_empty_scope_returns_base_turns() {
        let metrics = ScopeMetrics::new(0, 0, 0);
        assert_eq!(compute_turn_budget(&metrics), 5);
    }

    #[test]
    fn test_compute_turn_budget_small_scope_scales_above_base() {
        let metrics = ScopeMetrics::new(50, 0, 5);
        // 5 + (5*2) + (50/100) = 5 + 10 + 0 = 15
        assert_eq!(compute_turn_budget(&metrics), 15);
    }

    #[test]
    fn test_compute_turn_budget_medium_scope_returns_expected_budget() {
        let metrics = ScopeMetrics::new(500, 0, 25);
        // 5 + (25*2) + (500/100) = 5 + 50 + 5 = 60
        assert_eq!(compute_turn_budget(&metrics), 60);
    }

    #[test]
    fn test_compute_turn_budget_large_scope_returns_expected_budget() {
        let metrics = ScopeMetrics::new(2000, 0, 80);
        // 5 + (80*2) + (2000/100) = 5 + 160 + 20 = 185
        assert_eq!(compute_turn_budget(&metrics), 185);
    }

    #[test]
    fn test_compute_turn_budget_very_large_scope_is_capped_at_max_turns() {
        let metrics = ScopeMetrics::new(10_000, 0, 100);
        // 5 + (100*2) + (10000/100) = 5 + 200 + 100 = 305 -> capped at 200
        assert_eq!(compute_turn_budget(&metrics), 200);
    }

    #[test]
    fn test_compute_turn_budget_large_scope_exceeds_small_scope() {
        let small = ScopeMetrics::new(50, 0, 5);
        let large = ScopeMetrics::new(2000, 0, 80);
        assert!(compute_turn_budget(&large) > compute_turn_budget(&small));
    }

    #[test]
    fn test_compute_turn_budget_medium_scope_exceeds_small_scope() {
        let small = ScopeMetrics::new(50, 0, 5);
        let medium = ScopeMetrics::new(500, 0, 25);
        assert!(compute_turn_budget(&medium) > compute_turn_budget(&small));
    }

    #[test]
    fn test_compute_turn_budget_is_deterministic_for_same_metrics() {
        let metrics = ScopeMetrics::new(300, 1_000_000, 15);
        assert_eq!(compute_turn_budget(&metrics), compute_turn_budget(&metrics));
    }

    #[test]
    fn test_compute_turn_budget_only_matched_files_no_breadth() {
        // total_files = 0, so breadth = 0; only base + per_file
        let metrics = ScopeMetrics::new(0, 0, 10);
        // 5 + (10*2) + 0 = 25
        assert_eq!(compute_turn_budget(&metrics), 25);
    }

    #[test]
    fn test_compute_turn_budget_only_breadth_no_matched_files() {
        // matched = 0, total_files = 1000, breadth = 10
        let metrics = ScopeMetrics::new(1000, 0, 0);
        // 5 + 0 + (1000/100) = 5 + 0 + 10 = 15
        assert_eq!(compute_turn_budget(&metrics), 15);
    }

    // ------------------------------------------------------------------
    // decide_investigation_strategy
    // ------------------------------------------------------------------

    #[test]
    fn test_decide_investigation_strategy_small_scope_returns_single_session() {
        let metrics = ScopeMetrics::new(50, 1024, 5);
        let strategy = decide_investigation_strategy(&metrics, 20, 1_000_000, 4);
        assert!(matches!(strategy, InvestigationStrategy::SingleSession));
    }

    #[test]
    fn test_decide_investigation_strategy_exactly_at_file_threshold_returns_single_session() {
        // matched_file_count == threshold_files (not >, so single session)
        let metrics = ScopeMetrics::new(100, 0, 20);
        let strategy = decide_investigation_strategy(&metrics, 20, 1_000_000, 4);
        assert!(matches!(strategy, InvestigationStrategy::SingleSession));
    }

    #[test]
    fn test_decide_investigation_strategy_one_over_file_threshold_returns_batched() {
        let metrics = ScopeMetrics::new(100, 0, 21);
        let strategy = decide_investigation_strategy(&metrics, 20, 1_000_000, 4);
        assert!(matches!(strategy, InvestigationStrategy::BatchedSession(_)));
    }

    #[test]
    fn test_decide_investigation_strategy_over_byte_threshold_returns_batched() {
        let metrics = ScopeMetrics::new(10, 2_000_000, 5);
        let strategy = decide_investigation_strategy(&metrics, 100, 1_000_000, 4);
        assert!(matches!(strategy, InvestigationStrategy::BatchedSession(_)));
    }

    #[test]
    fn test_decide_investigation_strategy_batched_batch_size_is_ceil_divide() {
        // 40 matched files / 4 batches = 10 per batch
        let metrics = ScopeMetrics::new(100, 0, 40);
        let strategy = decide_investigation_strategy(&metrics, 20, 1_000_000, 4);
        if let InvestigationStrategy::BatchedSession(config) = strategy {
            assert_eq!(config.batch_size, 10);
            assert_eq!(config.max_batches, 4);
        } else {
            panic!("expected BatchedSession");
        }
    }

    #[test]
    fn test_decide_investigation_strategy_batched_batch_size_rounds_up() {
        // 10 matched files / 3 batches = ceil(10/3) = 4
        let metrics = ScopeMetrics::new(100, 0, 10);
        let strategy = decide_investigation_strategy(&metrics, 5, 1_000_000, 3);
        if let InvestigationStrategy::BatchedSession(config) = strategy {
            assert_eq!(config.batch_size, 4);
        } else {
            panic!("expected BatchedSession");
        }
    }

    #[test]
    fn test_decide_investigation_strategy_zero_batch_count_uses_defaults() {
        let metrics = ScopeMetrics::new(100, 0, 50);
        let strategy = decide_investigation_strategy(&metrics, 20, 1_000_000, 0);
        if let InvestigationStrategy::BatchedSession(config) = strategy {
            assert_eq!(config.max_batches, 1);
            assert_eq!(config.batch_size, 10); // DEFAULT_BATCH_SIZE
        } else {
            panic!("expected BatchedSession");
        }
    }

    #[test]
    fn test_decide_investigation_strategy_large_scope_large_batch_count() {
        let metrics = ScopeMetrics::new(5000, 0, 100);
        let strategy = decide_investigation_strategy(&metrics, 20, 1_000_000, 10);
        assert!(matches!(strategy, InvestigationStrategy::BatchedSession(_)));
    }

    #[test]
    fn test_decide_investigation_strategy_is_deterministic() {
        let metrics = ScopeMetrics::new(200, 500_000, 30);
        let s1 = decide_investigation_strategy(&metrics, 20, 1_000_000, 5);
        let s2 = decide_investigation_strategy(&metrics, 20, 1_000_000, 5);
        // Both should produce the same variant
        assert_eq!(s1.is_batched(), s2.is_batched());
    }
}
