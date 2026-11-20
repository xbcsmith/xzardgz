//! Error types for the ruleset acquisition layer.
//!
//! [`AcquisitionError`] is the single error type returned by every function in
//! `src/clients/rulesets/`.  Callers that surface errors to the user should
//! convert to [`crate::error::PipelineError`] via `.map_err(|e| PipelineError::Sast(e.to_string()))`.

use thiserror::Error;

use crate::clients::rulesets::license::LicenseClass;

/// All errors that can occur while acquiring a ruleset from any source.
///
/// Each variant captures the information needed to give the operator an
/// actionable diagnosis without requiring them to read a stack trace.
#[derive(Error, Debug)]
pub enum AcquisitionError {
    /// A git operation (clone, fetch, resolve) failed.
    #[error("git operation failed for '{source_id}': {cause}")]
    Git {
        /// The source identifier that triggered the git operation.
        source_id: String,
        /// Human-readable description of the git failure.
        cause: String,
    },

    /// An HTTP request to a URL source failed.
    #[error("HTTP fetch failed for '{url}': {cause}")]
    Http {
        /// The URL that was being fetched.
        url: String,
        /// Human-readable description of the failure.
        cause: String,
    },

    /// A filesystem I/O operation failed.
    #[error("I/O error for '{path}': {cause}")]
    Io {
        /// The path that was being accessed.
        path: String,
        /// Human-readable description of the I/O failure.
        cause: String,
    },

    /// The source's licence class is not permitted by the active policy.
    #[error("licence denied for '{source_id}': class {class:?} \u{2014} {reason}")]
    LicenceDenied {
        /// The source identifier whose licence was denied.
        source_id: String,
        /// The licence class that triggered the denial.
        class: LicenseClass,
        /// Human-readable reason for the denial.
        reason: String,
    },

    /// A sources manifest or bundle manifest could not be parsed.
    #[error("manifest parse error: {0}")]
    ManifestParse(String),

    /// No entry with the given ID exists in the sources manifest.
    #[error("source '{source_id}' not found in manifest")]
    SourceNotFound {
        /// The source identifier that could not be found.
        source_id: String,
    },

    /// The downloaded content's SHA-256 digest did not match the expected value.
    #[error("digest mismatch for '{source_id}': expected {expected}, got {actual}")]
    DigestMismatch {
        /// The source identifier whose content digest failed verification.
        source_id: String,
        /// The expected hex-encoded SHA-256 digest.
        expected: String,
        /// The actual hex-encoded SHA-256 digest computed from the fetched content.
        actual: String,
    },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acquisition_error_git_display_contains_source_id() {
        let err = AcquisitionError::Git {
            source_id: "hashicorp".to_string(),
            cause: "network unreachable".to_string(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("hashicorp"),
            "display must contain the source_id; got: {msg}"
        );
        assert!(msg.contains("network unreachable"));
    }

    #[test]
    fn test_acquisition_error_licence_denied_display_contains_class() {
        let err = AcquisitionError::LicenceDenied {
            source_id: "some-source".to_string(),
            class: LicenseClass::StrongCopyleft,
            reason: "GPL not allowed by policy".to_string(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("StrongCopyleft"),
            "display must contain the LicenseClass debug name; got: {msg}"
        );
        assert!(msg.contains("some-source"));
        assert!(msg.contains("GPL not allowed by policy"));
    }

    #[test]
    fn test_acquisition_error_digest_mismatch_display_contains_both_digests() {
        let err = AcquisitionError::DigestMismatch {
            source_id: "elttam".to_string(),
            expected: "aabbccddeeff0011".to_string(),
            actual: "1100ffeeddccbbaa".to_string(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("aabbccddeeff0011"),
            "display must contain expected digest; got: {msg}"
        );
        assert!(
            msg.contains("1100ffeeddccbbaa"),
            "display must contain actual digest; got: {msg}"
        );
        assert!(msg.contains("elttam"), "display must contain source_id");
    }

    #[test]
    fn test_acquisition_error_http_display_contains_url() {
        let err = AcquisitionError::Http {
            url: "https://example.com/rules.tar.gz".to_string(),
            cause: "connection timeout".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("https://example.com/rules.tar.gz"));
        assert!(msg.contains("connection timeout"));
    }

    #[test]
    fn test_acquisition_error_io_display_contains_path_and_cause() {
        let err = AcquisitionError::Io {
            path: "/var/cache/xzardgz/rules".to_string(),
            cause: "permission denied".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("/var/cache/xzardgz/rules"));
        assert!(msg.contains("permission denied"));
    }

    #[test]
    fn test_acquisition_error_manifest_parse_display_contains_message() {
        let err = AcquisitionError::ManifestParse("unexpected token at line 3".to_string());
        let msg = err.to_string();
        assert!(msg.contains("manifest parse error"));
        assert!(msg.contains("unexpected token at line 3"));
    }

    #[test]
    fn test_acquisition_error_source_not_found_display_contains_source_id() {
        let err = AcquisitionError::SourceNotFound {
            source_id: "unknown-source".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("unknown-source"));
    }
}
