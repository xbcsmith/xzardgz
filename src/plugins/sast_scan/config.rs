//! Configuration for the `sast_scan` plugin.
//!
//! [`SastScanConfig`] holds all knobs exposed to users through `config.yaml`
//! under the `sast_scan:` key. Both `ai_confidence_weight` and
//! `ai_analysis_enabled` are required from this first version per
//! `src/plugins/AGENTS.md`.

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

fn default_ai_confidence_weight() -> f64 {
    0.5
}

fn default_ai_analysis_enabled() -> bool {
    // AI analysis is disabled by default for SAST scan; the engine is fully
    // deterministic and does not benefit from an AI confidence leg unless
    // the operator explicitly opts in.
    false
}

fn default_confidence_threshold() -> f64 {
    0.5
}

// ---------------------------------------------------------------------------
// SastScanConfig
// ---------------------------------------------------------------------------

/// Configuration for the SAST scan plugin and the `xzardgz sast scan`
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
///   ai_confidence_weight: 0.5
///   ai_analysis_enabled: false
///   confidence_threshold: 0.5
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
/// assert!(!config.ai_analysis_enabled);
/// assert!((config.ai_confidence_weight - 0.5).abs() < f64::EPSILON);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SastScanConfig {
    /// Whether the SAST scan pipeline is active.
    ///
    /// When `false` the plugin returns immediately with no findings.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Ordered list of ruleset identifiers to load.
    ///
    /// Each entry is either a builtin identifier (`"builtin:crypto"`,
    /// `"builtin:security"`) or an absolute path to a directory containing
    /// YAML rule files.
    #[serde(default = "default_rulesets")]
    pub rulesets: Vec<String>,

    /// Minimum severity level for a finding to appear in the report.
    ///
    /// Accepted values (case-insensitive): `"info"`, `"warning"`, `"error"`.
    /// Default: `"warning"`.
    #[serde(default = "default_severity_threshold")]
    pub severity_threshold: String,

    /// Maximum number of findings returned by a single scan. Default: `500`.
    #[serde(default = "default_max_findings")]
    pub max_findings: usize,

    /// Output format(s) produced by the scan.
    ///
    /// Accepted values: `"json"`, `"sarif"`, `"cyclonedx"`. Default: `["json"]`.
    #[serde(default = "default_report_formats")]
    pub report_formats: Vec<String>,

    /// Weight of the AI confidence score in the blended result (`[0.0, 1.0]`).
    ///
    /// At `0.0` the AI score has no effect. At `1.0` only the AI score is used.
    /// Clamped to `[0.0, 1.0]` at blend time. Default: `0.5`.
    #[serde(default = "default_ai_confidence_weight")]
    pub ai_confidence_weight: f64,

    /// Whether to request an AI confidence estimate for the scan result.
    ///
    /// When `false` (the default) the blend step is skipped entirely and
    /// `blended_score == static_score`. No AI provider call is made.
    #[serde(default = "default_ai_analysis_enabled")]
    pub ai_analysis_enabled: bool,

    /// Minimum blended confidence score required for the scan result to be
    /// included in the final report. Findings that triggered an
    /// `AbsoluteViolation` bypass this threshold. Default: `0.5`.
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f64,
}

impl Default for SastScanConfig {
    /// Creates a `SastScanConfig` with production-safe defaults.
    ///
    /// # Returns
    ///
    /// A config with both builtin rulesets, `severity_threshold: "warning"`,
    /// `max_findings: 500`, `ai_analysis_enabled: false`, and JSON output.
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            rulesets: default_rulesets(),
            severity_threshold: default_severity_threshold(),
            max_findings: default_max_findings(),
            report_formats: default_report_formats(),
            ai_confidence_weight: default_ai_confidence_weight(),
            ai_analysis_enabled: default_ai_analysis_enabled(),
            confidence_threshold: default_confidence_threshold(),
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
        assert!(
            SastScanConfig::default()
                .rulesets
                .contains(&"builtin:crypto".to_string())
        );
    }

    #[test]
    fn test_sast_scan_config_default_rulesets_contains_builtin_security() {
        assert!(
            SastScanConfig::default()
                .rulesets
                .contains(&"builtin:security".to_string())
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
        assert!(
            SastScanConfig::default()
                .report_formats
                .contains(&"json".to_string())
        );
    }

    #[test]
    fn test_sast_scan_config_default_ai_confidence_weight_is_half() {
        let cfg = SastScanConfig::default();
        assert!((cfg.ai_confidence_weight - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_sast_scan_config_default_ai_analysis_enabled_is_false() {
        assert!(!SastScanConfig::default().ai_analysis_enabled);
    }

    #[test]
    fn test_sast_scan_config_default_confidence_threshold_is_half() {
        let cfg = SastScanConfig::default();
        assert!((cfg.confidence_threshold - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_sast_scan_config_serialization_roundtrip_preserves_all_fields() {
        let original = SastScanConfig::default();
        let yaml = serde_yaml::to_string(&original).expect("serialization must succeed");
        let restored: SastScanConfig =
            serde_yaml::from_str(&yaml).expect("deserialization must succeed");
        assert_eq!(original.enabled, restored.enabled);
        assert_eq!(original.rulesets, restored.rulesets);
        assert_eq!(original.severity_threshold, restored.severity_threshold);
        assert_eq!(original.max_findings, restored.max_findings);
        assert_eq!(original.report_formats, restored.report_formats);
        assert!(
            (original.ai_confidence_weight - restored.ai_confidence_weight).abs() < f64::EPSILON
        );
        assert_eq!(original.ai_analysis_enabled, restored.ai_analysis_enabled);
        assert!(
            (original.confidence_threshold - restored.confidence_threshold).abs() < f64::EPSILON
        );
    }

    #[test]
    fn test_sast_scan_config_partial_yaml_uses_serde_defaults() {
        let yaml = "enabled: false\n";
        let config: SastScanConfig =
            serde_yaml::from_str(yaml).expect("partial yaml must deserialize");
        assert!(!config.enabled);
        assert_eq!(config.severity_threshold, "warning");
        assert_eq!(config.max_findings, 500);
        assert!(!config.ai_analysis_enabled);
    }
}
