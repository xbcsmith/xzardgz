//! Rule set loading and resolution for the governance system.
//!
//! This module resolves the active [`RuleSet`] from a combination of
//! hardcoded embedded defaults and an optional `AGENTS.md` Markdown file in
//! the repository.  When a `rules_path` is configured and the file exists, the
//! file is parsed by [`parser::parse_agents_md`]; the resulting rules are
//! merged with [`embedded_defaults`] (parsed rules take precedence by ID).
//! On any failure the module falls back silently to embedded defaults.
//!
//! Typical call sequence:
//!
//! 1. Call [`load_for_config`] with the current [`GovernanceConfig`].
//! 2. Pass the returned [`RuleSet`] to
//!    [`crate::governance::validator::GovernanceValidator::new`].
//!
//! For testing or advanced use, [`embedded_defaults`] returns the full default
//! set without any repository customisation.

use std::path::Path;

use crate::config::GovernanceConfig;
use crate::error::Result;

use super::enrichment;
use super::parser;
use super::rules::{EnforcementLevel, GovernanceRule, RuleSource};

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
// Markdown-based loading
// ---------------------------------------------------------------------------

/// Merges a set of parsed rules with embedded defaults.
///
/// Parsed rules take precedence: any embedded rule whose `id` already exists
/// in `parsed` is not added again.  This preserves repository-specific
/// customisations while ensuring all security-critical defaults are present.
fn merge_with_defaults(parsed: Vec<GovernanceRule>) -> RuleSet {
    let defaults = embedded_defaults();
    let existing_ids: std::collections::HashSet<String> =
        parsed.iter().map(|r| r.id.clone()).collect();
    let mut rules = parsed;
    for rule in defaults.rules {
        if !existing_ids.contains(rule.id.as_str()) {
            rules.push(rule);
        }
    }
    RuleSet::new(rules)
}

/// Reads an `AGENTS.md` file at `path`, parses it into governance rules, and
/// merges the result with embedded defaults.
///
/// On any read or parse failure, logs a warning via `tracing::warn!` and falls
/// back to `embedded_defaults()` — never returns an error.
fn load_from_agents_md(path: &Path) -> RuleSet {
    match std::fs::read_to_string(path) {
        Err(e) => {
            tracing::warn!(
                "governance: could not read '{}': {}; falling back to embedded defaults",
                path.display(),
                e
            );
            embedded_defaults()
        }
        Ok(content) => {
            let parsed = parser::parse_agents_md(&content);
            if parsed.is_empty() {
                tracing::warn!(
                    "governance: no rules found in '{}'; embedded defaults will be used",
                    path.display()
                );
            }
            merge_with_defaults(parsed)
        }
    }
}

/// Detects the primary programming language of the repository by checking for
/// well-known manifest files in the current working directory.
///
/// # Returns
///
/// A `&'static str` language identifier: `"rust"`, `"javascript"`, `"python"`,
/// or `"unknown"` when no manifest is recognised.
fn detect_primary_language() -> &'static str {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    if cwd.join("Cargo.toml").exists() {
        return "rust";
    }
    if cwd.join("package.json").exists() {
        return "javascript";
    }
    if cwd.join("pyproject.toml").exists() || cwd.join("setup.py").exists() {
        return "python";
    }
    "unknown"
}

/// Appends derived enrichment rules to `base` for the detected primary language.
///
/// Calls [`detect_primary_language`] and [`enrichment::derive_from_context`] to
/// obtain derived rules, then adds any rule whose `id` is not already present in
/// `base`.  This is only called when no repository `AGENTS.md` was found.
///
/// # Arguments
///
/// * `base` - The starting [`RuleSet`] to enrich (typically [`embedded_defaults`]).
///
/// # Returns
///
/// A new [`RuleSet`] containing all rules from `base` plus any non-duplicate
/// derived rules.
fn apply_derived_enrichment(base: RuleSet) -> RuleSet {
    let language = detect_primary_language();
    let derived = enrichment::derive_from_context(language, &[]);
    if derived.is_empty() {
        return base;
    }
    let existing_ids: std::collections::HashSet<String> =
        base.rules.iter().map(|r| r.id.clone()).collect();
    let mut rules = base.rules;
    for rule in derived {
        if !existing_ids.contains(rule.id.as_str()) {
            rules.push(rule);
        }
    }
    RuleSet::new(rules)
}

/// Loads rules according to [`GovernanceConfig`].
///
/// If `config.rules_path` is non-empty and the file exists, it is parsed as
/// an `AGENTS.md` Markdown file via [`parser::parse_agents_md`] and merged
/// with [`embedded_defaults`] (parsed rules take precedence by rule id).
/// When AGENTS.md is found, no derived enrichment is performed; the repository
/// file takes full precedence.
///
/// When no AGENTS.md is found (empty path or file does not exist), derived
/// enrichment rules (tagged [`RuleSource::Derived`]) are appended to the
/// embedded defaults via language auto-detection.
///
/// A read or parse failure is logged as a warning and falls back to
/// [`embedded_defaults`] with enrichment; the function never returns `Err`.
///
/// # Arguments
///
/// * `config` - The governance section of the pipeline configuration.
///
/// # Errors
///
/// This function always returns `Ok`.  The `Result` wrapper is kept for API
/// compatibility with existing callers.
///
/// # Examples
///
/// ```
/// use xzardgz::config::GovernanceConfig;
/// use xzardgz::governance::load_for_config;
///
/// // With an empty rules_path, returns embedded defaults plus derived rules.
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
        return Ok(apply_derived_enrichment(embedded_defaults()));
    }
    let path = Path::new(&config.rules_path);
    if path.exists() {
        Ok(load_from_agents_md(path))
    } else {
        Ok(apply_derived_enrichment(embedded_defaults()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
        // Embedded defaults are always present.
        let rule_set = result.unwrap();
        assert!(rule_set.has("governance.path.no_traversal"));
        // Derived enrichment is appended when no AGENTS.md is found.
        assert!(rule_set.len() >= embedded_defaults().len());
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
        let rule_set = result.unwrap();
        // Embedded defaults must be present.
        assert!(rule_set.has("governance.path.no_traversal"));
        // Derived enrichment is appended for the no-AGENTS.md case.
        assert!(rule_set.len() >= embedded_defaults().len());
    }

    // ------------------------------------------------------------------
    // load_from_agents_md / load_for_config (Markdown-based)
    // ------------------------------------------------------------------

    fn write_temp_agents_md(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        // SAFETY: TempDir::new() only fails on OS error; not expected in tests.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("AGENTS.md");
        // SAFETY: writing to a freshly created TempDir cannot fail under normal conditions.
        std::fs::write(&path, content).unwrap();
        (tmp, path)
    }

    #[test]
    fn test_load_from_agents_md_with_valid_file_returns_rules_plus_defaults() {
        let content = "## My Rules\n\n- You MUST follow rule one.\n- Follow rule two.\n";
        let (_tmp, path) = write_temp_agents_md(content);

        // load_from_agents_md is private; test via load_for_config.
        let config = GovernanceConfig {
            enabled: true,
            rules_path: path.to_str().unwrap().to_string(),
            fail_on_violation: false,
        };
        let result = load_for_config(&config);
        assert!(result.is_ok());
        let rule_set = result.unwrap();
        // Parsed rules + embedded defaults.
        assert!(rule_set.len() > embedded_defaults().len());
        // Embedded defaults must still be present.
        assert!(rule_set.has("governance.path.no_traversal"));
    }

    #[test]
    fn test_load_for_config_with_agents_md_parsed_rules_have_correct_ids() {
        let content = "## Security\n\n- You MUST use HTTPS.\n";
        let (_tmp, path) = write_temp_agents_md(content);

        let config = GovernanceConfig {
            enabled: true,
            rules_path: path.to_str().unwrap().to_string(),
            fail_on_violation: false,
        };
        let rule_set = load_for_config(&config).unwrap();
        assert!(
            rule_set.has("agents_md.security.1"),
            "parsed rule must be present"
        );
        let rule = rule_set.get("agents_md.security.1").unwrap();
        assert_eq!(rule.enforcement, EnforcementLevel::Required);
    }

    #[test]
    fn test_load_from_agents_md_with_empty_file_returns_defaults_only() {
        let (_tmp, path) = write_temp_agents_md("");
        let config = GovernanceConfig {
            enabled: true,
            rules_path: path.to_str().unwrap().to_string(),
            fail_on_violation: false,
        };
        let result = load_for_config(&config);
        assert!(result.is_ok());
        // Empty AGENTS.md -> only embedded defaults.
        assert_eq!(result.unwrap().len(), embedded_defaults().len());
    }

    // ------------------------------------------------------------------
    // Phase 2: Derived enrichment
    // ------------------------------------------------------------------

    #[test]
    fn test_load_for_config_with_agents_md_has_no_derived_rules() {
        // A repository that has an AGENTS.md must NOT receive derived enrichment.
        let content = "## Rules\n\n- You MUST follow rule one.\n";
        let (_tmp, path) = write_temp_agents_md(content);
        let config = GovernanceConfig {
            enabled: true,
            rules_path: path.to_str().unwrap().to_string(),
            fail_on_violation: false,
        };
        let rule_set = load_for_config(&config).unwrap();
        let derived_count = rule_set
            .rules
            .iter()
            .filter(|r| matches!(r.source, RuleSource::Derived))
            .count();
        assert_eq!(
            derived_count, 0,
            "repository with AGENTS.md must not receive derived enrichment"
        );
    }

    #[test]
    fn test_load_for_config_without_agents_md_has_derived_rules() {
        // A repository without an AGENTS.md must receive embedded defaults + derived enrichment.
        let config = GovernanceConfig {
            enabled: true,
            rules_path: String::new(),
            fail_on_violation: false,
        };
        let rule_set = load_for_config(&config).unwrap();
        let derived_count = rule_set
            .rules
            .iter()
            .filter(|r| matches!(r.source, RuleSource::Derived))
            .count();
        // Running in a Rust project (Cargo.toml present) so Rust derived rules must be appended.
        assert!(
            derived_count > 0,
            "repository without AGENTS.md must receive derived enrichment rules"
        );
        // Embedded defaults must still be present.
        assert!(rule_set.has("governance.path.no_traversal"));
        // Total count must exceed embedded defaults alone.
        assert!(rule_set.len() > embedded_defaults().len());
    }

    #[test]
    fn test_load_for_config_without_agents_md_embedded_rules_all_present() {
        // Verify every embedded default rule survives enrichment.
        let config = GovernanceConfig {
            enabled: true,
            rules_path: String::new(),
            fail_on_violation: false,
        };
        let rule_set = load_for_config(&config).unwrap();
        for id in &[
            "governance.path.no_traversal",
            "governance.path.no_null",
            "governance.plugin.valid_name",
            "governance.event.known_type",
            "governance.endpoint.require_https",
            "governance.content.no_secrets_pattern",
            "governance.branch.safe_pattern",
            "governance.workspace.no_traversal",
            "governance.output.no_traversal",
            "governance.report.no_traversal",
        ] {
            assert!(
                rule_set.has(id),
                "embedded rule {} must survive enrichment",
                id
            );
        }
    }

    #[test]
    fn test_load_for_config_nonexistent_path_has_derived_rules() {
        // Nonexistent AGENTS.md also triggers enrichment.
        let config = GovernanceConfig {
            enabled: true,
            rules_path: "/tmp/xzardgz_does_not_exist_phase2.md".to_string(),
            fail_on_violation: false,
        };
        let rule_set = load_for_config(&config).unwrap();
        let derived_count = rule_set
            .rules
            .iter()
            .filter(|r| matches!(r.source, RuleSource::Derived))
            .count();
        assert!(
            derived_count > 0,
            "nonexistent AGENTS.md path must trigger derived enrichment"
        );
    }

    #[test]
    fn test_load_for_config_with_empty_agents_md_has_no_derived_rules() {
        // An empty AGENTS.md is still a found file; derived enrichment must NOT be added.
        let (_tmp, path) = write_temp_agents_md("");
        let config = GovernanceConfig {
            enabled: true,
            rules_path: path.to_str().unwrap().to_string(),
            fail_on_violation: false,
        };
        let rule_set = load_for_config(&config).unwrap();
        let derived_count = rule_set
            .rules
            .iter()
            .filter(|r| matches!(r.source, RuleSource::Derived))
            .count();
        assert_eq!(
            derived_count, 0,
            "empty but present AGENTS.md must not trigger derived enrichment"
        );
    }

    #[test]
    fn test_load_for_config_with_real_agents_md_returns_rules_and_defaults() {
        // Live fixture: load the real AGENTS.md of this repository.
        // CARGO_MANIFEST_DIR is set by cargo during test runs.
        // SAFETY: CARGO_MANIFEST_DIR is always set by cargo; unwrap is safe in tests.
        let manifest = std::env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR not set; this test must be run via cargo test");
        let agents_md = std::path::Path::new(&manifest).join("AGENTS.md");
        if !agents_md.exists() {
            // Skip if AGENTS.md is not present in this build environment.
            return;
        }
        let config = GovernanceConfig {
            enabled: true,
            // SAFETY: agents_md was just confirmed to exist; to_str() is safe for valid UTF-8 paths.
            rules_path: agents_md.to_str().unwrap().to_string(),
            fail_on_violation: false,
        };
        let result = load_for_config(&config);
        assert!(
            result.is_ok(),
            "load_for_config must not fail on real AGENTS.md"
        );
        let rule_set = result.unwrap();
        // Must contain at least the embedded defaults.
        assert!(
            rule_set.has("governance.path.no_traversal"),
            "embedded defaults must be merged in"
        );
        // Must have MORE rules than embedded defaults alone (parsed rules are present).
        assert!(
            rule_set.len() > embedded_defaults().len(),
            "real AGENTS.md must contribute at least one rule beyond embedded defaults"
        );
    }
}
