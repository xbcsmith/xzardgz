//! SARIF 2.1.0 report writer.
//!
//! [`SarifReportWriter`] converts a [`ReportEnvelope`] into a Static Analysis
//! Results Interchange Format (SARIF) 2.1.0 document. SARIF is the standard
//! interchange format understood by GitHub Advanced Security, Azure DevOps
//! Code Scanning, and most modern CI platforms.

use crate::error::{PipelineError, Result};
use crate::reports::envelope::ReportEnvelope;
use crate::reports::formatter::{PluginReportFormatter, validate_report_path};
use crate::scanner::findings::FindingSeverity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// SARIF wire types
// ---------------------------------------------------------------------------

/// Top-level SARIF 2.1.0 document.
///
/// Corresponds to the `sarifLog` object in the SARIF specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifReport {
    /// SARIF schema version. Always `"2.1.0"`.
    pub version: String,
    /// One or more analysis runs contained in this document.
    pub runs: Vec<SarifRun>,
}

/// A single analysis run within a SARIF document.
///
/// Binds the tool that performed the analysis with the results it produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifRun {
    /// Metadata about the analysis tool.
    pub tool: SarifTool,
    /// Results produced by the tool during this run.
    pub results: Vec<SarifResult>,
}

/// Metadata wrapper for the analysis tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifTool {
    /// The primary driver component of the tool.
    pub driver: SarifDriver,
}

/// The primary driver component of an analysis tool.
///
/// Carries the tool name, version, and the set of rules it can emit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifDriver {
    /// Human-readable tool name (e.g. the plugin name).
    pub name: String,
    /// Tool version string.
    pub version: String,
    /// Rules that the tool can fire, deduplicated by `id`.
    pub rules: Vec<SarifRule>,
}

/// A rule (or check) that the analysis tool can fire.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRule {
    /// Unique rule identifier matching the `ruleId` field in results.
    pub id: String,
    /// Short human-readable description of the rule.
    pub short_description: SarifMessage,
}

/// A single analysis result (finding).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifResult {
    /// Identifier of the rule that produced this result.
    pub rule_id: String,
    /// Human-readable message for this result.
    pub message: SarifMessage,
    /// Severity level: `"error"`, `"warning"`, or `"note"`.
    pub level: String,
    /// Physical locations associated with this result.
    pub locations: Vec<SarifLocation>,
}

/// A physical location reference within a SARIF result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifLocation {
    /// Physical file and region information.
    pub physical_location: SarifPhysicalLocation,
}

/// Physical location within an artifact (file).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifPhysicalLocation {
    /// Reference to the artifact (file).
    pub artifact_location: SarifArtifactLocation,
    /// Optional region (line/column range) within the artifact.
    pub region: Option<SarifRegion>,
}

/// A reference to a file artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifArtifactLocation {
    /// URI of the artifact, typically a relative or absolute file path.
    pub uri: String,
}

/// A region within a file artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRegion {
    /// 1-based start line number.
    pub start_line: u32,
}

/// A human-readable text message used in multiple SARIF contexts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifMessage {
    /// The message text.
    pub text: String,
}

// ---------------------------------------------------------------------------
// Severity -> SARIF level mapping
// ---------------------------------------------------------------------------

/// Maps a [`FindingSeverity`] to the corresponding SARIF result level string.
///
/// | Severity            | SARIF level  |
/// |---------------------|--------------|
/// | `Critical` / `High` | `"error"`    |
/// | `Medium`            | `"warning"`  |
/// | `Low` / `Info`      | `"note"`     |
fn severity_to_sarif_level(severity: FindingSeverity) -> &'static str {
    match severity {
        FindingSeverity::Critical | FindingSeverity::High => "error",
        FindingSeverity::Medium => "warning",
        FindingSeverity::Low | FindingSeverity::Info => "note",
    }
}

// ---------------------------------------------------------------------------
// SarifReportWriter
// ---------------------------------------------------------------------------

/// Writes a [`ReportEnvelope`] to disk as a SARIF 2.1.0 document.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use xzardgz::reports::envelope::ReportEnvelope;
/// use xzardgz::reports::formatter::PluginReportFormatter;
/// use xzardgz::reports::sarif::SarifReportWriter;
///
/// let writer = SarifReportWriter;
/// let envelope = ReportEnvelope::new("r-001", "my_plugin", "ws-abc");
/// writer.write(&envelope, Path::new("/tmp/out/report.sarif.json")).expect("write failed");
/// ```
pub struct SarifReportWriter;

impl PluginReportFormatter for SarifReportWriter {
    /// Returns `"sarif"`.
    fn format_name(&self) -> &str {
        "sarif"
    }

    /// Converts `envelope` findings to SARIF results and writes to `path`.
    ///
    /// The conversion rules are:
    /// - `Critical` / `High` severity => level `"error"`
    /// - `Medium` severity => level `"warning"`
    /// - `Low` / `Info` severity => level `"note"`
    ///
    /// Rules are deduplicated by `kind` (the first title seen for a given
    /// `kind` is used as the rule short description). Parent directories are
    /// created automatically.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::PipelineError::Report`] if the path is invalid
    /// or JSON serialization fails, or [`crate::error::PipelineError::Io`] for
    /// I/O errors.
    fn write(&self, envelope: &ReportEnvelope, path: &Path) -> Result<()> {
        validate_report_path(path)?;

        // Build deduplicated rules map (kind -> SarifRule, first-seen wins).
        let mut rules_map: HashMap<String, SarifRule> = HashMap::new();
        let mut results: Vec<SarifResult> = Vec::new();

        for finding in &envelope.findings.findings {
            // Register rule on first encounter.
            rules_map
                .entry(finding.kind.clone())
                .or_insert_with(|| SarifRule {
                    id: finding.kind.clone(),
                    short_description: SarifMessage {
                        text: finding.title.clone(),
                    },
                });

            let level = severity_to_sarif_level(finding.severity).to_string();

            let locations = if let Some(ref file_path) = finding.file_path {
                vec![SarifLocation {
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
                rule_id: finding.kind.clone(),
                message: SarifMessage {
                    text: finding.description.clone(),
                },
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
                        name: envelope.plugin_name.clone(),
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
    use crate::reports::findings::PluginFinding;
    use crate::reports::formatter::PluginReportFormatter;
    use crate::scanner::findings::FindingSeverity;

    fn make_envelope() -> ReportEnvelope {
        let mut env = ReportEnvelope::new("sarif-test-001", "sarif_plugin", "ws-sarif");
        env.findings.push(
            PluginFinding::new(
                "sql_injection",
                "SQL Injection",
                "Unsanitized SQL query input.",
                FindingSeverity::High,
                0.88,
            )
            .with_location("src/db.rs", Some(42)),
        );
        env.findings.push(PluginFinding::new(
            "xss",
            "Cross-Site Scripting",
            "Unescaped user output.",
            FindingSeverity::Medium,
            0.65,
        ));
        env.findings.push(
            PluginFinding::new(
                "info_leak",
                "Information Leak",
                "Verbose error messages.",
                FindingSeverity::Low,
                0.2,
            )
            .with_location("src/api.rs", None),
        );
        env
    }

    // ------------------------------------------------------------------
    // format_name
    // ------------------------------------------------------------------

    #[test]
    fn test_sarif_report_writer_format_name_returns_sarif() {
        let writer = SarifReportWriter;
        assert_eq!(writer.format_name(), "sarif");
    }

    // ------------------------------------------------------------------
    // severity_to_sarif_level
    // ------------------------------------------------------------------

    #[test]
    fn test_severity_to_sarif_level_critical_returns_error() {
        assert_eq!(severity_to_sarif_level(FindingSeverity::Critical), "error");
    }

    #[test]
    fn test_severity_to_sarif_level_high_returns_error() {
        assert_eq!(severity_to_sarif_level(FindingSeverity::High), "error");
    }

    #[test]
    fn test_severity_to_sarif_level_medium_returns_warning() {
        assert_eq!(severity_to_sarif_level(FindingSeverity::Medium), "warning");
    }

    #[test]
    fn test_severity_to_sarif_level_low_returns_note() {
        assert_eq!(severity_to_sarif_level(FindingSeverity::Low), "note");
    }

    #[test]
    fn test_severity_to_sarif_level_info_returns_note() {
        assert_eq!(severity_to_sarif_level(FindingSeverity::Info), "note");
    }

    // ------------------------------------------------------------------
    // write
    // ------------------------------------------------------------------

    #[test]
    fn test_sarif_report_writer_write_creates_file() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("report.sarif.json");

        let writer = SarifReportWriter;
        let env = make_envelope();
        // SAFETY: writing to a freshly created temp directory should not fail.
        writer.write(&env, &path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_sarif_report_writer_write_produces_valid_json() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("report.sarif.json");

        let writer = SarifReportWriter;
        let env = make_envelope();
        writer.write(&env, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: we just wrote this file so it is valid JSON.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["version"], "2.1.0");
        assert!(parsed["runs"].is_array());
    }

    #[test]
    fn test_sarif_report_writer_write_tool_driver_name_matches_plugin() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("report.sarif.json");

        let writer = SarifReportWriter;
        let env = make_envelope();
        writer.write(&env, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: we serialized this file ourselves.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["runs"][0]["tool"]["driver"]["name"], "sarif_plugin");
    }

    #[test]
    fn test_sarif_report_writer_write_results_count_matches_findings() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("report.sarif.json");

        let writer = SarifReportWriter;
        let env = make_envelope();
        writer.write(&env, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: we serialized this file ourselves.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let results = parsed["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 3, "expected 3 results matching 3 findings");
    }

    #[test]
    fn test_sarif_report_writer_write_severity_mapped_to_correct_level() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("levels.sarif.json");

        let writer = SarifReportWriter;
        let env = make_envelope();
        writer.write(&env, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: we serialized this file ourselves.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let results = parsed["runs"][0]["results"].as_array().unwrap();

        // sql_injection is High -> "error"
        let sql_result = results
            .iter()
            .find(|r| r["ruleId"] == "sql_injection")
            .expect("sql_injection result not found");
        assert_eq!(sql_result["level"], "error");

        // xss is Medium -> "warning"
        let xss_result = results
            .iter()
            .find(|r| r["ruleId"] == "xss")
            .expect("xss result not found");
        assert_eq!(xss_result["level"], "warning");

        // info_leak is Low -> "note"
        let info_result = results
            .iter()
            .find(|r| r["ruleId"] == "info_leak")
            .expect("info_leak result not found");
        assert_eq!(info_result["level"], "note");
    }

    #[test]
    fn test_sarif_report_writer_write_location_includes_file_and_line() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("locations.sarif.json");

        let writer = SarifReportWriter;
        let env = make_envelope();
        writer.write(&env, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: we serialized this file ourselves.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let results = parsed["runs"][0]["results"].as_array().unwrap();

        let sql_result = results
            .iter()
            .find(|r| r["ruleId"] == "sql_injection")
            .unwrap();
        let uri = &sql_result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"];
        assert_eq!(uri, "src/db.rs");
        let start_line = &sql_result["locations"][0]["physicalLocation"]["region"]["startLine"];
        assert_eq!(start_line, 42);
    }

    #[test]
    fn test_sarif_report_writer_write_rules_are_deduplicated() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("dedup.sarif.json");

        let mut env = ReportEnvelope::new("r", "p", "ws");
        // Same kind "xss" appears twice.
        env.findings.push(PluginFinding::new(
            "xss",
            "XSS",
            "desc1",
            FindingSeverity::High,
            0.8,
        ));
        env.findings.push(PluginFinding::new(
            "xss",
            "XSS variant",
            "desc2",
            FindingSeverity::Medium,
            0.5,
        ));

        let writer = SarifReportWriter;
        writer.write(&env, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: we serialized this file ourselves.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let rules = parsed["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .unwrap();
        // "xss" should appear only once.
        assert_eq!(rules.len(), 1, "expected exactly 1 deduplicated rule");
        assert_eq!(rules[0]["id"], "xss");
    }

    #[test]
    fn test_sarif_report_writer_write_empty_findings_produces_empty_results() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("empty.sarif.json");

        let env = ReportEnvelope::new("r", "plugin", "ws");
        let writer = SarifReportWriter;
        writer.write(&env, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: we serialized this file ourselves.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let results = parsed["runs"][0]["results"].as_array().unwrap();
        assert!(
            results.is_empty(),
            "empty findings should produce no results"
        );
    }

    #[test]
    fn test_sarif_report_writer_write_creates_parent_directories() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp
            .path()
            .join("nested")
            .join("out")
            .join("report.sarif.json");

        let writer = SarifReportWriter;
        let env = make_envelope();
        writer.write(&env, &path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_sarif_report_writer_write_invalid_path_returns_error() {
        let writer = SarifReportWriter;
        let env = make_envelope();
        let result = writer.write(&env, Path::new("/"));
        assert!(result.is_err());
    }
}
