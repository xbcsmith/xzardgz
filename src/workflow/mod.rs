//! Workflow subsystem for the XZardgz pipeline.
//!
//! This module contains the plugin-first workflow plan model (schema version 1),
//! plan parsing, structural validation, and step execution.

pub mod executor;
pub mod parser;
pub mod plan;
pub mod validator;
