//! Runtime context for plugin execution.
//!
//! [`PluginContext`] carries all pipeline resources a plugin needs during its
//! run: configuration, workspace, scan data, AI provider, tool registry,
//! governance, diagnostics, and optional watcher task metadata.
//!
//! [`ToolAccessLevel`] declares what level of filesystem access a plugin
//! requires, enabling the runner to configure the sandbox appropriately before
//! handing control to the plugin.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::agent::context::AgentContext;
use crate::agent::session::AgentSession;
use crate::config::Config;
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::error::{PipelineError, Result};
use crate::governance::GovernanceChecker;
use crate::prompts::PromptLoader;
use crate::providers::base::Provider;
use crate::providers::types::Message;
use crate::scanner::result::ScanResult;
use crate::tools::registry::ToolRegistry;
use crate::workspace::WorkspaceManager;
use crate::workspace::state::WorkspaceState;

// ---------------------------------------------------------------------------
// ToolAccessLevel
// ---------------------------------------------------------------------------

/// Access level required by a plugin for file system tools.
///
/// Declared by [`WorkflowPlugin::required_tool_access`][crate::plugins::trait_def::WorkflowPlugin::required_tool_access]
/// so that the plugin runner can configure the sandbox before handing control
/// to the plugin.
///
/// # Examples
///
/// ```
/// use xzardgz::plugins::context::ToolAccessLevel;
///
/// assert_eq!(ToolAccessLevel::None, ToolAccessLevel::None);
/// assert_ne!(ToolAccessLevel::ReadOnly, ToolAccessLevel::ReadWrite);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolAccessLevel {
    /// No file system tool access required.
    None,
    /// Read-only file system access.
    ReadOnly,
    /// Full read-write file system access.
    ReadWrite,
}

// ---------------------------------------------------------------------------
// PluginContext
// ---------------------------------------------------------------------------

/// Runtime context passed to a plugin when it executes.
///
/// Provides access to all pipeline resources: config, workspace, scan data,
/// AI provider, tool registry, governance, diagnostics, and optional watcher
/// task metadata.
///
/// The context is constructed by the plugin runner and passed by value to
/// [`WorkflowPlugin::run`][crate::plugins::trait_def::WorkflowPlugin::run].
/// Plugins own the context for the duration of their execution.
///
/// Use [`PluginContext::new`] to create an instance, and the builder methods
/// [`with_watcher_task_id`][Self::with_watcher_task_id] and
/// [`with_prompts`][Self::with_prompts] to attach optional in-memory prompt
/// overrides.
pub struct PluginContext {
    /// Effective pipeline configuration.
    pub config: Arc<Config>,
    /// Workspace manager owning the paths and state.
    pub workspace: Arc<WorkspaceManager>,
    /// Current workspace state snapshot at the time of plugin invocation.
    pub state: WorkspaceState,
    /// Repository scan result consumed by this plugin.
    pub scan_result: ScanResult,
    /// AI provider to use for completions.
    pub provider: Arc<dyn Provider + Send + Sync>,
    /// Tool registry available to this plugin.
    pub tool_registry: ToolRegistry,
    /// Governance checker for policy enforcement.
    pub governance: GovernanceChecker,
    /// Diagnostics collector for this plugin run.
    pub diagnostics: Diagnostics,
    /// Watcher task identifier, if this run was triggered by a watcher message.
    pub watcher_task_id: Option<String>,
    /// Prompt template loader providing three-level resolution (in-memory,
    /// file-based, embedded default) via [`PromptLoader::render`].
    pub prompt_loader: PromptLoader,
}

impl PluginContext {
    /// Creates a new `PluginContext` with empty diagnostics, no watcher task
    /// ID, and empty prompts.
    ///
    /// # Arguments
    ///
    /// * `config` - Effective pipeline configuration.
    /// * `workspace` - Workspace manager owning paths and state.
    /// * `state` - Current workspace state snapshot.
    /// * `scan_result` - Repository scan result for this run.
    /// * `provider` - AI provider for completions.
    /// * `tool_registry` - Tool registry available to the plugin.
    /// * `governance` - Governance checker for policy enforcement.
    ///
    /// # Returns
    ///
    /// A new `PluginContext` with:
    /// - `diagnostics = Diagnostics::new()`
    /// - `watcher_task_id = None`
    /// - `prompt_loader = PromptLoader::new(config.prompts.clone())`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use xzardgz::plugins::context::PluginContext;
    /// # use xzardgz::config::Config;
    /// // Construct a PluginContext for a plugin run.
    /// // All resources are provided by the pipeline runner.
    /// ```
    pub fn new(
        config: Arc<Config>,
        workspace: Arc<WorkspaceManager>,
        state: WorkspaceState,
        scan_result: ScanResult,
        provider: Arc<dyn Provider + Send + Sync>,
        tool_registry: ToolRegistry,
        governance: GovernanceChecker,
    ) -> Self {
        let prompts_cfg = config.prompts.clone();
        Self {
            config,
            workspace,
            state,
            scan_result,
            provider,
            tool_registry,
            governance,
            diagnostics: Diagnostics::new(),
            watcher_task_id: None,
            prompt_loader: PromptLoader::new(prompts_cfg),
        }
    }

    /// Sets the watcher task ID on this context and returns `self`.
    ///
    /// Used in a builder pattern to attach the watcher task identifier when
    /// a plugin run was triggered by a watcher message.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The watcher task identifier string.
    ///
    /// # Returns
    ///
    /// `self` with `watcher_task_id` set.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use xzardgz::plugins::context::PluginContext;
    /// // let ctx = PluginContext::new(...).with_watcher_task_id("task-001");
    /// ```
    pub fn with_watcher_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.watcher_task_id = Some(task_id.into());
        self
    }

    /// Sets in-memory prompt overrides on this context and returns `self`.
    ///
    /// Override keys must use the `"{plugin}/{key}"` format, e.g.
    /// `"security_review/system"`. Values are raw Tera template strings.
    /// In-memory overrides take the highest priority in the resolution chain
    /// (above file-based overrides and embedded defaults).
    ///
    /// This method is primarily intended for testing, where injecting a known
    /// template string is preferable to writing temporary files.
    ///
    /// # Arguments
    ///
    /// * `overrides` - Map of `"{plugin}/{key}"` to raw Tera template content.
    ///
    /// # Returns
    ///
    /// `self` with the in-memory overrides installed on `prompt_loader`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::collections::HashMap;
    /// # use xzardgz::plugins::context::PluginContext;
    /// // let mut overrides = HashMap::new();
    /// // overrides.insert("security_review/system".to_string(), "Custom.".to_string());
    /// // let ctx = PluginContext::new(...).with_prompts(overrides);
    /// ```
    pub fn with_prompts(mut self, overrides: HashMap<String, String>) -> Self {
        self.prompt_loader = self.prompt_loader.with_in_memory_overrides(overrides);
        self
    }

    /// Appends a diagnostic entry to the context's collector.
    ///
    /// # Arguments
    ///
    /// * `diagnostic` - The [`Diagnostic`] to append.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use xzardgz::diagnostics::{Diagnostic, DiagnosticCategory};
    /// # use xzardgz::plugins::context::PluginContext;
    /// // ctx.add_diagnostic(Diagnostic::warning(DiagnosticCategory::Plugin, "issue"));
    /// ```
    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Returns the workspace ID from the current state snapshot.
    ///
    /// Delegates to `self.state.workspace_id`.
    ///
    /// # Returns
    ///
    /// A `&str` slice of the workspace ID (a ULID string).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use xzardgz::plugins::context::PluginContext;
    /// // assert!(!ctx.workspace_id().is_empty());
    /// ```
    pub fn workspace_id(&self) -> &str {
        &self.state.workspace_id
    }

    /// Constructs an [`AgentSession`] from the provider and tool registry
    /// carried by this context.
    ///
    /// This is the sole entry point for starting a multi-turn, tool-augmented
    /// agent loop from within a plugin.  The method enforces the hard
    /// requirement that tool calling is mandatory: it returns
    /// [`PipelineError::Provider`] immediately when
    /// `provider.metadata().capabilities.tools` is `false`, so no single-shot
    /// fallback path exists.
    ///
    /// Calling this method moves the `tool_registry` out of `self` (replaced
    /// with an empty registry), so it may be called at most once per context.
    ///
    /// # Arguments
    ///
    /// * `system_prompt` - System instruction pre-seeded into the session
    ///   context before the first provider call.
    /// * `max_tokens` - Upper bound on the estimated token budget for the
    ///   session context window.
    /// * `max_turns` - Maximum number of provider-tool loop turns before the
    ///   session returns [`PipelineError::Agent`].
    ///
    /// # Returns
    ///
    /// An [`AgentSession`] ready to accept a user prompt via
    /// [`AgentSession::run`].
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Provider`] when the provider does not support
    /// tool calling (`capabilities.tools == false`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use xzardgz::plugins::context::PluginContext;
    /// // let session = ctx.build_agent_session(system_prompt, 8192, 15)?;
    /// // let response = session.run(&user_prompt).await?;
    /// ```
    pub fn build_agent_session(
        &mut self,
        system_prompt: String,
        max_tokens: usize,
        max_turns: usize,
    ) -> Result<AgentSession> {
        if !self.provider.metadata().capabilities.tools {
            return Err(PipelineError::Provider(
                "this provider does not support tool calling".to_string(),
            ));
        }
        let mut context = AgentContext::new(system_prompt.clone(), max_tokens);
        // Pre-seed the system message so every provider completion has the
        // correct persona and output-format instruction from the first turn.
        context.add_message(Message::system(system_prompt));
        // Move the tool registry out of this context; the session takes
        // ownership of all tool definitions and executors.
        let registry = std::mem::take(&mut self.tool_registry);
        Ok(
            AgentSession::new(Arc::clone(&self.provider), context, registry)
                .with_max_turns(max_turns),
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, GovernanceConfig};
    use crate::diagnostics::DiagnosticCategory;
    use crate::providers::base::MockProvider;
    use crate::scanner::result::{PluginPreselection, SCAN_RESULT_VERSION, ScanResult};
    use chrono::Utc;
    use std::collections::HashMap;

    /// Returns a [`GovernanceConfig`] safe for use in tests.
    ///
    /// Uses an empty `rules_path` so the loader falls back to embedded defaults
    /// rather than attempting to parse the project-root `AGENTS.md` file, which
    /// contains Markdown (not YAML) and would cause a parse error.
    fn test_governance_config() -> GovernanceConfig {
        GovernanceConfig {
            enabled: false,
            rules_path: String::new(),
            fail_on_violation: false,
        }
    }

    /// Builds a minimal [`ScanResult`] suitable for use in context tests.
    fn make_scan_result() -> ScanResult {
        ScanResult {
            version: SCAN_RESULT_VERSION.to_string(),
            repository_url: None,
            repository_name: Some("test-repo".to_string()),
            head_commit: None,
            scan_timestamp: Utc::now(),
            repository_structure: vec![],
            language_statistics: HashMap::new(),
            primary_language: None,
            frameworks: vec![],
            documentation_inventory: vec![],
            governance_rules: vec![],
            cli_commands: vec![],
            public_apis: vec![],
            entrypoints: vec![],
            config_surface: vec![],
            key_files: vec![],
            dependency_manifests: vec![],
            test_files: vec![],
            build_files: vec![],
            security_relevant_files: vec![],
            findings: vec![],
            plugin_preselection: PluginPreselection::default(),
        }
    }

    /// Builds a [`PluginContext`] rooted at `root` for use in tests.
    ///
    /// The caller must keep the `tempfile::TempDir` alive for the duration of
    /// any test that exercises filesystem-touching workspace operations.
    /// For the context tests here, all assertions are in-memory only.
    fn make_context_with_root(root: &str) -> PluginContext {
        // SAFETY: WorkspaceManager::create only fails on I/O errors; temp dirs
        // are always writable in a standard test environment.
        let manager = WorkspaceManager::create(root, "test://repo", None, None, None).unwrap();
        let state = manager.state.clone();
        let workspace = Arc::new(manager);
        let config = Arc::new(Config::default());
        let provider: Arc<dyn Provider + Send + Sync> = Arc::new(MockProvider::new());
        let tool_registry = ToolRegistry::new();
        // SAFETY: GovernanceChecker::from_config with empty rules_path cannot fail.
        let governance = GovernanceChecker::from_config(&test_governance_config()).unwrap();
        PluginContext::new(
            config,
            workspace,
            state,
            make_scan_result(),
            provider,
            tool_registry,
            governance,
        )
    }

    // ------------------------------------------------------------------
    // ToolAccessLevel
    // ------------------------------------------------------------------

    #[test]
    fn test_tool_access_level_variants_not_equal() {
        assert_ne!(ToolAccessLevel::None, ToolAccessLevel::ReadOnly);
        assert_ne!(ToolAccessLevel::ReadOnly, ToolAccessLevel::ReadWrite);
        assert_ne!(ToolAccessLevel::None, ToolAccessLevel::ReadWrite);
    }

    #[test]
    fn test_tool_access_level_same_variant_is_equal() {
        assert_eq!(ToolAccessLevel::None, ToolAccessLevel::None);
        assert_eq!(ToolAccessLevel::ReadOnly, ToolAccessLevel::ReadOnly);
        assert_eq!(ToolAccessLevel::ReadWrite, ToolAccessLevel::ReadWrite);
    }

    // ------------------------------------------------------------------
    // PluginContext::new
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_context_new_sets_empty_diagnostics() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let ctx = make_context_with_root(tmp.path().to_str().unwrap());
        assert!(ctx.diagnostics.is_empty());
        assert!(!ctx.prompt_loader.has_in_memory_overrides());
        assert!(ctx.watcher_task_id.is_none());
    }

    // ------------------------------------------------------------------
    // workspace_id
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_context_workspace_id_delegates_to_state() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let ctx = make_context_with_root(tmp.path().to_str().unwrap());
        assert!(!ctx.workspace_id().is_empty());
        assert_eq!(ctx.workspace_id(), ctx.state.workspace_id.as_str());
    }

    // ------------------------------------------------------------------
    // with_watcher_task_id
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_context_with_watcher_task_id_sets_id() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let ctx = make_context_with_root(tmp.path().to_str().unwrap())
            .with_watcher_task_id("task-abc-123");
        assert_eq!(ctx.watcher_task_id, Some("task-abc-123".to_string()));
    }

    // ------------------------------------------------------------------
    // with_prompts
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_context_with_prompts_sets_in_memory_overrides() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let mut overrides = HashMap::new();
        overrides.insert(
            "security_review/system".to_string(),
            "Overridden system prompt.".to_string(),
        );
        let ctx = make_context_with_root(tmp.path().to_str().unwrap()).with_prompts(overrides);
        assert!(
            ctx.prompt_loader.has_in_memory_overrides(),
            "with_prompts must install in-memory overrides on prompt_loader"
        );
        let result = ctx
            .prompt_loader
            .render("security_review", "system", &tera::Context::new());
        assert_eq!(result, "Overridden system prompt.");
    }

    // ------------------------------------------------------------------
    // add_diagnostic
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_context_add_diagnostic_increases_count() {
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let mut ctx = make_context_with_root(tmp.path().to_str().unwrap());
        assert_eq!(ctx.diagnostics.len(), 0);
        ctx.add_diagnostic(Diagnostic::warning(
            DiagnosticCategory::Plugin,
            "test warning",
        ));
        assert_eq!(ctx.diagnostics.len(), 1);
    }

    // ------------------------------------------------------------------
    // build_agent_session
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_context_build_agent_session_returns_err_when_provider_lacks_tools() {
        use crate::providers::types::{ProviderCapabilities, ProviderMetadata};
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let manager = crate::workspace::WorkspaceManager::create(
            tmp.path().to_str().unwrap(),
            "test://repo",
            None,
            None,
            None,
        )
        .unwrap();
        let state = manager.state.clone();
        let workspace = std::sync::Arc::new(manager);
        let config = std::sync::Arc::new(Config::default());
        let mut mock = MockProvider::new();
        mock.expect_metadata().returning(|| ProviderMetadata {
            name: "mock-no-tools".to_string(),
            models: vec![],
            capabilities: ProviderCapabilities {
                streaming: false,
                tools: false,
                vision: false,
            },
        });
        let provider: std::sync::Arc<dyn crate::providers::base::Provider + Send + Sync> =
            std::sync::Arc::new(mock);
        let tool_registry = ToolRegistry::new();
        let governance =
            crate::governance::GovernanceChecker::from_config(&test_governance_config()).unwrap();
        let mut ctx = PluginContext::new(
            config,
            workspace,
            state,
            make_scan_result(),
            provider,
            tool_registry,
            governance,
        );
        let result = ctx.build_agent_session("system".to_string(), 4096, 15);
        assert!(
            result.is_err(),
            "expected an error when provider does not support tools"
        );
        let msg = result.err().expect("expected Err").to_string();
        assert!(
            msg.contains("tool calling"),
            "error must mention tool calling, got: {msg}"
        );
    }

    #[test]
    fn test_plugin_context_build_agent_session_returns_ok_when_provider_supports_tools() {
        use crate::providers::types::{ProviderCapabilities, ProviderMetadata};
        // SAFETY: TempDir::new() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let manager = crate::workspace::WorkspaceManager::create(
            tmp.path().to_str().unwrap(),
            "test://repo",
            None,
            None,
            None,
        )
        .unwrap();
        let state = manager.state.clone();
        let workspace = std::sync::Arc::new(manager);
        let config = std::sync::Arc::new(Config::default());
        let mut mock = MockProvider::new();
        mock.expect_metadata().returning(|| ProviderMetadata {
            name: "mock-with-tools".to_string(),
            models: vec![],
            capabilities: ProviderCapabilities {
                streaming: false,
                tools: true,
                vision: false,
            },
        });
        let provider: std::sync::Arc<dyn crate::providers::base::Provider + Send + Sync> =
            std::sync::Arc::new(mock);
        let tool_registry = ToolRegistry::new();
        let governance =
            crate::governance::GovernanceChecker::from_config(&test_governance_config()).unwrap();
        let mut ctx = PluginContext::new(
            config,
            workspace,
            state,
            make_scan_result(),
            provider,
            tool_registry,
            governance,
        );
        let result = ctx.build_agent_session("system".to_string(), 4096, 15);
        assert!(
            result.is_ok(),
            "expected Ok(AgentSession) when provider supports tools"
        );
    }
}
