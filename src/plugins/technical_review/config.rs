//! Configuration validation for the technical review plugin.
//!
//! Provides [`validate_technical_review_config`], which enforces invariants on
//! a [`TechnicalReviewConfig`][crate::config::TechnicalReviewConfig] before a
//! plugin run begins.  The validator is intentionally strict so that
//! misconfigured pipelines fail fast rather than producing misleading reports.

use crate::config::TechnicalReviewConfig;
use crate::error::{PipelineError, Result};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Severity level strings accepted by the technical review plugin.
const VALID_SEVERITY_THRESHOLDS: &[&str] = &["info", "low", "medium", "high", "critical"];

// ---------------------------------------------------------------------------
// Public validation entry point
// ---------------------------------------------------------------------------

/// Validates a [`TechnicalReviewConfig`][crate::config::TechnicalReviewConfig]
/// and returns an error when any field is out of the allowed range.
///
/// # Arguments
///
/// * `config` - The technical review configuration to validate.
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
///
/// # Examples
///
/// ```
/// use xzardgz::config::TechnicalReviewConfig;
/// use xzardgz::plugins::technical_review::config::validate_technical_review_config;
///
/// let cfg = TechnicalReviewConfig::default();
/// assert!(validate_technical_review_config(&cfg).is_ok());
/// ```
pub fn validate_technical_review_config(config: &TechnicalReviewConfig) -> Result<()> {
    let lower = config.severity_threshold.to_lowercase();
    if !VALID_SEVERITY_THRESHOLDS.contains(&lower.as_str()) {
        return Err(PipelineError::Plugin(format!(
            "technical-review: invalid severity_threshold '{}'; must be one of: {}",
            config.severity_threshold,
            VALID_SEVERITY_THRESHOLDS.join(", ")
        )));
    }

    if config.max_findings == 0 {
        return Err(PipelineError::Plugin(
            "technical-review: max_findings must be >= 1".to_string(),
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TechnicalReviewConfig;

    fn cfg_with(severity: &str, max_findings: u32) -> TechnicalReviewConfig {
        TechnicalReviewConfig {
            max_findings,
            severity_threshold: severity.to_string(),
            ..TechnicalReviewConfig::default()
        }
    }

    // ------------------------------------------------------------------
    // validate_technical_review_config - valid cases
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_technical_review_config_with_defaults_returns_ok() {
        let cfg = TechnicalReviewConfig::default();
        assert!(validate_technical_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_technical_review_config_with_info_threshold_returns_ok() {
        let cfg = cfg_with("info", 10);
        assert!(validate_technical_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_technical_review_config_with_low_threshold_returns_ok() {
        let cfg = cfg_with("low", 1);
        assert!(validate_technical_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_technical_review_config_with_medium_threshold_returns_ok() {
        let cfg = cfg_with("medium", 25);
        assert!(validate_technical_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_technical_review_config_with_high_threshold_returns_ok() {
        let cfg = cfg_with("high", 5);
        assert!(validate_technical_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_technical_review_config_with_critical_threshold_returns_ok() {
        let cfg = cfg_with("critical", 1);
        assert!(validate_technical_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_technical_review_config_with_uppercase_threshold_returns_ok() {
        let cfg = cfg_with("HIGH", 5);
        assert!(validate_technical_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_technical_review_config_with_mixed_case_threshold_returns_ok() {
        let cfg = cfg_with("Medium", 10);
        assert!(validate_technical_review_config(&cfg).is_ok());
    }

    // ------------------------------------------------------------------
    // validate_technical_review_config - invalid severity threshold
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_technical_review_config_with_invalid_threshold_returns_plugin_error() {
        let cfg = cfg_with("unknown", 10);
        let result = validate_technical_review_config(&cfg);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Plugin(_)));
    }

    #[test]
    fn test_validate_technical_review_config_with_empty_threshold_returns_plugin_error() {
        let cfg = cfg_with("", 10);
        let result = validate_technical_review_config(&cfg);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Plugin(_)));
    }

    #[test]
    fn test_validate_technical_review_config_invalid_threshold_error_contains_value() {
        let cfg = cfg_with("severe", 10);
        let err = validate_technical_review_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("severe"));
    }

    #[test]
    fn test_validate_technical_review_config_invalid_threshold_error_lists_valid_values() {
        let cfg = cfg_with("extreme", 5);
        let err = validate_technical_review_config(&cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("info"));
        assert!(msg.contains("critical"));
    }

    // ------------------------------------------------------------------
    // validate_technical_review_config - invalid max_findings
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_technical_review_config_with_zero_max_findings_returns_plugin_error() {
        let cfg = cfg_with("medium", 0);
        let result = validate_technical_review_config(&cfg);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Plugin(_)));
    }

    #[test]
    fn test_validate_technical_review_config_with_one_max_findings_returns_ok() {
        let cfg = cfg_with("medium", 1);
        assert!(validate_technical_review_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_technical_review_config_max_findings_error_contains_hint() {
        let cfg = cfg_with("medium", 0);
        let err = validate_technical_review_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("max_findings"));
    }
}
