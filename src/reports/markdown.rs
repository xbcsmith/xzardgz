//! Markdown report writer.
//!
//! [`MarkdownReportWriter`] renders a [`ReportEnvelope`] as a human-readable
//! Markdown document. The output is suitable for display in code-review
//! tooling, CI job summaries, and static documentation sites.

use crate::error::{PipelineError, Result};
use crate::reports::envelope::ReportEnvelope;
use crate::reports::formatter::{PluginReportFormatter, validate_report_path};
use std::fmt::Write as FmtWrite;
use std::path::Path;

// ---------------------------------------------------------------------------
// Render helper
// ---------------------------------------------------------------------------

/// Maps a `std::fmt::Error` to a [`PipelineError::Report`].
///
/// Writing into a `String` via `fmt::Write` is infallible in practice, but
/// the trait returns `Result<(), fmt::Error>` so we must handle the type.
fn fmt_err(e: std::fmt::Error) -> PipelineError {
    PipelineError::Report(format!("markdown render error: {e}"))
}

// ---------------------------------------------------------------------------
// MarkdownReportWriter
// ---------------------------------------------------------------------------

/// Writes a [`ReportEnvelope`] to disk as a Markdown document.
///
/// The document contains:
/// - H1 heading with the plugin name.
/// - Generated-at timestamp and workspace/repository metadata.
/// - Risk band summary (if present).
/// - H2 Findings section with a Markdown table of all findings.
/// - H2 Diagnostics section (only if diagnostics are present).
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use xzardgz::reports::envelope::ReportEnvelope;
/// use xzardgz::reports::formatter::PluginReportFormatter;
/// use xzardgz::reports::markdown::MarkdownReportWriter;
///
/// let writer = MarkdownReportWriter;
/// let envelope = ReportEnvelope::new("r-001", "my_plugin", "ws-abc");
/// writer.write(&envelope, Path::new("/tmp/out/report.md")).expect("write failed");
/// ```
pub struct MarkdownReportWriter;

impl MarkdownReportWriter {
    /// Renders the envelope to a Markdown string.
    ///
    /// # Arguments
    ///
    /// * `envelope` - The report envelope to render.
    ///
    /// # Returns
    ///
    /// A `Result<String>` containing the Markdown document.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Report`] if string formatting fails (in
    /// practice this cannot occur when writing to a `String`).
    fn render(&self, envelope: &ReportEnvelope) -> Result<String> {
        let mut doc = String::new();

        // H1 title
        writeln!(doc, "# Report: {}", envelope.plugin_name).map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;

        // Metadata block
        writeln!(
            doc,
            "Generated at: {}",
            envelope.generated_at.format("%Y-%m-%dT%H:%M:%SZ")
        )
        .map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;

        // Repository info
        match (&envelope.repository_name, &envelope.repository_url) {
            (Some(name), Some(url)) => {
                writeln!(doc, "Repository: {} ({})", name, url).map_err(fmt_err)?;
                writeln!(doc).map_err(fmt_err)?;
            }
            (Some(name), None) => {
                writeln!(doc, "Repository: {}", name).map_err(fmt_err)?;
                writeln!(doc).map_err(fmt_err)?;
            }
            (None, Some(url)) => {
                writeln!(doc, "Repository URL: {}", url).map_err(fmt_err)?;
                writeln!(doc).map_err(fmt_err)?;
            }
            (None, None) => {}
        }

        // Head commit
        if let Some(ref commit) = envelope.head_commit {
            writeln!(doc, "Commit: {}", commit).map_err(fmt_err)?;
            writeln!(doc).map_err(fmt_err)?;
        }

        // Workspace
        writeln!(doc, "Workspace: {}", envelope.workspace_id).map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;

        // Risk band
        if let Some(ref band) = envelope.risk_band {
            writeln!(doc, "Risk band: {}", band.label()).map_err(fmt_err)?;
            writeln!(doc).map_err(fmt_err)?;
        }

        // H2: Findings
        writeln!(doc, "## Findings").map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;

        if envelope.findings.is_empty() {
            writeln!(doc, "*No findings recorded.*").map_err(fmt_err)?;
        } else {
            writeln!(doc, "| Severity | Kind | Title | Location |").map_err(fmt_err)?;
            writeln!(doc, "|----------|------|-------|----------|").map_err(fmt_err)?;

            for finding in &envelope.findings.findings {
                let location = match (&finding.file_path, finding.line) {
                    (Some(file), Some(line)) => format!("{}:{}", file, line),
                    (Some(file), None) => file.clone(),
                    (None, _) => String::new(),
                };
                writeln!(
                    doc,
                    "| {} | {} | {} | {} |",
                    finding.severity.label(),
                    finding.kind,
                    finding.title,
                    location,
                )
                .map_err(fmt_err)?;
            }
        }
        writeln!(doc).map_err(fmt_err)?;

        // H2: Diagnostics (only when present)
        if !envelope.diagnostics.is_empty() {
            writeln!(doc, "## Diagnostics").map_err(fmt_err)?;
            writeln!(doc).map_err(fmt_err)?;

            for diag in &envelope.diagnostics {
                let context = diag
                    .context
                    .as_deref()
                    .map(|c| format!(" ({})", c))
                    .unwrap_or_default();
                writeln!(doc, "- [{}] {}{}", diag.level, diag.message, context).map_err(fmt_err)?;
            }
        }

        Ok(doc)
    }
}

impl PluginReportFormatter for MarkdownReportWriter {
    /// Returns `"markdown"`.
    fn format_name(&self) -> &str {
        "markdown"
    }

    /// Renders the envelope as Markdown and writes it to `path`.
    ///
    /// Parent directories are created automatically.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Report`] if the path is invalid or rendering
    /// fails, or [`PipelineError::Io`] for I/O errors.
    fn write(&self, envelope: &ReportEnvelope, path: &Path) -> Result<()> {
        validate_report_path(path)?;

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        let markdown = self.render(envelope)?;
        std::fs::write(path, markdown)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{Diagnostic, DiagnosticCategory};
    use crate::reports::findings::PluginFinding;
    use crate::reports::formatter::PluginReportFormatter;
    use crate::reports::risk_band::RiskBand;
    use crate::scanner::findings::FindingSeverity;

    fn make_envelope() -> ReportEnvelope {
        let mut env = ReportEnvelope::new("md-test-001", "md_plugin", "ws-md");
        env.risk_band = Some(RiskBand::High);
        env.repository_name = Some("my-repo".to_string());
        env.repository_url = Some("https://github.com/org/my-repo".to_string());
        env.head_commit = Some("deadbeef".to_string());
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
        env.findings.push(PluginFinding::new(
            "xss",
            "Cross-Site Scripting",
            "Unescaped output.",
            FindingSeverity::Medium,
            0.65,
        ));
        env.diagnostics.push(Diagnostic::warning(
            DiagnosticCategory::Plugin,
            "model fallback occurred",
        ));
        env
    }

    // ------------------------------------------------------------------
    // format_name
    // ------------------------------------------------------------------

    #[test]
    fn test_markdown_report_writer_format_name_returns_markdown() {
        let writer = MarkdownReportWriter;
        assert_eq!(writer.format_name(), "markdown");
    }

    // ------------------------------------------------------------------
    // render
    // ------------------------------------------------------------------

    #[test]
    fn test_markdown_report_writer_render_contains_h1_with_plugin_name() {
        let writer = MarkdownReportWriter;
        let env = make_envelope();
        // SAFETY: render into String is infallible.
        let md = writer.render(&env).unwrap();
        assert!(md.contains("# Report: md_plugin"), "missing H1 heading");
    }

    #[test]
    fn test_markdown_report_writer_render_contains_generated_at() {
        let writer = MarkdownReportWriter;
        let env = make_envelope();
        let md = writer.render(&env).unwrap();
        assert!(md.contains("Generated at:"), "missing generated at");
    }

    #[test]
    fn test_markdown_report_writer_render_contains_repository_info() {
        let writer = MarkdownReportWriter;
        let env = make_envelope();
        let md = writer.render(&env).unwrap();
        assert!(md.contains("my-repo"), "missing repository name");
        assert!(
            md.contains("https://github.com/org/my-repo"),
            "missing repository URL"
        );
    }

    #[test]
    fn test_markdown_report_writer_render_contains_workspace_id() {
        let writer = MarkdownReportWriter;
        let env = make_envelope();
        let md = writer.render(&env).unwrap();
        assert!(md.contains("ws-md"), "missing workspace id");
    }

    #[test]
    fn test_markdown_report_writer_render_contains_risk_band() {
        let writer = MarkdownReportWriter;
        let env = make_envelope();
        let md = writer.render(&env).unwrap();
        assert!(md.contains("HIGH"), "missing risk band label");
    }

    #[test]
    fn test_markdown_report_writer_render_contains_findings_section() {
        let writer = MarkdownReportWriter;
        let env = make_envelope();
        let md = writer.render(&env).unwrap();
        assert!(md.contains("## Findings"), "missing findings section");
        assert!(md.contains("sql_injection"), "missing finding kind");
        assert!(md.contains("SQL Injection"), "missing finding title");
        assert!(md.contains("src/db.rs:42"), "missing file:line location");
    }

    #[test]
    fn test_markdown_report_writer_render_contains_diagnostics_section() {
        let writer = MarkdownReportWriter;
        let env = make_envelope();
        let md = writer.render(&env).unwrap();
        assert!(md.contains("## Diagnostics"), "missing diagnostics section");
        assert!(
            md.contains("model fallback occurred"),
            "missing diagnostic message"
        );
    }

    #[test]
    fn test_markdown_report_writer_render_no_findings_shows_placeholder() {
        let writer = MarkdownReportWriter;
        let env = ReportEnvelope::new("empty", "empty_plugin", "ws-empty");
        let md = writer.render(&env).unwrap();
        assert!(
            md.contains("*No findings recorded.*"),
            "missing placeholder"
        );
    }

    #[test]
    fn test_markdown_report_writer_render_omits_diagnostics_section_when_empty() {
        let writer = MarkdownReportWriter;
        let mut env = ReportEnvelope::new("no-diag", "p", "ws");
        env.findings
            .push(PluginFinding::new("k", "t", "d", FindingSeverity::Low, 0.1));
        let md = writer.render(&env).unwrap();
        assert!(
            !md.contains("## Diagnostics"),
            "diagnostics section should be absent when empty"
        );
    }

    #[test]
    fn test_markdown_report_writer_render_finding_without_location_shows_empty_location() {
        let writer = MarkdownReportWriter;
        let mut env = ReportEnvelope::new("r", "p", "ws");
        env.findings
            .push(PluginFinding::new("k", "t", "d", FindingSeverity::Low, 0.1));
        let md = writer.render(&env).unwrap();
        // Table row with empty location column: "| LOW | k | t |  |"
        assert!(md.contains("| k |"), "kind should appear in table");
    }

    // ------------------------------------------------------------------
    // write (filesystem)
    // ------------------------------------------------------------------

    #[test]
    fn test_markdown_report_writer_write_creates_file() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("report.md");

        let writer = MarkdownReportWriter;
        let env = make_envelope();
        // SAFETY: writing to a freshly created temp directory should not fail.
        writer.write(&env, &path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_markdown_report_writer_write_file_content_is_markdown() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("report.md");

        let writer = MarkdownReportWriter;
        let env = make_envelope();
        writer.write(&env, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("# Report:"), "should start with H1");
        assert!(content.contains("## Findings"), "should have findings H2");
    }

    #[test]
    fn test_markdown_report_writer_write_creates_parent_directories() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("nested").join("out").join("report.md");

        let writer = MarkdownReportWriter;
        let env = make_envelope();
        writer.write(&env, &path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_markdown_report_writer_write_invalid_path_returns_error() {
        let writer = MarkdownReportWriter;
        let env = make_envelope();
        let result = writer.write(&env, Path::new("/"));
        assert!(result.is_err());
    }
}
