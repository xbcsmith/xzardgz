//! JSON report writer.
//!
//! [`JsonReportWriter`] serializes a [`ReportEnvelope`] to a pretty-printed
//! JSON file. The output is identical to what
//! [`ReportEnvelope::write_to_file`] produces and is suitable for
//! programmatic consumption by downstream tooling.

use crate::error::Result;
use crate::reports::envelope::ReportEnvelope;
use crate::reports::formatter::{PluginReportFormatter, validate_report_path};
use std::path::Path;

// ---------------------------------------------------------------------------
// JsonReportWriter
// ---------------------------------------------------------------------------

/// Writes a [`ReportEnvelope`] to disk as a pretty-printed JSON file.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use xzardgz::reports::envelope::ReportEnvelope;
/// use xzardgz::reports::formatter::PluginReportFormatter;
/// use xzardgz::reports::json::JsonReportWriter;
///
/// let writer = JsonReportWriter;
/// let envelope = ReportEnvelope::new("r-001", "my_plugin", "ws-abc");
/// writer.write(&envelope, Path::new("/tmp/out/report.json")).expect("write failed");
/// ```
pub struct JsonReportWriter;

impl PluginReportFormatter for JsonReportWriter {
    /// Returns `"json"`.
    fn format_name(&self) -> &str {
        "json"
    }

    /// Validates `path`, then delegates to [`ReportEnvelope::write_to_file`].
    ///
    /// Parent directories are created automatically.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::PipelineError::Report`] if the path is invalid
    /// or serialization fails, or [`crate::error::PipelineError::Io`] for I/O
    /// errors.
    fn write(&self, envelope: &ReportEnvelope, path: &Path) -> Result<()> {
        validate_report_path(path)?;
        envelope.write_to_file(path)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::findings::PluginFinding;
    use crate::reports::formatter::PluginReportFormatter;
    use crate::reports::risk_band::RiskBand;
    use crate::scanner::findings::FindingSeverity;

    fn make_envelope() -> ReportEnvelope {
        let mut env = ReportEnvelope::new("json-test-001", "json_plugin", "ws-json");
        env.risk_band = Some(RiskBand::High);
        env.repository_name = Some("test-repo".to_string());
        env.findings.push(
            PluginFinding::new(
                "sql_injection",
                "SQL Injection",
                "Unsanitized input.",
                FindingSeverity::High,
                0.88,
            )
            .with_location("src/db.rs", Some(42)),
        );
        env
    }

    // ------------------------------------------------------------------
    // format_name
    // ------------------------------------------------------------------

    #[test]
    fn test_json_report_writer_format_name_returns_json() {
        let writer = JsonReportWriter;
        assert_eq!(writer.format_name(), "json");
    }

    // ------------------------------------------------------------------
    // write
    // ------------------------------------------------------------------

    #[test]
    fn test_json_report_writer_write_creates_file() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("report.json");

        let writer = JsonReportWriter;
        let env = make_envelope();
        // SAFETY: writing to a freshly created temp directory should not fail.
        writer.write(&env, &path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_json_report_writer_write_produces_valid_json() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("report.json");

        let writer = JsonReportWriter;
        let env = make_envelope();
        writer.write(&env, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: we just wrote this file so it is valid JSON.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["plugin_name"], "json_plugin");
        assert_eq!(parsed["report_id"], "json-test-001");
        assert_eq!(parsed["version"], "1");
    }

    #[test]
    fn test_json_report_writer_write_contains_findings() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("findings.json");

        let writer = JsonReportWriter;
        let env = make_envelope();
        writer.write(&env, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("sql_injection"));
        assert!(content.contains("SQL Injection"));
    }

    #[test]
    fn test_json_report_writer_write_creates_parent_directories() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("nested").join("out").join("report.json");

        let writer = JsonReportWriter;
        let env = make_envelope();
        writer.write(&env, &path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_json_report_writer_write_invalid_path_returns_error() {
        let writer = JsonReportWriter;
        let env = make_envelope();
        let result = writer.write(&env, Path::new("/"));
        assert!(result.is_err());
    }
}
