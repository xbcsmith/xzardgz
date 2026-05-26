//! Plugin report generation for the XZardgz pipeline.
//!
//! This module provides the end-to-end infrastructure for producing structured
//! analysis reports from plugin runs. A typical plugin workflow is:
//!
//! 1. Create a [`ReportEnvelope`] via [`ReportEnvelope::new`].
//! 2. Push [`PluginFinding`]s into `envelope.findings`.
//! 3. Optionally derive and set `envelope.risk_band` from `envelope.findings`.
//! 4. Select a [`PluginReportFormatter`] and call its `write` method.
//!
//! # Output formats
//!
//! | Format   | Writer                  | Extension        |
//! |----------|-------------------------|------------------|
//! | Markdown | [`MarkdownReportWriter`] | `.md`           |
//! | JSON     | [`JsonReportWriter`]     | `.json`         |
//! | SARIF    | [`SarifReportWriter`]    | `.sarif.json`   |
//!
//! # Risk classification
//!
//! [`RiskBand`] is a four-tier enum (`Low`, `Medium`, `High`, `Critical`)
//! used to summarize the overall risk exposure of a report.
//! [`RiskBand::from_confidence`] derives a band from an AI confidence score,
//! and [`PluginFindings::to_risk_band`] derives one from the highest-severity
//! finding in the collection.
//!
//! # Path validation
//!
//! All writers call [`formatter::validate_report_path`] before touching the
//! file system. Call it directly when you need to validate a path outside a
//! formatter.

pub mod envelope;
pub mod findings;
pub mod formatter;
pub mod json;
pub mod markdown;
pub mod risk_band;
pub mod sarif;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use envelope::{REPORT_ENVELOPE_VERSION, ReportEnvelope};
pub use findings::{PluginFinding, PluginFindings};
pub use formatter::{PluginReportFormatter, ReportFormat, validate_report_path};
pub use json::JsonReportWriter;
pub use markdown::MarkdownReportWriter;
pub use risk_band::RiskBand;
pub use sarif::SarifReportWriter;
