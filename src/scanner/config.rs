//! Scanner configuration.
//!
//! Defines [`ScannerConfig`] which controls file traversal behaviour:
//! exclusion patterns, size limits, hidden-file handling, gitignore
//! integration, and bounded-concurrency limits.

use serde::{Deserialize, Serialize};

/// Default maximum file size in bytes (1 MiB).
///
/// Files larger than this threshold are skipped during scanning to avoid
/// loading large binary or generated files into memory.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::config::DEFAULT_MAX_FILE_SIZE_BYTES;
///
/// assert_eq!(DEFAULT_MAX_FILE_SIZE_BYTES, 1_048_576);
/// ```
pub const DEFAULT_MAX_FILE_SIZE_BYTES: u64 = 1_048_576;

/// Default maximum number of files processed concurrently.
///
/// Limits parallelism during file scanning to avoid overwhelming the host
/// system with open file descriptors or thread contention.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::config::DEFAULT_MAX_CONCURRENCY;
///
/// assert_eq!(DEFAULT_MAX_CONCURRENCY, 4);
/// ```
pub const DEFAULT_MAX_CONCURRENCY: usize = 4;

/// Configuration controlling file traversal behaviour for the repository scanner.
///
/// Use [`ScannerConfig::default`] for sensible defaults, then customise with
/// the provided builder methods.  All builder methods take ownership and
/// return `Self`, enabling fluent chaining.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::config::ScannerConfig;
///
/// let config = ScannerConfig::default()
///     .with_include_hidden(true)
///     .with_max_file_size(512_000);
///
/// assert!(config.include_hidden);
/// assert_eq!(config.max_file_size_bytes, 512_000);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    /// Glob patterns for paths to exclude from scanning.
    pub exclude_patterns: Vec<String>,
    /// Maximum file size in bytes; files larger than this are skipped.
    pub max_file_size_bytes: u64,
    /// Whether to include hidden files and directories (starting with `.`).
    pub include_hidden: bool,
    /// Whether to respect `.gitignore` and other ignore files.
    pub respect_gitignore: bool,
    /// Maximum number of files processed concurrently.
    pub max_concurrency: usize,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            exclude_patterns: vec![],
            max_file_size_bytes: DEFAULT_MAX_FILE_SIZE_BYTES,
            include_hidden: false,
            respect_gitignore: true,
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
        }
    }
}

impl ScannerConfig {
    /// Replaces the exclusion pattern list.
    ///
    /// # Arguments
    ///
    /// * `patterns` - Glob patterns for paths to exclude from scanning.
    ///
    /// # Returns
    ///
    /// The updated `ScannerConfig` with the new exclusion patterns applied.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::config::ScannerConfig;
    ///
    /// let config = ScannerConfig::default()
    ///     .with_exclude_patterns(vec!["target/**".to_string(), "*.lock".to_string()]);
    ///
    /// assert_eq!(config.exclude_patterns.len(), 2);
    /// ```
    pub fn with_exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = patterns;
        self
    }

    /// Sets the maximum file size limit in bytes.
    ///
    /// Files whose size exceeds this threshold are skipped during scanning.
    ///
    /// # Arguments
    ///
    /// * `bytes` - Maximum file size in bytes.
    ///
    /// # Returns
    ///
    /// The updated `ScannerConfig` with the new file size limit applied.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::config::ScannerConfig;
    ///
    /// let config = ScannerConfig::default().with_max_file_size(2_097_152);
    /// assert_eq!(config.max_file_size_bytes, 2_097_152);
    /// ```
    pub fn with_max_file_size(mut self, bytes: u64) -> Self {
        self.max_file_size_bytes = bytes;
        self
    }

    /// Controls whether hidden files and directories are included.
    ///
    /// Hidden entries are those whose name begins with `.`.
    ///
    /// # Arguments
    ///
    /// * `include` - `true` to include hidden files; `false` to skip them.
    ///
    /// # Returns
    ///
    /// The updated `ScannerConfig` with the hidden-file flag applied.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::config::ScannerConfig;
    ///
    /// let config = ScannerConfig::default().with_include_hidden(true);
    /// assert!(config.include_hidden);
    /// ```
    pub fn with_include_hidden(mut self, include: bool) -> Self {
        self.include_hidden = include;
        self
    }

    /// Controls whether `.gitignore` and other ignore files are respected.
    ///
    /// # Arguments
    ///
    /// * `respect` - `true` to honour ignore files; `false` to traverse all entries.
    ///
    /// # Returns
    ///
    /// The updated `ScannerConfig` with the gitignore flag applied.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::config::ScannerConfig;
    ///
    /// let config = ScannerConfig::default().with_respect_gitignore(false);
    /// assert!(!config.respect_gitignore);
    /// ```
    pub fn with_respect_gitignore(mut self, respect: bool) -> Self {
        self.respect_gitignore = respect;
        self
    }

    /// Sets the maximum number of files processed concurrently.
    ///
    /// # Arguments
    ///
    /// * `n` - Upper bound on concurrent file-processing tasks.
    ///
    /// # Returns
    ///
    /// The updated `ScannerConfig` with the concurrency limit applied.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::config::ScannerConfig;
    ///
    /// let config = ScannerConfig::default().with_max_concurrency(8);
    /// assert_eq!(config.max_concurrency, 8);
    /// ```
    pub fn with_max_concurrency(mut self, n: usize) -> Self {
        self.max_concurrency = n;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_has_expected_defaults() {
        let config = ScannerConfig::default();
        assert!(config.exclude_patterns.is_empty());
        assert_eq!(config.max_file_size_bytes, DEFAULT_MAX_FILE_SIZE_BYTES);
        assert!(!config.include_hidden);
        assert!(config.respect_gitignore);
        assert_eq!(config.max_concurrency, DEFAULT_MAX_CONCURRENCY);
    }

    #[test]
    fn test_with_exclude_patterns_sets_patterns() {
        let patterns = vec!["target/**".to_string(), "*.lock".to_string()];
        let config = ScannerConfig::default().with_exclude_patterns(patterns.clone());
        assert_eq!(config.exclude_patterns, patterns);
    }

    #[test]
    fn test_with_max_file_size_sets_bytes() {
        let config = ScannerConfig::default().with_max_file_size(512_000);
        assert_eq!(config.max_file_size_bytes, 512_000);
    }

    #[test]
    fn test_with_include_hidden_sets_flag() {
        let config = ScannerConfig::default().with_include_hidden(true);
        assert!(config.include_hidden);
    }

    #[test]
    fn test_with_respect_gitignore_sets_flag() {
        let config = ScannerConfig::default().with_respect_gitignore(false);
        assert!(!config.respect_gitignore);
    }

    #[test]
    fn test_with_max_concurrency_sets_value() {
        let config = ScannerConfig::default().with_max_concurrency(8);
        assert_eq!(config.max_concurrency, 8);
    }

    #[test]
    fn test_builder_chain_returns_configured_instance() {
        let config = ScannerConfig::default()
            .with_exclude_patterns(vec!["*.log".to_string()])
            .with_max_file_size(2_097_152)
            .with_include_hidden(true)
            .with_respect_gitignore(false)
            .with_max_concurrency(16);
        assert_eq!(config.exclude_patterns, vec!["*.log".to_string()]);
        assert_eq!(config.max_file_size_bytes, 2_097_152);
        assert!(config.include_hidden);
        assert!(!config.respect_gitignore);
        assert_eq!(config.max_concurrency, 16);
    }
}
