//! Derived governance rules generated from language context.
//!
//! This module generates [`GovernanceRule`] values at runtime, synthesised
//! from the detected programming language of the repository being checked.
//! All rules produced here carry [`RuleSource::Derived`] as their source
//! and are only appended to the active rule set when no `AGENTS.md` file is
//! found in the repository root — ensuring that explicit repository-level
//! governance always takes precedence over language-level defaults.
//!
//! ## Supported languages
//!
//! | Language token  | Rules produced                                                               |
//! |-----------------|------------------------------------------------------------------------------|
//! | `rust`          | Rust-specific error-handling, code-quality, documentation, and testing rules |
//! | (anything else) | Empty — no derived rules                                                     |
//!
//! Language tokens are matched case-insensitively, so `"Rust"` and `"RUST"`
//! both resolve to the Rust rule set.
//!
//! ## Examples
//!
//! ```
//! use xzardgz::governance::enrichment::derive_from_context;
//! use xzardgz::governance::RuleSource;
//!
//! let rules = derive_from_context("rust", &[]);
//! assert!(!rules.is_empty());
//! assert!(rules.iter().all(|r| matches!(r.source, RuleSource::Derived)));
//! ```

use crate::governance::rules::{EnforcementLevel, GovernanceRule, RuleSource};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Derives governance rules from language context.
///
/// Inspects the provided `language` token and returns a set of
/// [`GovernanceRule`] values appropriate for that ecosystem.  All returned
/// rules carry `source: RuleSource::Derived` and are intended to be appended
/// to the active rule set only when no explicit repository governance file
/// (e.g. `AGENTS.md`) has been found.
///
/// Framework detection is reserved for future use; the `_frameworks` slice is
/// accepted for forward-compatibility but is not currently examined.
///
/// # Arguments
///
/// * `language`     - A string token identifying the primary language of the
///   repository, e.g. `"rust"`.  Matching is case-insensitive.
/// * `_frameworks`  - A slice of framework name tokens.  Currently unused;
///   passed for forward-compatibility.
///
/// # Returns
///
/// A [`Vec<GovernanceRule>`] of derived rules, or an empty vector if the
/// language is unrecognised or unsupported.
///
/// # Errors
///
/// This function is infallible and does not return a `Result`.
///
/// # Examples
///
/// ```
/// use xzardgz::governance::enrichment::derive_from_context;
/// use xzardgz::governance::RuleSource;
///
/// // Rust produces a non-empty rule set.
/// let rules = derive_from_context("rust", &[]);
/// assert!(!rules.is_empty());
/// assert!(rules.iter().all(|r| matches!(r.source, RuleSource::Derived)));
///
/// // Unknown languages produce no rules.
/// let empty = derive_from_context("cobol", &[]);
/// assert!(empty.is_empty());
/// ```
pub fn derive_from_context(language: &str, _frameworks: &[&str]) -> Vec<GovernanceRule> {
    match language.to_lowercase().as_str() {
        "rust" => rust_rules(),
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Returns the set of derived governance rules for Rust projects.
///
/// These rules encode the mandatory and recommended Rust coding standards
/// expected in this project.  All rules are tagged `RuleSource::Derived`.
fn rust_rules() -> Vec<GovernanceRule> {
    vec![
        GovernanceRule {
            id: "rust.error_handling.use_result".to_string(),
            description: concat!(
                "Use `Result<T, E>` for all recoverable errors; ",
                "never use `panic!` for recoverable error conditions."
            )
            .to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Derived,
        },
        GovernanceRule {
            id: "rust.error_handling.no_unwrap_without_justification".to_string(),
            description: concat!(
                "Never use `unwrap()` or `expect()` without a justification comment ",
                "explaining why the operation cannot fail."
            )
            .to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Derived,
        },
        GovernanceRule {
            id: "rust.error_handling.propagate_with_question_mark".to_string(),
            description: concat!(
                "Use `?` for error propagation instead of explicit `match` or `unwrap`; ",
                "never ignore errors with `let _ =`."
            )
            .to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Derived,
        },
        GovernanceRule {
            id: "rust.error_handling.use_thiserror".to_string(),
            description: concat!(
                "Use `thiserror` for custom error types to ensure consistent ",
                "`Display` and `Error` trait implementations."
            )
            .to_string(),
            enforcement: EnforcementLevel::Recommended,
            source: RuleSource::Derived,
        },
        GovernanceRule {
            id: "rust.code_quality.pass_clippy_clean".to_string(),
            description: concat!(
                "Code must pass `cargo clippy --all-targets --all-features -- -D warnings` ",
                "with no warnings."
            )
            .to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Derived,
        },
        GovernanceRule {
            id: "rust.code_quality.format_with_rustfmt".to_string(),
            description: concat!(
                "Code must be formatted with `cargo fmt --all`; ",
                "unformatted code must not be merged."
            )
            .to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Derived,
        },
        GovernanceRule {
            id: "rust.documentation.doc_comments_on_public_items".to_string(),
            description: concat!(
                "Every public function, struct, enum, and module must have `///` doc comments ",
                "with an `# Examples` section."
            )
            .to_string(),
            enforcement: EnforcementLevel::Required,
            source: RuleSource::Derived,
        },
        GovernanceRule {
            id: "rust.testing.test_public_functions".to_string(),
            description: concat!(
                "Write tests for all public functions covering success, failure, and edge cases; ",
                "target greater than 80 percent code coverage."
            )
            .to_string(),
            enforcement: EnforcementLevel::Recommended,
            source: RuleSource::Derived,
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_from_context_with_rust_returns_nonempty_vec() {
        let rules = derive_from_context("rust", &[]);
        assert!(!rules.is_empty());
    }

    #[test]
    fn test_derive_from_context_with_rust_all_sources_are_derived() {
        let rules = derive_from_context("rust", &[]);
        assert!(
            rules
                .iter()
                .all(|r| matches!(r.source, RuleSource::Derived)),
            "all Rust rules must carry RuleSource::Derived"
        );
    }

    #[test]
    fn test_derive_from_context_with_rust_contains_required_error_handling_rules() {
        let rules = derive_from_context("rust", &[]);
        let expected_ids = [
            "rust.error_handling.use_result",
            "rust.error_handling.no_unwrap_without_justification",
            "rust.error_handling.propagate_with_question_mark",
        ];
        for id in &expected_ids {
            let rule = rules.iter().find(|r| r.id == *id);
            assert!(rule.is_some(), "missing required error-handling rule: {id}");
            assert_eq!(
                rule.unwrap().enforcement, // SAFETY: asserted is_some above
                EnforcementLevel::Required,
                "rule {id} must be Required"
            );
        }
    }

    #[test]
    fn test_derive_from_context_with_rust_contains_recommended_rules() {
        let rules = derive_from_context("rust", &[]);
        let recommended_ids = [
            "rust.error_handling.use_thiserror",
            "rust.testing.test_public_functions",
        ];
        for id in &recommended_ids {
            let rule = rules.iter().find(|r| r.id == *id);
            assert!(rule.is_some(), "missing recommended rule: {id}");
            assert_eq!(
                rule.unwrap().enforcement, // SAFETY: asserted is_some above
                EnforcementLevel::Recommended,
                "rule {id} must be Recommended"
            );
        }
    }

    #[test]
    fn test_derive_from_context_with_unknown_language_returns_empty() {
        let rules = derive_from_context("cobol", &[]);
        assert!(rules.is_empty());
    }

    #[test]
    fn test_derive_from_context_with_empty_language_returns_empty() {
        let rules = derive_from_context("", &[]);
        assert!(rules.is_empty());
    }

    #[test]
    fn test_derive_from_context_language_matching_is_case_insensitive() {
        let lower = derive_from_context("rust", &[]);
        let upper = derive_from_context("RUST", &[]);
        let mixed = derive_from_context("Rust", &[]);
        assert!(!upper.is_empty(), "RUST should return rules");
        assert!(!mixed.is_empty(), "Rust should return rules");
        assert_eq!(
            upper.len(),
            lower.len(),
            "RUST and rust must return the same number of rules"
        );
        assert_eq!(
            mixed.len(),
            lower.len(),
            "Rust and rust must return the same number of rules"
        );
    }

    #[test]
    fn test_derive_from_context_frameworks_ignored_for_rust() {
        let with_frameworks = derive_from_context("rust", &["tokio", "actix-web", "serde"]);
        let without_frameworks = derive_from_context("rust", &[]);
        assert!(
            !with_frameworks.is_empty(),
            "frameworks slice must not suppress Rust rules"
        );
        assert_eq!(
            with_frameworks.len(),
            without_frameworks.len(),
            "frameworks slice must not change the number of Rust rules"
        );
    }

    #[test]
    fn test_derive_from_context_rust_has_expected_rule_count() {
        let rules = derive_from_context("rust", &[]);
        assert_eq!(rules.len(), 8, "expected exactly 8 Rust derived rules");
    }

    #[test]
    fn test_derive_from_context_rust_ids_are_unique() {
        let rules = derive_from_context("rust", &[]);
        let original_len = rules.len();
        let mut ids: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), original_len, "rule IDs must be unique");
    }
}
