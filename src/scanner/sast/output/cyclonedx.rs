//! CycloneDX 1.7 output projection for the SAST scanner.
//!
//! This module converts a [`SastScanReport`] into a CycloneDX 1.7 BOM with a
//! `vulnerabilities` array. Each SAST match becomes one
//! [`CycloneDxVulnerability`] entry.
//!
//! # Usage
//!
//! ```no_run
//! use xzardgz::scanner::sast::output::cyclonedx::render_cyclonedx;
//! use xzardgz::scanner::sast::SastScanReport;
//!
//! # fn example(report: &SastScanReport) {
//! let bom = render_cyclonedx(report);
//! let json = serde_json::to_string_pretty(&bom).expect("serialisation must not fail");
//! println!("{json}");
//! # }
//! ```
//!
//! [`SastScanReport`]: crate::scanner::sast::SastScanReport

use serde::{Deserialize, Serialize};

use crate::scanner::sast::SastScanReport;
use crate::scanner::sast::match_model::SastMatch;
use crate::scanner::sast::rule::metadata::Severity;

// ---------------------------------------------------------------------------
// CycloneDX BOM root
// ---------------------------------------------------------------------------

/// Top-level CycloneDX 1.7 Bill of Materials document.
///
/// Contains only the fields needed to represent a SAST vulnerability report.
/// All other CycloneDX top-level arrays (components, services, etc.) are
/// omitted since the SAST scanner does not populate them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CycloneDxBom {
    /// CycloneDX format identifier. Always `"CycloneDX"`.
    pub bom_format: String,
    /// Specification version. Always `"1.7"`.
    pub spec_version: String,
    /// BOM version number. Starts at `1`.
    pub version: u32,
    /// One entry per SAST match found during the scan.
    pub vulnerabilities: Vec<CycloneDxVulnerability>,
}

// ---------------------------------------------------------------------------
// Vulnerability
// ---------------------------------------------------------------------------

/// A single vulnerability entry in the CycloneDX BOM.
///
/// Maps one-to-one to a [`SastMatch`] from the SAST engine.
///
/// [`SastMatch`]: crate::scanner::sast::match_model::SastMatch
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CycloneDxVulnerability {
    /// BOM reference (same value as `id`; used for cross-BOM linking).
    #[serde(rename = "bom-ref")]
    pub bom_ref: String,
    /// Unique vulnerability identifier of the form `"sast:<rule_id>:<fp8>"`.
    pub id: String,
    /// The scanning tool that produced this finding.
    pub source: CycloneDxSource,
    /// Severity ratings for this finding.
    pub ratings: Vec<CycloneDxRating>,
    /// Numeric CWE identifiers extracted from the rule metadata.
    pub cwes: Vec<u32>,
    /// Human-readable description (the rule's match message).
    pub description: String,
    /// Files affected by this finding.
    pub affects: Vec<CycloneDxAffects>,
    /// Current triage state and freeform detail.
    pub analysis: CycloneDxAnalysis,
    /// Additional xzardgz-specific properties.
    pub properties: Vec<CycloneDxProperty>,
}

// ---------------------------------------------------------------------------
// Source
// ---------------------------------------------------------------------------

/// The vulnerability information source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycloneDxSource {
    /// Display name of the source tool.
    pub name: String,
    /// Optional URL to the source documentation or advisory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

// ---------------------------------------------------------------------------
// Rating
// ---------------------------------------------------------------------------

/// A single severity rating for a vulnerability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycloneDxRating {
    /// The source that produced this rating.
    pub source: CycloneDxSource,
    /// CycloneDX severity string (`"critical"`, `"high"`, `"medium"`, `"low"`,
    /// `"info"`, `"none"`, `"unknown"`).
    pub severity: String,
    /// Scoring methodology. `"other"` for SAST severity levels.
    pub method: String,
}

// ---------------------------------------------------------------------------
// Affects
// ---------------------------------------------------------------------------

/// An affected file reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycloneDxAffects {
    /// Repo-relative path of the affected file.
    ///
    /// The field is serialised as `"ref"` because `ref` is a Rust keyword.
    #[serde(rename = "ref")]
    pub ref_field: String,
}

// ---------------------------------------------------------------------------
// Analysis
// ---------------------------------------------------------------------------

/// Impact analysis state and freeform detail for a finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycloneDxAnalysis {
    /// Triage state. Fixed at `"in_triage"` for all SAST findings.
    pub state: String,
    /// Freeform detail, populated with the match message.
    pub detail: String,
}

// ---------------------------------------------------------------------------
// Property
// ---------------------------------------------------------------------------

/// A lightweight name-value property attached to a vulnerability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycloneDxProperty {
    /// Property name (e.g. `"xzardgz:sast:rule_id"`).
    pub name: String,
    /// Property value.
    pub value: String,
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Convert a SAST [`Severity`] to the CycloneDX severity string.
///
/// # Arguments
///
/// * `severity` - The SAST severity level to convert.
///
/// # Returns
///
/// A static string that matches a valid CycloneDX severity enum value.
fn severity_to_cyclonedx(severity: &Severity) -> &'static str {
    match severity {
        Severity::Error => "high",
        Severity::Warning => "medium",
        Severity::Info => "info",
    }
}

/// Extract numeric CWE IDs from a slice of `"CWE-NNN"` strings.
///
/// Entries that do not match the pattern `"CWE-"` followed by one or more
/// ASCII digits are silently ignored.
///
/// # Arguments
///
/// * `cwe_strs` - Slice of CWE identifier strings such as `["CWE-327"]`.
///
/// # Returns
///
/// A `Vec<u32>` of parsed CWE numbers in input order.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::output::cyclonedx::render_cyclonedx;
/// ```
fn extract_cwes(cwe_strs: &[String]) -> Vec<u32> {
    cwe_strs
        .iter()
        .filter_map(|s| {
            let digits = s.strip_prefix("CWE-")?;
            digits.parse::<u32>().ok()
        })
        .collect()
}

/// Build the vulnerability `id` (and `bom_ref`) for a single SAST match.
///
/// The format is `"sast:<rule_id>:<first_8_fingerprint_chars>"`.
fn vuln_id(m: &SastMatch) -> String {
    let fp_prefix = &m.fingerprint[..8_usize.min(m.fingerprint.len())];
    format!("sast:{}:{}", m.rule_id, fp_prefix)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Render a [`SastScanReport`] as a CycloneDX 1.7 BOM.
///
/// Each match in `report.matches` is projected to a
/// [`CycloneDxVulnerability`]. The returned [`CycloneDxBom`] can be
/// serialised directly with `serde_json::to_string_pretty`.
///
/// # Arguments
///
/// * `report` - The scan report produced by [`SastEngine::scan`].
///
/// # Returns
///
/// A [`CycloneDxBom`] ready for serialisation.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::scanner::sast::output::cyclonedx::render_cyclonedx;
/// use xzardgz::scanner::sast::SastScanReport;
///
/// # fn example(report: &SastScanReport) {
/// let bom = render_cyclonedx(report);
/// assert_eq!(bom.bom_format, "CycloneDX");
/// assert_eq!(bom.spec_version, "1.7");
/// # }
/// ```
///
/// [`SastEngine::scan`]: crate::scanner::sast::SastEngine::scan
pub fn render_cyclonedx(report: &SastScanReport) -> CycloneDxBom {
    let vulnerabilities = report
        .matches
        .iter()
        .map(|m| {
            let id = vuln_id(m);

            let source_url = m
                .metadata
                .references
                .as_deref()
                .and_then(|refs| refs.first().cloned());

            let source = CycloneDxSource {
                name: "xzardgz-sast".to_string(),
                url: source_url.clone(),
            };

            let ratings = vec![CycloneDxRating {
                source: CycloneDxSource {
                    name: "xzardgz-sast".to_string(),
                    url: source_url,
                },
                severity: severity_to_cyclonedx(&m.severity).to_string(),
                method: "other".to_string(),
            }];

            let cwes = m
                .metadata
                .cwe
                .as_deref()
                .map(extract_cwes)
                .unwrap_or_default();

            let affects = vec![CycloneDxAffects {
                ref_field: m.path.to_string_lossy().into_owned(),
            }];

            let analysis = CycloneDxAnalysis {
                state: "in_triage".to_string(),
                detail: m.message.clone(),
            };

            let mut properties = vec![
                CycloneDxProperty {
                    name: "xzardgz:sast:rule_id".to_string(),
                    value: m.rule_id.clone(),
                },
                CycloneDxProperty {
                    name: "xzardgz:sast:line".to_string(),
                    value: m.start.line.to_string(),
                },
                CycloneDxProperty {
                    name: "xzardgz:sast:fingerprint".to_string(),
                    value: m.fingerprint.clone(),
                },
            ];

            if let Some(license) = &m.metadata.license {
                properties.push(CycloneDxProperty {
                    name: "xzardgz:sast:license".to_string(),
                    value: license.clone(),
                });
            }

            CycloneDxVulnerability {
                bom_ref: id.clone(),
                id,
                source,
                ratings,
                cwes,
                description: m.message.clone(),
                affects,
                analysis,
                properties,
            }
        })
        .collect();

    CycloneDxBom {
        bom_format: "CycloneDX".to_string(),
        spec_version: "1.7".to_string(),
        version: 1,
        vulnerabilities,
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
    use crate::scanner::sast::match_model::{MatchSnippet, Position};
    use crate::scanner::sast::rule::metadata::{Category, Confidence, RuleMetadata, Severity};
    use crate::scanner::sast::{SastScanReport, SkippedRule};

    /// Construct a minimal empty [`SastScanReport`] for use in tests.
    fn empty_report() -> SastScanReport {
        SastScanReport {
            matches: vec![],
            skipped_rules: vec![],
            scanned_file_count: 0,
            skipped_file_count: 0,
            parse_error_count: 0,
            truncated_rule_file_pairs: 0,
            duration_ms: 0,
        }
    }

    /// Construct the canonical one-match report used by golden and schema tests.
    fn one_match_report() -> SastScanReport {
        let m = SastMatch {
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
        };
        SastScanReport {
            matches: vec![m],
            skipped_rules: vec![],
            scanned_file_count: 1,
            skipped_file_count: 0,
            parse_error_count: 0,
            truncated_rule_file_pairs: 0,
            duration_ms: 0,
        }
    }

    // -----------------------------------------------------------------------
    // Schema validation tests
    // -----------------------------------------------------------------------

    /// Load the CycloneDX 1.7 schema from disk and return it as a JSON value.
    fn load_cyclonedx_schema() -> serde_json::Value {
        let schema_str = include_str!("../../../../testdata/cyclonedx/bom-1.7.schema.json");
        serde_json::from_str(schema_str)
            // SAFETY: the schema file is bundled with the project and must be valid JSON.
            .expect("bom-1.7.schema.json must be valid JSON")
    }

    /// A one-match report serialises to JSON that is valid per the CycloneDX
    /// 1.7 schema.
    #[test]
    fn test_render_cyclonedx_one_match_passes_schema_validation() {
        let report = one_match_report();
        let bom = render_cyclonedx(&report);
        let bom_json = serde_json::to_value(&bom).expect("CycloneDxBom must serialise to JSON");
        let schema = load_cyclonedx_schema();

        let validator = jsonschema::validator_for(&schema).expect("CycloneDX schema must compile");
        let errors: Vec<_> = validator.iter_errors(&bom_json).collect();
        assert!(
            errors.is_empty(),
            "CycloneDX schema validation failed: {errors:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Empty report test
    // -----------------------------------------------------------------------

    /// An empty report produces a valid BOM with an empty vulnerabilities array.
    #[test]
    fn test_render_cyclonedx_empty_report_produces_empty_vulnerabilities() {
        let report = empty_report();
        let bom = render_cyclonedx(&report);
        assert_eq!(bom.bom_format, "CycloneDX");
        assert_eq!(bom.spec_version, "1.7");
        assert_eq!(bom.version, 1);
        assert!(bom.vulnerabilities.is_empty());
    }

    /// An empty report also passes the CycloneDX 1.7 schema.
    #[test]
    fn test_render_cyclonedx_empty_report_passes_schema_validation() {
        let report = empty_report();
        let bom = render_cyclonedx(&report);
        let bom_json = serde_json::to_value(&bom).expect("CycloneDxBom must serialise to JSON");
        let schema = load_cyclonedx_schema();

        let validator = jsonschema::validator_for(&schema).expect("CycloneDX schema must compile");
        let errors: Vec<_> = validator.iter_errors(&bom_json).collect();
        assert!(
            errors.is_empty(),
            "empty BOM failed schema validation: {errors:?}"
        );
    }

    // -----------------------------------------------------------------------
    // CWE extraction tests
    // -----------------------------------------------------------------------

    /// A match with `cwe: Some(["CWE-327"])` produces `cwes: [327]`.
    #[test]
    fn test_render_cyclonedx_cwe_extraction_parses_numeric_id() {
        let m = SastMatch {
            rule_id: "test-rule".to_string(),
            ruleset_id: String::new(),
            message: "test".to_string(),
            severity: Severity::Info,
            confidence: Confidence::High,
            path: PathBuf::from("a.rs"),
            start: Position {
                line: 1,
                col: 0,
                byte: 0,
            },
            end: Position {
                line: 1,
                col: 4,
                byte: 4,
            },
            snippet: MatchSnippet {
                text: "test".to_string(),
                context_before: vec![],
                context_after: vec![],
            },
            metavariables: BTreeMap::new(),
            metadata: RuleMetadata {
                cwe: Some(vec!["CWE-327".to_string()]),
                ..RuleMetadata::default()
            },
            fingerprint: "abc00000_0".to_string(),
            fix: None,
        };
        let report = SastScanReport {
            matches: vec![m],
            skipped_rules: vec![],
            scanned_file_count: 1,
            skipped_file_count: 0,
            parse_error_count: 0,
            truncated_rule_file_pairs: 0,
            duration_ms: 0,
        };
        let bom = render_cyclonedx(&report);
        assert_eq!(bom.vulnerabilities.len(), 1);
        assert_eq!(bom.vulnerabilities[0].cwes, vec![327u32]);
    }

    /// Entries that do not match `"CWE-NNN"` are silently ignored.
    #[test]
    fn test_extract_cwes_ignores_invalid_entries() {
        let input = vec![
            "CWE-100".to_string(),
            "not-a-cwe".to_string(),
            "CWE-abc".to_string(),
            "CWE-200".to_string(),
        ];
        let result = extract_cwes(&input);
        assert_eq!(result, vec![100u32, 200u32]);
    }

    /// An empty CWE slice produces an empty numeric list.
    #[test]
    fn test_extract_cwes_with_empty_input_produces_empty_output() {
        assert!(extract_cwes(&[]).is_empty());
    }

    // -----------------------------------------------------------------------
    // Severity mapping tests
    // -----------------------------------------------------------------------

    /// `Severity::Error` maps to `"high"`.
    #[test]
    fn test_severity_to_cyclonedx_error_maps_to_high() {
        assert_eq!(severity_to_cyclonedx(&Severity::Error), "high");
    }

    /// `Severity::Warning` maps to `"medium"`.
    #[test]
    fn test_severity_to_cyclonedx_warning_maps_to_medium() {
        assert_eq!(severity_to_cyclonedx(&Severity::Warning), "medium");
    }

    /// `Severity::Info` maps to `"info"`.
    #[test]
    fn test_severity_to_cyclonedx_info_maps_to_info() {
        assert_eq!(severity_to_cyclonedx(&Severity::Info), "info");
    }

    // -----------------------------------------------------------------------
    // Properties test
    // -----------------------------------------------------------------------

    /// A match with a license produces a `"xzardgz:sast:license"` property.
    #[test]
    fn test_render_cyclonedx_license_metadata_produces_license_property() {
        let m = SastMatch {
            rule_id: "test-rule".to_string(),
            ruleset_id: String::new(),
            message: "msg".to_string(),
            severity: Severity::Info,
            confidence: Confidence::Unknown,
            path: PathBuf::from("src/lib.rs"),
            start: Position {
                line: 1,
                col: 0,
                byte: 0,
            },
            end: Position {
                line: 1,
                col: 0,
                byte: 0,
            },
            snippet: MatchSnippet {
                text: String::new(),
                context_before: vec![],
                context_after: vec![],
            },
            metavariables: BTreeMap::new(),
            metadata: RuleMetadata {
                license: Some("Apache-2.0".to_string()),
                ..RuleMetadata::default()
            },
            fingerprint: "ff000000_0".to_string(),
            fix: None,
        };
        let report = SastScanReport {
            matches: vec![m],
            skipped_rules: vec![],
            scanned_file_count: 1,
            skipped_file_count: 0,
            parse_error_count: 0,
            truncated_rule_file_pairs: 0,
            duration_ms: 0,
        };
        let bom = render_cyclonedx(&report);
        let vuln = &bom.vulnerabilities[0];
        let license_prop = vuln
            .properties
            .iter()
            .find(|p| p.name == "xzardgz:sast:license");
        assert!(
            license_prop.is_some(),
            "expected xzardgz:sast:license property to be present"
        );
        assert_eq!(
            license_prop.unwrap().value,
            "Apache-2.0",
            "license property value must match metadata.license"
        );
    }

    /// A match without a license does not produce a `"xzardgz:sast:license"` property.
    #[test]
    fn test_render_cyclonedx_no_license_metadata_omits_license_property() {
        let m = SastMatch {
            rule_id: "test-rule".to_string(),
            ruleset_id: String::new(),
            message: "msg".to_string(),
            severity: Severity::Info,
            confidence: Confidence::Unknown,
            path: PathBuf::from("src/lib.rs"),
            start: Position {
                line: 1,
                col: 0,
                byte: 0,
            },
            end: Position {
                line: 1,
                col: 0,
                byte: 0,
            },
            snippet: MatchSnippet {
                text: String::new(),
                context_before: vec![],
                context_after: vec![],
            },
            metavariables: BTreeMap::new(),
            metadata: RuleMetadata::default(),
            fingerprint: "aa000000_0".to_string(),
            fix: None,
        };
        let report = SastScanReport {
            matches: vec![m],
            skipped_rules: vec![],
            scanned_file_count: 1,
            skipped_file_count: 0,
            parse_error_count: 0,
            truncated_rule_file_pairs: 0,
            duration_ms: 0,
        };
        let bom = render_cyclonedx(&report);
        let vuln = &bom.vulnerabilities[0];
        let has_license = vuln
            .properties
            .iter()
            .any(|p| p.name == "xzardgz:sast:license");
        assert!(
            !has_license,
            "no license property should be present when metadata.license is None"
        );
    }

    // -----------------------------------------------------------------------
    // Golden file tests
    // -----------------------------------------------------------------------

    /// Render the empty report and compare with the golden file.
    #[test]
    fn test_render_cyclonedx_empty_matches_golden_file() {
        let report = empty_report();
        let bom = render_cyclonedx(&report);
        let actual: serde_json::Value =
            serde_json::to_value(&bom).expect("serialisation must not fail");

        let golden_str = include_str!("../../../../testdata/sast/golden/cyclonedx_empty.json");
        let expected: serde_json::Value =
            serde_json::from_str(golden_str).expect("golden file must be valid JSON");

        assert_eq!(
            actual, expected,
            "rendered empty BOM does not match the golden file"
        );
    }

    /// Render the one-match report and compare with the golden file.
    #[test]
    fn test_render_cyclonedx_one_match_matches_golden_file() {
        let report = one_match_report();
        let bom = render_cyclonedx(&report);
        let actual: serde_json::Value =
            serde_json::to_value(&bom).expect("serialisation must not fail");

        let golden_str = include_str!("../../../../testdata/sast/golden/cyclonedx_one_match.json");
        let expected: serde_json::Value =
            serde_json::from_str(golden_str).expect("golden file must be valid JSON");

        assert_eq!(
            actual, expected,
            "rendered one-match BOM does not match the golden file"
        );
    }

    // -----------------------------------------------------------------------
    // ID and bom_ref tests
    // -----------------------------------------------------------------------

    /// The vulnerability `id` and `bom_ref` are both `"sast:<rule_id>:<fp8>"`.
    #[test]
    fn test_render_cyclonedx_id_uses_first_eight_fingerprint_chars() {
        let report = one_match_report();
        let bom = render_cyclonedx(&report);
        let vuln = &bom.vulnerabilities[0];
        assert_eq!(vuln.id, "sast:rust-weak-rsa-key:deadbeef");
        assert_eq!(vuln.bom_ref, vuln.id);
    }

    /// When the fingerprint is shorter than 8 characters the entire string is used.
    #[test]
    fn test_render_cyclonedx_id_handles_short_fingerprint() {
        let m = SastMatch {
            rule_id: "r".to_string(),
            ruleset_id: String::new(),
            message: "m".to_string(),
            severity: Severity::Info,
            confidence: Confidence::Unknown,
            path: PathBuf::from("a.rs"),
            start: Position {
                line: 1,
                col: 0,
                byte: 0,
            },
            end: Position {
                line: 1,
                col: 0,
                byte: 0,
            },
            snippet: MatchSnippet {
                text: String::new(),
                context_before: vec![],
                context_after: vec![],
            },
            metavariables: BTreeMap::new(),
            metadata: RuleMetadata::default(),
            fingerprint: "abc_0".to_string(),
            fix: None,
        };
        let report = SastScanReport {
            matches: vec![m],
            skipped_rules: vec![SkippedRule {
                rule_id: String::new(),
                reason: String::new(),
            }],
            scanned_file_count: 0,
            skipped_file_count: 0,
            parse_error_count: 0,
            truncated_rule_file_pairs: 0,
            duration_ms: 0,
        };
        let bom = render_cyclonedx(&report);
        // fingerprint = "abc_0" has 5 chars; 8.min(5) = 5
        assert_eq!(bom.vulnerabilities[0].id, "sast:r:abc_0");
    }

    // -----------------------------------------------------------------------
    // Affects test
    // -----------------------------------------------------------------------

    /// The `affects[0].ref` contains the repo-relative file path.
    #[test]
    fn test_render_cyclonedx_affects_contains_file_path() {
        let report = one_match_report();
        let bom = render_cyclonedx(&report);
        let vuln = &bom.vulnerabilities[0];
        assert_eq!(vuln.affects.len(), 1);
        assert_eq!(vuln.affects[0].ref_field, "src/crypto.rs");
    }
}
