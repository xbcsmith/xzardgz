//! Technical review plugin for XZardgz.
//!
//! This module implements the built-in `technical-review` plugin, which
//! evaluates codebase architecture, quality, and operational readiness across
//! 14 review dimensions by combining static scan data with AI-assisted analysis.
//!
//! # Architecture
//!
//! The plugin is composed of six submodules:
//!
//! - [`config`] - Configuration validation for the plugin.
//! - [`dimensions`] - The 14 [`ReviewDimension`] variants that frame analysis.
//! - [`finding`] - The [`TechnicalReviewFinding`] data type.
//! - [`plugin`] - The [`TechnicalReviewPlugin`] struct and [`WorkflowPlugin`] impl.
//! - [`prioritizer`] - File selection and ranking logic.
//! - [`report`] - Markdown and JSON report renderers.
//!
//! # Typical Usage
//!
//! ```no_run
//! use xzardgz::plugins::technical_review::TechnicalReviewPlugin;
//! use xzardgz::plugins::trait_def::WorkflowPlugin;
//!
//! let plugin = TechnicalReviewPlugin;
//! assert_eq!(plugin.name(), "technical-review");
//! ```

pub mod config;
pub mod dimensions;
pub mod finding;
pub mod plugin;
pub mod prioritizer;
pub mod report;

pub use config::validate_technical_review_config;
pub use dimensions::ReviewDimension;
pub use finding::TechnicalReviewFinding;
pub use plugin::TechnicalReviewPlugin;
pub use prioritizer::{FilePrioritizer, PrioritizedFiles};
pub use report::{TechnicalReviewJsonReport, TechnicalReviewMarkdownReport};
