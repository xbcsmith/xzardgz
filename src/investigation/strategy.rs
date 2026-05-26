//! Investigation strategy selection.
//!
//! Provides [`InvestigationStrategy`], which determines whether a plugin
//! investigates files in a single AI session or spreads work across multiple
//! batched sessions. The strategy is chosen based on the size of the
//! [`InvestigationScope`] and drives downstream plugin dispatch.

use crate::investigation::batch::{BatchConfig, compute_investigation_turns};
use crate::investigation::scope::InvestigationScope;
use serde::{Deserialize, Serialize};

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
}
