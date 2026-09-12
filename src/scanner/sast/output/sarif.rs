//! SARIF 2.1.0 projection for the SAST scanner output.
//!
//! This module converts a [`SastScanReport`] into a SARIF 2.1.0 log document
//! that can be consumed by GitHub Advanced Security, VS Code SARIF Viewer, and
//! any other SARIF-aware tool.
//!
//! # Usage
//!
//! ```no_run
//! use xzardgz::scanner::sast::output::sarif::render_sarif;
//! use xzardgz::scanner::sast::SastScanReport;
//!
//! # fn make_report() -> SastScanReport { unimplemented!() }
//! let report = make_report();
//! let log = render_sarif(&report);
//! let json = serde_json::to_string_pretty(&log).unwrap();
//! ```
//!
//! # SARIF column numbering
//!
//! SARIF regions use 1-based column numbers. The [`Position`] type in
//! [`match_model`] stores 0-based byte columns, so this module adds 1 when
//! populating [`SarifRegion::start_column`] and [`SarifRegion::end_column`].
//!
//! [`match_model`]: crate::scanner::sast::match_model
//! [`Position`]: crate::scanner::sast::match_model::Position

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::scanner::sast::SastScanReport;
use crate::scanner::sast::match_model::SastMatch;
use crate::scanner::sast::rule::metadata::Severity;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// SARIF format version emitted by this module.
const SARIF_VERSION: &str = "2.1.0";

/// JSON Schema URI for SARIF 2.1.0.
const SARIF_SCHEMA: &str =
    "https://docs.oasis-open.org/sarif/sarif/v2.1.0/cos01/schemas/sarif-schema-2.1.0.json";

/// Driver name used in every SARIF log produced by this crate.
const TOOL_NAME: &str = "xzardgz-sast";

/// Driver version reported in every SARIF log.
const TOOL_VERSION: &str = "0.1.0";

/// Informational URI for the tool.
const TOOL_INFO_URI: &str = "https://github.com/xbcsmith/xzardgz";

/// Key used in the SARIF `fingerprints` map for the xzardgz fingerprint.
const FINGERPRINT_KEY: &str = "xzardgz/v1";

/// SARIF `uriBaseId` used for all artifact locations to indicate repo-relative paths.
const URI_BASE_ID: &str = "%SRCROOT%";

// ---------------------------------------------------------------------------
// SARIF 2.1.0 type hierarchy
// ---------------------------------------------------------------------------

/// Top-level SARIF 2.1.0 log document.
///
/// Serialises to the root JSON object with `"version"` and `"$schema"` keys
/// alongside the `"runs"` array.
#[derive(Debug, Clone, Serialize)]
pub struct SarifLog {
    /// SARIF format version. Always `"2.1.0"`.
    pub version: &'static str,
    /// JSON Schema URI for this document.
    #[serde(rename = "$schema")]
    pub schema: &'static str,
    /// One or more scan runs contained in this log.
    pub runs: Vec<SarifRun>,
}

/// A single tool execution recorded in a [`SarifLog`].
#[derive(Debug, Clone, Serialize)]
pub struct SarifRun {
    /// Metadata about the tool that produced the results.
    pub tool: SarifTool,
    /// Results (matches) reported in this run.
    pub results: Vec<SarifResult>,
}

/// Tool descriptor for a [`SarifRun`].
#[derive(Debug, Clone, Serialize)]
pub struct SarifTool {
    /// The primary driver component of the tool.
    pub driver: SarifDriver,
}

/// Primary driver component descriptor.
///
/// The driver lists the rules that were evaluated and provides the
/// tool's identity information.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifDriver {
    /// Human-readable tool name.
    pub name: String,
    /// Tool version string.
    pub version: String,
    /// URI to the tool's home page or documentation.
    pub information_uri: String,
    /// Rules (deduplicated by `id`) that produced results in this run.
    pub rules: Vec<SarifRule>,
}

/// A rule descriptor included in [`SarifDriver::rules`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRule {
    /// Stable rule identifier (e.g. `"rust-weak-rsa-key"`).
    pub id: String,
    /// Readable rule name; equal to `id` in Phase 5.
    pub name: String,
    /// One-sentence description of what the rule detects.
    pub short_description: SarifMessage,
    /// Full message template for the rule.
    pub help: SarifMessage,
    /// Arbitrary metadata tags (CWE, OWASP, `"security"`, etc.).
    pub properties: SarifRuleProperties,
}

/// Bag of metadata properties for a [`SarifRule`].
#[derive(Debug, Clone, Serialize)]
pub struct SarifRuleProperties {
    /// Classification tags such as `"CWE-327"` or `"security"`.
    pub tags: Vec<String>,
}

/// A single match result reported in a [`SarifRun`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifResult {
    /// Identifier of the rule that produced this result.
    pub rule_id: String,
    /// Human-readable result message.
    pub message: SarifMessage,
    /// SARIF severity level (`"note"`, `"warning"`, or `"error"`).
    pub level: String,
    /// Source locations where the issue was detected.
    pub locations: Vec<SarifLocation>,
    /// Stable fingerprints for deduplication, keyed by namespace string.
    pub fingerprints: BTreeMap<String, String>,
}

/// A simple localised text message used in several SARIF objects.
#[derive(Debug, Clone, Serialize)]
pub struct SarifMessage {
    /// The message text.
    pub text: String,
}

/// A source location associated with a [`SarifResult`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifLocation {
    /// Physical location of the artifact and region.
    pub physical_location: SarifPhysicalLocation,
}

/// A file artifact together with the specific region within it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifPhysicalLocation {
    /// URI and base-ID of the file.
    pub artifact_location: SarifArtifactLocation,
    /// Start and end position of the finding within the file.
    pub region: SarifRegion,
}

/// A file URI with an optional base-ID for root-relative paths.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifArtifactLocation {
    /// Repo-relative path to the file.
    pub uri: String,
    /// Token that tooling replaces with the scan root (`"%SRCROOT%"`).
    pub uri_base_id: String,
}

/// A character region within a file.
///
/// All line and column numbers are **1-based** as required by the SARIF spec.
/// The [`Position`] type stores 0-based columns, so this module adds 1 during
/// projection.
///
/// [`Position`]: crate::scanner::sast::match_model::Position
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRegion {
    /// 1-based line number of the start of the region.
    pub start_line: u32,
    /// 1-based column number of the start of the region.
    pub start_column: u32,
    /// 1-based line number of the end of the region.
    pub end_line: u32,
    /// 1-based column number of the end of the region (exclusive).
    pub end_column: u32,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Project a [`SastScanReport`] to a SARIF 2.1.0 [`SarifLog`].
///
/// The returned value can be serialised directly with `serde_json`:
///
/// ```no_run
/// use xzardgz::scanner::sast::output::sarif::render_sarif;
/// use xzardgz::scanner::sast::SastScanReport;
///
/// # fn make_report() -> SastScanReport { unimplemented!() }
/// let report = make_report();
/// let log = render_sarif(&report);
/// let json = serde_json::to_string_pretty(&log).unwrap();
/// ```
///
/// # Arguments
///
/// * `report` - Reference to the completed scan report.
///
/// # Returns
///
/// A [`SarifLog`] with exactly one [`SarifRun`]. The run's `tool.driver.rules`
/// list contains one entry per distinct `rule_id` (deduplicated). The
/// `results` list contains one entry per match in `report.matches`.
pub fn render_sarif(report: &SastScanReport) -> SarifLog {
    let rules = build_driver_rules(report);
    let results = report
        .matches
        .iter()
        .map(match_to_result)
        .collect::<Vec<_>>();

    SarifLog {
        version: SARIF_VERSION,
        schema: SARIF_SCHEMA,
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: TOOL_NAME.to_string(),
                    version: TOOL_VERSION.to_string(),
                    information_uri: TOOL_INFO_URI.to_string(),
                    rules,
                },
            },
            results,
        }],
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Map a [`Severity`] to the corresponding SARIF `level` string.
///
/// | Severity        | SARIF level  |
/// |-----------------|--------------|
/// | `Info`          | `"note"`     |
/// | `Warning`       | `"warning"`  |
/// | `Error`         | `"error"`    |
fn severity_to_level(severity: &Severity) -> String {
    match severity {
        Severity::Info => "note".to_string(),
        Severity::Warning => "warning".to_string(),
        Severity::Error => "error".to_string(),
    }
}

/// Build the deduplicated list of [`SarifRule`] entries for the driver.
///
/// Rules are emitted in first-seen order (determined by the sort order of
/// `report.matches`). Only the first occurrence of each `rule_id` contributes
/// a rule entry.
fn build_driver_rules(report: &SastScanReport) -> Vec<SarifRule> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut rules = Vec::new();

    for m in &report.matches {
        if seen.insert(m.rule_id.as_str()) {
            rules.push(match_to_driver_rule(m));
        }
    }

    rules
}

/// Build a [`SarifRule`] descriptor from the first match for that rule.
///
/// The `short_description` is taken from `metadata.description` when present;
/// otherwise the match `message` is used. The `help` text is always the match
/// `message`.
fn match_to_driver_rule(m: &SastMatch) -> SarifRule {
    let short_desc = m
        .metadata
        .description
        .as_deref()
        .unwrap_or(&m.message)
        .to_string();

    let tags = build_tags(m);

    SarifRule {
        id: m.rule_id.clone(),
        name: m.rule_id.clone(),
        short_description: SarifMessage { text: short_desc },
        help: SarifMessage {
            text: m.message.clone(),
        },
        properties: SarifRuleProperties { tags },
    }
}

/// Build the `properties.tags` list for a rule.
///
/// The tags are assembled in this order:
/// 1. Each value from `metadata.cwe` (e.g. `"CWE-326"`).
/// 2. The literal `"security"` tag, added whenever at least one CWE is present.
/// 3. Each value from `metadata.owasp` (e.g. `"A02:2021"`).
fn build_tags(m: &SastMatch) -> Vec<String> {
    let mut tags = Vec::new();

    if let Some(cwes) = &m.metadata.cwe {
        tags.extend(cwes.iter().cloned());
        tags.push("security".to_string());
    }

    if let Some(owasps) = &m.metadata.owasp {
        tags.extend(owasps.iter().cloned());
    }

    tags
}

/// Project a single [`SastMatch`] to a [`SarifResult`].
///
/// SARIF columns are 1-based; this function converts the 0-based
/// [`Position::col`] values by adding 1.
///
/// [`Position::col`]: crate::scanner::sast::match_model::Position::col
fn match_to_result(m: &SastMatch) -> SarifResult {
    let uri = m.path.to_string_lossy().into_owned();

    // SARIF uses 1-based columns; Position.col is 0-based.
    let start_column = m.start.col + 1;
    let end_column = m.end.col + 1;

    let mut fingerprints = BTreeMap::new();
    fingerprints.insert(FINGERPRINT_KEY.to_string(), m.fingerprint.clone());

    SarifResult {
        rule_id: m.rule_id.clone(),
        message: SarifMessage {
            text: m.message.clone(),
        },
        level: severity_to_level(&m.severity),
        locations: vec![SarifLocation {
            physical_location: SarifPhysicalLocation {
                artifact_location: SarifArtifactLocation {
                    uri,
                    uri_base_id: URI_BASE_ID.to_string(),
                },
                region: SarifRegion {
                    start_line: m.start.line,
                    start_column,
                    end_line: m.end.line,
                    end_column,
                },
            },
        }],
        fingerprints,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;
    use crate::scanner::sast::match_model::{MatchSnippet, Position, SastMatch};
    use crate::scanner::sast::rule::metadata::{Category, Confidence, RuleMetadata, Severity};

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_report_with_matches(matches: Vec<SastMatch>) -> SastScanReport {
        SastScanReport {
            matches,
            skipped_rules: Vec::new(),
            scanned_file_count: 1,
            skipped_file_count: 0,
            parse_error_count: 0,
            truncated_rule_file_pairs: 0,
            duration_ms: 0,
        }
    }

    fn make_test_match() -> SastMatch {
        SastMatch {
            rule_id: "rust-weak-rsa-key".to_string(),
            ruleset_id: String::new(),
            message: "RSA keys must be at least 2048 bits.".to_string(),
            severity: Severity::Warning,
            confidence: Confidence::High,
            path: PathBuf::from("src/crypto.rs"),
            start: Position {
                line: 10,
                col: 4,
                byte: 200,
            },
            end: Position {
                line: 10,
                col: 44,
                byte: 240,
            },
            snippet: MatchSnippet {
                text: "RsaPrivateKey::new(&mut rng, 1024)".to_string(),
                context_before: vec!["fn generate_key() {".to_string()],
                context_after: vec!["}".to_string()],
            },
            metavariables: BTreeMap::new(),
            metadata: RuleMetadata {
                description: None,
                category: Some(Category::Security),
                confidence: Some(Confidence::High),
                technology: None,
                cwe: Some(vec!["CWE-326".to_string()]),
                owasp: Some(vec!["A02:2021".to_string()]),
                references: None,
                license: Some("Apache-2.0".to_string()),
            },
            fingerprint: "deadbeef_0".to_string(),
            fix: Some("RsaPrivateKey::new(&mut rng, 2048)".to_string()),
        }
    }

    fn make_empty_report() -> SastScanReport {
        SastScanReport {
            matches: Vec::new(),
            skipped_rules: Vec::new(),
            scanned_file_count: 0,
            skipped_file_count: 0,
            parse_error_count: 0,
            truncated_rule_file_pairs: 0,
            duration_ms: 0,
        }
    }

    fn load_schema() -> serde_json::Value {
        let schema_str = include_str!("../../../../testdata/sarif/sarif-2.1.0.schema.json");
        // SAFETY: the schema file is a known-good static asset embedded at compile time
        serde_json::from_str(schema_str).expect("SARIF schema must be valid JSON")
    }

    // -----------------------------------------------------------------------
    // Schema validation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_render_sarif_with_one_match_produces_schema_valid_output() {
        let report = make_report_with_matches(vec![make_test_match()]);
        let log = render_sarif(&report);
        let json_str =
            serde_json::to_string_pretty(&log).expect("SARIF must serialise without error");

        let schema_json = load_schema();
        // SAFETY: schema is a known-good static asset
        let validator = jsonschema::validator_for(&schema_json).expect("SARIF schema must compile");
        let sarif_value: serde_json::Value =
            serde_json::from_str(&json_str).expect("rendered SARIF must parse as JSON");

        assert!(
            validator.is_valid(&sarif_value),
            "rendered SARIF must be schema-valid; JSON was:\n{json_str}"
        );
    }

    #[test]
    fn test_render_sarif_with_empty_report_produces_schema_valid_output() {
        let report = make_empty_report();
        let log = render_sarif(&report);
        let json_str =
            serde_json::to_string_pretty(&log).expect("SARIF must serialise without error");

        let schema_json = load_schema();
        // SAFETY: schema is a known-good static asset
        let validator = jsonschema::validator_for(&schema_json).expect("SARIF schema must compile");
        let sarif_value: serde_json::Value =
            serde_json::from_str(&json_str).expect("rendered SARIF must parse as JSON");

        assert!(
            validator.is_valid(&sarif_value),
            "empty SARIF report must be schema-valid; JSON was:\n{json_str}"
        );
    }

    // -----------------------------------------------------------------------
    // Empty report structure tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_render_sarif_with_empty_report_has_empty_results_array() {
        let report = make_empty_report();
        let log = render_sarif(&report);
        assert_eq!(log.runs.len(), 1, "must have exactly one run");
        assert!(
            log.runs[0].results.is_empty(),
            "empty report must produce no results"
        );
    }

    #[test]
    fn test_render_sarif_with_empty_report_has_empty_rules_array() {
        let report = make_empty_report();
        let log = render_sarif(&report);
        assert!(
            log.runs[0].tool.driver.rules.is_empty(),
            "empty report must produce no driver rules"
        );
    }

    // -----------------------------------------------------------------------
    // Severity mapping tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_severity_to_level_info_maps_to_note() {
        assert_eq!(severity_to_level(&Severity::Info), "note");
    }

    #[test]
    fn test_severity_to_level_warning_maps_to_warning() {
        assert_eq!(severity_to_level(&Severity::Warning), "warning");
    }

    #[test]
    fn test_severity_to_level_error_maps_to_error() {
        assert_eq!(severity_to_level(&Severity::Error), "error");
    }

    #[test]
    fn test_render_sarif_result_level_reflects_match_severity() {
        let mut m = make_test_match();
        m.severity = Severity::Error;
        let report = make_report_with_matches(vec![m]);
        let log = render_sarif(&report);
        assert_eq!(log.runs[0].results[0].level, "error");
    }

    // -----------------------------------------------------------------------
    // Tags tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_tags_with_cwe_includes_cwe_value_and_security() {
        let m = make_test_match();
        let tags = build_tags(&m);
        assert!(
            tags.contains(&"CWE-326".to_string()),
            "tags must include CWE value from metadata"
        );
        assert!(
            tags.contains(&"security".to_string()),
            "tags must include 'security' when CWE is present"
        );
    }

    #[test]
    fn test_build_tags_with_owasp_includes_owasp_value() {
        let m = make_test_match();
        let tags = build_tags(&m);
        assert!(
            tags.contains(&"A02:2021".to_string()),
            "tags must include OWASP value from metadata"
        );
    }

    #[test]
    fn test_build_tags_without_cwe_does_not_include_security() {
        let mut m = make_test_match();
        m.metadata.cwe = None;
        let tags = build_tags(&m);
        assert!(
            !tags.contains(&"security".to_string()),
            "'security' tag must not appear when no CWE is present"
        );
    }

    #[test]
    fn test_build_tags_without_cwe_and_owasp_returns_empty_vec() {
        let mut m = make_test_match();
        m.metadata.cwe = None;
        m.metadata.owasp = None;
        let tags = build_tags(&m);
        assert!(
            tags.is_empty(),
            "tags must be empty when no CWE or OWASP is set"
        );
    }

    // -----------------------------------------------------------------------
    // Fingerprint tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_render_sarif_result_fingerprints_contains_xzardgz_v1_key() {
        let report = make_report_with_matches(vec![make_test_match()]);
        let log = render_sarif(&report);
        let fps = &log.runs[0].results[0].fingerprints;
        assert!(
            fps.contains_key("xzardgz/v1"),
            "fingerprints must contain the 'xzardgz/v1' key"
        );
    }

    #[test]
    fn test_render_sarif_result_fingerprints_value_matches_match_fingerprint() {
        let m = make_test_match();
        let expected_fp = m.fingerprint.clone();
        let report = make_report_with_matches(vec![m]);
        let log = render_sarif(&report);
        let actual_fp = log.runs[0].results[0]
            .fingerprints
            .get("xzardgz/v1")
            // SAFETY: key existence is verified in the previous test
            .expect("xzardgz/v1 key must be present");
        assert_eq!(actual_fp, &expected_fp);
    }

    // -----------------------------------------------------------------------
    // Column offset tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_render_sarif_region_columns_are_one_based() {
        let report = make_report_with_matches(vec![make_test_match()]);
        let log = render_sarif(&report);
        let region = &log.runs[0].results[0].locations[0].physical_location.region;
        // Position.col is 0-based (4 and 44); SARIF must be 1-based (5 and 45).
        assert_eq!(region.start_column, 5, "startColumn must be col + 1 = 5");
        assert_eq!(region.end_column, 45, "endColumn must be col + 1 = 45");
    }

    // -----------------------------------------------------------------------
    // Rule deduplication tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_render_sarif_driver_deduplicates_rules_by_rule_id() {
        let m1 = make_test_match();
        let mut m2 = make_test_match();
        m2.message = "Another RSA warning.".to_string();

        let report = make_report_with_matches(vec![m1, m2]);
        let log = render_sarif(&report);
        assert_eq!(
            log.runs[0].tool.driver.rules.len(),
            1,
            "two matches with the same rule_id must produce exactly one driver rule"
        );
    }

    #[test]
    fn test_render_sarif_driver_one_rule_per_distinct_rule_id() {
        let mut m2 = make_test_match();
        m2.rule_id = "rust-md5-usage".to_string();
        m2.metadata.cwe = None;
        m2.metadata.owasp = None;

        let report = make_report_with_matches(vec![make_test_match(), m2]);
        let log = render_sarif(&report);
        assert_eq!(
            log.runs[0].tool.driver.rules.len(),
            2,
            "two distinct rule_ids must produce two driver rules"
        );
    }

    // -----------------------------------------------------------------------
    // Tool metadata tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_render_sarif_driver_name_is_xzardgz_sast() {
        let report = make_empty_report();
        let log = render_sarif(&report);
        assert_eq!(log.runs[0].tool.driver.name, "xzardgz-sast");
    }

    #[test]
    fn test_render_sarif_version_field_is_2_1_0() {
        let report = make_empty_report();
        let log = render_sarif(&report);
        assert_eq!(log.version, "2.1.0");
    }

    #[test]
    fn test_render_sarif_schema_field_contains_oasis_uri() {
        let report = make_empty_report();
        let log = render_sarif(&report);
        assert!(
            log.schema.contains("oasis-open.org"),
            "schema field must reference the OASIS URI"
        );
    }

    // -----------------------------------------------------------------------
    // Golden file tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_render_sarif_one_match_matches_golden_file() {
        let report = make_report_with_matches(vec![make_test_match()]);
        let log = render_sarif(&report);
        let actual_json =
            serde_json::to_string_pretty(&log).expect("SARIF must serialise without error");

        let golden_str = include_str!("../../../../testdata/sast/golden/sarif_one_match.json");

        let actual_value: serde_json::Value =
            serde_json::from_str(&actual_json).expect("actual SARIF must parse as JSON");
        let expected_value: serde_json::Value =
            serde_json::from_str(golden_str).expect("golden file must parse as JSON");

        assert_eq!(
            actual_value, expected_value,
            "rendered SARIF must match the golden file at testdata/sast/golden/sarif_one_match.json"
        );
    }

    #[test]
    fn test_render_sarif_empty_report_matches_golden_file() {
        let report = make_empty_report();
        let log = render_sarif(&report);
        let actual_json =
            serde_json::to_string_pretty(&log).expect("SARIF must serialise without error");

        let golden_str = include_str!("../../../../testdata/sast/golden/sarif_empty.json");

        let actual_value: serde_json::Value =
            serde_json::from_str(&actual_json).expect("actual SARIF must parse as JSON");
        let expected_value: serde_json::Value =
            serde_json::from_str(golden_str).expect("golden file must parse as JSON");

        assert_eq!(
            actual_value, expected_value,
            "empty SARIF report must match the golden file at testdata/sast/golden/sarif_empty.json"
        );
    }
}
