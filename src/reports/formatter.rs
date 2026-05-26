//! Report formatter abstraction and format registration.
//!
//! This module defines the [`ReportFormat`] enum that names the supported
//! output formats, the [`PluginReportFormatter`] trait that all concrete
//! writers must implement, and the [`validate_report_path`] helper used by
//! every writer before it touches the file-system.

use crate::error::{PipelineError, Result};
use crate::reports::envelope::ReportEnvelope;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ---------------------------------------------------------------------------
// ReportFormat
// ---------------------------------------------------------------------------

/// Supported report output formats.
///
/// # Examples
///
/// ```
/// use xzardgz::reports::formatter::ReportFormat;
///
/// assert_eq!(ReportFormat::Markdown.extension(), ".md");
/// assert_eq!(ReportFormat::Json.as_str(), "json");
/// assert_eq!(ReportFormat::from_str("sarif"), Some(ReportFormat::Sarif));
/// assert_eq!(ReportFormat::from_str("MARKDOWN"), Some(ReportFormat::Markdown));
/// assert!(ReportFormat::from_str("unknown").is_none());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportFormat {
    /// Human-readable Markdown document.
    Markdown,
    /// Machine-readable JSON envelope.
    Json,
    /// SARIF 2.1.0 interoperability format.
    Sarif,
}

impl ReportFormat {
    /// Returns the conventional file extension (including the leading dot) for
    /// this format.
    ///
    /// | Format   | Extension       |
    /// |----------|-----------------|
    /// | Markdown | `".md"`         |
    /// | Json     | `".json"`       |
    /// | Sarif    | `".sarif.json"` |
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::formatter::ReportFormat;
    ///
    /// assert_eq!(ReportFormat::Markdown.extension(), ".md");
    /// assert_eq!(ReportFormat::Json.extension(), ".json");
    /// assert_eq!(ReportFormat::Sarif.extension(), ".sarif.json");
    /// ```
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Markdown => ".md",
            Self::Json => ".json",
            Self::Sarif => ".sarif.json",
        }
    }

    /// Returns the lowercase string name of this format.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::formatter::ReportFormat;
    ///
    /// assert_eq!(ReportFormat::Markdown.as_str(), "markdown");
    /// assert_eq!(ReportFormat::Json.as_str(), "json");
    /// assert_eq!(ReportFormat::Sarif.as_str(), "sarif");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Json => "json",
            Self::Sarif => "sarif",
        }
    }

    /// Parses a format from a string, case-insensitively.
    ///
    /// Accepts `"markdown"`, `"md"`, `"json"`, and `"sarif"` (any case).
    ///
    /// # Arguments
    ///
    /// * `s` - A string slice to parse.
    ///
    /// # Returns
    ///
    /// `Some(ReportFormat)` if the string is recognized, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::formatter::ReportFormat;
    ///
    /// assert_eq!(ReportFormat::from_str("markdown"), Some(ReportFormat::Markdown));
    /// assert_eq!(ReportFormat::from_str("MD"),       Some(ReportFormat::Markdown));
    /// assert_eq!(ReportFormat::from_str("JSON"),     Some(ReportFormat::Json));
    /// assert_eq!(ReportFormat::from_str("SARIF"),    Some(ReportFormat::Sarif));
    /// assert!(ReportFormat::from_str("txt").is_none());
    /// ```
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "markdown" | "md" => Some(Self::Markdown),
            "json" => Some(Self::Json),
            "sarif" => Some(Self::Sarif),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// PluginReportFormatter trait
// ---------------------------------------------------------------------------

/// Trait for types that can write a [`ReportEnvelope`] to a file path.
///
/// Implementors are responsible for:
/// 1. Calling [`validate_report_path`] before touching the file-system.
/// 2. Creating any missing parent directories.
/// 3. Rendering the envelope in the appropriate format and writing it.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use xzardgz::reports::envelope::ReportEnvelope;
/// use xzardgz::reports::formatter::PluginReportFormatter;
/// use xzardgz::reports::json::JsonReportWriter;
///
/// let writer = JsonReportWriter;
/// assert_eq!(writer.format_name(), "json");
/// ```
pub trait PluginReportFormatter: Send + Sync {
    /// Returns a short human-readable name for this formatter (e.g. `"json"`).
    fn format_name(&self) -> &str;

    /// Renders `envelope` and writes it to `path`.
    ///
    /// # Arguments
    ///
    /// * `envelope` - The report envelope to render.
    /// * `path`     - Destination file path.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::PipelineError::Report`] if the path is invalid
    /// or rendering fails, or [`crate::error::PipelineError::Io`] for I/O
    /// errors.
    fn write(&self, envelope: &ReportEnvelope, path: &Path) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Path validation helper
// ---------------------------------------------------------------------------

/// Validates that `path` is suitable for writing a report file.
///
/// The check passes when:
/// - `path` has a non-empty filename component.
/// - `path` has a parent component (even if it resolves to the current
///   directory).
///
/// # Arguments
///
/// * `path` - The candidate output path to validate.
///
/// # Returns
///
/// `Ok(())` if the path is valid.
///
/// # Errors
///
/// Returns [`PipelineError::Report`] if the path lacks a filename or a parent
/// directory component.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use xzardgz::reports::formatter::validate_report_path;
///
/// assert!(validate_report_path(Path::new("/tmp/out/report.json")).is_ok());
/// assert!(validate_report_path(Path::new("report.md")).is_ok());
/// assert!(validate_report_path(Path::new("/")).is_err());
/// ```
pub fn validate_report_path(path: &Path) -> Result<()> {
    let file_name = path.file_name().ok_or_else(|| {
        PipelineError::Report(format!(
            "report path '{}' does not contain a filename",
            path.display()
        ))
    })?;

    if file_name.is_empty() {
        return Err(PipelineError::Report(format!(
            "report path '{}' has an empty filename",
            path.display()
        )));
    }

    if path.parent().is_none() {
        return Err(PipelineError::Report(format!(
            "report path '{}' has no parent directory",
            path.display()
        )));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // ReportFormat::extension
    // ------------------------------------------------------------------

    #[test]
    fn test_report_format_extension_markdown() {
        assert_eq!(ReportFormat::Markdown.extension(), ".md");
    }

    #[test]
    fn test_report_format_extension_json() {
        assert_eq!(ReportFormat::Json.extension(), ".json");
    }

    #[test]
    fn test_report_format_extension_sarif() {
        assert_eq!(ReportFormat::Sarif.extension(), ".sarif.json");
    }

    // ------------------------------------------------------------------
    // ReportFormat::as_str
    // ------------------------------------------------------------------

    #[test]
    fn test_report_format_as_str_markdown() {
        assert_eq!(ReportFormat::Markdown.as_str(), "markdown");
    }

    #[test]
    fn test_report_format_as_str_json() {
        assert_eq!(ReportFormat::Json.as_str(), "json");
    }

    #[test]
    fn test_report_format_as_str_sarif() {
        assert_eq!(ReportFormat::Sarif.as_str(), "sarif");
    }

    // ------------------------------------------------------------------
    // ReportFormat::from_str
    // ------------------------------------------------------------------

    #[test]
    fn test_report_format_from_str_markdown_lowercase() {
        assert_eq!(
            ReportFormat::from_str("markdown"),
            Some(ReportFormat::Markdown)
        );
    }

    #[test]
    fn test_report_format_from_str_md_alias() {
        assert_eq!(ReportFormat::from_str("md"), Some(ReportFormat::Markdown));
    }

    #[test]
    fn test_report_format_from_str_markdown_uppercase() {
        assert_eq!(
            ReportFormat::from_str("MARKDOWN"),
            Some(ReportFormat::Markdown)
        );
    }

    #[test]
    fn test_report_format_from_str_json_lowercase() {
        assert_eq!(ReportFormat::from_str("json"), Some(ReportFormat::Json));
    }

    #[test]
    fn test_report_format_from_str_json_uppercase() {
        assert_eq!(ReportFormat::from_str("JSON"), Some(ReportFormat::Json));
    }

    #[test]
    fn test_report_format_from_str_sarif_lowercase() {
        assert_eq!(ReportFormat::from_str("sarif"), Some(ReportFormat::Sarif));
    }

    #[test]
    fn test_report_format_from_str_sarif_uppercase() {
        assert_eq!(ReportFormat::from_str("SARIF"), Some(ReportFormat::Sarif));
    }

    #[test]
    fn test_report_format_from_str_unknown_returns_none() {
        assert!(ReportFormat::from_str("txt").is_none());
        assert!(ReportFormat::from_str("html").is_none());
        assert!(ReportFormat::from_str("").is_none());
    }

    // ------------------------------------------------------------------
    // validate_report_path
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_report_path_with_nested_path_returns_ok() {
        let path = Path::new("/tmp/out/report.json");
        assert!(validate_report_path(path).is_ok());
    }

    #[test]
    fn test_validate_report_path_with_relative_path_returns_ok() {
        let path = Path::new("report.md");
        assert!(validate_report_path(path).is_ok());
    }

    #[test]
    fn test_validate_report_path_with_relative_nested_path_returns_ok() {
        let path = Path::new("output/report.json");
        assert!(validate_report_path(path).is_ok());
    }

    #[test]
    fn test_validate_report_path_with_root_path_returns_err() {
        let path = Path::new("/");
        let result = validate_report_path(path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Report(_)));
    }

    #[test]
    fn test_validate_report_path_with_dotdot_path_returns_err() {
        let path = Path::new("..");
        let result = validate_report_path(path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Report(_)));
    }
}
