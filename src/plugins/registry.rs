//! Plugin registry for discovering and dispatching named workflow plugins.
//!
//! [`PluginRegistry`] manages registration, lookup, listing, and dispatch of
//! named plugins. [`PluginRegistry::new`] returns an empty registry (useful
//! for tests that register their own fakes); [`PluginRegistry::with_builtins`]
//! returns a registry with every built-in plugin already registered, and is
//! what CLI command handlers should use. Unknown and disabled plugins are
//! explicitly rejected with structured errors.
//!
//! # Usage
//!
//! ```
//! use std::sync::Arc;
//! use xzardgz::plugins::registry::PluginRegistry;
//!
//! let registry = PluginRegistry::new();
//! assert_eq!(registry.plugin_count(), 0);
//!
//! let with_builtins = PluginRegistry::with_builtins();
//! assert!(with_builtins.is_registered("technical-review"));
//! assert!(with_builtins.is_registered("security-review"));
//! ```

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::error::{PipelineError, Result};
use crate::plugins::trait_def::{PluginMetadata, WorkflowPlugin};

// ---------------------------------------------------------------------------
// PluginRegistry
// ---------------------------------------------------------------------------

/// Registry for workflow plugins.
///
/// Manages registration, lookup, listing, and dispatch of named plugins.
/// Built-in plugins are registered at construction. Unknown and disabled
/// plugins are explicitly rejected with structured [`PipelineError`] variants.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use xzardgz::plugins::registry::PluginRegistry;
///
/// let mut registry = PluginRegistry::new();
/// assert!(registry.is_empty_for_doc_test());
/// ```
pub struct PluginRegistry {
    plugins: HashMap<String, Arc<dyn WorkflowPlugin>>,
    disabled: HashSet<String>,
}

impl PluginRegistry {
    /// Creates a new, empty `PluginRegistry`.
    ///
    /// No plugins are registered by default. Use [`register`][Self::register]
    /// to add plugins.
    ///
    /// # Returns
    ///
    /// An empty registry with no registered or disabled plugins.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::registry::PluginRegistry;
    ///
    /// let registry = PluginRegistry::new();
    /// assert_eq!(registry.plugin_count(), 0);
    /// ```
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            disabled: HashSet::new(),
        }
    }

    /// Creates a new `PluginRegistry` with every built-in plugin registered.
    ///
    /// Registers [`TechnicalReviewPlugin`][crate::plugins::technical_review::TechnicalReviewPlugin]
    /// under `"technical-review"` and
    /// [`SecurityReviewPlugin`][crate::plugins::security_review::SecurityReviewPlugin]
    /// under `"security-review"`. This is the registry CLI command handlers
    /// use; [`PluginRegistry::new`] remains an empty registry, primarily
    /// useful for tests that register their own fake plugins.
    ///
    /// # Returns
    ///
    /// A registry with both built-in plugins registered under their
    /// canonical names.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::registry::PluginRegistry;
    ///
    /// let registry = PluginRegistry::with_builtins();
    /// assert!(registry.is_registered("technical-review"));
    /// assert!(registry.is_registered("security-review"));
    /// assert_eq!(registry.plugin_count(), 2);
    /// ```
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(
            crate::plugins::technical_review::TechnicalReviewPlugin,
        ));
        registry.register(Arc::new(
            crate::plugins::security_review::SecurityReviewPlugin,
        ));
        registry
    }

    /// Registers a plugin in the registry under `plugin.name()`.
    ///
    /// If a plugin with the same name is already registered, it is replaced.
    ///
    /// # Arguments
    ///
    /// * `plugin` - The plugin to register, wrapped in an `Arc`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use xzardgz::plugins::registry::PluginRegistry;
    /// // registry.register(Arc::new(MyPlugin));
    /// // assert!(registry.is_registered("my_plugin"));
    /// ```
    pub fn register(&mut self, plugin: Arc<dyn WorkflowPlugin>) {
        let name = plugin.name().to_string();
        self.plugins.insert(name, plugin);
    }

    /// Marks a plugin name as disabled.
    ///
    /// Subsequent calls to [`get`][Self::get] with this name will return
    /// `Err(PipelineError::Plugin("plugin '...' is disabled"))`.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the plugin to disable.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use xzardgz::plugins::registry::PluginRegistry;
    /// // registry.disable("legacy_plugin");
    /// // assert!(registry.is_disabled("legacy_plugin"));
    /// ```
    pub fn disable(&mut self, name: impl Into<String>) {
        self.disabled.insert(name.into());
    }

    /// Looks up a plugin by name and returns an `Arc` clone if found and enabled.
    ///
    /// # Arguments
    ///
    /// * `name` - The canonical plugin name to look up.
    ///
    /// # Returns
    ///
    /// `Ok(Arc<dyn WorkflowPlugin>)` when the plugin exists and is not disabled.
    ///
    /// # Errors
    ///
    /// - Returns `Err(PipelineError::Plugin("plugin '{name}' is disabled"))` if
    ///   the plugin name is in the disabled set.
    /// - Returns `Err(PipelineError::PluginNotFound { name })` if the plugin is
    ///   not registered.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use xzardgz::plugins::registry::PluginRegistry;
    /// // let plugin = registry.get("technical_review").unwrap();
    /// ```
    pub fn get(&self, name: &str) -> Result<Arc<dyn WorkflowPlugin>> {
        if self.disabled.contains(name) {
            return Err(PipelineError::Plugin(format!(
                "plugin '{name}' is disabled"
            )));
        }
        self.plugins
            .get(name)
            .cloned()
            .ok_or_else(|| PipelineError::PluginNotFound {
                name: name.to_string(),
            })
    }

    /// Returns metadata for all registered plugins, sorted alphabetically by name.
    ///
    /// Disabled plugins are included in this listing; their disabled status is
    /// separate from their registration status.
    ///
    /// # Returns
    ///
    /// A `Vec<PluginMetadata>` sorted by `name`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::registry::PluginRegistry;
    ///
    /// let registry = PluginRegistry::new();
    /// let list = registry.list_plugins();
    /// assert!(list.is_empty());
    /// ```
    pub fn list_plugins(&self) -> Vec<PluginMetadata> {
        let mut metadata: Vec<PluginMetadata> =
            self.plugins.values().map(|p| p.metadata()).collect();
        metadata.sort_by(|a, b| a.name.cmp(&b.name));
        metadata
    }

    /// Returns `true` if a plugin with the given name is registered.
    ///
    /// Does not indicate whether the plugin is enabled or disabled.
    ///
    /// # Arguments
    ///
    /// * `name` - The plugin name to check.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::registry::PluginRegistry;
    ///
    /// let registry = PluginRegistry::new();
    /// assert!(!registry.is_registered("nonexistent"));
    /// ```
    pub fn is_registered(&self, name: &str) -> bool {
        self.plugins.contains_key(name)
    }

    /// Returns `true` if the given name appears in the disabled set.
    ///
    /// A name can be disabled even if it has never been registered.
    ///
    /// # Arguments
    ///
    /// * `name` - The plugin name to check.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::registry::PluginRegistry;
    ///
    /// let mut registry = PluginRegistry::new();
    /// assert!(!registry.is_disabled("anything"));
    /// registry.disable("legacy");
    /// assert!(registry.is_disabled("legacy"));
    /// ```
    pub fn is_disabled(&self, name: &str) -> bool {
        self.disabled.contains(name)
    }

    /// Returns the number of registered plugins.
    ///
    /// Disabled-but-registered plugins are counted.
    ///
    /// # Returns
    ///
    /// Number of entries in the registry.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::registry::PluginRegistry;
    ///
    /// let registry = PluginRegistry::new();
    /// assert_eq!(registry.plugin_count(), 0);
    /// ```
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Validates that a named plugin accepts the given configuration value.
    ///
    /// This is a stub implementation. The real validation would compare
    /// `config` against the JSON Schema in `plugin.metadata().config_schema`.
    ///
    /// # Arguments
    ///
    /// * `name` - The plugin name to validate against.
    /// * `_config` - The configuration value to validate (unused in this stub).
    ///
    /// # Returns
    ///
    /// `Ok(())` when the plugin is found and not disabled.
    ///
    /// # Errors
    ///
    /// Propagates the same errors as [`get`][Self::get]:
    /// - `Err(PipelineError::Plugin(...))` if disabled.
    /// - `Err(PipelineError::PluginNotFound { name })` if not registered.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use xzardgz::plugins::registry::PluginRegistry;
    /// // registry.validate_plugin_config("my_plugin", &serde_json::json!({})).unwrap();
    /// ```
    pub fn validate_plugin_config(&self, name: &str, _config: &serde_json::Value) -> Result<()> {
        self.get(name)?;
        Ok(())
    }

    /// Internal helper used in doc-tests to check emptiness without naming conflicts.
    #[doc(hidden)]
    pub fn is_empty_for_doc_test(&self) -> bool {
        self.plugins.is_empty()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    use crate::plugins::context::{PluginContext, ToolAccessLevel};
    use crate::plugins::output::PluginOutput;
    use crate::plugins::trait_def::{PluginMetadata, WorkflowPlugin};

    // ------------------------------------------------------------------
    // MockPlugin — a concrete test double for WorkflowPlugin
    // ------------------------------------------------------------------

    /// Minimal concrete implementation of [`WorkflowPlugin`] for registry tests.
    struct MockPlugin {
        /// The name this plugin reports via `name()` and `metadata()`.
        plugin_name: String,
    }

    impl MockPlugin {
        fn new(name: impl Into<String>) -> Arc<Self> {
            Arc::new(Self {
                plugin_name: name.into(),
            })
        }
    }

    #[async_trait]
    impl WorkflowPlugin for MockPlugin {
        fn name(&self) -> &str {
            &self.plugin_name
        }

        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(&self.plugin_name, "0.1.0", "Mock plugin for testing")
        }

        fn supported_formats(&self) -> Vec<String> {
            vec!["markdown".to_string()]
        }

        fn required_tool_access(&self) -> ToolAccessLevel {
            ToolAccessLevel::None
        }

        async fn run(&self, _ctx: PluginContext) -> crate::error::Result<PluginOutput> {
            Ok(PluginOutput::success("mock complete"))
        }
    }

    // ------------------------------------------------------------------
    // register / is_registered
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_registry_register_stores_plugin() {
        let mut registry = PluginRegistry::new();
        registry.register(MockPlugin::new("alpha"));
        assert!(registry.is_registered("alpha"));
        assert_eq!(registry.plugin_count(), 1);
    }

    #[test]
    fn test_plugin_registry_is_registered_returns_true() {
        let mut registry = PluginRegistry::new();
        registry.register(MockPlugin::new("beta"));
        assert!(registry.is_registered("beta"));
    }

    // ------------------------------------------------------------------
    // get
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_registry_get_known_plugin_returns_ok() {
        let mut registry = PluginRegistry::new();
        registry.register(MockPlugin::new("my_plugin"));
        let result = registry.get("my_plugin");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name(), "my_plugin");
    }

    #[test]
    fn test_plugin_registry_get_unknown_plugin_returns_not_found() {
        let registry = PluginRegistry::new();
        let result = registry.get("nonexistent");
        assert!(matches!(
            result,
            Err(PipelineError::PluginNotFound { name })
            if name == "nonexistent"
        ));
    }

    // ------------------------------------------------------------------
    // disable / is_disabled
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_registry_disable_marks_plugin_disabled() {
        let mut registry = PluginRegistry::new();
        registry.disable("legacy_plugin");
        assert!(registry.is_disabled("legacy_plugin"));
    }

    #[test]
    fn test_plugin_registry_is_disabled_returns_true() {
        let mut registry = PluginRegistry::new();
        registry.register(MockPlugin::new("old_plugin"));
        registry.disable("old_plugin");
        assert!(registry.is_disabled("old_plugin"));
    }

    #[test]
    fn test_plugin_registry_get_disabled_plugin_returns_err() {
        let mut registry = PluginRegistry::new();
        registry.register(MockPlugin::new("disabled_plugin"));
        registry.disable("disabled_plugin");
        let result = registry.get("disabled_plugin");
        assert!(matches!(result, Err(PipelineError::Plugin(_))));
        if let Err(PipelineError::Plugin(msg)) = result {
            assert!(msg.contains("disabled_plugin"));
            assert!(msg.contains("disabled"));
        }
    }

    // ------------------------------------------------------------------
    // list_plugins
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_registry_list_plugins_returns_sorted_metadata() {
        let mut registry = PluginRegistry::new();
        registry.register(MockPlugin::new("zebra"));
        registry.register(MockPlugin::new("alpha"));
        registry.register(MockPlugin::new("mango"));

        let list = registry.list_plugins();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "alpha");
        assert_eq!(list[1].name, "mango");
        assert_eq!(list[2].name, "zebra");
    }

    #[test]
    fn test_plugin_registry_list_plugins_empty_returns_empty_vec() {
        let registry = PluginRegistry::new();
        let list = registry.list_plugins();
        assert!(list.is_empty());
    }

    // ------------------------------------------------------------------
    // validate_plugin_config
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_registry_validate_plugin_config_known_plugin_returns_ok() {
        let mut registry = PluginRegistry::new();
        registry.register(MockPlugin::new("configured_plugin"));
        let config = serde_json::json!({"max_findings": 10});
        let result = registry.validate_plugin_config("configured_plugin", &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_plugin_registry_validate_plugin_config_unknown_plugin_returns_err() {
        let registry = PluginRegistry::new();
        let config = serde_json::json!({});
        let result = registry.validate_plugin_config("unknown_plugin", &config);
        assert!(matches!(
            result,
            Err(PipelineError::PluginNotFound { name })
            if name == "unknown_plugin"
        ));
    }

    #[test]
    fn test_plugin_registry_validate_plugin_config_disabled_plugin_returns_err() {
        let mut registry = PluginRegistry::new();
        registry.register(MockPlugin::new("to_disable"));
        registry.disable("to_disable");
        let result = registry.validate_plugin_config("to_disable", &serde_json::json!({}));
        assert!(matches!(result, Err(PipelineError::Plugin(_))));
    }

    // ------------------------------------------------------------------
    // plugin_count / Default
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_registry_plugin_count_reflects_registrations() {
        let mut registry = PluginRegistry::new();
        assert_eq!(registry.plugin_count(), 0);
        registry.register(MockPlugin::new("one"));
        assert_eq!(registry.plugin_count(), 1);
        registry.register(MockPlugin::new("two"));
        assert_eq!(registry.plugin_count(), 2);
    }

    #[test]
    fn test_plugin_registry_default_is_empty() {
        let registry = PluginRegistry::default();
        assert_eq!(registry.plugin_count(), 0);
        assert!(!registry.is_registered("anything"));
        assert!(!registry.is_disabled("anything"));
    }
}
