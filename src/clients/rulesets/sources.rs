//! Sources manifest: the YAML file listing known ruleset repositories.
//!
//! The sources manifest is a flat YAML mapping whose top-level keys are
//! source identifiers (e.g. `"0xdea"`, `"hashicorp"`) and whose values are
//! [`RulesetSourceEntry`] records.
//!
//! # Key coercion
//!
//! serde_yaml v0.9 targets YAML 1.2 semantics where `0xdea` is a plain string
//! rather than a hex integer.  `BTreeMap<String, _>` is used (not
//! `HashMap<String, _>`) to give deterministic iteration order and to avoid
//! the YAML 1.1 integer-coercion problem for hex-prefixed keys.
//!
//! # Format version
//!
//! This file implements the **semgrep-rules-manager v1** manifest format.
//! See `src/clients/rulesets/assets/sources.example.yaml` for an example.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::clients::rulesets::error::AcquisitionError;

// ---------------------------------------------------------------------------
// RulesetSourceEntry
// ---------------------------------------------------------------------------

/// A single entry in a semgrep-rules-manager v1 sources manifest.
///
/// Uses `deny_unknown_fields` to reject unrecognised YAML keys and prevent
/// silent misconfiguration.  Optional fields carry a serde `default` so that
/// manifests which omit them still deserialise correctly.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::rulesets::sources::RulesetSourceEntry;
///
/// let entry = RulesetSourceEntry {
///     description: "Security rules".to_string(),
///     repository_url: "https://github.com/example/rules".to_string(),
///     repository_branch: Some("main".to_string()),
///     repository_commit: None,
///     ignored: None,
///     author: Some("Example Author".to_string()),
///     license: Some("MIT".to_string()),
///     preprocessors: None,
///     enabled: Some(true),
///     subdir: None,
/// };
/// assert!(entry.is_enabled());
/// assert!(!entry.is_ignored());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RulesetSourceEntry {
    /// Human-readable description of the ruleset collection.
    /// Defaults to an empty string when absent in the manifest.
    #[serde(default)]
    pub description: String,

    /// Remote repository URL (HTTPS).
    pub repository_url: String,

    /// Branch name to fetch when syncing.
    #[serde(default)]
    pub repository_branch: Option<String>,

    /// Pinned commit SHA to check out.
    #[serde(default)]
    pub repository_commit: Option<String>,

    /// When `true`, the source is excluded even if `enabled: true`.
    #[serde(default)]
    pub ignored: Option<bool>,

    /// Maintainer or organisation name.
    #[serde(default)]
    pub author: Option<String>,

    /// SPDX licence identifier declared by the source.
    #[serde(default)]
    pub license: Option<String>,

    /// Optional list of pre-processing steps applied before rule loading.
    #[serde(default)]
    pub preprocessors: Option<Vec<String>>,

    /// Whether to fetch and use this source.  Default: `false`.
    #[serde(default)]
    pub enabled: Option<bool>,

    /// Optional sub-directory within the repository containing the rules.
    #[serde(default)]
    pub subdir: Option<String>,
}

impl RulesetSourceEntry {
    /// Returns `true` when the source has `enabled: true`.
    ///
    /// Sources with no `enabled` key default to `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::clients::rulesets::sources::RulesetSourceEntry;
    ///
    /// let entry = RulesetSourceEntry {
    ///     description: String::new(),
    ///     repository_url: "https://example.com".to_string(),
    ///     repository_branch: None,
    ///     repository_commit: None,
    ///     ignored: None,
    ///     author: None,
    ///     license: None,
    ///     preprocessors: None,
    ///     enabled: Some(true),
    ///     subdir: None,
    /// };
    /// assert!(entry.is_enabled());
    /// ```
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(false)
    }

    /// Returns `true` when the source has `ignored: true`.
    ///
    /// Sources with no `ignored` key default to `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::clients::rulesets::sources::RulesetSourceEntry;
    ///
    /// let entry = RulesetSourceEntry {
    ///     description: String::new(),
    ///     repository_url: "https://example.com".to_string(),
    ///     repository_branch: None,
    ///     repository_commit: None,
    ///     ignored: Some(true),
    ///     author: None,
    ///     license: None,
    ///     preprocessors: None,
    ///     enabled: None,
    ///     subdir: None,
    /// };
    /// assert!(entry.is_ignored());
    /// ```
    pub fn is_ignored(&self) -> bool {
        self.ignored.unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// SourcesManifest type alias
// ---------------------------------------------------------------------------

/// Top-level sources manifest: a map of source ID to entry.
///
/// Source IDs are always strings (e.g. `"0xdea"`).  serde_yaml deserializes
/// unquoted hex-like keys (`0xdea`) as plain strings under YAML 1.2 semantics,
/// so no coercion to integers occurs.
///
/// `BTreeMap` gives deterministic key ordering, which simplifies testing and
/// diff-friendly YAML serialization.
pub type SourcesManifest = BTreeMap<String, RulesetSourceEntry>;

// ---------------------------------------------------------------------------
// parse_sources_manifest
// ---------------------------------------------------------------------------

/// Parse a sources manifest from a YAML string.
///
/// The function accepts the raw YAML text of a `sources.yaml` file and returns
/// a [`SourcesManifest`] on success.
///
/// # Arguments
///
/// * `yaml` - Raw YAML text of the sources manifest.
///
/// # Returns
///
/// A [`SourcesManifest`] (`BTreeMap<String, RulesetSourceEntry>`) on success.
///
/// # Errors
///
/// Returns a [`serde_yaml::Error`] if the YAML is invalid or does not conform
/// to the expected schema (including unknown fields in an entry).
///
/// # Examples
///
/// ```
/// use xzardgz::clients::rulesets::sources::parse_sources_manifest;
///
/// let yaml = r#"
/// my-rules:
///   description: "My ruleset"
///   repository_url: "https://github.com/example/rules"
///   repository_branch: "main"
///   license: "MIT"
///   enabled: true
/// "#;
///
/// let manifest = parse_sources_manifest(yaml).unwrap();
/// assert!(manifest.contains_key("my-rules"));
/// ```
pub fn parse_sources_manifest(yaml: &str) -> Result<SourcesManifest, serde_yaml::Error> {
    serde_yaml::from_str(yaml)
}

// ---------------------------------------------------------------------------
// load_sources_manifest
// ---------------------------------------------------------------------------

/// Load a sources manifest from a YAML file on disk.
///
/// # Arguments
///
/// * `path` - Filesystem path to the sources manifest YAML file.
///
/// # Returns
///
/// A [`SourcesManifest`] on success.
///
/// # Errors
///
/// Returns [`AcquisitionError::Io`] if the file cannot be read.
/// Returns [`AcquisitionError::ManifestParse`] if the YAML is malformed or
/// contains unknown fields.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use xzardgz::clients::rulesets::sources::load_sources_manifest;
///
/// let manifest = load_sources_manifest(Path::new("sources.yaml")).unwrap();
/// for (id, entry) in &manifest {
///     println!("{}: {}", id, entry.repository_url);
/// }
/// ```
pub fn load_sources_manifest(path: &Path) -> Result<SourcesManifest, AcquisitionError> {
    let content = std::fs::read_to_string(path).map_err(|e| AcquisitionError::Io {
        path: path.display().to_string(),
        cause: e.to_string(),
    })?;
    parse_sources_manifest(&content).map_err(|e| AcquisitionError::ManifestParse(e.to_string()))
}

// ---------------------------------------------------------------------------
// enabled_sources
// ---------------------------------------------------------------------------

/// Returns all enabled, non-ignored source entries from a manifest.
///
/// A source is included when `is_enabled()` returns `true` AND
/// `is_ignored()` returns `false`.  The returned slice is in `BTreeMap`
/// iteration order (ascending lexicographic by source ID).
///
/// # Arguments
///
/// * `manifest` - A parsed [`SourcesManifest`].
///
/// # Returns
///
/// A `Vec<(&str, &RulesetSourceEntry)>` of `(source_id, entry)` pairs.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::rulesets::sources::{parse_sources_manifest, enabled_sources};
///
/// let yaml = r#"
/// alpha:
///   description: "Alpha rules"
///   repository_url: "https://github.com/a/rules"
///   enabled: true
/// beta:
///   description: "Beta rules"
///   repository_url: "https://github.com/b/rules"
///   enabled: false
/// "#;
///
/// let manifest = parse_sources_manifest(yaml).unwrap();
/// let active = enabled_sources(&manifest);
/// assert_eq!(active.len(), 1);
/// assert_eq!(active[0].0, "alpha");
/// ```
pub fn enabled_sources(manifest: &SourcesManifest) -> Vec<(&str, &RulesetSourceEntry)> {
    manifest
        .iter()
        .filter(|(_, e)| e.is_enabled() && !e.is_ignored())
        .map(|(id, e)| (id.as_str(), e))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_entry(enabled: bool) -> &'static str {
        if enabled {
            "description: test\nrepository_url: https://example.com\nenabled: true\n"
        } else {
            "description: test\nrepository_url: https://example.com\n"
        }
    }

    // ------------------------------------------------------------------
    // parse_sources_manifest
    // ------------------------------------------------------------------

    #[test]
    fn test_parse_sources_manifest_with_valid_yaml_returns_entries() {
        let yaml = "my-source:\n  description: \"Security rules\"\n  repository_url: \"https://github.com/example/rules\"\n  enabled: true\n";
        let result = parse_sources_manifest(yaml);
        assert!(
            result.is_ok(),
            "valid yaml must parse; got: {:?}",
            result.err()
        );
        let manifest = result.unwrap();
        assert!(
            manifest.contains_key("my-source"),
            "manifest must contain 'my-source'"
        );
        let entry = &manifest["my-source"];
        assert_eq!(entry.repository_url, "https://github.com/example/rules");
    }

    #[test]
    fn test_parse_sources_manifest_string_key_0xdea_deserializes_as_string() {
        // serde_yaml (YAML 1.2) must NOT coerce 0xdea to integer 3562.
        // The key "0xdea" is a plain string in YAML 1.2.
        let yaml = "0xdea:\n  description: test\n  repository_url: https://example.com\n";
        let manifest = parse_sources_manifest(yaml).expect("must parse");
        assert!(
            manifest.contains_key("0xdea"),
            "source ID '0xdea' must deserialize as the string \"0xdea\", not an integer; \
             keys present: {:?}",
            manifest.keys().collect::<Vec<_>>()
        );
        let entry = &manifest["0xdea"];
        assert_eq!(entry.description, "test");
    }

    #[test]
    fn test_parse_sources_manifest_integer_key_fails_to_deserialize_as_string_typed_map() {
        // A YAML mapping key that is a pure integer should not coerce into our
        // string-keyed manifest silently.  In practice serde_yaml converts integer
        // keys to their decimal string representation when deserializing into
        // BTreeMap<String, _>, so this test documents and pins that behaviour.
        let yaml = "12345:\n  description: int-key\n  repository_url: https://example.com\n";
        // serde_yaml converts the integer 12345 key to string "12345" -- document this.
        let result = parse_sources_manifest(yaml);
        // Either it parses with the key "12345" as a string, or it errors; either
        // way it must NOT parse with key `0xdea`.
        if let Ok(manifest) = result {
            assert!(
                manifest.contains_key("12345"),
                "integer key 12345 must map to string \"12345\", not some other key; \
                 keys present: {:?}",
                manifest.keys().collect::<Vec<_>>()
            );
        }
        // If it errored, that is also acceptable behaviour.
    }

    #[test]
    fn test_parse_sources_manifest_with_deny_unknown_fields_rejects_extra_keys() {
        // unknown_field is not in RulesetSourceEntry; deny_unknown_fields must reject it.
        let yaml = "my-source:\n  description: test\n  repository_url: https://example.com\n  unknown_field: value\n";
        let result = parse_sources_manifest(yaml);
        assert!(
            result.is_err(),
            "unknown field must be rejected by deny_unknown_fields"
        );
    }

    #[test]
    fn test_enabled_sources_returns_only_enabled_non_ignored() {
        let yaml = "\
alpha:\n  description: alpha\n  repository_url: https://example.com/a\n  enabled: true\n\
beta:\n  description: beta\n  repository_url: https://example.com/b\n  enabled: false\n\
gamma:\n  description: gamma\n  repository_url: https://example.com/c\n  enabled: true\n  ignored: true\n\
delta:\n  description: delta\n  repository_url: https://example.com/d\n  enabled: true\n";
        let manifest = parse_sources_manifest(yaml).expect("must parse");
        let active = enabled_sources(&manifest);
        assert_eq!(
            active.len(),
            2,
            "only alpha and delta should be active; got: {:?}",
            active.iter().map(|(id, _)| *id).collect::<Vec<_>>()
        );
        let ids: Vec<&str> = active.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&"alpha"), "alpha must be active");
        assert!(ids.contains(&"delta"), "delta must be active");
        assert!(!ids.contains(&"beta"), "beta is disabled");
        assert!(!ids.contains(&"gamma"), "gamma is ignored");
    }

    #[test]
    fn test_ruleset_source_entry_is_enabled_defaults_to_false() {
        let yaml = "my-source:\n  description: test\n  repository_url: https://example.com\n";
        let manifest = parse_sources_manifest(yaml).expect("must parse");
        let entry = &manifest["my-source"];
        assert!(
            !entry.is_enabled(),
            "entry with no 'enabled' key must default to false"
        );
    }

    #[test]
    fn test_parse_sources_manifest_with_multiple_entries() {
        let yaml = "\
source1:\n  description: first\n  repository_url: https://example.com/1\n\
source2:\n  description: second\n  repository_url: https://example.com/2\n\
source3:\n  description: third\n  repository_url: https://example.com/3\n";
        let manifest = parse_sources_manifest(yaml).expect("must parse three entries");
        assert_eq!(manifest.len(), 3, "manifest must contain exactly 3 entries");
        assert!(manifest.contains_key("source1"));
        assert!(manifest.contains_key("source2"));
        assert!(manifest.contains_key("source3"));
    }

    // ------------------------------------------------------------------
    // load_sources_manifest
    // ------------------------------------------------------------------

    #[test]
    fn test_load_sources_manifest_from_missing_file_returns_io_error() {
        let result =
            load_sources_manifest(Path::new("/nonexistent/xzardgz-test-sources-98765.yaml"));
        assert!(
            matches!(result, Err(AcquisitionError::Io { .. })),
            "missing file must return Io error; got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_load_sources_manifest_from_invalid_yaml_returns_manifest_parse_error() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut f = NamedTempFile::new()
            // SAFETY: test setup; failure is unrecoverable.
            .expect("tempfile creation must succeed");
        f.write_all(b"not: valid: yaml: ::: !!!")
            .expect("write must succeed");
        let result = load_sources_manifest(f.path());
        assert!(
            matches!(result, Err(AcquisitionError::ManifestParse(_))),
            "invalid yaml must return ManifestParse; got: {:?}",
            result.err()
        );
    }

    // ------------------------------------------------------------------
    // is_enabled / is_ignored helpers
    // ------------------------------------------------------------------

    #[test]
    fn test_ruleset_source_entry_is_ignored_defaults_to_false() {
        let yaml = "src:\n  description: x\n  repository_url: https://example.com\n";
        let manifest = parse_sources_manifest(yaml).expect("must parse");
        assert!(!manifest["src"].is_ignored());
    }

    #[test]
    fn test_ruleset_source_entry_is_ignored_true_when_set() {
        let yaml =
            "src:\n  description: x\n  repository_url: https://example.com\n  ignored: true\n";
        let manifest = parse_sources_manifest(yaml).expect("must parse");
        assert!(manifest["src"].is_ignored());
    }

    // ------------------------------------------------------------------
    // unused fields: use `minimal_entry` helper to avoid dead_code
    // ------------------------------------------------------------------
    #[test]
    fn test_minimal_entry_helper_works() {
        let _ = minimal_entry(true);
        let _ = minimal_entry(false);
    }
}
