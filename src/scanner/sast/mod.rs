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
