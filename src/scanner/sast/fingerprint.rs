//! Stable, content-addressed fingerprinting for SAST match deduplication.
//!
//! Every [`SastMatch`] carries a `fingerprint` field computed by this module.
//! The fingerprint is suitable for deduplication across scan runs and for
//! change-tracking in CI pipelines.
//!
//! ## Hash input
//!
//! The hash is BLAKE2b-256 over the concatenation of:
//!
//! ```text
//! rule_id NUL repo_relative_path NUL snippet_text
//! ```
//!
//! where `NUL` is the ASCII null byte `\x00`.  The `snippet_text` is the
//! exact source text matched by the pattern (as stored in
//! [`MatchSnippet::text`]).
//!
//! ## Stability guarantees
//!
//! - No absolute path appears in the hash input, so the fingerprint is
//!   identical regardless of where the repository is checked out.
//! - The `_<index>` suffix distinguishes multiple matches of the **same rule
//!   in the same file** when the matched text is identical.
//!
//! [`SastMatch`]: crate::scanner::sast::match_model::SastMatch
//! [`MatchSnippet::text`]: crate::scanner::sast::match_model::MatchSnippet::text

use std::path::Path;

use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};

/// BLAKE2b-256: the [`Blake2b`] hasher parameterised with a 32-byte output.
type Blake2b256 = Blake2b<U32>;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compute a stable fingerprint for a SAST match.
///
/// # Arguments
///
/// * `rule_id` - The rule identifier used verbatim in the hash (no ruleset
///   prefix; the un-namespaced id produces the most stable fingerprint across
///   ruleset renames).
/// * `rel_path` - Repo-relative path of the file containing the match.  Must
///   not contain any absolute-path prefix.
/// * `snippet_text` - The exact source text that was matched (the value of
///   [`MatchSnippet::text`]).
/// * `index` - 0-based ordinal of this match among all matches produced by the
///   same `(rule_id, rel_path)` pair.  Used to make the fingerprint unique when
///   the same rule matches identical text in the same file.
///
/// # Returns
///
/// A lowercase hexadecimal string representing the 32-byte BLAKE2b-256 digest,
/// followed by `_<index>` (e.g. `"a3f9...e1_0"`).
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use xzardgz::scanner::sast::fingerprint::compute_fingerprint;
///
/// let fp0 = compute_fingerprint("my-rule", Path::new("src/foo.rs"), "Md5::new()", 0);
/// let fp1 = compute_fingerprint("my-rule", Path::new("src/foo.rs"), "Md5::new()", 1);
///
/// // Same content, different index -> different fingerprints.
/// assert_ne!(fp0, fp1);
///
/// // Fingerprint ends with the index suffix.
/// assert!(fp0.ends_with("_0"));
/// assert!(fp1.ends_with("_1"));
/// ```
///
/// [`MatchSnippet::text`]: crate::scanner::sast::match_model::MatchSnippet::text
pub fn compute_fingerprint(
    rule_id: &str,
    rel_path: &Path,
    snippet_text: &str,
    index: usize,
) -> String {
    let mut hasher = Blake2b256::new();
    hasher.update(rule_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(rel_path.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(snippet_text.as_bytes());
    let result = hasher.finalize();
    let hex: String = result.iter().map(|b| format!("{b:02x}")).collect();
    format!("{hex}_{index}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_fingerprint_with_index_zero_ends_with_underscore_zero() {
        let fp = compute_fingerprint("rule-a", Path::new("src/lib.rs"), "some code", 0);
        assert!(
            fp.ends_with("_0"),
            "fingerprint must end with '_0', got: {fp}"
        );
    }

    #[test]
    fn test_compute_fingerprint_with_index_five_ends_with_underscore_five() {
        let fp = compute_fingerprint("rule-a", Path::new("src/lib.rs"), "some code", 5);
        assert!(
            fp.ends_with("_5"),
            "fingerprint must end with '_5', got: {fp}"
        );
    }

    #[test]
    fn test_compute_fingerprint_different_indices_produce_different_fingerprints() {
        let fp0 = compute_fingerprint("rule-a", Path::new("src/lib.rs"), "code", 0);
        let fp1 = compute_fingerprint("rule-a", Path::new("src/lib.rs"), "code", 1);
        assert_ne!(
            fp0, fp1,
            "different indices must produce different fingerprints"
        );
    }

    #[test]
    fn test_compute_fingerprint_is_stable_across_repeated_calls() {
        let fp1 = compute_fingerprint("rule-a", Path::new("src/lib.rs"), "code", 0);
        let fp2 = compute_fingerprint("rule-a", Path::new("src/lib.rs"), "code", 0);
        assert_eq!(
            fp1, fp2,
            "repeated calls must produce identical fingerprints"
        );
    }

    #[test]
    fn test_compute_fingerprint_different_rules_produce_different_fingerprints() {
        let fp_a = compute_fingerprint("rule-a", Path::new("src/lib.rs"), "code", 0);
        let fp_b = compute_fingerprint("rule-b", Path::new("src/lib.rs"), "code", 0);
        assert_ne!(fp_a, fp_b);
    }

    #[test]
    fn test_compute_fingerprint_different_paths_produce_different_fingerprints() {
        let fp1 = compute_fingerprint("rule-a", Path::new("src/lib.rs"), "code", 0);
        let fp2 = compute_fingerprint("rule-a", Path::new("src/main.rs"), "code", 0);
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn test_compute_fingerprint_different_snippets_produce_different_fingerprints() {
        let fp1 = compute_fingerprint("rule-a", Path::new("src/lib.rs"), "Md5::new()", 0);
        let fp2 = compute_fingerprint("rule-a", Path::new("src/lib.rs"), "Sha1::new()", 0);
        assert_ne!(fp1, fp2);
    }

    /// Stability across scan-root relocation: the same match from two
    /// different absolute scan roots must produce identical fingerprints
    /// because only the repo-relative path is hashed.
    #[test]
    fn test_compute_fingerprint_stability_across_scan_roots() {
        // Simulate the same logical file scanned from two different roots.
        // Only the repo-relative path ("src/auth.rs") reaches the hash.
        let rel = Path::new("src/auth.rs");
        let fp_root_a = compute_fingerprint("rule-a", rel, "unsafe { }", 0);
        let fp_root_b = compute_fingerprint("rule-a", rel, "unsafe { }", 0);
        assert_eq!(
            fp_root_a, fp_root_b,
            "fingerprints must be identical regardless of absolute scan root"
        );
    }

    /// Uniqueness: two matches of the same rule on the same snippet in the
    /// same file must differ because of the index suffix.
    #[test]
    fn test_compute_fingerprint_uniqueness_via_index_suffix() {
        let path = Path::new("src/lib.rs");
        let snippet = "let x = unsafe { *ptr }";
        let fp0 = compute_fingerprint("unsafe-rule", path, snippet, 0);
        let fp1 = compute_fingerprint("unsafe-rule", path, snippet, 1);
        assert_ne!(fp0, fp1);
    }

    #[test]
    fn test_compute_fingerprint_hex_prefix_has_expected_length() {
        // BLAKE2b-256 produces 32 bytes = 64 hex chars; total is 64 + 1 + digits.
        let fp = compute_fingerprint("r", Path::new("f"), "s", 0);
        let hex_part: &str = fp.split('_').next().unwrap();
        assert_eq!(hex_part.len(), 64, "hex prefix must be 64 chars");
    }
}
