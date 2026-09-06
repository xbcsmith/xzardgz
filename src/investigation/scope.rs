//! Investigation scope types.
//!
//! Provides [`FileMatchEntry`] and [`InvestigationScope`], which represent
//! the set of files selected for investigation by a plugin. The scope is built
//! during scanner preselection and consumed by the investigation strategy layer.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::scanner::result::ScanResult;

// ---------------------------------------------------------------------------
// FileMatchEntry
// ---------------------------------------------------------------------------

/// A single file entry in an investigation scope.
///
/// Represents a candidate file that a plugin should analyze.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::scope::FileMatchEntry;
///
/// let entry = FileMatchEntry::new("src/main.rs")
///     .with_language("Rust")
///     .with_size(4096)
///     .with_categories(vec!["entrypoint".to_string()]);
///
/// assert_eq!(entry.path, "src/main.rs");
/// assert_eq!(entry.language.as_deref(), Some("Rust"));
/// assert_eq!(entry.size_bytes, 4096);
/// assert!(entry.categories.contains(&"entrypoint".to_string()));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMatchEntry {
    /// Repository-relative file path using forward slashes.
    pub path: String,
    /// Detected programming language, if known.
    pub language: Option<String>,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Optional category tags derived from scanner preselection
    /// (e.g. `"entrypoint"`, `"security_relevant"`).
    pub categories: Vec<String>,
}

impl FileMatchEntry {
    /// Creates a new [`FileMatchEntry`] with the given path.
    ///
    /// All optional fields default to empty or zero values.
    ///
    /// # Arguments
    ///
    /// * `path` - Repository-relative file path using forward slashes.
    ///
    /// # Returns
    ///
    /// A new [`FileMatchEntry`] with `language = None`, `size_bytes = 0`,
    /// and `categories = []`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::FileMatchEntry;
    ///
    /// let entry = FileMatchEntry::new("src/main.rs");
    /// assert_eq!(entry.path, "src/main.rs");
    /// assert!(entry.language.is_none());
    /// assert_eq!(entry.size_bytes, 0);
    /// assert!(entry.categories.is_empty());
    /// ```
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            language: None,
            size_bytes: 0,
            categories: Vec::new(),
        }
    }

    /// Sets the detected programming language (builder method).
    ///
    /// # Arguments
    ///
    /// * `language` - Language name, e.g. `"Rust"` or `"Python"`.
    ///
    /// # Returns
    ///
    /// The updated [`FileMatchEntry`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::FileMatchEntry;
    ///
    /// let entry = FileMatchEntry::new("src/lib.rs").with_language("Rust");
    /// assert_eq!(entry.language.as_deref(), Some("Rust"));
    /// ```
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    /// Sets the file size in bytes (builder method).
    ///
    /// # Arguments
    ///
    /// * `size_bytes` - File size in bytes.
    ///
    /// # Returns
    ///
    /// The updated [`FileMatchEntry`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::FileMatchEntry;
    ///
    /// let entry = FileMatchEntry::new("src/lib.rs").with_size(2048);
    /// assert_eq!(entry.size_bytes, 2048);
    /// ```
    pub fn with_size(mut self, size_bytes: u64) -> Self {
        self.size_bytes = size_bytes;
        self
    }

    /// Sets the category tags for this entry (builder method).
    ///
    /// # Arguments
    ///
    /// * `categories` - List of category tag strings, e.g. `["entrypoint", "security_relevant"]`.
    ///
    /// # Returns
    ///
    /// The updated [`FileMatchEntry`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::FileMatchEntry;
    ///
    /// let entry = FileMatchEntry::new("src/main.rs")
    ///     .with_categories(vec!["entrypoint".to_string(), "security_relevant".to_string()]);
    /// assert_eq!(entry.categories.len(), 2);
    /// ```
    pub fn with_categories(mut self, categories: Vec<String>) -> Self {
        self.categories = categories;
        self
    }
}

// ---------------------------------------------------------------------------
// InvestigationScope
// ---------------------------------------------------------------------------

/// The set of files selected for investigation by a plugin.
///
/// Maps repository-relative file paths to their [`FileMatchEntry`] records.
/// Provides utilities for filtering, sizing, and splitting.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
///
/// let mut scope = InvestigationScope::new();
/// scope.insert(FileMatchEntry::new("src/main.rs").with_language("Rust").with_size(512));
/// scope.insert(FileMatchEntry::new("src/lib.rs").with_language("Rust").with_size(1024));
///
/// assert_eq!(scope.len(), 2);
/// assert_eq!(scope.total_bytes(), 1536);
/// assert_eq!(scope.paths(), vec!["src/lib.rs", "src/main.rs"]);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InvestigationScope {
    /// Map of path to [`FileMatchEntry`] for fast lookup and deduplication.
    pub files: HashMap<String, FileMatchEntry>,
}

impl InvestigationScope {
    /// Creates a new empty [`InvestigationScope`].
    ///
    /// # Returns
    ///
    /// An empty scope with no file entries.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::InvestigationScope;
    ///
    /// let scope = InvestigationScope::new();
    /// assert!(scope.is_empty());
    /// ```
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    /// Inserts a [`FileMatchEntry`] into the scope, keyed by its path.
    ///
    /// If an entry with the same path already exists, it is replaced.
    ///
    /// # Arguments
    ///
    /// * `entry` - The file entry to insert.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
    ///
    /// let mut scope = InvestigationScope::new();
    /// scope.insert(FileMatchEntry::new("src/main.rs"));
    /// assert_eq!(scope.len(), 1);
    /// ```
    pub fn insert(&mut self, entry: FileMatchEntry) {
        self.files.insert(entry.path.clone(), entry);
    }

    /// Removes a [`FileMatchEntry`] by path, returning it if it was present.
    ///
    /// # Arguments
    ///
    /// * `path` - Repository-relative file path to remove.
    ///
    /// # Returns
    ///
    /// The removed entry, or `None` if the path was not in the scope.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
    ///
    /// let mut scope = InvestigationScope::new();
    /// scope.insert(FileMatchEntry::new("src/main.rs"));
    /// let removed = scope.remove("src/main.rs");
    /// assert!(removed.is_some());
    /// assert!(scope.is_empty());
    /// ```
    pub fn remove(&mut self, path: &str) -> Option<FileMatchEntry> {
        self.files.remove(path)
    }

    /// Returns a reference to the [`FileMatchEntry`] for the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - Repository-relative file path to look up.
    ///
    /// # Returns
    ///
    /// `Some(&entry)` if the path is present, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
    ///
    /// let mut scope = InvestigationScope::new();
    /// scope.insert(FileMatchEntry::new("src/lib.rs").with_size(512));
    /// let entry = scope.get("src/lib.rs");
    /// assert!(entry.is_some());
    /// assert_eq!(entry.unwrap().size_bytes, 512);
    /// ```
    pub fn get(&self, path: &str) -> Option<&FileMatchEntry> {
        self.files.get(path)
    }

    /// Returns the number of file entries in the scope.
    ///
    /// # Returns
    ///
    /// The count of entries currently in the scope.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
    ///
    /// let mut scope = InvestigationScope::new();
    /// assert_eq!(scope.len(), 0);
    /// scope.insert(FileMatchEntry::new("a.rs"));
    /// assert_eq!(scope.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Returns `true` if the scope contains no file entries.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::InvestigationScope;
    ///
    /// let scope = InvestigationScope::new();
    /// assert!(scope.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Returns a sorted list of all file paths in the scope.
    ///
    /// Paths are sorted lexicographically in ascending order to ensure
    /// deterministic output.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` of all paths, sorted alphabetically.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
    ///
    /// let mut scope = InvestigationScope::new();
    /// scope.insert(FileMatchEntry::new("src/z.rs"));
    /// scope.insert(FileMatchEntry::new("src/a.rs"));
    /// let paths = scope.paths();
    /// assert_eq!(paths, vec!["src/a.rs", "src/z.rs"]);
    /// ```
    pub fn paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.files.keys().cloned().collect();
        paths.sort();
        paths
    }

    /// Returns all entries in the scope, sorted by path lexicographically.
    ///
    /// # Returns
    ///
    /// A `Vec<&FileMatchEntry>` sorted by `path` in ascending order.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
    ///
    /// let mut scope = InvestigationScope::new();
    /// scope.insert(FileMatchEntry::new("z.rs"));
    /// scope.insert(FileMatchEntry::new("a.rs"));
    /// let entries = scope.entries();
    /// assert_eq!(entries[0].path, "a.rs");
    /// assert_eq!(entries[1].path, "z.rs");
    /// ```
    pub fn entries(&self) -> Vec<&FileMatchEntry> {
        let mut entries: Vec<&FileMatchEntry> = self.files.values().collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        entries
    }

    /// Returns a new scope containing only entries that have the given category tag.
    ///
    /// Category matching is exact (case-sensitive).
    ///
    /// # Arguments
    ///
    /// * `category` - The category tag string to filter by.
    ///
    /// # Returns
    ///
    /// A new [`InvestigationScope`] containing only entries whose `categories`
    /// list includes `category`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
    ///
    /// let mut scope = InvestigationScope::new();
    /// scope.insert(
    ///     FileMatchEntry::new("main.rs").with_categories(vec!["entrypoint".to_string()]),
    /// );
    /// scope.insert(
    ///     FileMatchEntry::new("lib.rs").with_categories(vec!["security_relevant".to_string()]),
    /// );
    ///
    /// let filtered = scope.filter_by_category("entrypoint");
    /// assert_eq!(filtered.len(), 1);
    /// assert!(filtered.get("main.rs").is_some());
    /// ```
    pub fn filter_by_category(&self, category: &str) -> InvestigationScope {
        let mut result = InvestigationScope::new();
        for entry in self.files.values() {
            if entry.categories.iter().any(|c| c == category) {
                result.insert(entry.clone());
            }
        }
        result
    }

    /// Returns a new scope containing only entries matching the given language.
    ///
    /// Language comparison is case-insensitive. Entries with no language set
    /// are always excluded.
    ///
    /// # Arguments
    ///
    /// * `language` - The language name to filter by (case-insensitive).
    ///
    /// # Returns
    ///
    /// A new [`InvestigationScope`] with only language-matching entries.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
    ///
    /// let mut scope = InvestigationScope::new();
    /// scope.insert(FileMatchEntry::new("src/lib.rs").with_language("Rust"));
    /// scope.insert(FileMatchEntry::new("src/main.py").with_language("Python"));
    ///
    /// let filtered = scope.filter_by_language("rust");
    /// assert_eq!(filtered.len(), 1);
    /// assert!(filtered.get("src/lib.rs").is_some());
    /// ```
    pub fn filter_by_language(&self, language: &str) -> InvestigationScope {
        let target = language.to_lowercase();
        let mut result = InvestigationScope::new();
        for entry in self.files.values() {
            if let Some(lang) = &entry.language
                && lang.to_lowercase() == target
            {
                result.insert(entry.clone());
            }
        }
        result
    }

    /// Returns the sum of `size_bytes` across all entries in the scope.
    ///
    /// # Returns
    ///
    /// Total byte count, or `0` if the scope is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
    ///
    /// let mut scope = InvestigationScope::new();
    /// scope.insert(FileMatchEntry::new("a.rs").with_size(100));
    /// scope.insert(FileMatchEntry::new("b.rs").with_size(200));
    /// assert_eq!(scope.total_bytes(), 300);
    /// ```
    pub fn total_bytes(&self) -> u64 {
        self.files.values().map(|e| e.size_bytes).sum()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // FileMatchEntry
    // ------------------------------------------------------------------

    #[test]
    fn test_file_match_entry_new_sets_path() {
        let entry = FileMatchEntry::new("src/main.rs");
        assert_eq!(entry.path, "src/main.rs");
        assert!(entry.language.is_none());
        assert_eq!(entry.size_bytes, 0);
        assert!(entry.categories.is_empty());
    }

    #[test]
    fn test_file_match_entry_builder_methods_set_all_fields() {
        let entry = FileMatchEntry::new("src/lib.rs")
            .with_language("Rust")
            .with_size(4096)
            .with_categories(vec![
                "entrypoint".to_string(),
                "security_relevant".to_string(),
            ]);

        assert_eq!(entry.path, "src/lib.rs");
        assert_eq!(entry.language.as_deref(), Some("Rust"));
        assert_eq!(entry.size_bytes, 4096);
        assert_eq!(entry.categories.len(), 2);
        assert!(entry.categories.contains(&"entrypoint".to_string()));
        assert!(entry.categories.contains(&"security_relevant".to_string()));
    }

    #[test]
    fn test_file_match_entry_with_language_sets_language() {
        let entry = FileMatchEntry::new("main.py").with_language("Python");
        assert_eq!(entry.language.as_deref(), Some("Python"));
    }

    #[test]
    fn test_file_match_entry_with_size_sets_size_bytes() {
        let entry = FileMatchEntry::new("large.rs").with_size(99_999);
        assert_eq!(entry.size_bytes, 99_999);
    }

    #[test]
    fn test_file_match_entry_with_categories_replaces_empty_vec() {
        let entry = FileMatchEntry::new("a.rs").with_categories(vec!["tag".to_string()]);
        assert_eq!(entry.categories, vec!["tag".to_string()]);
    }

    #[test]
    fn test_file_match_entry_with_size_zero_stays_zero() {
        let entry = FileMatchEntry::new("empty.rs").with_size(0);
        assert_eq!(entry.size_bytes, 0);
    }

    // ------------------------------------------------------------------
    // InvestigationScope::new / default
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_scope_new_is_empty() {
        let scope = InvestigationScope::new();
        assert!(scope.is_empty());
        assert_eq!(scope.len(), 0);
    }

    #[test]
    fn test_investigation_scope_default_is_empty() {
        let scope = InvestigationScope::default();
        assert!(scope.is_empty());
    }

    // ------------------------------------------------------------------
    // insert
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_scope_insert_adds_entry() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("src/main.rs"));
        assert_eq!(scope.len(), 1);
    }

    #[test]
    fn test_investigation_scope_insert_replaces_existing_entry() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("src/main.rs").with_size(100));
        scope.insert(FileMatchEntry::new("src/main.rs").with_size(200));
        assert_eq!(scope.len(), 1);
        // SAFETY: we just inserted, so the entry is present
        assert_eq!(scope.get("src/main.rs").unwrap().size_bytes, 200);
    }

    #[test]
    fn test_investigation_scope_insert_multiple_distinct_paths() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("a.rs"));
        scope.insert(FileMatchEntry::new("b.rs"));
        scope.insert(FileMatchEntry::new("c.rs"));
        assert_eq!(scope.len(), 3);
    }

    // ------------------------------------------------------------------
    // remove
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_scope_remove_returns_entry_when_present() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("src/main.rs").with_size(42));
        let removed = scope.remove("src/main.rs");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().size_bytes, 42);
        assert!(scope.is_empty());
    }

    #[test]
    fn test_investigation_scope_remove_returns_none_for_missing_path() {
        let mut scope = InvestigationScope::new();
        let removed = scope.remove("nonexistent.rs");
        assert!(removed.is_none());
    }

    #[test]
    fn test_investigation_scope_remove_decrements_len() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("a.rs"));
        scope.insert(FileMatchEntry::new("b.rs"));
        scope.remove("a.rs");
        assert_eq!(scope.len(), 1);
    }

    // ------------------------------------------------------------------
    // get
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_scope_get_returns_reference_when_present() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("src/lib.rs").with_size(512));
        let entry = scope.get("src/lib.rs");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().size_bytes, 512);
    }

    #[test]
    fn test_investigation_scope_get_returns_none_for_missing_path() {
        let scope = InvestigationScope::new();
        assert!(scope.get("missing.rs").is_none());
    }

    // ------------------------------------------------------------------
    // len / is_empty
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_scope_len_counts_entries_correctly() {
        let mut scope = InvestigationScope::new();
        assert_eq!(scope.len(), 0);
        scope.insert(FileMatchEntry::new("a.rs"));
        assert_eq!(scope.len(), 1);
        scope.insert(FileMatchEntry::new("b.rs"));
        assert_eq!(scope.len(), 2);
    }

    #[test]
    fn test_investigation_scope_is_empty_returns_true_when_empty() {
        let scope = InvestigationScope::new();
        assert!(scope.is_empty());
    }

    #[test]
    fn test_investigation_scope_is_empty_returns_false_when_nonempty() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("a.rs"));
        assert!(!scope.is_empty());
    }

    // ------------------------------------------------------------------
    // paths
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_scope_paths_returns_sorted_list() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("src/z.rs"));
        scope.insert(FileMatchEntry::new("src/a.rs"));
        scope.insert(FileMatchEntry::new("src/m.rs"));
        let paths = scope.paths();
        assert_eq!(paths, vec!["src/a.rs", "src/m.rs", "src/z.rs"]);
    }

    #[test]
    fn test_investigation_scope_paths_returns_empty_vec_for_empty_scope() {
        let scope = InvestigationScope::new();
        assert!(scope.paths().is_empty());
    }

    // ------------------------------------------------------------------
    // entries
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_scope_entries_returns_sorted_by_path() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("z.rs").with_size(3));
        scope.insert(FileMatchEntry::new("a.rs").with_size(1));
        scope.insert(FileMatchEntry::new("m.rs").with_size(2));
        let entries = scope.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].path, "a.rs");
        assert_eq!(entries[1].path, "m.rs");
        assert_eq!(entries[2].path, "z.rs");
    }

    #[test]
    fn test_investigation_scope_entries_returns_empty_for_empty_scope() {
        let scope = InvestigationScope::new();
        assert!(scope.entries().is_empty());
    }

    // ------------------------------------------------------------------
    // filter_by_category
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_scope_filter_by_category_returns_matching_entries() {
        let mut scope = InvestigationScope::new();
        scope
            .insert(FileMatchEntry::new("main.rs").with_categories(vec!["entrypoint".to_string()]));
        scope.insert(
            FileMatchEntry::new("lib.rs").with_categories(vec!["security_relevant".to_string()]),
        );
        scope.insert(FileMatchEntry::new("auth.rs").with_categories(vec![
            "security_relevant".to_string(),
            "entrypoint".to_string(),
        ]));

        let filtered = scope.filter_by_category("security_relevant");
        assert_eq!(filtered.len(), 2);
        assert!(filtered.get("lib.rs").is_some());
        assert!(filtered.get("auth.rs").is_some());
        assert!(filtered.get("main.rs").is_none());
    }

    #[test]
    fn test_investigation_scope_filter_by_category_returns_empty_when_none_match() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("a.rs").with_categories(vec!["docs".to_string()]));
        let filtered = scope.filter_by_category("entrypoint");
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_investigation_scope_filter_by_category_returns_empty_for_empty_scope() {
        let scope = InvestigationScope::new();
        let filtered = scope.filter_by_category("entrypoint");
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_investigation_scope_filter_by_category_does_not_mutate_original() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("a.rs").with_categories(vec!["x".to_string()]));
        let _filtered = scope.filter_by_category("x");
        assert_eq!(scope.len(), 1);
    }

    // ------------------------------------------------------------------
    // filter_by_language
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_scope_filter_by_language_returns_matching_entries() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("src/lib.rs").with_language("Rust"));
        scope.insert(FileMatchEntry::new("src/main.py").with_language("Python"));
        scope.insert(FileMatchEntry::new("src/util.rs").with_language("Rust"));

        let filtered = scope.filter_by_language("Rust");
        assert_eq!(filtered.len(), 2);
        assert!(filtered.get("src/lib.rs").is_some());
        assert!(filtered.get("src/util.rs").is_some());
        assert!(filtered.get("src/main.py").is_none());
    }

    #[test]
    fn test_investigation_scope_filter_by_language_is_case_insensitive() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("main.rs").with_language("Rust"));
        scope.insert(FileMatchEntry::new("lib.rs").with_language("RUST"));

        let filtered = scope.filter_by_language("rust");
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_investigation_scope_filter_by_language_excludes_entries_without_language() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("a.rs").with_language("Rust"));
        scope.insert(FileMatchEntry::new("b.rs")); // no language

        let filtered = scope.filter_by_language("rust");
        assert_eq!(filtered.len(), 1);
        assert!(filtered.get("a.rs").is_some());
    }

    #[test]
    fn test_investigation_scope_filter_by_language_returns_empty_when_none_match() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("main.py").with_language("Python"));

        let filtered = scope.filter_by_language("Go");
        assert!(filtered.is_empty());
    }

    // ------------------------------------------------------------------
    // total_bytes
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_scope_total_bytes_sums_all_entries() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("a.rs").with_size(100));
        scope.insert(FileMatchEntry::new("b.rs").with_size(200));
        scope.insert(FileMatchEntry::new("c.rs").with_size(300));
        assert_eq!(scope.total_bytes(), 600);
    }

    #[test]
    fn test_investigation_scope_total_bytes_returns_zero_for_empty_scope() {
        let scope = InvestigationScope::new();
        assert_eq!(scope.total_bytes(), 0);
    }

    #[test]
    fn test_investigation_scope_total_bytes_handles_single_entry() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("big.rs").with_size(1_000_000));
        assert_eq!(scope.total_bytes(), 1_000_000);
    }
}

// ---------------------------------------------------------------------------
// ScopeMetrics
// ---------------------------------------------------------------------------

/// Repository-level scope metrics used to compute turn budgets and select
/// investigation strategies.
///
/// `ScopeMetrics` captures three numbers derived from a scan result:
/// - `total_files` — total file count across the whole repository.
/// - `total_size_bytes` — sum of all file sizes in the repository.
/// - `matched_file_count` — count of unique files that matched at least one
///   concern category in [`PluginPreselection`] (risky patterns, secrets-like,
///   unsafe Rust, command execution, network clients, authentication files).
///
/// Use [`ScopeMetrics::from_scan_result`] to build from a [`ScanResult`], or
/// [`ScopeMetrics::from_investigation_scope`] to build from an
/// [`InvestigationScope`] (where `total_files == matched_file_count`).
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::scope::ScopeMetrics;
///
/// let m = ScopeMetrics::new(500, 1_048_576, 25);
/// assert_eq!(m.total_files, 500);
/// assert_eq!(m.total_size_bytes, 1_048_576);
/// assert_eq!(m.matched_file_count, 25);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeMetrics {
    /// Total number of files in the repository.
    pub total_files: u64,
    /// Total size of all repository files in bytes.
    pub total_size_bytes: u64,
    /// Number of unique files matched by concern-category preselection.
    pub matched_file_count: u64,
}

impl ScopeMetrics {
    /// Creates a [`ScopeMetrics`] with the three scalar values.
    ///
    /// # Arguments
    ///
    /// * `total_files` - Total file count in the repository.
    /// * `total_size_bytes` - Total byte count for all repository files.
    /// * `matched_file_count` - Number of unique concern-matched files.
    ///
    /// # Returns
    ///
    /// A new [`ScopeMetrics`] with the given values.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::ScopeMetrics;
    ///
    /// let m = ScopeMetrics::new(1000, 2_097_152, 50);
    /// assert_eq!(m.total_files, 1000);
    /// assert_eq!(m.total_size_bytes, 2_097_152);
    /// assert_eq!(m.matched_file_count, 50);
    /// ```
    pub fn new(total_files: u64, total_size_bytes: u64, matched_file_count: u64) -> Self {
        Self {
            total_files,
            total_size_bytes,
            matched_file_count,
        }
    }

    /// Builds [`ScopeMetrics`] from a [`ScanResult`].
    ///
    /// - `total_files` = `scan_result.repository_structure.len()` cast to `u64`.
    /// - `total_size_bytes` = sum of `size_bytes` across all entries in
    ///   `scan_result.repository_structure`.
    /// - `matched_file_count` = count of unique file paths appearing in any of
    ///   the concern lists from `scan_result.plugin_preselection`:
    ///   `risky_pattern_files`, `secrets_like_files`, `unsafe_rust_files`,
    ///   `command_execution_files`, `network_client_files`, and `auth_files`.
    ///   Files appearing in multiple lists are counted once.
    ///
    /// # Arguments
    ///
    /// * `scan_result` - The scan result to derive metrics from.
    ///
    /// # Returns
    ///
    /// A [`ScopeMetrics`] populated from the scan result.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use std::collections::HashMap;
    /// use xzardgz::scanner::result::{PluginPreselection, ScanResult, SCAN_RESULT_VERSION};
    /// use xzardgz::investigation::scope::ScopeMetrics;
    ///
    /// let scan = ScanResult {
    ///     version: SCAN_RESULT_VERSION.to_string(),
    ///     repository_url: None,
    ///     repository_name: None,
    ///     head_commit: None,
    ///     scan_timestamp: Utc::now(),
    ///     repository_structure: vec![],
    ///     language_statistics: HashMap::new(),
    ///     primary_language: None,
    ///     frameworks: vec![],
    ///     documentation_inventory: vec![],
    ///     governance_rules: vec![],
    ///     cli_commands: vec![],
    ///     public_apis: vec![],
    ///     entrypoints: vec![],
    ///     config_surface: vec![],
    ///     key_files: vec![],
    ///     dependency_manifests: vec![],
    ///     test_files: vec![],
    ///     build_files: vec![],
    ///     security_relevant_files: vec![],
    ///     findings: vec![],
    ///     plugin_preselection: PluginPreselection::default(),
    /// };
    /// let m = ScopeMetrics::from_scan_result(&scan);
    /// assert_eq!(m.total_files, 0);
    /// assert_eq!(m.total_size_bytes, 0);
    /// assert_eq!(m.matched_file_count, 0);
    /// ```
    pub fn from_scan_result(scan_result: &ScanResult) -> Self {
        let total_files = scan_result.repository_structure.len() as u64;
        let total_size_bytes = scan_result
            .repository_structure
            .iter()
            .map(|f| f.size_bytes)
            .sum();
        let ps = &scan_result.plugin_preselection;
        let mut seen = std::collections::HashSet::new();
        for path in ps
            .risky_pattern_files
            .iter()
            .chain(ps.secrets_like_files.iter())
            .chain(ps.unsafe_rust_files.iter())
            .chain(ps.command_execution_files.iter())
            .chain(ps.network_client_files.iter())
            .chain(ps.auth_files.iter())
        {
            seen.insert(path.as_str());
        }
        let matched_file_count = seen.len() as u64;
        Self {
            total_files,
            total_size_bytes,
            matched_file_count,
        }
    }

    /// Builds [`ScopeMetrics`] from an [`InvestigationScope`].
    ///
    /// Since an [`InvestigationScope`] does not carry repository-wide totals,
    /// this constructor treats the scope as the full universe:
    /// `total_files` and `matched_file_count` both equal `scope.len()`, and
    /// `total_size_bytes` equals `scope.total_bytes()`.
    ///
    /// This is the right constructor when a [`ScanResult`] is not available, e.g.
    /// in tests or when building a scope from a filtered subset of files.
    ///
    /// # Arguments
    ///
    /// * `scope` - The investigation scope to derive metrics from.
    ///
    /// # Returns
    ///
    /// A [`ScopeMetrics`] where all totals reflect the given scope only.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope, ScopeMetrics};
    ///
    /// let mut scope = InvestigationScope::new();
    /// scope.insert(FileMatchEntry::new("a.rs").with_size(100));
    /// scope.insert(FileMatchEntry::new("b.rs").with_size(200));
    ///
    /// let m = ScopeMetrics::from_investigation_scope(&scope);
    /// assert_eq!(m.total_files, 2);
    /// assert_eq!(m.total_size_bytes, 300);
    /// assert_eq!(m.matched_file_count, 2);
    /// ```
    pub fn from_investigation_scope(scope: &InvestigationScope) -> Self {
        Self {
            total_files: scope.len() as u64,
            total_size_bytes: scope.total_bytes(),
            matched_file_count: scope.len() as u64,
        }
    }
}

#[cfg(test)]
mod scope_metrics_tests {
    use super::*;
    use crate::scanner::result::{FileEntry, PluginPreselection, ScanResult};
    use chrono::Utc;

    fn minimal_scan_result() -> ScanResult {
        ScanResult {
            version: "1".to_string(),
            repository_url: None,
            repository_name: None,
            head_commit: None,
            scan_timestamp: Utc::now(),
            repository_structure: vec![],
            language_statistics: std::collections::HashMap::new(),
            primary_language: None,
            frameworks: vec![],
            documentation_inventory: vec![],
            governance_rules: vec![],
            cli_commands: vec![],
            public_apis: vec![],
            entrypoints: vec![],
            config_surface: vec![],
            key_files: vec![],
            dependency_manifests: vec![],
            test_files: vec![],
            build_files: vec![],
            security_relevant_files: vec![],
            findings: vec![],
            plugin_preselection: PluginPreselection::default(),
        }
    }

    #[test]
    fn test_scope_metrics_new_sets_all_fields() {
        let m = ScopeMetrics::new(500, 1_048_576, 25);
        assert_eq!(m.total_files, 500);
        assert_eq!(m.total_size_bytes, 1_048_576);
        assert_eq!(m.matched_file_count, 25);
    }

    #[test]
    fn test_scope_metrics_new_with_zero_values_produces_zero_metrics() {
        let m = ScopeMetrics::new(0, 0, 0);
        assert_eq!(m.total_files, 0);
        assert_eq!(m.total_size_bytes, 0);
        assert_eq!(m.matched_file_count, 0);
    }

    #[test]
    fn test_scope_metrics_from_investigation_scope_empty_scope_returns_all_zeros() {
        let scope = InvestigationScope::new();
        let m = ScopeMetrics::from_investigation_scope(&scope);
        assert_eq!(m.total_files, 0);
        assert_eq!(m.total_size_bytes, 0);
        assert_eq!(m.matched_file_count, 0);
    }

    #[test]
    fn test_scope_metrics_from_investigation_scope_with_entries_counts_correctly() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("a.rs").with_size(100));
        scope.insert(FileMatchEntry::new("b.rs").with_size(200));
        let m = ScopeMetrics::from_investigation_scope(&scope);
        assert_eq!(m.total_files, 2);
        assert_eq!(m.total_size_bytes, 300);
        assert_eq!(m.matched_file_count, 2);
    }

    #[test]
    fn test_scope_metrics_from_investigation_scope_total_files_equals_matched_file_count() {
        let mut scope = InvestigationScope::new();
        scope.insert(FileMatchEntry::new("x.rs").with_size(50));
        scope.insert(FileMatchEntry::new("y.rs").with_size(75));
        scope.insert(FileMatchEntry::new("z.rs").with_size(25));
        let m = ScopeMetrics::from_investigation_scope(&scope);
        assert_eq!(m.total_files, m.matched_file_count);
    }

    #[test]
    fn test_scope_metrics_from_scan_result_empty_scan_result_returns_all_zeros() {
        let scan = minimal_scan_result();
        let m = ScopeMetrics::from_scan_result(&scan);
        assert_eq!(m.total_files, 0);
        assert_eq!(m.total_size_bytes, 0);
        assert_eq!(m.matched_file_count, 0);
    }

    #[test]
    fn test_scope_metrics_from_scan_result_counts_repository_structure() {
        let mut scan = minimal_scan_result();
        scan.repository_structure = vec![
            FileEntry {
                path: "src/main.rs".to_string(),
                size_bytes: 512,
                language: Some("Rust".to_string()),
                is_binary: false,
            },
            FileEntry {
                path: "src/lib.rs".to_string(),
                size_bytes: 1024,
                language: Some("Rust".to_string()),
                is_binary: false,
            },
            FileEntry {
                path: "README.md".to_string(),
                size_bytes: 256,
                language: None,
                is_binary: false,
            },
        ];
        let m = ScopeMetrics::from_scan_result(&scan);
        assert_eq!(m.total_files, 3);
    }

    #[test]
    fn test_scope_metrics_from_scan_result_sums_file_sizes() {
        let mut scan = minimal_scan_result();
        scan.repository_structure = vec![
            FileEntry {
                path: "a.rs".to_string(),
                size_bytes: 100,
                language: None,
                is_binary: false,
            },
            FileEntry {
                path: "b.rs".to_string(),
                size_bytes: 200,
                language: None,
                is_binary: false,
            },
        ];
        let m = ScopeMetrics::from_scan_result(&scan);
        assert_eq!(m.total_size_bytes, 300);
    }

    #[test]
    fn test_scope_metrics_from_scan_result_counts_unique_matched_files() {
        let mut scan = minimal_scan_result();
        scan.plugin_preselection.risky_pattern_files = vec!["src/risky.rs".to_string()];
        scan.plugin_preselection.secrets_like_files = vec!["config/secrets.toml".to_string()];
        scan.plugin_preselection.auth_files = vec!["src/auth.rs".to_string()];
        let m = ScopeMetrics::from_scan_result(&scan);
        assert_eq!(m.matched_file_count, 3);
    }

    #[test]
    fn test_scope_metrics_from_scan_result_deduplicates_files_in_multiple_categories() {
        let mut scan = minimal_scan_result();
        // The same file appears in two concern lists; it must be counted once.
        scan.plugin_preselection.risky_pattern_files = vec!["src/danger.rs".to_string()];
        scan.plugin_preselection.unsafe_rust_files = vec!["src/danger.rs".to_string()];
        let m = ScopeMetrics::from_scan_result(&scan);
        assert_eq!(m.matched_file_count, 1);
    }

    #[test]
    fn test_scope_metrics_from_scan_result_non_concern_preselection_fields_not_counted() {
        let mut scan = minimal_scan_result();
        // Populate only non-concern preselection fields; matched_file_count must stay 0.
        scan.plugin_preselection.entrypoints = vec!["src/main.rs".to_string()];
        scan.plugin_preselection.public_apis = vec!["src/lib.rs".to_string()];
        scan.plugin_preselection.test_files = vec!["tests/integration_test.rs".to_string()];
        scan.plugin_preselection.missing_test_signals = vec!["src/uncovered.rs".to_string()];
        let m = ScopeMetrics::from_scan_result(&scan);
        assert_eq!(m.matched_file_count, 0);
    }
}
