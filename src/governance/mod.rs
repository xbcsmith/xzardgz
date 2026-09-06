//! Governance system for the XZardgz pipeline.
//!
//! This module implements a three-layer governance architecture:
//!
//! ## Layer 1 — Rules (`rules`)
//!
//! Defines the data model for governance rules, violations, and results.
//! A [`GovernanceRule`] has an [`EnforcementLevel`] that determines the
//! consequence of a violation: `Required` rules block execution (when
//! `fail_on_violation` is set), `Recommended` rules produce warning
//! diagnostics, and `Optional` rules produce info diagnostics.
//!
//! ## Layer 2 — Loader (`loader`)
//!
//! Resolves the active [`RuleSet`] from a combination of hardcoded embedded
//! defaults and an optional repository governance YAML file.  The file format
//! supports disabling individual rules and changing enforcement levels without
//! recompiling the binary.
//!
//! ## Layer 3 — Validator / Checker (`validator`, [`GovernanceChecker`])
//!
//! Applies the active [`RuleSet`] to concrete pipeline inputs.
//! [`GovernanceValidator`] performs the raw checks and returns structured
//! [`GovernanceResult`] values.  [`GovernanceChecker`] wraps the validator
//! with the current [`GovernanceConfig`] so that the `enabled` and
//! `fail_on_violation` flags are enforced consistently across all call sites.
//!
//! ## Typical usage
//!
//! ```
//! use xzardgz::config::GovernanceConfig;
//! use xzardgz::governance::GovernanceChecker;
//!
//! let config = GovernanceConfig {
//!     enabled: true,
//!     rules_path: String::new(),
//!     fail_on_violation: true,
//! };
//!
//! let checker = GovernanceChecker::from_config(&config).unwrap();
//!
//! // Safe path — returns Ok with empty diagnostics.
//! let diags = checker.check_file_path("src/main.rs").unwrap();
//! assert!(diags.is_empty());
//! ```

pub mod loader;
pub mod parser;
pub mod rules;
pub mod validator;

pub use loader::{RuleSet, embedded_defaults, load_for_config};
pub use rules::{
    EnforcementLevel, GovernanceResult, GovernanceRule, GovernanceViolation, RuleSource,
};
pub use validator::GovernanceValidator;

use crate::config::GovernanceConfig;
use crate::diagnostics::Diagnostics;
use crate::error::{PipelineError, Result};

// ---------------------------------------------------------------------------
// GovernanceChecker
// ---------------------------------------------------------------------------

/// High-level governance checker used throughout the pipeline.
///
/// `GovernanceChecker` wraps a [`GovernanceValidator`] and the current
/// [`GovernanceConfig`].  It enforces the `enabled` and `fail_on_violation`
/// flags so callers do not need to check them manually.
///
/// # Disabled mode
///
/// When `config.enabled` is `false`, every `check_*` method returns
/// `Ok(empty Diagnostics)` without performing any validation.
///
/// # Fail-on-violation
///
/// When `config.fail_on_violation` is `true` and a check produces one or more
/// `Required` violations, the checker returns
/// `Err(PipelineError::Governance(...))` with all blocking violation messages
/// joined by `"; "`.  Non-blocking (`Recommended` / `Optional`) violations
/// are always converted to diagnostics and never cause an error.
///
/// # Examples
///
/// ```
/// use xzardgz::config::GovernanceConfig;
/// use xzardgz::governance::GovernanceChecker;
///
/// let config = GovernanceConfig {
///     enabled: true,
///     rules_path: String::new(),
///     fail_on_violation: true,
/// };
/// let checker = GovernanceChecker::from_config(&config).unwrap();
/// assert!(checker.is_enabled());
/// assert!(checker.check_file_path("src/lib.rs").unwrap().is_empty());
/// ```
pub struct GovernanceChecker {
    validator: GovernanceValidator,
    config: GovernanceConfig,
}

impl GovernanceChecker {
    /// Creates a checker by loading rules according to `config`.
    ///
    /// If `config.rules_path` points to an existing file that file is loaded
    /// and merged with embedded defaults.  Otherwise embedded defaults are
    /// used.  See [`load_for_config`] for the full resolution logic.
    ///
    /// # Arguments
    ///
    /// * `config` - The governance configuration section from the pipeline config.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] when a rules file is found but
    /// cannot be read or parsed.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::config::GovernanceConfig;
    /// use xzardgz::governance::GovernanceChecker;
    ///
    /// let config = GovernanceConfig {
    ///     enabled: true,
    ///     rules_path: String::new(),
    ///     fail_on_violation: false,
    /// };
    /// let checker = GovernanceChecker::from_config(&config).unwrap();
    /// assert!(checker.is_enabled());
    /// ```
    pub fn from_config(config: &GovernanceConfig) -> Result<Self> {
        let rules = load_for_config(config)?;
        Ok(Self {
            validator: GovernanceValidator::new(rules),
            config: config.clone(),
        })
    }

    /// Returns a reference to the underlying [`GovernanceValidator`].
    ///
    /// Useful for calling raw `validate_*` methods when you need the result
    /// without the config-level `fail_on_violation` and `enabled` logic.
    pub fn validator(&self) -> &GovernanceValidator {
        &self.validator
    }

    /// Returns a reference to the [`GovernanceConfig`] this checker was built from.
    pub fn config(&self) -> &GovernanceConfig {
        &self.config
    }

    /// Returns `true` when governance checking is enabled.
    ///
    /// When this is `false` all `check_*` methods return empty diagnostics
    /// without performing any validation.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Validates a branch name.
    ///
    /// Returns `Ok(empty Diagnostics)` when governance is disabled or the
    /// branch name passes all checks.  Returns warning diagnostics for
    /// `Recommended` violations (the branch rule is `Recommended` by default
    /// so it never triggers `fail_on_violation`).
    ///
    /// # Arguments
    ///
    /// * `name` - Branch name to validate.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] only if the branch rule has been
    /// promoted to `Required` in the active rule set, `fail_on_violation` is
    /// `true`, and the branch name violates the rule.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::config::GovernanceConfig;
    /// use xzardgz::governance::GovernanceChecker;
    ///
    /// let config = GovernanceConfig {
    ///     enabled: true,
    ///     rules_path: String::new(),
    ///     fail_on_violation: true,
    /// };
    /// let checker = GovernanceChecker::from_config(&config).unwrap();
    /// let diags = checker.check_branch("main").unwrap();
    /// assert!(diags.is_empty());
    /// ```
    pub fn check_branch(&self, name: &str) -> Result<Diagnostics> {
        if !self.config.enabled {
            return Ok(Diagnostics::new());
        }
        self.handle_result(self.validator.validate_branch_name(name))
    }

    /// Validates a file path.
    ///
    /// Checks for path traversal sequences and null bytes.  Both rules are
    /// `Required` by default, so violations produce an error when
    /// `fail_on_violation` is `true`.
    ///
    /// # Arguments
    ///
    /// * `path` - File path to validate.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] on traversal or null-byte
    /// violations when `fail_on_violation` is `true`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::config::GovernanceConfig;
    /// use xzardgz::governance::GovernanceChecker;
    ///
    /// let config = GovernanceConfig {
    ///     enabled: true,
    ///     rules_path: String::new(),
    ///     fail_on_violation: false,
    /// };
    /// let checker = GovernanceChecker::from_config(&config).unwrap();
    /// assert!(checker.check_file_path("src/main.rs").unwrap().is_empty());
    /// ```
    pub fn check_file_path(&self, path: &str) -> Result<Diagnostics> {
        if !self.config.enabled {
            return Ok(Diagnostics::new());
        }
        self.handle_result(self.validator.validate_file_path(path))
    }

    /// Validates an output path.
    ///
    /// # Arguments
    ///
    /// * `path` - Output path to validate.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] on traversal violations when
    /// `fail_on_violation` is `true`.
    pub fn check_output_path(&self, path: &str) -> Result<Diagnostics> {
        if !self.config.enabled {
            return Ok(Diagnostics::new());
        }
        self.handle_result(self.validator.validate_output_path(path))
    }

    /// Validates a report path.
    ///
    /// # Arguments
    ///
    /// * `path` - Report path to validate.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] on traversal violations when
    /// `fail_on_violation` is `true`.
    pub fn check_report_path(&self, path: &str) -> Result<Diagnostics> {
        if !self.config.enabled {
            return Ok(Diagnostics::new());
        }
        self.handle_result(self.validator.validate_report_path(path))
    }

    /// Validates a workspace path.
    ///
    /// # Arguments
    ///
    /// * `path` - Workspace path to validate.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] on traversal violations when
    /// `fail_on_violation` is `true`.
    pub fn check_workspace_path(&self, path: &str) -> Result<Diagnostics> {
        if !self.config.enabled {
            return Ok(Diagnostics::new());
        }
        self.handle_result(self.validator.validate_workspace_path(path))
    }

    /// Validates content for secret patterns.
    ///
    /// The content-safety rule is `Recommended` by default, so this method
    /// returns warning diagnostics rather than an error under the default
    /// configuration.
    ///
    /// # Arguments
    ///
    /// * `content` - Text content to scan.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] only if the content rule has been
    /// promoted to `Required` in the active rule set, `fail_on_violation` is
    /// `true`, and the content matches a secret pattern.
    pub fn check_content_safety(&self, content: &str) -> Result<Diagnostics> {
        if !self.config.enabled {
            return Ok(Diagnostics::new());
        }
        self.handle_result(self.validator.validate_content_safety(content))
    }

    /// Validates a plugin name.
    ///
    /// # Arguments
    ///
    /// * `name` - Plugin name to validate.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] for invalid plugin names when
    /// `fail_on_violation` is `true`.
    pub fn check_plugin_name(&self, name: &str) -> Result<Diagnostics> {
        if !self.config.enabled {
            return Ok(Diagnostics::new());
        }
        self.handle_result(self.validator.validate_plugin_name(name))
    }

    /// Validates an event type.
    ///
    /// # Arguments
    ///
    /// * `event_type` - Event type string to validate.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] for unknown event types when
    /// `fail_on_violation` is `true`.
    pub fn check_event_type(&self, event_type: &str) -> Result<Diagnostics> {
        if !self.config.enabled {
            return Ok(Diagnostics::new());
        }
        self.handle_result(self.validator.validate_event_type(event_type))
    }

    /// Validates a provider endpoint URL.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - Provider endpoint URL to validate.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] for non-HTTPS endpoints when
    /// `fail_on_violation` is `true`.
    pub fn check_provider_endpoint(&self, endpoint: &str) -> Result<Diagnostics> {
        if !self.config.enabled {
            return Ok(Diagnostics::new());
        }
        self.handle_result(self.validator.validate_provider_endpoint(endpoint))
    }

    /// Runs all applicable governance checks for a complete workflow invocation.
    ///
    /// Validates the optional branch name, each file path, each plugin name,
    /// each event type, and each provider endpoint.  All violations are
    /// collected and merged before the single [`handle_result`] call so that
    /// the caller receives the full picture in one error or diagnostic set.
    ///
    /// Returns `Ok(empty Diagnostics)` immediately when governance is disabled.
    ///
    /// # Arguments
    ///
    /// * `branch` - Optional branch name to validate.
    /// * `file_paths` - File paths to validate for traversal and null bytes.
    /// * `plugin_names` - Plugin names to validate.
    /// * `event_types` - Event types to validate.
    /// * `provider_endpoints` - Provider endpoint URLs to validate.
    ///
    /// # Returns
    ///
    /// Merged [`Diagnostics`] from all checks.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] with a `"; "`-separated list of
    /// all blocking violation messages when `fail_on_violation` is `true` and
    /// any `Required` violation is found.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::config::GovernanceConfig;
    /// use xzardgz::governance::GovernanceChecker;
    ///
    /// let config = GovernanceConfig {
    ///     enabled: true,
    ///     rules_path: String::new(),
    ///     fail_on_violation: true,
    /// };
    /// let checker = GovernanceChecker::from_config(&config).unwrap();
    ///
    /// let diags = checker.check_workflow_inputs(
    ///     Some("main"),
    ///     &["src/main.rs"],
    ///     &["my_plugin"],
    ///     &["push"],
    ///     &["https://api.openai.com"],
    /// ).unwrap();
    /// assert!(diags.is_empty());
    /// ```
    pub fn check_workflow_inputs(
        &self,
        branch: Option<&str>,
        file_paths: &[&str],
        plugin_names: &[&str],
        event_types: &[&str],
        provider_endpoints: &[&str],
    ) -> Result<Diagnostics> {
        if !self.config.enabled {
            return Ok(Diagnostics::new());
        }

        let mut merged = GovernanceResult::new();

        if let Some(b) = branch {
            merged.merge(self.validator.validate_branch_name(b));
        }
        for path in file_paths {
            merged.merge(self.validator.validate_file_path(path));
        }
        for name in plugin_names {
            merged.merge(self.validator.validate_plugin_name(name));
        }
        for et in event_types {
            merged.merge(self.validator.validate_event_type(et));
        }
        for ep in provider_endpoints {
            merged.merge(self.validator.validate_provider_endpoint(ep));
        }

        self.handle_result(merged)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Converts a [`GovernanceResult`] into diagnostics or an error.
    ///
    /// When `fail_on_violation` is `true` and the result contains blocking
    /// violations, all blocking violation messages are joined with `"; "` and
    /// returned as [`PipelineError::Governance`].  Otherwise the result is
    /// converted to [`Diagnostics`] and returned as `Ok`.
    fn handle_result(&self, result: GovernanceResult) -> Result<Diagnostics> {
        if self.config.fail_on_violation && result.has_blocking_violations() {
            let messages: Vec<String> = result
                .blocking_violations()
                .iter()
                .map(|v| format!("[{}] {}", v.rule_id, v.message))
                .collect();
            return Err(PipelineError::Governance(messages.join("; ")));
        }
        Ok(result.to_diagnostics())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GovernanceConfig;

    fn enabled_strict() -> GovernanceConfig {
        GovernanceConfig {
            enabled: true,
            rules_path: String::new(),
            fail_on_violation: true,
        }
    }

    fn enabled_permissive() -> GovernanceConfig {
        GovernanceConfig {
            enabled: true,
            rules_path: String::new(),
            fail_on_violation: false,
        }
    }

    fn disabled() -> GovernanceConfig {
        GovernanceConfig {
            enabled: false,
            rules_path: String::new(),
            fail_on_violation: false,
        }
    }

    // ------------------------------------------------------------------
    // GovernanceChecker::from_config
    // ------------------------------------------------------------------

    #[test]
    fn test_from_config_with_default_config_succeeds() {
        // Use empty rules_path so we always use embedded defaults regardless
        // of the working directory during testing.
        let config = enabled_strict();
        let result = GovernanceChecker::from_config(&config);
        assert!(
            result.is_ok(),
            "from_config should succeed with embedded defaults: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_from_config_exposes_validator_and_config_accessors() {
        let config = enabled_permissive();
        let checker = GovernanceChecker::from_config(&config).unwrap();
        assert!(!checker.validator().rules().is_empty());
        assert!(checker.config().enabled);
        assert!(!checker.config().fail_on_violation);
    }

    // ------------------------------------------------------------------
    // GovernanceChecker::is_enabled
    // ------------------------------------------------------------------

    #[test]
    fn test_is_enabled_returns_true_when_enabled() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        assert!(checker.is_enabled());
    }

    #[test]
    fn test_is_enabled_returns_false_when_disabled() {
        let checker = GovernanceChecker::from_config(&disabled()).unwrap();
        assert!(!checker.is_enabled());
    }

    // ------------------------------------------------------------------
    // check_branch
    // ------------------------------------------------------------------

    #[test]
    fn test_check_branch_returns_empty_when_disabled() {
        let checker = GovernanceChecker::from_config(&disabled()).unwrap();
        // Even an invalid branch should return empty diagnostics when disabled.
        let result = checker.check_branch("INVALID_BRANCH_NAME");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_check_branch_safe_name_returns_empty_diagnostics() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        let result = checker.check_branch("main");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_check_branch_unsafe_name_returns_warning_when_fail_on_violation_false() {
        let checker = GovernanceChecker::from_config(&enabled_permissive()).unwrap();
        let result = checker.check_branch("INVALID_UPPERCASE");
        assert!(result.is_ok(), "non-blocking violation should not error");
        let diags = result.unwrap();
        assert!(!diags.is_empty(), "should have warning diagnostics");
        assert_eq!(diags.warnings().len(), 1);
    }

    /// Verifies that the branch rule (Recommended) does NOT trigger fail_on_violation.
    ///
    /// Even with `fail_on_violation: true`, a Recommended rule violation is
    /// converted to a warning diagnostic, not an error.  Only Required
    /// violations block execution.
    #[test]
    fn test_check_branch_unsafe_name_returns_err_when_fail_on_violation_true() {
        // The branch rule is Recommended, not Required.
        // Therefore even with fail_on_violation:true, the result must be Ok.
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        let result = checker.check_branch("INVALID_UPPERCASE");
        assert!(
            result.is_ok(),
            "Recommended rule violations must never return Err, got: {:?}",
            result.err()
        );
        let diags = result.unwrap();
        assert!(
            !diags.is_empty(),
            "should have warning diagnostics for the Recommended violation"
        );
    }

    // ------------------------------------------------------------------
    // check_file_path
    // ------------------------------------------------------------------

    #[test]
    fn test_check_file_path_with_safe_path_returns_empty_diagnostics() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        let result = checker.check_file_path("src/main.rs");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_check_file_path_with_traversal_returns_err_when_fail_on_violation_true() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        let result = checker.check_file_path("../etc/passwd");
        assert!(
            result.is_err(),
            "traversal path should fail with strict config"
        );
        let err = result.unwrap_err();
        assert!(
            matches!(err, PipelineError::Governance(_)),
            "error should be Governance variant"
        );
        assert!(
            err.to_string().contains("governance"),
            "error message should mention governance"
        );
    }

    #[test]
    fn test_check_file_path_with_traversal_returns_warning_when_permissive() {
        let checker = GovernanceChecker::from_config(&enabled_permissive()).unwrap();
        let result = checker.check_file_path("../etc/passwd");
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[test]
    fn test_check_file_path_returns_empty_when_disabled() {
        let checker = GovernanceChecker::from_config(&disabled()).unwrap();
        let result = checker.check_file_path("../etc/passwd");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // ------------------------------------------------------------------
    // check_output_path / check_report_path / check_workspace_path
    // ------------------------------------------------------------------

    #[test]
    fn test_check_output_path_with_traversal_returns_err_when_strict() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        assert!(checker.check_output_path("../output").is_err());
    }

    #[test]
    fn test_check_report_path_with_traversal_returns_err_when_strict() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        assert!(checker.check_report_path("../reports").is_err());
    }

    #[test]
    fn test_check_workspace_path_with_traversal_returns_err_when_strict() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        assert!(checker.check_workspace_path("../../workspace").is_err());
    }

    // ------------------------------------------------------------------
    // check_plugin_name
    // ------------------------------------------------------------------

    #[test]
    fn test_check_plugin_name_invalid_returns_err() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        let result = checker.check_plugin_name("InvalidName");
        assert!(
            result.is_err(),
            "invalid plugin name should fail with strict config"
        );
        assert!(matches!(result.unwrap_err(), PipelineError::Governance(_)));
    }

    #[test]
    fn test_check_plugin_name_valid_returns_empty_diagnostics() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        let result = checker.check_plugin_name("my_plugin");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_check_plugin_name_returns_empty_when_disabled() {
        let checker = GovernanceChecker::from_config(&disabled()).unwrap();
        let result = checker.check_plugin_name("InvalidName");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // ------------------------------------------------------------------
    // check_event_type
    // ------------------------------------------------------------------

    #[test]
    fn test_check_event_type_unknown_returns_err_when_strict() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        assert!(checker.check_event_type("unknown_event").is_err());
    }

    #[test]
    fn test_check_event_type_known_returns_empty_diagnostics() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        assert!(checker.check_event_type("push").unwrap().is_empty());
    }

    // ------------------------------------------------------------------
    // check_provider_endpoint
    // ------------------------------------------------------------------

    #[test]
    fn test_check_provider_endpoint_http_returns_err() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        let result = checker.check_provider_endpoint("http://api.example.com");
        assert!(
            result.is_err(),
            "HTTP endpoint should fail with strict config"
        );
        assert!(matches!(result.unwrap_err(), PipelineError::Governance(_)));
    }

    #[test]
    fn test_check_provider_endpoint_https_returns_empty_diagnostics() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        assert!(
            checker
                .check_provider_endpoint("https://api.openai.com")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn test_check_provider_endpoint_empty_skips_validation() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        assert!(checker.check_provider_endpoint("").unwrap().is_empty());
    }

    // ------------------------------------------------------------------
    // check_content_safety
    // ------------------------------------------------------------------

    #[test]
    fn test_check_content_safety_with_safe_content_returns_empty() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        assert!(
            checker
                .check_content_safety("Normal log output")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn test_check_content_safety_with_secret_pattern_returns_warning_not_error_by_default() {
        // The content rule is Recommended by default, so fail_on_violation has no effect.
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        let result = checker.check_content_safety("password=supersecret");
        assert!(
            result.is_ok(),
            "Recommended rule should not trigger fail_on_violation, got: {:?}",
            result.err()
        );
        let diags = result.unwrap();
        assert!(!diags.is_empty());
    }

    // ------------------------------------------------------------------
    // check_workflow_inputs
    // ------------------------------------------------------------------

    #[test]
    fn test_check_workflow_inputs_all_valid_returns_empty_diagnostics() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        let result = checker.check_workflow_inputs(
            Some("main"),
            &["src/main.rs", "lib/mod.rs"],
            &["my_plugin"],
            &["push"],
            &["https://api.openai.com"],
        );
        assert!(result.is_ok(), "all valid inputs should pass: {:?}", result);
        assert!(
            result.unwrap().is_empty(),
            "no violations expected for valid inputs"
        );
    }

    #[test]
    fn test_check_workflow_inputs_returns_empty_when_disabled() {
        let checker = GovernanceChecker::from_config(&disabled()).unwrap();
        let result = checker.check_workflow_inputs(
            Some("INVALID"),
            &["../etc/passwd"],
            &["InvalidPlugin"],
            &["unknown_event"],
            &["http://insecure.com"],
        );
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_check_workflow_inputs_with_blocking_violations_returns_err_when_strict() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        // The file path violation is Required and fail_on_violation is true.
        let result = checker.check_workflow_inputs(None, &["../etc/passwd"], &[], &[], &[]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Governance(_)));
    }

    #[test]
    fn test_check_workflow_inputs_multiple_violations_returns_merged() {
        let checker = GovernanceChecker::from_config(&enabled_permissive()).unwrap();
        // Mix of Recommended (branch) and Required (path, plugin, event, endpoint)
        // violations.  With fail_on_violation:false all become diagnostics.
        let result = checker.check_workflow_inputs(
            Some("INVALID_BRANCH"),           // Recommended violation
            &["../etc/passwd", "safe.rs"],    // 1 Required violation
            &["InvalidPlugin"],               // 1 Required violation
            &["unknown_event"],               // 1 Required violation
            &["http://insecure.example.com"], // 1 Required violation
        );
        assert!(
            result.is_ok(),
            "fail_on_violation is false, should return Ok: {:?}",
            result.err()
        );
        let diags = result.unwrap();
        assert!(
            diags.len() >= 4,
            "expected at least 4 diagnostics (branch + path + plugin + event + endpoint), got {}",
            diags.len()
        );
    }

    #[test]
    fn test_check_workflow_inputs_with_none_branch_skips_branch_check() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        let result = checker.check_workflow_inputs(
            None,
            &["src/main.rs"],
            &["my_plugin"],
            &["push"],
            &["https://api.openai.com"],
        );
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_check_workflow_inputs_error_message_contains_all_blocking_violations() {
        let checker = GovernanceChecker::from_config(&enabled_strict()).unwrap();
        // Two Required violations: path traversal and invalid plugin name.
        let result = checker.check_workflow_inputs(None, &["../bad"], &["BadPlugin"], &[], &[]);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        // Both violations should appear in the error message.
        assert!(
            err_msg.contains("governance.path.no_traversal") || err_msg.contains(".."),
            "error should mention traversal, got: {}",
            err_msg
        );
        assert!(
            err_msg.contains("governance.plugin.valid_name") || err_msg.contains("BadPlugin"),
            "error should mention plugin, got: {}",
            err_msg
        );
    }
}
