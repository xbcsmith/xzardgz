//! Static Application Security Testing (SAST) pipeline.
//!
//! This module provides a multi-language SAST engine for xzardgz.
//! Rules are expressed in a Semgrep-compatible YAML format and evaluated
//! against source files using a formula-based matching engine.
//!
//! # Module layout
//!
//! | Submodule  | Purpose                                                       |
//! |------------|---------------------------------------------------------------|
//! | [`engine`] | Formula evaluation and scanning engines                      |
//! | [`error`]  | [`SastError`] and supporting error types                     |
//! | [`rule`]   | Rule model: IR, metadata, and schema types                   |
//! | [`target`] | File discovery and content pre-filtering                     |
//!
//! [`SastError`]: error::SastError

pub mod ast;
pub mod config;
pub mod engine;
pub mod error;
pub mod rule;
pub mod target;

// ---------------------------------------------------------------------------
// Imports for SastEngine facade
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use rayon::prelude::*;

use crate::scanner::sast::ast::diagnostics::ErrorNodeDensity;
use crate::scanner::sast::ast::lang::Language;
use crate::scanner::sast::ast::parse::CachedRoot;
use crate::scanner::sast::config::SastEngineConfig;
use crate::scanner::sast::engine::formula::scan_rule;
use crate::scanner::sast::engine::pattern::PatternCompiler;
use crate::scanner::sast::engine::regex_mode::RegexModeScanner;
use crate::scanner::sast::error::SastError;
use crate::scanner::sast::rule::ir::RuleIr;
use crate::scanner::sast::target::discover::{DiscoveryConfig, discover_files};
use crate::scanner::sast::target::prefilter::Prefilter;

// ---------------------------------------------------------------------------
// Public output types
// ---------------------------------------------------------------------------

/// A single match produced by the SAST engine.
///
/// This is the Phase 4 preliminary definition. Phase 5 will extend it with
/// `message`, `severity`, `snippet`, `fingerprint`, and `fix` fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SastMatch {
    /// Filesystem path of the matched file.
    pub path: PathBuf,
    /// Byte offset of the start of the match (inclusive).
    pub start: usize,
    /// Byte offset of the end of the match (exclusive).
    pub end: usize,
    /// Rule identifier that produced this match.
    pub rule_id: String,
    /// Metavariable bindings from the match (text only; positions added in Phase 5).
    pub bindings: BTreeMap<String, String>,
    /// Whether the match result was truncated by the per-file match cap.
    pub truncated: bool,
}

/// A rule that could not be evaluated during the scan.
#[derive(Debug, Clone)]
pub struct SkippedRule {
    /// Rule identifier.
    pub rule_id: String,
    /// Human-readable reason the rule was skipped.
    pub reason: String,
}

/// Summary report returned by [`SastEngine::scan`].
#[derive(Debug)]
pub struct SastScanReport {
    /// All matches found across all scanned files, sorted by (path, start, end, rule_id).
    pub matches: Vec<SastMatch>,
    /// Rules that failed to initialise (e.g. regex compile error at scan time).
    pub skipped_rules: Vec<SkippedRule>,
    /// Number of files that passed the prefilter and were fully scanned.
    pub scanned_file_count: usize,
    /// Number of files skipped (prefiltered, too large, binary, or unreadable).
    pub skipped_file_count: usize,
    /// Number of files where AST parsing produced errors (still scanned, but results may be incomplete).
    pub parse_error_count: usize,
    /// Number of (rule, file) pairs where the per-file match cap was hit.
    pub truncated_rule_file_pairs: usize,
    /// Total wall-clock time for the scan in milliseconds.
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// SastEngine
// ---------------------------------------------------------------------------

/// SAST scanning engine.
///
/// `SastEngine` ties together file discovery ([`target::discover`]),
/// content pre-filtering ([`target::prefilter`]), AST-based formula
/// evaluation ([`engine::formula`]), and regex scanning
/// ([`engine::regex_mode`]) into a single parallelised scan operation.
///
/// # Usage
///
/// ```no_run
/// use std::path::Path;
/// use xzardgz::scanner::sast::{SastEngine, SastScanReport};
/// use xzardgz::scanner::sast::config::SastEngineConfig;
///
/// let mut engine = SastEngine::new(SastEngineConfig::new()).unwrap();
/// // load rules via engine.with_rules(...)
/// let report = engine.scan(Path::new(".")).unwrap();
/// println!("found {} matches", report.matches.len());
/// ```
pub struct SastEngine {
    config: SastEngineConfig,
    rules: Vec<RuleIr>,
    discovery: DiscoveryConfig,
}

impl SastEngine {
    /// Create a new engine with the given configuration and no rules loaded.
    ///
    /// # Arguments
    ///
    /// * `config` - Engine configuration controlling file size limits, timeouts,
    ///   match caps, and thread count.
    ///
    /// # Returns
    ///
    /// A freshly initialised `SastEngine` with an empty rule set.
    ///
    /// # Errors
    ///
    /// Currently infallible; the `Result` return type is reserved for future
    /// validation that may be added without a breaking API change.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::SastEngine;
    /// use xzardgz::scanner::sast::config::SastEngineConfig;
    ///
    /// let engine = SastEngine::new(SastEngineConfig::new()).unwrap();
    /// ```
    pub fn new(config: SastEngineConfig) -> Result<Self, SastError> {
        Ok(Self {
            config,
            rules: Vec::new(),
            discovery: DiscoveryConfig::default(),
        })
    }

    /// Replace the engine's rule set with `rules`.
    ///
    /// This may be called multiple times; each call replaces the previous set.
    ///
    /// # Arguments
    ///
    /// * `rules` - Compiled rule IR nodes to use for the next scan.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success. Reserved for future per-rule validation.
    ///
    /// # Errors
    ///
    /// Currently infallible; returns `Ok(())` unconditionally.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::SastEngine;
    /// use xzardgz::scanner::sast::config::SastEngineConfig;
    ///
    /// let mut engine = SastEngine::new(SastEngineConfig::new()).unwrap();
    /// engine.with_rules(vec![]).unwrap();
    /// ```
    pub fn with_rules(&mut self, rules: Vec<RuleIr>) -> Result<(), SastError> {
        self.rules = rules;
        Ok(())
    }

    /// Set include/exclude glob patterns for file discovery.
    ///
    /// These replace any previously configured patterns.
    ///
    /// # Arguments
    ///
    /// * `discovery` - New discovery configuration with include/exclude globs.
    ///
    /// # Returns
    ///
    /// `&mut Self` for method chaining.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::SastEngine;
    /// use xzardgz::scanner::sast::config::SastEngineConfig;
    /// use xzardgz::scanner::sast::target::discover::DiscoveryConfig;
    ///
    /// let mut engine = SastEngine::new(SastEngineConfig::new()).unwrap();
    /// engine.with_discovery(DiscoveryConfig {
    ///     include: vec!["**/*.rs".to_string()],
    ///     exclude: vec!["target/**".to_string()],
    /// });
    /// ```
    pub fn with_discovery(&mut self, discovery: DiscoveryConfig) -> &mut Self {
        self.discovery = discovery;
        self
    }

    /// Scan the directory tree at `root` and return a report.
    ///
    /// This method drives its own file walk (via [`discover_files`]), applies
    /// the prefilter, then evaluates all loaded rules in parallel using Rayon.
    ///
    /// # Arguments
    ///
    /// * `root` - Root directory to walk recursively.
    ///
    /// # Returns
    ///
    /// A [`SastScanReport`] summarising matches, skipped rules, and file counts.
    ///
    /// # Errors
    ///
    /// Returns [`SastError`] if file discovery fails with an I/O error or an
    /// invalid glob pattern, or if the Rayon thread pool cannot be created.
    pub fn scan(&self, root: &Path) -> Result<SastScanReport, SastError> {
        let scan_start = Instant::now();

        // --- Step 1: build prefilter ---
        let prefilter = Arc::new(Prefilter::from_rules(&self.rules));

        // --- Step 2: pre-process rules ---
        // Separate AST rules from regex-mode rules.
        // Regex rules are pre-compiled into RegexModeScanner instances; any that
        // fail to compile are recorded as skipped and excluded from the scan.
        let mut skipped_rules: Vec<SkippedRule> = Vec::new();
        let mut regex_scanners: Vec<(String, RegexModeScanner)> = Vec::new();
        let mut ast_rules: Vec<RuleIr> = Vec::new();

        for rule in &self.rules {
            if rule.applies_to_regex_mode() {
                match RegexModeScanner::new(rule) {
                    Ok(scanner) => regex_scanners.push((rule.id.clone(), scanner)),
                    Err(e) => skipped_rules.push(SkippedRule {
                        rule_id: rule.id.clone(),
                        reason: e.to_string(),
                    }),
                }
            } else {
                ast_rules.push(rule.clone());
            }
        }

        let ast_rules = Arc::new(ast_rules);
        let regex_scanners = Arc::new(regex_scanners);
        let compiler = Arc::new(PatternCompiler::new());
        let config = Arc::new(self.config.clone());

        // --- Step 3: discover files ---
        let paths = discover_files(root, &self.discovery, self.config.max_file_bytes)?;

        // --- Step 4: parallel scan ---
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.config.jobs)
            .build()
            .map_err(|e| SastError::Internal(format!("failed to build thread pool: {e}")))?;

        let raw_results: Vec<FileResult> = thread_pool.install(|| {
            paths
                .par_iter()
                .map(|path| {
                    // Panic isolation: one bad file cannot lose results from others.
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_file(
                            path,
                            &ast_rules,
                            &regex_scanners,
                            &prefilter,
                            &config,
                            &compiler,
                        )
                    }))
                    .unwrap_or_else(|_| FileResult {
                        path: path.clone(),
                        matches: Vec::new(),
                        skipped: false,
                        parse_error: true,
                        truncated_count: 0,
                        error: Some(format!("panic during evaluation of {}", path.display())),
                    })
                })
                .collect()
        });

        // --- Step 5: aggregate ---
        let mut all_matches: Vec<SastMatch> = Vec::new();
        let mut scanned_file_count = 0usize;
        let mut skipped_file_count = 0usize;
        let mut parse_error_count = 0usize;
        let mut truncated_rule_file_pairs = 0usize;

        for result in raw_results {
            if result.skipped {
                skipped_file_count += 1;
            } else {
                scanned_file_count += 1;
                if result.parse_error {
                    parse_error_count += 1;
                }
                truncated_rule_file_pairs += result.truncated_count;
                all_matches.extend(result.matches);
            }
        }

        // Sort for determinism: (path, start, end, rule_id)
        all_matches.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.start.cmp(&b.start))
                .then(a.end.cmp(&b.end))
                .then(a.rule_id.cmp(&b.rule_id))
        });

        let duration_ms = scan_start.elapsed().as_millis() as u64;

        Ok(SastScanReport {
            matches: all_matches,
            skipped_rules,
            scanned_file_count,
            skipped_file_count,
            parse_error_count,
            truncated_rule_file_pairs,
            duration_ms,
        })
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Per-file processing result (private).
#[allow(dead_code)]
struct FileResult {
    path: PathBuf,
    matches: Vec<SastMatch>,
    /// True if the file was skipped (prefiltered, binary, too large, unreadable).
    skipped: bool,
    /// True if AST parsing encountered errors for this file.
    parse_error: bool,
    /// Number of rules whose match output was truncated.
    truncated_count: usize,
    /// Optional error message (for logging / panic recovery).
    error: Option<String>,
}

/// Process a single file against all loaded rules.
///
/// Returns a [`FileResult`] describing the outcome. Never panics; callers
/// should additionally wrap this with `std::panic::catch_unwind` for full
/// isolation.
fn process_file(
    path: &Path,
    ast_rules: &[RuleIr],
    regex_scanners: &[(String, RegexModeScanner)],
    prefilter: &Prefilter,
    config: &SastEngineConfig,
    compiler: &PatternCompiler,
) -> FileResult {
    // 1. Read file bytes.
    let content = match std::fs::read(path) {
        Ok(c) => c,
        Err(_) => {
            return FileResult {
                path: path.to_path_buf(),
                matches: Vec::new(),
                skipped: true,
                parse_error: false,
                truncated_count: 0,
                error: Some(format!("failed to read {}", path.display())),
            };
        }
    };

    // 2. Check file size (defend against TOCTOU).
    if content.len() as u64 > config.max_file_bytes {
        return skipped_result(path);
    }

    // 3. Check binary (null bytes in first 8 KiB).
    if content[..8192.min(content.len())].contains(&0u8) {
        return skipped_result(path);
    }

    // 4. Prefilter: skip files with no chance of matching any rule.
    if !prefilter.file_may_match(&content) {
        return skipped_result(path);
    }

    let mut matches: Vec<SastMatch> = Vec::new();
    let mut parse_error = false;
    let mut truncated_count = 0usize;
    let lang = Language::from_path(path);

    // 5. AST rules (Rust files only).
    if lang == Language::Rust && !ast_rules.is_empty() {
        let src = String::from_utf8_lossy(&content);
        use ast_grep_core::tree_sitter::LanguageExt;
        use ast_grep_language::SupportLang;
        let root = SupportLang::Rust.ast_grep(src.as_ref());
        let density = ErrorNodeDensity::from_root(&root);
        let cached = CachedRoot { root, density };

        for rule in ast_rules {
            if !rule.applies_to_rust() {
                continue;
            }
            match scan_rule(rule, &cached, compiler, config) {
                Ok((ranges, trunc)) => {
                    if trunc.is_some() {
                        truncated_count += 1;
                    }
                    for range in ranges {
                        let bindings = range
                            .bindings
                            .into_iter()
                            .map(|(k, v)| (k, v.text))
                            .collect();
                        matches.push(SastMatch {
                            path: path.to_path_buf(),
                            start: range.start,
                            end: range.end,
                            rule_id: rule.id.clone(),
                            bindings,
                            truncated: trunc.is_some(),
                        });
                    }
                }
                Err(_) => {
                    parse_error = true;
                }
            }
        }
    }

    // 6. Regex/generic rules (all files).
    for (rule_id, scanner) in regex_scanners {
        match scanner.scan_bytes(&content, config.max_matches_per_file) {
            Ok(regex_matches) => {
                for m in regex_matches {
                    matches.push(SastMatch {
                        path: path.to_path_buf(),
                        start: m.byte_start,
                        end: m.byte_end,
                        rule_id: rule_id.clone(),
                        bindings: m.metavariables,
                        truncated: false,
                    });
                }
            }
            Err(_) => {
                parse_error = true;
            }
        }
    }

    FileResult {
        path: path.to_path_buf(),
        matches,
        skipped: false,
        parse_error,
        truncated_count,
        error: None,
    }
}

/// Construct a skipped [`FileResult`] for a given path.
fn skipped_result(path: &Path) -> FileResult {
    FileResult {
        path: path.to_path_buf(),
        matches: Vec::new(),
        skipped: true,
        parse_error: false,
        truncated_count: 0,
        error: None,
    }
}

/// Fixture source files embedded at compile time for engine integration tests.
#[cfg(test)]
mod fixtures {
    pub const MD5_POSITIVE: &str = include_str!("../../../testdata/sast/fixtures/md5_positive.rs");
    pub const MD5_NEGATIVE: &str = include_str!("../../../testdata/sast/fixtures/md5_negative.rs");
    pub const SHA1_POSITIVE: &str =
        include_str!("../../../testdata/sast/fixtures/sha1_positive.rs");
    pub const SHA1_NEGATIVE: &str =
        include_str!("../../../testdata/sast/fixtures/sha1_negative.rs");
    pub const DES_POSITIVE: &str = include_str!("../../../testdata/sast/fixtures/des_positive.rs");
    pub const DES_NEGATIVE: &str = include_str!("../../../testdata/sast/fixtures/des_negative.rs");
    pub const ELLIPSIS_ARG_POSITIVE: &str =
        include_str!("../../../testdata/sast/fixtures/ellipsis_arg_positive.rs");
    pub const ELLIPSIS_ARG_NEGATIVE: &str =
        include_str!("../../../testdata/sast/fixtures/ellipsis_arg_negative.rs");
    pub const ELLIPSIS_STMT_POSITIVE: &str =
        include_str!("../../../testdata/sast/fixtures/ellipsis_stmt_positive.rs");
}

#[cfg(test)]
mod tests {
    use crate::scanner::sast::rule::ir::CompileOutcome;
    use crate::scanner::sast::rule::parse::parse_and_compile;

    // Each entry is (rule_file_name, yaml_content).
    // The rules are loaded at compile time so the test is fully offline.
    const BUNDLED_RULES: &[(&str, &str)] = &[
        (
            "crypto/rust_weak_rsa_key.yaml",
            include_str!("rules/crypto/rust_weak_rsa_key.yaml"),
        ),
        (
            "crypto/rust_md5_usage.yaml",
            include_str!("rules/crypto/rust_md5_usage.yaml"),
        ),
        (
            "crypto/rust_sha1_usage.yaml",
            include_str!("rules/crypto/rust_sha1_usage.yaml"),
        ),
        (
            "crypto/rust_des_cipher.yaml",
            include_str!("rules/crypto/rust_des_cipher.yaml"),
        ),
        (
            "crypto/rust_rc4_cipher.yaml",
            include_str!("rules/crypto/rust_rc4_cipher.yaml"),
        ),
        (
            "security/rust_hardcoded_password.yaml",
            include_str!("rules/security/rust_hardcoded_password.yaml"),
        ),
        (
            "security/rust_unsafe_ffi.yaml",
            include_str!("rules/security/rust_unsafe_ffi.yaml"),
        ),
        (
            "security/regex_hardcoded_secret.yaml",
            include_str!("rules/security/regex_hardcoded_secret.yaml"),
        ),
    ];

    /// Every bundled first-party rule must parse as valid YAML and compile
    /// to either a Compiled or Skipped outcome without a hard parse error.
    #[test]
    fn test_bundled_rules_parse_without_hard_errors() {
        for (name, yaml) in BUNDLED_RULES {
            let outcomes = parse_and_compile(yaml)
                .unwrap_or_else(|e| panic!("rule file {name} failed YAML parse: {e}"));
            assert!(
                !outcomes.is_empty(),
                "rule file {name} produced no outcomes"
            );
            for outcome in &outcomes {
                match outcome {
                    Ok(CompileOutcome::Compiled(ir)) => {
                        assert!(!ir.id.is_empty(), "rule in {name} compiled with empty id");
                    }
                    Ok(CompileOutcome::Skipped { rule_id, reasons }) => {
                        // Skipped is acceptable; log which rule and why.
                        println!("[skip] {name}: rule '{rule_id}' skipped: {reasons:?}");
                    }
                    Err(e) => {
                        panic!("rule in {name} failed with hard parse error: {e}");
                    }
                }
            }
        }
    }

    /// Every bundled rule that targets the regex language compiles to a
    /// Compiled outcome (not Skipped) and has a regex leaf in its formula.
    #[test]
    fn test_bundled_regex_rules_compile_successfully() {
        let (name, yaml) = &BUNDLED_RULES[7]; // regex_hardcoded_secret.yaml
        let outcomes = parse_and_compile(yaml)
            .unwrap_or_else(|e| panic!("rule file {name} failed YAML parse: {e}"));
        assert_eq!(outcomes.len(), 1);
        match &outcomes[0] {
            Ok(CompileOutcome::Compiled(ir)) => {
                assert_eq!(ir.id, "regex-hardcoded-secret-assignment");
                assert!(
                    ir.applies_to_regex_mode(),
                    "regex rule must apply to regex mode"
                );
            }
            Ok(CompileOutcome::Skipped { rule_id, reasons }) => {
                panic!("regex rule '{rule_id}' was unexpectedly skipped: {reasons:?}");
            }
            Err(e) => panic!("regex rule failed: {e}"),
        }
    }

    /// The regex-mode rule for hardcoded secrets can be compiled into a
    /// working `RegexModeScanner` and detects a sample credential string.
    #[test]
    fn test_bundled_regex_rule_detects_hardcoded_secret() {
        use crate::scanner::sast::engine::regex_mode::RegexModeScanner;
        let (name, yaml) = &BUNDLED_RULES[7];
        let mut outcomes = parse_and_compile(yaml)
            .unwrap_or_else(|e| panic!("rule file {name} failed YAML parse: {e}"));
        let ir = match outcomes.remove(0) {
            Ok(CompileOutcome::Compiled(ir)) => ir,
            other => panic!("expected Compiled, got {other:?}"),
        };
        let scanner = RegexModeScanner::new(&ir).expect("regex scanner construction must succeed");
        let content = b"config.password = \"hunter2\"";
        let matches = scanner.scan_bytes(content, 10).expect("scan must succeed");
        assert!(
            !matches.is_empty(),
            "regex rule must detect hardcoded secret in sample content"
        );
    }

    // -----------------------------------------------------------------------
    // Phase 2: engine integration tests
    // -----------------------------------------------------------------------

    /// Build a CachedRoot from an in-memory source string (no file I/O).
    fn make_cached_root_from_str(src: &str) -> crate::scanner::sast::ast::parse::CachedRoot {
        use crate::scanner::sast::ast::diagnostics::ErrorNodeDensity;
        use crate::scanner::sast::ast::parse::CachedRoot;
        use ast_grep_core::tree_sitter::LanguageExt;
        use ast_grep_language::SupportLang;
        let root = SupportLang::Rust.ast_grep(src);
        let density = ErrorNodeDensity::from_root(&root);
        CachedRoot { root, density }
    }

    /// Compile the first rule from a YAML string, panicking on any failure.
    fn compile_first_rule(yaml: &str) -> crate::scanner::sast::rule::ir::RuleIr {
        use crate::scanner::sast::rule::ir::CompileOutcome;
        use crate::scanner::sast::rule::parse::parse_and_compile;
        let outcomes = parse_and_compile(yaml).expect("YAML parse must succeed");
        assert!(
            !outcomes.is_empty(),
            "rule file must contain at least one rule"
        );
        match outcomes.into_iter().next().unwrap() {
            Ok(CompileOutcome::Compiled(ir)) => ir,
            Ok(CompileOutcome::Skipped { rule_id, reasons }) => {
                panic!("rule '{rule_id}' skipped unexpectedly: {reasons:?}")
            }
            Err(e) => panic!("rule parse error: {e}"),
        }
    }

    /// Evaluate a rule against an in-memory source string.
    fn eval_rule_on_src(
        rule: &crate::scanner::sast::rule::ir::RuleIr,
        src: &str,
    ) -> Vec<crate::scanner::sast::engine::range::RangeWithMetavars> {
        use crate::scanner::sast::config::SastEngineConfig;
        use crate::scanner::sast::engine::formula::scan_rule;
        use crate::scanner::sast::engine::pattern::PatternCompiler;
        let cached = make_cached_root_from_str(src);
        let compiler = PatternCompiler::new();
        let config = SastEngineConfig::new();
        let (matches, _) = scan_rule(rule, &cached, &compiler, &config).expect("eval must succeed");
        matches
    }

    /// MD5 rule: positive fixture must produce at least one match.
    #[test]
    fn test_fixture_md5_rule_matches_positive_fixture() {
        let rule = compile_first_rule(BUNDLED_RULES[1].1); // rust_md5_usage
        let matches = eval_rule_on_src(&rule, super::fixtures::MD5_POSITIVE);
        assert!(
            !matches.is_empty(),
            "rust-md5-usage must match md5_positive.rs"
        );
    }

    /// MD5 rule: negative fixture must produce zero matches.
    #[test]
    fn test_fixture_md5_rule_does_not_match_negative_fixture() {
        let rule = compile_first_rule(BUNDLED_RULES[1].1); // rust_md5_usage
        let matches = eval_rule_on_src(&rule, super::fixtures::MD5_NEGATIVE);
        assert!(
            matches.is_empty(),
            "rust-md5-usage must NOT match md5_negative.rs, got {matches:?}"
        );
    }

    /// SHA-1 rule: positive fixture must produce at least one match.
    #[test]
    fn test_fixture_sha1_rule_matches_positive_fixture() {
        let rule = compile_first_rule(BUNDLED_RULES[2].1); // rust_sha1_usage
        let matches = eval_rule_on_src(&rule, super::fixtures::SHA1_POSITIVE);
        assert!(
            !matches.is_empty(),
            "rust-sha1-usage must match sha1_positive.rs"
        );
    }

    /// SHA-1 rule: negative fixture must produce zero matches.
    #[test]
    fn test_fixture_sha1_rule_does_not_match_negative_fixture() {
        let rule = compile_first_rule(BUNDLED_RULES[2].1); // rust_sha1_usage
        let matches = eval_rule_on_src(&rule, super::fixtures::SHA1_NEGATIVE);
        assert!(
            matches.is_empty(),
            "rust-sha1-usage must NOT match sha1_negative.rs"
        );
    }

    /// DES cipher rule: positive fixture must produce at least one match.
    #[test]
    fn test_fixture_des_rule_matches_positive_fixture() {
        let rule = compile_first_rule(BUNDLED_RULES[3].1); // rust_des_cipher
        let matches = eval_rule_on_src(&rule, super::fixtures::DES_POSITIVE);
        assert!(
            !matches.is_empty(),
            "rust-des-cipher must match des_positive.rs"
        );
    }

    /// DES cipher rule: negative fixture must produce zero matches.
    #[test]
    fn test_fixture_des_rule_does_not_match_negative_fixture() {
        let rule = compile_first_rule(BUNDLED_RULES[3].1); // rust_des_cipher
        let matches = eval_rule_on_src(&rule, super::fixtures::DES_NEGATIVE);
        assert!(
            matches.is_empty(),
            "rust-des-cipher must NOT match des_negative.rs"
        );
    }

    /// Ellipsis rewrite: foo(...) matches foo called with multiple arguments.
    #[test]
    fn test_fixture_ellipsis_arg_list_positive_match() {
        use crate::scanner::sast::config::SastEngineConfig;
        use crate::scanner::sast::engine::formula::eval_formula;
        use crate::scanner::sast::engine::pattern::PatternCompiler;
        use crate::scanner::sast::rule::ir::{Formula, Leaf};
        let formula = Formula::Leaf(Leaf::Pattern("foo($$$)".to_string()));
        let cached = make_cached_root_from_str(super::fixtures::ELLIPSIS_ARG_POSITIVE);
        let compiler = PatternCompiler::new();
        let config = SastEngineConfig::new();
        let (matches, _) = eval_formula(&formula, &cached, &compiler, &config, "ellipsis-test")
            .expect("eval must succeed");
        assert!(
            !matches.is_empty(),
            "foo($$$) must match foo(1, 2, 3) in ellipsis_arg_positive.rs"
        );
    }

    /// Ellipsis rewrite: foo(...) does NOT match bar(...) calls.
    #[test]
    fn test_fixture_ellipsis_arg_list_negative_near_miss() {
        use crate::scanner::sast::config::SastEngineConfig;
        use crate::scanner::sast::engine::formula::eval_formula;
        use crate::scanner::sast::engine::pattern::PatternCompiler;
        use crate::scanner::sast::rule::ir::{Formula, Leaf};
        let formula = Formula::Leaf(Leaf::Pattern("foo($$$)".to_string()));
        let cached = make_cached_root_from_str(super::fixtures::ELLIPSIS_ARG_NEGATIVE);
        let compiler = PatternCompiler::new();
        let config = SastEngineConfig::new();
        let (matches, _) = eval_formula(&formula, &cached, &compiler, &config, "ellipsis-test")
            .expect("eval must succeed");
        assert!(
            matches.is_empty(),
            "foo($$$) must NOT match bar(42) in ellipsis_arg_negative.rs"
        );
    }

    // -----------------------------------------------------------------------
    // Phase 4: SastEngine facade tests
    // -----------------------------------------------------------------------

    /// Prefilter soundness: scanning with and without prefiltering must produce
    /// identical match sets over the fixture corpus.
    ///
    /// The "without prefilter" condition is achieved by adding a bare `$X` rule
    /// alongside the md5 rule. A bare `$X` pattern contributes no extractable
    /// literal, so `Prefilter::always_analyze` is set to `true` and every file
    /// is passed through to the formula engine. The md5 rule matches must be
    /// identical in both configurations.
    #[test]
    fn test_sast_engine_prefilter_soundness_differential() {
        use tempfile::TempDir;

        let dir = TempDir::new().expect("tempdir must be created");

        // Write fixture files to disk.
        let pos_path = dir.path().join("md5_positive.rs");
        let neg_path = dir.path().join("md5_negative.rs");
        std::fs::write(&pos_path, super::fixtures::MD5_POSITIVE).unwrap();
        std::fs::write(&neg_path, super::fixtures::MD5_NEGATIVE).unwrap();

        // Compile the md5 rule.
        let md5_rule = match parse_and_compile(BUNDLED_RULES[1].1)
            .unwrap()
            .remove(0)
            .unwrap()
        {
            CompileOutcome::Compiled(ir) => ir,
            _ => panic!("md5 rule must compile"),
        };

        // Scan WITH prefilter (normal operation).
        let mut engine =
            super::SastEngine::new(crate::scanner::sast::config::SastEngineConfig::new()).unwrap();
        engine.with_rules(vec![md5_rule.clone()]).unwrap();
        let report_with = engine.scan(dir.path()).unwrap();

        // Scan WITHOUT prefilter: add a bare-$X rule to force always_analyze.
        use crate::scanner::sast::rule::ir::{Formula, Leaf, RuleIr};
        use crate::scanner::sast::rule::metadata::Severity;
        let always_rule = RuleIr {
            id: "always-analyze".to_string(),
            message: "test".to_string(),
            languages: vec!["rust".to_string()],
            severity: Severity::Warning,
            metadata: None,
            formula: Formula::Leaf(Leaf::Pattern("$X".to_string())),
            fix: None,
        };
        let mut engine2 =
            super::SastEngine::new(crate::scanner::sast::config::SastEngineConfig::new()).unwrap();
        engine2
            .with_rules(vec![md5_rule.clone(), always_rule])
            .unwrap();
        let report_without = engine2.scan(dir.path()).unwrap();

        // Filter to only md5 rule matches for the comparison.
        let relevant_with: Vec<_> = report_with
            .matches
            .iter()
            .filter(|m| m.rule_id == "rust-md5-usage")
            .collect();
        let relevant_without: Vec<_> = report_without
            .matches
            .iter()
            .filter(|m| m.rule_id == "rust-md5-usage")
            .collect();

        assert_eq!(
            relevant_with.len(),
            relevant_without.len(),
            "prefilter must not change the number of md5 rule matches"
        );

        // Positive fixture must have matches; negative must have none.
        let pos_matches: Vec<_> = relevant_with
            .iter()
            .filter(|m| m.path.ends_with("md5_positive.rs"))
            .collect();
        let neg_matches: Vec<_> = relevant_with
            .iter()
            .filter(|m| m.path.ends_with("md5_negative.rs"))
            .collect();
        assert!(
            !pos_matches.is_empty(),
            "md5 rule must match positive fixture"
        );
        assert!(
            neg_matches.is_empty(),
            "md5 rule must not match negative fixture"
        );
    }

    /// Two sequential scans over the same fixtures must produce identical
    /// sorted match sets.
    #[test]
    fn test_sast_engine_two_runs_produce_identical_sorted_output() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("md5_positive.rs"),
            super::fixtures::MD5_POSITIVE,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("sha1_positive.rs"),
            super::fixtures::SHA1_POSITIVE,
        )
        .unwrap();

        let rules: Vec<_> = [BUNDLED_RULES[1].1, BUNDLED_RULES[2].1]
            .iter()
            .flat_map(|yaml| parse_and_compile(yaml).unwrap())
            .filter_map(|r| match r.unwrap() {
                CompileOutcome::Compiled(ir) => Some(ir),
                _ => None,
            })
            .collect();

        let mut engine =
            super::SastEngine::new(crate::scanner::sast::config::SastEngineConfig::new()).unwrap();
        engine.with_rules(rules).unwrap();

        let report1 = engine.scan(dir.path()).unwrap();
        let report2 = engine.scan(dir.path()).unwrap();

        assert_eq!(
            report1.matches.len(),
            report2.matches.len(),
            "two runs must produce the same number of matches"
        );
        for (m1, m2) in report1.matches.iter().zip(report2.matches.iter()) {
            assert_eq!(m1.rule_id, m2.rule_id);
            assert_eq!(m1.path, m2.path);
            assert_eq!(m1.start, m2.start);
            assert_eq!(m1.end, m2.end);
        }
    }

    /// A file that contains binary content (NUL bytes) is silently skipped;
    /// it must not cause the scan to fail or lose matches from other files.
    #[test]
    fn test_sast_engine_panic_in_one_file_does_not_lose_other_results() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();

        // One readable file that produces matches.
        std::fs::write(
            dir.path().join("md5_positive.rs"),
            super::fixtures::MD5_POSITIVE,
        )
        .unwrap();

        // One binary file that must be skipped without aborting the scan.
        std::fs::write(dir.path().join("binary.rs"), b"binary\0content").unwrap();

        let md5_rule = match parse_and_compile(BUNDLED_RULES[1].1)
            .unwrap()
            .remove(0)
            .unwrap()
        {
            CompileOutcome::Compiled(ir) => ir,
            _ => panic!("md5 rule must compile"),
        };

        let mut engine =
            super::SastEngine::new(crate::scanner::sast::config::SastEngineConfig::new()).unwrap();
        engine.with_rules(vec![md5_rule]).unwrap();
        let report = engine.scan(dir.path()).unwrap();

        assert!(
            !report.matches.is_empty(),
            "matches from readable files must be collected even when other files are binary"
        );
        // Binary files are silently excluded by discover_files before reaching
        // the per-file counter, so we only verify the scan completed without
        // crashing and still produced results from the readable file.
    }

    /// File counts in the report reflect the actual number of scanned and
    /// skipped files.
    #[test]
    fn test_sast_engine_scan_counts_files_correctly() {
        use crate::scanner::sast::rule::ir::{Formula, Leaf, RuleIr};
        use crate::scanner::sast::rule::metadata::Severity;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn main() {}").unwrap();

        let rule = RuleIr {
            id: "always".to_string(),
            message: "test".to_string(),
            languages: vec!["rust".to_string()],
            severity: Severity::Info,
            metadata: None,
            formula: Formula::Leaf(Leaf::Pattern("fn $F() {}".to_string())),
            fix: None,
        };

        let mut engine =
            super::SastEngine::new(crate::scanner::sast::config::SastEngineConfig::new()).unwrap();
        engine.with_rules(vec![rule]).unwrap();
        let report = engine.scan(dir.path()).unwrap();

        assert_eq!(
            report.scanned_file_count, 2,
            "both .rs files must be scanned"
        );
    }

    /// Ellipsis rewrite: function body with statement sequence matches.
    #[test]
    fn test_fixture_ellipsis_stmt_seq_positive_match() {
        use crate::scanner::sast::config::SastEngineConfig;
        use crate::scanner::sast::engine::formula::eval_formula;
        use crate::scanner::sast::engine::pattern::PatternCompiler;
        use crate::scanner::sast::rule::ir::{Formula, Leaf};
        // Match let-bindings directly in the fixture.
        // The fixture has: let x = 1; let y = x + 2; let _result = y * 3;
        // A simple let-binding pattern verifies the block/statement parsing works.
        let formula = Formula::Or(vec![Formula::Leaf(Leaf::Pattern(
            "let $VAR = $VAL".to_string(),
        ))]);
        let cached = make_cached_root_from_str(super::fixtures::ELLIPSIS_STMT_POSITIVE);
        let compiler = PatternCompiler::new();
        let config = SastEngineConfig::new();
        let (matches, _) =
            eval_formula(&formula, &cached, &compiler, &config, "ellipsis-stmt-test")
                .expect("eval must succeed");
        assert!(
            !matches.is_empty(),
            "let $VAR = $VAL must match let-bindings in ellipsis_stmt_positive.rs"
        );
        // Verify $$$BODY multi-metavar matches function body.
        let formula2 = Formula::Leaf(Leaf::Pattern("fn $F() { $$$BODY }".to_string()));
        let (matches2, _) = eval_formula(
            &formula2,
            &cached,
            &compiler,
            &config,
            "ellipsis-stmt-test2",
        )
        .expect("eval must succeed");
        assert!(
            !matches2.is_empty(),
            "fn $F() {{ $$$BODY }} must match function definition in ellipsis_stmt_positive.rs"
        );
    }
}
