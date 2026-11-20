//! Four-level ruleset search path and rule-id namespacing.
//!
//! # Search path (decreasing priority)
//!
//! | Level | Source                                                                   |
//! |-------|--------------------------------------------------------------------------|
//! | 1     | `ResolveConfig::cli_rulesets` (builtin ids or filesystem paths) and      |
//! |       | `ResolveConfig::cli_ruleset_paths` (extra search directories)            |
//! | 2     | `XZARDGZ_SAST_RULESET_PATHS` environment variable (colon-separated dirs) |
//! | 3     | `~/.config/xzardgz/rulesets/` user rulesets directory                   |
//! | 4     | Embedded built-in rules: loaded only when levels 1-3 provide no rules   |
//!
//! Rules from every source are namespaced as `"<ruleset_id>::<base_rule_id>"`.
//! Missing external directories are logged as warnings and skipped rather than
//! causing a hard failure.

use std::path::{Path, PathBuf};

use tracing::warn;
use walkdir::WalkDir;

use crate::scanner::sast::{
    embedded::{embedded_rules, ruleset_id_from_key},
    error::SastError,
    rule::{
        ir::{CompileOutcome, RuleIr},
        parse::parse_and_compile,
    },
};

// ---------------------------------------------------------------------------
// NamespacedRule
// ---------------------------------------------------------------------------

/// A compiled rule with its ruleset namespace applied.
///
/// The `namespaced_id` combines the ruleset identifier and the base rule id,
/// forming a globally unique identifier even when the same base id appears in
/// multiple rulesets.
#[derive(Debug, Clone)]
pub struct NamespacedRule {
    /// Identifier for the ruleset that owns this rule (e.g. `"builtin:crypto"`).
    pub ruleset_id: String,

    /// Fully-qualified rule identifier: `"<ruleset_id>::<base_id>"`.
    ///
    /// For example, `"builtin:crypto::rust-md5-usage"`.
    pub namespaced_id: String,

    /// The compiled rule.
    ///
    /// `rule.id` holds the base id (e.g. `"rust-md5-usage"`) exactly as
    /// declared in the YAML source; it is not modified by namespacing.
    pub rule: RuleIr,
}

// ---------------------------------------------------------------------------
// ResolveConfig
// ---------------------------------------------------------------------------

/// Configuration for the four-level ruleset resolution.
///
/// Build this from CLI flags before calling [`resolve_rules`].
#[derive(Debug, Default)]
pub struct ResolveConfig {
    /// Ruleset identifiers or filesystem paths from the `--ruleset` CLI flag.
    ///
    /// Entries beginning with `"builtin:"` are resolved against the embedded
    /// rule catalogue. All other entries are treated as filesystem directory
    /// paths and walked recursively for `.yaml` files.
    pub cli_rulesets: Vec<String>,

    /// Extra ruleset search directories from the `--ruleset-path` CLI flag.
    ///
    /// Each path is walked recursively; every `.yaml` file found is compiled
    /// and added to the result set.
    pub cli_ruleset_paths: Vec<PathBuf>,
}

// ---------------------------------------------------------------------------
// namespace_rule
// ---------------------------------------------------------------------------

/// Namespace a [`RuleIr`] by prepending the ruleset identifier to its `id`.
///
/// Returns a [`NamespacedRule`] where `namespaced_id = "<ruleset_id>::<rule.id>"`.
/// The `rule.id` field is left unchanged.
///
/// # Arguments
///
/// * `ruleset_id` - The ruleset identifier prefix (e.g. `"builtin:crypto"`).
/// * `rule` - The compiled rule to namespace.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::rule::resolve::namespace_rule;
/// use xzardgz::scanner::sast::rule::ir::{Formula, Leaf, RuleIr};
/// use xzardgz::scanner::sast::rule::metadata::Severity;
///
/// let rule = RuleIr {
///     id: "rust-md5".to_string(),
///     message: "MD5 usage detected".to_string(),
///     languages: vec!["rust".to_string()],
///     severity: Severity::Warning,
///     metadata: None,
///     formula: Formula::Leaf(Leaf::Pattern("$X".to_string())),
///     fix: None,
/// };
/// let nr = namespace_rule("builtin:crypto", rule);
/// assert_eq!(nr.namespaced_id, "builtin:crypto::rust-md5");
/// assert_eq!(nr.ruleset_id, "builtin:crypto");
/// assert_eq!(nr.rule.id, "rust-md5");
/// ```
pub fn namespace_rule(ruleset_id: &str, rule: RuleIr) -> NamespacedRule {
    let namespaced_id = format!("{}::{}", ruleset_id, rule.id);
    NamespacedRule {
        ruleset_id: ruleset_id.to_string(),
        namespaced_id,
        rule,
    }
}

// ---------------------------------------------------------------------------
// load_rules_from_dir
// ---------------------------------------------------------------------------

/// Load all `.yaml` rule files from a directory, returning namespaced rules.
///
/// The directory is walked recursively. Files that do not have a `.yaml`
/// extension are ignored. Each `.yaml` file is parsed with
/// [`parse_and_compile`]; successfully compiled rules are namespaced using
/// `ruleset_id` and collected into the return value. Skipped rules (those
/// that use unsupported constructs) are logged at the `warn` level and
/// omitted from the result.
///
/// Returns an empty [`Vec`] with a [`tracing::warn!`] when `dir` does not
/// exist or is not accessible.
///
/// # Arguments
///
/// * `dir` - Directory to search for `.yaml` rule files.
/// * `ruleset_id` - Identifier used to namespace every rule found in `dir`.
///
/// # Errors
///
/// Returns [`SastError::RuleRead`] when a `.yaml` file exists but cannot be
/// read from disk. Returns [`SastError::RuleParse`] when a `.yaml` file is
/// read successfully but cannot be parsed or compiled.
pub fn load_rules_from_dir(dir: &Path, ruleset_id: &str) -> Result<Vec<NamespacedRule>, SastError> {
    if !dir.exists() {
        warn!(
            dir = %dir.display(),
            "ruleset directory does not exist, skipping"
        );
        return Ok(Vec::new());
    }

    let mut rules = Vec::new();

    for entry in WalkDir::new(dir).follow_links(true) {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                warn!(
                    dir = %dir.display(),
                    error = %err,
                    "error walking ruleset directory, skipping entry"
                );
                continue;
            }
        };

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        // Only process files with the project-required `.yaml` extension.
        if path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
            continue;
        }

        let path_str = path.to_string_lossy().into_owned();

        let yaml = std::fs::read_to_string(path).map_err(|e| SastError::RuleRead {
            path: path_str.clone(),
            cause: e.to_string(),
        })?;

        let outcomes = parse_and_compile(&yaml).map_err(|e| SastError::RuleParse {
            path: path_str.clone(),
            source: e,
        })?;

        for outcome in outcomes {
            match outcome {
                Ok(CompileOutcome::Compiled(rule)) => {
                    rules.push(namespace_rule(ruleset_id, rule));
                }
                Ok(CompileOutcome::Skipped { rule_id, reasons }) => {
                    warn!(
                        rule_id = %rule_id,
                        path = %path_str,
                        reasons = ?reasons,
                        "rule skipped due to unsupported constructs"
                    );
                }
                Err(e) => {
                    return Err(SastError::RuleParse {
                        path: path_str,
                        source: e,
                    });
                }
            }
        }
    }

    Ok(rules)
}

// ---------------------------------------------------------------------------
// load_builtin_ruleset
// ---------------------------------------------------------------------------

/// Load a named builtin ruleset from the embedded rule catalogue.
///
/// Recognized ids are `"builtin:crypto"` and `"builtin:security"`. An unknown
/// id produces a [`tracing::warn!`] and returns an empty [`Vec`].
///
/// Each key in the embedded catalogue is matched against `ruleset_id` using
/// [`ruleset_id_from_key`]. Successfully compiled rules are namespaced with
/// the requested `ruleset_id`. Skipped rules are logged at the `warn` level.
///
/// # Arguments
///
/// * `ruleset_id` - The builtin ruleset identifier (e.g. `"builtin:crypto"`).
///
/// # Errors
///
/// Returns [`SastError::RuleParse`] when an embedded rule file fails to
/// parse or compile. Because embedded rules are compiled into the binary,
/// this indicates a programming error rather than a user error.
pub fn load_builtin_ruleset(ruleset_id: &str) -> Result<Vec<NamespacedRule>, SastError> {
    let all = embedded_rules();
    let mut rules = Vec::new();
    let mut found_any = false;

    for (&key, &yaml) in all.iter() {
        if ruleset_id_from_key(key) != Some(ruleset_id) {
            continue;
        }
        found_any = true;

        let outcomes = parse_and_compile(yaml).map_err(|e| SastError::RuleParse {
            path: key.to_string(),
            source: e,
        })?;

        for outcome in outcomes {
            match outcome {
                Ok(CompileOutcome::Compiled(rule)) => {
                    rules.push(namespace_rule(ruleset_id, rule));
                }
                Ok(CompileOutcome::Skipped { rule_id, reasons }) => {
                    warn!(
                        rule_id = %rule_id,
                        key = %key,
                        reasons = ?reasons,
                        "builtin rule skipped due to unsupported constructs"
                    );
                }
                Err(e) => {
                    return Err(SastError::RuleParse {
                        path: key.to_string(),
                        source: e,
                    });
                }
            }
        }
    }

    if !found_any {
        warn!(
            ruleset_id = %ruleset_id,
            "unknown builtin ruleset id; returning empty rule set"
        );
    }

    Ok(rules)
}

// ---------------------------------------------------------------------------
// resolve_rules
// ---------------------------------------------------------------------------

/// Resolve rules from the four-level search path.
///
/// Returns all compiled rules with their ruleset namespacing applied.
/// Never fails due to missing external paths; logs warnings instead.
///
/// # Search order (highest priority first)
///
/// 1. `config.cli_rulesets`: entries beginning with `"builtin:"` are loaded
///    from the embedded catalogue; all other entries are treated as filesystem
///    directory paths.
/// 2. `config.cli_ruleset_paths`: each directory is walked for `.yaml` files.
/// 3. `XZARDGZ_SAST_RULESET_PATHS` environment variable: colon-separated
///    (Unix) or semicolon-separated (Windows) list of directories.
/// 4. `~/.config/xzardgz/rulesets/` user rulesets directory.
/// 5. Embedded defaults (`"builtin:crypto"` and `"builtin:security"`): loaded
///    only when `cli_rulesets` is empty and levels 3-4 found no YAML files.
///
/// # Arguments
///
/// * `config` - CLI and environment configuration for ruleset resolution.
///
/// # Errors
///
/// Returns [`SastError`] only on hard parse errors in rule files that were
/// found and read successfully.
pub fn resolve_rules(config: &ResolveConfig) -> Result<Vec<NamespacedRule>, SastError> {
    let mut rules = Vec::new();

    // Level 1a: cli_rulesets - builtin catalogue ids or filesystem paths.
    for entry in &config.cli_rulesets {
        if entry.starts_with("builtin:") {
            rules.extend(load_builtin_ruleset(entry)?);
        } else {
            let path = PathBuf::from(entry);
            rules.extend(load_rules_from_dir(&path, entry)?);
        }
    }

    // Level 1b: cli_ruleset_paths - explicit extra search directories.
    for dir in &config.cli_ruleset_paths {
        let ruleset_id = dir.to_string_lossy().into_owned();
        rules.extend(load_rules_from_dir(dir, &ruleset_id)?);
    }

    // Levels 2 and 3: check for external YAML files.
    // Track whether any were found to control the embedded fallback (level 4).
    let mut external_found = false;

    // Level 2: XZARDGZ_SAST_RULESET_PATHS environment variable.
    if let Ok(env_paths) = std::env::var("XZARDGZ_SAST_RULESET_PATHS") {
        // Colon-separated on Unix; semicolon-separated on Windows.
        let separator = if cfg!(windows) { ';' } else { ':' };
        for dir_str in env_paths.split(separator).filter(|s| !s.is_empty()) {
            let dir = PathBuf::from(dir_str);
            let found = load_rules_from_dir(&dir, dir_str)?;
            if !found.is_empty() {
                external_found = true;
            }
            rules.extend(found);
        }
    }

    // Level 3: ~/.config/xzardgz/rulesets/ user rulesets directory.
    if let Some(config_base) = dirs::config_dir() {
        let user_dir = config_base.join("xzardgz").join("rulesets");
        let ruleset_id = user_dir.to_string_lossy().into_owned();
        let found = load_rules_from_dir(&user_dir, &ruleset_id)?;
        if !found.is_empty() {
            external_found = true;
        }
        rules.extend(found);
    }

    // Level 4: embedded defaults.
    // Activated only when cli_rulesets is empty AND levels 2-3 found no YAML files.
    if config.cli_rulesets.is_empty() && !external_found {
        rules.extend(load_builtin_ruleset("builtin:crypto")?);
        rules.extend(load_builtin_ruleset("builtin:security")?);
    }

    Ok(rules)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use tempfile::TempDir;

    use super::*;
    use crate::scanner::sast::rule::{
        ir::{Formula, Leaf},
        metadata::Severity,
    };

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_rule(id: &str) -> RuleIr {
        RuleIr {
            id: id.to_string(),
            message: format!("Test rule for {id}"),
            languages: vec!["rust".to_string()],
            severity: Severity::Warning,
            metadata: None,
            formula: Formula::Leaf(Leaf::Pattern("$X".to_string())),
            fix: None,
        }
    }

    /// Write a minimal valid YAML rule file into `dir` with the given filename and rule id.
    fn write_rule_yaml(dir: &TempDir, filename: &str, rule_id: &str) {
        let content = format!(
            "rules:\n\
             \x20 - id: {rule_id}\n\
             \x20   message: Test rule generated by resolve tests\n\
             \x20   languages:\n\
             \x20     - regex\n\
             \x20   severity: WARNING\n\
             \x20   pattern-regex: \"TODO\"\n"
        );
        // SAFETY: writing to a just-created temp dir in a test; failure is a test bug.
        std::fs::write(dir.path().join(filename), content)
            .expect("test helper: failed to write rule YAML");
    }

    // -----------------------------------------------------------------------
    // namespace_rule
    // -----------------------------------------------------------------------

    #[test]
    fn test_namespace_rule_produces_correct_namespaced_id() {
        let rule = make_rule("rust-md5");
        let nr = namespace_rule("builtin:crypto", rule);
        assert_eq!(nr.namespaced_id, "builtin:crypto::rust-md5");
        assert_eq!(nr.ruleset_id, "builtin:crypto");
        assert_eq!(nr.rule.id, "rust-md5");
    }

    // -----------------------------------------------------------------------
    // load_builtin_ruleset
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_builtin_ruleset_crypto_returns_at_least_15_rules() {
        let rules = load_builtin_ruleset("builtin:crypto").expect("builtin:crypto should load");
        assert!(
            rules.len() >= 15,
            "expected at least 15 compiled crypto rules, got {}",
            rules.len()
        );
    }

    #[test]
    fn test_load_builtin_ruleset_security_returns_at_least_10_rules() {
        let rules = load_builtin_ruleset("builtin:security").expect("builtin:security should load");
        assert!(
            rules.len() >= 10,
            "expected at least 10 compiled security rules, got {}",
            rules.len()
        );
    }

    #[test]
    fn test_load_builtin_ruleset_unknown_id_returns_empty_vec() {
        let rules =
            load_builtin_ruleset("builtin:unknown").expect("unknown id must not return Err");
        assert!(
            rules.is_empty(),
            "expected empty vec for unknown builtin ruleset id"
        );
    }

    // -----------------------------------------------------------------------
    // load_rules_from_dir
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_rules_from_dir_missing_directory_returns_empty() {
        let result = load_rules_from_dir(Path::new("/nonexistent/xzardgz/resolve/test"), "test:ns");
        assert!(
            result.is_ok(),
            "expected Ok for missing dir, got: {result:?}"
        );
        assert!(
            result.expect("already checked is_ok").is_empty(),
            "expected empty vec for missing directory"
        );
    }

    #[test]
    fn test_load_rules_from_dir_with_valid_yaml_returns_rules() {
        let dir = TempDir::new().expect("test: failed to create temp dir");
        write_rule_yaml(&dir, "my_rule.yaml", "my-test-rule");

        let rules =
            load_rules_from_dir(dir.path(), "test:ruleset").expect("should compile valid yaml");
        assert!(!rules.is_empty(), "expected at least one compiled rule");
        assert_eq!(rules[0].ruleset_id, "test:ruleset");
        assert_eq!(rules[0].namespaced_id, "test:ruleset::my-test-rule");
        assert_eq!(rules[0].rule.id, "my-test-rule");
    }

    // -----------------------------------------------------------------------
    // resolve_rules
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_rules_with_empty_config_loads_default_builtins() {
        // Clear the env var so we have a deterministic environment.
        temp_env::with_var("XZARDGZ_SAST_RULESET_PATHS", None::<&str>, || {
            let config = ResolveConfig {
                cli_rulesets: vec![],
                cli_ruleset_paths: vec![],
            };
            let rules = resolve_rules(&config).expect("resolve_rules must not fail");
            assert!(
                rules.len() >= 25,
                "expected at least 25 default builtin rules, got {}",
                rules.len()
            );
        });
    }

    #[test]
    fn test_resolve_rules_namespacing_produces_distinct_ids_across_rulesets() {
        let config = ResolveConfig {
            cli_rulesets: vec!["builtin:crypto".to_string(), "builtin:security".to_string()],
            cli_ruleset_paths: vec![],
        };
        let rules = resolve_rules(&config).expect("resolve_rules must not fail");
        let mut seen: HashSet<String> = HashSet::new();
        for r in &rules {
            assert!(
                seen.insert(r.namespaced_id.clone()),
                "duplicate namespaced_id detected: {}",
                r.namespaced_id
            );
        }
    }

    #[test]
    fn test_search_path_precedence_cli_ruleset_path_takes_priority() {
        let dir = TempDir::new().expect("test: failed to create temp dir");
        write_rule_yaml(&dir, "priority_rule.yaml", "priority-test-rule");

        let expected_ruleset_id = dir.path().to_string_lossy().into_owned();
        let config = ResolveConfig {
            cli_rulesets: vec![],
            cli_ruleset_paths: vec![dir.path().to_path_buf()],
        };
        let rules = resolve_rules(&config).expect("resolve_rules must not fail");
        let nr = rules
            .iter()
            .find(|r| r.rule.id == "priority-test-rule")
            .expect("SAFETY: we just wrote this rule; it must appear in results");
        assert_eq!(nr.ruleset_id, expected_ruleset_id);
        assert_eq!(
            nr.namespaced_id,
            format!("{}::priority-test-rule", expected_ruleset_id)
        );
    }

    #[test]
    fn test_missing_ruleset_path_does_not_fail_run() {
        let config = ResolveConfig {
            cli_rulesets: vec![],
            cli_ruleset_paths: vec![PathBuf::from("/nonexistent/xzardgz/ruleset/path")],
        };
        let result = resolve_rules(&config);
        assert!(
            result.is_ok(),
            "resolve_rules must not fail for a missing cli_ruleset_path: {result:?}"
        );
    }
}
