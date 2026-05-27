//! Configuration validation for the security review plugin.
//!
//! Provides [`validate_security_review_config`], which enforces invariants on
//! a [`SecurityReviewConfig`][crate::config::SecurityReviewConfig] before a
//! plugin run begins. The validator is intentionally strict so that
//! misconfigured pipelines fail fast rather than producing misleading reports.

use crate::config::SecurityReviewConfig;
use crate::error::{PipelineError, Result};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Severity level strings accepted by the security review plugin.
const VALID_SEVERITY_THRESHOLDS: &[&str] = &["info", "low", "medium", "high", "critical"];

/// Report format strings accepted by the security review plugin.
const VALID_REPORT_FORMATS: &[&str] = &["markdown", "json", "sarif"];

// ---------------------------------------------------------------------------
// Public validation entry point
// ---------------------------------------------------------------------------

/// Validates a [`SecurityReviewConfig`][crate::config::SecurityReviewConfig]
/// and returns an error when any field is out of the allowed range.
///
/// # Arguments
///
/// * `config` - The security review configuration to validate.
///
/// # Returns
///
/// `Ok(())` when all fields pass validation.
///
/// # Errors
///
/// Returns [`PipelineError::Plugin`] when:
///
/// - `severity_threshold` is not one of `"info"`, `"low"`, `"medium"`,
///   `"high"`, or `"critical"` (case-insensitive).
/// - `max_findings` is zero.
/// - `confidence_threshold` is not in `[0.0, 1.0]`.
/// - Any element of `report_formats` is not one of `"markdown"`, `"json"`, or `"sarif"`.
///
/// # Examples
///
/// ```
/// use xzardgz::config::SecurityReviewConfig;
/// use xzardgz::plugins::security_review::config::validate_security_review_config;
///
/// let cfg = SecurityReviewConfig::default();
/// assert!(validate_security_review_config(&cfg).is_ok());
/// ```
pub fn validate_security_review_config(config: &SecurityReviewConfig) -> Result<()> {
    // Validate severity_threshold
    let lower = config.severity_threshold.to_lowercase();
    if !VALID_SEVERITY_THRESHOLDS.contains(&lower.as_str()) {
        return Err(PipelineError::Plugin(format!(
            "security-review: invalid severity_threshold '{}'; must be one of: {}",
            config.severity_threshold,
            VALID_SEVERITY_THRESHOLDS.join(", ")
        )));
    }

    // Validate max_findings
    if config.max_findings == 0 {
        return Err(PipelineError::Plugin(
            "security-review: max_findings must be >= 1".to_string(),
        ));
    }

    // Validate confidence_threshold
    if !(0.0..=1.0).contains(&config.confidence_threshold) {
        return Err(PipelineError::Plugin(format!(
            "security-review: confidence_threshold {} is out of [0.0, 1.0]",
            config.confidence_threshold
        )));
    }

    // Validate report_formats
    for fmt in &config.report_formats {
        let fmt_lower = fmt.to_lowercase();
        if !VALID_REPORT_FORMATS.contains(&fmt_lower.as_str()) {
            return Err(PipelineError::Plugin(format!(
                "security-review: invalid report format '{}'; must be one of: {}",
                fmt,
                VALID_REPORT_FORMATS.join(", ")
            )));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SecurityReviewConfig;

    fn cfg_with(severity: &str, max_findings: u32) -> SecurityReviewConfig {
        SecurityReviewConfig {
            max_findings,
            severity_threshold: severity.to_string(),
            ..SecurityReviewConfig::default()
        }
    }

    // ------------------------------------------------------------------
    // validate_security_review_config - valid cases
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_security_review_config_with_defaults_returns_ok() {
        let cfg = SecurityReviewConfig::default();
        assert!(validate_security_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_security_review_config_with_info_threshold_returns_ok() {
        let cfg = cfg_with("info", 10);
        assert!(validate_security_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_security_review_config_with_low_threshold_returns_ok() {
        let cfg = cfg_with("low", 1);
        assert!(validate_security_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_security_review_config_with_medium_threshold_returns_ok() {
        let cfg = cfg_with("medium", 25);
        assert!(validate_security_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_security_review_config_with_high_threshold_returns_ok() {
        let cfg = cfg_with("high", 5);
        assert!(validate_security_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_security_review_config_with_critical_threshold_returns_ok() {
        let cfg = cfg_with("critical", 1);
        assert!(validate_security_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_security_review_config_with_uppercase_threshold_returns_ok() {
        let cfg = cfg_with("HIGH", 5);
        assert!(validate_security_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_security_review_config_with_mixed_case_threshold_returns_ok() {
        let cfg = cfg_with("Medium", 10);
        assert!(validate_security_review_config(&cfg).is_ok());
    }

    // ------------------------------------------------------------------
    // validate_security_review_config - invalid severity threshold
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_security_review_config_with_invalid_threshold_returns_plugin_error() {
        let cfg = cfg_with("unknown", 10);
        let result = validate_security_review_config(&cfg);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Plugin(_)));
    }

    #[test]
    fn test_validate_security_review_config_with_empty_threshold_returns_plugin_error() {
        let cfg = cfg_with("", 10);
        let result = validate_security_review_config(&cfg);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Plugin(_)));
    }

    #[test]
    fn test_validate_security_review_config_invalid_threshold_error_contains_value() {
        let cfg = cfg_with("severe", 10);
        let err = validate_security_review_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("severe"));
    }

    #[test]
    fn test_validate_security_review_config_invalid_threshold_error_lists_valid_values() {
        let cfg = cfg_with("extreme", 5);
        let err = validate_security_review_config(&cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("info"));
        assert!(msg.contains("critical"));
    }

    // ------------------------------------------------------------------
    // validate_security_review_config - invalid max_findings
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_security_review_config_with_zero_max_findings_returns_plugin_error() {
        let cfg = cfg_with("medium", 0);
        let result = validate_security_review_config(&cfg);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Plugin(_)));
    }

    #[test]
    fn test_validate_security_review_config_with_one_max_findings_returns_ok() {
        let cfg = cfg_with("medium", 1);
        assert!(validate_security_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_security_review_config_max_findings_error_contains_hint() {
        let cfg = cfg_with("medium", 0);
        let err = validate_security_review_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("max_findings"));
    }

    // ------------------------------------------------------------------
    // validate_security_review_config - confidence_threshold
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_security_review_config_with_confidence_above_one_returns_plugin_error() {
        let cfg = SecurityReviewConfig {
            confidence_threshold: 1.1,
            ..SecurityReviewConfig::default()
        };
        let result = validate_security_review_config(&cfg);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Plugin(_)));
    }

    #[test]
    fn test_validate_security_review_config_with_confidence_below_zero_returns_plugin_error() {
        let cfg = SecurityReviewConfig {
            confidence_threshold: -0.1,
            ..SecurityReviewConfig::default()
        };
        let result = validate_security_review_config(&cfg);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Plugin(_)));
    }

    #[test]
    fn test_validate_security_review_config_with_confidence_at_zero_returns_ok() {
        let cfg = SecurityReviewConfig {
            confidence_threshold: 0.0,
            ..SecurityReviewConfig::default()
        };
        assert!(validate_security_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_security_review_config_with_confidence_at_one_returns_ok() {
        let cfg = SecurityReviewConfig {
            confidence_threshold: 1.0,
            ..SecurityReviewConfig::default()
        };
        assert!(validate_security_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_security_review_config_confidence_error_contains_value() {
        let cfg = SecurityReviewConfig {
            confidence_threshold: 2.5,
            ..SecurityReviewConfig::default()
        };
        let err = validate_security_review_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("2.5"));
    }

    // ------------------------------------------------------------------
    // validate_security_review_config - report_formats
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_security_review_config_with_invalid_report_format_returns_plugin_error() {
        let cfg = SecurityReviewConfig {
            report_formats: vec!["xml".to_string()],
            ..SecurityReviewConfig::default()
        };
        let result = validate_security_review_config(&cfg);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Plugin(_)));
    }

    #[test]
    fn test_validate_security_review_config_with_valid_report_formats_returns_ok() {
        let cfg = SecurityReviewConfig {
            report_formats: vec![
                "markdown".to_string(),
                "json".to_string(),
                "sarif".to_string(),
            ],
            ..SecurityReviewConfig::default()
        };
        assert!(validate_security_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_security_review_config_invalid_format_error_contains_value() {
        let cfg = SecurityReviewConfig {
            report_formats: vec!["html".to_string()],
            ..SecurityReviewConfig::default()
        };
        let err = validate_security_review_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("html"));
    }
}
