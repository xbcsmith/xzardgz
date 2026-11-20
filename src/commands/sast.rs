//! SAST security scan command handler.
//!
//! Implements the `xzardgz sast scan` subcommand. The entry point is
//! [`execute`], which loads configuration and delegates to [`execute_with`].
//! Use [`execute_with`] directly in tests or programmatic callers that supply
//! a pre-built [`Config`].
//!
//! # Execution flow
//!
//! 1. Merge CLI rulesets with project configuration to build a [`ResolveConfig`].
//! 2. Resolve rules via [`resolve_rules`] (embedded and/or filesystem sources).
//! 3. Configure and initialise [`SastEngine`].
//! 4. Run `engine.scan(root)`.
//! 5. Post-process matches: apply the namespace from each [`NamespacedRule`].
//! 6. Filter by severity threshold, then cap at `max_findings`.
//! 7. Serialise and write output (stdout or file).
//! 8. Log a one-line summary via `tracing::info!`.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::Path;

use crate::cli::{SastCommands, SastScanArgs};
use crate::config::Config;
use crate::error::{PipelineError, Result};
use crate::scanner::sast::config::SastEngineConfig;
use crate::scanner::sast::output::cyclonedx::render_cyclonedx;
use crate::scanner::sast::output::sarif::render_sarif;
use crate::scanner::sast::rule::metadata::Severity;
use crate::scanner::sast::rule::resolve::{ResolveConfig, resolve_rules};
use crate::scanner::sast::{SastEngine, SastScanReport};

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Execute the SAST command group.
///
/// Loads [`Config`] from the default path (`config.yaml`) and dispatches to
/// the appropriate subcommand handler. Use [`execute_with`] when you need to
/// inject a configuration for testing or programmatic use.
///
/// # Arguments
///
/// * `command` - SAST subcommand variant parsed from the CLI.
///
/// # Errors
///
/// Returns [`PipelineError::Config`] if the configuration file exists but
/// cannot be parsed. See [`execute_with`] for further error conditions.
///
/// # Examples
///
/// ```no_run
/// use xzardgz::cli::{SastCommands, SastScanArgs};
/// use xzardgz::commands::sast::execute;
///
/// # #[tokio::main]
/// # async fn main() {
/// let args = SastScanArgs {
///     path: std::path::PathBuf::from("."),
///     ruleset: vec![],
///     ruleset_path: vec![],
///     format: "json".to_string(),
///     output: None,
///     jobs: 0,
///     severity: "warning".to_string(),
/// };
/// // execute(SastCommands::Scan(args)).await.unwrap();
/// # }
/// ```
pub async fn execute(command: SastCommands) -> Result<()> {
    let config = Config::load()?;
    match command {
        SastCommands::Scan(args) => execute_with(args, config),
    }
}

/// Execute the SAST scan subcommand with an explicit configuration.
///
/// This is the testable core of the `sast scan` subcommand. It performs all
/// scanning work without touching the filesystem for configuration.
///
/// # Arguments
///
/// * `args` - Parsed CLI arguments for the `sast scan` subcommand.
/// * `config` - Fully-loaded pipeline configuration.
///
/// # Errors
///
/// Returns [`PipelineError::Scanner`] if rule resolution or the SAST engine
/// encounters a hard error. Returns [`PipelineError::Io`] if the output file
/// cannot be written. Returns [`PipelineError::Sast`] if output serialisation
/// fails.
///
/// # Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use xzardgz::cli::SastScanArgs;
/// use xzardgz::commands::sast::execute_with;
/// use xzardgz::config::Config;
///
/// let args = SastScanArgs {
///     path: PathBuf::from("."),
///     ruleset: vec!["builtin:crypto".to_string()],
///     ruleset_path: vec![],
///     format: "json".to_string(),
///     output: None,
///     jobs: 0,
///     severity: "warning".to_string(),
/// };
/// let config = Config::default();
/// // execute_with(args, config).unwrap();
/// ```
pub fn execute_with(args: SastScanArgs, config: Config) -> Result<()> {
    // 1. Build ResolveConfig: CLI args take priority over config file.
    let cli_rulesets = if !args.ruleset.is_empty() {
        args.ruleset.clone()
    } else if !config.sast_scan.rulesets.is_empty() {
        config.sast_scan.rulesets.clone()
    } else {
        // resolve_rules will fall back to the embedded defaults.
        Vec::new()
    };

    let resolve_config = ResolveConfig {
        cli_rulesets,
        cli_ruleset_paths: args.ruleset_path.clone(),
    };

    // 2. Resolve rules from the four-level search path.
    let namespaced_rules = resolve_rules(&resolve_config)?;

    // 3. Build a lookup table: base rule id -> (namespaced id, ruleset id).
    //    Used after scanning to populate ruleset_id and rule_id on each match.
    let rule_namespace: HashMap<String, (String, String)> = namespaced_rules
        .iter()
        .map(|nr| {
            (
                nr.rule.id.clone(),
                (nr.namespaced_id.clone(), nr.ruleset_id.clone()),
            )
        })
        .collect();

    // 4. Build engine configuration; allow CLI --jobs to override config.
    let mut engine_config: SastEngineConfig = config.sast.clone();
    if args.jobs > 0 {
        engine_config.jobs = args.jobs;
    }

    // 5. Initialise the engine.
    let mut engine = SastEngine::new(engine_config)?;

    // 6. Load compiled rules into the engine.
    let rules = namespaced_rules.into_iter().map(|nr| nr.rule).collect();
    engine.with_rules(rules)?;

    // 7. Run the scan.
    let mut report = engine.scan(&args.path)?;

    // 8. Post-process: stamp each match with its fully-qualified rule/ruleset id.
    for m in &mut report.matches {
        if let Some((ns_id, rs_id)) = rule_namespace.get(&m.rule_id) {
            m.rule_id = ns_id.clone();
            m.ruleset_id = rs_id.clone();
        }
    }

    // 9. Apply severity threshold filter.
    report
        .matches
        .retain(|m| severity_passes_threshold(&m.severity, &args.severity));

    // 10. Cap total findings at the configured maximum.
    report.matches.truncate(config.sast_scan.max_findings);

    // 11. Serialise and write output.
    write_output(&report, &args.format, args.output.as_deref())?;

    // 12. Emit a summary line to the structured log.
    tracing::info!(
        findings = report.matches.len(),
        scanned_files = report.scanned_file_count,
        skipped_files = report.skipped_file_count,
        duration_ms = report.duration_ms,
        "SAST scan complete"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Severity filtering
// ---------------------------------------------------------------------------

/// Returns `true` when `match_severity` meets or exceeds `threshold`.
///
/// The severity scale used is `info < warning < error`.  An unrecognised
/// `threshold` string is treated as `"warning"` to fail safe.
///
/// # Arguments
///
/// * `match_severity` - The severity level reported by the matched rule.
/// * `threshold` - Minimum severity string from `--severity` or config.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::rule::metadata::Severity;
/// use xzardgz::commands::sast::severity_passes_threshold;
///
/// assert!(severity_passes_threshold(&Severity::Error, "info"));
/// assert!(!severity_passes_threshold(&Severity::Info, "warning"));
/// ```
pub fn severity_passes_threshold(match_severity: &Severity, threshold: &str) -> bool {
    // Numeric levels: info=0, warning=1, error=2.
    let threshold_level: u8 = match threshold.to_lowercase().as_str() {
        "info" => 0,
        "warning" | "warn" => 1,
        "error" | "err" => 2,
        _ => 1, // unknown threshold defaults to warning
    };
    let match_level: u8 = match match_severity {
        Severity::Info => 0,
        Severity::Warning => 1,
        Severity::Error => 2,
    };
    match_level >= threshold_level
}

// ---------------------------------------------------------------------------
// Output projection
// ---------------------------------------------------------------------------

/// Serialise `report` in `format` and write to `output` (or stdout).
///
/// Supported formats:
///
/// | `format`    | Description                                       |
/// |-------------|---------------------------------------------------|
/// | `"sarif"`   | SARIF 2.1.0 JSON via [`render_sarif`]             |
/// | `"cyclonedx"` | CycloneDX 1.7 BOM JSON via [`render_cyclonedx`] |
/// | `"json"` (default) | JSON array of [`SastMatch`] entries          |
///
/// # Arguments
///
/// * `report` - The completed scan report.
/// * `format` - Output format selector string (case-insensitive).
/// * `output` - Optional file path; writes to stdout when `None`.
///
/// # Errors
///
/// Returns [`PipelineError::Sast`] if JSON serialisation fails.
/// Returns [`PipelineError::Io`] if file or stdout write fails.
///
/// [`SastMatch`]: crate::scanner::sast::match_model::SastMatch
pub fn write_output(report: &SastScanReport, format: &str, output: Option<&Path>) -> Result<()> {
    let content = match format.to_lowercase().as_str() {
        "sarif" => {
            let log = render_sarif(report);
            serde_json::to_string_pretty(&log)
                .map_err(|e| PipelineError::Sast(format!("SARIF serialisation failed: {e}")))?
        }
        "cyclonedx" => {
            let bom = render_cyclonedx(report);
            serde_json::to_string_pretty(&bom)
                .map_err(|e| PipelineError::Sast(format!("CycloneDX serialisation failed: {e}")))?
        }
        _ => {
            // Default: JSON array of SastMatch entries.
            serde_json::to_string_pretty(&report.matches)
                .map_err(|e| PipelineError::Sast(format!("JSON serialisation failed: {e}")))?
        }
    };

    if let Some(path) = output {
        std::fs::write(path, content.as_bytes())?;
    } else {
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        writeln!(handle, "{content}")?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::sast::rule::metadata::Severity;

    // ------------------------------------------------------------------
    // severity_passes_threshold
    // ------------------------------------------------------------------

    #[test]
    fn test_severity_passes_threshold_info_passes_all_levels() {
        assert!(
            severity_passes_threshold(&Severity::Info, "info"),
            "info severity must pass the info threshold"
        );
        assert!(
            severity_passes_threshold(&Severity::Warning, "info"),
            "warning severity must pass the info threshold"
        );
        assert!(
            severity_passes_threshold(&Severity::Error, "info"),
            "error severity must pass the info threshold"
        );
    }

    #[test]
    fn test_severity_passes_threshold_warning_blocks_info() {
        assert!(
            !severity_passes_threshold(&Severity::Info, "warning"),
            "info severity must not pass the warning threshold"
        );
        assert!(
            severity_passes_threshold(&Severity::Warning, "warning"),
            "warning severity must pass the warning threshold"
        );
        assert!(
            severity_passes_threshold(&Severity::Error, "warning"),
            "error severity must pass the warning threshold"
        );
    }

    #[test]
    fn test_severity_passes_threshold_error_blocks_warning() {
        assert!(
            !severity_passes_threshold(&Severity::Info, "error"),
            "info severity must not pass the error threshold"
        );
        assert!(
            !severity_passes_threshold(&Severity::Warning, "error"),
            "warning severity must not pass the error threshold"
        );
        assert!(
            severity_passes_threshold(&Severity::Error, "error"),
            "error severity must pass the error threshold"
        );
    }

    #[test]
    fn test_severity_passes_threshold_unknown_threshold_defaults_to_warning() {
        // Unknown threshold must behave like "warning".
        assert!(
            !severity_passes_threshold(&Severity::Info, "critical"),
            "info must not pass an unknown threshold (defaults to warning)"
        );
        assert!(
            severity_passes_threshold(&Severity::Warning, "critical"),
            "warning must pass an unknown threshold (defaults to warning)"
        );
        assert!(
            severity_passes_threshold(&Severity::Error, "critical"),
            "error must pass an unknown threshold (defaults to warning)"
        );
    }

    // ------------------------------------------------------------------
    // execute_with integration
    // ------------------------------------------------------------------

    #[test]
    fn test_execute_sast_scan_on_empty_directory_returns_ok() {
        let dir = tempfile::tempdir()
            // SAFETY: tempdir creation in test environment; failure is unrecoverable test setup.
            .expect("tempdir creation failed");
        let output_file = dir.path().join("output.json");
        let args = SastScanArgs {
            path: dir.path().to_path_buf(),
            ruleset: vec![],
            ruleset_path: vec![],
            format: "json".to_string(),
            output: Some(output_file.clone()),
            jobs: 0,
            severity: "warning".to_string(),
        };
        let config = crate::config::Config::default();
        let result = execute_with(args, config);
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        assert!(
            output_file.exists(),
            "output file must be written even for an empty directory"
        );
    }
}
