//! Compile-time embedded SAST rules.
//!
//! All first-party rules are embedded into the binary at compile time via
//! `include_str!`. They are therefore available offline with no filesystem
//! dependency and form the final fallback tier of the four-level ruleset
//! search path defined in [`crate::scanner::sast::rule::resolve`].
//!
//! ## Ruleset identifiers
//!
//! Each map key follows the form `"<ruleset_id>/<filename>.yaml"` where
//! `ruleset_id` is one of `"builtin:crypto"` or `"builtin:security"`. The
//! resolver uses the key prefix to namespace every compiled rule id:
//! e.g. `"builtin:crypto::rust-weak-rsa-key"`.
//!
//! ## Adding a new builtin rule
//!
//! 1. Add the YAML rule file under `src/scanner/sast/rules/<ruleset>/`.
//! 2. Add a corresponding `include_str!` entry in [`embedded_rules`].
//! 3. Add positive and negative fixtures under `testdata/sast/builtin/`.
//! 4. Run `cargo test --all-features` to verify the rule parses cleanly.

use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns every embedded first-party SAST rule as a map of path key to YAML
/// content.
///
/// Keys have the form `"<ruleset_id>/<filename>.yaml"` and are sorted
/// lexicographically by the [`BTreeMap`] contract.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::embedded::embedded_rules;
///
/// let rules = embedded_rules();
/// assert!(
///     rules.len() >= 25,
///     "expected at least 25 builtin rules, got {}",
///     rules.len()
/// );
/// assert!(rules.contains_key("builtin:crypto/rust_weak_rsa_key.yaml"));
/// assert!(rules.contains_key("builtin:security/rust_hardcoded_password.yaml"));
/// ```
#[must_use]
pub fn embedded_rules() -> BTreeMap<&'static str, &'static str> {
    let mut map: BTreeMap<&'static str, &'static str> = BTreeMap::new();

    // -----------------------------------------------------------------------
    // builtin:crypto
    // -----------------------------------------------------------------------

    map.insert(
        "builtin:crypto/rust_weak_rsa_key.yaml",
        include_str!("rules/crypto/rust_weak_rsa_key.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_md5_usage.yaml",
        include_str!("rules/crypto/rust_md5_usage.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_sha1_usage.yaml",
        include_str!("rules/crypto/rust_sha1_usage.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_des_cipher.yaml",
        include_str!("rules/crypto/rust_des_cipher.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_rc4_cipher.yaml",
        include_str!("rules/crypto/rust_rc4_cipher.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_triple_des_cipher.yaml",
        include_str!("rules/crypto/rust_triple_des_cipher.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_blowfish_cipher.yaml",
        include_str!("rules/crypto/rust_blowfish_cipher.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_weak_prng.yaml",
        include_str!("rules/crypto/rust_weak_prng.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_hardcoded_iv.yaml",
        include_str!("rules/crypto/rust_hardcoded_iv.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_hardcoded_salt.yaml",
        include_str!("rules/crypto/rust_hardcoded_salt.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_aes_ecb_mode.yaml",
        include_str!("rules/crypto/rust_aes_ecb_mode.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_pkcs1v15_encrypt.yaml",
        include_str!("rules/crypto/rust_pkcs1v15_encrypt.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_pbkdf2_low_rounds.yaml",
        include_str!("rules/crypto/rust_pbkdf2_low_rounds.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_sha1_hmac.yaml",
        include_str!("rules/crypto/rust_sha1_hmac.yaml"),
    );
    map.insert(
        "builtin:crypto/rust_crc32_for_security.yaml",
        include_str!("rules/crypto/rust_crc32_for_security.yaml"),
    );

    // -----------------------------------------------------------------------
    // builtin:security
    // -----------------------------------------------------------------------

    map.insert(
        "builtin:security/rust_hardcoded_password.yaml",
        include_str!("rules/security/rust_hardcoded_password.yaml"),
    );
    map.insert(
        "builtin:security/rust_unsafe_ffi.yaml",
        include_str!("rules/security/rust_unsafe_ffi.yaml"),
    );
    map.insert(
        "builtin:security/regex_hardcoded_secret.yaml",
        include_str!("rules/security/regex_hardcoded_secret.yaml"),
    );
    map.insert(
        "builtin:security/rust_reqwest_invalid_certs.yaml",
        include_str!("rules/security/rust_reqwest_invalid_certs.yaml"),
    );
    map.insert(
        "builtin:security/rust_reqwest_invalid_hostnames.yaml",
        include_str!("rules/security/rust_reqwest_invalid_hostnames.yaml"),
    );
    map.insert(
        "builtin:security/regex_hardcoded_db_url.yaml",
        include_str!("rules/security/regex_hardcoded_db_url.yaml"),
    );
    map.insert(
        "builtin:security/regex_hardcoded_const_secret.yaml",
        include_str!("rules/security/regex_hardcoded_const_secret.yaml"),
    );
    map.insert(
        "builtin:security/rust_unsafe_transmute.yaml",
        include_str!("rules/security/rust_unsafe_transmute.yaml"),
    );
    map.insert(
        "builtin:security/rust_shell_command.yaml",
        include_str!("rules/security/rust_shell_command.yaml"),
    );
    map.insert(
        "builtin:security/rust_openssl_no_verify.yaml",
        include_str!("rules/security/rust_openssl_no_verify.yaml"),
    );

    map
}

/// Returns the ruleset identifier prefix for a given embedded rule key.
///
/// For example, `"builtin:crypto/rust_md5_usage.yaml"` returns
/// `"builtin:crypto"`.
///
/// Returns `None` when the key does not contain a `/` separator.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::embedded::ruleset_id_from_key;
///
/// assert_eq!(
///     ruleset_id_from_key("builtin:crypto/rust_md5_usage.yaml"),
///     Some("builtin:crypto")
/// );
/// assert_eq!(ruleset_id_from_key("no-separator"), None);
/// ```
#[must_use]
pub fn ruleset_id_from_key(key: &str) -> Option<&str> {
    key.split_once('/').map(|(prefix, _)| prefix)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::sast::rule::ir::CompileOutcome;
    use crate::scanner::sast::rule::parse::parse_and_compile;

    #[test]
    fn test_embedded_rules_contains_at_least_25_entries() {
        let rules = embedded_rules();
        assert!(
            rules.len() >= 25,
            "expected at least 25 embedded rules, got {}",
            rules.len()
        );
    }

    #[test]
    fn test_embedded_rules_contains_expected_crypto_keys() {
        let rules = embedded_rules();
        let required = [
            "builtin:crypto/rust_weak_rsa_key.yaml",
            "builtin:crypto/rust_md5_usage.yaml",
            "builtin:crypto/rust_sha1_usage.yaml",
            "builtin:crypto/rust_des_cipher.yaml",
            "builtin:crypto/rust_rc4_cipher.yaml",
            "builtin:crypto/rust_triple_des_cipher.yaml",
            "builtin:crypto/rust_blowfish_cipher.yaml",
            "builtin:crypto/rust_weak_prng.yaml",
            "builtin:crypto/rust_hardcoded_iv.yaml",
            "builtin:crypto/rust_hardcoded_salt.yaml",
            "builtin:crypto/rust_aes_ecb_mode.yaml",
            "builtin:crypto/rust_pkcs1v15_encrypt.yaml",
            "builtin:crypto/rust_pbkdf2_low_rounds.yaml",
            "builtin:crypto/rust_sha1_hmac.yaml",
            "builtin:crypto/rust_crc32_for_security.yaml",
        ];
        for key in &required {
            assert!(rules.contains_key(key), "missing embedded key: {key}");
        }
    }

    #[test]
    fn test_embedded_rules_contains_expected_security_keys() {
        let rules = embedded_rules();
        let required = [
            "builtin:security/rust_hardcoded_password.yaml",
            "builtin:security/rust_unsafe_ffi.yaml",
            "builtin:security/regex_hardcoded_secret.yaml",
            "builtin:security/rust_reqwest_invalid_certs.yaml",
            "builtin:security/rust_reqwest_invalid_hostnames.yaml",
            "builtin:security/regex_hardcoded_db_url.yaml",
            "builtin:security/regex_hardcoded_const_secret.yaml",
            "builtin:security/rust_unsafe_transmute.yaml",
            "builtin:security/rust_shell_command.yaml",
            "builtin:security/rust_openssl_no_verify.yaml",
        ];
        for key in &required {
            assert!(rules.contains_key(key), "missing embedded key: {key}");
        }
    }

    #[test]
    fn test_all_embedded_rules_parse_without_hard_errors() {
        let rules = embedded_rules();
        for (key, yaml) in &rules {
            let outcomes = parse_and_compile(yaml)
                .unwrap_or_else(|e| panic!("embedded rule {key} failed YAML parse: {e}"));
            assert!(
                !outcomes.is_empty(),
                "embedded rule {key} produced no outcomes"
            );
            for outcome in &outcomes {
                match outcome {
                    Ok(CompileOutcome::Compiled(ir)) => {
                        assert!(!ir.id.is_empty(), "rule in {key} compiled with empty id");
                    }
                    Ok(CompileOutcome::Skipped { rule_id, reasons }) => {
                        println!("[skip] {key}: rule '{rule_id}' skipped: {reasons:?}");
                    }
                    Err(e) => {
                        panic!("embedded rule {key} failed with parse error: {e}");
                    }
                }
            }
        }
    }

    #[test]
    fn test_ruleset_id_from_key_extracts_correct_prefix() {
        assert_eq!(
            ruleset_id_from_key("builtin:crypto/rust_md5_usage.yaml"),
            Some("builtin:crypto")
        );
        assert_eq!(
            ruleset_id_from_key("builtin:security/rust_unsafe_ffi.yaml"),
            Some("builtin:security")
        );
    }

    #[test]
    fn test_ruleset_id_from_key_returns_none_for_key_without_separator() {
        assert_eq!(ruleset_id_from_key("no-separator"), None);
    }

    #[test]
    fn test_embedded_crypto_ruleset_has_at_least_15_entries() {
        let count = embedded_rules()
            .keys()
            .filter(|k| k.starts_with("builtin:crypto/"))
            .count();
        assert!(
            count >= 15,
            "expected at least 15 crypto rules, got {count}"
        );
    }

    #[test]
    fn test_embedded_security_ruleset_has_at_least_10_entries() {
        let count = embedded_rules()
            .keys()
            .filter(|k| k.starts_with("builtin:security/"))
            .count();
        assert!(
            count >= 10,
            "expected at least 10 security rules, got {count}"
        );
    }
}
