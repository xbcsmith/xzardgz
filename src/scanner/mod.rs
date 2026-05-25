//! Repository scanner module.
//!
//! The scanner walks a repository directory tree, respects `.gitignore` and
//! configured exclusion patterns, detects file languages, runs cross-cutting
//! hooks, categorises files for plugin preselection, and returns a
//! deterministic, versioned [`ScanResult`].
//!
//! ## Quick start
//!
//! ```no_run
//! use std::path::Path;
//! use xzardgz::scanner::{Scanner, ScannerConfig};
//!
//! let scanner = Scanner::new(
//!     ScannerConfig::default().with_exclude_patterns(vec!["target".to_string()]),
//! );
//! let result = scanner.scan(Path::new("/path/to/repo"), None).unwrap();
//! println!("primary language: {:?}", result.primary_language);
//! ```
//!
//! ## Module layout
//!
//! | Submodule     | Purpose                                         |
//! |---------------|-------------------------------------------------|
//! | [`config`]    | [`ScannerConfig`] — traversal settings          |
//! | [`findings`]  | [`FindingSeverity`], [`ScanFinding`]             |
//! | [`hooks`]     | [`CrossCutHook`] trait + built-in hooks         |
//! | [`language`]  | Language detection and binary detection         |
//! | [`patterns`]  | [`PatternSet`], [`PatternRegistry`]             |
//! | [`preselect`] | [`PluginContentScanner`]                        |
//! | [`result`]    | [`ScanResult`] and supporting data types        |
//! | [`scoring`]   | [`ScoringSignal`], [`ScoringInput`], [`ConfidenceScorer`] |

pub mod config;
pub mod findings;
pub mod hooks;
pub mod language;
pub mod patterns;
pub mod preselect;
pub mod result;
pub mod scoring;

// ---------------------------------------------------------------------------
// Public re-exports
// ---------------------------------------------------------------------------

pub use self::config::{DEFAULT_MAX_CONCURRENCY, DEFAULT_MAX_FILE_SIZE_BYTES, ScannerConfig};
pub use self::findings::{FindingSeverity, ScanFinding};
pub use self::hooks::{
    CommandExecutionHook, CrossCutHook, SecretsHook, UnsafeRustHook, default_hooks,
};
pub use self::language::{detect_language, is_binary_content};
pub use self::patterns::{PatternRegistry, PatternSet};
pub use self::preselect::PluginContentScanner;
pub use self::result::{
    FileEntry, LanguageStats, PluginPreselection, SCAN_RESULT_VERSION, ScanResult,
};
pub use self::scoring::{ConfidenceScorer, ScoringInput, ScoringSignal};

use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

use crate::error::{PipelineError, Result};

// ---------------------------------------------------------------------------
// File categorisation helpers
// ---------------------------------------------------------------------------

/// Internal categorisation flags derived from a file's path alone.
struct FileCategory {
    is_entrypoint: bool,
    is_documentation: bool,
    is_governance: bool,
    is_cli_command: bool,
    is_config: bool,
    is_dep_manifest: bool,
    is_test: bool,
    is_build: bool,
    is_key_file: bool,
    is_security_relevant: bool,
    is_public_api: bool,
}

fn is_entrypoint_file(name: &str) -> bool {
    matches!(
        name,
        "main.rs"
            | "lib.rs"
            | "main.py"
            | "app.py"
            | "index.js"
            | "index.ts"
            | "app.js"
            | "app.ts"
            | "main.go"
            | "main.c"
            | "main.cpp"
            | "server.py"
            | "server.js"
            | "server.ts"
            | "wsgi.py"
            | "asgi.py"
    )
}

fn is_doc_file(name: &str, rel_path: &str) -> bool {
    let name_upper = name.to_uppercase();
    name_upper.starts_with("README")
        || name_upper.starts_with("CHANGELOG")
        || name_upper.starts_with("HISTORY")
        || rel_path.ends_with(".md")
        || rel_path.ends_with(".rst")
        || rel_path.starts_with("docs/")
        || rel_path.contains("/docs/")
}

fn is_governance_file(name: &str, rel_path: &str) -> bool {
    let name_upper = name.to_uppercase();
    matches!(
        name_upper.as_str(),
        "CODEOWNERS"
            | "LICENSE"
            | "LICENSE.TXT"
            | "LICENSE.MD"
            | "SECURITY.MD"
            | "CONTRIBUTING.MD"
            | "CODE_OF_CONDUCT.MD"
    ) || rel_path.contains(".github/")
}

fn is_cli_file(name: &str, rel_path: &str) -> bool {
    name == "cli.rs"
        || rel_path.contains("/commands/")
        || rel_path.starts_with("commands/")
        || rel_path.contains("/cmd/")
        || rel_path.starts_with("cmd/")
}

fn is_config_file(name_lower: &str) -> bool {
    matches!(
        name_lower,
        "config.yaml"
            | "config.yml"
            | "config.toml"
            | "config.json"
            | "settings.yaml"
            | "settings.toml"
            | "settings.json"
            | "application.yaml"
            | "application.yml"
            | "application.properties"
    ) || name_lower.contains("config")
        || name_lower.contains("settings")
        || (name_lower.starts_with('.') && name_lower.ends_with("rc"))
}

fn is_dep_manifest_file(name: &str) -> bool {
    matches!(
        name,
        "Cargo.toml"
            | "Cargo.lock"
            | "package.json"
            | "package-lock.json"
            | "yarn.lock"
            | "requirements.txt"
            | "requirements-dev.txt"
            | "Pipfile"
            | "Pipfile.lock"
            | "pyproject.toml"
            | "poetry.lock"
            | "go.mod"
            | "go.sum"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "Gemfile"
            | "Gemfile.lock"
            | "composer.json"
            | "composer.lock"
            | "pubspec.yaml"
    )
}

fn is_test_file(name_lower: &str, rel_path: &str) -> bool {
    name_lower.starts_with("test_")
        || name_lower.ends_with("_test.rs")
        || name_lower.ends_with("_test.py")
        || name_lower.ends_with("_test.go")
        || name_lower.ends_with("_spec.rb")
        || name_lower.ends_with(".test.js")
        || name_lower.ends_with(".test.ts")
        || name_lower.ends_with(".spec.js")
        || name_lower.ends_with(".spec.ts")
        || rel_path.contains("/tests/")
        || rel_path.starts_with("tests/")
        || rel_path.contains("/test/")
        || rel_path.starts_with("test/")
        || rel_path.contains("/__tests__/")
        || rel_path.contains("/spec/")
}

fn is_build_file(name: &str, rel_path: &str) -> bool {
    matches!(
        name,
        "Makefile"
            | "makefile"
            | "GNUmakefile"
            | "build.rs"
            | "build.py"
            | "build.sh"
            | "CMakeLists.txt"
            | "configure"
            | "configure.ac"
            | "Dockerfile"
            | ".dockerignore"
            | "docker-compose.yaml"
            | "docker-compose.yml"
            | "Jenkinsfile"
            | "Taskfile.yaml"
    ) || rel_path.contains(".github/workflows/")
        || rel_path.contains(".circleci/")
        || rel_path.contains(".gitlab-ci")
}

fn is_key_file(name: &str) -> bool {
    matches!(
        name,
        "README.md"
            | "README"
            | "README.txt"
            | "README.rst"
            | "LICENSE"
            | "LICENSE.txt"
            | "LICENSE.md"
            | "CONTRIBUTING.md"
            | "CHANGELOG.md"
            | "CHANGELOG"
            | "CODE_OF_CONDUCT.md"
            | "SECURITY.md"
    )
}

fn is_security_relevant_file(name_lower: &str, rel_path: &str) -> bool {
    name_lower == ".env"
        || name_lower.starts_with(".env.")
        || name_lower.contains("secret")
        || name_lower.contains("credential")
        || name_lower.contains("password")
        || rel_path.contains("secrets/")
        || rel_path.contains("credentials/")
}

fn is_public_api_file(name: &str, rel_path: &str, name_lower: &str) -> bool {
    // Rust non-test .rs files
    (name.ends_with(".rs") && !is_test_file(name_lower, rel_path))
        // Python non-test .py files
        || (name.ends_with(".py")
            && !is_test_file(name_lower, rel_path)
            && !rel_path.contains("test"))
        // Go non-test .go files
        || (name.ends_with(".go") && !is_test_file(name_lower, rel_path))
        // JS/TS index files
        || matches!(name, "index.js" | "index.ts")
}

fn categorize(rel_path: &str) -> FileCategory {
    let path = Path::new(rel_path);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let name_lower = name.to_lowercase();

    FileCategory {
        is_entrypoint: is_entrypoint_file(name),
        is_documentation: is_doc_file(name, rel_path),
        is_governance: is_governance_file(name, rel_path),
        is_cli_command: is_cli_file(name, rel_path),
        is_config: is_config_file(&name_lower),
        is_dep_manifest: is_dep_manifest_file(name),
        is_test: is_test_file(&name_lower, rel_path),
        is_build: is_build_file(name, rel_path),
        is_key_file: is_key_file(name),
        is_security_relevant: is_security_relevant_file(&name_lower, rel_path),
        is_public_api: is_public_api_file(name, rel_path, &name_lower),
    }
}

// ---------------------------------------------------------------------------
// Content-based preselection flags
// ---------------------------------------------------------------------------

const SECRETS_CONTENT_PATTERNS: &[&str] = &[
    "password=",
    "secret=",
    "api_key=",
    "apikey=",
    "token=",
    "private_key=",
    "access_key=",
    "SECRET_KEY",
    "API_KEY",
    "PRIVATE_KEY",
    "-----BEGIN RSA PRIVATE KEY",
    "-----BEGIN EC PRIVATE KEY",
    "-----BEGIN OPENSSH PRIVATE KEY",
];

const CMD_EXEC_PATTERNS: &[&str] = &[
    "std::process::Command",
    "subprocess.",
    "os.system(",
    "shell_exec(",
    "popen(",
    "Process::new(",
];

const NETWORK_PATTERNS: &[&str] = &[
    "reqwest::",
    "hyper::",
    "TcpStream",
    "HttpClient",
    "axios",
    "fetch(",
    "requests.get",
    "requests.post",
    "urllib.request",
];

const AUTH_PATTERNS: &[&str] = &[
    "authenticate",
    "authorize",
    " jwt",
    " oauth",
    "bearer",
    "bcrypt",
    "argon2",
    "verify_password",
    "password_hash",
    "session_token",
    "check_password",
];

const RISKY_PATTERNS: &[&str] = &[
    "eval(",
    "exec(",
    "__import__(",
    "raw_query",
    "execute(",
    "format!(\"SELECT",
    "format!(\"INSERT",
    "format!(\"UPDATE",
    "format!(\"DELETE",
];

struct ContentFlags {
    has_risky: bool,
    has_secrets: bool,
    has_unsafe_rust: bool,
    has_cmd_exec: bool,
    has_network: bool,
    has_auth: bool,
}

fn detect_content_flags(rel_path: &str, content: &str) -> ContentFlags {
    let content_lower = content.to_lowercase();
    ContentFlags {
        has_risky: RISKY_PATTERNS.iter().any(|p| content.contains(p)),
        has_secrets: SECRETS_CONTENT_PATTERNS.iter().any(|p| content.contains(p)),
        has_unsafe_rust: rel_path.ends_with(".rs")
            && (content.contains("unsafe {")
                || content.contains("unsafe fn")
                || content.contains("unsafe impl")),
        has_cmd_exec: CMD_EXEC_PATTERNS.iter().any(|p| content.contains(p)),
        has_network: NETWORK_PATTERNS.iter().any(|p| content.contains(p)),
        has_auth: AUTH_PATTERNS.iter().any(|p| content_lower.contains(p)),
    }
}

// ---------------------------------------------------------------------------
// Framework detection
// ---------------------------------------------------------------------------

fn detect_frameworks(files: &[FileEntry]) -> Vec<String> {
    let mut frameworks = Vec::new();
    let paths: Vec<&str> = files.iter().map(|e| e.path.as_str()).collect();

    if paths.iter().any(|p| p.ends_with("Cargo.toml")) {
        frameworks.push("Rust".to_string());
    }
    if paths.iter().any(|p| p.ends_with("package.json")) {
        frameworks.push("Node.js".to_string());
    }
    if paths.iter().any(|p| {
        p.ends_with("requirements.txt") || p.ends_with("setup.py") || p.ends_with("pyproject.toml")
    }) {
        frameworks.push("Python".to_string());
    }
    if paths.iter().any(|p| p.ends_with("go.mod")) {
        frameworks.push("Go".to_string());
    }
    if paths.iter().any(|p| p.ends_with("pom.xml")) {
        frameworks.push("Maven".to_string());
    }
    if paths
        .iter()
        .any(|p| p.ends_with("build.gradle") || p.ends_with("build.gradle.kts"))
    {
        frameworks.push("Gradle".to_string());
    }
    if paths.iter().any(|p| p.ends_with("Dockerfile")) {
        frameworks.push("Docker".to_string());
    }
    if paths.iter().any(|p| p.contains(".github/workflows/")) {
        frameworks.push("GitHub Actions".to_string());
    }
    if paths.iter().any(|p| p.ends_with("manage.py")) {
        frameworks.push("Django".to_string());
    }
    if paths.iter().any(|p| p.ends_with("CMakeLists.txt")) {
        frameworks.push("CMake".to_string());
    }
    frameworks
}

// ---------------------------------------------------------------------------
// Missing test signals
// ---------------------------------------------------------------------------

fn compute_missing_test_signals(entries: &[FileEntry], test_files: &[String]) -> Vec<String> {
    let mut signals = Vec::new();
    for entry in entries {
        if entry.is_binary {
            continue;
        }
        let Some(lang) = &entry.language else {
            continue;
        };
        if !matches!(
            lang.as_str(),
            "Rust" | "Python" | "Go" | "JavaScript" | "TypeScript"
        ) {
            continue;
        }
        let name_lower = Path::new(&entry.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        if is_test_file(&name_lower, &entry.path) {
            continue;
        }
        let stem = Path::new(&entry.path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if !stem.is_empty() && !test_files.iter().any(|t| t.contains(stem)) {
            signals.push(entry.path.clone());
        }
    }
    signals
}

// ---------------------------------------------------------------------------
// Sort + dedup helper
// ---------------------------------------------------------------------------

fn sort_dedup(v: &mut Vec<String>) {
    v.sort();
    v.dedup();
}

// ---------------------------------------------------------------------------
// Scanner
// ---------------------------------------------------------------------------

/// Repository scanner that produces a structured, deterministic [`ScanResult`].
///
/// The scanner walks the repository directory tree using the `ignore` crate
/// (which respects `.gitignore` by default), applies configured exclusions,
/// detects file languages, skips binary and oversized files, runs
/// cross-cutting hooks, and categorises every file for plugin preselection.
///
/// Output ordering is deterministic: file paths are sorted alphabetically.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use xzardgz::scanner::{Scanner, ScannerConfig};
///
/// let scanner = Scanner::new(
///     ScannerConfig::default().with_exclude_patterns(vec!["target".to_string()]),
/// );
/// // SAFETY: only valid with a real repository path
/// let result = scanner.scan(Path::new("/path/to/repo"), None).unwrap();
/// println!("files: {}", result.repository_structure.len());
/// ```
pub struct Scanner {
    config: ScannerConfig,
    registry: PatternRegistry,
    hooks: Vec<Box<dyn CrossCutHook>>,
}

impl Scanner {
    /// Creates a new `Scanner` with the given configuration.
    ///
    /// Uses the [`PatternRegistry::default_registry`] and no hooks.
    /// Add hooks with [`Scanner::with_hook`].
    ///
    /// # Arguments
    ///
    /// * `config` - Controls traversal, size limits, and exclusion patterns.
    ///
    /// # Returns
    ///
    /// A configured `Scanner`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::{Scanner, ScannerConfig};
    ///
    /// let scanner = Scanner::new(ScannerConfig::default());
    /// ```
    pub fn new(config: ScannerConfig) -> Self {
        Self {
            registry: PatternRegistry::default_registry(),
            hooks: Vec::new(),
            config,
        }
    }

    /// Creates a `Scanner` with all default settings.
    ///
    /// # Returns
    ///
    /// A `Scanner` using [`ScannerConfig::default`] and the default pattern registry.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::Scanner;
    ///
    /// let scanner = Scanner::default_scanner();
    /// ```
    pub fn default_scanner() -> Self {
        Self::new(ScannerConfig::default())
    }

    /// Returns a reference to the pattern registry used by this scanner.
    ///
    /// Plugins and callers can use the registry to look up built-in pattern
    /// sets or to check which custom patterns have been registered.
    ///
    /// # Returns
    ///
    /// A reference to the scanner's [`PatternRegistry`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::{Scanner, ScannerConfig};
    ///
    /// let scanner = Scanner::new(ScannerConfig::default());
    /// assert!(scanner.registry().get("secrets").is_some());
    /// ```
    pub fn registry(&self) -> &PatternRegistry {
        &self.registry
    }

    /// Adds a cross-cutting hook applied to every non-binary file.
    ///
    /// # Arguments
    ///
    /// * `hook` - A boxed [`CrossCutHook`] implementation.
    ///
    /// # Returns
    ///
    /// The updated `Scanner` (builder pattern).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::{Scanner, ScannerConfig, SecretsHook};
    ///
    /// let scanner = Scanner::new(ScannerConfig::default())
    ///     .with_hook(Box::new(SecretsHook));
    /// ```
    pub fn with_hook(mut self, hook: Box<dyn CrossCutHook>) -> Self {
        self.hooks.push(hook);
        self
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn collect_paths(&self, root: &Path) -> Result<Vec<PathBuf>> {
        let mut builder = WalkBuilder::new(root);
        builder.hidden(!self.config.include_hidden);
        builder.git_ignore(self.config.respect_gitignore);

        if !self.config.exclude_patterns.is_empty() {
            let mut overrides = ignore::overrides::OverrideBuilder::new(root);
            // Include everything by default; negations exclude specific patterns.
            overrides
                .add("**")
                .map_err(|e| PipelineError::Scanner(format!("override error: {e}")))?;
            for pat in &self.config.exclude_patterns {
                overrides.add(&format!("!{pat}")).map_err(|e| {
                    PipelineError::Scanner(format!("bad exclude pattern '{pat}': {e}"))
                })?;
            }
            let built = overrides
                .build()
                .map_err(|e| PipelineError::Scanner(format!("override build error: {e}")))?;
            builder.overrides(built);
        }

        let mut paths = Vec::new();
        for entry in builder.build() {
            match entry {
                Ok(e) if e.file_type().is_some_and(|ft| ft.is_file()) => {
                    paths.push(e.into_path());
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("scanner walk error: {}", e),
            }
        }
        paths.sort();
        Ok(paths)
    }

    // -----------------------------------------------------------------------
    // Public scan API
    // -----------------------------------------------------------------------

    /// Scans the repository rooted at `root` and returns a structured result.
    ///
    /// The scan is synchronous and sequential. Output ordering is
    /// deterministic: all file path lists are sorted alphabetically.
    ///
    /// Binary files (detected by null-byte heuristic) are included in the
    /// file inventory but skipped during hook execution and content analysis.
    /// Files exceeding `config.max_file_size_bytes` are omitted entirely.
    ///
    /// # Arguments
    ///
    /// * `root` - Path to the repository root directory.
    /// * `git_metadata` - Optional git metadata to embed in the result.
    ///
    /// # Returns
    ///
    /// A versioned [`ScanResult`].
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Scanner`] if `root` does not exist, is not a
    /// directory, or directory traversal fails unrecoverably.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use xzardgz::scanner::{Scanner, ScannerConfig};
    ///
    /// let scanner = Scanner::new(ScannerConfig::default());
    /// // SAFETY: only valid with a real repository path
    /// let result = scanner.scan(Path::new("/tmp/my_repo"), None).unwrap();
    /// assert_eq!(result.version, "1");
    /// ```
    pub fn scan(
        &self,
        root: &Path,
        git_metadata: Option<&crate::git::metadata::GitMetadata>,
    ) -> Result<ScanResult> {
        if !root.exists() {
            return Err(PipelineError::Scanner(format!(
                "root path does not exist: {}",
                root.display()
            )));
        }
        if !root.is_dir() {
            return Err(PipelineError::Scanner(format!(
                "root path is not a directory: {}",
                root.display()
            )));
        }

        let all_paths = self.collect_paths(root)?;

        // Categorised file list accumulators
        let mut file_entries: Vec<FileEntry> = Vec::new();
        let mut all_findings: Vec<ScanFinding> = Vec::new();

        let mut entrypoints: Vec<String> = Vec::new();
        let mut public_apis: Vec<String> = Vec::new();
        let mut config_surface: Vec<String> = Vec::new();
        let mut dep_manifests: Vec<String> = Vec::new();
        let mut test_files_list: Vec<String> = Vec::new();
        let mut build_files_list: Vec<String> = Vec::new();
        let mut doc_inventory: Vec<String> = Vec::new();
        let mut governance_rules: Vec<String> = Vec::new();
        let mut cli_commands: Vec<String> = Vec::new();
        let mut key_files: Vec<String> = Vec::new();
        let mut security_files: Vec<String> = Vec::new();

        // Preselection accumulators
        let mut risky_files: Vec<String> = Vec::new();
        let mut secrets_files: Vec<String> = Vec::new();
        let mut unsafe_rust_files: Vec<String> = Vec::new();
        let mut cmd_exec_files: Vec<String> = Vec::new();
        let mut network_files: Vec<String> = Vec::new();
        let mut auth_files_list: Vec<String> = Vec::new();

        for abs_path in &all_paths {
            let metadata = match std::fs::metadata(abs_path) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(
                        "scanner: failed to read metadata for {}: {}",
                        abs_path.display(),
                        e
                    );
                    continue;
                }
            };
            let size = metadata.len();

            if size > self.config.max_file_size_bytes {
                tracing::debug!(
                    "scanner: skipping large file {} ({} bytes)",
                    abs_path.display(),
                    size
                );
                continue;
            }

            // Build a portable, repository-relative path
            let rel_path = abs_path
                .strip_prefix(root)
                .unwrap_or(abs_path)
                .to_string_lossy()
                .replace('\\', "/");

            let content_bytes = match std::fs::read(abs_path) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("scanner: failed to read {}: {}", abs_path.display(), e);
                    continue;
                }
            };

            let is_binary = is_binary_content(&content_bytes);
            let language = detect_language(abs_path);

            let entry = FileEntry {
                path: rel_path.clone(),
                size_bytes: size,
                language: language.clone(),
                is_binary,
            };

            // Path-based categorisation
            let cat = categorize(&rel_path);

            if cat.is_entrypoint {
                entrypoints.push(rel_path.clone());
            }
            if cat.is_public_api {
                public_apis.push(rel_path.clone());
            }
            if cat.is_config {
                config_surface.push(rel_path.clone());
            }
            if cat.is_dep_manifest {
                dep_manifests.push(rel_path.clone());
            }
            if cat.is_test {
                test_files_list.push(rel_path.clone());
            }
            if cat.is_build {
                build_files_list.push(rel_path.clone());
            }
            if cat.is_documentation {
                doc_inventory.push(rel_path.clone());
            }
            if cat.is_governance {
                governance_rules.push(rel_path.clone());
            }
            if cat.is_cli_command {
                cli_commands.push(rel_path.clone());
            }
            if cat.is_key_file {
                key_files.push(rel_path.clone());
            }
            if cat.is_security_relevant {
                security_files.push(rel_path.clone());
            }

            // Content analysis for non-binary files
            if !is_binary {
                let content_str = String::from_utf8_lossy(&content_bytes).to_string();

                for hook in &self.hooks {
                    all_findings.extend(hook.scan(abs_path, &content_str));
                }

                let flags = detect_content_flags(&rel_path, &content_str);
                if flags.has_risky {
                    risky_files.push(rel_path.clone());
                }
                if flags.has_secrets {
                    secrets_files.push(rel_path.clone());
                }
                if flags.has_unsafe_rust {
                    unsafe_rust_files.push(rel_path.clone());
                }
                if flags.has_cmd_exec {
                    cmd_exec_files.push(rel_path.clone());
                }
                if flags.has_network {
                    network_files.push(rel_path.clone());
                }
                if flags.has_auth {
                    auth_files_list.push(rel_path.clone());
                }
            }

            file_entries.push(entry);
        }

        // Deterministic ordering
        file_entries.sort_by(|a, b| a.path.cmp(&b.path));
        all_findings.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
        sort_dedup(&mut entrypoints);
        sort_dedup(&mut public_apis);
        sort_dedup(&mut config_surface);
        sort_dedup(&mut dep_manifests);
        sort_dedup(&mut test_files_list);
        sort_dedup(&mut build_files_list);
        sort_dedup(&mut doc_inventory);
        sort_dedup(&mut governance_rules);
        sort_dedup(&mut cli_commands);
        sort_dedup(&mut key_files);
        sort_dedup(&mut security_files);
        sort_dedup(&mut risky_files);
        sort_dedup(&mut secrets_files);
        sort_dedup(&mut unsafe_rust_files);
        sort_dedup(&mut cmd_exec_files);
        sort_dedup(&mut network_files);
        sort_dedup(&mut auth_files_list);

        // Language statistics
        let mut lang_stats: std::collections::HashMap<String, LanguageStats> =
            std::collections::HashMap::new();
        for entry in &file_entries {
            if entry.is_binary {
                continue;
            }
            if let Some(lang) = &entry.language {
                let s = lang_stats.entry(lang.clone()).or_insert(LanguageStats {
                    file_count: 0,
                    total_bytes: 0,
                });
                s.file_count += 1;
                s.total_bytes += entry.size_bytes;
            }
        }

        let primary_language = lang_stats
            .iter()
            .max_by_key(|(_, s)| s.file_count)
            .map(|(lang, _)| lang.clone());

        let frameworks = detect_frameworks(&file_entries);
        let missing_tests = compute_missing_test_signals(&file_entries, &test_files_list);

        let plugin_preselection = PluginPreselection {
            entrypoints: entrypoints.clone(),
            public_apis: public_apis.clone(),
            config_surfaces: config_surface.clone(),
            dependency_manifests: dep_manifests.clone(),
            risky_pattern_files: risky_files,
            secrets_like_files: secrets_files,
            unsafe_rust_files,
            command_execution_files: cmd_exec_files,
            network_client_files: network_files,
            auth_files: auth_files_list,
            test_files: test_files_list.clone(),
            missing_test_signals: missing_tests,
        };

        let repo_name = root.file_name().map(|n| n.to_string_lossy().to_string());

        Ok(ScanResult {
            version: SCAN_RESULT_VERSION.to_string(),
            repository_url: git_metadata.and_then(|m| m.repository_url.clone()),
            repository_name: repo_name,
            head_commit: git_metadata.and_then(|m| m.head_commit.clone()),
            scan_timestamp: chrono::Utc::now(),
            repository_structure: file_entries,
            language_statistics: lang_stats,
            primary_language,
            frameworks,
            documentation_inventory: doc_inventory,
            governance_rules,
            cli_commands,
            public_apis,
            entrypoints,
            config_surface,
            key_files,
            dependency_manifests: dep_manifests,
            test_files: test_files_list,
            build_files: build_files_list,
            security_relevant_files: security_files,
            findings: all_findings,
            plugin_preselection,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_scanner() -> Scanner {
        Scanner::new(ScannerConfig::default())
    }

    fn make_dir() -> TempDir {
        tempfile::tempdir().expect("SAFETY: test tempdir creation")
    }

    fn write_file(dir: &TempDir, rel: &str, content: &str) {
        let path = dir.path().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("SAFETY: test dir creation");
        }
        fs::write(&path, content).expect("SAFETY: test file write");
    }

    fn write_bytes(dir: &TempDir, rel: &str, content: &[u8]) {
        let path = dir.path().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("SAFETY: test dir creation");
        }
        fs::write(&path, content).expect("SAFETY: test bytes write");
    }

    // -----------------------------------------------------------------------
    // 7.5 Testing requirements
    // -----------------------------------------------------------------------

    #[test]
    fn test_scan_empty_repository_returns_empty_structure() {
        let dir = make_dir();
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: empty dir scan");
        assert!(result.repository_structure.is_empty());
        assert_eq!(result.version, SCAN_RESULT_VERSION);
        assert!(result.language_statistics.is_empty());
        assert!(result.primary_language.is_none());
        assert!(result.frameworks.is_empty());
    }

    #[test]
    fn test_scan_rust_repository_detects_rust_language() {
        let dir = make_dir();
        // Three Rust files vs one TOML file — Rust must be the primary language.
        write_file(&dir, "src/main.rs", "fn main() { println!(\"hello\"); }");
        write_file(&dir, "src/lib.rs", "pub fn hello() {}");
        write_file(&dir, "src/utils.rs", "pub fn util() {}");
        write_file(
            &dir,
            "Cargo.toml",
            "[package]\nname = \"test\"\nversion = \"0.1.0\"",
        );
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: rust repo scan");
        assert_eq!(result.primary_language.as_deref(), Some("Rust"));
        let rust_stats = result
            .language_statistics
            .get("Rust")
            .expect("Rust stats missing");
        assert_eq!(rust_stats.file_count, 3);
    }

    #[test]
    fn test_scan_mixed_language_repository_detects_multiple_languages() {
        let dir = make_dir();
        write_file(&dir, "main.rs", "fn main() {}");
        write_file(&dir, "script.py", "print('hello')");
        write_file(&dir, "index.js", "console.log('hello');");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: mixed repo scan");
        assert!(result.language_statistics.contains_key("Rust"));
        assert!(result.language_statistics.contains_key("Python"));
        assert!(result.language_statistics.contains_key("JavaScript"));
        assert_eq!(result.language_statistics.len(), 3);
    }

    #[test]
    fn test_scan_respects_max_file_size_limit() {
        let dir = make_dir();
        // big.rs = 100 bytes, small.rs < 20 bytes
        write_file(&dir, "big.rs", &"x".repeat(100));
        write_file(&dir, "small.rs", "fn s() {}");
        let scanner = Scanner::new(ScannerConfig::default().with_max_file_size(50));
        let result = scanner
            .scan(dir.path(), None)
            .expect("SAFETY: max size scan");
        assert!(
            result
                .repository_structure
                .iter()
                .all(|e| e.path != "big.rs"),
            "big.rs should be excluded"
        );
        assert!(
            result
                .repository_structure
                .iter()
                .any(|e| e.path == "small.rs"),
            "small.rs should be included"
        );
    }

    #[test]
    fn test_scan_marks_binary_files_correctly() {
        let dir = make_dir();
        // PNG header contains a null byte
        write_bytes(
            &dir,
            "image.png",
            &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x00],
        );
        write_file(&dir, "text.rs", "fn hello() {}");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: binary scan");
        let binary = result
            .repository_structure
            .iter()
            .find(|e| e.path.ends_with("image.png"))
            .expect("image.png must be in structure");
        assert!(binary.is_binary, "image.png must be marked binary");
        let text = result
            .repository_structure
            .iter()
            .find(|e| e.path == "text.rs")
            .expect("text.rs must be in structure");
        assert!(!text.is_binary, "text.rs must not be binary");
    }

    #[test]
    fn test_scan_language_statistics_counts_are_correct() {
        let dir = make_dir();
        write_file(&dir, "a.rs", "fn a() {}");
        write_file(&dir, "b.rs", "fn b() {}");
        write_file(&dir, "c.rs", "fn c() {}");
        write_file(&dir, "main.py", "pass");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: stats scan");
        let rust = result
            .language_statistics
            .get("Rust")
            .expect("Rust stats missing");
        assert_eq!(rust.file_count, 3);
        let py = result
            .language_statistics
            .get("Python")
            .expect("Python stats missing");
        assert_eq!(py.file_count, 1);
    }

    #[test]
    fn test_scan_framework_detection_rust() {
        let dir = make_dir();
        write_file(&dir, "Cargo.toml", "[package]\nname = \"test\"");
        write_file(&dir, "src/main.rs", "fn main() {}");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: framework scan");
        assert!(result.frameworks.contains(&"Rust".to_string()));
    }

    #[test]
    fn test_scan_framework_detection_docker() {
        let dir = make_dir();
        write_file(&dir, "Dockerfile", "FROM rust:latest\nCMD [\"/bin/sh\"]");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: docker framework scan");
        assert!(result.frameworks.contains(&"Docker".to_string()));
    }

    #[test]
    fn test_scan_framework_detection_node() {
        let dir = make_dir();
        write_file(&dir, "package.json", "{\"name\": \"app\"}");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: node framework scan");
        assert!(result.frameworks.contains(&"Node.js".to_string()));
    }

    #[test]
    fn test_scan_cli_command_detection() {
        let dir = make_dir();
        fs::create_dir_all(dir.path().join("src/commands")).expect("SAFETY: mkdir commands");
        write_file(&dir, "src/cli.rs", "pub fn cli() {}");
        write_file(&dir, "src/commands/run.rs", "pub fn run() {}");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: cli scan");
        assert!(
            result.cli_commands.iter().any(|p| p.contains("cli.rs")),
            "cli.rs must be in cli_commands"
        );
        assert!(
            result.cli_commands.iter().any(|p| p.contains("commands")),
            "commands/ files must be in cli_commands"
        );
    }

    #[test]
    fn test_scan_public_api_detection() {
        let dir = make_dir();
        write_file(&dir, "src/lib.rs", "pub fn public_fn() {}");
        write_file(&dir, "src/internal.rs", "fn private_fn() {}");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: public api scan");
        assert!(!result.public_apis.is_empty());
        assert!(result.public_apis.iter().any(|p| p.ends_with("lib.rs")));
    }

    #[test]
    fn test_scan_entrypoint_detection() {
        let dir = make_dir();
        write_file(&dir, "src/main.rs", "fn main() {}");
        write_file(&dir, "src/lib.rs", "pub fn lib() {}");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: entrypoint scan");
        assert!(
            result.entrypoints.iter().any(|p| p.ends_with("main.rs")),
            "main.rs must be in entrypoints"
        );
        assert!(
            result.entrypoints.iter().any(|p| p.ends_with("lib.rs")),
            "lib.rs must be in entrypoints"
        );
    }

    #[test]
    fn test_scan_config_surface_detection() {
        let dir = make_dir();
        write_file(&dir, "config.yaml", "key: value");
        write_file(&dir, "settings.toml", "[server]\nport = 8080");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: config scan");
        assert!(
            result
                .config_surface
                .iter()
                .any(|p| p.ends_with("config.yaml")),
            "config.yaml must be in config_surface"
        );
    }

    #[test]
    fn test_scan_dependency_manifest_detection() {
        let dir = make_dir();
        write_file(&dir, "Cargo.toml", "[package]\nname = \"t\"");
        write_file(&dir, "package.json", "{\"name\": \"t\"}");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: manifest scan");
        assert!(
            result
                .dependency_manifests
                .iter()
                .any(|p| p.ends_with("Cargo.toml"))
        );
        assert!(
            result
                .dependency_manifests
                .iter()
                .any(|p| p.ends_with("package.json"))
        );
    }

    #[test]
    fn test_scan_security_preselection_secrets() {
        let dir = make_dir();
        write_file(&dir, "config.rs", "const API_KEY: &str = \"abc123\";");
        write_file(&dir, "clean.rs", "fn hello() {}");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: secrets preselection scan");
        assert!(
            result
                .plugin_preselection
                .secrets_like_files
                .iter()
                .any(|p| p.ends_with("config.rs")),
            "config.rs must be in secrets_like_files"
        );
        assert!(
            !result
                .plugin_preselection
                .secrets_like_files
                .iter()
                .any(|p| p.ends_with("clean.rs")),
            "clean.rs must not be in secrets_like_files"
        );
    }

    #[test]
    fn test_scan_security_preselection_unsafe_rust() {
        let dir = make_dir();
        write_file(&dir, "risky.rs", "fn bad() { unsafe { let _x: i32 = 1; } }");
        write_file(&dir, "safe.rs", "fn good() {}");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: unsafe preselection scan");
        assert!(
            result
                .plugin_preselection
                .unsafe_rust_files
                .iter()
                .any(|p| p.ends_with("risky.rs")),
            "risky.rs must be in unsafe_rust_files"
        );
        assert!(
            !result
                .plugin_preselection
                .unsafe_rust_files
                .iter()
                .any(|p| p.ends_with("safe.rs")),
            "safe.rs must not be in unsafe_rust_files"
        );
    }

    #[test]
    fn test_scan_output_is_deterministic() {
        let dir = make_dir();
        write_file(&dir, "b.rs", "fn b() {}");
        write_file(&dir, "a.rs", "fn a() {}");
        write_file(&dir, "c.rs", "fn c() {}");
        let scanner = make_scanner();
        let r1 = scanner.scan(dir.path(), None).expect("SAFETY: scan 1");
        let r2 = scanner.scan(dir.path(), None).expect("SAFETY: scan 2");
        let p1: Vec<&str> = r1
            .repository_structure
            .iter()
            .map(|e| e.path.as_str())
            .collect();
        let p2: Vec<&str> = r2
            .repository_structure
            .iter()
            .map(|e| e.path.as_str())
            .collect();
        assert_eq!(p1, p2, "scan output must be deterministic");
        let mut sorted = p1.clone();
        sorted.sort();
        assert_eq!(p1, sorted, "file paths must be in sorted order");
    }

    #[test]
    fn test_scan_nonexistent_root_returns_error() {
        let result = make_scanner().scan(Path::new("/nonexistent/xyz_repo"), None);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Scanner(_)));
    }

    #[test]
    fn test_scan_test_file_detection() {
        let dir = make_dir();
        fs::create_dir_all(dir.path().join("tests")).expect("SAFETY: mkdir tests");
        write_file(&dir, "tests/integration_test.rs", "#[test] fn t() {}");
        write_file(&dir, "src/foo_test.rs", "#[test] fn t2() {}");
        write_file(&dir, "src/lib.rs", "pub fn lib() {}");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: test file scan");
        assert!(
            result
                .test_files
                .iter()
                .any(|p| p.contains("integration_test")),
            "integration_test.rs must be in test_files"
        );
        assert!(
            result.test_files.iter().any(|p| p.contains("foo_test")),
            "foo_test.rs must be in test_files"
        );
        assert!(
            !result.test_files.iter().any(|p| p.ends_with("lib.rs")),
            "lib.rs must not be in test_files"
        );
    }

    #[test]
    fn test_scan_with_hook_produces_findings() {
        let dir = make_dir();
        write_file(&dir, "src/ops.rs", "let api_key=\"secret123\";");
        let scanner = Scanner::new(ScannerConfig::default()).with_hook(Box::new(SecretsHook));
        let result = scanner.scan(dir.path(), None).expect("SAFETY: hook scan");
        assert!(
            !result.findings.is_empty(),
            "SecretsHook must produce findings on api_key= pattern"
        );
        assert!(result.findings.iter().any(|f| f.kind == "secrets"));
    }

    #[test]
    fn test_scan_ignored_files_via_gitignore() {
        let dir = make_dir();
        // The ignore crate only processes .gitignore when inside a git repository.
        // Initialise a bare repo so gitignore rules take effect.
        git2::Repository::init(dir.path()).expect("SAFETY: git init for test");
        write_file(&dir, ".gitignore", "ignored_dir/\n");
        fs::create_dir_all(dir.path().join("ignored_dir")).expect("SAFETY: mkdir ignored");
        write_file(&dir, "ignored_dir/secret.rs", "fn ignored() {}");
        write_file(&dir, "src/kept.rs", "fn kept() {}");
        let result = make_scanner()
            .scan(dir.path(), None)
            .expect("SAFETY: gitignore scan");
        assert!(
            !result
                .repository_structure
                .iter()
                .any(|e| e.path.contains("ignored_dir")),
            "files in ignored_dir must be excluded by .gitignore"
        );
        assert!(
            result
                .repository_structure
                .iter()
                .any(|e| e.path.contains("kept.rs")),
            "kept.rs must be included"
        );
    }
}
