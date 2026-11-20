//! Bundle manifest: the persistent record of a successful ruleset fetch.
//!
//! One [`BundleManifest`] is written per source after `xzardgz ruleset sync`.
//! It is used by the resolver to verify cache freshness and by the licence
//! policy enforcer to gate which rulesets may load.

use serde::{Deserialize, Serialize};

use crate::clients::rulesets::license::{self, LicenseClass};

/// Persistent record written to the cache after every successful ruleset fetch.
///
/// One `BundleManifest` is stored per `(source_id, resolved_commit)` pair.
/// Consumers read this record to decide whether a re-fetch is necessary and
/// to verify that licence policy was checked at fetch time.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::rulesets::manifest::BundleManifest;
/// use xzardgz::clients::rulesets::license::LicenseClass;
///
/// let m = BundleManifest::new(
///     "my-source",
///     None,
///     Some("MIT".to_string()),
///     None,
///     "deadbeef00112233",
///     42,
///     "2024-01-01T00:00:00Z",
/// );
/// assert_eq!(m.license_class, LicenseClass::Permissive);
/// assert!(m.allowed_by_policy);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BundleManifest {
    /// Identifier of the source as it appears in the sources manifest.
    pub source_id: String,

    /// Resolved git commit SHA (40-char hex), or `None` for path/URL sources.
    pub resolved_commit: Option<String>,

    /// Licence declared in the source entry (`metadata.license` field).
    pub declared_license: Option<String>,

    /// Licence observed in the fetched repository (`LICENSE` / `COPYING` file content).
    pub observed_license: Option<String>,

    /// Classification of the effective licence (`observed_license` takes precedence
    /// over `declared_license`; `Unknown` when both are absent).
    pub license_class: LicenseClass,

    /// SHA-256 hex digest of the fetched content tree.
    pub content_digest: String,

    /// Number of YAML rule files found in the fetched content.
    pub rule_count: usize,

    /// ISO-8601 UTC timestamp of this manifest creation.
    pub fetched_at: String,

    /// Whether the default licence policy allows this source to load.
    pub allowed_by_policy: bool,
}

impl BundleManifest {
    /// Constructs a new `BundleManifest`, computing `license_class` and
    /// `allowed_by_policy` from the provided licence strings.
    ///
    /// Classification precedence:
    /// 1. `observed_license` — the licence text found in the fetched repo.
    /// 2. `declared_license` — the SPDX identifier from the manifest entry.
    /// 3. `LicenseClass::Unknown` when both are `None`.
    ///
    /// # Arguments
    ///
    /// * `source_id`         - Source identifier from the manifest.
    /// * `resolved_commit`   - Resolved git SHA, or `None` for non-git sources.
    /// * `declared_license`  - Licence declared in the sources manifest entry.
    /// * `observed_license`  - Licence text found in the fetched repository.
    /// * `content_digest`    - SHA-256 hex digest of the content tree.
    /// * `rule_count`        - Number of YAML rule files found.
    /// * `fetched_at`        - ISO-8601 UTC timestamp.
    ///
    /// # Returns
    ///
    /// A fully populated [`BundleManifest`] with `license_class` and
    /// `allowed_by_policy` derived from the effective licence.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::clients::rulesets::manifest::BundleManifest;
    /// use xzardgz::clients::rulesets::license::LicenseClass;
    ///
    /// // Observed licence takes precedence over declared.
    /// let m = BundleManifest::new(
    ///     "hashicorp",
    ///     Some("abc123".to_string()),
    ///     Some("MPL-2.0".to_string()),      // declared
    ///     Some("MIT".to_string()),           // observed (wins)
    ///     "cafebabe",
    ///     10,
    ///     "2024-06-01T12:00:00Z",
    /// );
    /// assert_eq!(m.license_class, LicenseClass::Permissive);
    /// assert!(m.allowed_by_policy);
    /// ```
    pub fn new(
        source_id: impl Into<String>,
        resolved_commit: Option<String>,
        declared_license: Option<String>,
        observed_license: Option<String>,
        content_digest: impl Into<String>,
        rule_count: usize,
        fetched_at: impl Into<String>,
    ) -> Self {
        let effective = observed_license.as_deref().or(declared_license.as_deref());
        let license_class = match effective {
            Some(text) => license::classify_license(text),
            None => LicenseClass::Unknown,
        };
        let allowed_by_policy = license_class.is_allowed_by_default_policy();

        BundleManifest {
            source_id: source_id.into(),
            resolved_commit,
            declared_license,
            observed_license,
            license_class,
            content_digest: content_digest.into(),
            rule_count,
            fetched_at: fetched_at.into(),
            allowed_by_policy,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> BundleManifest {
        BundleManifest {
            source_id: "test-source".to_string(),
            resolved_commit: Some("abc123def456".to_string()),
            declared_license: Some("MIT".to_string()),
            observed_license: None,
            license_class: LicenseClass::Permissive,
            content_digest: "deadbeef".to_string(),
            rule_count: 42,
            fetched_at: "2024-01-01T00:00:00Z".to_string(),
            allowed_by_policy: true,
        }
    }

    // ------------------------------------------------------------------
    // BundleManifest::new: spec-required tests
    // ------------------------------------------------------------------

    #[test]
    fn test_bundle_manifest_new_permissive_observed_license_is_allowed() {
        let m = BundleManifest::new(
            "example",
            None,
            None,
            Some("MIT".to_string()),
            "aabbcc",
            5,
            "2024-01-01T00:00:00Z",
        );
        assert_eq!(m.license_class, LicenseClass::Permissive);
        assert!(
            m.allowed_by_policy,
            "Permissive licence must be allowed by default policy"
        );
    }

    #[test]
    fn test_bundle_manifest_new_strong_copyleft_observed_license_is_denied() {
        let m = BundleManifest::new(
            "example",
            None,
            None,
            Some("GPL-3.0".to_string()),
            "aabbcc",
            5,
            "2024-01-01T00:00:00Z",
        );
        assert_eq!(m.license_class, LicenseClass::StrongCopyleft);
        assert!(
            !m.allowed_by_policy,
            "StrongCopyleft licence must be denied by default policy"
        );
    }

    #[test]
    fn test_bundle_manifest_new_no_license_produces_unknown_and_denied() {
        let m = BundleManifest::new(
            "example",
            None,
            None, // no declared
            None, // no observed
            "aabbcc",
            0,
            "2024-01-01T00:00:00Z",
        );
        assert_eq!(
            m.license_class,
            LicenseClass::Unknown,
            "both licences absent must produce Unknown class"
        );
        assert!(
            !m.allowed_by_policy,
            "Unknown licence must be denied by default policy"
        );
    }

    #[test]
    fn test_bundle_manifest_new_observed_takes_precedence_over_declared() {
        let m = BundleManifest::new(
            "example",
            None,
            Some("MPL-2.0".to_string()),  // declared: WeakCopyleft
            Some("AGPL-3.0".to_string()), // observed: NetworkCopyleft (wins)
            "aabbcc",
            3,
            "2024-01-01T00:00:00Z",
        );
        assert_eq!(
            m.license_class,
            LicenseClass::NetworkCopyleft,
            "observed_license must take precedence over declared_license"
        );
        assert!(!m.allowed_by_policy);
    }

    #[test]
    fn test_bundle_manifest_serialization_roundtrip() {
        let manifest = BundleManifest::new(
            "roundtrip-source",
            Some("deadbeef12345678".to_string()),
            Some("Apache-2.0".to_string()),
            None,
            "cafebabe99887766",
            100,
            "2024-06-15T08:30:00Z",
        );
        let yaml = serde_yaml::to_string(&manifest)
            // SAFETY: BundleManifest is always serializable.
            .expect("serialization must succeed");
        let restored: BundleManifest =
            serde_yaml::from_str(&yaml).expect("deserialization must succeed");
        assert_eq!(
            manifest, restored,
            "roundtrip must produce identical manifest"
        );
    }

    #[test]
    fn test_bundle_manifest_new_commons_clause_observed_is_restricted_and_denied() {
        let m = BundleManifest::new(
            "example",
            None,
            None,
            Some("MIT WITH Commons Clause".to_string()),
            "aabbcc",
            7,
            "2024-01-01T00:00:00Z",
        );
        assert_eq!(
            m.license_class,
            LicenseClass::Restricted,
            "Commons Clause must classify as Restricted, not WeakCopyleft"
        );
        assert!(
            !m.allowed_by_policy,
            "Restricted licence must be denied by default policy"
        );
    }

    // ------------------------------------------------------------------
    // Pre-existing tests (preserved)
    // ------------------------------------------------------------------

    #[test]
    fn test_bundle_manifest_roundtrips_via_serde_yaml() {
        let manifest = sample_manifest();
        let yaml = serde_yaml::to_string(&manifest).unwrap();
        let restored: BundleManifest = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(restored.source_id, manifest.source_id);
        assert_eq!(restored.resolved_commit, manifest.resolved_commit);
        assert_eq!(restored.declared_license, manifest.declared_license);
        assert_eq!(restored.rule_count, manifest.rule_count);
        assert_eq!(restored.content_digest, manifest.content_digest);
        assert_eq!(restored.allowed_by_policy, manifest.allowed_by_policy);
    }

    #[test]
    fn test_bundle_manifest_with_no_commit_roundtrips_via_serde_yaml() {
        let mut manifest = sample_manifest();
        manifest.resolved_commit = None;
        let yaml = serde_yaml::to_string(&manifest).unwrap();
        let restored: BundleManifest = serde_yaml::from_str(&yaml).unwrap();
        assert!(restored.resolved_commit.is_none());
    }

    #[test]
    fn test_bundle_manifest_license_class_serializes_as_string() {
        let manifest = sample_manifest();
        let yaml = serde_yaml::to_string(&manifest).unwrap();
        assert!(yaml.contains("Permissive"));
    }
}
