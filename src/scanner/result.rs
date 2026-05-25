//! Versioned scan result artifact.
//!
//! Defines [`ScanResult`], the top-level data structure produced by a
//! complete repository scan.  It captures the repository structure,
//! per-language statistics, pre-AI findings, plugin preselection metadata,
//! and assorted categorised file lists.  The struct serialises to and from
//! YAML via [`ScanResult::to_yaml`] and [`ScanResult::load_from_str`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{PipelineError, Result};
use crate::scanner::findings::ScanFinding;

/// Schema version for the scan result artifact. Increment for breaking changes.
///
/// Consumers should reject or migrate results whose `version` field does not
/// match the version they were compiled against.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::result::SCAN_RESULT_VERSION;
///
/// assert_eq!(SCAN_RESULT_VERSION, "1");
/// ```
pub const SCAN_RESULT_VERSION: &str = "1";

/// A single file discovered during repository traversal.
///
/// Paths are stored as forward-slash-separated strings relative to the
/// repository root, regardless of the host operating system.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::result::FileEntry;
///
/// let entry = FileEntry {
///     path: "src/main.rs".to_string(),
///     size_bytes: 1024,
///     language: Some("Rust".to_string()),
///     is_binary: false,
/// };
/// assert_eq!(entry.path, "src/main.rs");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Repository-relative path using forward slashes.
    pub path: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Detected programming language, if recognised.
    pub language: Option<String>,
    /// Whether the file appears to contain binary (non-text) content.
    pub is_binary: bool,
}

/// Aggregated file count and byte total for a single programming language.
///
/// Used as the value type in [`ScanResult::language_statistics`].
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::result::LanguageStats;
///
/// let stats = LanguageStats { file_count: 42, total_bytes: 1_048_576 };
/// assert_eq!(stats.file_count, 42);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageStats {
    /// Number of files attributed to this language.
    pub file_count: usize,
    /// Sum of file sizes in bytes for this language.
    pub total_bytes: u64,
}

/// Consolidated file preselection metadata for plugin consumption.
///
/// Each field holds a list of repository-relative file paths that belong to
/// a particular semantic category.  Plugins can read these lists to focus
/// analysis on the most relevant files without re-scanning the repository.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::result::PluginPreselection;
///
/// let ps = PluginPreselection::default();
/// assert!(ps.entrypoints.is_empty());
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginPreselection {
    /// Repository entrypoint files (main.rs, index.js, etc.)
    pub entrypoints: Vec<String>,
    /// Files forming the public API surface.
    pub public_apis: Vec<String>,
    /// Configuration file paths.
    pub config_surfaces: Vec<String>,
    /// Dependency manifest paths (Cargo.toml, package.json, etc.)
    pub dependency_manifests: Vec<String>,
    /// Files matching risky execution patterns.
    pub risky_pattern_files: Vec<String>,
    /// Files containing secrets-like patterns.
    pub secrets_like_files: Vec<String>,
    /// Rust files containing `unsafe` blocks.
    pub unsafe_rust_files: Vec<String>,
    /// Files containing OS command execution.
    pub command_execution_files: Vec<String>,
    /// Files containing network client usage.
    pub network_client_files: Vec<String>,
    /// Files containing authentication or authorization logic.
    pub auth_files: Vec<String>,
    /// Test file paths.
    pub test_files: Vec<String>,
    /// Source files that appear to lack corresponding test coverage.
    pub missing_test_signals: Vec<String>,
}

/// The top-level artifact produced by a complete repository scan.
///
/// A `ScanResult` captures everything the scanner learns about a repository
/// in a single, version-stamped value.  It is designed to be written to disk
/// as YAML and later consumed by plugins, AI agents, and report generators.
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use std::collections::HashMap;
/// use xzardgz::scanner::result::{PluginPreselection, ScanResult, SCAN_RESULT_VERSION};
///
/// let result = ScanResult {
///     version: SCAN_RESULT_VERSION.to_string(),
///     repository_url: None,
///     repository_name: Some("my-repo".to_string()),
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
/// assert_eq!(result.version, "1");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    /// Schema version; always `SCAN_RESULT_VERSION` for this release.
    pub version: String,
    /// Remote origin URL of the scanned repository.
    pub repository_url: Option<String>,
    /// Name of the repository (last path component of the root directory).
    pub repository_name: Option<String>,
    /// HEAD commit SHA at the time of scanning.
    pub head_commit: Option<String>,
    /// UTC timestamp when the scan was performed.
    pub scan_timestamp: DateTime<Utc>,
    /// All files discovered during the scan (sorted by path).
    pub repository_structure: Vec<FileEntry>,
    /// Per-language file count and byte totals.
    pub language_statistics: HashMap<String, LanguageStats>,
    /// Language with the highest file count.
    pub primary_language: Option<String>,
    /// Detected build frameworks and toolchains.
    pub frameworks: Vec<String>,
    /// Documentation files (README, docs/, *.md, etc.)
    pub documentation_inventory: Vec<String>,
    /// Governance files (CODEOWNERS, LICENSE, SECURITY.md, etc.)
    pub governance_rules: Vec<String>,
    /// Files forming the CLI command surface.
    pub cli_commands: Vec<String>,
    /// Files forming the public API surface.
    pub public_apis: Vec<String>,
    /// Repository entrypoint files.
    pub entrypoints: Vec<String>,
    /// Configuration file paths.
    pub config_surface: Vec<String>,
    /// Key project files (README, LICENSE, CONTRIBUTING, etc.)
    pub key_files: Vec<String>,
    /// Dependency manifest paths.
    pub dependency_manifests: Vec<String>,
    /// Test file paths.
    pub test_files: Vec<String>,
    /// Build system files (Makefile, build.rs, Dockerfile, CI configs, etc.)
    pub build_files: Vec<String>,
    /// Files potentially containing secrets or credentials.
    pub security_relevant_files: Vec<String>,
    /// Pre-AI scan findings from hooks and pattern matching.
    pub findings: Vec<ScanFinding>,
    /// Consolidated preselection metadata for plugin consumption.
    pub plugin_preselection: PluginPreselection,
}

impl ScanResult {
    /// Serialises the scan result to a YAML string.
    ///
    /// # Returns
    ///
    /// A `String` containing the YAML representation of this `ScanResult`.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Scanner`] if serialisation fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use std::collections::HashMap;
    /// use xzardgz::scanner::result::{PluginPreselection, ScanResult, SCAN_RESULT_VERSION};
    ///
    /// let result = ScanResult {
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
    /// let yaml = result.to_yaml().unwrap();
    /// assert!(yaml.contains("version"));
    /// ```
    pub fn to_yaml(&self) -> Result<String> {
        serde_yaml::to_string(self)
            .map_err(|e| PipelineError::Scanner(format!("failed to serialise scan result: {e}")))
    }

    /// Deserialises a `ScanResult` from a YAML string.
    ///
    /// # Arguments
    ///
    /// * `content` - A YAML string previously produced by [`ScanResult::to_yaml`].
    ///
    /// # Returns
    ///
    /// The deserialised `ScanResult`.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Scanner`] if `content` is not valid YAML or
    /// does not match the expected schema.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use std::collections::HashMap;
    /// use xzardgz::scanner::result::{PluginPreselection, ScanResult, SCAN_RESULT_VERSION};
    ///
    /// let original = ScanResult {
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
    /// let yaml = original.to_yaml().unwrap();
    /// let loaded = ScanResult::load_from_str(&yaml).unwrap();
    /// assert_eq!(loaded.version, SCAN_RESULT_VERSION);
    /// ```
    pub fn load_from_str(content: &str) -> Result<Self> {
        serde_yaml::from_str(content)
            .map_err(|e| PipelineError::Scanner(format!("failed to deserialise scan result: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    /// Constructs a minimal, valid `ScanResult` suitable for use in tests.
    fn minimal_scan_result() -> ScanResult {
        ScanResult {
            version: SCAN_RESULT_VERSION.to_string(),
            repository_url: None,
            repository_name: Some("test-repo".to_string()),
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

    #[test]
    fn test_scan_result_version_constant_is_one() {
        assert_eq!(SCAN_RESULT_VERSION, "1");
    }

    #[test]
    fn test_file_entry_serializes_to_yaml() {
        let entry = FileEntry {
            path: "src/lib.rs".to_string(),
            size_bytes: 4096,
            language: Some("Rust".to_string()),
            is_binary: false,
        };
        let yaml = serde_yaml::to_string(&entry).expect("serialization must succeed");
        assert!(yaml.contains("src/lib.rs"));
        assert!(yaml.contains("4096"));
        assert!(yaml.contains("Rust"));
    }

    #[test]
    fn test_language_stats_file_count_and_bytes() {
        let stats = LanguageStats {
            file_count: 17,
            total_bytes: 83_456,
        };
        assert_eq!(stats.file_count, 17);
        assert_eq!(stats.total_bytes, 83_456);
    }

    #[test]
    fn test_plugin_preselection_default_has_empty_vecs() {
        let ps = PluginPreselection::default();
        assert!(ps.entrypoints.is_empty());
        assert!(ps.public_apis.is_empty());
        assert!(ps.config_surfaces.is_empty());
        assert!(ps.dependency_manifests.is_empty());
        assert!(ps.risky_pattern_files.is_empty());
        assert!(ps.secrets_like_files.is_empty());
        assert!(ps.unsafe_rust_files.is_empty());
        assert!(ps.command_execution_files.is_empty());
        assert!(ps.network_client_files.is_empty());
        assert!(ps.auth_files.is_empty());
        assert!(ps.test_files.is_empty());
        assert!(ps.missing_test_signals.is_empty());
    }

    #[test]
    fn test_scan_result_to_yaml_contains_version() {
        let result = minimal_scan_result();
        let yaml = result.to_yaml().expect("to_yaml must succeed");
        assert!(yaml.contains("version"));
        assert!(yaml.contains(SCAN_RESULT_VERSION));
    }

    #[test]
    fn test_scan_result_load_from_str_round_trips() {
        let original = minimal_scan_result();
        let yaml = original.to_yaml().expect("to_yaml must succeed");
        let loaded = ScanResult::load_from_str(&yaml).expect("load_from_str must succeed");
        assert_eq!(loaded.version, original.version);
        assert_eq!(loaded.repository_name, original.repository_name);
        assert!(loaded.findings.is_empty());
        assert!(loaded.repository_structure.is_empty());
    }

    #[test]
    fn test_scan_result_load_from_str_rejects_invalid_yaml() {
        let bad = "version: : : not valid yaml :::";
        let result = ScanResult::load_from_str(bad);
        assert!(result.is_err());
        if let Err(PipelineError::Scanner(msg)) = result {
            assert!(msg.contains("failed to deserialise scan result"));
        } else {
            panic!("expected PipelineError::Scanner");
        }
    }
}
