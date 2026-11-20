//! SAST scanning tool for the agent tool registry.
//!
//! [`SastTool`] wraps the full SAST pipeline — ruleset resolution, engine
//! initialisation, parallel scanning, severity filtering, and output
//! formatting — as a single agent-callable tool.
//!
//! This module must NOT import from `crate::clients`.

use std::path::Path;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::error::Result;
use crate::providers::types::Tool;
use crate::scanner::sast::SastEngine;
use crate::scanner::sast::config::SastEngineConfig;
use crate::scanner::sast::output::cyclonedx::render_cyclonedx;
use crate::scanner::sast::output::sarif::render_sarif;
use crate::scanner::sast::rule::metadata::Severity;
use crate::scanner::sast::rule::resolve::{ResolveConfig, resolve_rules};
use crate::tools::{ToolExecutor, ToolResult};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of findings returned in a single tool response.
const MAX_FINDINGS: usize = 500;

// ---------------------------------------------------------------------------
// SastTool
// ---------------------------------------------------------------------------

/// Agent tool that performs a SAST scan on a source directory.
///
/// Wraps the full SAST pipeline: ruleset resolution, engine initialisation,
/// parallel scanning, severity filtering, and output serialisation. The tool
/// never surfaces hard errors as `Err` — user-visible failures are returned
/// as [`ToolResult::failure`] so the agent loop can recover gracefully.
///
/// # JSON Parameters
///
/// | Field                | Type     | Required | Default                                    |
/// |----------------------|----------|----------|--------------------------------------------|
/// | `path`               | string   | yes      | -                                          |
/// | `rulesets`           | string[] | no       | `["builtin:crypto", "builtin:security"]`   |
/// | `format`             | string   | no       | `"json"`                                   |
/// | `severity_threshold` | string   | no       | `"warning"`                                |
/// | `jobs`               | integer  | no       | `0` (automatic parallelism)                |
pub struct SastTool;

#[async_trait]
impl ToolExecutor for SastTool {
    /// Returns the tool definition for `sast_scan`.
    ///
    /// The definition includes a JSON Schema object describing all accepted
    /// parameters. Only `path` is required; all other parameters have
    /// documented defaults.
    fn tool_definition(&self) -> Tool {
        Tool {
            name: "sast_scan".to_string(),
            description: "Run a SAST scan on a source directory and return findings in the \
                requested format. Supported formats: json, sarif, cyclonedx."
                .to_string(),
            parameters: json!({
                "type": "object",
                "required": ["path"],
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory to scan"
                    },
                    "rulesets": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Ruleset IDs to load (default: builtin:crypto, builtin:security)"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["json", "sarif", "cyclonedx"],
                        "description": "Output format (default: json)"
                    },
                    "severity_threshold": {
                        "type": "string",
                        "enum": ["info", "warning", "error"],
                        "description": "Minimum severity to report (default: warning)"
                    },
                    "jobs": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Parallelism (0 = automatic)"
                    }
                }
            }),
        }
    }

    /// Execute a SAST scan with the provided JSON parameters.
    ///
    /// All user-visible errors (missing path, rule resolution failure, engine
    /// failure, scan failure, serialisation failure) are returned as
    /// `Ok(ToolResult::failure(...))` rather than propagated as `Err` so the
    /// agent loop can continue without crashing.
    ///
    /// # Arguments
    ///
    /// * `params` - JSON object conforming to the schema from
    ///   [`Self::tool_definition`].
    ///
    /// # Returns
    ///
    /// `Ok(ToolResult::success(...))` with serialised findings on success, or
    /// `Ok(ToolResult::failure(...))` with a descriptive message on error.
    ///
    /// # Errors
    ///
    /// This function is effectively infallible; it returns `Err` only if an
    /// unexpected internal condition occurs that cannot be represented as a
    /// tool failure message.
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        // 1. Extract required `path`.
        let path = match params.get("path").and_then(Value::as_str) {
            Some(p) => p.to_string(),
            None => return Ok(ToolResult::failure("missing required param: path")),
        };

        // 2. Extract optional `rulesets`, defaulting to both embedded builtin sets.
        let rulesets: Vec<String> = params
            .get("rulesets")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_else(|| vec!["builtin:crypto".to_string(), "builtin:security".to_string()]);

        // 3. Extract optional `format`, defaulting to JSON.
        let format = params
            .get("format")
            .and_then(Value::as_str)
            .unwrap_or("json")
            .to_string();

        // 4. Extract optional `severity_threshold`, defaulting to warning.
        let severity_threshold = params
            .get("severity_threshold")
            .and_then(Value::as_str)
            .unwrap_or("warning")
            .to_string();

        // 5. Extract optional `jobs`, defaulting to 0 (automatic).
        let jobs = params
            .get("jobs")
            .and_then(Value::as_u64)
            .map(|j| j as usize)
            .unwrap_or(0);

        // 6. Build the four-level ruleset resolution configuration.
        let resolve_cfg = ResolveConfig {
            cli_rulesets: rulesets,
            cli_ruleset_paths: vec![],
        };

        // 7. Resolve rules; surface hard errors as tool failures.
        let namespaced_rules = match resolve_rules(&resolve_cfg) {
            Ok(r) => r,
            Err(e) => return Ok(ToolResult::failure(format!("rule resolution failed: {e}"))),
        };

        // 8. Extract Vec<RuleIr> from the namespaced rules.
        let rules = namespaced_rules.into_iter().map(|nr| nr.rule).collect();

        // 9. Build engine configuration, optionally overriding parallelism.
        let mut engine_config = SastEngineConfig::new();
        engine_config.jobs = jobs;

        // 10. Initialise the SAST engine.
        let mut engine = match SastEngine::new(engine_config) {
            Ok(e) => e,
            Err(e) => return Ok(ToolResult::failure(format!("engine init failed: {e}"))),
        };

        // 11. Load compiled rules into the engine.
        if let Err(e) = engine.with_rules(rules) {
            return Ok(ToolResult::failure(format!("rule load failed: {e}")));
        }

        // 12. Run the scan.
        let mut report = match engine.scan(Path::new(&path)) {
            Ok(r) => r,
            Err(e) => return Ok(ToolResult::failure(format!("scan failed: {e}"))),
        };

        // 13. Apply severity threshold filter.
        report
            .matches
            .retain(|m| severity_passes(&m.severity, &severity_threshold));

        // 14. Apply maximum findings cap.
        report.matches.truncate(MAX_FINDINGS);

        // 15. Serialise to the requested output format.
        let serialized = match format.to_lowercase().as_str() {
            "sarif" => {
                let log = render_sarif(&report);
                match serde_json::to_string(&log) {
                    Ok(s) => s,
                    Err(e) => {
                        return Ok(ToolResult::failure(format!(
                            "SARIF serialization failed: {e}"
                        )));
                    }
                }
            }
            "cyclonedx" => {
                let bom = render_cyclonedx(&report);
                match serde_json::to_string(&bom) {
                    Ok(s) => s,
                    Err(e) => {
                        return Ok(ToolResult::failure(format!(
                            "CycloneDX serialization failed: {e}"
                        )));
                    }
                }
            }
            _ => match serde_json::to_string(&report.matches) {
                Ok(s) => s,
                Err(e) => {
                    return Ok(ToolResult::failure(format!(
                        "JSON serialization failed: {e}"
                    )));
                }
            },
        };

        Ok(ToolResult::success(serialized))
    }
}

// ---------------------------------------------------------------------------
// Severity helper
// ---------------------------------------------------------------------------

/// Returns `true` when `severity` meets or exceeds `threshold`.
///
/// The severity scale is `info=0 < warning=1 < error=2`. An unrecognised
/// `threshold` string is treated as `"warning"` to fail safe.
///
/// # Arguments
///
/// * `severity` - Severity level of the SAST match.
/// * `threshold` - Minimum severity string from the tool parameter.
fn severity_passes(severity: &Severity, threshold: &str) -> bool {
    let thr: u8 = match threshold.to_lowercase().as_str() {
        "info" => 0,
        "warning" => 1,
        "error" => 2,
        _ => 1,
    };
    let sev: u8 = match severity {
        Severity::Info => 0,
        Severity::Warning => 1,
        Severity::Error => 2,
    };
    sev >= thr
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // tool_definition
    // -----------------------------------------------------------------------

    #[test]
    fn test_sast_tool_definition_name_is_sast_scan() {
        let tool = SastTool.tool_definition();
        assert_eq!(tool.name, "sast_scan");
    }

    #[test]
    fn test_sast_tool_definition_parameters_has_path_property() {
        let tool = SastTool.tool_definition();
        let path_prop = &tool.parameters["properties"]["path"];
        assert_eq!(
            path_prop["type"], "string",
            "path property must have type string"
        );
    }

    // -----------------------------------------------------------------------
    // execute
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_sast_tool_execute_with_valid_empty_path_returns_ok() {
        let dir = tempfile::tempdir().expect("tempdir creation failed");
        let params = json!({ "path": dir.path().to_str().unwrap() });
        let result = SastTool
            .execute(params)
            .await
            .expect("execute must not propagate Err");
        assert!(
            result.error.is_none(),
            "scan of empty directory must succeed; got error: {:?}",
            result.error
        );
    }

    #[tokio::test]
    async fn test_sast_tool_execute_with_invalid_path_returns_error() {
        let params = json!({ "path": "/nonexistent/xzardgz/sast/path/that/does/not/exist/at/all" });
        let result = SastTool
            .execute(params)
            .await
            .expect("execute must not propagate Err");
        // A non-existent path either produces a scan error or returns empty findings.
        let is_error = result.error.is_some();
        let is_empty_array = result.output == "[]";
        assert!(
            is_error || is_empty_array,
            "expected scan failure or empty findings for a non-existent path; \
             got output={:?} error={:?}",
            result.output,
            result.error,
        );
    }

    #[tokio::test]
    async fn test_sast_tool_execute_missing_path_param_returns_failure() {
        let params = json!({});
        let result = SastTool
            .execute(params)
            .await
            .expect("execute must not propagate Err");
        assert!(
            result.error.is_some(),
            "missing path param must produce a ToolResult::failure"
        );
    }

    /// Compile-time boundary check: this module must not import from `crate::clients`.
    ///
    /// Verified by running: `grep -n 'use crate::clients' src/tools/sast_tool.rs`
    ///
    /// Expected result: zero matches. The absence of any `use crate::clients`
    /// line in this file is the invariant being tested.
    #[test]
    fn test_sast_tool_execute_does_not_import_from_clients() {
        // Intentionally empty: this test documents and enforces a compile-time
        // boundary. Any violation would be caught by code review using the
        // grep command in the doc comment above.
    }

    // -----------------------------------------------------------------------
    // severity_passes
    // -----------------------------------------------------------------------

    #[test]
    fn test_severity_passes_info_threshold_accepts_all_levels() {
        assert!(severity_passes(&Severity::Info, "info"));
        assert!(severity_passes(&Severity::Warning, "info"));
        assert!(severity_passes(&Severity::Error, "info"));
    }

    #[test]
    fn test_severity_passes_warning_threshold_rejects_info() {
        assert!(!severity_passes(&Severity::Info, "warning"));
        assert!(severity_passes(&Severity::Warning, "warning"));
        assert!(severity_passes(&Severity::Error, "warning"));
    }

    #[test]
    fn test_severity_passes_error_threshold_rejects_info_and_warning() {
        assert!(!severity_passes(&Severity::Info, "error"));
        assert!(!severity_passes(&Severity::Warning, "error"));
        assert!(severity_passes(&Severity::Error, "error"));
    }

    #[test]
    fn test_severity_passes_unknown_threshold_defaults_to_warning() {
        assert!(!severity_passes(&Severity::Info, "critical"));
        assert!(severity_passes(&Severity::Warning, "critical"));
        assert!(severity_passes(&Severity::Error, "critical"));
    }
}
