//! Technical review report writers.
//!
//! This module provides two report writers for the technical review plugin:
//!
//! - [`TechnicalReviewMarkdownReport`] renders a human-readable Markdown
//!   document grouped by review dimension with evidence, impact, and
//!   recommendations for each finding.
//! - [`TechnicalReviewJsonReport`] writes the shared [`ReportEnvelope`] JSON
//!   format, converting each [`TechnicalReviewFinding`] to a [`PluginFinding`].
//!
//! Both writers call [`validate_report_path`] before touching the file system
//! and create parent directories automatically.

use std::fmt::Write as FmtWrite;
use std::path::Path;

use chrono::Utc;

use crate::error::{PipelineError, Result};
use crate::reports::envelope::ReportEnvelope;
use crate::reports::findings::PluginFinding;
use crate::reports::formatter::validate_report_path;
use crate::reports::risk_band::RiskBand;
use crate::scanner::findings::FindingSeverity;
use crate::scanner::result::ScanResult;

use super::dimensions::ReviewDimension;
use super::finding::TechnicalReviewFinding;

use crate::clients::ExternalSignals;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Maps a `fmt::Error` to a [`PipelineError::Report`].
fn fmt_err(e: std::fmt::Error) -> PipelineError {
    PipelineError::Report(format!("markdown render error: {e}"))
}

// ---------------------------------------------------------------------------
// TechnicalReviewMarkdownReport
// ---------------------------------------------------------------------------

/// Renders technical review findings as a human-readable Markdown document.
///
/// The document is grouped by review dimension and includes evidence, impact,
/// and recommendation columns for each finding.
///
/// # Examples
///
/// ```
/// use xzardgz::plugins::technical_review::report::TechnicalReviewMarkdownReport;
/// use xzardgz::scanner::result::{ScanResult, PluginPreselection, SCAN_RESULT_VERSION};
/// use chrono::Utc;
/// use std::collections::HashMap;
///
/// let scan = ScanResult {
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
/// let md = TechnicalReviewMarkdownReport::render(&[], &scan, "ws-001", None).unwrap();
/// assert!(md.contains("# Technical Review Report"));
/// ```
pub struct TechnicalReviewMarkdownReport;

impl TechnicalReviewMarkdownReport {
    /// Renders a Markdown technical review document from findings and scan metadata.
    ///
    /// # Arguments
    ///
    /// * `findings`     - Slice of [`TechnicalReviewFinding`] to include.
    /// * `scan_result`  - Repository scan metadata for provenance and summary.
    /// * `workspace_id` - Workspace identifier to embed in the document.
    /// * `risk_band`    - Overall risk classification, if computed.
    ///
    /// # Returns
    ///
    /// A `Result<String>` containing the Markdown document.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Report`] if string formatting fails (should not
    /// occur in practice when writing to a `String`).
    pub fn render(
        findings: &[TechnicalReviewFinding],
        scan_result: &ScanResult,
        workspace_id: &str,
        risk_band: Option<RiskBand>,
    ) -> Result<String> {
        let mut doc = String::new();

        // H1 title
        writeln!(doc, "# Technical Review Report").map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;

        // Metadata
        writeln!(
            doc,
            "Generated at: {}",
            Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
        )
        .map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;

        // Repository info
        match (&scan_result.repository_name, &scan_result.repository_url) {
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

        if let Some(ref commit) = scan_result.head_commit {
            writeln!(doc, "Commit: {}", commit).map_err(fmt_err)?;
            writeln!(doc).map_err(fmt_err)?;
        }

        writeln!(doc, "Workspace: {}", workspace_id).map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;

        if let Some(band) = risk_band {
            writeln!(doc, "Risk band: {}", band.label()).map_err(fmt_err)?;
            writeln!(doc).map_err(fmt_err)?;
        }

        // Summary section
        writeln!(doc, "## Summary").map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;

        let primary_lang = scan_result
            .primary_language
            .as_deref()
            .unwrap_or("Not detected");
        writeln!(doc, "Primary language: {}", primary_lang).map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;

        if scan_result.frameworks.is_empty() {
            writeln!(doc, "Frameworks: None detected").map_err(fmt_err)?;
        } else {
            writeln!(doc, "Frameworks: {}", scan_result.frameworks.join(", ")).map_err(fmt_err)?;
        }
        writeln!(doc).map_err(fmt_err)?;

        writeln!(doc, "Total findings: {}", findings.len()).map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;

        if !findings.is_empty() {
            writeln!(doc, "Findings by severity:").map_err(fmt_err)?;
            writeln!(doc).map_err(fmt_err)?;

            for severity in &[
                FindingSeverity::Critical,
                FindingSeverity::High,
                FindingSeverity::Medium,
                FindingSeverity::Low,
                FindingSeverity::Info,
            ] {
                let count = findings.iter().filter(|f| f.severity == *severity).count();
                if count > 0 {
                    writeln!(doc, "- {}: {}", severity.label(), count).map_err(fmt_err)?;
                }
            }
            writeln!(doc).map_err(fmt_err)?;
        }

        // Findings by dimension
        writeln!(doc, "## Findings by Dimension").map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;

        let all_dimensions = ReviewDimension::all();
        let mut any_findings = false;

        for dimension in &all_dimensions {
            let dim_findings: Vec<&TechnicalReviewFinding> = findings
                .iter()
                .filter(|f| f.category == dimension.as_str())
                .collect();

            if dim_findings.is_empty() {
                continue;
            }
            any_findings = true;

            writeln!(doc, "### {}", dimension.display_name()).map_err(fmt_err)?;
            writeln!(doc).map_err(fmt_err)?;

            writeln!(
                doc,
                "| Severity | File | Symbol | Evidence | Impact | Recommendation |"
            )
            .map_err(fmt_err)?;
            writeln!(
                doc,
                "|----------|------|--------|----------|--------|----------------|"
            )
            .map_err(fmt_err)?;

            for f in &dim_findings {
                let location = match (&f.file, f.line) {
                    (Some(file), Some(line)) => format!("{}:{}", file, line),
                    (Some(file), None) => file.clone(),
                    (None, _) => String::new(),
                };
                let symbol = f.symbol.as_deref().unwrap_or("");
                // Escape pipe characters in cell text
                let evidence = f.evidence.replace('|', "\\|");
                let impact = f.impact.replace('|', "\\|");
                let recommendation = f.recommendation.replace('|', "\\|");

                writeln!(
                    doc,
                    "| {} | {} | {} | {} | {} | {} |",
                    f.severity.label(),
                    location,
                    symbol,
                    evidence,
                    impact,
                    recommendation,
                )
                .map_err(fmt_err)?;
            }
            writeln!(doc).map_err(fmt_err)?;
        }

        if !any_findings {
            writeln!(doc, "*No findings recorded.*").map_err(fmt_err)?;
            writeln!(doc).map_err(fmt_err)?;
        }

        // Dimensions with no findings
        let empty_dims: Vec<&ReviewDimension> = all_dimensions
            .iter()
            .filter(|d| !findings.iter().any(|f| f.category == d.as_str()))
            .collect();

        if !empty_dims.is_empty() && !findings.is_empty() {
            writeln!(doc, "## Dimensions with No Findings").map_err(fmt_err)?;
            writeln!(doc).map_err(fmt_err)?;
            for dim in &empty_dims {
                writeln!(doc, "- {}", dim.display_name()).map_err(fmt_err)?;
            }
            writeln!(doc).map_err(fmt_err)?;
        }

        // Confidence section
        if !findings.is_empty() {
            let avg_confidence =
                findings.iter().map(|f| f.confidence).sum::<f64>() / findings.len() as f64;
            writeln!(doc, "## Confidence").map_err(fmt_err)?;
            writeln!(doc).map_err(fmt_err)?;
            writeln!(doc, "Average confidence: {:.2}", avg_confidence).map_err(fmt_err)?;
            writeln!(doc).map_err(fmt_err)?;
        }

        Ok(doc)
    }

    /// Renders and writes the Markdown report to `path`.
    ///
    /// Parent directories are created automatically.
    ///
    /// # Arguments
    ///
    /// * `findings`     - Slice of [`TechnicalReviewFinding`] to include.
    /// * `scan_result`  - Repository scan metadata.
    /// * `workspace_id` - Workspace identifier.
    /// * `risk_band`    - Overall risk classification, if computed.
    /// * `path`         - Destination file path.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Report`] if the path is invalid or rendering
    /// fails, or [`PipelineError::Io`] for I/O errors.
    pub fn write(
        findings: &[TechnicalReviewFinding],
        scan_result: &ScanResult,
        workspace_id: &str,
        risk_band: Option<RiskBand>,
        path: &Path,
    ) -> Result<()> {
        validate_report_path(path)?;

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        let markdown = Self::render(findings, scan_result, workspace_id, risk_band)?;
        std::fs::write(path, markdown)?;
        Ok(())
    }

    /// Renders a Markdown technical review document with an optional supply-chain
    /// signals section appended after the confidence section.
    ///
    /// Calls [`TechnicalReviewMarkdownReport::render`] for the main body and
    /// appends a "Supply Chain Signals" section when `signals` is `Some` and
    /// non-empty.
    ///
    /// # Arguments
    ///
    /// * `findings`     - Slice of [`TechnicalReviewFinding`] to include.
    /// * `scan_result`  - Repository scan metadata for provenance and summary.
    /// * `workspace_id` - Workspace identifier to embed in the document.
    /// * `risk_band`    - Overall risk classification, if computed.
    /// * `signals`      - Optional external supply-chain signals. When `None`
    ///   or empty, the section is omitted.
    ///
    /// # Returns
    ///
    /// A `Result<String>` containing the Markdown document.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Report`] if string formatting fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::report::TechnicalReviewMarkdownReport;
    /// use xzardgz::clients::ExternalSignals;
    /// use xzardgz::scanner::result::{ScanResult, PluginPreselection, SCAN_RESULT_VERSION};
    /// use chrono::Utc;
    /// use std::collections::HashMap;
    ///
    /// let scan = ScanResult {
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
    /// let signals = ExternalSignals { scorecard: None, repodata: None };
    /// let md = TechnicalReviewMarkdownReport::render_with_signals(
    ///     &[], &scan, "ws-001", None, Some(&signals)
    /// ).unwrap();
    /// assert!(md.contains("# Technical Review Report"));
    /// ```
    pub fn render_with_signals(
        findings: &[TechnicalReviewFinding],
        scan_result: &ScanResult,
        workspace_id: &str,
        risk_band: Option<RiskBand>,
        signals: Option<&ExternalSignals>,
    ) -> Result<String> {
        let mut doc = Self::render(findings, scan_result, workspace_id, risk_band)?;
        if let Some(sig) = signals
            && !sig.is_empty()
        {
            render_signals_section(&mut doc, sig)?;
        }
        Ok(doc)
    }

    /// Renders the Markdown report with optional supply-chain signals and writes
    /// it to `path`.
    ///
    /// Parent directories are created automatically.
    ///
    /// # Arguments
    ///
    /// * `findings`     - Slice of [`TechnicalReviewFinding`] to include.
    /// * `scan_result`  - Repository scan metadata.
    /// * `workspace_id` - Workspace identifier.
    /// * `risk_band`    - Overall risk classification, if computed.
    /// * `path`         - Destination file path.
    /// * `signals`      - Optional external supply-chain signals.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Report`] if the path is invalid or rendering
    /// fails, or [`PipelineError::Io`] for I/O errors.
    pub fn write_with_signals(
        findings: &[TechnicalReviewFinding],
        scan_result: &ScanResult,
        workspace_id: &str,
        risk_band: Option<RiskBand>,
        path: &Path,
        signals: Option<&ExternalSignals>,
    ) -> Result<()> {
        validate_report_path(path)?;
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let markdown =
            Self::render_with_signals(findings, scan_result, workspace_id, risk_band, signals)?;
        std::fs::write(path, markdown)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Supply-chain signals section renderer
// ---------------------------------------------------------------------------

/// Appends a "Supply Chain Signals" Markdown section to `doc`.
///
/// Writes Scorecard score and per-check table if scorecard data is present,
/// and a metadata list if repository metadata is present.
fn render_signals_section(doc: &mut String, signals: &ExternalSignals) -> Result<()> {
    writeln!(doc, "## Supply Chain Signals").map_err(fmt_err)?;
    writeln!(doc).map_err(fmt_err)?;

    if let Some(sc) = &signals.scorecard {
        writeln!(doc, "### OpenSSF Scorecard").map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;
        writeln!(doc, "Score: {:.1}/10", sc.score).map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;
        if !sc.checks.is_empty() {
            writeln!(doc, "| Check | Score | Reason |").map_err(fmt_err)?;
            writeln!(doc, "|-------|-------|--------|").map_err(fmt_err)?;
            for check in &sc.checks {
                let reason = check.reason.replace('|', "\\|");
                writeln!(doc, "| {} | {} | {} |", check.name, check.score, reason)
                    .map_err(fmt_err)?;
            }
            writeln!(doc).map_err(fmt_err)?;
        }
    }

    if let Some(rd) = &signals.repodata {
        writeln!(doc, "### Repository Metadata").map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;
        writeln!(doc, "- Full name: {}", rd.full_name).map_err(fmt_err)?;
        if let Some(ref desc) = rd.description {
            writeln!(doc, "- Description: {}", desc).map_err(fmt_err)?;
        }
        if let Some(ref lang) = rd.language {
            writeln!(doc, "- Language: {}", lang).map_err(fmt_err)?;
        }
        writeln!(doc, "- Default branch: {}", rd.default_branch).map_err(fmt_err)?;
        writeln!(doc, "- Stars: {}", rd.stargazers_count).map_err(fmt_err)?;
        writeln!(doc, "- Forks: {}", rd.forks_count).map_err(fmt_err)?;
        writeln!(doc, "- Open issues: {}", rd.open_issues_count).map_err(fmt_err)?;
        writeln!(doc, "- Archived: {}", rd.archived).map_err(fmt_err)?;
        writeln!(doc, "- Fork: {}", rd.fork).map_err(fmt_err)?;
        if !rd.topics.is_empty() {
            writeln!(doc, "- Topics: {}", rd.topics.join(", ")).map_err(fmt_err)?;
        }
        if let Some(ref license) = rd.license {
            writeln!(doc, "- License: {}", license.name).map_err(fmt_err)?;
        }
        writeln!(doc).map_err(fmt_err)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// TechnicalReviewJsonReport
// ---------------------------------------------------------------------------

/// Writes technical review findings as a JSON report using the shared
/// [`ReportEnvelope`] format.
///
/// Each [`PluginFinding`] is written directly into the envelope, preserving
/// the blended `confidence` score and the `static_score` / `ai_score` audit
/// fields set by the confidence scorer.
pub struct TechnicalReviewJsonReport;

impl TechnicalReviewJsonReport {
    /// Writes a scored findings slice as a JSON [`ReportEnvelope`] to `path`.
    ///
    /// The caller is responsible for building each [`PluginFinding`] with
    /// scoring audit data (via
    /// [`PluginFinding::with_scoring`][crate::reports::findings::PluginFinding::with_scoring])
    /// before passing the slice here.
    ///
    /// # Arguments
    ///
    /// * `findings`     - Pre-built [`PluginFinding`] slice with scoring data.
    /// * `scan_result`  - Repository scan metadata for provenance.
    /// * `workspace_id` - Workspace identifier.
    /// * `report_id`    - Unique report identifier (e.g. a ULID string).
    /// * `risk_band`    - Overall risk classification, if computed.
    /// * `path`         - Destination file path.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Report`] if the path is invalid or
    /// serialization fails, or [`PipelineError::Io`] for I/O errors.
    pub fn write(
        findings: &[PluginFinding],
        scan_result: &ScanResult,
        workspace_id: &str,
        report_id: &str,
        risk_band: Option<RiskBand>,
        path: &Path,
    ) -> Result<()> {
        validate_report_path(path)?;

        let mut envelope = ReportEnvelope::new(report_id, "technical-review", workspace_id);
        envelope.repository_name = scan_result.repository_name.clone();
        envelope.repository_url = scan_result.repository_url.clone();
        envelope.head_commit = scan_result.head_commit.clone();
        envelope.risk_band = risk_band;

        for finding in findings {
            envelope.findings.push(finding.clone());
        }

        envelope.write_to_file(path)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::findings::FindingSeverity;
    use crate::scanner::result::{PluginPreselection, SCAN_RESULT_VERSION, ScanResult};
    use std::collections::HashMap;

    // ------------------------------------------------------------------
    // Test helpers
    // ------------------------------------------------------------------

    fn minimal_scan() -> ScanResult {
        ScanResult {
            version: SCAN_RESULT_VERSION.to_string(),
            repository_url: Some("https://github.com/org/repo".to_string()),
            repository_name: Some("repo".to_string()),
            head_commit: Some("abc123".to_string()),
            scan_timestamp: Utc::now(),
            repository_structure: vec![],
            language_statistics: HashMap::new(),
            primary_language: Some("Rust".to_string()),
            frameworks: vec!["Tokio".to_string()],
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

    fn make_finding(category: &str, severity: FindingSeverity) -> TechnicalReviewFinding {
        TechnicalReviewFinding::new(
            category,
            severity,
            "Observed something significant.",
            "This impacts system stability.",
            "Refactor to improve design.",
            0.85,
        )
    }

    // ------------------------------------------------------------------
    // TechnicalReviewMarkdownReport::render
    // ------------------------------------------------------------------

    #[test]
    fn test_markdown_report_render_starts_with_h1_title() {
        let scan = minimal_scan();
        // SAFETY: render writes into a String, which cannot fail.
        let md = TechnicalReviewMarkdownReport::render(&[], &scan, "ws-001", None).unwrap();
        assert!(
            md.starts_with("# Technical Review Report"),
            "must start with H1"
        );
    }

    #[test]
    fn test_markdown_report_render_contains_workspace_id() {
        let scan = minimal_scan();
        let md = TechnicalReviewMarkdownReport::render(&[], &scan, "ws-abc", None).unwrap();
        assert!(md.contains("ws-abc"), "must contain workspace id");
    }

    #[test]
    fn test_markdown_report_render_contains_repository_name() {
        let scan = minimal_scan();
        let md = TechnicalReviewMarkdownReport::render(&[], &scan, "ws-001", None).unwrap();
        assert!(md.contains("repo"), "must contain repository name");
    }

    #[test]
    fn test_markdown_report_render_contains_commit() {
        let scan = minimal_scan();
        let md = TechnicalReviewMarkdownReport::render(&[], &scan, "ws-001", None).unwrap();
        assert!(md.contains("abc123"), "must contain commit hash");
    }

    #[test]
    fn test_markdown_report_render_contains_primary_language() {
        let scan = minimal_scan();
        let md = TechnicalReviewMarkdownReport::render(&[], &scan, "ws-001", None).unwrap();
        assert!(md.contains("Rust"), "must contain primary language");
    }

    #[test]
    fn test_markdown_report_render_contains_framework() {
        let scan = minimal_scan();
        let md = TechnicalReviewMarkdownReport::render(&[], &scan, "ws-001", None).unwrap();
        assert!(md.contains("Tokio"), "must contain framework name");
    }

    #[test]
    fn test_markdown_report_render_empty_findings_shows_placeholder() {
        let scan = minimal_scan();
        let md = TechnicalReviewMarkdownReport::render(&[], &scan, "ws-001", None).unwrap();
        assert!(
            md.contains("*No findings recorded.*"),
            "empty findings must show placeholder"
        );
    }

    #[test]
    fn test_markdown_report_render_with_risk_band_shows_label() {
        let scan = minimal_scan();
        let md = TechnicalReviewMarkdownReport::render(&[], &scan, "ws-001", Some(RiskBand::High))
            .unwrap();
        assert!(md.contains("HIGH"), "must contain risk band label");
    }

    #[test]
    fn test_markdown_report_render_with_finding_shows_dimension_heading() {
        let scan = minimal_scan();
        let f = make_finding("architecture", FindingSeverity::High);
        let md = TechnicalReviewMarkdownReport::render(&[f], &scan, "ws-001", Some(RiskBand::High))
            .unwrap();
        assert!(
            md.contains("### Architecture"),
            "must show dimension heading"
        );
    }

    #[test]
    fn test_markdown_report_render_with_finding_shows_severity_label() {
        let scan = minimal_scan();
        let f = make_finding("architecture", FindingSeverity::High);
        let md = TechnicalReviewMarkdownReport::render(&[f], &scan, "ws-001", None).unwrap();
        assert!(md.contains("HIGH"), "must show severity label");
    }

    #[test]
    fn test_markdown_report_render_with_finding_shows_evidence() {
        let scan = minimal_scan();
        let f = make_finding("error_handling", FindingSeverity::Medium);
        let md = TechnicalReviewMarkdownReport::render(&[f], &scan, "ws-001", None).unwrap();
        assert!(
            md.contains("Observed something significant."),
            "must show evidence text"
        );
    }

    #[test]
    fn test_markdown_report_render_with_finding_shows_confidence_section() {
        let scan = minimal_scan();
        let f = make_finding("architecture", FindingSeverity::Medium);
        let md = TechnicalReviewMarkdownReport::render(&[f], &scan, "ws-001", None).unwrap();
        assert!(
            md.contains("## Confidence"),
            "must include confidence section"
        );
        assert!(md.contains("0.85"), "must include confidence value");
    }

    #[test]
    fn test_markdown_report_render_with_finding_severity_summary_non_empty() {
        let scan = minimal_scan();
        let f = make_finding("architecture", FindingSeverity::Critical);
        let md = TechnicalReviewMarkdownReport::render(&[f], &scan, "ws-001", None).unwrap();
        assert!(md.contains("CRITICAL: 1"), "must show severity count");
    }

    #[test]
    fn test_markdown_report_render_finding_with_location_shows_file_and_line() {
        let scan = minimal_scan();
        let f = make_finding("architecture", FindingSeverity::High)
            .with_location("src/main.rs", Some(42));
        let md = TechnicalReviewMarkdownReport::render(&[f], &scan, "ws-001", None).unwrap();
        assert!(md.contains("src/main.rs:42"), "must show file:line");
    }

    #[test]
    fn test_markdown_report_render_dimensions_with_no_findings_section() {
        let scan = minimal_scan();
        let f = make_finding("architecture", FindingSeverity::High);
        let md = TechnicalReviewMarkdownReport::render(&[f], &scan, "ws-001", None).unwrap();
        assert!(
            md.contains("## Dimensions with No Findings"),
            "must list empty dimensions"
        );
    }

    #[test]
    fn test_markdown_report_render_no_repo_info_when_both_none() {
        let mut scan = minimal_scan();
        scan.repository_name = None;
        scan.repository_url = None;
        let md = TechnicalReviewMarkdownReport::render(&[], &scan, "ws-001", None).unwrap();
        // Should not contain "Repository:" when both are None
        assert!(!md.contains("Repository:"), "must omit repo line when none");
    }

    // ------------------------------------------------------------------
    // TechnicalReviewMarkdownReport::write
    // ------------------------------------------------------------------

    #[test]
    fn test_markdown_report_write_creates_file() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("technical_review.md");
        let scan = minimal_scan();

        // SAFETY: writing to a freshly created temp directory cannot fail.
        TechnicalReviewMarkdownReport::write(&[], &scan, "ws-001", None, &path).unwrap();

        assert!(path.exists(), "report file must be created");
    }

    #[test]
    fn test_markdown_report_write_creates_parent_directories() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp
            .path()
            .join("nested")
            .join("reports")
            .join("technical_review.md");
        let scan = minimal_scan();

        TechnicalReviewMarkdownReport::write(&[], &scan, "ws-001", None, &path).unwrap();

        assert!(path.exists(), "must create nested directories");
    }

    #[test]
    fn test_markdown_report_write_file_content_starts_with_h1() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("technical_review.md");
        let scan = minimal_scan();

        TechnicalReviewMarkdownReport::write(&[], &scan, "ws-001", None, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.starts_with("# Technical Review Report"),
            "file must start with H1"
        );
    }

    #[test]
    fn test_markdown_report_write_invalid_path_returns_error() {
        let scan = minimal_scan();
        let result =
            TechnicalReviewMarkdownReport::write(&[], &scan, "ws-001", None, Path::new("/"));
        assert!(result.is_err(), "invalid path must return error");
    }

    // ------------------------------------------------------------------
    // TechnicalReviewJsonReport::write
    // ------------------------------------------------------------------

    #[test]
    fn test_json_report_write_creates_file() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("technical_review.json");
        let scan = minimal_scan();

        // SAFETY: writing to a freshly created temp directory cannot fail.
        TechnicalReviewJsonReport::write(&[], &scan, "ws-001", "r-001", None, &path).unwrap();

        assert!(path.exists(), "json file must be created");
    }

    #[test]
    fn test_json_report_write_produces_valid_json() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("technical_review.json");
        let scan = minimal_scan();

        TechnicalReviewJsonReport::write(&[], &scan, "ws-001", "r-test", None, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: we just wrote this file so it is valid JSON.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["plugin_name"], "technical-review");
        assert_eq!(parsed["report_id"], "r-test");
        assert_eq!(parsed["workspace_id"], "ws-001");
    }

    #[test]
    fn test_json_report_write_contains_repository_name() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("technical_review.json");
        let scan = minimal_scan();

        TechnicalReviewJsonReport::write(&[], &scan, "ws-001", "r-001", None, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("repo"), "must contain repository name");
    }

    #[test]
    fn test_json_report_write_with_findings_includes_them() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("technical_review.json");
        let scan = minimal_scan();
        let pf = make_finding("architecture", FindingSeverity::High).to_plugin_finding();

        TechnicalReviewJsonReport::write(
            &[pf],
            &scan,
            "ws-001",
            "r-001",
            Some(RiskBand::High),
            &path,
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: just written, valid JSON.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["risk_band"], "High");
        assert_eq!(parsed["findings"]["findings"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_json_report_write_creates_parent_directories() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp
            .path()
            .join("nested")
            .join("out")
            .join("technical_review.json");
        let scan = minimal_scan();

        TechnicalReviewJsonReport::write(&[], &scan, "ws-001", "r-001", None, &path).unwrap();

        assert!(path.exists(), "must create nested directories");
    }

    #[test]
    fn test_json_report_write_invalid_path_returns_error() {
        let scan = minimal_scan();
        let result =
            TechnicalReviewJsonReport::write(&[], &scan, "ws-001", "r-001", None, Path::new("/"));
        assert!(result.is_err(), "invalid path must return error");
    }

    // ------------------------------------------------------------------
    // TechnicalReviewMarkdownReport::render_with_signals
    // ------------------------------------------------------------------

    #[test]
    fn test_render_with_signals_none_produces_same_as_render() {
        let scan = minimal_scan();
        let with =
            TechnicalReviewMarkdownReport::render_with_signals(&[], &scan, "ws-1", None, None)
                .unwrap();
        let without = TechnicalReviewMarkdownReport::render(&[], &scan, "ws-1", None).unwrap();
        assert_eq!(with, without);
    }

    #[test]
    fn test_render_with_signals_empty_signals_produces_same_as_render() {
        use crate::clients::ExternalSignals;
        let scan = minimal_scan();
        let signals = ExternalSignals {
            scorecard: None,
            repodata: None,
        };
        let with = TechnicalReviewMarkdownReport::render_with_signals(
            &[],
            &scan,
            "ws-1",
            None,
            Some(&signals),
        )
        .unwrap();
        let without = TechnicalReviewMarkdownReport::render(&[], &scan, "ws-1", None).unwrap();
        assert_eq!(with, without);
    }

    #[test]
    fn test_render_with_signals_scorecard_appends_section() {
        use crate::clients::ExternalSignals;
        use crate::clients::scorecard::{ScorecardCheck, ScorecardRepoInfo, ScorecardResult};
        let scan = minimal_scan();
        let signals = ExternalSignals {
            scorecard: Some(ScorecardResult {
                date: "2024-01-01".to_string(),
                repo: ScorecardRepoInfo {
                    name: "github.com/ossf/scorecard".to_string(),
                    commit: None,
                },
                score: 7.8,
                checks: vec![ScorecardCheck {
                    name: "Binary-Artifacts".to_string(),
                    score: 10,
                    reason: "no binaries found".to_string(),
                    details: vec![],
                    documentation: None,
                }],
            }),
            repodata: None,
        };
        let md = TechnicalReviewMarkdownReport::render_with_signals(
            &[],
            &scan,
            "ws-1",
            None,
            Some(&signals),
        )
        .unwrap();
        assert!(md.contains("## Supply Chain Signals"));
        assert!(md.contains("### OpenSSF Scorecard"));
        assert!(md.contains("7.8/10"));
        assert!(md.contains("Binary-Artifacts"));
    }

    #[test]
    fn test_render_with_signals_repodata_appends_metadata() {
        use crate::clients::ExternalSignals;
        use crate::clients::repodata::RepoMetadata;
        let scan = minimal_scan();
        let signals = ExternalSignals {
            scorecard: None,
            repodata: Some(RepoMetadata {
                full_name: "ossf/scorecard".to_string(),
                description: Some("a security tool".to_string()),
                language: Some("Go".to_string()),
                default_branch: "main".to_string(),
                stargazers_count: 4200,
                forks_count: 300,
                open_issues_count: 12,
                topics: vec!["security".to_string()],
                archived: false,
                fork: false,
                visibility: Some("public".to_string()),
                size: 5000,
                license: None,
                pushed_at: None,
                updated_at: None,
            }),
        };
        let md = TechnicalReviewMarkdownReport::render_with_signals(
            &[],
            &scan,
            "ws-1",
            None,
            Some(&signals),
        )
        .unwrap();
        assert!(md.contains("### Repository Metadata"));
        assert!(md.contains("ossf/scorecard"));
        assert!(md.contains("4200"));
        assert!(md.contains("security"));
    }

    // ------------------------------------------------------------------
    // TechnicalReviewMarkdownReport::write_with_signals
    // ------------------------------------------------------------------

    #[test]
    fn test_write_with_signals_none_creates_file() {
        let tmp = tempfile::TempDir::new().expect("tmp dir");
        let path = tmp.path().join("report.md");
        TechnicalReviewMarkdownReport::write_with_signals(
            &[],
            &minimal_scan(),
            "ws-1",
            None,
            &path,
            None,
        )
        .unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_write_with_signals_with_scorecard_includes_section() {
        use crate::clients::ExternalSignals;
        use crate::clients::scorecard::{ScorecardRepoInfo, ScorecardResult};
        let tmp = tempfile::TempDir::new().expect("tmp dir");
        let path = tmp.path().join("report.md");
        let signals = ExternalSignals {
            scorecard: Some(ScorecardResult {
                date: "2024-01-01".to_string(),
                repo: ScorecardRepoInfo {
                    name: "github.com/test/repo".to_string(),
                    commit: None,
                },
                score: 5.0,
                checks: vec![],
            }),
            repodata: None,
        };
        TechnicalReviewMarkdownReport::write_with_signals(
            &[],
            &minimal_scan(),
            "ws-1",
            None,
            &path,
            Some(&signals),
        )
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("## Supply Chain Signals"));
        assert!(content.contains("5.0/10"));
    }
}
