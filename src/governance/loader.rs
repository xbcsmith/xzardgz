//! Rule set loading and resolution for the governance system.
//!
//! This module is responsible for resolving the active [`RuleSet`] that the
//! validator uses.  It merges hardcoded embedded defaults with overrides and
//! additions supplied by a YAML governance file in the repository.
//!
//! Typical call sequence:
//!
//! 1. Call [`load_for_config`] with the current [`GovernanceConfig`].
//! 2. Pass the returned [`RuleSet`] to [`crate::governance::validator::GovernanceValidator::new`].
//!
//! For testing or advanced use, [`embedded_defaults`] returns the full default
//! set and [`load_from_path`] merges a specific file on top of those defaults.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::GovernanceConfig;
use crate::error::{PipelineError, Result};

use super::rules::{EnforcementLevel, GovernanceRule, RuleSource};

// ---------------------------------------------------------------------------
// Repository file schema
// ---------------------------------------------------------------------------

/// A rule override entry in a repository governance file.
///
/// Overrides can disable an embedded rule entirely or change its enforcement
/// level without modifying the binary.
///
/// # Examples
///
/// ```yaml
/// overrides:
///   - id: "governance.branch.safe_pattern"
///     enforcement: Optional
///   - id: "governance.content.no_secrets_pattern"
///     disabled: true
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleOverride {
    /// ID of the rule to override (must match an existing rule ID).
    pub id: String,
    /// New enforcement level for the rule.  `None` means keep the default.
    pub enforcement: Option<EnforcementLevel>,
    /// When `true` the rule is removed from the active set entirely.
    #[serde(default)]
    pub disabled: bool,
}

/// A custom rule defined entirely within the repository governance file.
///
/// Custom rules are validated with the same mechanisms as embedded rules;
/// their [`RuleSource`] is set to [`RuleSource::Derived`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRule {
    /// Unique identifier for this custom rule.
    pub id: String,
    /// Human-readable description of what this rule enforces.
    pub description: String,
    /// How violations of this rule are handled.
    pub enforcement: EnforcementLevel,
}

/// Top-level schema for a YAML governance file stored in the repository.
///
/// The file can contain zero or more [`RuleOverride`] entries and zero or more
/// [`CustomRule`] additions.  Both fields default to empty lists so a minimal
/// file can be as short as `{}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesFile {
    /// Overrides to apply on top of embedded defaults.
    #[serde(default)]
    pub overrides: Vec<RuleOverride>,
    /// Additional rules to append after embedded defaults have been merged.
    #[serde(default)]
    pub additional_rules: Vec<CustomRule>,
}

// ---------------------------------------------------------------------------
// RuleSet
// ---------------------------------------------------------------------------

/// A resolved, ordered collection of active governance rules.
///
/// `RuleSet` is the output of the loader layer and the input to the validator
/// layer.  Rules are stored in the order they were added; look-up by ID is
/// linear because rule sets are typically small (tens of rules).
///
/// # Examples
///
/// ```
/// use xzardgz::governance::{embedded_defaults, EnforcementLevel};
///
/// let rule_set = embedded_defaults();
/// assert!(!rule_set.is_empty());
/// assert!(rule_set.has("governance.path.no_traversal"));
/// ```
#[derive(Debug, Clone)]
pub struct RuleSet {
    /// The active rules in the order they were resolved.
    pub rules: Vec<GovernanceRule>,
}

impl RuleSet {
    /// Creates a [`RuleSet`] from a pre-built list of rules.
    ///
    /// # Arguments
    ///
    /// * `rules` - A vector of [`GovernanceRule`] values.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceRule, EnforcementLevel, RuleSource, RuleSet};
    ///
    /// let rules = vec![GovernanceRule {
    ///     id: "my.rule".to_string(),
    ///     description: "A rule".to_string(),
    ///     enforcement: EnforcementLevel::Required,
    ///     source: RuleSource::Embedded,
    /// }];
    /// let rule_set = RuleSet::new(rules);
    /// assert_eq!(rule_set.len(), 1);
    /// ```
    pub fn new(rules: Vec<GovernanceRule>) -> Self {
        Self { rules }
    }

    /// Returns a reference to the rule with the given `id`, or `None`.
    ///
    /// # Arguments
    ///
    /// * `id` - The rule identifier to look up.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::embedded_defaults;
    ///
    /// let rule_set = embedded_defaults();
    /// let rule = rule_set.get("governance.path.no_traversal");
    /// assert!(rule.is_some());
    /// assert_eq!(rule.unwrap().id, "governance.path.no_traversal");
    /// ```
    pub fn get(&self, id: &str) -> Option<&GovernanceRule> {
        self.rules.iter().find(|r| r.id == id)
    }

    /// Returns references to all rules with the given enforcement level.
    ///
    /// # Arguments
    ///
    /// * `level` - The [`EnforcementLevel`] to filter by.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{embedded_defaults, EnforcementLevel};
    ///
    /// let rule_set = embedded_defaults();
    /// let required = rule_set.by_enforcement(&EnforcementLevel::Required);
    /// assert!(!required.is_empty());
    /// ```
    pub fn by_enforcement(&self, level: &EnforcementLevel) -> Vec<&GovernanceRule> {
        self.rules
            .iter()
            .filter(|r| r.enforcement == *level)
            .collect()
    }

    /// Returns `true` if a rule with the given `id` exists in the set.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::embedded_defaults;
    ///
    /// let rule_set = embedded_defaults();
    /// assert!(rule_set.has("governance.plugin.valid_name"));
    /// assert!(!rule_set.has("governance.nonexistent.rule"));
    /// ```
    pub fn has(&self, id: &str) -> bool {
        self.rules.iter().any(|r| r.id == id)
    }

    /// Returns the number of active rules in the set.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::embedded_defaults;
    ///
    /// let rule_set = embedded_defaults();
    /// assert!(rule_set.len() > 0);
    /// ```
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Returns `true` when the set contains no rules.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::RuleSet;
    ///
    /// let empty = RuleSet::new(vec![]);
    /// assert!(empty.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Embedded defaults
// ---------------------------------------------------------------------------

/// Returns the hardcoded default governance rules compiled into the binary.
///
/// This function is the canonical source of truth for the governance rule
/// catalogue.  Callers that need the full set without any repository
/// customisation can use these rules directly; [`load_for_config`] calls this
/// internally when no repository governance file is found.
///
/// # Rule catalogue
///
/// | ID | Enforcement | Description |
/// |----|-------------|-------------|
/// | `governance.branch.safe_pattern` | Recommended | Branch names must match safe patterns |
/// | `governance.path.no_traversal` | Required | File paths must not contain `..` |
/// | `governance.path.no_null` | Required | File paths must not contain null bytes |
/// | `governance.plugin.valid_name` | Required | Plugin names must be valid identifiers |
/// | `governance.event.known_type` | Required | Event types from allowed list |
/// | `governance.endpoint.require_https` | Required | Provider endpoints must use HTTPS |
/// | `governance.content.no_secrets_pattern` | Recommended | Content must not match secret patterns |
/// | `governance.workspace.no_traversal` | Required | Workspace paths must not contain `..` |
/// | `governance.output.no_traversal` | Required | Output paths must not contain `..` |
/// | `governance.report.no_traversal` | Required | Report paths must not contain `..` |
///
/// # Examples
///
/// ```
/// use xzardgz::governance::embedded_defaults;
///
/// let rule_set = embedded_defaults();
/// assert!(rule_set.has("governance.path.no_traversal"));
/// assert!(rule_set.has("governance.endpoint.require_https"));
/// ```
pub fn embedded_defaults() -> RuleSet {
    let rules = vec![
        GovernanceRule {
            id: "governance.branch.safe_pattern".to_string(),
            description: "Branch names must match safe patterns (main, master, develop, or an \
                          approved prefix such as feature/, fix/, hotfix/, etc.)"
                .to_string(),
            enforcement: EnforcementLevel::Recommended,
            source: RuleSource::Embedded,
        },
        GovernanceRule {
            id: "governance.path.no_traversal".to_string(),
            description: "File paths must not contain path traversal sequences (..)".to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Embedded,
        },
        GovernanceRule {
            id: "governance.path.no_null".to_string(),
            description: "File paths must not contain null bytes".to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Embedded,
        },
        GovernanceRule {
            id: "governance.plugin.valid_name".to_string(),
            description: "Plugin names must be valid lowercase identifiers (start with a \
                          lowercase letter, contain only lowercase letters, digits, and underscores)"
                .to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Embedded,
        },
        GovernanceRule {
            id: "governance.event.known_type".to_string(),
            description: "Event types must be from the approved list: push, pull_request, issue, \
                          release, schedule, workflow_dispatch, tag, commit, merge"
                .to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Embedded,
        },
        GovernanceRule {
            id: "governance.endpoint.require_https".to_string(),
            description: "Provider endpoints must use HTTPS (https://)".to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Embedded,
        },
        GovernanceRule {
            id: "governance.content.no_secrets_pattern".to_string(),
            description: "Content must not contain patterns that resemble credentials, API keys, \
                          or private key material"
                .to_string(),
            enforcement: EnforcementLevel::Recommended,
            source: RuleSource::Embedded,
        },
        GovernanceRule {
            id: "governance.workspace.no_traversal".to_string(),
            description: "Workspace paths must not contain path traversal sequences (..)".to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Embedded,
        },
        GovernanceRule {
            id: "governance.output.no_traversal".to_string(),
            description: "Output paths must not contain path traversal sequences (..)".to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Embedded,
        },
        GovernanceRule {
            id: "governance.report.no_traversal".to_string(),
            description: "Report paths must not contain path traversal sequences (..)".to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Embedded,
        },
    ];
    RuleSet::new(rules)
}

// ---------------------------------------------------------------------------
// File-based loading
// ---------------------------------------------------------------------------

/// Loads a governance rules file from `path` and merges it with embedded defaults.
///
/// The merge order is:
/// 1. Start with all embedded default rules.
/// 2. Apply each [`RuleOverride`]: disabled rules are removed; enforcement
///    changes are applied in place.
/// 3. Append [`CustomRule`] additions as new [`GovernanceRule`] entries with
///    [`RuleSource::Derived`].
///
/// # Arguments
///
/// * `path` - Filesystem path to a YAML governance rules file.
///
/// # Errors
///
/// Returns [`PipelineError::Governance`] when the file cannot be read or when
/// its contents cannot be parsed as a [`RulesFile`].
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use xzardgz::governance::load_from_path;
///
/// let rule_set = load_from_path(Path::new("governance_rules.yaml")).unwrap();
/// assert!(!rule_set.is_empty());
/// ```
pub fn load_from_path(path: &Path) -> Result<RuleSet> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        PipelineError::Governance(format!(
            "failed to read governance rules file '{}': {}",
            path.display(),
            e
        ))
    })?;

    let rules_file: RulesFile = serde_yaml::from_str(&content).map_err(|e| {
        PipelineError::Governance(format!(
            "failed to parse governance rules file '{}': {}",
            path.display(),
            e
        ))
    })?;

    let defaults = embedded_defaults();

    // Apply overrides: keep rules that are not disabled; update enforcement levels.
    let mut active: Vec<GovernanceRule> = defaults
        .rules
        .into_iter()
        .filter_map(|mut rule| {
            if let Some(ovr) = rules_file.overrides.iter().find(|o| o.id == rule.id) {
                if ovr.disabled {
                    return None;
                }
                if let Some(level) = &ovr.enforcement {
                    rule.enforcement = level.clone();
                }
            }
            Some(rule)
        })
        .collect();

    // Append additional custom rules.
    for custom in rules_file.additional_rules {
        active.push(GovernanceRule {
            id: custom.id,
            description: custom.description,
            enforcement: custom.enforcement,
            source: RuleSource::Derived,
        });
    }

    Ok(RuleSet::new(active))
}

/// Loads rules according to [`GovernanceConfig`].
///
/// If `config.rules_path` is non-empty and the file exists on disk, the rules
/// are loaded via [`load_from_path`].  Otherwise the function falls back to
/// [`embedded_defaults`] without error.  This allows a repository to ship a
/// governance file without breaking deployments that do not have one.
///
/// # Arguments
///
/// * `config` - The governance section of the pipeline configuration.
///
/// # Errors
///
/// Returns [`PipelineError::Governance`] only when a rules file is found but
/// cannot be read or parsed.
///
/// # Examples
///
/// ```
/// use xzardgz::config::GovernanceConfig;
/// use xzardgz::governance::load_for_config;
///
/// // With an empty rules_path, always falls back to embedded defaults.
/// let config = GovernanceConfig {
///     enabled: true,
///     rules_path: String::new(),
///     fail_on_violation: true,
/// };
/// let rule_set = load_for_config(&config).unwrap();
/// assert!(!rule_set.is_empty());
/// ```
pub fn load_for_config(config: &GovernanceConfig) -> Result<RuleSet> {
    if config.rules_path.is_empty() {
        return Ok(embedded_defaults());
    }
    let path = std::path::Path::new(&config.rules_path);
    if path.exists() {
        load_from_path(path)
    } else {
        Ok(embedded_defaults())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PipelineError;

    // ------------------------------------------------------------------
    // embedded_defaults
    // ------------------------------------------------------------------

    #[test]
    fn test_embedded_defaults_has_expected_rule_count() {
        let rule_set = embedded_defaults();
        assert_eq!(rule_set.len(), 10, "expected exactly 10 embedded rules");
    }

    #[test]
    fn test_embedded_defaults_has_required_path_traversal_rule() {
        let rule_set = embedded_defaults();
        let rule = rule_set.get("governance.path.no_traversal");
        assert!(rule.is_some());
        assert_eq!(rule.unwrap().enforcement, EnforcementLevel::Required);
    }

    #[test]
    fn test_embedded_defaults_has_required_path_no_null_rule() {
        let rule_set = embedded_defaults();
        assert!(rule_set.has("governance.path.no_null"));
    }

    #[test]
    fn test_embedded_defaults_branch_rule_is_recommended() {
        let rule_set = embedded_defaults();
        let rule = rule_set.get("governance.branch.safe_pattern");
        assert!(rule.is_some());
        assert_eq!(rule.unwrap().enforcement, EnforcementLevel::Recommended);
    }

    #[test]
    fn test_embedded_defaults_content_rule_is_recommended() {
        let rule_set = embedded_defaults();
        let rule = rule_set.get("governance.content.no_secrets_pattern");
        assert!(rule.is_some());
        assert_eq!(rule.unwrap().enforcement, EnforcementLevel::Recommended);
    }

    #[test]
    fn test_embedded_defaults_all_traversal_rules_are_required() {
        let rule_set = embedded_defaults();
        for id in &[
            "governance.path.no_traversal",
            "governance.workspace.no_traversal",
            "governance.output.no_traversal",
            "governance.report.no_traversal",
        ] {
            let rule = rule_set.get(id);
            assert!(rule.is_some(), "rule {} should exist", id);
            assert_eq!(
                rule.unwrap().enforcement,
                EnforcementLevel::Required,
                "rule {} should be Required",
                id
            );
        }
    }

    // ------------------------------------------------------------------
    // RuleSet methods
    // ------------------------------------------------------------------

    #[test]
    fn test_rule_set_get_finds_existing_rule() {
        let rule_set = embedded_defaults();
        let rule = rule_set.get("governance.plugin.valid_name");
        assert!(rule.is_some());
        assert_eq!(rule.unwrap().id, "governance.plugin.valid_name");
    }

    #[test]
    fn test_rule_set_get_returns_none_for_missing_id() {
        let rule_set = embedded_defaults();
        assert!(rule_set.get("governance.nonexistent").is_none());
    }

    #[test]
    fn test_rule_set_has_returns_true_for_existing_rule() {
        let rule_set = embedded_defaults();
        assert!(rule_set.has("governance.endpoint.require_https"));
    }

    #[test]
    fn test_rule_set_has_returns_false_for_missing_rule() {
        let rule_set = embedded_defaults();
        assert!(!rule_set.has("governance.does_not_exist"));
    }

    #[test]
    fn test_rule_set_by_enforcement_returns_only_required_rules() {
        let rule_set = embedded_defaults();
        let required = rule_set.by_enforcement(&EnforcementLevel::Required);
        assert!(!required.is_empty());
        for r in &required {
            assert_eq!(
                r.enforcement,
                EnforcementLevel::Required,
                "rule {} should be Required",
                r.id
            );
        }
    }

    #[test]
    fn test_rule_set_by_enforcement_returns_recommended_rules() {
        let rule_set = embedded_defaults();
        let recommended = rule_set.by_enforcement(&EnforcementLevel::Recommended);
        // branch.safe_pattern and content.no_secrets_pattern are Recommended
        assert_eq!(recommended.len(), 2, "expected 2 Recommended rules");
    }

    #[test]
    fn test_rule_set_by_enforcement_returns_empty_for_optional() {
        let rule_set = embedded_defaults();
        let optional = rule_set.by_enforcement(&EnforcementLevel::Optional);
        assert!(
            optional.is_empty(),
            "no Optional rules should exist in defaults"
        );
    }

    #[test]
    fn test_rule_set_len_returns_rule_count() {
        let rule_set = embedded_defaults();
        assert_eq!(rule_set.len(), 10);
    }

    #[test]
    fn test_rule_set_is_empty_returns_false_when_has_rules() {
        let rule_set = embedded_defaults();
        assert!(!rule_set.is_empty());
    }

    #[test]
    fn test_rule_set_is_empty_returns_true_for_empty_set() {
        let empty = RuleSet::new(vec![]);
        assert!(empty.is_empty());
    }

    // ------------------------------------------------------------------
    // load_from_path
    // ------------------------------------------------------------------

    fn write_temp_yaml(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        // SAFETY: TempDir::new() only fails when the OS cannot create a temporary
        // directory, which is not expected in a normal test environment.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("governance_rules.yaml");
        // SAFETY: Writing to a freshly created TempDir cannot fail under normal conditions.
        std::fs::write(&path, content).unwrap();
        (tmp, path)
    }

    #[test]
    fn test_load_from_path_with_empty_overrides_returns_all_defaults() {
        let content = "overrides: []\nadditional_rules: []\n";
        let (_tmp, path) = write_temp_yaml(content);

        let result = load_from_path(&path);
        assert!(result.is_ok(), "should parse successfully: {:?}", result);
        let rule_set = result.unwrap();
        assert_eq!(
            rule_set.len(),
            embedded_defaults().len(),
            "empty overrides file should return all default rules"
        );
    }

    #[test]
    fn test_load_from_path_with_disabled_rule_removes_it() {
        let content = "\
overrides:
  - id: governance.branch.safe_pattern
    disabled: true
additional_rules: []
";
        let (_tmp, path) = write_temp_yaml(content);

        let result = load_from_path(&path);
        assert!(result.is_ok());
        let rule_set = result.unwrap();
        assert!(
            !rule_set.has("governance.branch.safe_pattern"),
            "disabled rule should be removed"
        );
        assert_eq!(
            rule_set.len(),
            embedded_defaults().len() - 1,
            "set should have one fewer rule after disabling"
        );
    }

    #[test]
    fn test_load_from_path_with_enforcement_override_changes_level() {
        let content = "\
overrides:
  - id: governance.branch.safe_pattern
    enforcement: Required
additional_rules: []
";
        let (_tmp, path) = write_temp_yaml(content);

        let result = load_from_path(&path);
        assert!(result.is_ok());
        let rule_set = result.unwrap();
        let rule = rule_set.get("governance.branch.safe_pattern");
        assert!(rule.is_some());
        assert_eq!(
            rule.unwrap().enforcement,
            EnforcementLevel::Required,
            "enforcement override should be applied"
        );
    }

    #[test]
    fn test_load_from_path_with_additional_rules_appends_them() {
        let content = "\
overrides: []
additional_rules:
  - id: custom.my_rule
    description: A custom rule for this repository
    enforcement: Recommended
";
        let (_tmp, path) = write_temp_yaml(content);

        let result = load_from_path(&path);
        assert!(result.is_ok());
        let rule_set = result.unwrap();
        assert_eq!(
            rule_set.len(),
            embedded_defaults().len() + 1,
            "additional rule should be appended"
        );
        let custom = rule_set.get("custom.my_rule");
        assert!(custom.is_some(), "custom rule should be findable by id");
        assert_eq!(custom.unwrap().enforcement, EnforcementLevel::Recommended);
        assert!(
            matches!(custom.unwrap().source, RuleSource::Derived),
            "custom rule source should be Derived"
        );
    }

    #[test]
    fn test_load_from_path_with_invalid_yaml_returns_governance_error() {
        // A YAML mapping key followed immediately by another ':' is illegal.
        let content = "overrides: [{id: [broken yaml}\n";
        let (_tmp, path) = write_temp_yaml(content);

        let result = load_from_path(&path);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), PipelineError::Governance(_)),
            "parse failure should produce PipelineError::Governance"
        );
    }

    #[test]
    fn test_load_from_path_with_missing_file_returns_governance_error() {
        let path = std::path::Path::new("/tmp/xzardgz_does_not_exist_abc123.yaml");
        // Ensure this path truly does not exist before testing.
        assert!(!path.exists(), "test precondition: path must not exist");

        // load_from_path should fail with Governance error on a missing file.
        let result = load_from_path(path);
        // The path should not exist (we constructed a highly-unique name), but
        // if by some cosmic coincidence it does, skip the assertion.
        if !path.exists() {
            assert!(result.is_err());
            assert!(
                matches!(result.unwrap_err(), PipelineError::Governance(_)),
                "missing file should produce PipelineError::Governance"
            );
        }
    }

    // ------------------------------------------------------------------
    // load_for_config
    // ------------------------------------------------------------------

    #[test]
    fn test_load_for_config_with_empty_rules_path_returns_defaults() {
        let config = GovernanceConfig {
            enabled: true,
            rules_path: String::new(),
            fail_on_violation: true,
        };
        let result = load_for_config(&config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), embedded_defaults().len());
    }

    #[test]
    fn test_load_for_config_with_nonexistent_file_falls_back_to_defaults() {
        let config = GovernanceConfig {
            enabled: true,
            rules_path: "/tmp/xzardgz_governance_does_not_exist_xyz.yaml".to_string(),
            fail_on_violation: false,
        };
        let result = load_for_config(&config);
        assert!(
            result.is_ok(),
            "missing file should fall back to defaults, not error"
        );
        assert_eq!(result.unwrap().len(), embedded_defaults().len());
    }

    #[test]
    fn test_load_for_config_with_valid_file_merges_correctly() {
        let content = "\
overrides:
  - id: governance.branch.safe_pattern
    disabled: true
additional_rules: []
";
        let (_tmp, path) = write_temp_yaml(content);

        let config = GovernanceConfig {
            enabled: true,
            // SAFETY: path was just created by write_temp_yaml; to_str() is safe
            rules_path: path.to_str().unwrap().to_string(),
            fail_on_violation: true,
        };
        let result = load_for_config(&config);
        assert!(result.is_ok());
        let rule_set = result.unwrap();
        assert!(!rule_set.has("governance.branch.safe_pattern"));
    }
}
