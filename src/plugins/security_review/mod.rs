//! Security review plugin for XZardgz.
//!
//! This module implements the built-in `security-review` plugin, which
//! performs AI-assisted security analysis of a repository and produces
//! Markdown, JSON, and SARIF 2.1.0 reports.
//!
//! # Architecture
//!
//! The plugin is composed of five submodules:
//!
//! - [`config`] - Configuration validation for the plugin.
//! - [`finding`] - The [`SecurityReviewFinding`] data type with CWE/OWASP fields.
//! - [`plugin`] - The [`SecurityReviewPlugin`] struct and [`WorkflowPlugin`] impl.
//! - [`report`] - Markdown, JSON, and SARIF report writers.
//! - [`scope`] - The 19 [`SecurityCategory`] variants and file prioritization.
//!
//! # Typical Usage
//!
//! ```no_run
//! use xzardgz::plugins::security_review::SecurityReviewPlugin;
//! use xzardgz::plugins::trait_def::WorkflowPlugin;
//!
//! let plugin = SecurityReviewPlugin;
//! assert_eq!(plugin.name(), "security-review");
//! ```

pub mod config;
pub mod finding;
pub mod plugin;
pub mod report;
pub mod scope;

pub use config::validate_security_review_config;
pub use finding::SecurityReviewFinding;
pub use plugin::SecurityReviewPlugin;
pub use report::{
    SecurityReviewJsonReport, SecurityReviewMarkdownReport, SecurityReviewSarifReport,
};
pub use scope::{SecurityCategory, SecurityFilePrioritizer};
