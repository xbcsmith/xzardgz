//! Batch splitting utilities for investigation scopes.
//!
//! Provides [`BatchConfig`] for configuring batch parameters,
//! [`InvestigationBatch`] as a slice of a larger scope, and
//! [`split_into_batches`] for deterministically partitioning an
//! [`InvestigationScope`] into bounded chunks.

use crate::investigation::scope::{FileMatchEntry, InvestigationScope};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// BatchConfig
// ---------------------------------------------------------------------------

/// Configuration for batched investigation.
///
/// Controls how many batches are produced and how many files each batch holds.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::batch::BatchConfig;
///
/// let config = BatchConfig::new(5, 10, 2);
/// assert_eq!(config.max_batches, 5);
/// assert_eq!(config.batch_size, 10);
/// assert_eq!(config.clean_verification_turns, 2);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    /// Maximum number of batches to produce.
    pub max_batches: usize,
    /// Maximum number of files per batch.
    pub batch_size: usize,
    /// Number of verification turns to run after each batch.
    pub clean_verification_turns: usize,
}

impl BatchConfig {
    /// Creates a new [`BatchConfig`] with the specified parameters.
    ///
    /// # Arguments
    ///
    /// * `max_batches` - Maximum number of batches to produce.
    /// * `batch_size` - Maximum number of files per batch.
    /// * `clean_verification_turns` - Number of verification turns per batch.
    ///
    /// # Returns
    ///
    /// A new [`BatchConfig`] with the given values.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::batch::BatchConfig;
    ///
    /// let config = BatchConfig::new(5, 10, 2);
    /// assert_eq!(config.max_batches, 5);
    /// assert_eq!(config.batch_size, 10);
    /// assert_eq!(config.clean_verification_turns, 2);
    /// ```
    pub fn new(max_batches: usize, batch_size: usize, clean_verification_turns: usize) -> Self {
        Self {
            max_batches,
            batch_size,
            clean_verification_turns,
        }
    }
}

impl Default for BatchConfig {
    /// Returns a [`BatchConfig`] with `max_batches=10`, `batch_size=20`,
    /// and `clean_verification_turns=1`.
    ///
    /// These defaults are suitable for most mid-sized repositories.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::batch::BatchConfig;
    ///
    /// let config = BatchConfig::default();
    /// assert_eq!(config.max_batches, 10);
    /// assert_eq!(config.batch_size, 20);
    /// assert_eq!(config.clean_verification_turns, 1);
    /// ```
    fn default() -> Self {
        Self {
            max_batches: 10,
            batch_size: 20,
            clean_verification_turns: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// InvestigationBatch
// ---------------------------------------------------------------------------

/// A single investigation batch: a sorted slice of the full scope.
///
/// Produced by [`split_into_batches`] and consumed by the plugin execution
/// layer. Each batch carries a 0-based index and the total number of batches
/// so that callers can report progress.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
/// use xzardgz::investigation::batch::{BatchConfig, split_into_batches};
///
/// let mut scope = InvestigationScope::new();
/// for i in 0..3 {
///     scope.insert(FileMatchEntry::new(format!("file_{}.rs", i)));
/// }
/// let config = BatchConfig::new(10, 2, 1);
/// let batches = split_into_batches(&scope, &config);
/// assert_eq!(batches[0].index, 0);
/// assert_eq!(batches[0].total_batches, 2);
/// ```
#[derive(Debug, Clone)]
pub struct InvestigationBatch {
    /// 0-based batch index.
    pub index: usize,
    /// Total number of batches in this run.
    pub total_batches: usize,
    /// The files assigned to this batch.
    pub scope: InvestigationScope,
}

// ---------------------------------------------------------------------------
// split_into_batches
// ---------------------------------------------------------------------------

/// Splits an [`InvestigationScope`] into batches according to [`BatchConfig`].
///
/// Files are sorted by path before splitting to ensure deterministic output.
/// Returns at most `config.max_batches` batches of at most `config.batch_size`
/// files each. Returns an empty `Vec` if the scope is empty or if
/// `config.batch_size` is zero.
///
/// # Arguments
///
/// * `scope` - The investigation scope to split.
/// * `config` - Batch configuration controlling chunk size and batch cap.
///
/// # Returns
///
/// A `Vec<InvestigationBatch>` ordered by ascending index. Each batch's
/// `total_batches` field reflects the actual number of batches created.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
/// use xzardgz::investigation::batch::{BatchConfig, split_into_batches};
///
/// let mut scope = InvestigationScope::new();
/// for i in 0..5 {
///     scope.insert(FileMatchEntry::new(format!("file_{}.rs", i)));
/// }
/// let config = BatchConfig::new(10, 3, 1);
/// let batches = split_into_batches(&scope, &config);
/// assert_eq!(batches.len(), 2);
/// assert_eq!(batches[0].scope.len(), 3);
/// assert_eq!(batches[1].scope.len(), 2);
/// ```
pub fn split_into_batches(
    scope: &InvestigationScope,
    config: &BatchConfig,
) -> Vec<InvestigationBatch> {
    if scope.is_empty() || config.batch_size == 0 {
        return Vec::new();
    }

    // entries() already returns entries sorted by path, giving deterministic
    // chunk membership across independent invocations.
    let sorted_entries: Vec<&FileMatchEntry> = scope.entries();

    let owned_chunks: Vec<Vec<FileMatchEntry>> = sorted_entries
        .chunks(config.batch_size)
        .take(config.max_batches)
        .map(|chunk| chunk.iter().map(|e| (*e).clone()).collect())
        .collect();

    let total_batches = owned_chunks.len();

    owned_chunks
        .into_iter()
        .enumerate()
        .map(|(index, files)| {
            let mut batch_scope = InvestigationScope::new();
            for entry in files {
                batch_scope.insert(entry);
            }
            InvestigationBatch {
                index,
                total_batches,
                scope: batch_scope,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// compute_investigation_turns
// ---------------------------------------------------------------------------

/// Computes the number of investigation turns needed given a scope and config.
///
/// Returns the number of batches that would be created (capped by
/// `config.max_batches`). Returns `0` for an empty scope or a
/// `config.batch_size` of zero.
///
/// # Arguments
///
/// * `scope` - The investigation scope to evaluate.
/// * `config` - Batch configuration.
///
/// # Returns
///
/// The number of turns (batches), in the range `[0, config.max_batches]`.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
/// use xzardgz::investigation::batch::{BatchConfig, compute_investigation_turns};
///
/// let mut scope = InvestigationScope::new();
/// for i in 0..25 {
///     scope.insert(FileMatchEntry::new(format!("file_{}.rs", i)));
/// }
/// let config = BatchConfig::new(10, 20, 1);
/// let turns = compute_investigation_turns(&scope, &config);
/// assert_eq!(turns, 2);
/// ```
pub fn compute_investigation_turns(scope: &InvestigationScope, config: &BatchConfig) -> usize {
    if scope.is_empty() || config.batch_size == 0 {
        return 0;
    }
    let raw = scope.len().div_ceil(config.batch_size);
    raw.min(config.max_batches)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Builds a scope with `n` entries named `file_0.rs` .. `file_{n-1}.rs`.
    fn make_scope(n: usize) -> InvestigationScope {
        let mut scope = InvestigationScope::new();
        for i in 0..n {
            scope.insert(FileMatchEntry::new(format!("file_{i}.rs")));
        }
        scope
    }

    // ------------------------------------------------------------------
    // BatchConfig
    // ------------------------------------------------------------------

    #[test]
    fn test_batch_config_new_sets_all_fields() {
        let config = BatchConfig::new(5, 10, 2);
        assert_eq!(config.max_batches, 5);
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.clean_verification_turns, 2);
    }

    #[test]
    fn test_batch_config_default_has_expected_values() {
        let config = BatchConfig::default();
        assert_eq!(config.max_batches, 10);
        assert_eq!(config.batch_size, 20);
        assert_eq!(config.clean_verification_turns, 1);
    }

    #[test]
    fn test_batch_config_new_with_zeros_accepts_edge_values() {
        let config = BatchConfig::new(0, 0, 0);
        assert_eq!(config.max_batches, 0);
        assert_eq!(config.batch_size, 0);
        assert_eq!(config.clean_verification_turns, 0);
    }

    // ------------------------------------------------------------------
    // split_into_batches - empty / guard cases
    // ------------------------------------------------------------------

    #[test]
    fn test_split_into_batches_empty_scope_returns_empty_vec() {
        let scope = InvestigationScope::new();
        let config = BatchConfig::default();
        let batches = split_into_batches(&scope, &config);
        assert!(batches.is_empty());
    }

    #[test]
    fn test_split_into_batches_batch_size_zero_returns_empty_vec() {
        let scope = make_scope(5);
        let config = BatchConfig::new(10, 0, 1);
        let batches = split_into_batches(&scope, &config);
        assert!(batches.is_empty());
    }

    // ------------------------------------------------------------------
    // split_into_batches - single batch
    // ------------------------------------------------------------------

    #[test]
    fn test_split_into_batches_single_batch_when_files_lte_batch_size() {
        let scope = make_scope(5);
        let config = BatchConfig::new(10, 20, 1);
        let batches = split_into_batches(&scope, &config);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].scope.len(), 5);
    }

    #[test]
    fn test_split_into_batches_exact_batch_size_boundary_produces_one_batch() {
        let scope = make_scope(20);
        let config = BatchConfig::new(10, 20, 1);
        let batches = split_into_batches(&scope, &config);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].scope.len(), 20);
    }

    // ------------------------------------------------------------------
    // split_into_batches - multiple batches
    // ------------------------------------------------------------------

    #[test]
    fn test_split_into_batches_multiple_batches_when_files_gt_batch_size() {
        let scope = make_scope(7);
        let config = BatchConfig::new(10, 3, 1);
        let batches = split_into_batches(&scope, &config);
        // ceil(7/3) = 3
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].scope.len(), 3);
        assert_eq!(batches[1].scope.len(), 3);
        assert_eq!(batches[2].scope.len(), 1);
    }

    #[test]
    fn test_split_into_batches_caps_at_max_batches() {
        // 100 files, batch_size=5 would normally give 20 batches, but max=4
        let scope = make_scope(100);
        let config = BatchConfig::new(4, 5, 1);
        let batches = split_into_batches(&scope, &config);
        assert_eq!(batches.len(), 4);
        // Each of the 4 batches should have 5 files
        for batch in &batches {
            assert_eq!(batch.scope.len(), 5);
        }
    }

    // ------------------------------------------------------------------
    // split_into_batches - index / total_batches fields
    // ------------------------------------------------------------------

    #[test]
    fn test_split_into_batches_index_and_total_batches_are_correct() {
        let scope = make_scope(5);
        let config = BatchConfig::new(10, 3, 1);
        let batches = split_into_batches(&scope, &config);
        // ceil(5/3) = 2 batches
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].index, 0);
        assert_eq!(batches[0].total_batches, 2);
        assert_eq!(batches[1].index, 1);
        assert_eq!(batches[1].total_batches, 2);
    }

    // ------------------------------------------------------------------
    // split_into_batches - determinism
    // ------------------------------------------------------------------

    #[test]
    fn test_split_into_batches_files_are_sorted_deterministically() {
        let mut scope = InvestigationScope::new();
        // Insert in non-alphabetical order.
        scope.insert(FileMatchEntry::new("c.rs"));
        scope.insert(FileMatchEntry::new("a.rs"));
        scope.insert(FileMatchEntry::new("b.rs"));
        let config = BatchConfig::new(10, 2, 1);
        let batches = split_into_batches(&scope, &config);
        // First batch should contain a.rs and b.rs (sorted).
        assert_eq!(batches.len(), 2);
        // SAFETY: batch is non-empty by construction
        let first_batch_paths = batches[0].scope.paths();
        assert_eq!(first_batch_paths, vec!["a.rs", "b.rs"]);
        let second_batch_paths = batches[1].scope.paths();
        assert_eq!(second_batch_paths, vec!["c.rs"]);
    }

    #[test]
    fn test_split_into_batches_two_calls_produce_identical_batches() {
        let scope = make_scope(10);
        let config = BatchConfig::new(10, 3, 1);
        let batches_a = split_into_batches(&scope, &config);
        let batches_b = split_into_batches(&scope, &config);
        assert_eq!(batches_a.len(), batches_b.len());
        for (a, b) in batches_a.iter().zip(batches_b.iter()) {
            assert_eq!(a.scope.paths(), b.scope.paths());
        }
    }

    // ------------------------------------------------------------------
    // compute_investigation_turns
    // ------------------------------------------------------------------

    #[test]
    fn test_compute_investigation_turns_empty_scope_returns_zero() {
        let scope = InvestigationScope::new();
        let config = BatchConfig::default();
        assert_eq!(compute_investigation_turns(&scope, &config), 0);
    }

    #[test]
    fn test_compute_investigation_turns_batch_size_zero_returns_zero() {
        let scope = make_scope(10);
        let config = BatchConfig::new(10, 0, 1);
        assert_eq!(compute_investigation_turns(&scope, &config), 0);
    }

    #[test]
    fn test_compute_investigation_turns_small_scope_returns_one() {
        let scope = make_scope(5);
        let config = BatchConfig::new(10, 20, 1);
        assert_eq!(compute_investigation_turns(&scope, &config), 1);
    }

    #[test]
    fn test_compute_investigation_turns_exact_boundary_returns_one() {
        let scope = make_scope(20);
        let config = BatchConfig::new(10, 20, 1);
        assert_eq!(compute_investigation_turns(&scope, &config), 1);
    }

    #[test]
    fn test_compute_investigation_turns_one_over_boundary_returns_two() {
        let scope = make_scope(21);
        let config = BatchConfig::new(10, 20, 1);
        assert_eq!(compute_investigation_turns(&scope, &config), 2);
    }

    #[test]
    fn test_compute_investigation_turns_large_scope_is_capped_by_max_batches() {
        // 100 files / batch_size 5 = 20 raw, capped to 10
        let scope = make_scope(100);
        let config = BatchConfig::new(10, 5, 1);
        assert_eq!(compute_investigation_turns(&scope, &config), 10);
    }

    #[test]
    fn test_compute_investigation_turns_matches_split_into_batches_len() {
        let scope = make_scope(37);
        let config = BatchConfig::new(8, 7, 1);
        let turns = compute_investigation_turns(&scope, &config);
        let batches = split_into_batches(&scope, &config);
        assert_eq!(turns, batches.len());
    }
}
