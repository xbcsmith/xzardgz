//! Three-level prompt template resolver for workflow plugins.
//!
//! See [`PromptLoader`] for the full resolution algorithm and priority order.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::PromptsConfig;

// ---------------------------------------------------------------------------
// Embedded template registry
// ---------------------------------------------------------------------------

/// Statically embedded default templates indexed by `(plugin, key, content)`.
///
/// These are compiled into the binary via [`include_str!`] and serve as the
/// lowest-priority fallback when no override is found.
static EMBEDDED_TEMPLATES: &[(&str, &str, &str)] = &[
    (
        "security_review",
        "system",
        include_str!("templates/security_review/system.tera"),
    ),
    (
        "technical_review",
        "system",
        include_str!("templates/technical_review/system.tera"),
    ),
];

/// All plugin identifiers known to this loader.
static KNOWN_PLUGINS: &[&str] = &["security_review", "technical_review"];

/// Per-plugin lists of known template keys.
static PLUGIN_KEYS: &[(&str, &[&str])] = &[
    ("security_review", &["system"]),
    ("technical_review", &["system"]),
];

// ---------------------------------------------------------------------------
// PromptLoader
// ---------------------------------------------------------------------------

/// Three-level prompt template resolver for workflow plugins.
///
/// Resolves a `(plugin, key)` pair to a rendered string by checking, in order:
///
/// 1. In-memory overrides set via
///    [`with_in_memory_overrides`][Self::with_in_memory_overrides].
/// 2. File-based overrides in the directories listed in
///    [`PromptsConfig::directories`][crate::config::PromptsConfig::directories]
///    (only when
///    [`PromptsConfig::allow_overrides`][crate::config::PromptsConfig::allow_overrides]
///    is `true`).
/// 3. Compiled-in embedded defaults from
///    `src/prompts/templates/{plugin}/{key}.tera`.
///
/// The loader is infallible: override failures emit a [`tracing::warn!`] and
/// fall through to the next priority level. If no embedded template is found
/// either, an empty string is returned and a warning is emitted.
///
/// # Examples
///
/// ```
/// use xzardgz::prompts::PromptLoader;
/// use xzardgz::config::PromptsConfig;
///
/// let loader = PromptLoader::new(PromptsConfig::default());
/// let ctx = tera::Context::new();
/// let prompt = loader.render("security_review", "system", &ctx);
/// assert!(!prompt.is_empty());
/// ```
#[derive(Clone, Debug)]
pub struct PromptLoader {
    config: PromptsConfig,
    in_memory_overrides: HashMap<String, String>,
}

impl Default for PromptLoader {
    /// Creates a `PromptLoader` using [`PromptsConfig::default()`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::prompts::PromptLoader;
    ///
    /// let loader = PromptLoader::default();
    /// assert!(!loader.has_in_memory_overrides());
    /// ```
    fn default() -> Self {
        Self::new(PromptsConfig::default())
    }
}

impl PromptLoader {
    /// Creates a new `PromptLoader` with the given configuration and no
    /// in-memory overrides.
    ///
    /// # Arguments
    ///
    /// * `config` - Prompt configuration controlling override directories and
    ///   whether file-based overrides are honoured.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::prompts::PromptLoader;
    /// use xzardgz::config::PromptsConfig;
    ///
    /// let loader = PromptLoader::new(PromptsConfig::default());
    /// assert!(!loader.has_in_memory_overrides());
    /// ```
    pub fn new(config: PromptsConfig) -> Self {
        Self {
            config,
            in_memory_overrides: HashMap::new(),
        }
    }

    /// Replaces this loader's in-memory override map and returns `self`.
    ///
    /// Keys must use the `"{plugin}/{key}"` format, e.g.
    /// `"security_review/system"`. Values are raw Tera template strings.
    ///
    /// # Arguments
    ///
    /// * `overrides` - Map from `"{plugin}/{key}"` to Tera template content.
    ///
    /// # Returns
    ///
    /// `self` with the override map replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use xzardgz::prompts::PromptLoader;
    ///
    /// let mut overrides = HashMap::new();
    /// overrides.insert("security_review/system".to_string(), "Custom.".to_string());
    /// let loader = PromptLoader::default().with_in_memory_overrides(overrides);
    /// assert!(loader.has_in_memory_overrides());
    /// ```
    pub fn with_in_memory_overrides(mut self, overrides: HashMap<String, String>) -> Self {
        self.in_memory_overrides = overrides;
        self
    }

    /// Returns `true` if at least one in-memory override is registered.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::prompts::PromptLoader;
    ///
    /// let loader = PromptLoader::default();
    /// assert!(!loader.has_in_memory_overrides());
    /// ```
    pub fn has_in_memory_overrides(&self) -> bool {
        !self.in_memory_overrides.is_empty()
    }

    /// Resolves and renders the template for `(plugin, key)`.
    ///
    /// Resolution priority (highest to lowest):
    ///
    /// 1. In-memory override keyed `"{plugin}/{key}"`.
    /// 2. File override at `{dir}/{plugin}/{key}.tera` for each directory in
    ///    [`PromptsConfig::directories`][crate::config::PromptsConfig::directories]
    ///    (only when `allow_overrides` is `true`).
    /// 3. Compiled-in embedded default.
    ///
    /// If a higher-priority source exists but fails to render, a warning is
    /// emitted and the next priority level is tried. If no template can be
    /// resolved, a warning is emitted and an empty string is returned.
    ///
    /// # Arguments
    ///
    /// * `plugin` - Plugin identifier, e.g. `"security_review"`.
    /// * `key` - Template key within the plugin, e.g. `"system"`.
    /// * `ctx` - Tera rendering context supplying template variables.
    ///
    /// # Returns
    ///
    /// The rendered template string, or an empty string when no template could
    /// be resolved.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::prompts::PromptLoader;
    /// use xzardgz::config::PromptsConfig;
    ///
    /// let loader = PromptLoader::new(PromptsConfig::default());
    /// let result = loader.render("security_review", "system", &tera::Context::new());
    /// assert!(!result.is_empty());
    /// ```
    pub fn render(&self, plugin: &str, key: &str, ctx: &tera::Context) -> String {
        let map_key = format!("{plugin}/{key}");

        // Priority 1: in-memory override.
        if let Some(template) = self.in_memory_overrides.get(&map_key) {
            match Self::render_tera(template, ctx) {
                Ok(rendered) => return rendered,
                Err(e) => {
                    tracing::warn!(
                        plugin = plugin,
                        key = key,
                        error = %e,
                        "In-memory prompt override failed to render; falling through to next level"
                    );
                }
            }
        }

        // Priority 2: file-based overrides.
        if self.config.allow_overrides {
            for dir in &self.config.directories {
                let path = PathBuf::from(dir).join(plugin).join(format!("{key}.tera"));
                if !path.exists() {
                    continue;
                }
                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "Failed to read prompt override file; skipping"
                        );
                        continue;
                    }
                };
                match Self::render_tera(&content, ctx) {
                    Ok(rendered) => return rendered,
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "File prompt override failed to render; skipping to next directory"
                        );
                        continue;
                    }
                }
            }
        }

        // Priority 3: compiled-in embedded default.
        Self::render_embedded(plugin, key, ctx)
    }

    /// Returns the statically compiled list of all known plugin identifiers.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::prompts::PromptLoader;
    ///
    /// let plugins = PromptLoader::known_plugins();
    /// assert!(plugins.contains(&"security_review"));
    /// assert!(plugins.contains(&"technical_review"));
    /// ```
    pub fn known_plugins() -> &'static [&'static str] {
        KNOWN_PLUGINS
    }

    /// Returns the statically compiled list of known template keys for the
    /// given plugin, or an empty slice if the plugin is unknown.
    ///
    /// # Arguments
    ///
    /// * `plugin` - Plugin identifier to look up.
    ///
    /// # Returns
    ///
    /// A static slice of key names, or `&[]` if `plugin` is not registered.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::prompts::PromptLoader;
    ///
    /// let keys = PromptLoader::known_keys("security_review");
    /// assert!(keys.contains(&"system"));
    ///
    /// let empty = PromptLoader::known_keys("unknown_plugin");
    /// assert!(empty.is_empty());
    /// ```
    pub fn known_keys(plugin: &str) -> &'static [&'static str] {
        for (p, keys) in PLUGIN_KEYS {
            if *p == plugin {
                return keys;
            }
        }
        &[]
    }

    /// Returns the raw embedded template content for `(plugin, key)` without
    /// rendering.
    ///
    /// This is the source text that `prompts export` writes to disk, and the
    /// same content that [`render`][Self::render] uses as the lowest-priority
    /// fallback. Returns `None` when no embedded template is registered for
    /// the `(plugin, key)` pair.
    ///
    /// # Arguments
    ///
    /// * `plugin` - Plugin identifier, e.g. `"security_review"`.
    /// * `key`    - Template key, e.g. `"system"`.
    ///
    /// # Returns
    ///
    /// `Some(&str)` with the raw Tera template source, or `None` if not found.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::prompts::PromptLoader;
    ///
    /// let raw = PromptLoader::embedded_raw("security_review", "system");
    /// assert!(raw.is_some());
    /// assert!(!raw.unwrap().is_empty());
    ///
    /// let missing = PromptLoader::embedded_raw("unknown", "system");
    /// assert!(missing.is_none());
    /// ```
    pub fn embedded_raw(plugin: &str, key: &str) -> Option<&'static str> {
        Self::embedded_template(plugin, key)
    }
    ///
    /// Returns `None` when no embedded template is registered for the pair.
    fn embedded_template(plugin: &str, key: &str) -> Option<&'static str> {
        for (p, k, content) in EMBEDDED_TEMPLATES {
            if *p == plugin && *k == key {
                return Some(content);
            }
        }
        None
    }

    /// Renders the embedded default template for `(plugin, key)`.
    ///
    /// If no embedded template is registered, emits a warning and returns an
    /// empty string. If the embedded template fails to render, emits a warning
    /// and returns the raw template content unchanged.
    fn render_embedded(plugin: &str, key: &str, ctx: &tera::Context) -> String {
        match Self::embedded_template(plugin, key) {
            None => {
                tracing::warn!(
                    plugin = plugin,
                    key = key,
                    "No embedded template found for plugin/key pair"
                );
                String::new()
            }
            Some(template) => match Self::render_tera(template, ctx) {
                Ok(rendered) => rendered,
                Err(e) => {
                    tracing::warn!(
                        plugin = plugin,
                        key = key,
                        error = %e,
                        "Embedded template failed to render; returning raw content"
                    );
                    template.to_string()
                }
            },
        }
    }

    /// Renders a Tera template string against the given context.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` containing the Tera error message if rendering
    /// fails.
    fn render_tera(template: &str, ctx: &tera::Context) -> Result<String, String> {
        tera::Tera::one_off(template, ctx, false).map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PromptsConfig;

    #[test]
    fn test_render_security_review_system_returns_nonempty_string() {
        let loader = PromptLoader::new(PromptsConfig::default());
        let result = loader.render("security_review", "system", &tera::Context::new());
        assert!(!result.is_empty());
    }

    #[test]
    fn test_render_technical_review_system_returns_nonempty_string() {
        let loader = PromptLoader::new(PromptsConfig::default());
        let result = loader.render("technical_review", "system", &tera::Context::new());
        assert!(!result.is_empty());
    }

    #[test]
    fn test_render_embedded_default_contains_expected_keyword() {
        let loader = PromptLoader::new(PromptsConfig::default());
        let result = loader.render("security_review", "system", &tera::Context::new());
        assert!(result.contains("security"));
    }

    #[test]
    fn test_render_with_in_memory_override_uses_override() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "security_review/system".to_string(),
            "Custom prompt.".to_string(),
        );
        let loader = PromptLoader::default().with_in_memory_overrides(overrides);
        let result = loader.render("security_review", "system", &tera::Context::new());
        assert_eq!(result, "Custom prompt.");
    }

    #[test]
    fn test_render_without_override_falls_back_to_embedded() {
        let config = PromptsConfig {
            directories: vec![],
            allow_overrides: true,
        };
        let loader = PromptLoader::new(config);
        let result = loader.render("security_review", "system", &tera::Context::new());
        assert!(!result.is_empty());
    }

    #[test]
    fn test_render_with_malformed_in_memory_override_falls_back_to_embedded() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "security_review/system".to_string(),
            "{% for bad syntax".to_string(),
        );
        let loader = PromptLoader::default().with_in_memory_overrides(overrides);
        let result = loader.render("security_review", "system", &tera::Context::new());
        assert!(!result.is_empty());
    }

    #[test]
    fn test_render_with_file_override_uses_file_when_allow_overrides_true() {
        let tmp = tempfile::TempDir::new()
            // SAFETY: TempDir::new fails only under extreme OS conditions
            // (e.g. no writable temp directory); acceptable to unwrap in tests.
            .expect("failed to create temp dir");
        let plugin_dir = tmp.path().join("security_review");
        std::fs::create_dir_all(&plugin_dir).expect("failed to create plugin directory");
        std::fs::write(plugin_dir.join("system.tera"), "File override prompt.")
            .expect("failed to write template file");
        let config = PromptsConfig {
            directories: vec![tmp.path().to_string_lossy().into_owned()],
            allow_overrides: true,
        };
        let loader = PromptLoader::new(config);
        let result = loader.render("security_review", "system", &tera::Context::new());
        assert_eq!(result, "File override prompt.");
    }

    #[test]
    fn test_render_with_allow_overrides_false_ignores_file_override() {
        let tmp = tempfile::TempDir::new().expect("failed to create temp dir");
        let plugin_dir = tmp.path().join("security_review");
        std::fs::create_dir_all(&plugin_dir).expect("failed to create plugin directory");
        std::fs::write(plugin_dir.join("system.tera"), "File override prompt.")
            .expect("failed to write template file");
        let config = PromptsConfig {
            directories: vec![tmp.path().to_string_lossy().into_owned()],
            allow_overrides: false,
        };
        let loader = PromptLoader::new(config);
        let result = loader.render("security_review", "system", &tera::Context::new());
        assert!(!result.is_empty());
        assert_ne!(result, "File override prompt.");
    }

    #[test]
    fn test_render_with_missing_plugin_returns_empty_string() {
        let loader = PromptLoader::new(PromptsConfig::default());
        let result = loader.render("nonexistent_plugin", "system", &tera::Context::new());
        assert!(result.is_empty());
    }

    #[test]
    fn test_render_with_context_variables_substituted() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "security_review/system".to_string(),
            "Hello {{ name }}.".to_string(),
        );
        let loader = PromptLoader::default().with_in_memory_overrides(overrides);
        let mut ctx = tera::Context::new();
        ctx.insert("name", &"World");
        let result = loader.render("security_review", "system", &ctx);
        assert_eq!(result, "Hello World.");
    }

    #[test]
    fn test_known_plugins_contains_both() {
        let plugins = PromptLoader::known_plugins();
        assert!(plugins.contains(&"security_review"));
        assert!(plugins.contains(&"technical_review"));
    }

    #[test]
    fn test_known_keys_security_review_contains_system() {
        let keys = PromptLoader::known_keys("security_review");
        assert!(keys.contains(&"system"));
    }

    #[test]
    fn test_known_keys_technical_review_contains_system() {
        let keys = PromptLoader::known_keys("technical_review");
        assert!(keys.contains(&"system"));
    }

    #[test]
    fn test_known_keys_unknown_plugin_returns_empty() {
        let keys = PromptLoader::known_keys("unknown");
        assert!(keys.is_empty());
    }

    #[test]
    fn test_has_in_memory_overrides_returns_false_when_empty() {
        let loader = PromptLoader::default();
        assert!(!loader.has_in_memory_overrides());
    }

    #[test]
    fn test_has_in_memory_overrides_returns_true_when_set() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "security_review/system".to_string(),
            "Override.".to_string(),
        );
        let loader = PromptLoader::default().with_in_memory_overrides(overrides);
        assert!(loader.has_in_memory_overrides());
    }

    #[test]
    fn test_embedded_raw_security_review_system_returns_some() {
        let raw = PromptLoader::embedded_raw("security_review", "system");
        assert!(
            raw.is_some(),
            "embedded_raw must return Some for security_review/system"
        );
        assert!(!raw.unwrap().is_empty());
    }

    #[test]
    fn test_embedded_raw_technical_review_system_returns_some() {
        let raw = PromptLoader::embedded_raw("technical_review", "system");
        assert!(
            raw.is_some(),
            "embedded_raw must return Some for technical_review/system"
        );
        assert!(!raw.unwrap().is_empty());
    }

    #[test]
    fn test_embedded_raw_unknown_plugin_returns_none() {
        let raw = PromptLoader::embedded_raw("nonexistent_plugin", "system");
        assert!(raw.is_none());
    }

    #[test]
    fn test_embedded_raw_unknown_key_returns_none() {
        let raw = PromptLoader::embedded_raw("security_review", "nonexistent_key");
        assert!(raw.is_none());
    }

    #[test]
    fn test_embedded_raw_content_matches_rendered_default() {
        // When no Tera variables are used, raw content and rendered output match.
        let raw = PromptLoader::embedded_raw("security_review", "system").unwrap();
        let rendered =
            PromptLoader::default().render("security_review", "system", &tera::Context::new());
        assert_eq!(raw, rendered.as_str());
    }
}
