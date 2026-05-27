//! File selection and ranking for technical review analysis.
//!
//! [`FilePrioritizer`] extracts and ranks files from a [`ScanResult`], giving
//! priority to entrypoints, public API surfaces, configuration, and key files.
//! The result is a [`PrioritizedFiles`] collection that can be flattened into
//! a deduplicated ordered list capped at a configurable maximum.

use std::collections::HashSet;

use crate::scanner::result::ScanResult;

// ---------------------------------------------------------------------------
// PrioritizedFiles
// ---------------------------------------------------------------------------

/// Files organised by semantic category for technical review.
///
/// Each field holds repository-relative paths belonging to a particular
/// category.  Use [`flat_list`][Self::flat_list] to obtain a single
/// deduplicated list in analysis priority order.
///
/// # Examples
///
/// ```
/// use xzardgz::plugins::technical_review::prioritizer::PrioritizedFiles;
///
/// let pf = PrioritizedFiles::new();
/// assert_eq!(pf.total_unique_files(), 0);
/// assert!(pf.flat_list().is_empty());
/// ```
#[derive(Debug, Clone, Default)]
pub struct PrioritizedFiles {
    /// Repository entrypoint files (main.rs, index.js, etc.)
    pub entrypoints: Vec<String>,
    /// Files forming the public API surface.
    pub public_apis: Vec<String>,
    /// Configuration file paths.
    pub config_surfaces: Vec<String>,
    /// Key project files (README, LICENSE, CONTRIBUTING, etc.)
    pub key_files: Vec<String>,
    /// Test file paths.
    pub test_files: Vec<String>,
    /// Build system files (Makefile, build.rs, Dockerfile, CI configs, etc.)
    pub build_files: Vec<String>,
    /// Dependency manifest paths (Cargo.toml, package.json, etc.)
    pub dependency_manifests: Vec<String>,
    /// Documentation files (README, docs/*.md, etc.)
    pub documentation_files: Vec<String>,
    /// Source files with high fan-in signals (frequently imported or central).
    pub high_fan_in_signals: Vec<String>,
}

impl PrioritizedFiles {
    /// Creates a new empty `PrioritizedFiles`.
    ///
    /// All category vectors start empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::prioritizer::PrioritizedFiles;
    ///
    /// let pf = PrioritizedFiles::new();
    /// assert!(pf.entrypoints.is_empty());
    /// assert!(pf.flat_list().is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a deduplicated flat list of all files in analysis priority order.
    ///
    /// Files are emitted in this order: entrypoints, public APIs, config
    /// surfaces, key files, high fan-in signals, build files, dependency
    /// manifests, test files, documentation files.  Each file path appears at
    /// most once (first occurrence wins).
    ///
    /// # Returns
    ///
    /// A `Vec<String>` of deduplicated, ordered file paths.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::prioritizer::PrioritizedFiles;
    ///
    /// let mut pf = PrioritizedFiles::new();
    /// pf.entrypoints = vec!["src/main.rs".to_string()];
    /// pf.public_apis = vec!["src/lib.rs".to_string(), "src/main.rs".to_string()];
    ///
    /// let flat = pf.flat_list();
    /// // src/main.rs appears first from entrypoints; the duplicate in public_apis is dropped.
    /// assert_eq!(flat.len(), 2);
    /// assert_eq!(flat[0], "src/main.rs");
    /// assert_eq!(flat[1], "src/lib.rs");
    /// ```
    pub fn flat_list(&self) -> Vec<String> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut result: Vec<String> = Vec::new();

        let categories: &[&Vec<String>] = &[
            &self.entrypoints,
            &self.public_apis,
            &self.config_surfaces,
            &self.key_files,
            &self.high_fan_in_signals,
            &self.build_files,
            &self.dependency_manifests,
            &self.test_files,
            &self.documentation_files,
        ];

        for category in categories {
            for file in *category {
                if seen.insert(file.clone()) {
                    result.push(file.clone());
                }
            }
        }

        result
    }

    /// Returns the total number of unique files across all categories.
    ///
    /// Equivalent to `self.flat_list().len()`.
    ///
    /// # Returns
    ///
    /// The count of unique file paths.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::prioritizer::PrioritizedFiles;
    ///
    /// let mut pf = PrioritizedFiles::new();
    /// pf.entrypoints = vec!["a.rs".to_string(), "b.rs".to_string()];
    /// pf.public_apis = vec!["b.rs".to_string(), "c.rs".to_string()];
    ///
    /// assert_eq!(pf.total_unique_files(), 3);
    /// ```
    pub fn total_unique_files(&self) -> usize {
        self.flat_list().len()
    }
}

// ---------------------------------------------------------------------------
// FilePrioritizer
// ---------------------------------------------------------------------------

/// Extracts and prioritises files from a [`ScanResult`] for technical review.
///
/// Uses `plugin_preselection` data first (more accurate), falling back to
/// top-level [`ScanResult`] lists when preselection fields are empty.
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use std::collections::HashMap;
/// use xzardgz::plugins::technical_review::prioritizer::FilePrioritizer;
/// use xzardgz::scanner::result::{PluginPreselection, ScanResult, SCAN_RESULT_VERSION};
///
/// let scan = ScanResult {
///     version: SCAN_RESULT_VERSION.to_string(),
///     repository_name: None,
///     repository_url: None,
///     head_commit: None,
///     scan_timestamp: Utc::now(),
///     repository_structure: vec![],
///     language_statistics: HashMap::new(),
///     primary_language: None,
///     frameworks: vec![],
///     documentation_inventory: vec![],
///     governance_rules: vec![],
///     cli_commands: vec![],
///     public_apis: vec!["src/lib.rs".to_string()],
///     entrypoints: vec!["src/main.rs".to_string()],
///     config_surface: vec![],
///     key_files: vec![],
///     dependency_manifests: vec![],
///     test_files: vec![],
///     build_files: vec![],
///     security_relevant_files: vec![],
///     findings: vec![],
///     plugin_preselection: PluginPreselection::default(),
/// };
///
/// let files = FilePrioritizer::capped_flat_list(&scan, 10, true, true);
/// assert!(files.contains(&"src/main.rs".to_string()));
/// ```
pub struct FilePrioritizer;

impl FilePrioritizer {
    /// Builds a [`PrioritizedFiles`] from a [`ScanResult`].
    ///
    /// Uses `plugin_preselection` data first (more accurate), falling back to
    /// top-level scan result fields when the preselection field is empty.
    ///
    /// `include_tests` controls whether test files are populated in the result.
    /// `include_docs` controls whether documentation files are populated.
    ///
    /// # Arguments
    ///
    /// * `scan_result`   - The scan result to extract files from.
    /// * `max_files`     - Maximum total files; passed to `capped_flat_list` if
    ///   you need a flat capped view.  This method does NOT apply the cap.
    /// * `include_tests` - Whether to include test files.
    /// * `include_docs`  - Whether to include documentation files.
    ///
    /// # Returns
    ///
    /// A [`PrioritizedFiles`] populated from the scan result.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use std::collections::HashMap;
    /// use xzardgz::plugins::technical_review::prioritizer::FilePrioritizer;
    /// use xzardgz::scanner::result::{PluginPreselection, ScanResult, SCAN_RESULT_VERSION};
    ///
    /// let mut presel = PluginPreselection::default();
    /// presel.entrypoints = vec!["src/main.rs".to_string()];
    ///
    /// let scan = ScanResult {
    ///     version: SCAN_RESULT_VERSION.to_string(),
    ///     repository_name: None,
    ///     repository_url: None,
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
    ///     entrypoints: vec!["other.rs".to_string()],
    ///     config_surface: vec![],
    ///     key_files: vec![],
    ///     dependency_manifests: vec![],
    ///     test_files: vec![],
    ///     build_files: vec![],
    ///     security_relevant_files: vec![],
    ///     findings: vec![],
    ///     plugin_preselection: presel,
    /// };
    ///
    /// let pf = FilePrioritizer::prioritize(&scan, 50, true, true);
    /// // Preselection takes precedence over top-level entrypoints.
    /// assert!(pf.entrypoints.contains(&"src/main.rs".to_string()));
    /// ```
    pub fn prioritize(
        scan_result: &ScanResult,
        _max_files: u32,
        include_tests: bool,
        include_docs: bool,
    ) -> PrioritizedFiles {
        let presel = &scan_result.plugin_preselection;

        let entrypoints = if !presel.entrypoints.is_empty() {
            presel.entrypoints.clone()
        } else {
            scan_result.entrypoints.clone()
        };

        let public_apis = if !presel.public_apis.is_empty() {
            presel.public_apis.clone()
        } else {
            scan_result.public_apis.clone()
        };

        let config_surfaces = if !presel.config_surfaces.is_empty() {
            presel.config_surfaces.clone()
        } else {
            scan_result.config_surface.clone()
        };

        let dependency_manifests = if !presel.dependency_manifests.is_empty() {
            presel.dependency_manifests.clone()
        } else {
            scan_result.dependency_manifests.clone()
        };

        let test_files = if include_tests {
            if !presel.test_files.is_empty() {
                presel.test_files.clone()
            } else {
                scan_result.test_files.clone()
            }
        } else {
            Vec::new()
        };

        let documentation_files = if include_docs {
            scan_result.documentation_inventory.clone()
        } else {
            Vec::new()
        };

        PrioritizedFiles {
            entrypoints,
            public_apis,
            config_surfaces,
            key_files: scan_result.key_files.clone(),
            test_files,
            build_files: scan_result.build_files.clone(),
            dependency_manifests,
            documentation_files,
            high_fan_in_signals: Vec::new(),
        }
    }

    /// Returns a flat, deduplicated, and capped list of prioritised files.
    ///
    /// Calls [`prioritize`][Self::prioritize] then truncates the result to
    /// `max_files` entries.
    ///
    /// # Arguments
    ///
    /// * `scan_result`   - The scan result to extract files from.
    /// * `max_files`     - Maximum number of file paths to return.
    /// * `include_tests` - Whether to include test files.
    /// * `include_docs`  - Whether to include documentation files.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` of at most `max_files` unique file paths.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use std::collections::HashMap;
    /// use xzardgz::plugins::technical_review::prioritizer::FilePrioritizer;
    /// use xzardgz::scanner::result::{PluginPreselection, ScanResult, SCAN_RESULT_VERSION};
    ///
    /// let scan = ScanResult {
    ///     version: SCAN_RESULT_VERSION.to_string(),
    ///     repository_name: None,
    ///     repository_url: None,
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
    ///     entrypoints: vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()],
    ///     config_surface: vec![],
    ///     key_files: vec![],
    ///     dependency_manifests: vec![],
    ///     test_files: vec![],
    ///     build_files: vec![],
    ///     security_relevant_files: vec![],
    ///     findings: vec![],
    ///     plugin_preselection: PluginPreselection::default(),
    /// };
    ///
    /// let capped = FilePrioritizer::capped_flat_list(&scan, 2, false, false);
    /// assert_eq!(capped.len(), 2);
    /// ```
    pub fn capped_flat_list(
        scan_result: &ScanResult,
        max_files: u32,
        include_tests: bool,
        include_docs: bool,
    ) -> Vec<String> {
        let pf = Self::prioritize(scan_result, max_files, include_tests, include_docs);
        let all = pf.flat_list();
        let cap = max_files as usize;
        if all.len() > cap {
            all.into_iter().take(cap).collect()
        } else {
            all
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::result::{PluginPreselection, SCAN_RESULT_VERSION, ScanResult};
    use chrono::Utc;
    use std::collections::HashMap;

    /// Builds a minimal [`ScanResult`] with all vectors empty.
    fn empty_scan() -> ScanResult {
        ScanResult {
            version: SCAN_RESULT_VERSION.to_string(),
            repository_url: None,
            repository_name: None,
            head_commit: None,
            scan_timestamp: Utc::now(),
            repository_structure: vec![],
            language_statistics: HashMap::new(),
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

    // ------------------------------------------------------------------
    // PrioritizedFiles::new
    // ------------------------------------------------------------------

    #[test]
    fn test_prioritized_files_new_creates_empty_struct() {
        let pf = PrioritizedFiles::new();
        assert!(pf.entrypoints.is_empty());
        assert!(pf.public_apis.is_empty());
        assert!(pf.config_surfaces.is_empty());
        assert!(pf.key_files.is_empty());
        assert!(pf.test_files.is_empty());
        assert!(pf.build_files.is_empty());
        assert!(pf.dependency_manifests.is_empty());
        assert!(pf.documentation_files.is_empty());
        assert!(pf.high_fan_in_signals.is_empty());
    }

    // ------------------------------------------------------------------
    // PrioritizedFiles::flat_list
    // ------------------------------------------------------------------

    #[test]
    fn test_prioritized_files_flat_list_empty_returns_empty() {
        let pf = PrioritizedFiles::new();
        assert!(pf.flat_list().is_empty());
    }

    #[test]
    fn test_prioritized_files_flat_list_deduplicates_across_categories() {
        let mut pf = PrioritizedFiles::new();
        pf.entrypoints = vec!["src/main.rs".to_string()];
        pf.public_apis = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
        let flat = pf.flat_list();
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0], "src/main.rs");
        assert_eq!(flat[1], "src/lib.rs");
    }

    #[test]
    fn test_prioritized_files_flat_list_preserves_priority_order() {
        let mut pf = PrioritizedFiles::new();
        pf.documentation_files = vec!["README.md".to_string()];
        pf.entrypoints = vec!["src/main.rs".to_string()];
        // entrypoints come before documentation_files in priority order.
        let flat = pf.flat_list();
        assert_eq!(flat[0], "src/main.rs");
        assert_eq!(flat[1], "README.md");
    }

    #[test]
    fn test_prioritized_files_flat_list_all_categories_are_included() {
        let mut pf = PrioritizedFiles::new();
        pf.entrypoints = vec!["e.rs".to_string()];
        pf.public_apis = vec!["a.rs".to_string()];
        pf.config_surfaces = vec!["c.toml".to_string()];
        pf.key_files = vec!["k.md".to_string()];
        pf.build_files = vec!["b.yaml".to_string()];
        pf.dependency_manifests = vec!["d.toml".to_string()];
        pf.test_files = vec!["t.rs".to_string()];
        pf.documentation_files = vec!["doc.md".to_string()];
        pf.high_fan_in_signals = vec!["h.rs".to_string()];
        assert_eq!(pf.flat_list().len(), 9);
    }

    // ------------------------------------------------------------------
    // PrioritizedFiles::total_unique_files
    // ------------------------------------------------------------------

    #[test]
    fn test_prioritized_files_total_unique_files_counts_without_duplicates() {
        let mut pf = PrioritizedFiles::new();
        pf.entrypoints = vec!["a.rs".to_string(), "b.rs".to_string()];
        pf.public_apis = vec!["b.rs".to_string(), "c.rs".to_string()];
        assert_eq!(pf.total_unique_files(), 3);
    }

    #[test]
    fn test_prioritized_files_total_unique_files_zero_when_empty() {
        assert_eq!(PrioritizedFiles::new().total_unique_files(), 0);
    }

    // ------------------------------------------------------------------
    // FilePrioritizer::prioritize - preselection preference
    // ------------------------------------------------------------------

    #[test]
    fn test_file_prioritizer_prioritize_prefers_preselection_entrypoints() {
        let mut scan = empty_scan();
        scan.entrypoints = vec!["fallback.rs".to_string()];
        scan.plugin_preselection.entrypoints = vec!["preferred.rs".to_string()];

        let pf = FilePrioritizer::prioritize(&scan, 50, true, true);
        assert!(pf.entrypoints.contains(&"preferred.rs".to_string()));
        assert!(!pf.entrypoints.contains(&"fallback.rs".to_string()));
    }

    #[test]
    fn test_file_prioritizer_prioritize_falls_back_to_scan_result_entrypoints() {
        let mut scan = empty_scan();
        scan.entrypoints = vec!["src/main.rs".to_string()];
        // plugin_preselection.entrypoints is empty by default.

        let pf = FilePrioritizer::prioritize(&scan, 50, true, true);
        assert!(pf.entrypoints.contains(&"src/main.rs".to_string()));
    }

    #[test]
    fn test_file_prioritizer_prioritize_prefers_preselection_test_files() {
        let mut scan = empty_scan();
        scan.test_files = vec!["tests/fallback.rs".to_string()];
        scan.plugin_preselection.test_files = vec!["tests/preferred.rs".to_string()];

        let pf = FilePrioritizer::prioritize(&scan, 50, true, true);
        assert!(pf.test_files.contains(&"tests/preferred.rs".to_string()));
        assert!(!pf.test_files.contains(&"tests/fallback.rs".to_string()));
    }

    // ------------------------------------------------------------------
    // FilePrioritizer::prioritize - include_tests
    // ------------------------------------------------------------------

    #[test]
    fn test_file_prioritizer_prioritize_excludes_tests_when_include_tests_false() {
        let mut scan = empty_scan();
        scan.test_files = vec!["tests/foo.rs".to_string()];

        let pf = FilePrioritizer::prioritize(&scan, 50, false, true);
        assert!(pf.test_files.is_empty());
    }

    #[test]
    fn test_file_prioritizer_prioritize_includes_tests_when_include_tests_true() {
        let mut scan = empty_scan();
        scan.test_files = vec!["tests/foo.rs".to_string()];

        let pf = FilePrioritizer::prioritize(&scan, 50, true, true);
        assert!(!pf.test_files.is_empty());
    }

    // ------------------------------------------------------------------
    // FilePrioritizer::prioritize - include_docs
    // ------------------------------------------------------------------

    #[test]
    fn test_file_prioritizer_prioritize_excludes_docs_when_include_docs_false() {
        let mut scan = empty_scan();
        scan.documentation_inventory = vec!["README.md".to_string()];

        let pf = FilePrioritizer::prioritize(&scan, 50, true, false);
        assert!(pf.documentation_files.is_empty());
    }

    #[test]
    fn test_file_prioritizer_prioritize_includes_docs_when_include_docs_true() {
        let mut scan = empty_scan();
        scan.documentation_inventory = vec!["README.md".to_string()];

        let pf = FilePrioritizer::prioritize(&scan, 50, true, true);
        assert!(!pf.documentation_files.is_empty());
        assert!(pf.documentation_files.contains(&"README.md".to_string()));
    }

    // ------------------------------------------------------------------
    // FilePrioritizer::capped_flat_list - max_files cap
    // ------------------------------------------------------------------

    #[test]
    fn test_file_prioritizer_capped_flat_list_respects_max_files() {
        let mut scan = empty_scan();
        scan.entrypoints = vec![
            "a.rs".to_string(),
            "b.rs".to_string(),
            "c.rs".to_string(),
            "d.rs".to_string(),
            "e.rs".to_string(),
        ];

        let capped = FilePrioritizer::capped_flat_list(&scan, 3, false, false);
        assert_eq!(capped.len(), 3);
    }

    #[test]
    fn test_file_prioritizer_capped_flat_list_returns_all_when_below_cap() {
        let mut scan = empty_scan();
        scan.entrypoints = vec!["a.rs".to_string(), "b.rs".to_string()];

        let capped = FilePrioritizer::capped_flat_list(&scan, 10, false, false);
        assert_eq!(capped.len(), 2);
    }

    #[test]
    fn test_file_prioritizer_capped_flat_list_empty_scan_returns_empty() {
        let scan = empty_scan();
        let capped = FilePrioritizer::capped_flat_list(&scan, 50, true, true);
        assert!(capped.is_empty());
    }

    #[test]
    fn test_file_prioritizer_capped_flat_list_zero_max_returns_empty() {
        let mut scan = empty_scan();
        scan.entrypoints = vec!["a.rs".to_string()];

        let capped = FilePrioritizer::capped_flat_list(&scan, 0, true, true);
        assert!(capped.is_empty());
    }

    #[test]
    fn test_file_prioritizer_capped_flat_list_deduplicates_across_categories() {
        let mut scan = empty_scan();
        scan.entrypoints = vec!["main.rs".to_string()];
        scan.public_apis = vec!["main.rs".to_string(), "lib.rs".to_string()];

        let capped = FilePrioritizer::capped_flat_list(&scan, 50, false, false);
        assert_eq!(capped.len(), 2);
        assert_eq!(capped[0], "main.rs");
        assert_eq!(capped[1], "lib.rs");
    }
}
