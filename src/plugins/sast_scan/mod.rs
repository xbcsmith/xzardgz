//! SAST scan plugin.
//!
//! This module delivers the SAST scan pipeline as a [`WorkflowPlugin`] that
//! the pipeline runner can invoke through the plugin registry.
//!
//! # Submodules
//!
//! | Module    | Purpose                                               |
//! |-----------|-------------------------------------------------------|
//! | `config`  | [`SastScanConfig`] serialisable configuration struct  |
//! | `plugin`  | [`SastScanPlugin`] workflow plugin implementation     |
//! | `report`  | Markdown report builder                               |
//! | `scoring` | [`SastScanResult`] implementing `ScoringInput`        |
//!
//! [`WorkflowPlugin`]: crate::plugins::trait_def::WorkflowPlugin
//! [`SastScanConfig`]: config::SastScanConfig
//! [`SastScanPlugin`]: plugin::SastScanPlugin
//! [`SastScanResult`]: scoring::SastScanResult

pub mod config;
pub mod plugin;
pub mod report;
pub mod scoring;

pub use plugin::SastScanPlugin;
