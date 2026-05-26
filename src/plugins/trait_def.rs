//! Core plugin trait and metadata definitions.
//!
//! [`WorkflowPlugin`] is the trait every analysis plugin must implement.
//! [`PluginMetadata`] describes a plugin's identity and schema.
//!
//! # Object Safety
//!
//! [`WorkflowPlugin`] is object-safe. Use `Arc<dyn WorkflowPlugin>` to store
//! plugins in the registry.
//!
//! # Mocking in Tests
//!
//! When compiled with `cfg(test)`, `mockall::automock` generates a
//! `MockWorkflowPlugin` type that implements the trait. Use it for unit tests
//! that need to stub out plugin behaviour without running real analysis.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::plugins::context::{PluginContext, ToolAccessLevel};
use crate::plugins::output::PluginOutput;

// ---------------------------------------------------------------------------
// PluginMetadata
// ---------------------------------------------------------------------------

/// Metadata describing a workflow plugin.
///
/// Returned by [`WorkflowPlugin::metadata`] and stored in the registry for
/// introspection and configuration validation.
///
/// # Examples
///
/// ```
/// use xzardgz::plugins::trait_def::PluginMetadata;
///
/// let meta = PluginMetadata::new("technical_review", "1.0.0", "Performs technical review.");
/// assert_eq!(meta.name, "technical_review");
/// assert_eq!(meta.version, "1.0.0");
/// assert!(meta.config_schema.is_none());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin name (must be unique in the registry).
    pub name: String,
    /// Plugin version string.
    pub version: String,
    /// Human-readable description of the plugin's purpose.
    pub description: String,
    /// Optional JSON Schema describing the plugin's configuration shape.
    pub config_schema: Option<serde_json::Value>,
}

impl PluginMetadata {
    /// Creates a new `PluginMetadata` with no config schema.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique plugin name (e.g. `"technical_review"`).
    /// * `version` - Semantic version string (e.g. `"1.0.0"`).
    /// * `description` - Human-readable description.
    ///
    /// # Returns
    ///
    /// A new `PluginMetadata` with `config_schema = None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::trait_def::PluginMetadata;
    ///
    /// let meta = PluginMetadata::new("security_review", "0.2.1", "Security analysis plugin.");
    /// assert_eq!(meta.name, "security_review");
    /// assert_eq!(meta.version, "0.2.1");
    /// assert_eq!(meta.description, "Security analysis plugin.");
    /// assert!(meta.config_schema.is_none());
    /// ```
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: description.into(),
            config_schema: None,
        }
    }
}

// ---------------------------------------------------------------------------
// WorkflowPlugin
// ---------------------------------------------------------------------------

/// The core plugin abstraction.
///
/// All analysis plugins implement this trait. A plugin receives a
/// [`PluginContext`] containing all pipeline resources and returns a
/// [`PluginOutput`] describing the results of its analysis.
///
/// # Object Safety
///
/// The trait is object-safe. Use `Arc<dyn WorkflowPlugin>` to store plugins
/// in the registry.
///
/// # Error Handling
///
/// Return `Err(PipelineError::Plugin(_))` for unrecoverable plugin failures.
/// Use [`PluginOutput::failure`][crate::plugins::output::PluginOutput::failure]
/// for graceful failures that should be reported but not abort the pipeline.
///
/// # Mocking
///
/// When compiled with `cfg(test)`, `mockall::automock` generates a
/// `MockWorkflowPlugin` struct. Configure expectations via the standard
/// `mockall` API.
///
/// # Examples
///
/// ```no_run
/// # use async_trait::async_trait;
/// # use xzardgz::plugins::context::{PluginContext, ToolAccessLevel};
/// # use xzardgz::plugins::output::PluginOutput;
/// # use xzardgz::plugins::trait_def::{PluginMetadata, WorkflowPlugin};
/// # use xzardgz::error::Result;
///
/// struct MyPlugin;
///
/// #[async_trait]
/// impl WorkflowPlugin for MyPlugin {
///     fn name(&self) -> &str { "my_plugin" }
///
///     fn metadata(&self) -> PluginMetadata {
///         PluginMetadata::new("my_plugin", "1.0.0", "My custom plugin.")
///     }
///
///     fn supported_formats(&self) -> Vec<String> {
///         vec!["markdown".to_string()]
///     }
///
///     fn required_tool_access(&self) -> ToolAccessLevel {
///         ToolAccessLevel::ReadOnly
///     }
///
///     async fn run(&self, _ctx: PluginContext) -> Result<PluginOutput> {
///         Ok(PluginOutput::success("my_plugin complete"))
///     }
/// }
/// ```
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait WorkflowPlugin: Send + Sync {
    /// Returns the canonical name of this plugin (e.g. `"technical_review"`).
    ///
    /// Must match the name returned by [`metadata`][Self::metadata] and must
    /// be unique within a [`PluginRegistry`][crate::plugins::registry::PluginRegistry].
    fn name(&self) -> &str;

    /// Returns metadata describing this plugin.
    ///
    /// # Returns
    ///
    /// A [`PluginMetadata`] containing the plugin name, version, description,
    /// and optional JSON Schema.
    fn metadata(&self) -> PluginMetadata;

    /// Returns the report formats this plugin supports.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` of format labels (e.g. `["markdown", "json", "sarif"]`).
    fn supported_formats(&self) -> Vec<String>;

    /// Returns the tool access level this plugin requires.
    ///
    /// The plugin runner uses this to configure the filesystem sandbox before
    /// handing control to the plugin.
    ///
    /// # Returns
    ///
    /// A [`ToolAccessLevel`] variant.
    fn required_tool_access(&self) -> ToolAccessLevel;

    /// Executes the plugin with the given context.
    ///
    /// Receives the full [`PluginContext`] for this run and returns a
    /// [`PluginOutput`] describing the results. The output is persisted to the
    /// workspace state by the runner after this method returns.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Plugin`][crate::error::PipelineError::Plugin]
    /// for unrecoverable failures that the pipeline cannot resume from.
    /// Use [`PluginOutput::failure`][crate::plugins::output::PluginOutput::failure]
    /// for graceful, reportable failures.
    async fn run(&self, ctx: PluginContext) -> Result<PluginOutput>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // PluginMetadata
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_metadata_new_sets_required_fields() {
        let meta = PluginMetadata::new("my_plugin", "2.0.0", "Does something useful.");
        assert_eq!(meta.name, "my_plugin");
        assert_eq!(meta.version, "2.0.0");
        assert_eq!(meta.description, "Does something useful.");
        assert!(meta.config_schema.is_none());
    }

    #[test]
    fn test_plugin_metadata_config_schema_defaults_to_none() {
        let meta = PluginMetadata::new("x", "1", "desc");
        assert!(meta.config_schema.is_none());
    }

    #[test]
    fn test_plugin_metadata_clone_produces_equal_values() {
        let meta = PluginMetadata::new("a_plugin", "0.1.0", "A plugin.");
        let cloned = meta.clone();
        assert_eq!(cloned.name, meta.name);
        assert_eq!(cloned.version, meta.version);
        assert_eq!(cloned.description, meta.description);
    }

    #[test]
    fn test_plugin_metadata_with_schema_roundtrips_through_json() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "max_findings": { "type": "integer" }
            }
        });
        let meta = PluginMetadata {
            name: "plugin_with_schema".to_string(),
            version: "1.0.0".to_string(),
            description: "Plugin that has a schema.".to_string(),
            config_schema: Some(schema.clone()),
        };
        // SAFETY: PluginMetadata is known-valid; serialization cannot fail.
        let json = serde_json::to_string(&meta).unwrap();
        // SAFETY: we just serialized this data so it is valid JSON.
        let restored: PluginMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "plugin_with_schema");
        assert_eq!(restored.config_schema, Some(schema));
    }

    // ------------------------------------------------------------------
    // MockWorkflowPlugin (generated by mockall::automock)
    // ------------------------------------------------------------------

    #[test]
    fn test_mock_workflow_plugin_name_returns_configured_value() {
        let mut mock = MockWorkflowPlugin::new();
        mock.expect_name().return_const("mock_plugin".to_string());
        assert_eq!(mock.name(), "mock_plugin");
    }

    #[test]
    fn test_mock_workflow_plugin_supported_formats_returns_configured_list() {
        let mut mock = MockWorkflowPlugin::new();
        mock.expect_supported_formats()
            .returning(|| vec!["markdown".to_string(), "json".to_string()]);
        assert_eq!(mock.supported_formats(), vec!["markdown", "json"]);
    }

    #[test]
    fn test_mock_workflow_plugin_required_tool_access_returns_configured_level() {
        let mut mock = MockWorkflowPlugin::new();
        mock.expect_required_tool_access()
            .returning(|| ToolAccessLevel::ReadOnly);
        assert_eq!(mock.required_tool_access(), ToolAccessLevel::ReadOnly);
    }

    #[test]
    fn test_mock_workflow_plugin_metadata_returns_configured_metadata() {
        let mut mock = MockWorkflowPlugin::new();
        mock.expect_metadata()
            .returning(|| PluginMetadata::new("mock", "1.0.0", "A mock plugin."));
        let meta = mock.metadata();
        assert_eq!(meta.name, "mock");
        assert_eq!(meta.version, "1.0.0");
    }
}
