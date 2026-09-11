//! Parse cache: one parse per (path, Language) pair per scan session.
//!
//! [`ParseCache`] amortises the cost of tree-sitter parsing across multiple
//! rules that all match against the same source file.  The cache is
//! concurrency-safe via a [`Mutex`]-protected [`HashMap`] so that multiple
//! Rayon worker threads can share it safely.
//!
//! Only AST-backed languages (currently [`Language::Rust`]) produce cache
//! entries.  Calls for [`Language::Regex`] or [`Language::Generic`] return
//! `Ok(None)` immediately without touching the cache.

use super::diagnostics::ErrorNodeDensity;
use super::lang::Language;
use crate::scanner::sast::config::SastEngineConfig;
use crate::scanner::sast::error::SastError;
use ast_grep_core::AstGrep;
use ast_grep_core::tree_sitter::{LanguageExt, StrDoc};
use ast_grep_language::SupportLang;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Concrete document type used throughout the SAST engine.
type SgDoc = StrDoc<SupportLang>;

// ---------------------------------------------------------------------------
// CachedRoot
// ---------------------------------------------------------------------------

/// A parsed AST root together with its pre-computed error density.
///
/// Instances are stored inside an [`Arc`] so that multiple rule evaluators
/// can hold references to the same parse without copying the tree.
pub struct CachedRoot {
    // NOTE: AstGrep<SgDoc> does not implement Debug, so a manual impl
    // is provided below that omits the raw tree for brevity.
    /// The parsed ast-grep document root.
    pub root: AstGrep<SgDoc>,
    /// Pre-computed error node density for this parse.
    pub density: ErrorNodeDensity,
}

impl std::fmt::Debug for CachedRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedRoot")
            .field("density", &self.density)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// ParseCache
// ---------------------------------------------------------------------------

/// Concurrency-safe parse cache keyed on `(PathBuf, Language)`.
///
/// A single `ParseCache` instance is created at the start of a scan session
/// and shared (via `Arc<ParseCache>`) across all rule-evaluation workers.
/// The first worker to request a `(path, lang)` pair performs the parse and
/// inserts the result; subsequent requests for the same pair return the
/// cached [`Arc<CachedRoot>`] without re-parsing.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use xzardgz::scanner::sast::ast::parse::ParseCache;
/// use xzardgz::scanner::sast::ast::lang::Language;
///
/// let cache = ParseCache::new(5_242_880);
/// let result = cache.get_or_parse(Path::new("src/main.rs"), Language::Rust);
/// assert!(result.is_ok());
/// ```
pub struct ParseCache {
    cache: Mutex<HashMap<(PathBuf, Language), Arc<CachedRoot>>>,
    max_file_bytes: u64,
    parse_count: AtomicUsize,
}

impl ParseCache {
    /// Create a new, empty `ParseCache` with the given file size limit.
    ///
    /// Files whose size exceeds `max_file_bytes` are not parsed and
    /// [`get_or_parse`](ParseCache::get_or_parse) returns `Ok(None)` for them.
    ///
    /// # Arguments
    ///
    /// * `max_file_bytes` - Maximum file size in bytes that will be parsed.
    ///
    /// # Returns
    ///
    /// A fresh, empty `ParseCache`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::ast::parse::ParseCache;
    ///
    /// let cache = ParseCache::new(5_242_880);
    /// assert_eq!(cache.parse_count(), 0);
    /// ```
    #[must_use]
    pub fn new(max_file_bytes: u64) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            max_file_bytes,
            parse_count: AtomicUsize::new(0),
        }
    }

    /// Create a `ParseCache` from a [`SastEngineConfig`].
    ///
    /// The `max_file_bytes` field of the config is used as the file size
    /// limit.
    ///
    /// # Arguments
    ///
    /// * `config` - Reference to the engine configuration.
    ///
    /// # Returns
    ///
    /// A fresh, empty `ParseCache` configured to match `config`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::ast::parse::ParseCache;
    /// use xzardgz::scanner::sast::config::SastEngineConfig;
    ///
    /// let config = SastEngineConfig::new();
    /// let cache = ParseCache::from_config(&config);
    /// assert_eq!(cache.parse_count(), 0);
    /// ```
    #[must_use]
    pub fn from_config(config: &SastEngineConfig) -> Self {
        Self::new(config.max_file_bytes)
    }

    /// Get the cached parse result for `(path, lang)`, parsing if necessary.
    ///
    /// # Behaviour
    ///
    /// 1. If the `(path, lang)` pair is already cached, return the cached
    ///    [`Arc<CachedRoot>`] without incrementing the parse counter.
    /// 2. If `lang` has no AST backing ([`Language::Regex`] or
    ///    [`Language::Generic`]), return `Ok(None)` immediately.
    /// 3. If the file is larger than `max_file_bytes`, return `Ok(None)`
    ///    without caching.
    /// 4. Read the file, parse it with tree-sitter, compute
    ///    [`ErrorNodeDensity`], insert the result into the cache, increment
    ///    the parse counter, and return `Ok(Some(...))`.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the source file to parse.
    /// * `lang` - Language of the file.  Pass an AST-backed language for a
    ///   cache entry to be created.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(Arc<CachedRoot>))` on a successful (or cached) parse.
    /// - `Ok(None)` when the language has no AST backend or the file exceeds
    ///   the size limit.
    /// - `Err(SastError::FileRead { .. })` on any IO error.
    ///
    /// # Errors
    ///
    /// Returns [`SastError::FileRead`] if the file cannot be read from disk.
    pub fn get_or_parse(
        &self,
        path: &Path,
        lang: Language,
    ) -> Result<Option<Arc<CachedRoot>>, SastError> {
        // Early return for non-AST languages; no cache entry is ever created.
        let sg_lang = match lang.to_ast_grep() {
            Some(l) => l,
            None => return Ok(None),
        };

        let key = (path.to_path_buf(), lang);

        // Fast path: return a cached result without IO.
        {
            // SAFETY: A poisoned lock means another thread panicked while
            // holding the guard. Propagating the poison is the correct
            // recovery strategy here.
            let cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(&key) {
                return Ok(Some(Arc::clone(cached)));
            }
        }

        // Check file size before reading the full contents.
        let meta = std::fs::metadata(path).map_err(|e| SastError::FileRead {
            path: path.to_string_lossy().into_owned(),
            cause: e.to_string(),
        })?;
        if meta.len() > self.max_file_bytes {
            return Ok(None);
        }

        // Read the source.
        let src = std::fs::read_to_string(path).map_err(|e| SastError::FileRead {
            path: path.to_string_lossy().into_owned(),
            cause: e.to_string(),
        })?;

        // Parse and compute density.
        let root = sg_lang.ast_grep(&src);
        let density = ErrorNodeDensity::from_root(&root);
        let entry = Arc::new(CachedRoot { root, density });

        // Insert under lock.  A racing thread may have beaten us; prefer the
        // entry already in the map and do not double-count parse_count.
        // SAFETY: Same poison rationale as above.
        let mut cache = self.cache.lock().unwrap();
        let cached = cache.entry(key).or_insert_with(|| {
            self.parse_count.fetch_add(1, Ordering::Relaxed);
            Arc::clone(&entry)
        });
        Ok(Some(Arc::clone(cached)))
    }

    /// Returns the total number of actual parses performed.
    ///
    /// Cache hits do not increment this counter.  The value is useful in
    /// tests to assert the single-parse invariant.
    ///
    /// # Returns
    ///
    /// Number of parse operations that inserted a new entry into the cache.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::ast::parse::ParseCache;
    ///
    /// let cache = ParseCache::new(u64::MAX);
    /// assert_eq!(cache.parse_count(), 0);
    /// ```
    #[must_use]
    pub fn parse_count(&self) -> usize {
        self.parse_count.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_rust_file(src: &str) -> NamedTempFile {
        let mut file = tempfile::Builder::new()
            .prefix("sast_parse_test_")
            .suffix(".rs")
            .tempfile()
            .expect("tempfile creation must succeed in tests");
        write!(file, "{src}").expect("write must succeed in tests");
        file
    }

    #[test]
    fn test_parse_cache_parses_rust_file_successfully() {
        let file = write_rust_file("fn main() {}");
        let cache = ParseCache::new(u64::MAX);
        let result = cache.get_or_parse(file.path(), Language::Rust);
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        assert!(result.unwrap().is_some(), "expected Some cached root");
    }

    #[test]
    fn test_parse_cache_single_parse_invariant_on_double_call() {
        let file = write_rust_file("fn add(a: i32, b: i32) -> i32 { a + b }");
        let cache = ParseCache::new(u64::MAX);

        let first = cache.get_or_parse(file.path(), Language::Rust);
        assert!(first.is_ok());

        let second = cache.get_or_parse(file.path(), Language::Rust);
        assert!(second.is_ok());

        assert_eq!(
            cache.parse_count(),
            1,
            "double call on same path must result in exactly one parse"
        );
    }

    #[test]
    fn test_parse_cache_rejects_oversized_file() {
        let file = write_rust_file("fn main() {}");
        // Set the limit to 1 byte so the file (which is at least 12 bytes) is rejected.
        let cache = ParseCache::new(1);
        let result = cache.get_or_parse(file.path(), Language::Rust);
        assert!(result.is_ok(), "oversized check must not return Err");
        assert!(
            result.unwrap().is_none(),
            "oversized file must return Ok(None)"
        );
        assert_eq!(
            cache.parse_count(),
            0,
            "oversized file must not increment parse_count"
        );
    }

    #[test]
    fn test_parse_cache_returns_none_for_generic_language() {
        let cache = ParseCache::new(u64::MAX);
        // Generic has no AST backend; we return early without any IO.
        let result = cache.get_or_parse(Path::new("/nonexistent/file"), Language::Generic);
        assert!(result.is_ok(), "Generic must not return Err");
        assert!(
            result.unwrap().is_none(),
            "Generic language must return Ok(None)"
        );
    }

    #[test]
    fn test_parse_cache_returns_none_for_regex_language() {
        let cache = ParseCache::new(u64::MAX);
        let result = cache.get_or_parse(Path::new("/nonexistent/file"), Language::Regex);
        assert!(result.is_ok(), "Regex must not return Err");
        assert!(
            result.unwrap().is_none(),
            "Regex language must return Ok(None)"
        );
    }

    #[test]
    fn test_parse_cache_io_error_on_nonexistent_path_returns_sast_error() {
        let cache = ParseCache::new(u64::MAX);
        let result = cache.get_or_parse(
            Path::new("/nonexistent/definitely/missing/file.rs"),
            Language::Rust,
        );
        assert!(result.is_err(), "nonexistent file must return Err");
        assert!(
            matches!(result.unwrap_err(), SastError::FileRead { .. }),
            "IO error must map to SastError::FileRead"
        );
    }

    #[test]
    fn test_parse_cache_density_is_zero_for_clean_source() {
        let file = write_rust_file("fn main() {}");
        let cache = ParseCache::new(u64::MAX);
        let result = cache
            .get_or_parse(file.path(), Language::Rust)
            .expect("parse must succeed")
            .expect("result must be Some");
        assert_eq!(
            result.density.error_nodes, 0,
            "clean source must have zero error nodes"
        );
    }

    #[test]
    fn test_parse_cache_two_different_paths_have_independent_entries() {
        let file_a = write_rust_file("fn foo() {}");
        let file_b = write_rust_file("fn bar() {}");
        let cache = ParseCache::new(u64::MAX);

        cache
            .get_or_parse(file_a.path(), Language::Rust)
            .expect("file_a parse must succeed");
        cache
            .get_or_parse(file_b.path(), Language::Rust)
            .expect("file_b parse must succeed");

        assert_eq!(
            cache.parse_count(),
            2,
            "two distinct paths must produce two cache entries"
        );
    }
}
