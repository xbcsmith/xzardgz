//! SAST scan plugin module.
//!
//! This module contains the configuration type for the SAST scan pipeline
//! (`xzardgz sast scan`). The full plugin implementation — including
//! `ToolExecutor` integration and `security_review` wiring — is delivered
//! in Phase 7.

pub mod config;
