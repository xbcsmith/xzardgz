//! Cross-cutting scan hooks applied to every file during a repository scan.
//!
//! The [`CrossCutHook`] trait defines a uniform interface: given a file path
//! and its UTF-8 content, a hook returns zero or more [`ScanFinding`] values.
//! All built-in hooks perform case-sensitive substring matching and record the
//! first line on which each pattern appears.
//!
//! Built-in implementations:
//! - [`SecretsHook`] — credential and secret leakage patterns
//! - [`UnsafeRustHook`] — `unsafe` usage in `.rs` files
//! - [`CommandExecutionHook`] — OS command execution patterns
//!
//! Use [`default_hooks`] to obtain a ready-to-use collection of all three.

use crate::scanner::findings::{FindingSeverity, ScanFinding};
use std::path::Path;

// ---------------------------------------------------------------------------
// CrossCutHook trait
// ---------------------------------------------------------------------------

/// A reusable scan hook that inspects file content and produces findings.
///
/// Hooks are applied to every non-binary file during the scan. They produce
/// zero or more [`ScanFinding`] values based on the file path and content.
/// All implementors must be [`Send`] + [`Sync`] for concurrent scanning.
pub trait CrossCutHook: Send + Sync {
    /// Returns the unique name of this hook.
    ///
    /// Names are used for logging, reporting, and deduplication.
    fn name(&self) -> &str;

    /// Inspects `content` at `path` and returns any findings.
    ///
    /// # Arguments
    ///
    /// * `path` - Absolute (or workspace-relative) path of the file being scanned.
    /// * `content` - UTF-8 text content of the file.
    ///
    /// # Returns
    ///
    /// A [`Vec<ScanFinding>`] containing one entry per matched pattern.
    /// Returns an empty `Vec` when no patterns are found.
    fn scan(&self, path: &Path, content: &str) -> Vec<ScanFinding>;
}

// ---------------------------------------------------------------------------
// SecretsHook
// ---------------------------------------------------------------------------

/// Detects common credential and secret leakage patterns in file content.
///
/// Scans every file regardless of extension.  For each of the built-in
/// credential patterns that appears anywhere in the file, one [`ScanFinding`]
/// of severity [`FindingSeverity::High`] is emitted with the first matching
/// line recorded.
pub struct SecretsHook;

/// Patterns that indicate a credential or secret may be present.
const SECRETS_PATTERNS: &[&str] = &[
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

impl CrossCutHook for SecretsHook {
    fn name(&self) -> &str {
        "secrets"
    }

    fn scan(&self, path: &Path, content: &str) -> Vec<ScanFinding> {
        let file = path.to_string_lossy().to_string();
        let mut findings = Vec::new();

        for pattern in SECRETS_PATTERNS {
            if let Some((idx, _)) = content
                .lines()
                .enumerate()
                .find(|(_, line)| line.contains(*pattern))
            {
                findings.push(ScanFinding {
                    kind: "secrets".to_string(),
                    file: file.clone(),
                    line: Some((idx + 1) as u32),
                    evidence: (*pattern).to_string(),
                    severity: FindingSeverity::High,
                });
            }
        }

        findings
    }
}

// ---------------------------------------------------------------------------
// UnsafeRustHook
// ---------------------------------------------------------------------------

/// Detects `unsafe` blocks, functions, impls, and traits in Rust source files.
///
/// Only files whose path ends with `.rs` are scanned; all other files are
/// skipped.  Each matched pattern produces one [`ScanFinding`] of severity
/// [`FindingSeverity::Medium`] with the first matching line recorded.
pub struct UnsafeRustHook;

/// Patterns that indicate unsafe Rust usage.
const UNSAFE_RUST_PATTERNS: &[&str] = &["unsafe {", "unsafe fn", "unsafe impl", "unsafe trait"];

impl CrossCutHook for UnsafeRustHook {
    fn name(&self) -> &str {
        "unsafe_rust"
    }

    fn scan(&self, path: &Path, content: &str) -> Vec<ScanFinding> {
        // Only analyse Rust source files.
        let is_rust = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext == "rs")
            .unwrap_or(false);

        if !is_rust {
            return Vec::new();
        }

        let file = path.to_string_lossy().to_string();
        let mut findings = Vec::new();

        for pattern in UNSAFE_RUST_PATTERNS {
            if let Some((idx, _)) = content
                .lines()
                .enumerate()
                .find(|(_, line)| line.contains(*pattern))
            {
                findings.push(ScanFinding {
                    kind: "unsafe_rust".to_string(),
                    file: file.clone(),
                    line: Some((idx + 1) as u32),
                    evidence: (*pattern).to_string(),
                    severity: FindingSeverity::Medium,
                });
            }
        }

        findings
    }
}

// ---------------------------------------------------------------------------
// CommandExecutionHook
// ---------------------------------------------------------------------------

/// Detects OS command execution patterns that may indicate injection risk.
///
/// Scans every file regardless of extension.  Each matched pattern produces
/// one [`ScanFinding`] of severity [`FindingSeverity::Medium`] with the first
/// matching line recorded.
pub struct CommandExecutionHook;

/// Patterns that indicate OS command execution.
const COMMAND_EXECUTION_PATTERNS: &[&str] = &[
    "std::process::Command",
    "subprocess.",
    "os.system(",
    "shell_exec(",
    "popen(",
    "Process::new(",
];

impl CrossCutHook for CommandExecutionHook {
    fn name(&self) -> &str {
        "command_execution"
    }

    fn scan(&self, path: &Path, content: &str) -> Vec<ScanFinding> {
        let file = path.to_string_lossy().to_string();
        let mut findings = Vec::new();

        for pattern in COMMAND_EXECUTION_PATTERNS {
            if let Some((idx, _)) = content
                .lines()
                .enumerate()
                .find(|(_, line)| line.contains(*pattern))
            {
                findings.push(ScanFinding {
                    kind: "command_execution".to_string(),
                    file: file.clone(),
                    line: Some((idx + 1) as u32),
                    evidence: (*pattern).to_string(),
                    severity: FindingSeverity::Medium,
                });
            }
        }

        findings
    }
}

// ---------------------------------------------------------------------------
// default_hooks
// ---------------------------------------------------------------------------

/// Returns the default set of cross-cutting hooks.
///
/// Includes [`SecretsHook`], [`UnsafeRustHook`], and [`CommandExecutionHook`]
/// in that order.  The returned `Vec` owns boxed trait objects so the hooks
/// can be stored and called uniformly through the [`CrossCutHook`] interface.
///
/// # Returns
///
/// A [`Vec<Box<dyn CrossCutHook>>`] containing the three built-in hooks.
pub fn default_hooks() -> Vec<Box<dyn CrossCutHook>> {
    vec![
        Box::new(SecretsHook),
        Box::new(UnsafeRustHook),
        Box::new(CommandExecutionHook),
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // SecretsHook
    // -----------------------------------------------------------------------

    #[test]
    fn test_secrets_hook_detects_api_key_pattern() {
        let hook = SecretsHook;
        let path = Path::new("config.yaml");
        let content = "api_key=supersecret123";
        let findings = hook.scan(path, content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "secrets");
        assert_eq!(findings[0].evidence, "api_key=");
        assert_eq!(findings[0].line, Some(1));
        assert_eq!(findings[0].severity, FindingSeverity::High);
    }

    #[test]
    fn test_secrets_hook_detects_begin_private_key() {
        let hook = SecretsHook;
        let path = Path::new("keys.pem");
        let content = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAK...";
        let findings = hook.scan(path, content);
        assert!(
            findings
                .iter()
                .any(|f| f.evidence == "-----BEGIN RSA PRIVATE KEY"),
            "expected RSA private key finding"
        );
        assert!(findings.iter().all(|f| f.severity == FindingSeverity::High));
    }

    #[test]
    fn test_secrets_hook_returns_empty_for_clean_content() {
        let hook = SecretsHook;
        let path = Path::new("README.md");
        let content = "This file contains no credentials at all.";
        let findings = hook.scan(path, content);
        assert!(findings.is_empty());
    }

    // -----------------------------------------------------------------------
    // UnsafeRustHook
    // -----------------------------------------------------------------------

    #[test]
    fn test_unsafe_rust_hook_detects_unsafe_block_in_rs_file() {
        let hook = UnsafeRustHook;
        let path = Path::new("src/lib.rs");
        let content = "fn example() {\n    unsafe { *ptr = 1; }\n}";
        let findings = hook.scan(path, content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "unsafe_rust");
        assert_eq!(findings[0].evidence, "unsafe {");
        assert_eq!(findings[0].line, Some(2));
        assert_eq!(findings[0].severity, FindingSeverity::Medium);
    }

    #[test]
    fn test_unsafe_rust_hook_skips_non_rs_files() {
        let hook = UnsafeRustHook;
        let path = Path::new("notes.txt");
        let content = "unsafe { some text }";
        let findings = hook.scan(path, content);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_unsafe_rust_hook_detects_unsafe_fn() {
        let hook = UnsafeRustHook;
        let path = Path::new("ffi.rs");
        let content = "pub unsafe fn raw_write(ptr: *mut u8, val: u8) {}";
        let findings = hook.scan(path, content);
        assert!(
            findings.iter().any(|f| f.evidence == "unsafe fn"),
            "expected unsafe fn finding"
        );
    }

    // -----------------------------------------------------------------------
    // CommandExecutionHook
    // -----------------------------------------------------------------------

    #[test]
    fn test_command_execution_hook_detects_process_command() {
        let hook = CommandExecutionHook;
        let path = Path::new("runner.rs");
        let content = "let output = std::process::Command::new(\"ls\").output();";
        let findings = hook.scan(path, content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "command_execution");
        assert_eq!(findings[0].evidence, "std::process::Command");
        assert_eq!(findings[0].line, Some(1));
        assert_eq!(findings[0].severity, FindingSeverity::Medium);
    }

    #[test]
    fn test_command_execution_hook_detects_os_system() {
        let hook = CommandExecutionHook;
        let path = Path::new("script.py");
        let content = "import os\nos.system(\"rm -rf /tmp/cache\")";
        let findings = hook.scan(path, content);
        assert!(
            findings.iter().any(|f| f.evidence == "os.system("),
            "expected os.system finding"
        );
        assert_eq!(
            findings
                .iter()
                .find(|f| f.evidence == "os.system(")
                .unwrap()
                .line,
            Some(2)
        );
    }

    #[test]
    fn test_command_execution_hook_returns_empty_for_clean_content() {
        let hook = CommandExecutionHook;
        let path = Path::new("math.rs");
        let content = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let findings = hook.scan(path, content);
        assert!(findings.is_empty());
    }

    // -----------------------------------------------------------------------
    // default_hooks
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_hooks_returns_three_hooks() {
        let hooks = default_hooks();
        assert_eq!(hooks.len(), 3);
        let names: Vec<&str> = hooks.iter().map(|h| h.name()).collect();
        assert!(names.contains(&"secrets"));
        assert!(names.contains(&"unsafe_rust"));
        assert!(names.contains(&"command_execution"));
    }
}
