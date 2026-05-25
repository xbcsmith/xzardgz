//! Validation logic that applies a [`RuleSet`] to concrete pipeline inputs.
//!
//! [`GovernanceValidator`] is the workhorse of the governance system.  It
//! exposes one method per class of pipeline input (branch names, file paths,
//! plugin names, etc.) and returns a [`GovernanceResult`] for each call.
//!
//! All pattern-matching is implemented with plain Rust string operations —
//! no external regex crate is used.

use super::loader::RuleSet;
use super::rules::{GovernanceResult, GovernanceViolation};

// ---------------------------------------------------------------------------
// GovernanceValidator
// ---------------------------------------------------------------------------

/// Governance validator that applies a [`RuleSet`] to pipeline inputs.
///
/// Create an instance via [`GovernanceValidator::new`] and call the
/// `validate_*` family of methods.  The validator itself is stateless beyond
/// its `rules` field; it can be called any number of times.
///
/// High-level code usually goes through [`crate::governance::GovernanceChecker`]
/// which wraps a `GovernanceValidator` with config-level flags.
///
/// # Examples
///
/// ```
/// use xzardgz::governance::{GovernanceValidator, embedded_defaults};
///
/// let validator = GovernanceValidator::new(embedded_defaults());
///
/// let result = validator.validate_file_path("src/main.rs");
/// assert!(result.is_ok(), "safe path should have no violations");
///
/// let result = validator.validate_file_path("../../etc/passwd");
/// assert!(!result.is_ok(), "traversal path should have violations");
/// ```
pub struct GovernanceValidator {
    rules: RuleSet,
}

impl GovernanceValidator {
    /// Creates a new validator from the provided rule set.
    ///
    /// # Arguments
    ///
    /// * `rules` - The active [`RuleSet`] to enforce.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceValidator, embedded_defaults};
    ///
    /// let validator = GovernanceValidator::new(embedded_defaults());
    /// ```
    pub fn new(rules: RuleSet) -> Self {
        Self { rules }
    }

    /// Returns a reference to the underlying [`RuleSet`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceValidator, embedded_defaults};
    ///
    /// let validator = GovernanceValidator::new(embedded_defaults());
    /// assert!(!validator.rules().is_empty());
    /// ```
    pub fn rules(&self) -> &RuleSet {
        &self.rules
    }

    /// Validates that a branch name matches safe naming patterns.
    ///
    /// Safe branch names are `main`, `master`, or `develop`, or any name that
    /// begins with one of the approved prefixes followed by a non-empty suffix:
    /// `feature/`, `fix/`, `hotfix/`, `release/`, `chore/`, `refactor/`,
    /// `test/`, `ci/`, `docs/`.
    ///
    /// The check only fires when the rule `governance.branch.safe_pattern` is
    /// present in the active rule set; if the rule has been disabled the result
    /// is always empty.
    ///
    /// # Arguments
    ///
    /// * `name` - The branch name to validate.
    ///
    /// # Returns
    ///
    /// A [`GovernanceResult`] containing a violation when the name does not
    /// match any safe pattern.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceValidator, embedded_defaults};
    ///
    /// let validator = GovernanceValidator::new(embedded_defaults());
    ///
    /// assert!(validator.validate_branch_name("main").is_ok());
    /// assert!(validator.validate_branch_name("feature/add-login").is_ok());
    /// assert!(!validator.validate_branch_name("ALLCAPS").is_ok());
    /// ```
    pub fn validate_branch_name(&self, name: &str) -> GovernanceResult {
        let mut result = GovernanceResult::new();
        if let Some(rule) = self.rules.get("governance.branch.safe_pattern")
            && !is_safe_branch_name(name)
        {
            result.violations.push(GovernanceViolation::new(
                rule,
                name,
                format!(
                    "branch name '{}' does not match safe patterns; allowed names: \
                     main, master, develop, or names prefixed with feature/, fix/, \
                     hotfix/, release/, chore/, refactor/, test/, ci/, docs/ \
                     (suffix after '/' must be non-empty)",
                    name
                ),
            ));
        }
        result
    }

    /// Validates a file path for path-traversal sequences and null bytes.
    ///
    /// Two rules are checked:
    /// - `governance.path.no_traversal`: path components must not equal `..`.
    /// - `governance.path.no_null`: the path must not contain null bytes (`\0`).
    ///
    /// Backslashes are normalised to forward slashes before checking for
    /// traversal so that Windows-style paths are handled correctly.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path string to validate.
    ///
    /// # Returns
    ///
    /// A [`GovernanceResult`] with one violation per triggered rule.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceValidator, embedded_defaults};
    ///
    /// let validator = GovernanceValidator::new(embedded_defaults());
    ///
    /// assert!(validator.validate_file_path("src/lib.rs").is_ok());
    /// assert!(!validator.validate_file_path("../etc/passwd").is_ok());
    /// ```
    pub fn validate_file_path(&self, path: &str) -> GovernanceResult {
        let mut result = GovernanceResult::new();

        if let Some(rule) = self.rules.get("governance.path.no_traversal")
            && has_path_traversal(path)
        {
            result.violations.push(GovernanceViolation::new(
                rule,
                path,
                format!(
                    "file path '{}' contains path traversal sequences (..)",
                    path
                ),
            ));
        }

        if let Some(rule) = self.rules.get("governance.path.no_null")
            && path.contains('\0')
        {
            result.violations.push(GovernanceViolation::new(
                rule,
                path.replace('\0', "<null>"),
                "file path contains null bytes",
            ));
        }

        result
    }

    /// Validates an output directory or file path for path-traversal sequences.
    ///
    /// Applies rule `governance.output.no_traversal`.
    ///
    /// # Arguments
    ///
    /// * `path` - The output path to validate.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceValidator, embedded_defaults};
    ///
    /// let validator = GovernanceValidator::new(embedded_defaults());
    ///
    /// assert!(validator.validate_output_path("output/reports").is_ok());
    /// assert!(!validator.validate_output_path("../output").is_ok());
    /// ```
    pub fn validate_output_path(&self, path: &str) -> GovernanceResult {
        let mut result = GovernanceResult::new();
        if let Some(rule) = self.rules.get("governance.output.no_traversal")
            && has_path_traversal(path)
        {
            result.violations.push(GovernanceViolation::new(
                rule,
                path,
                format!(
                    "output path '{}' contains path traversal sequences (..)",
                    path
                ),
            ));
        }
        result
    }

    /// Validates a report output path for path-traversal sequences.
    ///
    /// Applies rule `governance.report.no_traversal`.
    ///
    /// # Arguments
    ///
    /// * `path` - The report path to validate.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceValidator, embedded_defaults};
    ///
    /// let validator = GovernanceValidator::new(embedded_defaults());
    ///
    /// assert!(validator.validate_report_path("reports/summary.json").is_ok());
    /// assert!(!validator.validate_report_path("../reports").is_ok());
    /// ```
    pub fn validate_report_path(&self, path: &str) -> GovernanceResult {
        let mut result = GovernanceResult::new();
        if let Some(rule) = self.rules.get("governance.report.no_traversal")
            && has_path_traversal(path)
        {
            result.violations.push(GovernanceViolation::new(
                rule,
                path,
                format!(
                    "report path '{}' contains path traversal sequences (..)",
                    path
                ),
            ));
        }
        result
    }

    /// Validates a workspace root path for path-traversal sequences.
    ///
    /// Applies rule `governance.workspace.no_traversal`.
    ///
    /// # Arguments
    ///
    /// * `path` - The workspace path to validate.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceValidator, embedded_defaults};
    ///
    /// let validator = GovernanceValidator::new(embedded_defaults());
    ///
    /// assert!(validator.validate_workspace_path("/tmp/workspace").is_ok());
    /// assert!(!validator.validate_workspace_path("../../workspace").is_ok());
    /// ```
    pub fn validate_workspace_path(&self, path: &str) -> GovernanceResult {
        let mut result = GovernanceResult::new();
        if let Some(rule) = self.rules.get("governance.workspace.no_traversal")
            && has_path_traversal(path)
        {
            result.violations.push(GovernanceViolation::new(
                rule,
                path,
                format!(
                    "workspace path '{}' contains path traversal sequences (..)",
                    path
                ),
            ));
        }
        result
    }

    /// Validates content for patterns that resemble embedded secrets.
    ///
    /// Applies rule `governance.content.no_secrets_pattern`.
    ///
    /// Detection is case-insensitive for assignment-style patterns
    /// (`password=`, `api_key=`, etc.) and exact-case for PEM headers
    /// (`BEGIN RSA PRIVATE KEY`, etc.).
    ///
    /// Detected patterns:
    /// - `password=`, `passwd=`, `secret=`, `api_key=`, `apikey=`
    /// - `access_token=`, `auth_token=`, `private_key=`
    /// - `-----BEGIN RSA PRIVATE KEY-----`
    /// - `-----BEGIN OPENSSH PRIVATE KEY-----`
    /// - `-----BEGIN EC PRIVATE KEY-----`
    /// - `-----BEGIN PRIVATE KEY-----`
    ///
    /// # Arguments
    ///
    /// * `content` - The text content to scan.
    ///
    /// # Returns
    ///
    /// A [`GovernanceResult`] with a single violation when a pattern is found.
    /// The offending value is replaced with `"(content redacted)"` to avoid
    /// leaking secret material into diagnostic output.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceValidator, embedded_defaults};
    ///
    /// let validator = GovernanceValidator::new(embedded_defaults());
    ///
    /// assert!(validator.validate_content_safety("normal log entry").is_ok());
    /// assert!(!validator.validate_content_safety("password=hunter2").is_ok());
    /// ```
    pub fn validate_content_safety(&self, content: &str) -> GovernanceResult {
        let mut result = GovernanceResult::new();
        if let Some(rule) = self.rules.get("governance.content.no_secrets_pattern")
            && has_secret_pattern(content)
        {
            result.violations.push(GovernanceViolation::new(
                rule,
                "(content redacted)",
                "content appears to contain secret patterns (credentials, keys, or tokens)",
            ));
        }
        result
    }

    /// Validates a plugin name against the required identifier format.
    ///
    /// Applies rule `governance.plugin.valid_name`.
    ///
    /// A valid plugin name:
    /// - Is non-empty.
    /// - Starts with a lowercase ASCII letter (`a`-`z`).
    /// - Contains only lowercase ASCII letters, ASCII digits (`0`-`9`), or
    ///   underscores (`_`).
    ///
    /// # Arguments
    ///
    /// * `name` - The plugin name to validate.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceValidator, embedded_defaults};
    ///
    /// let validator = GovernanceValidator::new(embedded_defaults());
    ///
    /// assert!(validator.validate_plugin_name("my_plugin").is_ok());
    /// assert!(validator.validate_plugin_name("plugin1").is_ok());
    /// assert!(!validator.validate_plugin_name("MyPlugin").is_ok());
    /// assert!(!validator.validate_plugin_name("1plugin").is_ok());
    /// assert!(!validator.validate_plugin_name("").is_ok());
    /// ```
    pub fn validate_plugin_name(&self, name: &str) -> GovernanceResult {
        let mut result = GovernanceResult::new();
        if let Some(rule) = self.rules.get("governance.plugin.valid_name")
            && !is_valid_plugin_name(name)
        {
            result.violations.push(GovernanceViolation::new(
                rule,
                name,
                format!(
                    "plugin name '{}' is not a valid identifier; names must start with a \
                     lowercase letter and contain only lowercase letters, digits, or underscores",
                    name
                ),
            ));
        }
        result
    }

    /// Validates an event type against the approved list.
    ///
    /// Applies rule `governance.event.known_type`.
    ///
    /// Approved event types: `push`, `pull_request`, `issue`, `release`,
    /// `schedule`, `workflow_dispatch`, `tag`, `commit`, `merge`.
    ///
    /// # Arguments
    ///
    /// * `event_type` - The event type string to validate.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceValidator, embedded_defaults};
    ///
    /// let validator = GovernanceValidator::new(embedded_defaults());
    ///
    /// assert!(validator.validate_event_type("push").is_ok());
    /// assert!(validator.validate_event_type("pull_request").is_ok());
    /// assert!(!validator.validate_event_type("custom_event").is_ok());
    /// ```
    pub fn validate_event_type(&self, event_type: &str) -> GovernanceResult {
        const KNOWN_TYPES: &[&str] = &[
            "push",
            "pull_request",
            "issue",
            "release",
            "schedule",
            "workflow_dispatch",
            "tag",
            "commit",
            "merge",
        ];

        let mut result = GovernanceResult::new();
        if let Some(rule) = self.rules.get("governance.event.known_type")
            && !KNOWN_TYPES.contains(&event_type)
        {
            result.violations.push(GovernanceViolation::new(
                rule,
                event_type,
                format!(
                    "event type '{}' is not in the approved list: {}",
                    event_type,
                    KNOWN_TYPES.join(", ")
                ),
            ));
        }
        result
    }

    /// Validates a provider endpoint URL, requiring HTTPS.
    ///
    /// Applies rule `governance.endpoint.require_https`.
    ///
    /// The check is case-insensitive: `HTTPS://...` passes, `HTTP://...`
    /// fails.  An empty `endpoint` string skips validation entirely (the
    /// endpoint is considered absent rather than invalid).
    ///
    /// # Arguments
    ///
    /// * `endpoint` - The provider endpoint URL to validate.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::governance::{GovernanceValidator, embedded_defaults};
    ///
    /// let validator = GovernanceValidator::new(embedded_defaults());
    ///
    /// // Empty endpoint is skipped.
    /// assert!(validator.validate_provider_endpoint("").is_ok());
    ///
    /// // HTTPS passes (case-insensitive).
    /// assert!(validator.validate_provider_endpoint("https://api.example.com").is_ok());
    ///
    /// // HTTP fails.
    /// assert!(!validator.validate_provider_endpoint("http://api.example.com").is_ok());
    /// ```
    pub fn validate_provider_endpoint(&self, endpoint: &str) -> GovernanceResult {
        let mut result = GovernanceResult::new();
        if endpoint.is_empty() {
            return result;
        }
        if let Some(rule) = self.rules.get("governance.endpoint.require_https")
            && !endpoint.to_lowercase().starts_with("https://")
        {
            result.violations.push(GovernanceViolation::new(
                rule,
                endpoint,
                format!(
                    "provider endpoint '{}' does not use HTTPS; \
                     only https:// endpoints are permitted",
                    endpoint
                ),
            ));
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Returns `true` when `path` contains a `..` traversal component.
///
/// Backslashes are normalised to forward slashes before splitting so that
/// Windows-style paths such as `foo\..\..\bar` are handled correctly.
fn has_path_traversal(path: &str) -> bool {
    path.replace('\\', "/").split('/').any(|c| c == "..")
}

/// Returns `true` when `name` is a valid plugin identifier.
///
/// Valid: starts with a lowercase ASCII letter, followed by zero or more
/// lowercase ASCII letters, ASCII digits, or underscores.  The empty string
/// is invalid.
fn is_valid_plugin_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    // SAFETY: we just checked name is non-empty so next() is guaranteed to
    // return Some.
    let first = chars.next().unwrap();
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Returns `true` when `name` is a known-safe branch name.
///
/// Exact matches: `main`, `master`, `develop`.
/// Prefix matches (suffix must be non-empty): `feature/`, `fix/`, `hotfix/`,
/// `release/`, `chore/`, `refactor/`, `test/`, `ci/`, `docs/`.
fn is_safe_branch_name(name: &str) -> bool {
    const EXACT: &[&str] = &["main", "master", "develop"];
    const PREFIXES: &[&str] = &[
        "feature/",
        "fix/",
        "hotfix/",
        "release/",
        "chore/",
        "refactor/",
        "test/",
        "ci/",
        "docs/",
    ];

    if EXACT.contains(&name) {
        return true;
    }
    for prefix in PREFIXES {
        if let Some(suffix) = name.strip_prefix(prefix)
            && !suffix.is_empty()
        {
            return true;
        }
    }
    false
}

/// Returns `true` when `content` contains any recognised secret pattern.
///
/// Case-insensitive assignment patterns: `password=`, `passwd=`, `secret=`,
/// `api_key=`, `apikey=`, `access_token=`, `auth_token=`, `private_key=`.
///
/// Exact-case PEM headers: `BEGIN RSA PRIVATE KEY`, `BEGIN OPENSSH PRIVATE
/// KEY`, `BEGIN EC PRIVATE KEY`, `BEGIN PRIVATE KEY`.
fn has_secret_pattern(content: &str) -> bool {
    let lower = content.to_lowercase();
    const ASSIGNMENT_PATTERNS: &[&str] = &[
        "password=",
        "passwd=",
        "secret=",
        "api_key=",
        "apikey=",
        "access_token=",
        "auth_token=",
        "private_key=",
    ];
    for pattern in ASSIGNMENT_PATTERNS {
        if lower.contains(pattern) {
            return true;
        }
    }

    // PEM headers are checked with exact case.
    const PEM_PATTERNS: &[&str] = &[
        "BEGIN RSA PRIVATE KEY",
        "BEGIN OPENSSH PRIVATE KEY",
        "BEGIN EC PRIVATE KEY",
        "BEGIN PRIVATE KEY",
    ];
    for pattern in PEM_PATTERNS {
        if content.contains(pattern) {
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::loader::embedded_defaults;

    fn validator() -> GovernanceValidator {
        GovernanceValidator::new(embedded_defaults())
    }

    // ------------------------------------------------------------------
    // Private helper: has_path_traversal
    // ------------------------------------------------------------------

    #[test]
    fn test_has_path_traversal_with_unix_dotdot_returns_true() {
        assert!(has_path_traversal("../etc/passwd"));
    }

    #[test]
    fn test_has_path_traversal_with_embedded_dotdot_returns_true() {
        assert!(has_path_traversal("a/b/../c"));
    }

    #[test]
    fn test_has_path_traversal_with_root_relative_dotdot_returns_true() {
        assert!(has_path_traversal(".."));
    }

    #[test]
    fn test_has_path_traversal_with_windows_backslash_traversal_returns_true() {
        assert!(has_path_traversal("a\\..\\b"));
    }

    #[test]
    fn test_has_path_traversal_with_safe_path_returns_false() {
        assert!(!has_path_traversal("src/main.rs"));
    }

    #[test]
    fn test_has_path_traversal_with_dotdot_in_name_not_component_returns_false() {
        // "a/..b/c" has component "..b" which is not exactly ".."
        assert!(!has_path_traversal("a/..b/c"));
        // "a/b../c" has component "b.." which is not exactly ".."
        assert!(!has_path_traversal("a/b../c"));
    }

    #[test]
    fn test_has_path_traversal_with_empty_string_returns_false() {
        assert!(!has_path_traversal(""));
    }

    // ------------------------------------------------------------------
    // Private helper: is_valid_plugin_name
    // ------------------------------------------------------------------

    #[test]
    fn test_is_valid_plugin_name_with_simple_lowercase_returns_true() {
        assert!(is_valid_plugin_name("myplugin"));
    }

    #[test]
    fn test_is_valid_plugin_name_with_underscore_returns_true() {
        assert!(is_valid_plugin_name("my_plugin"));
    }

    #[test]
    fn test_is_valid_plugin_name_with_trailing_digits_returns_true() {
        assert!(is_valid_plugin_name("plugin1"));
    }

    #[test]
    fn test_is_valid_plugin_name_with_single_char_returns_true() {
        assert!(is_valid_plugin_name("a"));
    }

    #[test]
    fn test_is_valid_plugin_name_with_empty_string_returns_false() {
        assert!(!is_valid_plugin_name(""));
    }

    #[test]
    fn test_is_valid_plugin_name_with_uppercase_returns_false() {
        assert!(!is_valid_plugin_name("MyPlugin"));
    }

    #[test]
    fn test_is_valid_plugin_name_with_digit_first_char_returns_false() {
        assert!(!is_valid_plugin_name("1plugin"));
    }

    #[test]
    fn test_is_valid_plugin_name_with_hyphen_returns_false() {
        assert!(!is_valid_plugin_name("my-plugin"));
    }

    #[test]
    fn test_is_valid_plugin_name_with_uppercase_in_middle_returns_false() {
        assert!(!is_valid_plugin_name("myPlugin"));
    }

    // ------------------------------------------------------------------
    // Private helper: has_secret_pattern
    // ------------------------------------------------------------------

    #[test]
    fn test_has_secret_pattern_with_lowercase_password_assignment_returns_true() {
        assert!(has_secret_pattern("password=hunter2"));
    }

    #[test]
    fn test_has_secret_pattern_with_uppercase_password_assignment_returns_true() {
        assert!(has_secret_pattern("PASSWORD=hunter2"));
    }

    #[test]
    fn test_has_secret_pattern_with_api_key_assignment_returns_true() {
        assert!(has_secret_pattern("api_key=abc123"));
    }

    #[test]
    fn test_has_secret_pattern_with_access_token_assignment_returns_true() {
        assert!(has_secret_pattern("access_token=tok_live_abc"));
    }

    #[test]
    fn test_has_secret_pattern_with_auth_token_returns_true() {
        assert!(has_secret_pattern("AUTH_TOKEN=mysecret"));
    }

    #[test]
    fn test_has_secret_pattern_with_private_key_assignment_returns_true() {
        assert!(has_secret_pattern("private_key=xyz"));
    }

    #[test]
    fn test_has_secret_pattern_with_pem_rsa_header_returns_true() {
        assert!(has_secret_pattern(
            "-----BEGIN RSA PRIVATE KEY-----\nMIIE..."
        ));
    }

    #[test]
    fn test_has_secret_pattern_with_pem_openssh_header_returns_true() {
        assert!(has_secret_pattern("-----BEGIN OPENSSH PRIVATE KEY-----"));
    }

    #[test]
    fn test_has_secret_pattern_with_pem_ec_header_returns_true() {
        assert!(has_secret_pattern("-----BEGIN EC PRIVATE KEY-----"));
    }

    #[test]
    fn test_has_secret_pattern_with_pem_generic_header_returns_true() {
        assert!(has_secret_pattern("-----BEGIN PRIVATE KEY-----"));
    }

    #[test]
    fn test_has_secret_pattern_with_lowercase_pem_header_returns_false() {
        // PEM headers are checked with exact case.
        assert!(!has_secret_pattern("-----begin rsa private key-----"));
    }

    #[test]
    fn test_has_secret_pattern_with_safe_content_returns_false() {
        assert!(!has_secret_pattern("Hello, world! This is safe text."));
    }

    #[test]
    fn test_has_secret_pattern_with_empty_content_returns_false() {
        assert!(!has_secret_pattern(""));
    }

    // ------------------------------------------------------------------
    // Private helper: is_safe_branch_name
    // ------------------------------------------------------------------

    #[test]
    fn test_is_safe_branch_name_main_returns_true() {
        assert!(is_safe_branch_name("main"));
    }

    #[test]
    fn test_is_safe_branch_name_master_returns_true() {
        assert!(is_safe_branch_name("master"));
    }

    #[test]
    fn test_is_safe_branch_name_develop_returns_true() {
        assert!(is_safe_branch_name("develop"));
    }

    #[test]
    fn test_is_safe_branch_name_feature_prefix_with_suffix_returns_true() {
        assert!(is_safe_branch_name("feature/add-login"));
    }

    #[test]
    fn test_is_safe_branch_name_fix_prefix_with_suffix_returns_true() {
        assert!(is_safe_branch_name("fix/null-pointer"));
    }

    #[test]
    fn test_is_safe_branch_name_hotfix_prefix_returns_true() {
        assert!(is_safe_branch_name("hotfix/urgent-patch"));
    }

    #[test]
    fn test_is_safe_branch_name_release_prefix_returns_true() {
        assert!(is_safe_branch_name("release/1.0.0"));
    }

    #[test]
    fn test_is_safe_branch_name_chore_prefix_returns_true() {
        assert!(is_safe_branch_name("chore/update-deps"));
    }

    #[test]
    fn test_is_safe_branch_name_refactor_prefix_returns_true() {
        assert!(is_safe_branch_name("refactor/extract-service"));
    }

    #[test]
    fn test_is_safe_branch_name_test_prefix_returns_true() {
        assert!(is_safe_branch_name("test/integration-tests"));
    }

    #[test]
    fn test_is_safe_branch_name_ci_prefix_returns_true() {
        assert!(is_safe_branch_name("ci/add-workflow"));
    }

    #[test]
    fn test_is_safe_branch_name_docs_prefix_returns_true() {
        assert!(is_safe_branch_name("docs/update-readme"));
    }

    #[test]
    fn test_is_safe_branch_name_feature_without_suffix_returns_false() {
        assert!(!is_safe_branch_name("feature/"));
    }

    #[test]
    fn test_is_safe_branch_name_uppercase_returns_false() {
        assert!(!is_safe_branch_name("MAIN"));
    }

    #[test]
    fn test_is_safe_branch_name_arbitrary_name_returns_false() {
        assert!(!is_safe_branch_name("my-custom-branch"));
    }

    #[test]
    fn test_is_safe_branch_name_empty_returns_false() {
        assert!(!is_safe_branch_name(""));
    }

    // ------------------------------------------------------------------
    // validate_branch_name
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_branch_name_with_main_returns_no_violations() {
        let v = validator();
        assert!(v.validate_branch_name("main").is_ok());
    }

    #[test]
    fn test_validate_branch_name_with_develop_returns_no_violations() {
        let v = validator();
        assert!(v.validate_branch_name("develop").is_ok());
    }

    #[test]
    fn test_validate_branch_name_with_feature_prefix_returns_no_violations() {
        let v = validator();
        assert!(v.validate_branch_name("feature/my-feature").is_ok());
    }

    #[test]
    fn test_validate_branch_name_with_uppercase_returns_violation() {
        let v = validator();
        let result = v.validate_branch_name("MAIN");
        assert!(!result.is_ok());
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_validate_branch_name_with_empty_prefix_suffix_returns_violation() {
        let v = validator();
        let result = v.validate_branch_name("feature/");
        assert!(!result.is_ok());
    }

    #[test]
    fn test_validate_branch_name_with_arbitrary_name_returns_violation() {
        let v = validator();
        let result = v.validate_branch_name("my-random-branch");
        assert!(!result.is_ok());
    }

    // ------------------------------------------------------------------
    // validate_file_path
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_file_path_with_safe_path_returns_no_violations() {
        let v = validator();
        assert!(v.validate_file_path("src/main.rs").is_ok());
    }

    #[test]
    fn test_validate_file_path_with_unix_traversal_returns_violation() {
        let v = validator();
        let result = v.validate_file_path("../etc/passwd");
        assert!(!result.is_ok());
        assert!(result.has_blocking_violations());
    }

    #[test]
    fn test_validate_file_path_with_embedded_traversal_returns_violation() {
        let v = validator();
        let result = v.validate_file_path("a/b/../c");
        assert!(!result.is_ok());
    }

    #[test]
    fn test_validate_file_path_with_windows_traversal_returns_violation() {
        let v = validator();
        let result = v.validate_file_path("a\\..\\b");
        assert!(!result.is_ok());
    }

    #[test]
    fn test_validate_file_path_with_null_byte_returns_violation() {
        let v = validator();
        let result = v.validate_file_path("file\0name.txt");
        assert!(!result.is_ok());
        assert!(result.has_blocking_violations());
    }

    #[test]
    fn test_validate_file_path_with_traversal_and_null_returns_two_violations() {
        let v = validator();
        let result = v.validate_file_path("../etc/\0passwd");
        assert_eq!(
            result.len(),
            2,
            "should have both traversal and null violations"
        );
    }

    #[test]
    fn test_validate_file_path_with_empty_string_returns_no_violations() {
        let v = validator();
        assert!(v.validate_file_path("").is_ok());
    }

    // ------------------------------------------------------------------
    // validate_output_path
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_output_path_with_safe_path_returns_no_violations() {
        let v = validator();
        assert!(v.validate_output_path("output/reports").is_ok());
    }

    #[test]
    fn test_validate_output_path_with_traversal_returns_violation() {
        let v = validator();
        let result = v.validate_output_path("../output");
        assert!(!result.is_ok());
        assert!(result.has_blocking_violations());
    }

    // ------------------------------------------------------------------
    // validate_report_path
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_report_path_with_safe_path_returns_no_violations() {
        let v = validator();
        assert!(v.validate_report_path("reports/summary.json").is_ok());
    }

    #[test]
    fn test_validate_report_path_with_traversal_returns_violation() {
        let v = validator();
        let result = v.validate_report_path("../reports");
        assert!(!result.is_ok());
        assert!(result.has_blocking_violations());
    }

    // ------------------------------------------------------------------
    // validate_workspace_path
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_workspace_path_with_safe_path_returns_no_violations() {
        let v = validator();
        assert!(v.validate_workspace_path("/tmp/workspace").is_ok());
    }

    #[test]
    fn test_validate_workspace_path_with_traversal_returns_violation() {
        let v = validator();
        let result = v.validate_workspace_path("../../workspace");
        assert!(!result.is_ok());
        assert!(result.has_blocking_violations());
    }

    // ------------------------------------------------------------------
    // validate_content_safety
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_content_safety_with_safe_content_returns_no_violations() {
        let v = validator();
        assert!(v.validate_content_safety("Just a normal log line.").is_ok());
    }

    #[test]
    fn test_validate_content_safety_with_password_pattern_returns_violation() {
        let v = validator();
        let result = v.validate_content_safety("password=supersecret");
        assert!(!result.is_ok());
    }

    #[test]
    fn test_validate_content_safety_with_api_key_pattern_returns_violation() {
        let v = validator();
        let result = v.validate_content_safety("api_key=sk-live-abc123");
        assert!(!result.is_ok());
    }

    #[test]
    fn test_validate_content_safety_with_pem_header_returns_violation() {
        let v = validator();
        let result = v.validate_content_safety("-----BEGIN RSA PRIVATE KEY-----\nMIIE...");
        assert!(!result.is_ok());
    }

    #[test]
    fn test_validate_content_safety_with_empty_content_returns_no_violations() {
        let v = validator();
        assert!(v.validate_content_safety("").is_ok());
    }

    #[test]
    fn test_validate_content_safety_violation_value_is_redacted() {
        let v = validator();
        let result = v.validate_content_safety("secret=my_secret_value");
        assert!(!result.is_ok());
        let violation = &result.violations[0];
        assert_eq!(
            violation.value, "(content redacted)",
            "secret content must not appear in violation value"
        );
    }

    // ------------------------------------------------------------------
    // validate_plugin_name
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_plugin_name_with_valid_name_returns_no_violations() {
        let v = validator();
        assert!(v.validate_plugin_name("my_plugin").is_ok());
    }

    #[test]
    fn test_validate_plugin_name_with_digits_in_name_returns_no_violations() {
        let v = validator();
        assert!(v.validate_plugin_name("plugin1").is_ok());
    }

    #[test]
    fn test_validate_plugin_name_with_uppercase_returns_violation() {
        let v = validator();
        let result = v.validate_plugin_name("MyPlugin");
        assert!(!result.is_ok());
        assert!(result.has_blocking_violations());
    }

    #[test]
    fn test_validate_plugin_name_with_hyphen_returns_violation() {
        let v = validator();
        let result = v.validate_plugin_name("my-plugin");
        assert!(!result.is_ok());
    }

    #[test]
    fn test_validate_plugin_name_with_empty_string_returns_violation() {
        let v = validator();
        let result = v.validate_plugin_name("");
        assert!(!result.is_ok());
    }

    #[test]
    fn test_validate_plugin_name_with_leading_digit_returns_violation() {
        let v = validator();
        let result = v.validate_plugin_name("1invalid");
        assert!(!result.is_ok());
    }

    // ------------------------------------------------------------------
    // validate_event_type
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_event_type_with_push_returns_no_violations() {
        let v = validator();
        assert!(v.validate_event_type("push").is_ok());
    }

    #[test]
    fn test_validate_event_type_with_pull_request_returns_no_violations() {
        let v = validator();
        assert!(v.validate_event_type("pull_request").is_ok());
    }

    #[test]
    fn test_validate_event_type_with_all_known_types_returns_no_violations() {
        let v = validator();
        for known in &[
            "push",
            "pull_request",
            "issue",
            "release",
            "schedule",
            "workflow_dispatch",
            "tag",
            "commit",
            "merge",
        ] {
            assert!(
                v.validate_event_type(known).is_ok(),
                "known event type '{}' should pass",
                known
            );
        }
    }

    #[test]
    fn test_validate_event_type_with_unknown_type_returns_violation() {
        let v = validator();
        let result = v.validate_event_type("custom_event");
        assert!(!result.is_ok());
        assert!(result.has_blocking_violations());
    }

    #[test]
    fn test_validate_event_type_with_empty_string_returns_violation() {
        let v = validator();
        let result = v.validate_event_type("");
        assert!(!result.is_ok());
    }

    // ------------------------------------------------------------------
    // validate_provider_endpoint
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_provider_endpoint_with_https_returns_no_violations() {
        let v = validator();
        assert!(
            v.validate_provider_endpoint("https://api.openai.com")
                .is_ok()
        );
    }

    #[test]
    fn test_validate_provider_endpoint_with_uppercase_https_returns_no_violations() {
        let v = validator();
        // Case-insensitive check — HTTPS:// should pass.
        assert!(
            v.validate_provider_endpoint("HTTPS://api.example.com")
                .is_ok()
        );
    }

    #[test]
    fn test_validate_provider_endpoint_with_http_returns_violation() {
        let v = validator();
        let result = v.validate_provider_endpoint("http://api.example.com");
        assert!(!result.is_ok());
        assert!(result.has_blocking_violations());
    }

    #[test]
    fn test_validate_provider_endpoint_with_ftp_returns_violation() {
        let v = validator();
        let result = v.validate_provider_endpoint("ftp://example.com");
        assert!(!result.is_ok());
    }

    #[test]
    fn test_validate_provider_endpoint_with_empty_string_skips_validation() {
        let v = validator();
        // Empty endpoint is not an error — it means the endpoint is absent.
        assert!(v.validate_provider_endpoint("").is_ok());
    }

    // ------------------------------------------------------------------
    // GovernanceValidator::rules accessor
    // ------------------------------------------------------------------

    #[test]
    fn test_governance_validator_rules_returns_loaded_rule_set() {
        let v = validator();
        assert!(!v.rules().is_empty());
        assert!(v.rules().has("governance.path.no_traversal"));
    }
}
