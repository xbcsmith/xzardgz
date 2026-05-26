//! Report envelope: the top-level container for a plugin analysis report.
//!
//! A [`ReportEnvelope`] is the single serializable document that a plugin
//! produces at the end of its analysis run. It bundles together provenance
//! metadata (repository, commit, workspace), findings, diagnostics, and an
//! overall risk band into one self-describing JSON artifact.

use crate::diagnostics::Diagnostic;
use crate::error::{PipelineError, Result};
use crate::providers::types::ProviderMetadata;
use crate::reports::findings::PluginFindings;
use crate::reports::risk_band::RiskBand;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Version constant
// ---------------------------------------------------------------------------

/// Schema version for the [`ReportEnvelope`] format.
///
/// Consumers should check this value to decide whether they can process the
/// envelope. This release always writes `"1"`.
pub const REPORT_ENVELOPE_VERSION: &str = "1";

// ---------------------------------------------------------------------------
// ReportEnvelope
// ---------------------------------------------------------------------------

/// Top-level container for a plugin analysis report.
///
/// A `ReportEnvelope` is produced once per plugin invocation and captures
/// everything needed to understand, reproduce, and act on the analysis:
/// provenance metadata, findings, diagnostics, and overall risk.
///
/// # Examples
///
/// ```
/// use xzardgz::reports::envelope::ReportEnvelope;
///
/// let env = ReportEnvelope::new("report-001", "secret_scanner", "ws-abc");
/// assert_eq!(env.plugin_name, "secret_scanner");
/// assert_eq!(env.workspace_id, "ws-abc");
/// assert_eq!(env.version, "1");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportEnvelope {
    /// Schema version. Always [`REPORT_ENVELOPE_VERSION`] for this release.
    pub version: String,
    /// Unique report identifier (ULID or UUID string).
    pub report_id: String,
    /// UTC timestamp when this report was generated.
    pub generated_at: DateTime<Utc>,
    /// Name of the plugin that produced this report.
    pub plugin_name: String,
    /// Repository name, if known.
    pub repository_name: Option<String>,
    /// Repository URL, if known.
    pub repository_url: Option<String>,
    /// HEAD commit SHA at scan time, if known.
    pub head_commit: Option<String>,
    /// Workspace identifier.
    pub workspace_id: String,
    /// Scan artifact schema version.
    pub scan_artifact_version: Option<String>,
    /// Provider metadata (name, model list, capabilities).
    pub provider_metadata: Option<ProviderMetadata>,
    /// Model identifier used for analysis.
    pub model_id: Option<String>,
    /// All findings produced by the plugin.
    pub findings: PluginFindings,
    /// Diagnostics collected during the plugin run.
    pub diagnostics: Vec<Diagnostic>,
    /// Overall risk band derived from findings.
    pub risk_band: Option<RiskBand>,
    /// Named numeric scores produced by the plugin (e.g. `"quality_score"`, `"security_score"`).
    ///
    /// Values are in the range `[0.0, 1.0]` by convention but this is not
    /// enforced at the type level.
    #[serde(default)]
    pub scores: HashMap<String, f64>,
}

impl ReportEnvelope {
    /// Creates a new `ReportEnvelope` with the current UTC timestamp.
    ///
    /// All optional fields are set to `None`, findings and diagnostics are
    /// empty, and `version` is set to [`REPORT_ENVELOPE_VERSION`].
    ///
    /// # Arguments
    ///
    /// * `report_id`    - Unique report identifier (e.g. a ULID).
    /// * `plugin_name`  - Name of the plugin generating this report.
    /// * `workspace_id` - Workspace identifier for the current run.
    ///
    /// # Returns
    ///
    /// A new [`ReportEnvelope`] ready to be populated with findings.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::envelope::ReportEnvelope;
    ///
    /// let env = ReportEnvelope::new("r-001", "my_plugin", "ws-xyz");
    /// assert_eq!(env.version, "1");
    /// assert_eq!(env.report_id, "r-001");
    /// assert_eq!(env.plugin_name, "my_plugin");
    /// assert_eq!(env.workspace_id, "ws-xyz");
    /// assert!(env.findings.is_empty());
    /// assert!(env.diagnostics.is_empty());
    /// assert!(env.risk_band.is_none());
    /// ```
    pub fn new(
        report_id: impl Into<String>,
        plugin_name: impl Into<String>,
        workspace_id: impl Into<String>,
    ) -> Self {
        Self {
            version: REPORT_ENVELOPE_VERSION.to_string(),
            report_id: report_id.into(),
            generated_at: Utc::now(),
            plugin_name: plugin_name.into(),
            repository_name: None,
            repository_url: None,
            head_commit: None,
            workspace_id: workspace_id.into(),
            scan_artifact_version: None,
            provider_metadata: None,
            model_id: None,
            findings: PluginFindings::new(),
            diagnostics: Vec::new(),
            risk_band: None,
            scores: HashMap::new(),
        }
    }

    /// Serializes this envelope to a pretty-printed JSON string.
    ///
    /// # Returns
    ///
    /// A `Result<String>` containing the JSON, or a [`PipelineError::Report`]
    /// if serialization fails.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Report`] if `serde_json` cannot serialize the
    /// envelope (in practice this should not occur for well-formed data).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::envelope::ReportEnvelope;
    ///
    /// let env = ReportEnvelope::new("r-001", "plugin", "ws-001");
    /// let json = env.to_json().expect("serialization failed");
    /// assert!(json.contains("\"plugin_name\""));
    /// assert!(json.contains("plugin"));
    /// ```
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| PipelineError::Report(format!("failed to serialize report envelope: {e}")))
    }

    /// Deserializes a `ReportEnvelope` from a JSON string.
    ///
    /// # Arguments
    ///
    /// * `json` - A JSON string previously produced by [`ReportEnvelope::to_json`].
    ///
    /// # Returns
    ///
    /// A `Result<ReportEnvelope>`.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Report`] if the JSON is malformed or does not
    /// match the expected schema.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::envelope::ReportEnvelope;
    ///
    /// let env = ReportEnvelope::new("r-001", "plugin", "ws-001");
    /// let json = env.to_json().expect("serialization failed");
    /// let restored = ReportEnvelope::load_from_json(&json).expect("deserialization failed");
    /// assert_eq!(restored.report_id, "r-001");
    /// ```
    pub fn load_from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| {
            PipelineError::Report(format!("failed to deserialize report envelope: {e}"))
        })
    }

    /// Returns the total number of findings in this envelope.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::envelope::ReportEnvelope;
    ///
    /// let env = ReportEnvelope::new("r-001", "plugin", "ws-001");
    /// assert_eq!(env.finding_count(), 0);
    /// ```
    pub fn finding_count(&self) -> usize {
        self.findings.len()
    }

    /// Writes this envelope as pretty-printed JSON to `path`.
    ///
    /// Parent directories are created automatically when they do not already
    /// exist.
    ///
    /// # Arguments
    ///
    /// * `path` - Destination file path. Parent directories are created if absent.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Report`] if serialization fails, or
    /// [`PipelineError::Io`] if file-system operations fail.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use xzardgz::reports::envelope::ReportEnvelope;
    ///
    /// let env = ReportEnvelope::new("r-001", "plugin", "ws-001");
    /// env.write_to_file(Path::new("/tmp/reports/report.json")).expect("write failed");
    /// ```
    pub fn write_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let json = self.to_json()?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::findings::PluginFinding;
    use crate::scanner::findings::FindingSeverity;

    fn make_envelope() -> ReportEnvelope {
        ReportEnvelope::new("test-report-001", "test_plugin", "ws-test")
    }

    // ------------------------------------------------------------------
    // new
    // ------------------------------------------------------------------

    #[test]
    fn test_report_envelope_new_sets_version() {
        let env = make_envelope();
        assert_eq!(env.version, REPORT_ENVELOPE_VERSION);
        assert_eq!(env.version, "1");
    }

    #[test]
    fn test_report_envelope_new_sets_report_id() {
        let env = make_envelope();
        assert_eq!(env.report_id, "test-report-001");
    }

    #[test]
    fn test_report_envelope_new_sets_plugin_name() {
        let env = make_envelope();
        assert_eq!(env.plugin_name, "test_plugin");
    }

    #[test]
    fn test_report_envelope_new_sets_workspace_id() {
        let env = make_envelope();
        assert_eq!(env.workspace_id, "ws-test");
    }

    #[test]
    fn test_report_envelope_new_sets_all_optionals_to_none() {
        let env = make_envelope();
        assert!(env.repository_name.is_none());
        assert!(env.repository_url.is_none());
        assert!(env.head_commit.is_none());
        assert!(env.scan_artifact_version.is_none());
        assert!(env.provider_metadata.is_none());
        assert!(env.model_id.is_none());
        assert!(env.risk_band.is_none());
    }

    #[test]
    fn test_report_envelope_new_sets_empty_findings_and_diagnostics() {
        let env = make_envelope();
        assert!(env.findings.is_empty());
        assert!(env.diagnostics.is_empty());
    }

    #[test]
    fn test_report_envelope_new_generated_at_is_recent() {
        let before = Utc::now();
        let env = make_envelope();
        let after = Utc::now();
        assert!(env.generated_at >= before);
        assert!(env.generated_at <= after);
    }

    // ------------------------------------------------------------------
    // to_json / load_from_json round-trip
    // ------------------------------------------------------------------

    #[test]
    fn test_report_envelope_to_json_produces_non_empty_string() {
        let env = make_envelope();
        // SAFETY: ReportEnvelope with valid data cannot fail serialization.
        let json = env.to_json().unwrap();
        assert!(!json.is_empty());
        assert!(json.contains("test_plugin"));
        assert!(json.contains("test-report-001"));
    }

    #[test]
    fn test_report_envelope_to_json_and_load_from_json_roundtrip() {
        let mut env = make_envelope();
        env.repository_name = Some("my-repo".to_string());
        env.repository_url = Some("https://github.com/org/my-repo".to_string());
        env.head_commit = Some("abc123".to_string());
        env.risk_band = Some(RiskBand::High);
        env.findings.push(PluginFinding::new(
            "secret",
            "Hardcoded secret",
            "Found in src/config.rs",
            FindingSeverity::Critical,
            0.95,
        ));

        // SAFETY: ReportEnvelope is well-formed; serialization cannot fail.
        let json = env.to_json().unwrap();
        // SAFETY: we serialized the string ourselves.
        let restored = ReportEnvelope::load_from_json(&json).unwrap();

        assert_eq!(restored.report_id, env.report_id);
        assert_eq!(restored.plugin_name, env.plugin_name);
        assert_eq!(restored.workspace_id, env.workspace_id);
        assert_eq!(restored.repository_name, Some("my-repo".to_string()));
        assert_eq!(restored.head_commit, Some("abc123".to_string()));
        assert_eq!(restored.risk_band, Some(RiskBand::High));
        assert_eq!(restored.finding_count(), 1);
        assert_eq!(restored.findings.findings[0].kind, "secret");
    }

    #[test]
    fn test_report_envelope_load_from_json_invalid_returns_error() {
        let result = ReportEnvelope::load_from_json("this is not json");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, PipelineError::Report(_)));
    }

    // ------------------------------------------------------------------
    // finding_count
    // ------------------------------------------------------------------

    #[test]
    fn test_report_envelope_finding_count_zero_when_empty() {
        let env = make_envelope();
        assert_eq!(env.finding_count(), 0);
    }

    #[test]
    fn test_report_envelope_finding_count_reflects_pushed_findings() {
        let mut env = make_envelope();
        env.findings
            .push(PluginFinding::new("a", "t", "d", FindingSeverity::Low, 0.1));
        env.findings.push(PluginFinding::new(
            "b",
            "t",
            "d",
            FindingSeverity::High,
            0.8,
        ));
        assert_eq!(env.finding_count(), 2);
    }

    // ------------------------------------------------------------------
    // write_to_file
    // ------------------------------------------------------------------

    #[test]
    fn test_report_envelope_write_to_file_creates_file() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("report.json");

        let env = make_envelope();
        // SAFETY: writing to a freshly created temp directory should not fail.
        env.write_to_file(&path).unwrap();

        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("test_plugin"));
        assert!(content.contains("test-report-001"));
    }

    #[test]
    fn test_report_envelope_write_to_file_creates_parent_directories() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("nested").join("dirs").join("report.json");

        let env = make_envelope();
        env.write_to_file(&path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_report_envelope_write_to_file_content_is_valid_json() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp directory.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("report.json");

        let mut env = make_envelope();
        env.findings.push(PluginFinding::new(
            "vuln",
            "Vulnerability",
            "Found an issue.",
            FindingSeverity::High,
            0.8,
        ));
        env.write_to_file(&path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // SAFETY: we just wrote this file so it is valid JSON.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["plugin_name"], "test_plugin");
        assert_eq!(parsed["version"], "1");
    }
}
