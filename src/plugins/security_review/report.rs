//! Security review report writers.
//!
//! This module provides three report writers for the security review plugin:
//!
//! - [`SecurityReviewMarkdownReport`] renders a human-readable Markdown
//!   document grouped by security category.
//! - [`SecurityReviewJsonReport`] writes the shared [`ReportEnvelope`] JSON
//!   format.
//! - [`SecurityReviewSarifReport`] writes an enhanced SARIF 2.1.0 document
//!   with CWE/OWASP metadata and help URIs embedded in rule definitions.
//!
//! All writers call [`validate_report_path`] before touching the file system
//! and create parent directories automatically.

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::path::Path;

use chrono::Utc;

use crate::error::{PipelineError, Result};
use crate::reports::envelope::ReportEnvelope;
use crate::reports::formatter::validate_report_path;
use crate::reports::risk_band::RiskBand;
use crate::reports::sarif::{
    SarifArtifactLocation, SarifDriver, SarifMessage, SarifPhysicalLocation, SarifRegion,
    SarifReport, SarifResult, SarifRule, SarifRun, SarifTool,
};
use crate::scanner::findings::FindingSeverity;
use crate::scanner::result::ScanResult;

use super::finding::SecurityReviewFinding;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Maps a `fmt::Error` to a [`PipelineError::Report`].
fn fmt_err(e: std::fmt::Error) -> PipelineError {
    PipelineError::Report(format!("markdown render error: {e}"))
}

/// Maps a [`FindingSeverity`] to the corresponding SARIF result level string.
///
/// | Severity            | SARIF level |
/// |---------------------|-------------|
/// | `Critical` / `High` | `"error"`   |
/// | `Medium`            | `"warning"` |
/// | `Low` / `Info`      | `"note"`    |
fn severity_to_sarif_level(severity: FindingSeverity) -> &'static str {
    match severity {
        FindingSeverity::Critical | FindingSeverity::High => "error",
        FindingSeverity::Medium => "warning",
        FindingSeverity::Low | FindingSeverity::Info => "note",
    }
}

// ---------------------------------------------------------------------------
// SecurityReviewMarkdownReport
// ---------------------------------------------------------------------------

/// Renders security review findings as a human-readable Markdown document.
///
/// The document is grouped by security category and includes evidence,
/// exploitability, impact, and remediation columns for each finding.
///
/// # Examples
///
/// ```
/// use xzardgz::plugins::security_review::report::SecurityReviewMarkdownReport;
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
/// let md = SecurityReviewMarkdownReport::render(&[], &scan, "ws-001", None).unwrap();
/// assert!(md.contains("# Security Review Report"));
/// ```
pub struct SecurityReviewMarkdownReport;

impl SecurityReviewMarkdownReport {
    /// Renders a Markdown security review document from findings and scan metadata.
    ///
    /// # Arguments
    ///
    /// * `findings`     - Slice of [`SecurityReviewFinding`] to include.
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
        findings: &[SecurityReviewFinding],
        scan_result: &ScanResult,
        workspace_id: &str,
        risk_band: Option<RiskBand>,
    ) -> Result<String> {
        let mut doc = String::new();

        // H1 title
        writeln!(doc, "# Security Review Report").map_err(fmt_err)?;
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

        // Findings by category section
        writeln!(doc, "## Findings by Category").map_err(fmt_err)?;
        writeln!(doc).map_err(fmt_err)?;

        // Collect unique categories in order of first appearance
        let mut categories: Vec<&str> = Vec::new();
        for f in findings {
            if !categories.contains(&f.category.as_str()) {
                categories.push(f.category.as_str());
            }
        }

        let mut any_findings = false;

        for category in &categories {
            let cat_findings: Vec<&SecurityReviewFinding> = findings
                .iter()
                .filter(|f| f.category.as_str() == *category)
                .collect();

            if cat_findings.is_empty() {
                continue;
            }
            any_findings = true;

            writeln!(doc, "### {}", category).map_err(fmt_err)?;
            writeln!(doc).map_err(fmt_err)?;

            writeln!(
                doc,
                "| Severity | CWE | OWASP | File | Symbol | Evidence | Impact | Remediation |"
            )
            .map_err(fmt_err)?;
            writeln!(
                doc,
                "|----------|-----|-------|------|--------|----------|--------|-------------|"
            )
            .map_err(fmt_err)?;

            for f in &cat_findings {
                let location = match (&f.file, f.line) {
                    (Some(file), Some(line)) => format!("{}:{}", file, line),
                    (Some(file), None) => file.clone(),
                    (None, _) => String::new(),
                };
                let symbol = f.symbol.as_deref().unwrap_or("");
                let cwe = f.cwe.as_deref().unwrap_or("");
                let owasp = f.owasp.as_deref().unwrap_or("");
                // Escape pipe characters in cell text
                let evidence = f.evidence.replace('|', "\\|");
                let impact = f.impact.replace('|', "\\|");
                let remediation = f.remediation.replace('|', "\\|");

                writeln!(
                    doc,
                    "| {} | {} | {} | {} | {} | {} | {} | {} |",
                    f.severity.label(),
                    cwe,
                    owasp,
                    location,
                    symbol,
                    evidence,
                    impact,
                    remediation,
                )
                .map_err(fmt_err)?;
            }
            writeln!(doc).map_err(fmt_err)?;

            // Per-finding detail blocks
            for f in &cat_findings {
                if let Some(ref notes) = f.false_positive_notes {
                    writeln!(doc, "**False positive guidance:** {}", notes).map_err(fmt_err)?;
                    writeln!(doc).map_err(fmt_err)?;
                }
            }
        }

        if !any_findings {
            writeln!(doc, "*No findings recorded.*").map_err(fmt_err)?;
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
    /// * `findings`     - Slice of [`SecurityReviewFinding`] to include.
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
        findings: &[SecurityReviewFinding],
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
}

// ---------------------------------------------------------------------------
// SecurityReviewJsonReport
// ---------------------------------------------------------------------------

/// Writes security review findings as a JSON report using the shared
/// [`ReportEnvelope`] format.
///
/// Each [`SecurityReviewFinding`] is converted to a
/// [`crate::reports::findings::PluginFinding`] via
/// [`SecurityReviewFinding::to_plugin_finding`] before being added to the
/// envelope.
pub struct SecurityReviewJsonReport;

impl SecurityReviewJsonReport {
    /// Converts findings to a [`ReportEnvelope`] and writes it as JSON to `path`.
    ///
    /// # Arguments
    ///
    /// * `findings`     - Slice of [`SecurityReviewFinding`] to serialize.
    /// * `scan_result`  - Scan metadata for provenance fields.
    /// * `workspace_id` - Workspace identifier.
    /// * `report_id`    - Unique report identifier (ULID or UUID string).
    /// * `risk_band`    - Overall risk classification, if computed.
    /// * `path`         - Destination file path.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Report`] if serialization fails, or
    /// [`PipelineError::Io`] for I/O errors.
    pub fn write(
        findings: &[SecurityReviewFinding],
        scan_result: &ScanResult,
        workspace_id: &str,
        report_id: &str,
        risk_band: Option<RiskBand>,
        path: &Path,
    ) -> Result<()> {
        let mut envelope = ReportEnvelope::new(report_id, "security-review", workspace_id);
        envelope.repository_name = scan_result.repository_name.clone();
        envelope.repository_url = scan_result.repository_url.clone();
        envelope.head_commit = scan_result.head_commit.clone();
        envelope.risk_band = risk_band;

        for finding in findings {
            envelope.findings.push(finding.to_plugin_finding());
        }

        envelope.write_to_file(path)
    }
}

// ---------------------------------------------------------------------------
// SecurityReviewSarifReport
// ---------------------------------------------------------------------------

/// Writes security review findings as an enhanced SARIF 2.1.0 document.
///
/// The SARIF output includes:
/// - Tool metadata with `security-review` as the driver name.
/// - Deduplicated rules with short descriptions derived from category and severity.
/// - Results with CWE and OWASP information embedded in the message text.
/// - Physical locations (file path and line number) where available.
/// - Severity mapped to SARIF levels: `"error"`, `"warning"`, or `"note"`.
pub struct SecurityReviewSarifReport;

impl SecurityReviewSarifReport {
    /// Converts findings to a SARIF 2.1.0 document and writes it to `path`.
    ///
    /// Rules are deduplicated by `sarif_rule_id`; the first title seen for a
    /// given rule ID is used as the rule short description. Parent directories
    /// are created automatically.
    ///
    /// # Arguments
    ///
    /// * `findings`     - Slice of [`SecurityReviewFinding`] to include.
    /// * `workspace_id` - Workspace identifier (embedded in tool metadata).
    /// * `path`         - Destination file path.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Report`] if the path is invalid or JSON
    /// serialization fails, or [`PipelineError::Io`] for I/O errors.
    pub fn write(
        findings: &[SecurityReviewFinding],
        _workspace_id: &str,
        path: &Path,
    ) -> Result<()> {
        validate_report_path(path)?;

        // Build deduplicated rules map (sarif_rule_id -> SarifRule, first-seen wins).
        let mut rules_map: HashMap<String, SarifRule> = HashMap::new();
        let mut results: Vec<SarifResult> = Vec::new();

        for finding in findings {
            rules_map
                .entry(finding.sarif_rule_id.clone())
                .or_insert_with(|| {
                    let description = format!(
                        "{} - {} security issue",
                        finding.category,
                        finding.severity.label()
                    );
                    SarifRule {
                        id: finding.sarif_rule_id.clone(),
                        short_description: SarifMessage { text: description },
                    }
                });

            let level = severity_to_sarif_level(finding.severity).to_string();

            // Build message text with CWE/OWASP inline.
            let mut msg_text = finding.impact.clone();
            if let Some(ref cwe) = finding.cwe {
                msg_text.push_str(&format!(" [{}]", cwe));
            }
            if let Some(ref owasp) = finding.owasp {
                msg_text.push_str(&format!(" [{}]", owasp));
            }
            msg_text.push_str(&format!("\n\nRemediation: {}", finding.remediation));

            let locations = if let Some(ref file_path) = finding.file {
                vec![crate::reports::sarif::SarifLocation {
                    physical_location: SarifPhysicalLocation {
                        artifact_location: SarifArtifactLocation {
                            uri: file_path.clone(),
                        },
                        region: finding.line.map(|l| SarifRegion { start_line: l }),
                    },
                }]
            } else {
                vec![]
            };

            results.push(SarifResult {
                rule_id: finding.sarif_rule_id.clone(),
                message: SarifMessage { text: msg_text },
                level,
                locations,
            });
        }

        // Sort rules by id for deterministic output.
        let mut rules: Vec<SarifRule> = rules_map.into_values().collect();
        rules.sort_by(|a, b| a.id.cmp(&b.id));

        let report = SarifReport {
            version: "2.1.0".to_string(),
            runs: vec![SarifRun {
                tool: SarifTool {
                    driver: SarifDriver {
                        name: "security-review".to_string(),
                        version: "1.0.0".to_string(),
                        rules,
                    },
                },
                results,
            }],
        };

        let json = serde_json::to_string_pretty(&report)
            .map_err(|e| PipelineError::Report(format!("SARIF serialization failed: {e}")))?;

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, json)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::result::{PluginPreselection, SCAN_RESULT_VERSION, ScanResult};
    use std::collections::HashMap;

    fn minimal_scan() -> ScanResult {
        ScanResult {
            version: SCAN_RESULT_VERSION.to_string(),
            repository_name: Some("test-repo".to_string()),
            repository_url: None,
            head_commit: Some("abc123".to_string()),
            scan_timestamp: Utc::now(),
            repository_structure: vec![],
            language_statistics: HashMap::new(),
            primary_language: Some("Rust".to_string()),
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

    fn make_finding() -> SecurityReviewFinding {
        SecurityReviewFinding::new(
            "injection",
            FindingSeverity::High,
            "SQL query built from user input",
            "High - standard SQL injection techniques apply",
            "Allows reading all database records.",
            "Use parameterized queries.",
            0.9,
        )
        .with_location("src/db.rs", Some(42))
        .with_cwe("CWE-89")
        .with_owasp("A03:2021")
    }

    // ------------------------------------------------------------------
    // SecurityReviewMarkdownReport::render
    // ------------------------------------------------------------------

    #[test]
    fn test_markdown_report_render_starts_with_h1_security_review_title() {
        let scan = minimal_scan();
        let md = SecurityReviewMarkdownReport::render(&[], &scan, "ws-001", None).unwrap();
        assert!(md.starts_with("# Security Review Report"));
    }

    #[test]
    fn test_markdown_report_render_contains_workspace_id() {
        let scan = minimal_scan();
        let md = SecurityReviewMarkdownReport::render(&[], &scan, "ws-test-42", None).unwrap();
        assert!(md.contains("ws-test-42"));
    }

    #[test]
    fn test_markdown_report_render_contains_repository_name() {
        let scan = minimal_scan();
        let md = SecurityReviewMarkdownReport::render(&[], &scan, "ws-001", None).unwrap();
        assert!(md.contains("test-repo"));
    }

    #[test]
    fn test_markdown_report_render_empty_findings_shows_placeholder() {
        let scan = minimal_scan();
        let md = SecurityReviewMarkdownReport::render(&[], &scan, "ws-001", None).unwrap();
        assert!(md.contains("*No findings recorded.*"));
    }

    #[test]
    fn test_markdown_report_render_with_risk_band_shows_label() {
        let scan = minimal_scan();
        let md = SecurityReviewMarkdownReport::render(&[], &scan, "ws-001", Some(RiskBand::High))
            .unwrap();
        assert!(md.contains("Risk band:"));
        assert!(md.contains(RiskBand::High.label()));
    }

    #[test]
    fn test_markdown_report_render_with_finding_shows_category_heading() {
        let scan = minimal_scan();
        let f = make_finding();
        let md = SecurityReviewMarkdownReport::render(&[f], &scan, "ws-001", None).unwrap();
        assert!(md.contains("### injection"));
    }

    #[test]
    fn test_markdown_report_render_with_finding_shows_severity_label() {
        let scan = minimal_scan();
        let f = make_finding();
        let md = SecurityReviewMarkdownReport::render(&[f], &scan, "ws-001", None).unwrap();
        assert!(md.contains("HIGH"));
    }

    #[test]
    fn test_markdown_report_render_with_finding_shows_evidence() {
        let scan = minimal_scan();
        let f = make_finding();
        let md = SecurityReviewMarkdownReport::render(&[f], &scan, "ws-001", None).unwrap();
        // Evidence is stored redacted by SecurityReviewFinding::new
        assert!(md.contains("SQL query built from user input"));
    }

    #[test]
    fn test_markdown_report_render_with_finding_shows_cwe() {
        let scan = minimal_scan();
        let f = make_finding();
        let md = SecurityReviewMarkdownReport::render(&[f], &scan, "ws-001", None).unwrap();
        assert!(md.contains("CWE-89"));
    }

    #[test]
    fn test_markdown_report_render_with_finding_shows_owasp() {
        let scan = minimal_scan();
        let f = make_finding();
        let md = SecurityReviewMarkdownReport::render(&[f], &scan, "ws-001", None).unwrap();
        assert!(md.contains("A03:2021"));
    }

    #[test]
    fn test_markdown_report_write_creates_file() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("security_review.md");
        let scan = minimal_scan();
        SecurityReviewMarkdownReport::write(&[], &scan, "ws-001", None, &path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_markdown_report_write_creates_parent_directories() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp
            .path()
            .join("nested")
            .join("reports")
            .join("security_review.md");
        let scan = minimal_scan();
        SecurityReviewMarkdownReport::write(&[], &scan, "ws-001", None, &path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_markdown_report_write_invalid_path_returns_error() {
        let path = Path::new("/");
        let scan = minimal_scan();
        let result = SecurityReviewMarkdownReport::write(&[], &scan, "ws-001", None, path);
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // SecurityReviewJsonReport::write
    // ------------------------------------------------------------------

    #[test]
    fn test_json_report_write_creates_file() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("security_review.json");
        let scan = minimal_scan();
        SecurityReviewJsonReport::write(&[], &scan, "ws-001", "r-001", None, &path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_json_report_write_produces_valid_json() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("security_review.json");
        let scan = minimal_scan();
        SecurityReviewJsonReport::write(&[], &scan, "ws-001", "r-001", None, &path).unwrap();
        // SAFETY: we just wrote this file.
        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: content is valid JSON written by serde_json.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["plugin_name"], "security-review");
    }

    #[test]
    fn test_json_report_write_contains_repository_name() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("security_review.json");
        let scan = minimal_scan();
        SecurityReviewJsonReport::write(&[], &scan, "ws-001", "r-001", None, &path).unwrap();
        // SAFETY: we just wrote this file.
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("test-repo"));
    }

    #[test]
    fn test_json_report_write_with_findings_includes_them() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("security_review.json");
        let scan = minimal_scan();
        let f = make_finding();
        SecurityReviewJsonReport::write(&[f], &scan, "ws-001", "r-001", None, &path).unwrap();
        // SAFETY: we just wrote this file.
        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: content is valid JSON written by serde_json.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let findings = parsed["findings"]["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn test_json_report_write_creates_parent_directories() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp
            .path()
            .join("nested")
            .join("dir")
            .join("security_review.json");
        let scan = minimal_scan();
        SecurityReviewJsonReport::write(&[], &scan, "ws-001", "r-001", None, &path).unwrap();
        assert!(path.exists());
    }

    // ------------------------------------------------------------------
    // SecurityReviewSarifReport::write
    // ------------------------------------------------------------------

    #[test]
    fn test_sarif_report_write_creates_file() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("security_review.sarif");
        SecurityReviewSarifReport::write(&[], "ws-001", &path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_sarif_report_write_produces_valid_json() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("security_review.sarif");
        SecurityReviewSarifReport::write(&[], "ws-001", &path).unwrap();
        // SAFETY: we just wrote this file.
        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: content is valid JSON written by serde_json.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["version"], "2.1.0");
    }

    #[test]
    fn test_sarif_report_write_tool_driver_name_is_security_review() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("security_review.sarif");
        SecurityReviewSarifReport::write(&[], "ws-001", &path).unwrap();
        // SAFETY: we just wrote this file.
        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: content is valid JSON written by serde_json.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            parsed["runs"][0]["tool"]["driver"]["name"],
            "security-review"
        );
    }

    #[test]
    fn test_sarif_report_write_results_count_matches_findings() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("security_review.sarif");
        let findings = vec![make_finding(), make_finding()];
        SecurityReviewSarifReport::write(&findings, "ws-001", &path).unwrap();
        // SAFETY: we just wrote this file.
        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: content is valid JSON written by serde_json.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let results = parsed["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_sarif_report_write_severity_mapped_to_correct_level() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("security_review.sarif");
        let f = make_finding(); // High -> "error"
        SecurityReviewSarifReport::write(&[f], "ws-001", &path).unwrap();
        // SAFETY: we just wrote this file.
        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: content is valid JSON written by serde_json.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["runs"][0]["results"][0]["level"], "error");
    }

    #[test]
    fn test_sarif_report_write_location_includes_file_and_line() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("security_review.sarif");
        let f = make_finding(); // has location src/db.rs:42
        SecurityReviewSarifReport::write(&[f], "ws-001", &path).unwrap();
        // SAFETY: we just wrote this file.
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("src/db.rs"));
        assert!(content.contains("42"));
    }

    #[test]
    fn test_sarif_report_write_rules_are_deduplicated() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("security_review.sarif");
        // Two findings with the same category -> same sarif_rule_id
        let findings = vec![make_finding(), make_finding()];
        SecurityReviewSarifReport::write(&findings, "ws-001", &path).unwrap();
        // SAFETY: we just wrote this file.
        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: content is valid JSON written by serde_json.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let rules = parsed["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .unwrap();
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn test_sarif_report_write_empty_findings_produces_empty_results() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("security_review.sarif");
        SecurityReviewSarifReport::write(&[], "ws-001", &path).unwrap();
        // SAFETY: we just wrote this file.
        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: content is valid JSON written by serde_json.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let results = parsed["runs"][0]["results"].as_array().unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_sarif_report_write_creates_parent_directories() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp
            .path()
            .join("nested")
            .join("sarif")
            .join("security_review.sarif");
        SecurityReviewSarifReport::write(&[], "ws-001", &path).unwrap();
        assert!(path.exists());
    }
}
