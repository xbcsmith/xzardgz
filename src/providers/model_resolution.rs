//! Model capability resolver for the XZardgz pipeline.
//!
//! [`ModelResolver`] implements the provider and model selection precedence
//! chain.  It accepts a [`ResolutionContext`] (inputs from CLI, watcher,
//! workflow, plugin, and config) and returns a fully-resolved [`ResolvedModel`]
//! record that captures every selection decision made during the run.
//!
//! # Precedence chain
//!
//! Provider selection (highest to lowest):
//! 1. CLI override (`--provider`)
//! 2. Watcher task metadata
//! 3. Workflow plan specification
//! 4. Plugin preference
//! 5. `config.provider.default`
//! 6. Hardcoded fallback `"openai"`
//!
//! Model selection (highest to lowest):
//! 1. CLI override (`--model`)
//! 2. Watcher task metadata
//! 3. Workflow plan specification
//! 4. Plugin preference
//! 5. `config.model_selection.preferred_models[0]`
//! 6. Provider-specific config default (passed as parameter)

use serde::{Deserialize, Serialize};

use super::types::{MetadataSource, ModelCapabilities, ModelMetadata, ThinkingMode};
use crate::config::Config;
use crate::diagnostics::{DiagnosticCategory, Diagnostics};
use crate::error::{PipelineError, Result};

// ---------------------------------------------------------------------------
// ResolutionContext
// ---------------------------------------------------------------------------

/// Inputs to the resolution process, ordered from highest to lowest precedence.
///
/// `None` at any level means "not specified here; try the next level down."
/// Build a context using the builder methods ([`with_cli`][ResolutionContext::with_cli],
/// [`with_watcher`][ResolutionContext::with_watcher], etc.) for a fluent API.
///
/// # Examples
///
/// ```
/// use xzardgz::providers::model_resolution::ResolutionContext;
///
/// let ctx = ResolutionContext::new()
///     .with_cli(Some("openai".to_string()), Some("gpt-4o".to_string()));
/// assert_eq!(ctx.effective_provider("ollama"), "openai");
/// assert_eq!(ctx.effective_model_override(), Some("gpt-4o".to_string()));
/// ```
#[derive(Debug, Clone, Default)]
pub struct ResolutionContext {
    /// Provider name supplied at the CLI level (highest precedence).
    pub cli_provider: Option<String>,
    /// Model identifier supplied at the CLI level (highest precedence).
    pub cli_model: Option<String>,
    /// Provider name supplied by a watcher task.
    pub watcher_provider: Option<String>,
    /// Model identifier supplied by a watcher task.
    pub watcher_model: Option<String>,
    /// Provider name declared in a workflow plan.
    pub workflow_provider: Option<String>,
    /// Model identifier declared in a workflow plan.
    pub workflow_model: Option<String>,
    /// Provider preference expressed by a plugin.
    pub plugin_provider: Option<String>,
    /// Model preference expressed by a plugin.
    pub plugin_model: Option<String>,
    /// Requested thinking/reasoning mode for this execution.
    pub thinking_mode: ThinkingMode,
}

impl ResolutionContext {
    /// Creates a new [`ResolutionContext`] with all fields set to `None` /
    /// [`ThinkingMode::None`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies CLI-level provider and model overrides (highest precedence).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::providers::model_resolution::ResolutionContext;
    ///
    /// let ctx = ResolutionContext::new()
    ///     .with_cli(Some("anthropic".to_string()), None);
    /// assert_eq!(ctx.effective_provider("openai"), "anthropic");
    /// ```
    pub fn with_cli(mut self, provider: Option<String>, model: Option<String>) -> Self {
        self.cli_provider = provider;
        self.cli_model = model;
        self
    }

    /// Applies watcher-level provider and model overrides.
    ///
    /// Watcher overrides are lower in precedence than CLI overrides.
    pub fn with_watcher(mut self, provider: Option<String>, model: Option<String>) -> Self {
        self.watcher_provider = provider;
        self.watcher_model = model;
        self
    }

    /// Applies workflow-level provider and model overrides.
    ///
    /// Workflow overrides are lower in precedence than watcher overrides.
    pub fn with_workflow(mut self, provider: Option<String>, model: Option<String>) -> Self {
        self.workflow_provider = provider;
        self.workflow_model = model;
        self
    }

    /// Applies plugin-level provider and model preferences.
    ///
    /// Plugin preferences are lower in precedence than workflow overrides.
    pub fn with_plugin(mut self, provider: Option<String>, model: Option<String>) -> Self {
        self.plugin_provider = provider;
        self.plugin_model = model;
        self
    }

    /// Sets the requested thinking/reasoning mode.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::providers::model_resolution::ResolutionContext;
    /// use xzardgz::providers::types::ThinkingMode;
    ///
    /// let ctx = ResolutionContext::new().with_thinking_mode(ThinkingMode::High);
    /// assert_eq!(ctx.thinking_mode, ThinkingMode::High);
    /// ```
    pub fn with_thinking_mode(mut self, mode: ThinkingMode) -> Self {
        self.thinking_mode = mode;
        self
    }

    /// Returns the effective provider name by walking the 5-level precedence chain.
    ///
    /// Precedence: CLI > watcher > workflow > plugin > `config_default`.
    ///
    /// # Arguments
    ///
    /// * `config_default` - The `config.provider.default` value to use when no
    ///   higher-precedence level specifies a provider.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::providers::model_resolution::ResolutionContext;
    ///
    /// let ctx = ResolutionContext::new()
    ///     .with_cli(Some("anthropic".to_string()), None);
    /// assert_eq!(ctx.effective_provider("openai"), "anthropic");
    ///
    /// let empty = ResolutionContext::new();
    /// assert_eq!(empty.effective_provider("ollama"), "ollama");
    /// ```
    pub fn effective_provider(&self, config_default: &str) -> String {
        self.cli_provider
            .clone()
            .or_else(|| self.watcher_provider.clone())
            .or_else(|| self.workflow_provider.clone())
            .or_else(|| self.plugin_provider.clone())
            .unwrap_or_else(|| config_default.to_string())
    }

    /// Returns the effective model override from the 4 explicit precedence levels
    /// (CLI > watcher > workflow > plugin).
    ///
    /// Returns `None` when no level specifies a model; the caller then falls
    /// back to `config.model_selection.preferred_models` or the provider default.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::providers::model_resolution::ResolutionContext;
    ///
    /// let ctx = ResolutionContext::new()
    ///     .with_cli(None, Some("gpt-4o".to_string()));
    /// assert_eq!(ctx.effective_model_override(), Some("gpt-4o".to_string()));
    ///
    /// let empty = ResolutionContext::new();
    /// assert_eq!(empty.effective_model_override(), None);
    /// ```
    pub fn effective_model_override(&self) -> Option<String> {
        self.cli_model
            .clone()
            .or_else(|| self.watcher_model.clone())
            .or_else(|| self.workflow_model.clone())
            .or_else(|| self.plugin_model.clone())
    }
}

// ---------------------------------------------------------------------------
// ResolvedModel
// ---------------------------------------------------------------------------

/// The fully-resolved provider and model selection for a single pipeline run.
///
/// Persisted in workspace state, included in report envelopes, and emitted in
/// watcher result messages so every downstream consumer knows exactly which
/// model executed a request and why.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedModel {
    /// The provider that was originally requested (before any fallback).
    pub requested_provider: String,
    /// The provider that was ultimately selected.
    pub selected_provider: String,
    /// The model that was requested, if any (from CLI/watcher/workflow/plugin
    /// or `preferred_models`).  `None` means the provider default was used.
    pub requested_model: Option<String>,
    /// The model identifier that was ultimately selected.
    pub selected_model: String,
    /// Whether a fallback model was substituted for the requested model.
    pub fallback_used: bool,
    /// Human-readable reason for the fallback, if one occurred.
    pub fallback_reason: Option<String>,
    /// Capability flags of the selected model.
    pub capabilities: ModelCapabilities,
    /// The thinking mode that was requested.
    pub thinking_mode_requested: ThinkingMode,
    /// The thinking mode that will actually be used (may differ from requested
    /// after degraded-mode resolution).
    pub thinking_mode_selected: ThinkingMode,
    /// Provenance of the model metadata used during resolution.
    pub metadata_source: MetadataSource,
    /// Warnings and informational messages produced during resolution.
    pub diagnostics: Diagnostics,
}

// ---------------------------------------------------------------------------
// ModelResolver
// ---------------------------------------------------------------------------

/// Stateless model resolution service.
///
/// Resolves provider and model selection from a [`ResolutionContext`] and a
/// slice of available [`ModelMetadata`] records, applying the precedence and
/// capability rules defined in the Phase 9 specification.
///
/// # Examples
///
/// ```
/// use xzardgz::config::Config;
/// use xzardgz::providers::model_resolution::{ModelResolver, ResolutionContext};
/// use xzardgz::providers::types::{MetadataSource, ModelCapabilities, ModelMetadata};
///
/// let resolver = ModelResolver::new();
/// let ctx = ResolutionContext::new();
/// let mut config = Config::default();
/// config.model_selection.require_tools = false;
/// config.model_selection.require_structured_output = false;
/// config.model_selection.min_context_tokens = 0;
/// config.model_selection.preferred_models = vec!["my-model".to_string()];
/// let available = vec![ModelMetadata::new("my-model", ModelCapabilities::default())];
///
/// let result = resolver
///     .resolve_with_static(&ctx, &config, &available, "my-model", MetadataSource::Static)
///     .unwrap();
/// assert_eq!(result.selected_model, "my-model");
/// assert!(!result.fallback_used);
/// ```
pub struct ModelResolver;

impl ModelResolver {
    /// Creates a new `ModelResolver`.
    pub fn new() -> Self {
        Self
    }

    /// Resolves provider and model selection using the provided model metadata.
    ///
    /// # Resolution steps
    ///
    /// 1. Determine effective provider via [`ResolutionContext::effective_provider`].
    /// 2. Determine requested model from
    ///    [`effective_model_override`][ResolutionContext::effective_model_override],
    ///    falling back to:
    ///    - `config.model_selection.preferred_models[0]` if set, then
    ///    - `provider_config_default_model`.
    /// 3. Search `available_models` for the requested model:
    ///    - Found: use it.
    ///    - Not found + `config.model_selection.auto_fallback` true: iterate
    ///      `config.model_selection.fallback_models` then all `available_models`
    ///      for a model satisfying capability requirements.
    ///    - Not found + `auto_fallback` false: return
    ///      [`PipelineError::Provider`].
    /// 4. Validate capability requirements from
    ///    `config.model_selection` against the selected model.
    /// 5. Resolve thinking mode via
    ///    [`resolve_thinking_mode`][ModelResolver::resolve_thinking_mode].
    /// 6. Build and return [`ResolvedModel`].
    ///
    /// # Arguments
    ///
    /// * `context` - Resolution inputs in precedence order.
    /// * `config` - Full pipeline configuration.
    /// * `available_models` - Model metadata slice to search.  Pass static
    ///   tables when live API data is unavailable.
    /// * `provider_config_default_model` - The default model from the
    ///   provider-specific config section (e.g., `config.openai.model`).
    /// * `metadata_source` - Provenance of `available_models`.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Provider`] when:
    /// - The requested model is not found and `auto_fallback` is false.
    /// - No suitable fallback model exists.
    /// - The selected model does not meet the capability requirements.
    /// - An explicit thinking level is required but unsupported and
    ///   `allow_degraded_metadata` is false.
    pub fn resolve_with_static(
        &self,
        context: &ResolutionContext,
        config: &Config,
        available_models: &[ModelMetadata],
        provider_config_default_model: &str,
        metadata_source: MetadataSource,
    ) -> Result<ResolvedModel> {
        let mut diagnostics = Diagnostics::new();

        // Step 1: effective provider.
        let requested_provider = context.effective_provider(&config.provider.default);

        // Step 2: effective model.
        // Levels 1-4: explicit override from CLI/watcher/workflow/plugin.
        let model_override = context.effective_model_override();
        // Level 5: first entry in preferred_models.
        let preferred_model = config.model_selection.preferred_models.first().cloned();
        // The explicitly-requested model (or None if only provider-default applies).
        let explicit_request = model_override.or(preferred_model);
        // Level 6: provider-specific config default (always present).
        let requested_model_str = explicit_request
            .clone()
            .unwrap_or_else(|| provider_config_default_model.to_string());

        // Step 3: find the model in available_models.
        let (selected_meta, fallback_used, fallback_reason) = {
            if let Some(found) = available_models
                .iter()
                .find(|m| m.id == requested_model_str)
            {
                (found.clone(), false, None)
            } else if config.model_selection.auto_fallback {
                // Try the fallback_models list in order first.
                let mut chosen: Option<ModelMetadata> = None;
                for fallback_id in &config.model_selection.fallback_models {
                    if let Some(m) = available_models.iter().find(|m| m.id == *fallback_id)
                        && self.satisfies_requirements(m, config)
                    {
                        chosen = Some(m.clone());
                        break;
                    }
                }

                // If not found in fallback_models, search all available_models.
                if chosen.is_none() {
                    chosen = available_models
                        .iter()
                        .find(|m| self.satisfies_requirements(m, config))
                        .cloned();
                }

                match chosen {
                    Some(m) => {
                        let reason = format!(
                            "requested model '{}' not available; fell back to '{}'",
                            requested_model_str, m.id
                        );
                        diagnostics.push_warning(DiagnosticCategory::ProviderFallback, &reason);
                        (m, true, Some(reason))
                    }
                    None => {
                        return Err(PipelineError::Provider(format!(
                            "no suitable fallback model found for provider '{}'",
                            requested_provider
                        )));
                    }
                }
            } else {
                return Err(PipelineError::Provider(format!(
                    "model '{}' not found for provider '{}' and auto_fallback is disabled",
                    requested_model_str, requested_provider
                )));
            }
        };

        // Step 4: validate capability requirements against the selected model.
        if config.model_selection.require_tools && !selected_meta.capabilities.supports_tools {
            return Err(PipelineError::Provider(format!(
                "model '{}' does not support tools as required by configuration",
                selected_meta.id
            )));
        }
        if config.model_selection.require_structured_output
            && !selected_meta.capabilities.supports_structured_output
        {
            return Err(PipelineError::Provider(format!(
                "model '{}' does not support structured output as required by configuration",
                selected_meta.id
            )));
        }
        if selected_meta.capabilities.context_window_tokens
            < config.model_selection.min_context_tokens
        {
            return Err(PipelineError::Provider(format!(
                "model '{}' context window ({} tokens) is below required minimum ({} tokens)",
                selected_meta.id,
                selected_meta.capabilities.context_window_tokens,
                config.model_selection.min_context_tokens
            )));
        }

        // Step 5: resolve thinking mode.
        let thinking_mode_selected = Self::resolve_thinking_mode(
            &context.thinking_mode,
            selected_meta.capabilities.supports_thinking,
            config.model_metadata.allow_degraded_metadata,
            &mut diagnostics,
        )?;

        // Step 6: build ResolvedModel.
        Ok(ResolvedModel {
            requested_provider: requested_provider.clone(),
            selected_provider: requested_provider,
            requested_model: explicit_request,
            selected_model: selected_meta.id,
            fallback_used,
            fallback_reason,
            capabilities: selected_meta.capabilities,
            thinking_mode_requested: context.thinking_mode.clone(),
            thinking_mode_selected,
            metadata_source,
            diagnostics,
        })
    }

    /// Resolves the effective thinking mode for the selected model.
    ///
    /// # Rules
    ///
    /// | Requested          | Supports thinking | allow_degraded | Result                           |
    /// |--------------------|-------------------|---------------|----------------------------------|
    /// | `None`             | any               | any           | `None`                           |
    /// | `Auto`             | true              | any           | `Low` + no diagnostic            |
    /// | `Auto`             | false             | any           | `None` + info diagnostic         |
    /// | explicit level     | true              | any           | level unchanged                  |
    /// | explicit level     | false             | true          | `None` + warning diagnostic      |
    /// | explicit level     | false             | false         | `Err(PipelineError::Provider)`   |
    ///
    /// # Arguments
    ///
    /// * `requested` - The thinking mode requested by the caller.
    /// * `model_supports_thinking` - Whether the selected model supports thinking.
    /// * `allow_degraded` - Whether to continue with degraded capabilities.
    /// * `diagnostics` - Mutable diagnostic collector; warnings/infos are pushed here.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Provider`] when an explicit thinking level is
    /// requested, the model does not support thinking, and `allow_degraded` is
    /// false.
    fn resolve_thinking_mode(
        requested: &ThinkingMode,
        model_supports_thinking: bool,
        allow_degraded: bool,
        diagnostics: &mut Diagnostics,
    ) -> Result<ThinkingMode> {
        match requested {
            ThinkingMode::None => Ok(ThinkingMode::None),

            ThinkingMode::Auto => {
                if model_supports_thinking {
                    Ok(ThinkingMode::Low)
                } else {
                    diagnostics.push_info(
                        DiagnosticCategory::ProviderFallback,
                        "thinking mode 'Auto' requested but model does not support thinking; \
                         proceeding without thinking",
                    );
                    Ok(ThinkingMode::None)
                }
            }

            explicit => {
                if model_supports_thinking {
                    Ok(explicit.clone())
                } else if allow_degraded {
                    diagnostics.push_warning(
                        DiagnosticCategory::ProviderFallback,
                        format!(
                            "explicit thinking mode '{:?}' requested but model does not support \
                             thinking; degrading to ThinkingMode::None",
                            explicit
                        ),
                    );
                    Ok(ThinkingMode::None)
                } else {
                    Err(PipelineError::Provider(format!(
                        "explicit thinking mode '{:?}' requested but model does not support \
                         thinking and allow_degraded_metadata is false",
                        explicit
                    )))
                }
            }
        }
    }

    /// Returns `true` when the model satisfies all capability requirements in
    /// `config.model_selection` (tools, structured output, minimum context).
    ///
    /// Used internally during fallback model search.
    fn satisfies_requirements(&self, model: &ModelMetadata, config: &Config) -> bool {
        if config.model_selection.require_tools && !model.capabilities.supports_tools {
            return false;
        }
        if config.model_selection.require_structured_output
            && !model.capabilities.supports_structured_output
        {
            return false;
        }
        if model.capabilities.context_window_tokens < config.model_selection.min_context_tokens {
            return false;
        }
        true
    }
}

impl Default for ModelResolver {
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
    use crate::diagnostics::DiagnosticLevel;

    // ------------------------------------------------------------------
    // Test helpers
    // ------------------------------------------------------------------

    /// Builds a model with all capabilities enabled and a large context window,
    /// suitable for satisfying the default Config requirements.
    fn make_capable_model(id: &str) -> ModelMetadata {
        ModelMetadata {
            id: id.to_string(),
            display_name: Some(id.to_string()),
            capabilities: ModelCapabilities {
                supports_tools: true,
                supports_structured_output: true,
                supports_thinking: false,
                supports_streaming: true,
                supports_vision: false,
                context_window_tokens: 32_000,
            },
        }
    }

    /// Builds a model that additionally supports extended thinking.
    fn make_thinking_model(id: &str) -> ModelMetadata {
        ModelMetadata {
            id: id.to_string(),
            display_name: Some(id.to_string()),
            capabilities: ModelCapabilities {
                supports_tools: true,
                supports_structured_output: true,
                supports_thinking: true,
                supports_streaming: true,
                supports_vision: false,
                context_window_tokens: 32_000,
            },
        }
    }

    /// Builds a minimal config with relaxed capability requirements, suitable
    /// for tests that only care about model selection logic.
    fn make_permissive_config() -> Config {
        let mut config = Config::default();
        config.model_selection.require_tools = false;
        config.model_selection.require_structured_output = false;
        config.model_selection.min_context_tokens = 0;
        config
    }

    // ------------------------------------------------------------------
    // ResolutionContext::effective_provider
    // ------------------------------------------------------------------

    #[test]
    fn test_resolution_context_effective_provider_prefers_cli_over_config() {
        let ctx = ResolutionContext::new().with_cli(Some("anthropic".to_string()), None);
        assert_eq!(ctx.effective_provider("openai"), "anthropic");
    }

    #[test]
    fn test_resolution_context_effective_provider_falls_through_to_config_default() {
        let ctx = ResolutionContext::new();
        assert_eq!(ctx.effective_provider("ollama"), "ollama");
    }

    #[test]
    fn test_resolution_context_effective_provider_watcher_beats_workflow() {
        let ctx = ResolutionContext::new()
            .with_watcher(Some("anthropic".to_string()), None)
            .with_workflow(Some("ollama".to_string()), None);
        assert_eq!(ctx.effective_provider("openai"), "anthropic");
    }

    #[test]
    fn test_resolution_context_effective_provider_plugin_beats_config_default() {
        let ctx = ResolutionContext::new().with_plugin(Some("copilot".to_string()), None);
        assert_eq!(ctx.effective_provider("openai"), "copilot");
    }

    // ------------------------------------------------------------------
    // ResolutionContext::effective_model_override
    // ------------------------------------------------------------------

    #[test]
    fn test_resolution_context_effective_model_override_returns_cli_model() {
        let ctx = ResolutionContext::new().with_cli(None, Some("gpt-4o".to_string()));
        assert_eq!(ctx.effective_model_override(), Some("gpt-4o".to_string()));
    }

    #[test]
    fn test_resolution_context_effective_model_override_returns_none_when_no_override() {
        let ctx = ResolutionContext::new();
        assert_eq!(ctx.effective_model_override(), None);
    }

    #[test]
    fn test_resolution_context_effective_model_override_cli_beats_watcher() {
        let ctx = ResolutionContext::new()
            .with_cli(None, Some("cli-model".to_string()))
            .with_watcher(None, Some("watcher-model".to_string()));
        assert_eq!(
            ctx.effective_model_override(),
            Some("cli-model".to_string())
        );
    }

    // ------------------------------------------------------------------
    // ModelResolver::resolve_with_static
    // ------------------------------------------------------------------

    #[test]
    fn test_resolve_with_static_selects_requested_model_when_available() {
        let resolver = ModelResolver::new();
        // No override: resolution uses preferred_models[0].
        let mut config = Config::default();
        config.model_selection.preferred_models = vec!["gpt-4.1-mini".to_string()];

        let available = vec![make_capable_model("gpt-4.1-mini")];

        // SAFETY: the model satisfies all default capability requirements.
        let result = resolver
            .resolve_with_static(
                &ResolutionContext::new(),
                &config,
                &available,
                "gpt-4.1-mini",
                MetadataSource::Static,
            )
            .unwrap();

        assert_eq!(result.selected_model, "gpt-4.1-mini");
        assert!(!result.fallback_used);
        assert!(result.fallback_reason.is_none());
    }

    #[test]
    fn test_resolve_with_static_fallback_when_requested_model_unavailable_and_auto_fallback_true() {
        let resolver = ModelResolver::new();
        let ctx = ResolutionContext::new().with_cli(None, Some("nonexistent-model".to_string()));

        let mut config = Config::default();
        config.model_selection.auto_fallback = true;
        config.model_selection.fallback_models = vec!["gpt-4.1".to_string()];

        let available = vec![make_capable_model("gpt-4.1")];

        // SAFETY: fallback model exists and satisfies capability requirements.
        let result = resolver
            .resolve_with_static(
                &ctx,
                &config,
                &available,
                "gpt-4.1-mini",
                MetadataSource::Static,
            )
            .unwrap();

        assert_eq!(result.selected_model, "gpt-4.1");
        assert!(result.fallback_used);
        assert!(result.fallback_reason.is_some());
    }

    #[test]
    fn test_resolve_with_static_fallback_searches_available_when_fallback_list_empty() {
        let resolver = ModelResolver::new();
        let ctx = ResolutionContext::new().with_cli(None, Some("nonexistent-model".to_string()));

        let mut config = make_permissive_config();
        config.model_selection.auto_fallback = true;
        config.model_selection.fallback_models = vec![]; // no explicit fallback list

        let available = vec![make_capable_model("any-available-model")];

        // SAFETY: fallback found by scanning available_models.
        let result = resolver
            .resolve_with_static(
                &ctx,
                &config,
                &available,
                "gpt-4.1-mini",
                MetadataSource::Static,
            )
            .unwrap();

        assert_eq!(result.selected_model, "any-available-model");
        assert!(result.fallback_used);
    }

    #[test]
    fn test_resolve_with_static_error_when_model_unavailable_and_auto_fallback_false() {
        let resolver = ModelResolver::new();
        let ctx = ResolutionContext::new().with_cli(None, Some("nonexistent-model".to_string()));

        let mut config = Config::default();
        config.model_selection.auto_fallback = false;

        let available = vec![make_capable_model("gpt-4.1-mini")];

        let result = resolver.resolve_with_static(
            &ctx,
            &config,
            &available,
            "gpt-4.1-mini",
            MetadataSource::Static,
        );

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Provider(_)));
    }

    #[test]
    fn test_resolve_with_static_error_when_no_fallback_satisfies_requirements() {
        let resolver = ModelResolver::new();
        let ctx = ResolutionContext::new().with_cli(None, Some("nonexistent".to_string()));

        let mut config = Config::default();
        config.model_selection.auto_fallback = true;
        config.model_selection.require_tools = true;
        config.model_selection.fallback_models = vec![];

        // Available model does NOT support tools.
        let available = vec![ModelMetadata::new(
            "weak-model",
            ModelCapabilities::default(),
        )];

        let result = resolver.resolve_with_static(
            &ctx,
            &config,
            &available,
            "default",
            MetadataSource::Static,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_with_static_uses_provider_default_when_no_override() {
        let resolver = ModelResolver::new();
        let ctx = ResolutionContext::new(); // no overrides

        let mut config = make_permissive_config();
        config.model_selection.preferred_models = vec![]; // no preferred models

        let available = vec![make_capable_model("provider-default-model")];

        // SAFETY: the model is in available_models and requirements are relaxed.
        let result = resolver
            .resolve_with_static(
                &ctx,
                &config,
                &available,
                "provider-default-model",
                MetadataSource::Static,
            )
            .unwrap();

        assert_eq!(result.selected_model, "provider-default-model");
        assert!(result.requested_model.is_none());
    }

    // ------------------------------------------------------------------
    // ModelResolver::resolve_thinking_mode
    // ------------------------------------------------------------------

    #[test]
    fn test_resolve_thinking_mode_none_stays_none() {
        let mut diags = Diagnostics::new();
        // SAFETY: ThinkingMode::None always succeeds.
        let result =
            ModelResolver::resolve_thinking_mode(&ThinkingMode::None, false, false, &mut diags)
                .unwrap();
        assert_eq!(result, ThinkingMode::None);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_resolve_thinking_mode_auto_selects_low_for_thinking_capable_model() {
        let mut diags = Diagnostics::new();
        // SAFETY: Auto with supports_thinking=true always succeeds.
        let result =
            ModelResolver::resolve_thinking_mode(&ThinkingMode::Auto, true, false, &mut diags)
                .unwrap();
        assert_eq!(result, ThinkingMode::Low);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_resolve_thinking_mode_auto_selects_none_for_non_thinking_model_and_adds_diagnostic() {
        let mut diags = Diagnostics::new();
        // SAFETY: Auto degrades gracefully regardless of allow_degraded.
        let result =
            ModelResolver::resolve_thinking_mode(&ThinkingMode::Auto, false, false, &mut diags)
                .unwrap();
        assert_eq!(result, ThinkingMode::None);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags.entries[0].level, DiagnosticLevel::Info);
    }

    #[test]
    fn test_resolve_thinking_mode_explicit_level_used_when_model_supports_thinking() {
        let mut diags = Diagnostics::new();
        // SAFETY: explicit level with supports_thinking=true always succeeds.
        let result =
            ModelResolver::resolve_thinking_mode(&ThinkingMode::High, true, false, &mut diags)
                .unwrap();
        assert_eq!(result, ThinkingMode::High);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_resolve_thinking_mode_explicit_errors_when_unsupported_and_no_degraded() {
        let mut diags = Diagnostics::new();
        let result = ModelResolver::resolve_thinking_mode(
            &ThinkingMode::High,
            false, // model does not support thinking
            false, // degraded mode not allowed
            &mut diags,
        );
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PipelineError::Provider(_)));
    }

    #[test]
    fn test_resolve_thinking_mode_explicit_degrades_to_none_when_unsupported_and_degraded_allowed()
    {
        let mut diags = Diagnostics::new();
        // SAFETY: allow_degraded=true ensures no error is returned.
        let result = ModelResolver::resolve_thinking_mode(
            &ThinkingMode::High,
            false, // model does not support thinking
            true,  // degraded mode allowed
            &mut diags,
        )
        .unwrap();
        assert_eq!(result, ThinkingMode::None);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags.entries[0].level, DiagnosticLevel::Warning);
    }

    #[test]
    fn test_resolve_thinking_mode_medium_explicit_errors_without_degraded() {
        let mut diags = Diagnostics::new();
        let result =
            ModelResolver::resolve_thinking_mode(&ThinkingMode::Medium, false, false, &mut diags);
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // ResolvedModel serialization
    // ------------------------------------------------------------------

    #[test]
    fn test_resolved_model_serializes_to_json() {
        let model = ResolvedModel {
            requested_provider: "openai".to_string(),
            selected_provider: "openai".to_string(),
            requested_model: Some("gpt-4o".to_string()),
            selected_model: "gpt-4o".to_string(),
            fallback_used: false,
            fallback_reason: None,
            capabilities: ModelCapabilities::default(),
            thinking_mode_requested: ThinkingMode::None,
            thinking_mode_selected: ThinkingMode::None,
            metadata_source: MetadataSource::Static,
            diagnostics: Diagnostics::new(),
        };

        // SAFETY: serialization of well-formed in-memory data cannot fail.
        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains("\"selected_provider\":\"openai\""));
        assert!(json.contains("\"selected_model\":\"gpt-4o\""));
        assert!(json.contains("\"fallback_used\":false"));
        assert!(json.contains("\"requested_model\":\"gpt-4o\""));
    }

    #[test]
    fn test_resolved_model_with_fallback_serializes_correctly() {
        let model = ResolvedModel {
            requested_provider: "openai".to_string(),
            selected_provider: "openai".to_string(),
            requested_model: Some("gpt-5".to_string()),
            selected_model: "gpt-4o".to_string(),
            fallback_used: true,
            fallback_reason: Some("gpt-5 unavailable".to_string()),
            capabilities: ModelCapabilities::default(),
            thinking_mode_requested: ThinkingMode::None,
            thinking_mode_selected: ThinkingMode::None,
            metadata_source: MetadataSource::Static,
            diagnostics: Diagnostics::new(),
        };

        // SAFETY: serialization of well-formed in-memory data cannot fail.
        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains("\"fallback_used\":true"));
        assert!(json.contains("gpt-5 unavailable"));
    }

    // ------------------------------------------------------------------
    // ModelResolver::Default
    // ------------------------------------------------------------------

    #[test]
    fn test_model_resolver_default_creates_successfully() {
        let _resolver = ModelResolver::new();
    }

    // ------------------------------------------------------------------
    // thinking_mode in ResolvedModel
    // ------------------------------------------------------------------

    #[test]
    fn test_resolve_with_static_thinking_model_selects_low_for_auto_mode() {
        let resolver = ModelResolver::new();
        let ctx = ResolutionContext::new()
            .with_thinking_mode(ThinkingMode::Auto)
            .with_cli(None, Some("thinking-model".to_string()));

        let mut config = make_permissive_config();
        config.model_selection.preferred_models = vec![];

        let available = vec![make_thinking_model("thinking-model")];

        // SAFETY: model supports thinking, Auto resolves to Low.
        let result = resolver
            .resolve_with_static(
                &ctx,
                &config,
                &available,
                "thinking-model",
                MetadataSource::Static,
            )
            .unwrap();

        assert_eq!(result.thinking_mode_selected, ThinkingMode::Low);
        assert_eq!(result.thinking_mode_requested, ThinkingMode::Auto);
    }

    #[test]
    fn test_resolve_with_static_uses_static_metadata_when_remote_unavailable_and_degraded_allowed()
    {
        let resolver = ModelResolver::new();
        let ctx = ResolutionContext::new().with_cli(None, Some("my-model".to_string()));

        let mut config = make_permissive_config();
        config.model_metadata.allow_degraded_metadata = true;
        config.model_selection.auto_fallback = false;

        let available = vec![make_capable_model("my-model")];

        // MetadataSource::Degraded indicates remote metadata was unavailable
        // and static metadata is being used as a fallback.
        // SAFETY: model is available in static list, requirements are permissive.
        let result = resolver
            .resolve_with_static(
                &ctx,
                &config,
                &available,
                "my-model",
                MetadataSource::Degraded, // signals static fallback was used
            )
            .unwrap();

        assert_eq!(
            result.metadata_source,
            MetadataSource::Degraded,
            "metadata_source should reflect that degraded static metadata was used"
        );
        assert_eq!(result.selected_model, "my-model");
        assert!(
            !result.fallback_used,
            "fallback_used should be false when the requested model was found"
        );
    }
}
