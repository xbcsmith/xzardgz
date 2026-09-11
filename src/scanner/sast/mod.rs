//! Static Application Security Testing (SAST) pipeline.
//!
//! This module provides a multi-language SAST engine for xzardgz.
//! Rules are expressed in a Semgrep-compatible YAML format and evaluated
//! against source files using a formula-based matching engine.
//!
//! # Module layout
//!
//! | Submodule | Purpose                                                       |
//! |-----------|---------------------------------------------------------------|
//! | [`engine`] | Formula evaluation and scanning engines                      |
//! | [`error`]  | [`SastError`] and supporting error types                     |
//! | [`rule`]   | Rule model: IR, metadata, and schema types                   |
//!
//! [`SastError`]: error::SastError

pub mod ast;
pub mod config;
pub mod engine;
pub mod error;
pub mod rule;

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
}
