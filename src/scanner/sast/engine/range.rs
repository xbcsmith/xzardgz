//! Range algebra for SAST pattern matching.
//!
//! This module provides the core types and operations for working with byte
//! ranges produced by SAST pattern matching. Ranges carry metavariable
//! bindings and support set operations (intersection, union, subtraction)
//! used during formula evaluation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The text and byte-range of a single metavariable capture.
///
/// A `MetavarValue` is produced by the AST pattern engine when a named
/// metavariable (e.g. `$X`) is matched against a source node.  The `text`
/// field contains the source text of the matched node; `start` and `end` are
/// its half-open byte offsets `[start, end)` within the source file.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::engine::range::MetavarValue;
///
/// let mv = MetavarValue { text: "my_function".to_string(), start: 3, end: 14 };
/// assert_eq!(mv.text, "my_function");
/// assert_eq!(mv.end - mv.start, mv.text.len());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetavarValue {
    /// Source text of the matched node.
    pub text: String,
    /// Byte offset of the start of the matched node (inclusive).
    pub start: usize,
    /// Byte offset of the end of the matched node (exclusive).
    pub end: usize,
}

/// Metavariable bindings from a pattern match.
///
/// The key is the metavariable name with the `$` prefix (e.g. `"$X"`, `"$FOO"`).
/// The value is a [`MetavarValue`] holding the bound source text and its byte
/// offsets within the source file.
pub type MetavarBindings = BTreeMap<String, MetavarValue>;

/// A matched byte range in a source file together with metavariable bindings.
///
/// `start` and `end` are byte offsets in the source file (half-open interval
/// `[start, end)`).  `bindings` maps metavariable names (with the `$` prefix)
/// to the source text they were bound to during matching.
///
/// Ranges are ordered by `(start, end)` for deterministic output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeWithMetavars {
    /// Byte offset of the start of the match (inclusive).
    pub start: usize,
    /// Byte offset of the end of the match (exclusive).
    pub end: usize,
    /// Metavariable bindings produced by this match.
    pub bindings: MetavarBindings,
}

impl RangeWithMetavars {
    /// Create a new `RangeWithMetavars`.
    ///
    /// # Arguments
    ///
    /// * `start` - Byte offset of the start of the match (inclusive).
    /// * `end` - Byte offset of the end of the match (exclusive).
    /// * `bindings` - Metavariable bindings produced by this match.
    ///
    /// # Returns
    ///
    /// A new `RangeWithMetavars` with the given fields.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::engine::range::{RangeWithMetavars, MetavarBindings};
    ///
    /// let r = RangeWithMetavars::new(0, 10, MetavarBindings::new());
    /// assert_eq!(r.start, 0);
    /// assert_eq!(r.end, 10);
    /// assert!(r.bindings.is_empty());
    /// ```
    pub fn new(start: usize, end: usize, bindings: MetavarBindings) -> Self {
        Self {
            start,
            end,
            bindings,
        }
    }

    /// Returns `true` if this range and `other` share at least one byte.
    ///
    /// Two ranges overlap when `self.start < other.end && other.start < self.end`.
    /// Adjacent ranges (e.g. `[0, 5)` and `[5, 10)`) do NOT overlap.
    ///
    /// # Arguments
    ///
    /// * `other` - The range to test against.
    ///
    /// # Returns
    ///
    /// `true` if the ranges share at least one byte position.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::engine::range::{RangeWithMetavars, MetavarBindings};
    ///
    /// let a = RangeWithMetavars::new(0, 10, MetavarBindings::new());
    /// let b = RangeWithMetavars::new(5, 15, MetavarBindings::new());
    /// assert!(a.overlaps(&b));
    ///
    /// let c = RangeWithMetavars::new(10, 20, MetavarBindings::new());
    /// assert!(!a.overlaps(&c)); // adjacent, not overlapping
    /// ```
    pub fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Returns `true` if `other` is fully within this range.
    ///
    /// A range `other` is contained within `self` when
    /// `self.start <= other.start && other.end <= self.end`.
    /// Equal ranges are considered to contain each other.
    ///
    /// # Arguments
    ///
    /// * `other` - The range to test for containment.
    ///
    /// # Returns
    ///
    /// `true` if `other` lies entirely within `self`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::engine::range::{RangeWithMetavars, MetavarBindings};
    ///
    /// let outer = RangeWithMetavars::new(0, 10, MetavarBindings::new());
    /// let inner = RangeWithMetavars::new(2, 8, MetavarBindings::new());
    /// assert!(outer.contains(&inner));
    ///
    /// let partial = RangeWithMetavars::new(5, 15, MetavarBindings::new());
    /// assert!(!outer.contains(&partial));
    /// ```
    pub fn contains(&self, other: &Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }
}

impl Ord for RangeWithMetavars {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.start, self.end).cmp(&(other.start, other.end))
    }
}

impl PartialOrd for RangeWithMetavars {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Attempt to intersect two ranges with metavariable binding unification.
///
/// Returns `Some(merged)` if and only if:
/// 1. The ranges overlap (share at least one byte position).
/// 2. Their metavariable bindings are compatible: no metavariable is bound to
///    two different texts in the two ranges.
///
/// The merged range spans `[max(a.start, b.start), min(a.end, b.end)]` with
/// bindings from both ranges combined.
///
/// Returns `None` if ranges do not overlap or have conflicting bindings.
///
/// # Arguments
///
/// * `a` - The first range.
/// * `b` - The second range.
///
/// # Returns
///
/// `Some(merged)` if the ranges overlap and bindings are compatible,
/// `None` otherwise.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::engine::range::{RangeWithMetavars, MetavarBindings, intersect};
///
/// let a = RangeWithMetavars::new(0, 10, MetavarBindings::new());
/// let b = RangeWithMetavars::new(5, 15, MetavarBindings::new());
/// let merged = intersect(&a, &b).unwrap();
/// assert_eq!(merged.start, 5);
/// assert_eq!(merged.end, 10);
/// ```
pub fn intersect(a: &RangeWithMetavars, b: &RangeWithMetavars) -> Option<RangeWithMetavars> {
    // Ranges must share at least one byte.
    if !(a.start < b.end && b.start < a.end) {
        return None;
    }

    let start = a.start.max(b.start);
    let end = a.end.min(b.end);

    // Build merged bindings; a conflicting text value invalidates the intersection.
    // Only the bound text is compared for compatibility; positional metadata is
    // supplementary and does not affect whether two bindings agree.
    let mut bindings = a.bindings.clone();
    for (key, val) in &b.bindings {
        match bindings.get(key) {
            Some(existing) if existing.text != val.text => return None,
            _ => {
                bindings.insert(key.clone(), val.clone());
            }
        }
    }

    Some(RangeWithMetavars {
        start,
        end,
        bindings,
    })
}

/// Compute the union of multiple sets of ranges, deduplicating identical entries.
///
/// Ranges are deduplicated by `(start, end, bindings)` equality.
/// The result is sorted by `(start, end)` for deterministic output.
///
/// Ranges with the same position but different bindings represent distinct matches
/// and are kept as separate entries.
///
/// # Arguments
///
/// * `sets` - A list of range sets to merge.
///
/// # Returns
///
/// A deduplicated, sorted `Vec<RangeWithMetavars>`.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::engine::range::{RangeWithMetavars, MetavarBindings, union};
///
/// let r1 = RangeWithMetavars::new(0, 5, MetavarBindings::new());
/// let r2 = RangeWithMetavars::new(0, 5, MetavarBindings::new()); // duplicate
/// let r3 = RangeWithMetavars::new(10, 20, MetavarBindings::new());
/// let result = union(vec![vec![r1, r3], vec![r2]]);
/// assert_eq!(result.len(), 2);
/// ```
pub fn union(sets: Vec<Vec<RangeWithMetavars>>) -> Vec<RangeWithMetavars> {
    let mut all: Vec<RangeWithMetavars> = sets.into_iter().flatten().collect();
    all.sort();
    // dedup uses PartialEq, which compares start + end + bindings, so ranges with
    // the same position but different bindings are correctly kept as distinct entries.
    all.dedup();
    all
}

/// Remove positive ranges that are fully contained within any negative range.
///
/// A positive range `p` is removed if there exists a negative range `n` such
/// that `n.start <= p.start && p.end <= n.end`.
///
/// Metavariable bindings are NOT considered; containment is purely geometric.
/// The result is sorted by `(start, end)`.
///
/// # Arguments
///
/// * `positive` - The set of positive (candidate) matches.
/// * `negative` - The set of ranges to subtract (pattern-not matches).
///
/// # Returns
///
/// The positive ranges that are NOT fully contained in any negative range.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::engine::range::{RangeWithMetavars, MetavarBindings, subtract};
///
/// let pos = vec![
///     RangeWithMetavars::new(5, 10, MetavarBindings::new()),
///     RangeWithMetavars::new(0, 100, MetavarBindings::new()),
/// ];
/// let neg = vec![RangeWithMetavars::new(0, 50, MetavarBindings::new())];
/// let result = subtract(pos, &neg);
/// assert_eq!(result.len(), 1);
/// assert_eq!(result[0].start, 0); // large range NOT contained in neg
/// ```
pub fn subtract(
    positive: Vec<RangeWithMetavars>,
    negative: &[RangeWithMetavars],
) -> Vec<RangeWithMetavars> {
    if negative.is_empty() {
        return positive;
    }
    let mut result: Vec<RangeWithMetavars> = positive
        .into_iter()
        .filter(|p| !negative.iter().any(|n| n.contains(p)))
        .collect();
    result.sort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- RangeWithMetavars tests ---

    #[test]
    fn test_range_new_sets_all_fields() {
        let mut bindings = MetavarBindings::new();
        bindings.insert(
            "$X".to_string(),
            MetavarValue {
                text: "foo".to_string(),
                start: 0,
                end: 3,
            },
        );
        let r = RangeWithMetavars::new(3, 7, bindings.clone());
        assert_eq!(r.start, 3);
        assert_eq!(r.end, 7);
        assert_eq!(r.bindings, bindings);
    }

    #[test]
    fn test_range_overlaps_with_overlapping_ranges_returns_true() {
        let a = RangeWithMetavars::new(0, 10, MetavarBindings::new());
        let b = RangeWithMetavars::new(5, 15, MetavarBindings::new());
        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a));
    }

    #[test]
    fn test_range_overlaps_with_adjacent_nonoverlapping_ranges_returns_false() {
        // [0, 5) and [5, 10) share no bytes.
        let a = RangeWithMetavars::new(0, 5, MetavarBindings::new());
        let b = RangeWithMetavars::new(5, 10, MetavarBindings::new());
        assert!(!a.overlaps(&b));
        assert!(!b.overlaps(&a));
    }

    #[test]
    fn test_range_overlaps_with_same_range_returns_true() {
        let a = RangeWithMetavars::new(2, 8, MetavarBindings::new());
        let b = RangeWithMetavars::new(2, 8, MetavarBindings::new());
        assert!(a.overlaps(&b));
    }

    #[test]
    fn test_range_contains_with_fully_contained_returns_true() {
        // [0, 10) contains [2, 8).
        let outer = RangeWithMetavars::new(0, 10, MetavarBindings::new());
        let inner = RangeWithMetavars::new(2, 8, MetavarBindings::new());
        assert!(outer.contains(&inner));
    }

    #[test]
    fn test_range_contains_with_equal_range_returns_true() {
        // [0, 10) contains [0, 10).
        let a = RangeWithMetavars::new(0, 10, MetavarBindings::new());
        let b = RangeWithMetavars::new(0, 10, MetavarBindings::new());
        assert!(a.contains(&b));
    }

    #[test]
    fn test_range_contains_with_partial_overlap_returns_false() {
        // [0, 10) does NOT contain [5, 15).
        let a = RangeWithMetavars::new(0, 10, MetavarBindings::new());
        let b = RangeWithMetavars::new(5, 15, MetavarBindings::new());
        assert!(!a.contains(&b));
    }

    #[test]
    fn test_range_ord_sorts_by_start_then_end() {
        let mut ranges = [
            RangeWithMetavars::new(10, 20, MetavarBindings::new()),
            RangeWithMetavars::new(0, 5, MetavarBindings::new()),
            RangeWithMetavars::new(0, 10, MetavarBindings::new()),
            RangeWithMetavars::new(5, 15, MetavarBindings::new()),
        ];
        ranges.sort();
        assert_eq!((ranges[0].start, ranges[0].end), (0, 5));
        assert_eq!((ranges[1].start, ranges[1].end), (0, 10));
        assert_eq!((ranges[2].start, ranges[2].end), (5, 15));
        assert_eq!((ranges[3].start, ranges[3].end), (10, 20));
    }

    // --- intersect tests ---

    #[test]
    fn test_intersect_overlapping_ranges_returns_merged_range() {
        let a = RangeWithMetavars::new(0, 10, MetavarBindings::new());
        let b = RangeWithMetavars::new(5, 15, MetavarBindings::new());
        // SAFETY: ranges are known to overlap.
        let merged = intersect(&a, &b).unwrap();
        assert_eq!(merged.start, 5);
        assert_eq!(merged.end, 10);
    }

    #[test]
    fn test_intersect_non_overlapping_returns_none() {
        let a = RangeWithMetavars::new(0, 5, MetavarBindings::new());
        let b = RangeWithMetavars::new(10, 15, MetavarBindings::new());
        assert!(intersect(&a, &b).is_none());
    }

    #[test]
    fn test_intersect_adjacent_ranges_return_none() {
        // [0, 5) and [5, 10) share no bytes.
        let a = RangeWithMetavars::new(0, 5, MetavarBindings::new());
        let b = RangeWithMetavars::new(5, 10, MetavarBindings::new());
        assert!(intersect(&a, &b).is_none());
    }

    #[test]
    fn test_intersect_fully_contained_range_returns_inner() {
        let outer = RangeWithMetavars::new(0, 20, MetavarBindings::new());
        let inner = RangeWithMetavars::new(5, 10, MetavarBindings::new());
        // SAFETY: inner is fully within outer.
        let merged = intersect(&outer, &inner).unwrap();
        assert_eq!(merged.start, 5);
        assert_eq!(merged.end, 10);
    }

    #[test]
    fn test_intersect_compatible_bindings_are_merged() {
        let mut b1 = MetavarBindings::new();
        b1.insert(
            "$X".to_string(),
            MetavarValue {
                text: "foo".to_string(),
                start: 0,
                end: 3,
            },
        );
        let mut b2 = MetavarBindings::new();
        b2.insert(
            "$Y".to_string(),
            MetavarValue {
                text: "bar".to_string(),
                start: 10,
                end: 13,
            },
        );
        let a = RangeWithMetavars::new(0, 10, b1);
        let b = RangeWithMetavars::new(3, 13, b2);
        // SAFETY: ranges overlap and bindings are disjoint (compatible).
        let merged = intersect(&a, &b).unwrap();
        assert_eq!(
            merged.bindings.get("$X").map(|v| v.text.as_str()),
            Some("foo")
        );
        assert_eq!(
            merged.bindings.get("$Y").map(|v| v.text.as_str()),
            Some("bar")
        );
    }

    #[test]
    fn test_intersect_conflicting_bindings_returns_none() {
        let mut b1 = MetavarBindings::new();
        b1.insert(
            "$X".to_string(),
            MetavarValue {
                text: "foo".to_string(),
                start: 0,
                end: 3,
            },
        );
        let mut b2 = MetavarBindings::new();
        b2.insert(
            "$X".to_string(),
            MetavarValue {
                text: "bar".to_string(),
                start: 0,
                end: 3,
            },
        );
        let a = RangeWithMetavars::new(0, 10, b1);
        let b = RangeWithMetavars::new(5, 15, b2);
        assert!(intersect(&a, &b).is_none());
    }

    #[test]
    fn test_intersect_same_binding_both_sides_returns_some() {
        let mut b1 = MetavarBindings::new();
        b1.insert(
            "$X".to_string(),
            MetavarValue {
                text: "foo".to_string(),
                start: 0,
                end: 3,
            },
        );
        let mut b2 = MetavarBindings::new();
        b2.insert(
            "$X".to_string(),
            MetavarValue {
                text: "foo".to_string(),
                start: 0,
                end: 3,
            },
        );
        let a = RangeWithMetavars::new(0, 10, b1);
        let b = RangeWithMetavars::new(5, 15, b2);
        // SAFETY: identical binding text is compatible.
        let merged = intersect(&a, &b).unwrap();
        assert_eq!(
            merged.bindings.get("$X").map(|v| v.text.as_str()),
            Some("foo")
        );
    }

    #[test]
    fn test_intersect_empty_bindings_on_both_sides() {
        let a = RangeWithMetavars::new(1, 8, MetavarBindings::new());
        let b = RangeWithMetavars::new(4, 12, MetavarBindings::new());
        // SAFETY: ranges are known to overlap.
        let merged = intersect(&a, &b).unwrap();
        assert_eq!(merged.start, 4);
        assert_eq!(merged.end, 8);
        assert!(merged.bindings.is_empty());
    }

    // --- union tests ---

    #[test]
    fn test_union_empty_input_returns_empty() {
        let result = union(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_union_single_set_returns_sorted() {
        let set = vec![
            RangeWithMetavars::new(10, 20, MetavarBindings::new()),
            RangeWithMetavars::new(0, 5, MetavarBindings::new()),
        ];
        let result = union(vec![set]);
        assert_eq!(result.len(), 2);
        assert!(result[0] < result[1]);
    }

    #[test]
    fn test_union_deduplicates_identical_ranges() {
        let r1 = RangeWithMetavars::new(0, 5, MetavarBindings::new());
        let r2 = RangeWithMetavars::new(0, 5, MetavarBindings::new()); // duplicate
        let r3 = RangeWithMetavars::new(10, 20, MetavarBindings::new());
        let result = union(vec![vec![r1, r3], vec![r2]]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_union_keeps_different_bindings_as_distinct() {
        let mut b1 = MetavarBindings::new();
        b1.insert(
            "$X".to_string(),
            MetavarValue {
                text: "foo".to_string(),
                start: 0,
                end: 3,
            },
        );
        let mut b2 = MetavarBindings::new();
        b2.insert(
            "$X".to_string(),
            MetavarValue {
                text: "bar".to_string(),
                start: 0,
                end: 3,
            },
        );
        let r1 = RangeWithMetavars::new(0, 5, b1);
        let r2 = RangeWithMetavars::new(0, 5, b2);
        let result = union(vec![vec![r1], vec![r2]]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_union_result_is_sorted_by_start_end() {
        let r1 = RangeWithMetavars::new(20, 30, MetavarBindings::new());
        let r2 = RangeWithMetavars::new(0, 10, MetavarBindings::new());
        let r3 = RangeWithMetavars::new(5, 15, MetavarBindings::new());
        let result = union(vec![vec![r1], vec![r2, r3]]);
        assert_eq!(result.len(), 3);
        for w in result.windows(2) {
            assert!(w[0] <= w[1]);
        }
    }

    // --- subtract tests ---

    #[test]
    fn test_subtract_empty_negative_returns_all_positive() {
        let pos = vec![
            RangeWithMetavars::new(0, 10, MetavarBindings::new()),
            RangeWithMetavars::new(20, 30, MetavarBindings::new()),
        ];
        let result = subtract(pos.clone(), &[]);
        assert_eq!(result, pos);
    }

    #[test]
    fn test_subtract_contained_range_is_removed() {
        let pos = vec![RangeWithMetavars::new(5, 10, MetavarBindings::new())];
        let neg = vec![RangeWithMetavars::new(0, 20, MetavarBindings::new())];
        let result = subtract(pos, &neg);
        assert!(result.is_empty());
    }

    #[test]
    fn test_subtract_non_contained_range_is_kept() {
        let pos = vec![RangeWithMetavars::new(0, 20, MetavarBindings::new())];
        let neg = vec![RangeWithMetavars::new(5, 10, MetavarBindings::new())];
        let result = subtract(pos, &neg);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].start, 0);
        assert_eq!(result[0].end, 20);
    }

    #[test]
    fn test_subtract_equal_range_is_removed() {
        // Exact match counts as fully contained.
        let pos = vec![RangeWithMetavars::new(5, 15, MetavarBindings::new())];
        let neg = vec![RangeWithMetavars::new(5, 15, MetavarBindings::new())];
        let result = subtract(pos, &neg);
        assert!(result.is_empty());
    }

    #[test]
    fn test_subtract_partial_overlap_is_kept() {
        // [0, 10) overlaps [5, 15) but is not fully contained.
        let pos = vec![RangeWithMetavars::new(0, 10, MetavarBindings::new())];
        let neg = vec![RangeWithMetavars::new(5, 15, MetavarBindings::new())];
        let result = subtract(pos, &neg);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].start, 0);
    }

    #[test]
    fn test_subtract_multiple_negatives() {
        let pos = vec![
            RangeWithMetavars::new(5, 10, MetavarBindings::new()), // inside neg[0]
            RangeWithMetavars::new(55, 60, MetavarBindings::new()), // inside neg[1]
            RangeWithMetavars::new(0, 100, MetavarBindings::new()), // not fully contained
        ];
        let neg = vec![
            RangeWithMetavars::new(0, 20, MetavarBindings::new()),
            RangeWithMetavars::new(50, 70, MetavarBindings::new()),
        ];
        let result = subtract(pos, &neg);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].start, 0);
        assert_eq!(result[0].end, 100);
    }

    #[test]
    fn test_subtract_result_is_sorted() {
        let pos = vec![
            RangeWithMetavars::new(20, 25, MetavarBindings::new()),
            RangeWithMetavars::new(5, 8, MetavarBindings::new()), // removed: inside [0, 15)
            RangeWithMetavars::new(0, 30, MetavarBindings::new()),
        ];
        let neg = vec![RangeWithMetavars::new(0, 15, MetavarBindings::new())];
        let result = subtract(pos, &neg);
        for w in result.windows(2) {
            assert!(w[0] <= w[1]);
        }
    }

    // --- Property test ---

    #[test]
    fn test_subtract_length_never_exceeds_positive_length_property() {
        // Property: subtract(pos, neg).len() <= pos.len() for any pos, neg.
        // Verified across >10,000 generated cases using nested coordinate ranges.
        let max_coord = 100usize;
        let mut case_count = 0u32;
        for start_a in (0..max_coord).step_by(2) {
            for end_a in (start_a + 1)..=max_coord {
                for start_b in (0..max_coord).step_by(3) {
                    for end_b in (start_b + 1)..=max_coord {
                        let pos = vec![RangeWithMetavars::new(
                            start_a,
                            end_a,
                            MetavarBindings::new(),
                        )];
                        let neg = vec![RangeWithMetavars::new(
                            start_b,
                            end_b,
                            MetavarBindings::new(),
                        )];
                        let result = subtract(pos.clone(), &neg);
                        assert!(
                            result.len() <= pos.len(),
                            "property failed: pos_len={}, result_len={}, \
                             case=({},{}) subtracted by ({},{})",
                            pos.len(),
                            result.len(),
                            start_a,
                            end_a,
                            start_b,
                            end_b
                        );
                        case_count += 1;
                    }
                }
            }
        }
        assert!(
            case_count >= 10_000,
            "must generate at least 10,000 cases, got {}",
            case_count
        );
    }
}
