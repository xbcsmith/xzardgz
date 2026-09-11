//! Compiled-pattern cache for AST-mode pattern matching.
//!
//! [`PatternCompiler`] wraps the ast-grep [`Pattern`] compilation step and
//! caches the results so that the same pattern string (and strictness level)
//! is never compiled more than once per scan session.
//!
//! The cache is protected by a [`std::sync::Mutex`] so the same
//! [`PatternCompiler`] can be shared safely across multiple Rayon worker
//! threads in Phase 4.
//!
//! Semgrep ellipsis syntax (`...`) is transparently rewritten to ast-grep
//! multi-metavariable syntax (`$$$`) before compilation. See
//! [`rewrite_ellipsis`] for details.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ast_grep_core::{MatchStrictness, Pattern};
use ast_grep_language::SupportLang;

use crate::scanner::sast::error::SastError;

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Map a [`MatchStrictness`] variant to a stable `u8` discriminant.
///
/// The discriminant is used as part of the [`PatternCompiler`] cache key so
/// that the same pattern text compiled with different strictness levels
/// produces independent cache entries.
///
/// # Arguments
///
/// * `s` - The strictness level to convert.
///
/// # Returns
///
/// A `u8` in the range `0..=5` that uniquely identifies the variant:
/// `Cst=0`, `Smart=1`, `Ast=2`, `Relaxed=3`, `Signature=4`, `Template=5`.
fn strictness_to_u8(s: MatchStrictness) -> u8 {
    match s {
        MatchStrictness::Cst => 0,
        MatchStrictness::Smart => 1,
        MatchStrictness::Ast => 2,
        MatchStrictness::Relaxed => 3,
        MatchStrictness::Signature => 4,
        MatchStrictness::Template => 5,
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Rewrite Semgrep `...` ellipsis syntax to ast-grep `$$$` multi-metavar syntax.
///
/// In Semgrep patterns, `...` (three dots) is used as a wildcard that matches
/// zero or more elements in argument lists, statement sequences, parameter
/// lists, and array/slice literals. In ast-grep the equivalent is `$$$`.
///
/// This function replaces ALL occurrences of the three-character sequence `...`
/// with `$$$`. It is a simple string substitution; context-awareness is
/// provided by tree-sitter's grammar, which parses `$$$` correctly in each
/// syntactic position.
///
/// Note: Rust's two-character range operator `..` is not affected because
/// only the exact three-character sequence `...` is replaced.
///
/// # Arguments
///
/// * `pattern` - A Semgrep pattern string possibly containing `...`.
///
/// # Returns
///
/// A new `String` with all `...` replaced by `$$$`.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::engine::pattern::rewrite_ellipsis;
///
/// assert_eq!(rewrite_ellipsis("foo(...)"), "foo($$$)");
/// assert_eq!(rewrite_ellipsis("fn $F(..) {}"), "fn $F(..) {}");
/// assert_eq!(rewrite_ellipsis("$X = 1"), "$X = 1");
/// ```
pub fn rewrite_ellipsis(pattern: &str) -> String {
    pattern.replace("...", "$$$")
}

// ---------------------------------------------------------------------------
// PatternCompiler
// ---------------------------------------------------------------------------

/// Thread-safe cache of compiled ast-grep [`Pattern`] instances.
///
/// Compiling a [`Pattern`] from a source string is non-trivial (it invokes
/// tree-sitter parsing). `PatternCompiler` amortises this cost across all
/// rules and files in a scan session by caching compiled patterns keyed on
/// `(rewritten_pattern_text, language_name, strictness_index)`.
///
/// The cache is protected by a [`Mutex`] so the same `PatternCompiler` can
/// be shared across multiple Rayon worker threads added in Phase 4.
///
/// # Examples
///
/// ```
/// use ast_grep_core::MatchStrictness;
/// use ast_grep_language::SupportLang;
/// use xzardgz::scanner::sast::engine::pattern::PatternCompiler;
///
/// let compiler = PatternCompiler::new();
/// let p1 = compiler
///     .compile("$X + $Y", SupportLang::Rust, MatchStrictness::Relaxed)
///     .expect("pattern must compile");
/// let p2 = compiler
///     .compile("$X + $Y", SupportLang::Rust, MatchStrictness::Relaxed)
///     .expect("second compile must hit cache");
/// assert!(std::sync::Arc::ptr_eq(&p1, &p2), "cache must return the same Arc");
/// ```
pub struct PatternCompiler {
    // Key: (rewritten_pattern, language_debug_name, strictness_index)
    cache: Mutex<HashMap<(String, String, u8), Arc<Pattern>>>,
}

impl PatternCompiler {
    /// Create a new, empty `PatternCompiler`.
    ///
    /// # Returns
    ///
    /// A `PatternCompiler` with an empty cache ready for use.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::engine::pattern::PatternCompiler;
    ///
    /// let compiler = PatternCompiler::new();
    /// assert_eq!(compiler.cache_len(), 0);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Compile a Semgrep pattern string for the given language and strictness.
    ///
    /// The `...` ellipsis in `pattern_text` is rewritten to `$$$` before
    /// compilation. The result is cached; subsequent calls with the same
    /// arguments return the cached `Arc<Pattern>` without re-parsing.
    ///
    /// # Arguments
    ///
    /// * `pattern_text` - A Semgrep pattern string (may contain `...`).
    /// * `lang` - The tree-sitter language to compile the pattern for.
    /// * `strictness` - Match strictness level applied after compilation.
    ///
    /// # Returns
    ///
    /// `Arc<Pattern>` on success.
    ///
    /// # Errors
    ///
    /// Returns [`SastError::Internal`] if ast-grep's `Pattern::try_new` fails
    /// (for example, if the pattern string is empty or syntactically invalid).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast_grep_core::MatchStrictness;
    /// use ast_grep_language::SupportLang;
    /// use xzardgz::scanner::sast::engine::pattern::PatternCompiler;
    ///
    /// let compiler = PatternCompiler::new();
    /// let result = compiler.compile("$X + $Y", SupportLang::Rust, MatchStrictness::Relaxed);
    /// assert!(result.is_ok());
    /// ```
    pub fn compile(
        &self,
        pattern_text: &str,
        lang: SupportLang,
        strictness: MatchStrictness,
    ) -> Result<Arc<Pattern>, SastError> {
        let rewritten = rewrite_ellipsis(pattern_text);
        // Clone strictness before consuming it in strictness_to_u8 so that
        // the original value remains available for Pattern::with_strictness.
        let strictness_idx = strictness_to_u8(strictness.clone());
        let key = (rewritten.clone(), format!("{lang:?}"), strictness_idx);

        // Fast path: return the cached Arc without re-compiling.
        {
            // SAFETY: A poisoned lock means another thread panicked while
            // holding the guard. Propagating the poison is the correct
            // recovery strategy here.
            let cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(&key) {
                return Ok(Arc::clone(cached));
            }
        }

        // Slow path: compile the pattern outside the lock so that other
        // threads can continue serving cache hits concurrently.
        let pattern = Pattern::try_new(&rewritten, lang).map_err(|e| {
            SastError::Internal(format!("pattern compile error for '{rewritten}': {e}"))
        })?;
        let pattern = pattern.with_strictness(strictness);
        let arc = Arc::new(pattern);

        // Re-acquire the lock to insert. A racing thread may have already
        // inserted an entry for the same key; use `or_insert_with` so we
        // always return whichever Arc was inserted first.
        // SAFETY: Same poison rationale as above.
        let mut cache = self.cache.lock().unwrap();
        let cached = cache.entry(key).or_insert_with(|| Arc::clone(&arc));
        Ok(Arc::clone(cached))
    }

    /// Return the number of entries currently held in the compiled-pattern cache.
    ///
    /// Each unique `(rewritten_pattern, language, strictness)` triple counts
    /// as one entry. Useful in tests to assert cache growth behaviour.
    ///
    /// # Returns
    ///
    /// Number of cache entries.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast_grep_core::MatchStrictness;
    /// use ast_grep_language::SupportLang;
    /// use xzardgz::scanner::sast::engine::pattern::PatternCompiler;
    ///
    /// let compiler = PatternCompiler::new();
    /// assert_eq!(compiler.cache_len(), 0);
    /// compiler
    ///     .compile("$X", SupportLang::Rust, MatchStrictness::Relaxed)
    ///     .unwrap();
    /// assert_eq!(compiler.cache_len(), 1);
    /// ```
    #[must_use]
    pub fn cache_len(&self) -> usize {
        // SAFETY: A poisoned lock means another thread panicked while holding
        // the guard. Propagating the poison is the correct recovery strategy.
        self.cache.lock().unwrap().len()
    }
}

impl Default for PatternCompiler {
    /// Create a default (empty) `PatternCompiler`.
    ///
    /// Equivalent to [`PatternCompiler::new`].
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ast_grep_core::tree_sitter::LanguageExt;

    // -----------------------------------------------------------------------
    // rewrite_ellipsis
    // -----------------------------------------------------------------------

    #[test]
    fn test_rewrite_ellipsis_three_dots_replaced() {
        assert_eq!(rewrite_ellipsis("foo(...)"), "foo($$$)");
    }

    #[test]
    fn test_rewrite_ellipsis_two_dots_not_replaced() {
        assert_eq!(rewrite_ellipsis("foo(..)"), "foo(..)");
    }

    #[test]
    fn test_rewrite_ellipsis_no_dots() {
        assert_eq!(rewrite_ellipsis("$X + $Y"), "$X + $Y");
    }

    #[test]
    fn test_rewrite_ellipsis_multiple_occurrences() {
        assert_eq!(rewrite_ellipsis("f(..., ...)"), "f($$$, $$$)");
    }

    #[test]
    fn test_rewrite_ellipsis_at_start() {
        // "..." is replaced with "$$$"; the trailing "$X" is unchanged,
        // so the full result is "$$$$X" (three dollars from replacement + one
        // dollar from the metavar prefix).
        assert_eq!(rewrite_ellipsis("...$X"), "$$$$X");
    }

    // -----------------------------------------------------------------------
    // PatternCompiler::new / Default
    // -----------------------------------------------------------------------

    #[test]
    fn test_pattern_compiler_new_creates_empty_cache() {
        let compiler = PatternCompiler::new();
        assert_eq!(
            compiler.cache_len(),
            0,
            "a freshly constructed PatternCompiler must have an empty cache"
        );
    }

    // -----------------------------------------------------------------------
    // PatternCompiler::compile - success paths
    // -----------------------------------------------------------------------

    #[test]
    fn test_pattern_compiler_compile_rust_pattern_succeeds() {
        let compiler = PatternCompiler::new();
        let result = compiler.compile("$X + $Y", SupportLang::Rust, MatchStrictness::Relaxed);
        assert!(
            result.is_ok(),
            "binary-expression pattern must compile: {result:?}"
        );
    }

    #[test]
    fn test_pattern_compiler_compile_ellipsis_pattern_succeeds() {
        let compiler = PatternCompiler::new();
        // "foo(...)" is rewritten to "foo($$$)" before compilation.
        let result = compiler.compile("foo(...)", SupportLang::Rust, MatchStrictness::Relaxed);
        assert!(
            result.is_ok(),
            "ellipsis call pattern must compile after rewrite: {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // PatternCompiler::compile - failure path
    // -----------------------------------------------------------------------

    #[test]
    fn test_pattern_compiler_compile_invalid_pattern_returns_error() {
        let compiler = PatternCompiler::new();
        // An empty string has no content; Pattern::try_new returns
        // PatternError::NoContent, wrapped here as SastError::Internal.
        let result = compiler.compile("", SupportLang::Rust, MatchStrictness::Relaxed);
        assert!(result.is_err(), "empty pattern string must return an error");
        assert!(
            matches!(result.unwrap_err(), SastError::Internal(_)),
            "compile error must be wrapped as SastError::Internal"
        );
    }

    // -----------------------------------------------------------------------
    // PatternCompiler::compile - caching behaviour
    // -----------------------------------------------------------------------

    #[test]
    fn test_pattern_compiler_compile_caches_result() {
        let compiler = PatternCompiler::new();
        let p1 = compiler
            .compile("$X", SupportLang::Rust, MatchStrictness::Relaxed)
            .expect("first compile must succeed");
        let p2 = compiler
            .compile("$X", SupportLang::Rust, MatchStrictness::Relaxed)
            .expect("second compile must succeed");
        assert!(
            Arc::ptr_eq(&p1, &p2),
            "second call with identical arguments must return the same Arc"
        );
        assert_eq!(
            compiler.cache_len(),
            1,
            "cache must contain exactly one entry after two identical compiles"
        );
    }

    #[test]
    fn test_pattern_compiler_different_strictness_produces_different_entries() {
        let compiler = PatternCompiler::new();
        let p_relaxed = compiler
            .compile("$X", SupportLang::Rust, MatchStrictness::Relaxed)
            .expect("Relaxed compile must succeed");
        let p_smart = compiler
            .compile("$X", SupportLang::Rust, MatchStrictness::Smart)
            .expect("Smart compile must succeed");

        assert!(
            !Arc::ptr_eq(&p_relaxed, &p_smart),
            "different strictness levels must produce distinct cache entries"
        );
        assert_eq!(
            compiler.cache_len(),
            2,
            "cache must contain two entries for the same pattern with different strictness"
        );
    }

    // -----------------------------------------------------------------------
    // PatternCompiler::compile - pattern matching integration
    // -----------------------------------------------------------------------

    #[test]
    fn test_pattern_compiler_compile_pattern_matches_ast_node() {
        let compiler = PatternCompiler::new();
        let pattern = compiler
            .compile(
                "fn $FNAME() {}",
                SupportLang::Rust,
                MatchStrictness::Relaxed,
            )
            .expect("function pattern must compile");

        let root = SupportLang::Rust.ast_grep("fn foo() {}");
        let matched = root.root().find(&*pattern);
        assert!(
            matched.is_some(),
            "compiled pattern must match the corresponding Rust source node"
        );
    }

    #[test]
    fn test_pattern_compiler_compile_pattern_does_not_match_unrelated_node() {
        let compiler = PatternCompiler::new();
        let pattern = compiler
            .compile(
                "fn $FNAME() {}",
                SupportLang::Rust,
                MatchStrictness::Relaxed,
            )
            .expect("function pattern must compile");

        // A struct declaration must not match a function pattern.
        let root = SupportLang::Rust.ast_grep("struct Foo {}");
        let matched = root.root().find(&*pattern);
        assert!(
            matched.is_none(),
            "function pattern must not match a struct declaration"
        );
    }

    // -----------------------------------------------------------------------
    // strictness_to_u8 - discriminant stability
    // -----------------------------------------------------------------------

    #[test]
    fn test_strictness_to_u8_all_variants_are_distinct() {
        let values = [
            strictness_to_u8(MatchStrictness::Cst),
            strictness_to_u8(MatchStrictness::Smart),
            strictness_to_u8(MatchStrictness::Ast),
            strictness_to_u8(MatchStrictness::Relaxed),
            strictness_to_u8(MatchStrictness::Signature),
            strictness_to_u8(MatchStrictness::Template),
        ];
        let unique: std::collections::HashSet<u8> = values.iter().copied().collect();
        assert_eq!(
            unique.len(),
            values.len(),
            "every MatchStrictness variant must map to a unique u8"
        );
    }

    #[test]
    fn test_strictness_to_u8_relaxed_is_three() {
        // Relaxed is the Phase 2 default; its index is documented as 3.
        assert_eq!(strictness_to_u8(MatchStrictness::Relaxed), 3);
    }
}
