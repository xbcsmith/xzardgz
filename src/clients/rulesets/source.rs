//! Ruleset source descriptors.
//!
//! [`RulesetSource`] is the canonical type for describing where a ruleset
//! originates.  It is consumed by the fetch layer and stored in the bundle
//! manifest cache.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Describes the origin of a ruleset bundle.
///
/// Each variant maps to a distinct fetch strategy in the acquisition layer.
/// Only `Path` and `Git` are fully implemented in Phase 8; `Url` and `Oci`
/// are reserved for future phases.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use xzardgz::clients::rulesets::source::RulesetSource;
///
/// let local = RulesetSource::Path(PathBuf::from("/opt/rules"));
/// assert_eq!(local.source_kind(), "path");
///
/// let remote = RulesetSource::Git {
///     url: "https://github.com/example/rules.git".to_string(),
///     branch: "main".to_string(),
///     commit: None,
/// };
/// assert_eq!(remote.source_kind(), "git");
/// assert!(remote.display_location().contains("https://github.com/example/rules.git"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RulesetSource {
    /// A directory on the local filesystem containing YAML rule files.
    Path(PathBuf),

    /// A git repository reachable over HTTPS or SSH.
    Git {
        /// Remote repository URL (HTTPS or SSH).
        url: String,
        /// Branch to check out.
        branch: String,
        /// Optional pinned commit SHA.  When `None`, the tip of `branch` is used.
        commit: Option<String>,
    },

    /// A compressed archive served over HTTPS, identified by its SHA-256 digest.
    Url {
        /// Download URL for the archive.
        url: String,
        /// Expected hex-encoded SHA-256 digest of the downloaded archive.
        digest: String,
    },

    /// An OCI image layer containing rule files.
    Oci {
        /// OCI image reference (e.g. `registry.example.com/rules:v1.2.3`).
        reference: String,
        /// Expected OCI digest of the image manifest.
        digest: String,
    },
}

impl RulesetSource {
    /// Returns a short static string identifying the acquisition strategy.
    ///
    /// # Returns
    ///
    /// One of `"path"`, `"git"`, `"url"`, or `"oci"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzardgz::clients::rulesets::source::RulesetSource;
    ///
    /// assert_eq!(RulesetSource::Path(PathBuf::from("/tmp")).source_kind(), "path");
    /// assert_eq!(
    ///     RulesetSource::Git {
    ///         url: "https://github.com/x/y.git".to_string(),
    ///         branch: "main".to_string(),
    ///         commit: None,
    ///     }.source_kind(),
    ///     "git"
    /// );
    /// ```
    pub fn source_kind(&self) -> &'static str {
        match self {
            RulesetSource::Path(_) => "path",
            RulesetSource::Git { .. } => "git",
            RulesetSource::Url { .. } => "url",
            RulesetSource::Oci { .. } => "oci",
        }
    }

    /// Returns a human-readable string identifying where this source originates.
    ///
    /// For `Path`, the filesystem path is returned.
    /// For `Git`, the URL and branch are combined as `url@branch`; if a commit
    /// is pinned, the first 12 characters are appended as `url@branch#commit`.
    /// For `Url`, the download URL is returned.
    /// For `Oci`, the image reference is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzardgz::clients::rulesets::source::RulesetSource;
    ///
    /// let src = RulesetSource::Git {
    ///     url: "https://github.com/example/rules.git".to_string(),
    ///     branch: "main".to_string(),
    ///     commit: Some("abc123".to_string()),
    /// };
    /// let loc = src.display_location();
    /// assert!(loc.contains("https://github.com/example/rules.git"));
    /// assert!(loc.contains("main"));
    /// assert!(loc.contains("abc123"));
    /// ```
    pub fn display_location(&self) -> String {
        match self {
            RulesetSource::Path(p) => p.display().to_string(),
            RulesetSource::Git {
                url,
                branch,
                commit,
            } => match commit {
                Some(sha) => format!("{url}@{branch}#{sha}"),
                None => format!("{url}@{branch}"),
            },
            RulesetSource::Url { url, .. } => url.clone(),
            RulesetSource::Oci { reference, .. } => reference.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // source_kind
    // ------------------------------------------------------------------

    #[test]
    fn test_ruleset_source_path_kind_is_path() {
        let src = RulesetSource::Path(PathBuf::from("/opt/rules"));
        assert_eq!(src.source_kind(), "path");
    }

    #[test]
    fn test_ruleset_source_git_kind_is_git() {
        let src = RulesetSource::Git {
            url: "https://github.com/example/rules.git".to_string(),
            branch: "main".to_string(),
            commit: None,
        };
        assert_eq!(src.source_kind(), "git");
    }

    #[test]
    fn test_ruleset_source_url_kind_is_url() {
        let src = RulesetSource::Url {
            url: "https://example.com/rules.tar.gz".to_string(),
            digest: "deadbeef".to_string(),
        };
        assert_eq!(src.source_kind(), "url");
    }

    #[test]
    fn test_ruleset_source_oci_kind_is_oci() {
        let src = RulesetSource::Oci {
            reference: "registry.example.com/rules:v1.0.0".to_string(),
            digest: "sha256:deadbeef".to_string(),
        };
        assert_eq!(src.source_kind(), "oci");
    }

    // ------------------------------------------------------------------
    // display_location
    // ------------------------------------------------------------------

    #[test]
    fn test_ruleset_source_git_display_location_contains_url() {
        let url = "https://github.com/example/rules.git";
        let src = RulesetSource::Git {
            url: url.to_string(),
            branch: "main".to_string(),
            commit: None,
        };
        assert!(
            src.display_location().contains(url),
            "display_location must contain the URL; got: {}",
            src.display_location()
        );
    }

    #[test]
    fn test_ruleset_source_git_display_location_with_commit_contains_sha() {
        let src = RulesetSource::Git {
            url: "https://github.com/example/rules.git".to_string(),
            branch: "main".to_string(),
            commit: Some("abc123def456".to_string()),
        };
        let loc = src.display_location();
        assert!(
            loc.contains("abc123def456"),
            "must include commit SHA; got: {loc}"
        );
    }

    #[test]
    fn test_ruleset_source_path_display_location_contains_path_string() {
        let src = RulesetSource::Path(PathBuf::from("/opt/rules/rust"));
        assert!(src.display_location().contains("opt/rules/rust"));
    }

    #[test]
    fn test_ruleset_source_url_display_location_returns_url() {
        let url = "https://cdn.example.com/rules.tar.gz";
        let src = RulesetSource::Url {
            url: url.to_string(),
            digest: "deadbeef".to_string(),
        };
        assert_eq!(src.display_location(), url);
    }

    #[test]
    fn test_ruleset_source_oci_display_location_returns_reference() {
        let reference = "registry.example.com/rules:v2.0.0";
        let src = RulesetSource::Oci {
            reference: reference.to_string(),
            digest: "sha256:deadbeef".to_string(),
        };
        assert_eq!(src.display_location(), reference);
    }

    // ------------------------------------------------------------------
    // serialization roundtrip
    // ------------------------------------------------------------------

    #[test]
    fn test_ruleset_source_path_serialization_roundtrip() {
        let src = RulesetSource::Path(PathBuf::from("/tmp/rules"));
        let yaml = serde_yaml::to_string(&src)
            // SAFETY: Path(PathBuf) is always serializable.
            .expect("serialization must succeed");
        let restored: RulesetSource =
            serde_yaml::from_str(&yaml).expect("deserialization must succeed for valid yaml");
        assert_eq!(src, restored);
    }

    #[test]
    fn test_ruleset_source_git_serialization_roundtrip() {
        let src = RulesetSource::Git {
            url: "https://github.com/example/rules.git".to_string(),
            branch: "main".to_string(),
            commit: Some("abc123def456789".to_string()),
        };
        let yaml = serde_yaml::to_string(&src).expect("serialization must succeed");
        let restored: RulesetSource =
            serde_yaml::from_str(&yaml).expect("deserialization must succeed for valid yaml");
        assert_eq!(src, restored);
    }

    #[test]
    fn test_ruleset_source_git_with_no_commit_serialization_roundtrip() {
        let src = RulesetSource::Git {
            url: "https://github.com/example/rules.git".to_string(),
            branch: "main".to_string(),
            commit: None,
        };
        let yaml = serde_yaml::to_string(&src).expect("serialization must succeed");
        let restored: RulesetSource =
            serde_yaml::from_str(&yaml).expect("deserialization must succeed");
        assert_eq!(src, restored);
    }
}
