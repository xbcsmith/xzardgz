//! Helpers for SAST corpus integration tests.
//!
//! Tests that require the `semgrep-rules` corpus are gated behind the
//! `sast-integration-tests` Cargo feature and the `XZARDGZ_SEMGREP_RULES_DIR`
//! environment variable. This module provides the shared skip logic so every
//! corpus test has consistent, observable behaviour when the corpus is absent.

use std::path::PathBuf;

/// Returns the path to the `semgrep-rules` corpus, or `None` if unavailable.
///
/// Reads `XZARDGZ_SEMGREP_RULES_DIR` from the environment. If it is unset or
/// points at a path that does not exist as a directory, this function prints a
/// human-readable skip reason and returns `None`. Callers should treat a `None`
/// return as a skip signal -- the test should return early, not fail.
///
/// # Examples
///
/// ```no_run
/// # use std::path::PathBuf;
/// // In a corpus test gated on the `sast-integration-tests` feature:
/// // let Some(corpus) = sast_corpus::semgrep_rules_dir() else { return; };
/// // ... use corpus ...
/// ```
pub fn semgrep_rules_dir() -> Option<PathBuf> {
    match std::env::var("XZARDGZ_SEMGREP_RULES_DIR") {
        Ok(val) => {
            let path = PathBuf::from(&val);
            if path.is_dir() {
                Some(path)
            } else {
                println!(
                    "SKIP: XZARDGZ_SEMGREP_RULES_DIR=\"{}\" does not point to an existing \
                     directory. Set it to a local checkout of semgrep-rules to run corpus tests.",
                    val
                );
                None
            }
        }
        Err(_) => {
            println!(
                "SKIP: XZARDGZ_SEMGREP_RULES_DIR is not set. \
                 Set it to a local checkout of the semgrep-rules corpus to run corpus tests."
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the helper returns `None` and does not panic when the
    /// environment variable is unset.
    #[test]
    fn test_semgrep_rules_dir_returns_none_when_env_var_unset() {
        let result = temp_env::with_var_unset("XZARDGZ_SEMGREP_RULES_DIR", semgrep_rules_dir);
        assert!(
            result.is_none(),
            "semgrep_rules_dir() must return None when XZARDGZ_SEMGREP_RULES_DIR is unset"
        );
    }

    /// Verify that the helper returns `None` when the environment variable
    /// points at a path that does not exist.
    #[test]
    fn test_semgrep_rules_dir_returns_none_when_path_does_not_exist() {
        let result = temp_env::with_var(
            "XZARDGZ_SEMGREP_RULES_DIR",
            Some("/this/path/does/not/exist/xzardgz-sast-corpus"),
            semgrep_rules_dir,
        );
        assert!(
            result.is_none(),
            "semgrep_rules_dir() must return None when the path does not exist"
        );
    }

    /// Verify that the helper returns `Some` when the environment variable
    /// points at a real directory.
    #[cfg(feature = "sast-integration-tests")]
    #[test]
    fn test_semgrep_rules_dir_returns_some_when_path_exists() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let result = temp_env::with_var(
            "XZARDGZ_SEMGREP_RULES_DIR",
            Some(
                dir.path()
                    .to_str()
                    .expect("temp dir path should be valid UTF-8"),
            ),
            semgrep_rules_dir,
        );
        assert!(
            result.is_some(),
            "semgrep_rules_dir() must return Some when XZARDGZ_SEMGREP_RULES_DIR points to a \
             valid directory"
        );
    }
}
