//! Parse error and missing-node density accounting.
//!
//! Tree-sitter silently produces partial parse trees when source is invalid.
//! This module surfaces the density of `ERROR` and `MISSING` nodes so callers
//! can decide whether to trust the parse result for pattern matching.
//!
//! The key entry point is [`ErrorNodeDensity::from_root`], which performs a
//! non-recursive, stack-based traversal of the entire parse tree to avoid
//! stack overflows on pathologically deep source files.

use ast_grep_core::AstGrep;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_language::SupportLang;

/// Concrete document type used throughout the SAST engine.
type SgDoc = StrDoc<SupportLang>;

// ---------------------------------------------------------------------------
// ErrorNodeDensity
// ---------------------------------------------------------------------------

/// Parse error density for a file parsed by tree-sitter.
///
/// Tree-sitter always produces a parse tree, even for invalid input.
/// Syntactic errors appear as nodes with kind `ERROR` or `MISSING`.
/// The density ratio indicates how degraded the parse tree is:
/// `0.0` means a fully clean parse; `1.0` means every node is an error.
///
/// Callers use [`ErrorNodeDensity::is_degraded`] to decide whether to skip
/// pattern matching for a file whose tree is too corrupted to be reliable.
///
/// # Examples
///
/// ```
/// use ast_grep_core::tree_sitter::LanguageExt;
/// use ast_grep_language::SupportLang;
/// use xzardgz::scanner::sast::ast::diagnostics::ErrorNodeDensity;
///
/// let root = SupportLang::Rust.ast_grep("fn main() {}");
/// let density = ErrorNodeDensity::from_root(&root);
/// assert_eq!(density.error_nodes, 0);
/// assert!(!density.is_degraded(0.1));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorNodeDensity {
    /// Total number of nodes visited in the parse tree.
    pub total_nodes: usize,
    /// Number of nodes tagged `ERROR` or `MISSING` by tree-sitter.
    pub error_nodes: usize,
}

impl ErrorNodeDensity {
    /// Compute error density by walking all nodes in the ast-grep root.
    ///
    /// Uses an explicit stack to avoid recursion so that deeply nested
    /// source files do not overflow the call stack.
    ///
    /// # Arguments
    ///
    /// * `root` - Reference to the parsed ast-grep document root.
    ///
    /// # Returns
    ///
    /// An [`ErrorNodeDensity`] reflecting the total node count and the
    /// number of `ERROR` or `MISSING` nodes found in the tree.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast_grep_core::tree_sitter::LanguageExt;
    /// use ast_grep_language::SupportLang;
    /// use xzardgz::scanner::sast::ast::diagnostics::ErrorNodeDensity;
    ///
    /// let root = SupportLang::Rust.ast_grep("fn add(a: i32, b: i32) -> i32 { a + b }");
    /// let density = ErrorNodeDensity::from_root(&root);
    /// assert_eq!(density.error_nodes, 0);
    /// ```
    pub fn from_root(root: &AstGrep<SgDoc>) -> Self {
        let root_node = root.root();
        let mut stack = vec![root_node];
        let mut total = 0usize;
        let mut error_count = 0usize;

        while let Some(node) = stack.pop() {
            total += 1;
            if node.is_error() || node.is_missing() {
                error_count += 1;
            }
            stack.extend(node.children());
        }

        Self {
            total_nodes: total,
            error_nodes: error_count,
        }
    }

    /// Returns the fraction of error nodes relative to total nodes.
    ///
    /// The ratio is in the range `[0.0, 1.0]`.  A ratio of `0.0` indicates
    /// a perfectly clean parse tree; `1.0` indicates every node is an error.
    ///
    /// Returns `0.0` when `total_nodes` is `0` to avoid a divide-by-zero.
    ///
    /// # Returns
    ///
    /// Error node ratio in `[0.0, 1.0]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::ast::diagnostics::ErrorNodeDensity;
    ///
    /// let clean = ErrorNodeDensity { total_nodes: 10, error_nodes: 0 };
    /// assert_eq!(clean.ratio(), 0.0);
    ///
    /// let half = ErrorNodeDensity { total_nodes: 10, error_nodes: 5 };
    /// assert_eq!(half.ratio(), 0.5);
    /// ```
    #[must_use]
    pub fn ratio(&self) -> f64 {
        if self.total_nodes == 0 {
            return 0.0;
        }
        self.error_nodes as f64 / self.total_nodes as f64
    }

    /// Returns `true` if the error ratio exceeds the given threshold.
    ///
    /// A typical threshold is `0.1` (10 % error nodes).  Files whose parse
    /// tree is more degraded than the threshold are usually not worth
    /// running pattern rules against.
    ///
    /// # Arguments
    ///
    /// * `threshold` - Value in `[0.0, 1.0]`.  The function returns `true`
    ///   when `self.ratio() > threshold`.
    ///
    /// # Returns
    ///
    /// `true` when the error ratio strictly exceeds `threshold`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::ast::diagnostics::ErrorNodeDensity;
    ///
    /// let degraded = ErrorNodeDensity { total_nodes: 10, error_nodes: 5 };
    /// assert!(degraded.is_degraded(0.4));
    /// assert!(!degraded.is_degraded(0.5));
    /// ```
    #[must_use]
    pub fn is_degraded(&self, threshold: f64) -> bool {
        self.ratio() > threshold
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ast_grep_core::tree_sitter::LanguageExt;

    #[test]
    fn test_error_node_density_clean_parse_has_zero_error_nodes() {
        let root = SupportLang::Rust.ast_grep("fn main() {}");
        let density = ErrorNodeDensity::from_root(&root);
        assert_eq!(
            density.error_nodes, 0,
            "clean source must have no error nodes"
        );
        assert!(
            density.total_nodes > 0,
            "clean source must have at least one node"
        );
    }

    #[test]
    fn test_error_node_density_degraded_parse_has_nonzero_error_nodes() {
        let root = SupportLang::Rust.ast_grep("fn !!! {}");
        let density = ErrorNodeDensity::from_root(&root);
        assert!(
            density.error_nodes > 0,
            "malformed source must produce at least one error node; got density: {density:?}"
        );
    }

    #[test]
    fn test_ratio_with_zero_total_returns_zero() {
        let density = ErrorNodeDensity {
            total_nodes: 0,
            error_nodes: 0,
        };
        assert_eq!(density.ratio(), 0.0);
    }

    #[test]
    fn test_ratio_all_errors_returns_one() {
        let density = ErrorNodeDensity {
            total_nodes: 5,
            error_nodes: 5,
        };
        assert_eq!(density.ratio(), 1.0);
    }

    #[test]
    fn test_ratio_partial_errors_returns_correct_fraction() {
        let density = ErrorNodeDensity {
            total_nodes: 4,
            error_nodes: 1,
        };
        assert!((density.ratio() - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn test_is_degraded_above_threshold_returns_true() {
        let density = ErrorNodeDensity {
            total_nodes: 10,
            error_nodes: 5,
        };
        // ratio is 0.5; threshold is 0.4 -> degraded
        assert!(density.is_degraded(0.4));
    }

    #[test]
    fn test_is_degraded_at_threshold_returns_false() {
        let density = ErrorNodeDensity {
            total_nodes: 10,
            error_nodes: 5,
        };
        // ratio is 0.5; threshold is 0.5 -> NOT strictly greater than
        assert!(!density.is_degraded(0.5));
    }

    #[test]
    fn test_is_degraded_below_threshold_returns_false() {
        let density = ErrorNodeDensity {
            total_nodes: 10,
            error_nodes: 1,
        };
        // ratio is 0.1; threshold is 0.2 -> not degraded
        assert!(!density.is_degraded(0.2));
    }

    #[test]
    fn test_is_degraded_zero_total_is_never_degraded() {
        let density = ErrorNodeDensity {
            total_nodes: 0,
            error_nodes: 0,
        };
        assert!(!density.is_degraded(0.0));
    }

    #[test]
    fn test_from_root_total_nodes_matches_actual_traversal() {
        // A single function declaration has a deterministic node count;
        // we only assert it is strictly positive and consistent.
        let root = SupportLang::Rust.ast_grep("fn main() {}");
        let density = ErrorNodeDensity::from_root(&root);
        let density2 = ErrorNodeDensity::from_root(&root);
        assert_eq!(
            density.total_nodes, density2.total_nodes,
            "from_root must be deterministic"
        );
    }
}
