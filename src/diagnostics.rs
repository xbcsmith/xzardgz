//! Structured diagnostics for the XZardgz pipeline.
//!
//! This module provides types and utilities for collecting, persisting, and
//! reporting warnings and informational messages produced during config, scan,
//! plugin, provider fallback, watcher routing, Kafka publish, and MCP operations.
//!
//! Diagnostics can be accumulated across pipeline stages, serialized to JSON for
//! inclusion in reports, and persisted to disk as part of workspace state.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Severity level of a diagnostic entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticLevel {
    /// A warning-level diagnostic that indicates a potential problem.
    Warning,
    /// An informational diagnostic that provides context without indicating a problem.
    Info,
}

impl fmt::Display for DiagnosticLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticLevel::Warning => write!(f, "WARNING"),
            DiagnosticLevel::Info => write!(f, "INFO"),
        }
    }
}

/// Category of a diagnostic entry, identifying which pipeline stage produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticCategory {
    /// A diagnostic produced during configuration loading or validation.
    Config,
    /// A diagnostic produced during repository scanning.
    Scan,
    /// A diagnostic produced by a plugin.
    Plugin,
    /// A diagnostic produced during provider fallback.
    ProviderFallback,
    /// A diagnostic produced during watcher routing.
    WatcherRouting,
    /// A diagnostic produced during Kafka publish.
    KafkaPublish,
    /// A diagnostic produced by an MCP operation.
    Mcp,
    /// A diagnostic produced by the governance system.
    Governance,
}

impl fmt::Display for DiagnosticCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticCategory::Config => write!(f, "config"),
            DiagnosticCategory::Scan => write!(f, "scan"),
            DiagnosticCategory::Plugin => write!(f, "plugin"),
            DiagnosticCategory::ProviderFallback => write!(f, "provider_fallback"),
            DiagnosticCategory::WatcherRouting => write!(f, "watcher_routing"),
            DiagnosticCategory::KafkaPublish => write!(f, "kafka_publish"),
            DiagnosticCategory::Mcp => write!(f, "mcp"),
            DiagnosticCategory::Governance => write!(f, "governance"),
        }
    }
}

/// A single diagnostic entry capturing a warning or informational message from the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// The severity level of this diagnostic.
    pub level: DiagnosticLevel,
    /// The pipeline category that produced this diagnostic.
    pub category: DiagnosticCategory,
    /// The human-readable message describing the diagnostic.
    pub message: String,
    /// Optional extra context, such as a plugin name or server name.
    pub context: Option<String>,
    /// The UTC timestamp at which this diagnostic was created.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Diagnostic {
    /// Creates a new [`DiagnosticLevel::Warning`]-level diagnostic with the current UTC time
    /// and no context.
    pub fn warning(category: DiagnosticCategory, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            category,
            message: message.into(),
            context: None,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Creates a new [`DiagnosticLevel::Warning`]-level diagnostic with the current UTC time
    /// and the provided context string (e.g., a plugin name or server name).
    pub fn warning_with_context(
        category: DiagnosticCategory,
        message: impl Into<String>,
        context: impl Into<String>,
    ) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            category,
            message: message.into(),
            context: Some(context.into()),
            timestamp: chrono::Utc::now(),
        }
    }

    /// Creates a new [`DiagnosticLevel::Info`]-level diagnostic with the current UTC time
    /// and no context.
    pub fn info(category: DiagnosticCategory, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Info,
            category,
            message: message.into(),
            context: None,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// A collection of [`Diagnostic`] entries accumulated during a pipeline run.
///
/// `Diagnostics` can be persisted to disk, loaded back, merged with other
/// collections, and serialized to JSON for inclusion in reports or watcher
/// result messages.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Diagnostics {
    /// The ordered list of diagnostic entries collected so far.
    pub entries: Vec<Diagnostic>,
}

impl Diagnostics {
    /// Creates a new, empty `Diagnostics` collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a [`Diagnostic`] entry to the collection.
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.entries.push(diagnostic);
    }

    /// Convenience method: creates a [`DiagnosticLevel::Warning`] diagnostic and appends it.
    pub fn push_warning(&mut self, category: DiagnosticCategory, message: impl Into<String>) {
        self.push(Diagnostic::warning(category, message));
    }

    /// Convenience method: creates a [`DiagnosticLevel::Warning`] diagnostic with context
    /// and appends it.
    pub fn push_warning_with_context(
        &mut self,
        category: DiagnosticCategory,
        message: impl Into<String>,
        context: impl Into<String>,
    ) {
        self.push(Diagnostic::warning_with_context(category, message, context));
    }

    /// Convenience method: creates a [`DiagnosticLevel::Info`] diagnostic and appends it.
    pub fn push_info(&mut self, category: DiagnosticCategory, message: impl Into<String>) {
        self.push(Diagnostic::info(category, message));
    }

    /// Returns references to all [`DiagnosticLevel::Warning`]-level entries.
    pub fn warnings(&self) -> Vec<&Diagnostic> {
        self.entries
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Warning)
            .collect()
    }

    /// Returns references to all entries whose category matches `category`.
    pub fn by_category(&self, category: &DiagnosticCategory) -> Vec<&Diagnostic> {
        self.entries
            .iter()
            .filter(|d| &d.category == category)
            .collect()
    }

    /// Returns `true` if the collection contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the total number of entries in the collection.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Serializes the entire collection to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserializes a `Diagnostics` collection from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Writes the collection as JSON to `path`.
    ///
    /// Parent directories are created automatically when they do not already exist.
    pub fn persist(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = self.to_json().map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Reads a `Diagnostics` collection from `path`.
    ///
    /// Returns an empty [`Diagnostics`] when the file does not exist, so callers
    /// do not need to special-case a missing workspace state file.
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Appends all entries from `other` into this collection.
    pub fn merge(&mut self, other: Diagnostics) {
        self.entries.extend(other.entries);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_warning_creates_with_warning_level() {
        let d = Diagnostic::warning(DiagnosticCategory::Config, "config issue");
        assert_eq!(d.level, DiagnosticLevel::Warning);
        assert_eq!(d.category, DiagnosticCategory::Config);
        assert_eq!(d.message, "config issue");
        assert!(d.context.is_none());
    }

    #[test]
    fn test_diagnostic_info_creates_with_info_level() {
        let d = Diagnostic::info(DiagnosticCategory::Scan, "scan complete");
        assert_eq!(d.level, DiagnosticLevel::Info);
        assert_eq!(d.category, DiagnosticCategory::Scan);
        assert_eq!(d.message, "scan complete");
        assert!(d.context.is_none());
    }

    #[test]
    fn test_diagnostic_warning_with_context_sets_context() {
        let d = Diagnostic::warning_with_context(
            DiagnosticCategory::Plugin,
            "plugin failed",
            "my_plugin",
        );
        assert_eq!(d.level, DiagnosticLevel::Warning);
        assert_eq!(d.category, DiagnosticCategory::Plugin);
        assert_eq!(d.message, "plugin failed");
        assert_eq!(d.context, Some("my_plugin".to_string()));
    }

    #[test]
    fn test_diagnostics_push_increases_len() {
        let mut diags = Diagnostics::new();
        assert_eq!(diags.len(), 0);
        diags.push(Diagnostic::warning(DiagnosticCategory::Config, "test"));
        assert_eq!(diags.len(), 1);
        diags.push(Diagnostic::info(DiagnosticCategory::Scan, "test2"));
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn test_diagnostics_push_warning_convenience() {
        let mut diags = Diagnostics::new();
        diags.push_warning(DiagnosticCategory::Scan, "scan warning");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags.entries[0].level, DiagnosticLevel::Warning);
        assert_eq!(diags.entries[0].message, "scan warning");
    }

    #[test]
    fn test_diagnostics_warnings_filters_correctly() {
        let mut diags = Diagnostics::new();
        diags.push_warning(DiagnosticCategory::Config, "warn1");
        diags.push_info(DiagnosticCategory::Config, "info1");
        diags.push_warning(DiagnosticCategory::Scan, "warn2");

        let warnings = diags.warnings();
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().all(|d| d.level == DiagnosticLevel::Warning));
    }

    #[test]
    fn test_diagnostics_by_category_filters_correctly() {
        let mut diags = Diagnostics::new();
        diags.push_warning(DiagnosticCategory::Config, "config warn");
        diags.push_warning(DiagnosticCategory::Scan, "scan warn");
        diags.push_info(DiagnosticCategory::Config, "config info");

        let config_diags = diags.by_category(&DiagnosticCategory::Config);
        assert_eq!(config_diags.len(), 2);
        assert!(
            config_diags
                .iter()
                .all(|d| d.category == DiagnosticCategory::Config)
        );

        let scan_diags = diags.by_category(&DiagnosticCategory::Scan);
        assert_eq!(scan_diags.len(), 1);
    }

    #[test]
    fn test_diagnostics_to_json_and_from_json_roundtrip() {
        let mut diags = Diagnostics::new();
        diags.push_warning(DiagnosticCategory::Mcp, "mcp issue");
        diags.push_info(DiagnosticCategory::KafkaPublish, "published");

        // SAFETY: serialization of well-formed in-memory data cannot fail.
        let json = diags.to_json().unwrap();
        assert!(!json.is_empty());

        // SAFETY: we just serialized this string ourselves so it is valid JSON.
        let restored = Diagnostics::from_json(&json).unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(restored.entries[0].message, "mcp issue");
        assert_eq!(restored.entries[0].level, DiagnosticLevel::Warning);
        assert_eq!(restored.entries[1].message, "published");
        assert_eq!(restored.entries[1].level, DiagnosticLevel::Info);
    }

    #[test]
    fn test_diagnostics_persist_and_load_roundtrip() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory,
        // which is not expected in a standard test environment.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("diagnostics.json");

        let mut diags = Diagnostics::new();
        diags.push_warning(DiagnosticCategory::WatcherRouting, "route miss");
        diags.push_info(DiagnosticCategory::ProviderFallback, "fell back to ollama");

        // SAFETY: writing to a freshly created temp directory should not fail.
        diags.persist(&path).unwrap();
        assert!(path.exists());

        // SAFETY: the file was just written and contains valid JSON.
        let loaded = Diagnostics::load(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.entries[0].message, "route miss");
        assert_eq!(
            loaded.entries[0].category,
            DiagnosticCategory::WatcherRouting
        );
        assert_eq!(loaded.entries[1].message, "fell back to ollama");
    }

    #[test]
    fn test_diagnostics_load_returns_empty_when_file_missing() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("nonexistent.json");

        // SAFETY: load() returns Ok(empty) when the path does not exist.
        let loaded = Diagnostics::load(&path).unwrap();
        assert!(loaded.is_empty());
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn test_diagnostics_merge_combines_entries() {
        let mut a = Diagnostics::new();
        a.push_warning(DiagnosticCategory::Config, "a1");

        let mut b = Diagnostics::new();
        b.push_info(DiagnosticCategory::Scan, "b1");
        b.push_warning(DiagnosticCategory::Plugin, "b2");

        a.merge(b);
        assert_eq!(a.len(), 3);
        assert_eq!(a.entries[0].message, "a1");
        assert_eq!(a.entries[1].message, "b1");
        assert_eq!(a.entries[2].message, "b2");
    }

    #[test]
    fn test_diagnostic_level_display() {
        assert_eq!(DiagnosticLevel::Warning.to_string(), "WARNING");
        assert_eq!(DiagnosticLevel::Info.to_string(), "INFO");
    }

    #[test]
    fn test_diagnostic_category_display() {
        assert_eq!(DiagnosticCategory::Config.to_string(), "config");
        assert_eq!(DiagnosticCategory::Scan.to_string(), "scan");
        assert_eq!(DiagnosticCategory::Plugin.to_string(), "plugin");
        assert_eq!(
            DiagnosticCategory::ProviderFallback.to_string(),
            "provider_fallback"
        );
        assert_eq!(
            DiagnosticCategory::WatcherRouting.to_string(),
            "watcher_routing"
        );
        assert_eq!(
            DiagnosticCategory::KafkaPublish.to_string(),
            "kafka_publish"
        );
        assert_eq!(DiagnosticCategory::Mcp.to_string(), "mcp");
        assert_eq!(DiagnosticCategory::Governance.to_string(), "governance");
    }

    #[test]
    fn test_diagnostics_is_empty_when_no_entries() {
        let diags = Diagnostics::new();
        assert!(diags.is_empty());
        assert_eq!(diags.len(), 0);
    }
}
