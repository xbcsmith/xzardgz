//! Plugin runtime for the XZardgz pipeline.
//!
//! This module provides the core plugin infrastructure used to register,
//! look up, and execute named analysis plugins. Plugins receive a
//! [`PluginContext`] containing all pipeline resources and return a
//! [`PluginOutput`] describing the results of their analysis.
//!
//! # Architecture
//!
//! The plugin system is composed of four submodules:
//!
//! - [`context`] - Runtime context and tool access level for plugin execution.
//! - [`output`] - Output and token usage types returned by plugin runs.
//! - [`registry`] - Registry for discovering and dispatching plugins by name.
//! - [`trait_def`] - Core [`WorkflowPlugin`] trait and plugin metadata.
//!
//! # Typical Usage
//!
//! ```
//! use xzardgz::plugins::registry::PluginRegistry;
//! use xzardgz::plugins::output::PluginOutput;
//! use xzardgz::plugins::trait_def::PluginMetadata;
//!
//! let registry = PluginRegistry::new();
//! assert_eq!(registry.plugin_count(), 0);
//!
//! let output = PluginOutput::success("analysis complete");
//! assert!(output.completed);
//!
//! let meta = PluginMetadata::new("my_plugin", "1.0.0", "My plugin.");
//! assert_eq!(meta.name, "my_plugin");
//! ```

pub mod context;
pub mod output;
pub mod registry;
pub mod trait_def;

pub use context::{PluginContext, ToolAccessLevel};
pub use output::{PluginOutput, TokenUsage};
pub use registry::PluginRegistry;
pub use trait_def::{PluginMetadata, WorkflowPlugin};
