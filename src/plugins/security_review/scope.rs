//! Security scope definitions and file prioritization.
//!
//! [`SecurityCategory`] enumerates the 19 security concern areas that the
//! security review plugin inspects. [`SecurityFilePrioritizer`] selects and
//! ranks source files from a [`ScanResult`] based on which categories are
//! enabled by the plugin configuration.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// SecurityCategory
// ---------------------------------------------------------------------------

/// Enumerates the security concern areas that the security review plugin inspects.
///
/// Each variant corresponds to a distinct category of security risk that may
/// be present in a codebase. The set of active categories for a given scan is
/// determined by the plugin configuration flags passed to
/// [`SecurityFilePrioritizer::active_categories`].
///
/// # Examples
///
/// ```no_run
/// use xzardgz::plugins::security_review::scope::SecurityCategory;
///
/// assert_eq!(SecurityCategory::Secrets.as_str(), "secrets");
/// assert_eq!(SecurityCategory::UnsafeRust.default_cwe(), Some("CWE-119"));
/// assert_eq!(SecurityCategory::all_categories().len(), 19);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SecurityCategory {
    /// Hardcoded credentials, tokens, and API keys embedded in source.
    Secrets,
    /// Dependency manifest files and their third-party vulnerability surface.
    DependencyManifest,
    /// Authentication mechanisms, session management, and identity verification.
    Authentication,
    /// Access control logic, permission checks, and privilege boundaries.
    Authorization,
    /// HTTP request handler functions that process inbound data.
    RequestHandlers,
    /// URL and routing definition files that map requests to handlers.
    RouteDefinitions,
    /// File system read/write operations that may expose path traversal risks.
    FileOperations,
    /// OS command and shell execution that may permit injection attacks.
    CommandExecution,
    /// Deserialisation of untrusted data that may allow object injection.
    Deserialization,
    /// Cryptographic algorithm usage, key generation, and hashing.
    Cryptography,
    /// Use of Rust `unsafe` blocks that bypass memory-safety guarantees.
    UnsafeRust,
    /// Environment variable access that may inadvertently expose secrets.
    EnvironmentVariables,
    /// HTTP and network client code, including proxy and timeout configuration.
    NetworkClients,
    /// Hardcoded URLs, IP addresses, and hostnames in source code.
    HardcodedEndpoints,
    /// Logging statements that may emit passwords, tokens, or PII.
    SensitiveLogging,
    /// Error messages that may leak stack traces or internal implementation details.
    ErrorLeakage,
    /// TLS/SSL certificate validation and configuration.
    TlsHandling,
    /// Input validation and sanitisation at external trust boundaries.
    InputValidation,
    /// Output encoding and sanitisation before rendering to a consumer.
    OutputSanitization,
}

impl SecurityCategory {
    /// Returns the lowercase, snake-case identifier string for this category.
    ///
    /// These identifiers are stable across releases and are used as keys in
    /// serialised configuration, report metadata, and SARIF rule IDs.
    ///
    /// # Returns
    ///
    /// A `&'static str` snake-case name for this variant.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::plugins::security_review::scope::SecurityCategory;
    ///
    /// assert_eq!(SecurityCategory::Secrets.as_str(), "secrets");
    /// assert_eq!(SecurityCategory::DependencyManifest.as_str(), "dependency_manifest");
    /// assert_eq!(SecurityCategory::UnsafeRust.as_str(), "unsafe_rust");
    /// assert_eq!(SecurityCategory::TlsHandling.as_str(), "tls_handling");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Secrets => "secrets",
            Self::DependencyManifest => "dependency_manifest",
            Self::Authentication => "authentication",
            Self::Authorization => "authorization",
            Self::RequestHandlers => "request_handlers",
            Self::RouteDefinitions => "route_definitions",
            Self::FileOperations => "file_operations",
            Self::CommandExecution => "command_execution",
            Self::Deserialization => "deserialization",
            Self::Cryptography => "cryptography",
            Self::UnsafeRust => "unsafe_rust",
            Self::EnvironmentVariables => "environment_variables",
            Self::NetworkClients => "network_clients",
            Self::HardcodedEndpoints => "hardcoded_endpoints",
            Self::SensitiveLogging => "sensitive_logging",
            Self::ErrorLeakage => "error_leakage",
            Self::TlsHandling => "tls_handling",
            Self::InputValidation => "input_validation",
            Self::OutputSanitization => "output_sanitization",
        }
    }

    /// Returns the human-readable display name for this category.
    ///
    /// Display names are suitable for use in report headings, UI labels, and
    /// structured output visible to end users.
    ///
    /// # Returns
    ///
    /// A `&'static str` title-case name for this variant.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::plugins::security_review::scope::SecurityCategory;
    ///
    /// assert_eq!(SecurityCategory::Secrets.display_name(), "Secrets");
    /// assert_eq!(SecurityCategory::DependencyManifest.display_name(), "Dependency Manifest");
    /// assert_eq!(SecurityCategory::TlsHandling.display_name(), "TLS Handling");
    /// assert_eq!(SecurityCategory::UnsafeRust.display_name(), "Unsafe Rust");
    /// ```
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Secrets => "Secrets",
            Self::DependencyManifest => "Dependency Manifest",
            Self::Authentication => "Authentication",
            Self::Authorization => "Authorization",
            Self::RequestHandlers => "Request Handlers",
            Self::RouteDefinitions => "Route Definitions",
            Self::FileOperations => "File Operations",
            Self::CommandExecution => "Command Execution",
            Self::Deserialization => "Deserialization",
            Self::Cryptography => "Cryptography",
            Self::UnsafeRust => "Unsafe Rust",
            Self::EnvironmentVariables => "Environment Variables",
            Self::NetworkClients => "Network Clients",
            Self::HardcodedEndpoints => "Hardcoded Endpoints",
            Self::SensitiveLogging => "Sensitive Logging",
            Self::ErrorLeakage => "Error Leakage",
            Self::TlsHandling => "TLS Handling",
            Self::InputValidation => "Input Validation",
            Self::OutputSanitization => "Output Sanitization",
        }
    }

    /// Returns the primary CWE identifier associated with this category, if any.
    ///
    /// The returned string is in the canonical `"CWE-NNN"` format. Categories
    /// without a single dominant CWE mapping return `None`.
    ///
    /// # Returns
    ///
    /// `Some("CWE-NNN")` for categories with a dominant weakness entry, or
    /// `None` for categories that span multiple or no CWE entries.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::plugins::security_review::scope::SecurityCategory;
    ///
    /// assert_eq!(SecurityCategory::Secrets.default_cwe(), Some("CWE-798"));
    /// assert_eq!(SecurityCategory::Authentication.default_cwe(), Some("CWE-287"));
    /// assert_eq!(SecurityCategory::DependencyManifest.default_cwe(), None);
    /// ```
    pub fn default_cwe(&self) -> Option<&'static str> {
        match self {
            Self::Secrets => Some("CWE-798"),
            Self::Authentication => Some("CWE-287"),
            Self::Authorization => Some("CWE-285"),
            Self::CommandExecution => Some("CWE-78"),
            Self::Deserialization => Some("CWE-502"),
            Self::Cryptography => Some("CWE-327"),
            Self::UnsafeRust => Some("CWE-119"),
            Self::TlsHandling => Some("CWE-295"),
            Self::InputValidation => Some("CWE-20"),
            Self::OutputSanitization => Some("CWE-116"),
            _ => None,
        }
    }

    /// Returns the OWASP Top 10 (2021) category associated with this security area, if any.
    ///
    /// The returned string is in the canonical `"ANN:2021"` format. Categories
    /// that do not map cleanly to a single OWASP entry return `None`.
    ///
    /// # Returns
    ///
    /// `Some("ANN:2021")` for categories with a dominant OWASP mapping, or
    /// `None` for categories without one.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::plugins::security_review::scope::SecurityCategory;
    ///
    /// assert_eq!(SecurityCategory::Secrets.default_owasp(), Some("A07:2021"));
    /// assert_eq!(SecurityCategory::Cryptography.default_owasp(), Some("A02:2021"));
    /// assert_eq!(SecurityCategory::RequestHandlers.default_owasp(), None);
    /// ```
    pub fn default_owasp(&self) -> Option<&'static str> {
        match self {
            Self::Secrets => Some("A07:2021"),
            Self::DependencyManifest => Some("A06:2021"),
            Self::Authentication => Some("A07:2021"),
            Self::Authorization => Some("A01:2021"),
            Self::CommandExecution => Some("A03:2021"),
            Self::Deserialization => Some("A08:2021"),
            Self::Cryptography => Some("A02:2021"),
            Self::TlsHandling => Some("A02:2021"),
            Self::InputValidation => Some("A03:2021"),
            _ => None,
        }
    }

    /// Returns the SARIF help URI linking to the relevant CWE definition, if any.
    ///
    /// For categories with a `default_cwe`, the URI points to the canonical
    /// MITRE CWE detail page. Categories without a CWE mapping return `None`.
    ///
    /// # Returns
    ///
    /// `Some("https://cwe.mitre.org/data/definitions/NNN.html")` for categories
    /// with a CWE, or `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::plugins::security_review::scope::SecurityCategory;
    ///
    /// assert_eq!(
    ///     SecurityCategory::Secrets.sarif_help_uri(),
    ///     Some("https://cwe.mitre.org/data/definitions/798.html"),
    /// );
    /// assert_eq!(SecurityCategory::DependencyManifest.sarif_help_uri(), None);
    /// ```
    pub fn sarif_help_uri(&self) -> Option<&'static str> {
        match self {
            Self::Secrets => Some("https://cwe.mitre.org/data/definitions/798.html"),
            Self::Authentication => Some("https://cwe.mitre.org/data/definitions/287.html"),
            Self::Authorization => Some("https://cwe.mitre.org/data/definitions/285.html"),
            Self::CommandExecution => Some("https://cwe.mitre.org/data/definitions/78.html"),
            Self::Deserialization => Some("https://cwe.mitre.org/data/definitions/502.html"),
            Self::Cryptography => Some("https://cwe.mitre.org/data/definitions/327.html"),
            Self::UnsafeRust => Some("https://cwe.mitre.org/data/definitions/119.html"),
            Self::TlsHandling => Some("https://cwe.mitre.org/data/definitions/295.html"),
            Self::InputValidation => Some("https://cwe.mitre.org/data/definitions/20.html"),
            Self::OutputSanitization => Some("https://cwe.mitre.org/data/definitions/116.html"),
            _ => None,
        }
    }

    /// Returns all 19 security category variants in declaration order.
    ///
    /// This function is the canonical source of truth for the complete set of
    /// categories. Use it to iterate all categories or to build configuration
    /// defaults.
    ///
    /// # Returns
    ///
    /// A `Vec<SecurityCategory>` containing every variant exactly once.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::plugins::security_review::scope::SecurityCategory;
    ///
    /// let all = SecurityCategory::all_categories();
    /// assert_eq!(all.len(), 19);
    /// assert_eq!(all[0], SecurityCategory::Secrets);
    /// ```
    pub fn all_categories() -> Vec<SecurityCategory> {
        vec![
            Self::Secrets,
            Self::DependencyManifest,
            Self::Authentication,
            Self::Authorization,
            Self::RequestHandlers,
            Self::RouteDefinitions,
            Self::FileOperations,
            Self::CommandExecution,
            Self::Deserialization,
            Self::Cryptography,
            Self::UnsafeRust,
            Self::EnvironmentVariables,
            Self::NetworkClients,
            Self::HardcodedEndpoints,
            Self::SensitiveLogging,
            Self::ErrorLeakage,
            Self::TlsHandling,
            Self::InputValidation,
            Self::OutputSanitization,
        ]
    }
}

// ---------------------------------------------------------------------------
// SecurityFilePrioritizer
// ---------------------------------------------------------------------------

/// Selects and ranks security-relevant source files from a repository scan.
///
/// `SecurityFilePrioritizer` provides class methods that operate on a
/// [`ScanResult`][crate::scanner::result::ScanResult] to produce an ordered,
/// deduplicated list of file paths most likely to be relevant to a security
/// review.  The ordering places explicitly tagged security files first,
/// followed by dependency manifests (when enabled), and finally any additional
/// files from the full repository structure that pass the
/// [`is_security_relevant`][Self::is_security_relevant] heuristic.
pub struct SecurityFilePrioritizer;

impl SecurityFilePrioritizer {
    /// Builds a deduplicated, capped flat list of security-relevant file paths.
    ///
    /// Files are added in the following priority order:
    ///
    /// 1. `scan_result.security_relevant_files`
    /// 2. `scan_result.dependency_manifests` (only when `include_dep_manifests` is `true`)
    /// 3. Files from `scan_result.repository_structure` whose paths pass
    ///    [`is_security_relevant`][Self::is_security_relevant]
    ///
    /// Each path appears at most once (first occurrence wins). The list is
    /// truncated to `max_files` entries unless `max_files` is `0`, in which
    /// case all collected files are returned.
    ///
    /// # Arguments
    ///
    /// * `scan_result` - The completed repository scan to draw files from.
    /// * `max_files` - Maximum number of files to return. Pass `0` for no cap.
    /// * `include_dep_manifests` - When `true`, dependency manifests are
    ///   inserted after the security-relevant files.
    ///
    /// # Returns
    ///
    /// An ordered `Vec<String>` of deduplicated, security-relevant file paths.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::plugins::security_review::scope::SecurityFilePrioritizer;
    /// // Requires a populated ScanResult; see integration tests for usage.
    /// ```
    pub fn capped_flat_list(
        scan_result: &crate::scanner::result::ScanResult,
        max_files: u32,
        include_dep_manifests: bool,
    ) -> Vec<String> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut result: Vec<String> = Vec::new();

        // Priority 1: explicitly tagged security-relevant files.
        for path in &scan_result.security_relevant_files {
            if seen.insert(path.clone()) {
                result.push(path.clone());
            }
        }

        // Priority 2: dependency manifests (optional).
        if include_dep_manifests {
            for path in &scan_result.dependency_manifests {
                if seen.insert(path.clone()) {
                    result.push(path.clone());
                }
            }
        }

        // Priority 3: heuristic scan over the full repository structure.
        for entry in &scan_result.repository_structure {
            if Self::is_security_relevant(&entry.path) && seen.insert(entry.path.clone()) {
                result.push(entry.path.clone());
            }
        }

        if max_files == 0 {
            result
        } else {
            result.truncate(max_files as usize);
            result
        }
    }

    /// Returns `true` if the given file path is considered security-relevant.
    ///
    /// A path is security-relevant when either:
    ///
    /// - Its extension is in the set of security-relevant extensions (source
    ///   code, manifests, shell scripts, configs, etc.), **or**
    /// - Its filename exactly matches or contains a known sensitive filename
    ///   pattern (e.g. `".env"`, `"secrets"`, `"Dockerfile"`).
    ///
    /// The check is purely name-based and does not read file contents.
    ///
    /// # Arguments
    ///
    /// * `path` - A repository-relative file path (forward-slash separated).
    ///
    /// # Returns
    ///
    /// `true` if the path should be included in a security-focused file list.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::plugins::security_review::scope::SecurityFilePrioritizer;
    ///
    /// assert!(SecurityFilePrioritizer::is_security_relevant("src/auth.rs"));
    /// assert!(SecurityFilePrioritizer::is_security_relevant("config/.env"));
    /// assert!(!SecurityFilePrioritizer::is_security_relevant("assets/logo.png"));
    /// ```
    pub fn is_security_relevant(path: &str) -> bool {
        let filename = path.rsplit('/').next().unwrap_or(path);
        let extension = filename.rsplit('.').next().unwrap_or("");

        const SECURITY_EXTENSIONS: &[&str] = &[
            "rs", "py", "js", "ts", "go", "java", "kt", "rb", "php", "c", "cpp", "h", "toml",
            "yaml", "yml", "json", "env", "xml", "sh", "bash",
        ];

        const SECURITY_FILENAMES: &[&str] = &[
            "Cargo.toml",
            "requirements.txt",
            "package.json",
            "go.mod",
            "pom.xml",
            ".env",
            "Dockerfile",
            "docker-compose",
            ".github",
            "settings.py",
            "config.py",
            "secrets",
            "credentials",
        ];

        if SECURITY_EXTENSIONS.contains(&extension) {
            return true;
        }

        for pattern in SECURITY_FILENAMES {
            if filename == *pattern || filename.contains(pattern) {
                return true;
            }
        }

        false
    }

    /// Computes the list of active [`SecurityCategory`] variants from plugin config flags.
    ///
    /// Six categories are always active regardless of flags:
    /// `SensitiveLogging`, `ErrorLeakage`, `FileOperations`, `NetworkClients`,
    /// `InputValidation`, and `OutputSanitization`. Additional categories are
    /// opted in through the boolean parameters.
    ///
    /// # Arguments
    ///
    /// * `secret_scanning` - Enables `Secrets` and `EnvironmentVariables`.
    /// * `dependency_scanning` - Enables `DependencyManifest`.
    /// * `check_unsafe_code` - Enables `UnsafeRust`.
    /// * `check_auth` - Enables `Authentication` and `Authorization`.
    /// * `check_endpoints` - Enables `HardcodedEndpoints`, `RequestHandlers`,
    ///   and `RouteDefinitions`.
    /// * `check_command_execution` - Enables `CommandExecution`.
    /// * `check_deserialization` - Enables `Deserialization`.
    /// * `check_cryptography` - Enables `Cryptography` and `TlsHandling`.
    ///
    /// # Returns
    ///
    /// A `Vec<SecurityCategory>` listing every enabled category. The always-on
    /// categories appear first, followed by the opt-in categories in the order
    /// their controlling flags are listed above.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzardgz::plugins::security_review::scope::{SecurityCategory, SecurityFilePrioritizer};
    ///
    /// let categories = SecurityFilePrioritizer::active_categories(
    ///     true, false, false, false, false, false, false, false,
    /// );
    /// assert!(categories.contains(&SecurityCategory::Secrets));
    /// assert!(categories.contains(&SecurityCategory::SensitiveLogging));
    /// assert!(!categories.contains(&SecurityCategory::UnsafeRust));
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub fn active_categories(
        secret_scanning: bool,
        dependency_scanning: bool,
        check_unsafe_code: bool,
        check_auth: bool,
        check_endpoints: bool,
        check_command_execution: bool,
        check_deserialization: bool,
        check_cryptography: bool,
    ) -> Vec<SecurityCategory> {
        let mut categories = vec![
            SecurityCategory::SensitiveLogging,
            SecurityCategory::ErrorLeakage,
            SecurityCategory::FileOperations,
            SecurityCategory::NetworkClients,
            SecurityCategory::InputValidation,
            SecurityCategory::OutputSanitization,
        ];

        if secret_scanning {
            categories.push(SecurityCategory::Secrets);
            categories.push(SecurityCategory::EnvironmentVariables);
        }

        if dependency_scanning {
            categories.push(SecurityCategory::DependencyManifest);
        }

        if check_unsafe_code {
            categories.push(SecurityCategory::UnsafeRust);
        }

        if check_auth {
            categories.push(SecurityCategory::Authentication);
            categories.push(SecurityCategory::Authorization);
        }

        if check_endpoints {
            categories.push(SecurityCategory::HardcodedEndpoints);
            categories.push(SecurityCategory::RequestHandlers);
            categories.push(SecurityCategory::RouteDefinitions);
        }

        if check_command_execution {
            categories.push(SecurityCategory::CommandExecution);
        }

        if check_deserialization {
            categories.push(SecurityCategory::Deserialization);
        }

        if check_cryptography {
            categories.push(SecurityCategory::Cryptography);
            categories.push(SecurityCategory::TlsHandling);
        }

        categories
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::result::{
        FileEntry, LanguageStats, PluginPreselection, SCAN_RESULT_VERSION, ScanResult,
    };
    use chrono::Utc;
    use std::collections::HashMap;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Constructs a minimal valid [`ScanResult`] for use in unit tests.
    fn minimal_scan_result() -> ScanResult {
        ScanResult {
            version: SCAN_RESULT_VERSION.to_string(),
            repository_url: None,
            repository_name: Some("test-repo".to_string()),
            head_commit: None,
            scan_timestamp: Utc::now(),
            repository_structure: vec![],
            language_statistics: HashMap::new(),
            primary_language: None,
            frameworks: vec![],
            documentation_inventory: vec![],
            governance_rules: vec![],
            cli_commands: vec![],
            public_apis: vec![],
            entrypoints: vec![],
            config_surface: vec![],
            key_files: vec![],
            dependency_manifests: vec![],
            test_files: vec![],
            build_files: vec![],
            security_relevant_files: vec![],
            findings: vec![],
            plugin_preselection: PluginPreselection::default(),
        }
    }

    /// Creates a [`FileEntry`] with only the path set; all other fields use safe defaults.
    fn file_entry(path: &str) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            size_bytes: 0,
            language: None,
            is_binary: false,
        }
    }

    // -----------------------------------------------------------------------
    // SecurityCategory::as_str
    // -----------------------------------------------------------------------

    #[test]
    fn test_security_category_as_str_secrets_returns_secrets() {
        assert_eq!(SecurityCategory::Secrets.as_str(), "secrets");
    }

    #[test]
    fn test_security_category_as_str_dependency_manifest_returns_underscore_form() {
        assert_eq!(
            SecurityCategory::DependencyManifest.as_str(),
            "dependency_manifest"
        );
    }

    #[test]
    fn test_security_category_as_str_unsafe_rust_returns_unsafe_rust() {
        assert_eq!(SecurityCategory::UnsafeRust.as_str(), "unsafe_rust");
    }

    // -----------------------------------------------------------------------
    // SecurityCategory::default_cwe
    // -----------------------------------------------------------------------

    #[test]
    fn test_security_category_default_cwe_secrets_returns_cwe798() {
        assert_eq!(SecurityCategory::Secrets.default_cwe(), Some("CWE-798"));
    }

    #[test]
    fn test_security_category_default_cwe_authentication_returns_cwe287() {
        assert_eq!(
            SecurityCategory::Authentication.default_cwe(),
            Some("CWE-287")
        );
    }

    #[test]
    fn test_security_category_default_cwe_dependency_manifest_returns_none() {
        assert_eq!(SecurityCategory::DependencyManifest.default_cwe(), None);
    }

    // -----------------------------------------------------------------------
    // SecurityCategory::default_owasp
    // -----------------------------------------------------------------------

    #[test]
    fn test_security_category_default_owasp_secrets_returns_a07() {
        assert_eq!(SecurityCategory::Secrets.default_owasp(), Some("A07:2021"));
    }

    #[test]
    fn test_security_category_default_owasp_cryptography_returns_a02() {
        assert_eq!(
            SecurityCategory::Cryptography.default_owasp(),
            Some("A02:2021")
        );
    }

    #[test]
    fn test_security_category_default_owasp_request_handlers_returns_none() {
        assert_eq!(SecurityCategory::RequestHandlers.default_owasp(), None);
    }

    // -----------------------------------------------------------------------
    // SecurityCategory::sarif_help_uri
    // -----------------------------------------------------------------------

    #[test]
    fn test_security_category_sarif_help_uri_secrets_returns_url() {
        assert_eq!(
            SecurityCategory::Secrets.sarif_help_uri(),
            Some("https://cwe.mitre.org/data/definitions/798.html")
        );
    }

    #[test]
    fn test_security_category_sarif_help_uri_dependency_manifest_returns_none() {
        assert_eq!(SecurityCategory::DependencyManifest.sarif_help_uri(), None);
    }

    // -----------------------------------------------------------------------
    // SecurityCategory::all_categories
    // -----------------------------------------------------------------------

    #[test]
    fn test_security_category_all_categories_returns_19_items() {
        assert_eq!(SecurityCategory::all_categories().len(), 19);
    }

    // -----------------------------------------------------------------------
    // SecurityFilePrioritizer::is_security_relevant
    // -----------------------------------------------------------------------

    #[test]
    fn test_security_file_prioritizer_is_security_relevant_rs_file_returns_true() {
        assert!(SecurityFilePrioritizer::is_security_relevant("src/auth.rs"));
    }

    #[test]
    fn test_security_file_prioritizer_is_security_relevant_env_file_returns_true() {
        assert!(SecurityFilePrioritizer::is_security_relevant("config/.env"));
    }

    #[test]
    fn test_security_file_prioritizer_is_security_relevant_txt_file_returns_false() {
        assert!(!SecurityFilePrioritizer::is_security_relevant(
            "notes/todo.txt"
        ));
    }

    #[test]
    fn test_security_file_prioritizer_is_security_relevant_cargo_toml_returns_true() {
        assert!(SecurityFilePrioritizer::is_security_relevant("Cargo.toml"));
    }

    // -----------------------------------------------------------------------
    // SecurityFilePrioritizer::active_categories
    // -----------------------------------------------------------------------

    #[test]
    fn test_security_file_prioritizer_active_categories_all_flags_includes_secrets() {
        let cats = SecurityFilePrioritizer::active_categories(
            true, true, true, true, true, true, true, true,
        );
        assert!(cats.contains(&SecurityCategory::Secrets));
        assert!(cats.contains(&SecurityCategory::UnsafeRust));
        assert!(cats.contains(&SecurityCategory::Cryptography));
    }

    #[test]
    fn test_security_file_prioritizer_active_categories_secret_scanning_false_excludes_secrets() {
        let cats = SecurityFilePrioritizer::active_categories(
            false, false, false, false, false, false, false, false,
        );
        assert!(!cats.contains(&SecurityCategory::Secrets));
        assert!(!cats.contains(&SecurityCategory::EnvironmentVariables));
    }

    #[test]
    fn test_security_file_prioritizer_active_categories_always_includes_sensitive_logging() {
        let cats = SecurityFilePrioritizer::active_categories(
            false, false, false, false, false, false, false, false,
        );
        assert!(cats.contains(&SecurityCategory::SensitiveLogging));
        assert!(cats.contains(&SecurityCategory::ErrorLeakage));
        assert!(cats.contains(&SecurityCategory::FileOperations));
        assert!(cats.contains(&SecurityCategory::NetworkClients));
        assert!(cats.contains(&SecurityCategory::InputValidation));
        assert!(cats.contains(&SecurityCategory::OutputSanitization));
    }

    // -----------------------------------------------------------------------
    // SecurityFilePrioritizer::capped_flat_list
    // -----------------------------------------------------------------------

    #[test]
    fn test_security_file_prioritizer_capped_flat_list_respects_max_files() {
        let mut sr = minimal_scan_result();
        sr.security_relevant_files = vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
        ];

        let result = SecurityFilePrioritizer::capped_flat_list(&sr, 2, false);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "src/a.rs");
        assert_eq!(result[1], "src/b.rs");
    }

    #[test]
    fn test_security_file_prioritizer_capped_flat_list_deduplicates_files() {
        let mut sr = minimal_scan_result();
        sr.security_relevant_files = vec!["src/auth.rs".to_string(), "Cargo.toml".to_string()];
        sr.dependency_manifests = vec!["Cargo.toml".to_string()];

        let result = SecurityFilePrioritizer::capped_flat_list(&sr, 0, true);
        // "Cargo.toml" must appear exactly once.
        let toml_count = result.iter().filter(|p| p.as_str() == "Cargo.toml").count();
        assert_eq!(toml_count, 1);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_security_file_prioritizer_capped_flat_list_zero_max_returns_all() {
        let mut sr = minimal_scan_result();
        sr.security_relevant_files = (0..10).map(|i| format!("src/file_{i}.rs")).collect();

        let result = SecurityFilePrioritizer::capped_flat_list(&sr, 0, false);
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn test_security_file_prioritizer_capped_flat_list_includes_repo_structure_files() {
        let mut sr = minimal_scan_result();
        sr.repository_structure = vec![
            file_entry("src/main.rs"),
            file_entry("assets/logo.png"),
            file_entry("scripts/deploy.sh"),
        ];

        let result = SecurityFilePrioritizer::capped_flat_list(&sr, 0, false);
        assert!(result.contains(&"src/main.rs".to_string()));
        assert!(result.contains(&"scripts/deploy.sh".to_string()));
        // logo.png is not security-relevant.
        assert!(!result.contains(&"assets/logo.png".to_string()));
    }

    #[test]
    fn test_security_file_prioritizer_capped_flat_list_dep_manifests_excluded_when_flag_false() {
        let mut sr = minimal_scan_result();
        sr.dependency_manifests = vec!["Cargo.toml".to_string()];

        let result = SecurityFilePrioritizer::capped_flat_list(&sr, 0, false);
        // Cargo.toml would be picked up by the repo-structure heuristic only if
        // it's in repository_structure.  Here it is not, so it should be absent.
        assert!(!result.contains(&"Cargo.toml".to_string()));
    }

    #[test]
    fn test_security_file_prioritizer_capped_flat_list_dep_manifests_included_when_flag_true() {
        let mut sr = minimal_scan_result();
        sr.dependency_manifests = vec!["Cargo.toml".to_string()];

        let result = SecurityFilePrioritizer::capped_flat_list(&sr, 0, true);
        assert!(result.contains(&"Cargo.toml".to_string()));
    }

    // -----------------------------------------------------------------------
    // Additional language stats usage (ensures LanguageStats import is used)
    // -----------------------------------------------------------------------

    #[test]
    fn test_scan_result_with_language_stats_compiles() {
        let mut sr = minimal_scan_result();
        sr.language_statistics.insert(
            "Rust".to_string(),
            LanguageStats {
                file_count: 1,
                total_bytes: 512,
            },
        );
        assert_eq!(sr.language_statistics["Rust"].file_count, 1);
    }
}
