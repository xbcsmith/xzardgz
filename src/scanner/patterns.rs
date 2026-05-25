//! Pattern sets and registry for scanner and plugin content matching.
//!
//! A [`PatternSet`] groups keywords, dependency names, and file-name globs
//! that identify a particular concern (secrets, unsafe code, etc.).
//! [`PatternRegistry`] is a named lookup table for pattern sets, with
//! [`PatternRegistry::default_registry`] providing built-in sets.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// PatternSet
// ---------------------------------------------------------------------------

/// A named collection of content-matching patterns for a single concern.
///
/// `PatternSet` bundles together all textual signals that indicate a
/// particular concern is present in a repository: in-file keywords,
/// dependency names that trigger investigation, and file-system glob patterns
/// to narrow the search to relevant paths.
///
/// Construct an empty set with [`PatternSet::new`] and then populate the
/// public `Vec` fields directly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatternSet {
    /// Unique name identifying this pattern set (e.g. `"secrets"`).
    pub name: String,
    /// Literal substrings to search for inside file content.
    pub keywords: Vec<String>,
    /// Dependency names to search for (e.g. crate names, npm packages).
    pub dependencies: Vec<String>,
    /// Exact file names or glob patterns to match against the file path.
    pub file_names: Vec<String>,
}

impl PatternSet {
    /// Creates a new [`PatternSet`] with the given name and empty lists.
    ///
    /// All three collections (`keywords`, `dependencies`, `file_names`) start
    /// empty. Populate them by assigning directly to the public fields after
    /// construction.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name identifying this pattern set.
    ///
    /// # Returns
    ///
    /// A [`PatternSet`] with the given name and empty keyword, dependency,
    /// and file-name collections.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            keywords: Vec::new(),
            dependencies: Vec::new(),
            file_names: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// PatternRegistry
// ---------------------------------------------------------------------------

/// Registry mapping pattern-set names to their definitions.
///
/// Use [`PatternRegistry::new`] for an empty registry, or
/// [`PatternRegistry::default_registry`] to get all built-in pattern sets
/// pre-registered.  Custom pattern sets can be added at any time via
/// [`PatternRegistry::register`].
#[derive(Debug, Default)]
pub struct PatternRegistry {
    patterns: HashMap<String, PatternSet>,
}

impl PatternRegistry {
    /// Creates an empty [`PatternRegistry`] with no pattern sets registered.
    ///
    /// # Returns
    ///
    /// An empty `PatternRegistry`.
    pub fn new() -> Self {
        Self {
            patterns: HashMap::new(),
        }
    }

    /// Registers a pattern set, replacing any existing entry with the same name.
    ///
    /// If a pattern set with the same `name` already exists it is silently
    /// replaced by `set`.
    ///
    /// # Arguments
    ///
    /// * `set` - The [`PatternSet`] to register.
    pub fn register(&mut self, set: PatternSet) {
        self.patterns.insert(set.name.clone(), set);
    }

    /// Returns the pattern set with the given name, or [`None`].
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the pattern set to retrieve.
    ///
    /// # Returns
    ///
    /// A reference to the [`PatternSet`] if registered, or `None` otherwise.
    pub fn get(&self, name: &str) -> Option<&PatternSet> {
        self.patterns.get(name)
    }

    /// Returns all registered pattern set names.
    ///
    /// The order is not guaranteed.  Collect and sort if deterministic
    /// ordering is required.
    ///
    /// # Returns
    ///
    /// A [`Vec<&str>`] containing all registered pattern set names.
    pub fn names(&self) -> Vec<&str> {
        self.patterns.keys().map(|k| k.as_str()).collect()
    }

    /// Creates a registry pre-populated with the built-in pattern sets.
    ///
    /// Built-in sets:
    /// - `"secrets"` — common credential leakage patterns
    /// - `"unsafe_rust"` — Rust unsafe blocks
    /// - `"command_execution"` — OS command execution patterns
    /// - `"network_clients"` — HTTP/TCP network client usage
    /// - `"auth"` — authentication and authorization logic
    /// - `"risky"` — dangerous execution patterns (eval, exec, raw SQL)
    ///
    /// # Returns
    ///
    /// A [`PatternRegistry`] with all six built-in pattern sets registered.
    pub fn default_registry() -> Self {
        let mut registry = Self::new();

        let mut secrets = PatternSet::new("secrets");
        secrets.keywords = vec![
            "password=".to_string(),
            "secret=".to_string(),
            "api_key=".to_string(),
            "apikey=".to_string(),
            "token=".to_string(),
            "private_key=".to_string(),
            "access_key=".to_string(),
            "SECRET_KEY".to_string(),
            "API_KEY".to_string(),
            "PRIVATE_KEY".to_string(),
            "password:".to_string(),
            "secret:".to_string(),
            "-----BEGIN".to_string(),
        ];
        registry.register(secrets);

        let mut unsafe_rust = PatternSet::new("unsafe_rust");
        unsafe_rust.keywords = vec![
            "unsafe {".to_string(),
            "unsafe fn".to_string(),
            "unsafe impl".to_string(),
            "unsafe trait".to_string(),
        ];
        registry.register(unsafe_rust);

        let mut command_execution = PatternSet::new("command_execution");
        command_execution.keywords = vec![
            "std::process::Command".to_string(),
            "subprocess.".to_string(),
            "os.system(".to_string(),
            "shell_exec(".to_string(),
            "popen(".to_string(),
            "Process::new(".to_string(),
        ];
        registry.register(command_execution);

        let mut network_clients = PatternSet::new("network_clients");
        network_clients.keywords = vec![
            "reqwest::".to_string(),
            "hyper::".to_string(),
            "TcpStream".to_string(),
            "HttpClient".to_string(),
            "axios".to_string(),
            "fetch(".to_string(),
            "requests.get".to_string(),
            "requests.post".to_string(),
            "urllib.request".to_string(),
        ];
        registry.register(network_clients);

        let mut auth = PatternSet::new("auth");
        auth.keywords = vec![
            "authenticate".to_string(),
            "authorize".to_string(),
            " jwt".to_string(),
            " oauth".to_string(),
            "bearer".to_string(),
            "bcrypt".to_string(),
            "argon2".to_string(),
            "verify_password".to_string(),
            "password_hash".to_string(),
            "session_token".to_string(),
            "check_password".to_string(),
        ];
        registry.register(auth);

        let mut risky = PatternSet::new("risky");
        risky.keywords = vec![
            "eval(".to_string(),
            "exec(".to_string(),
            "__import__(".to_string(),
            "raw_query".to_string(),
            "execute(".to_string(),
            r#"format!("SELECT"#.to_string(),
            r#"format!("INSERT"#.to_string(),
            r#"format!("UPDATE"#.to_string(),
            r#"format!("DELETE"#.to_string(),
        ];
        registry.register(risky);

        registry
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_set_new_sets_name_and_empty_lists() {
        let ps = PatternSet::new("test_set");
        assert_eq!(ps.name, "test_set");
        assert!(ps.keywords.is_empty());
        assert!(ps.dependencies.is_empty());
        assert!(ps.file_names.is_empty());
    }

    #[test]
    fn test_pattern_registry_register_and_get() {
        let mut registry = PatternRegistry::new();
        let set = PatternSet::new("my_set");
        registry.register(set);
        let retrieved = registry.get("my_set");
        assert!(retrieved.is_some());
        // SAFETY: asserted is_some above
        assert_eq!(retrieved.unwrap().name, "my_set");
    }

    #[test]
    fn test_pattern_registry_get_missing_returns_none() {
        let registry = PatternRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_pattern_registry_names_returns_all_keys() {
        let mut registry = PatternRegistry::new();
        registry.register(PatternSet::new("alpha"));
        registry.register(PatternSet::new("beta"));
        registry.register(PatternSet::new("gamma"));
        let mut names = registry.names();
        names.sort();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn test_default_registry_contains_secrets_set() {
        let registry = PatternRegistry::default_registry();
        assert!(registry.get("secrets").is_some());
    }

    #[test]
    fn test_default_registry_contains_unsafe_rust_set() {
        let registry = PatternRegistry::default_registry();
        assert!(registry.get("unsafe_rust").is_some());
    }

    #[test]
    fn test_default_registry_contains_command_execution_set() {
        let registry = PatternRegistry::default_registry();
        assert!(registry.get("command_execution").is_some());
    }

    #[test]
    fn test_default_registry_secrets_keywords_not_empty() {
        let registry = PatternRegistry::default_registry();
        // SAFETY: default_registry always registers "secrets"
        let secrets = registry.get("secrets").unwrap();
        assert!(!secrets.keywords.is_empty());
    }
}
