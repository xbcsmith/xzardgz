//! Workflow subsystem for the XZardgz pipeline.
//!
//! This module contains the plugin-first workflow plan model (schema version 1),
//! plan parsing, structural validation, and the shared step execution engine.
//!
//! # Key types
//!
//! - [`executor::WorkflowExecutor`] — shared execution engine for CLI and watcher
//! - [`executor::ExecutionInput`] — describes the execution mode
//! - [`executor::ExecutionResult`] — structured result from every execution run
//! - [`plan::WorkflowPlan`] — the versioned plugin-first plan model

pub mod executor;
pub mod parser;
pub mod plan;
pub mod validator;

pub use executor::{ExecutionInput, ExecutionResult, WorkflowExecutor};
