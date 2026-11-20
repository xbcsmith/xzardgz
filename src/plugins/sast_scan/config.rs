//! Configuration for the `sast_scan` plugin.
//!
//! [`SastScanConfig`] holds all knobs exposed to users through `config.yaml`
//! under the `sast_scan:` key. The struct is defined here (pre-populated for
//! Phase 6) and will be extended with AI-analysis fields in Phase 7.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Default helpers
// ---------------------------------------------------------------------------

fn default_enabled() -> bool {
    true
}

fn default_rulesets() -> Vec<String> {
    vec!["builtin:crypto".to_string(), "builtin:security".to_string()]
}

fn default_severity_threshold() -> String {
    "warning".to_string()
}

fn default_max_findings() -> usize {
    500
}

fn default_report_formats() -> Vec<String> {
    vec!["json".to_string()]
}

// ---------------------------------------------------------------------------
// SastScanConfig
// ---------------------------------------------------------------------------

/// Configuration for the SAST scan pipeline and the `xzardgz sast scan`
/// CLI subcommand.
///
/// Embed this under the `sast_scan:` key in `config.yaml`:
///
/// ```yaml
/// sast_scan:
///   enabled: true
///   rulesets:
///     - builtin:crypto
///     - builtin:security
///   severity_threshold: warning
///   max_findings: 500
///   report_formats:
///     - json
///     - sarif
/// ```
///
/// # Examples
///
/// ```
/// use xzardgz::plugins::sast_scan::config::SastScanConfig;
///
/// let config = SastScanConfig::default();
/// assert!(config.enabled);
/// assert_eq!(config.rulesets, vec!["builtin:crypto", "builtin:security"]);
/// assert_eq!(config.severity_threshold, "warning");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SastScanConfig {
    /// Whether the SAST scan pipeline is active. When `false`, the
    /// `xzardgz sast scan` subcommand exits immediately with no findings.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Ordered list of ruleset identifiers to load.
    ///
    /// Each entry is either a builtin identifier (`"builtin:crypto"`,
    /// `"builtin:security"`) or an absolute path to a directory containing
    /// YAML rule files. Builtin identifiers always fall back to the embedded
    /// rules when no external file matches.
    #[serde(default = "default_rulesets")]
    pub rulesets: Vec<String>,

    /// Minimum severity level for a finding to appear in the report.
    ///
    /// Accepted values (case-insensitive): `"info"`, `"warning"`, `"error"`.
    /// Findings below this threshold are discarded before output projection.
    /// Default: `"warning"`.
    #[serde(default = "default_severity_threshold")]
    pub severity_threshold: String,

    /// Maximum number of findings returned by a single scan.
    ///
    /// When this limit is reached the scan stops collecting additional
    /// findings. Default: `500`.
    #[serde(default = "default_max_findings")]
    pub max_findings: usize,

    /// Output format(s) produced by the scan.
    ///
    /// Accepted values: `"json"`, `"sarif"`, `"cyclonedx"`. Multiple formats
    /// may be listed; each produces a separate output file when `--output` is
    /// specified via the CLI. Default: `["json"]`.
    #[serde(default = "default_report_formats")]
    pub report_formats: Vec<String>,
}

impl Default for SastScanConfig {
    /// Creates a `SastScanConfig` with production-safe defaults.
    ///
    /// # Returns
    ///
    /// A `SastScanConfig` with `enabled: true`, both builtin rulesets active,
    /// `severity_threshold: "warning"`, `max_findings: 500`, and JSON output.
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            rulesets: default_rulesets(),
            severity_threshold: default_severity_threshold(),
            max_findings: default_max_findings(),
            report_formats: default_report_formats(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sast_scan_config_default_enabled_is_true() {
        assert!(SastScanConfig::default().enabled);
    }

    #[test]
    fn test_sast_scan_config_default_rulesets_contains_builtin_crypto() {
        let config = SastScanConfig::default();
        assert!(
            config.rulesets.contains(&"builtin:crypto".to_string()),
            "default rulesets must include builtin:crypto"
        );
    }

    #[test]
    fn test_sast_scan_config_default_rulesets_contains_builtin_security() {
        let config = SastScanConfig::default();
        assert!(
            config.rulesets.contains(&"builtin:security".to_string()),
            "default rulesets must include builtin:security"
        );
    }

    #[test]
    fn test_sast_scan_config_default_severity_threshold_is_warning() {
        assert_eq!(SastScanConfig::default().severity_threshold, "warning");
    }

    #[test]
    fn test_sast_scan_config_default_max_findings_is_500() {
        assert_eq!(SastScanConfig::default().max_findings, 500);
    }

    #[test]
    fn test_sast_scan_config_default_report_formats_contains_json() {
        let config = SastScanConfig::default();
        assert!(
            config.report_formats.contains(&"json".to_string()),
            "default report_formats must include json"
        );
    }

    #[test]
    fn test_sast_scan_config_serialization_roundtrip_preserves_defaults() {
        let original = SastScanConfig::default();
        let yaml = serde_yaml::to_string(&original).expect("serialization must succeed");
        let restored: SastScanConfig =
            serde_yaml::from_str(&yaml).expect("deserialization must succeed");
        assert_eq!(original.enabled, restored.enabled);
        assert_eq!(original.rulesets, restored.rulesets);
        assert_eq!(original.severity_threshold, restored.severity_threshold);
        assert_eq!(original.max_findings, restored.max_findings);
        assert_eq!(original.report_formats, restored.report_formats);
    }

    #[test]
    fn test_sast_scan_config_partial_yaml_uses_serde_defaults() {
        let yaml = "enabled: false\n";
        let config: SastScanConfig =
            serde_yaml::from_str(yaml).expect("partial yaml must deserialize");
        assert!(!config.enabled);
        assert_eq!(config.severity_threshold, "warning");
        assert_eq!(config.max_findings, 500);
    }
}
