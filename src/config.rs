//! Configuration system for XZardgz.
//!
//! This module defines the complete configuration hierarchy for the XZardgz
//! autonomous agent pipeline.  Configuration is loaded from a YAML file
//! (default: `config.yaml`), with environment variable overrides applied on
//! top.  Programmatic overrides are available via [`ConfigOverrides`] and
//! model-selection tuning via [`ModelSelectionOverrides`].
//!
//! # Loading order
//!
//! 1. Start with `Config::default()` (compiled-in defaults).
//! 2. If a config file exists, parse it and merge via `#[serde(default)]`.
//! 3. Apply environment variable overrides.
//! 4. Validate the resulting configuration.
//! 5. Optionally apply [`ConfigOverrides`] for CLI / programmatic overrides.

use crate::error::{PipelineError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Top-level Config
// ---------------------------------------------------------------------------

/// Top-level application configuration.
///
/// All fields carry `#[serde(default)]` so that a partial YAML file is valid;
/// any missing section falls back to the compiled-in [`Default`] value.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// AI provider selection and fallback settings.
    #[serde(default)]
    pub provider: ProviderTopConfig,
    /// Shared defaults applied across all providers unless overridden.
    #[serde(default)]
    pub provider_defaults: ProviderDefaultsConfig,
    /// OpenAI-specific configuration.
    #[serde(default)]
    pub openai: OpenAiConfig,
    /// Anthropic-specific configuration.
    #[serde(default)]
    pub anthropic: AnthropicConfig,
    /// Ollama local model server configuration.
    #[serde(default)]
    pub ollama: OllamaConfig,
    /// GitHub Copilot provider configuration.
    #[serde(default)]
    pub copilot: CopilotConfig,
    /// Repository scanner configuration.
    #[serde(default)]
    pub scanner: ScannerConfig,
    /// Git operation configuration.
    #[serde(default)]
    pub git: GitConfig,
    /// Workspace management configuration.
    #[serde(default)]
    pub workspace: WorkspaceConfig,
    /// Plugin registry configuration.
    #[serde(default)]
    pub plugins: PluginsConfig,
    /// Technical review plugin configuration.
    #[serde(default)]
    pub technical_review: TechnicalReviewConfig,
    /// Security review plugin configuration.
    #[serde(default)]
    pub security_review: SecurityReviewConfig,
    /// Governance rule enforcement configuration.
    #[serde(default)]
    pub governance: GovernanceConfig,
    /// Kafka watcher configuration.
    #[serde(default)]
    pub watcher: WatcherConfig,
    /// Kafka broker and security configuration.
    #[serde(default)]
    pub kafka: KafkaConfig,
    /// Kafka topic names used by the watcher.
    #[serde(default)]
    pub topics: TopicsConfig,
    /// Event and repository routing matcher.
    #[serde(default)]
    pub matcher: MatcherConfig,
    /// Prompt discovery and override configuration.
    #[serde(default)]
    pub prompts: PromptsConfig,
    /// Sub-agent delegation configuration.
    #[serde(default)]
    pub subagent: SubagentConfig,
    /// Trace transcript recording configuration.
    #[serde(default)]
    pub trace_transcript: TraceTranscriptConfig,
    /// Scan output file configuration.
    #[serde(default)]
    pub scan_output: ScanOutputConfig,
    /// Report generation configuration.
    #[serde(default)]
    pub reports: ReportsConfig,
    /// Model Context Protocol (MCP) server configuration.
    #[serde(default)]
    pub mcp: McpConfig,
    /// Model metadata caching configuration.
    #[serde(default)]
    pub model_metadata: ModelMetadataConfig,
    /// Automatic model selection configuration.
    #[serde(default)]
    pub model_selection: ModelSelectionConfig,
    /// Project identity metadata.
    #[serde(default)]
    pub project: ProjectConfig,
}

impl Config {
    /// Loads configuration from `config.yaml` in the current working directory.
    ///
    /// Equivalent to calling `Config::load_from_path(Path::new("config.yaml"))`.
    /// If the file is absent, returns `Config::default()` without error.
    pub fn load() -> Result<Self> {
        Self::load_from_path(std::path::Path::new("config.yaml"))
    }

    /// Loads configuration from the given file path.
    ///
    /// If `path` does not exist, returns `Config::default()` without error.
    /// Any read or parse failure is reported as `PipelineError::Config`.
    pub fn load_from_path(path: &std::path::Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| PipelineError::Config(format!("failed to read config file: {}", e)))?;
        Self::load_from_str(&content)
    }

    /// Parses configuration from a YAML string.
    ///
    /// # Steps
    ///
    /// 1. Parse raw YAML and reject any legacy top-level keys
    ///    (`documentation`, `export_scan`).
    /// 2. Deserialize into [`Config`] using `serde_yaml`.
    /// 3. Apply environment variable overrides.
    /// 4. Validate the resulting configuration.
    ///
    /// Returns `PipelineError::Config` on any failure.
    pub fn load_from_str(content: &str) -> Result<Self> {
        // Step 1: parse raw YAML for legacy key detection.
        let raw: serde_yaml::Value = serde_yaml::from_str(content)
            .map_err(|e| PipelineError::Config(format!("invalid YAML: {}", e)))?;

        if let serde_yaml::Value::Mapping(ref map) = raw {
            for key in ["documentation", "export_scan"] {
                let yaml_key = serde_yaml::Value::String(key.to_string());
                if map.contains_key(&yaml_key) {
                    return Err(PipelineError::Config(format!(
                        "legacy config field '{}' is no longer supported; \
                         remove it from your config file",
                        key
                    )));
                }
            }
        }

        // Step 2: full deserialisation.
        let mut config: Config = serde_yaml::from_str(content)
            .map_err(|e| PipelineError::Config(format!("failed to parse config: {}", e)))?;

        // Step 3: environment variable overrides.
        config.apply_env_overrides();

        // Step 4: validate.
        config.validate()?;

        Ok(config)
    }

    /// Applies environment variable overrides to fields in this configuration.
    ///
    /// Recognised variables:
    ///
    /// | Variable                  | Target field            |
    /// |---------------------------|-------------------------|
    /// | `XZARDGZ_PROVIDER`        | `provider.default`      |
    /// | `XZARDGZ_OPENAI_ENDPOINT` | `openai.endpoint`       |
    /// | `XZARDGZ_OPENAI_MODEL`    | `openai.model`          |
    /// | `XZARDGZ_OLLAMA_HOST`     | `ollama.host`           |
    /// | `XZARDGZ_OLLAMA_MODEL`    | `ollama.model`          |
    fn apply_env_overrides(&mut self) {
        if let Ok(val) = std::env::var("XZARDGZ_PROVIDER") {
            self.provider.default = val;
        }
        if let Ok(val) = std::env::var("XZARDGZ_OPENAI_ENDPOINT") {
            self.openai.endpoint = val;
        }
        if let Ok(val) = std::env::var("XZARDGZ_OPENAI_MODEL") {
            self.openai.model = val;
        }
        if let Ok(val) = std::env::var("XZARDGZ_OLLAMA_HOST") {
            self.ollama.host = val;
        }
        if let Ok(val) = std::env::var("XZARDGZ_OLLAMA_MODEL") {
            self.ollama.model = val;
        }
    }

    /// Validates the configuration and returns an error for any invalid value.
    ///
    /// # Checks
    ///
    /// 1. `provider.default` must be one of `"openai"`, `"anthropic"`,
    ///    `"ollama"`, or `"copilot"`.
    /// 2. `openai.endpoint` must start with `"https://"` unless
    ///    `openai.allow_insecure_endpoint` is `true`.
    /// 3. `model_selection.min_context_tokens` must be greater than zero.
    /// 4. Every entry in `mcp.servers` must have `timeout_seconds > 0`.
    pub fn validate(&self) -> Result<()> {
        // Check 1: valid provider name.
        const VALID_PROVIDERS: [&str; 4] = ["openai", "anthropic", "ollama", "copilot"];
        if !VALID_PROVIDERS.contains(&self.provider.default.as_str()) {
            return Err(PipelineError::Config(format!(
                "unknown provider '{}'; must be one of: openai, anthropic, ollama, copilot",
                self.provider.default
            )));
        }

        // Check 2: HTTPS enforcement for the OpenAI endpoint.
        if !self.openai.allow_insecure_endpoint && !self.openai.endpoint.starts_with("https://") {
            return Err(PipelineError::Config(format!(
                "openai endpoint '{}' requires HTTPS; \
                 set allow_insecure_endpoint: true to permit insecure endpoints",
                self.openai.endpoint
            )));
        }

        // Check 3: min_context_tokens must be non-zero.
        if self.model_selection.min_context_tokens == 0 {
            return Err(PipelineError::Config(
                "model_selection.min_context_tokens must be greater than zero".to_string(),
            ));
        }

        // Check 4: MCP server timeouts must be non-zero.
        for server in &self.mcp.servers {
            if server.timeout_seconds == 0 {
                return Err(PipelineError::Config(format!(
                    "mcp server '{}' has timeout_seconds = 0; must be greater than zero",
                    server.name
                )));
            }
        }

        Ok(())
    }

    /// Applies CLI or programmatic overrides from a [`ConfigOverrides`] value.
    ///
    /// Only `Some` fields are applied; `None` fields leave the existing
    /// configuration unchanged.
    pub fn apply_overrides(&mut self, overrides: &ConfigOverrides) {
        if let Some(ref val) = overrides.provider {
            self.provider.default = val.clone();
        }
        if let Some(ref val) = overrides.openai_model {
            self.openai.model = val.clone();
        }
        if let Some(ref val) = overrides.ollama_model {
            self.ollama.model = val.clone();
        }
        if let Some(ref val) = overrides.ollama_host {
            self.ollama.host = val.clone();
        }
        if let Some(ref val) = overrides.workspace_root {
            self.workspace.root = val.clone();
        }
    }

    /// Merges model-selection overrides into `self.model_selection`.
    ///
    /// Only `Some` fields are applied; `None` fields leave the existing
    /// model selection configuration unchanged.
    pub fn merge_model_selection(&mut self, overrides: &ModelSelectionOverrides) {
        if let Some(ref val) = overrides.preferred_models {
            self.model_selection.preferred_models = val.clone();
        }
        if let Some(ref val) = overrides.fallback_models {
            self.model_selection.fallback_models = val.clone();
        }
        if let Some(val) = overrides.auto_fallback {
            self.model_selection.auto_fallback = val;
        }
        if let Some(val) = overrides.require_tools {
            self.model_selection.require_tools = val;
        }
        if let Some(val) = overrides.require_structured_output {
            self.model_selection.require_structured_output = val;
        }
        if let Some(val) = overrides.min_context_tokens {
            self.model_selection.min_context_tokens = val;
        }
    }
}

// ---------------------------------------------------------------------------
// Sub-structs
// ---------------------------------------------------------------------------

/// AI provider selection and fallback settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTopConfig {
    /// The default provider to use (`"openai"`, `"anthropic"`, `"ollama"`, `"copilot"`).
    #[serde(default)]
    pub default: String,
    /// Whether to automatically fall back to another provider on failure.
    #[serde(default)]
    pub allow_fallback: bool,
}

impl Default for ProviderTopConfig {
    fn default() -> Self {
        Self {
            default: "openai".to_string(),
            allow_fallback: true,
        }
    }
}

/// Shared defaults applied across all providers unless overridden per-provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDefaultsConfig {
    /// Sampling temperature for completions.
    #[serde(default)]
    pub temperature: f32,
    /// Request timeout in seconds.
    #[serde(default)]
    pub timeout_seconds: u64,
    /// Maximum number of retries on transient errors.
    #[serde(default)]
    pub max_retries: u32,
    /// Maximum tokens to generate per request.
    #[serde(default)]
    pub max_tokens: u32,
}

impl Default for ProviderDefaultsConfig {
    fn default() -> Self {
        Self {
            temperature: 0.2,
            timeout_seconds: 120,
            max_retries: 2,
            max_tokens: 4096,
        }
    }
}

/// OpenAI provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    /// Environment variable name that holds the OpenAI API key.
    #[serde(default)]
    pub api_key_env: String,
    /// Model identifier to use with the OpenAI API.
    #[serde(default)]
    pub model: String,
    /// Base URL for the OpenAI-compatible API endpoint.
    #[serde(default)]
    pub endpoint: String,
    /// When `false`, the endpoint must use `https://`.
    #[serde(default)]
    pub allow_insecure_endpoint: bool,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key_env: "OPENAI_API_KEY".to_string(),
            model: "gpt-4.1-mini".to_string(),
            endpoint: "https://api.openai.com/v1".to_string(),
            allow_insecure_endpoint: false,
        }
    }
}

/// Anthropic provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    /// Environment variable name that holds the Anthropic API key.
    #[serde(default)]
    pub api_key_env: String,
    /// Model identifier to use with the Anthropic API.
    #[serde(default)]
    pub model: String,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key_env: "ANTHROPIC_API_KEY".to_string(),
            model: "claude-3-5-sonnet-latest".to_string(),
        }
    }
}

/// Ollama local model server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Base URL of the Ollama server.
    #[serde(default)]
    pub host: String,
    /// Model identifier to use with Ollama.
    #[serde(default)]
    pub model: String,
    /// Context length in tokens for the model.
    #[serde(default)]
    pub context_length: u32,
    /// Optional per-request timeout override in seconds.
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    /// Optional per-request temperature override.
    #[serde(default)]
    pub temperature: Option<f32>,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
            context_length: 32768,
            timeout_seconds: None,
            temperature: None,
        }
    }
}

/// GitHub Copilot provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotConfig {
    /// Model identifier to use with the Copilot API.
    #[serde(default)]
    pub model: String,
    /// Credential store backend identifier (e.g. `"keychain"`).
    #[serde(default)]
    pub auth_store: String,
}

impl Default for CopilotConfig {
    fn default() -> Self {
        Self {
            model: "gpt-4o".to_string(),
            auth_store: "keychain".to_string(),
        }
    }
}

/// Repository scanner configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    /// Whether to include hidden files and directories in the scan.
    #[serde(default)]
    pub include_hidden: bool,
    /// Whether to follow symbolic links during scanning.
    #[serde(default)]
    pub follow_symlinks: bool,
    /// Maximum file size in bytes to include in the scan.
    #[serde(default)]
    pub max_file_size_bytes: u64,
    /// Glob patterns for paths to exclude from the scan.
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            include_hidden: false,
            follow_symlinks: false,
            max_file_size_bytes: 1_048_576,
            ignore_patterns: vec![
                "target".to_string(),
                ".git".to_string(),
                "node_modules".to_string(),
            ],
        }
    }
}

/// Git operation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    /// Depth for shallow clones.
    #[serde(default)]
    pub clone_depth: u32,
    /// Whether to fetch tags during clone/fetch operations.
    #[serde(default)]
    pub fetch_tags: bool,
    /// Whether to run `git clean` before each pipeline run.
    #[serde(default)]
    pub clean_before_run: bool,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            clone_depth: 1,
            fetch_tags: false,
            clean_before_run: false,
        }
    }
}

/// Workspace management configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Root directory for workspace storage.
    #[serde(default)]
    pub root: String,
    /// Whether to resume an existing workspace on restart.
    #[serde(default)]
    pub resume: bool,
    /// Whether to retain workspaces for failed runs.
    #[serde(default)]
    pub keep_failed: bool,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            root: ".xzardgz/workspaces".to_string(),
            resume: true,
            keep_failed: true,
        }
    }
}

/// Plugin registry configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginsConfig {
    /// Default plugin to execute when none is specified.
    #[serde(default)]
    pub default: String,
    /// List of plugins to activate for each pipeline run.
    #[serde(default)]
    pub enabled: Vec<String>,
}

impl Default for PluginsConfig {
    fn default() -> Self {
        Self {
            default: "technical-review".to_string(),
            enabled: vec![
                "technical-review".to_string(),
                "security-review".to_string(),
            ],
        }
    }
}

/// Technical review plugin configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalReviewConfig {
    /// Whether the technical review plugin is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Optional path to a directory containing custom prompt templates.
    /// When `None`, built-in default prompts are used.
    #[serde(default)]
    pub prompt_dir: Option<String>,
    /// Maximum number of repository files to include in a single analysis.
    #[serde(default = "default_max_files")]
    pub max_files: u32,
    /// Whether to include test files in the analysis.
    #[serde(default = "default_true")]
    pub include_tests: bool,
    /// Whether to include documentation files in the analysis.
    #[serde(default = "default_true")]
    pub include_docs: bool,
    /// Number of files per AI request batch.
    #[serde(default = "default_batch_size")]
    pub batch_size: u32,
    /// Optional model identifier override for this plugin.
    /// When `None`, the global provider model is used.
    #[serde(default)]
    pub model_override: Option<String>,
    /// Number of verification passes the AI performs over initial findings.
    #[serde(default = "default_verification_turns")]
    pub verification_turns: u32,
    /// Minimum AI confidence score `[0.0, 1.0]` for a finding to be included.
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f64,
    /// Maximum number of findings to include in the report.
    #[serde(default = "default_max_findings")]
    pub max_findings: u32,
    /// Minimum severity level for a finding to be reported.
    #[serde(default = "default_severity_threshold")]
    pub severity_threshold: String,
    /// Code quality dimensions to focus the review on.
    /// When empty, all 14 dimensions are evaluated.
    #[serde(default)]
    pub focus_areas: Vec<String>,
    /// Output formats for the generated report (e.g. `["markdown", "json"]`).
    #[serde(default = "default_report_formats")]
    pub report_formats: Vec<String>,
    /// Maximum number of agent turns allowed in the AI analysis loop.
    #[serde(default = "default_agent_max_turns")]
    pub agent_max_turns: u32,
    /// Weight of the AI confidence score in the final blended confidence
    /// computation.  Clamped to `[0.0, 1.0]` at use time.
    ///
    /// Default: `0.5`.
    #[serde(default = "default_ai_confidence_weight")]
    pub ai_confidence_weight: f64,
    /// Master switch for the AI confidence blend step.
    ///
    /// When `false`, only the static signal score is used to gate findings;
    /// the AI's self-reported confidence value is ignored entirely.  Setting
    /// this to `false` produces identical filtering behaviour to a pipeline
    /// configured with no AI provider.
    ///
    /// Default: `true`.
    #[serde(default = "default_true")]
    pub ai_analysis_enabled: bool,
    /// Maximum number of matched files before the investigation switches to
    /// batched sessions. When `None`, the default of `20` is used.
    #[serde(default)]
    pub investigation_threshold_files: Option<u64>,
    /// Maximum total repository size in bytes before investigation switches to
    /// batched sessions. When `None`, the default of `10_000_000` (10 MB) is used.
    #[serde(default)]
    pub investigation_threshold_bytes: Option<u64>,
    /// Number of concurrent investigation batches.
    /// When `None`, the default of `4` is used.
    #[serde(default)]
    pub investigation_batch_count: Option<u32>,
    /// Whether to attempt OpenSSF Scorecard resolution for supply-chain signal.
    ///
    /// When `true`, [`resolve_scorecard`] is called during each `technical-review`
    /// plugin run. Resolution checks a local `scorecard.json` file at the
    /// repository root first, then falls back to the public OpenSSF Scorecard
    /// REST API. Defaults to `true`.
    ///
    /// [`resolve_scorecard`]: crate::clients::scorecard::resolve_scorecard
    #[serde(default = "default_true")]
    pub scorecard_enabled: bool,
    /// Whether to attempt GitHub repository metadata resolution.
    ///
    /// When `true`, [`resolve_repodata`] is called during each `technical-review`
    /// plugin run. Resolution checks a local `repodata.json` file at the
    /// repository root first, then falls back to the GitHub REST API. Set
    /// `GITHUB_TOKEN` in the environment for authenticated requests.
    /// Defaults to `true`.
    ///
    /// [`resolve_repodata`]: crate::clients::repodata::resolve_repodata
    #[serde(default = "default_true")]
    pub repodata_enabled: bool,
}

fn default_agent_max_turns() -> u32 {
    15
}

/// Default AI confidence weight shared by both review plugin configurations.
fn default_ai_confidence_weight() -> f64 {
    0.5
}

fn default_true() -> bool {
    true
}

fn default_max_files() -> u32 {
    50
}

fn default_batch_size() -> u32 {
    10
}

fn default_verification_turns() -> u32 {
    1
}

fn default_confidence_threshold() -> f64 {
    0.7
}

fn default_max_findings() -> u32 {
    25
}

fn default_severity_threshold() -> String {
    "medium".to_string()
}

fn default_report_formats() -> Vec<String> {
    vec!["markdown".to_string(), "json".to_string()]
}

impl Default for TechnicalReviewConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            prompt_dir: None,
            max_files: default_max_files(),
            include_tests: default_true(),
            include_docs: default_true(),
            batch_size: default_batch_size(),
            model_override: None,
            verification_turns: default_verification_turns(),
            confidence_threshold: default_confidence_threshold(),
            max_findings: default_max_findings(),
            severity_threshold: default_severity_threshold(),
            focus_areas: vec![],
            report_formats: default_report_formats(),
            agent_max_turns: default_agent_max_turns(),
            ai_confidence_weight: default_ai_confidence_weight(),
            ai_analysis_enabled: default_true(),
            investigation_threshold_files: None,
            investigation_threshold_bytes: None,
            investigation_batch_count: None,
            scorecard_enabled: default_true(),
            repodata_enabled: default_true(),
        }
    }
}

/// Default max_findings value for security review.
fn default_sec_max_findings() -> u32 {
    50
}

/// Default severity threshold for security review.
fn default_sec_severity_threshold() -> String {
    "medium".to_string()
}

/// Default batch size for security review.
fn default_sec_batch_size() -> u32 {
    10
}

/// Default verification turns for security review.
fn default_sec_verification_turns() -> u32 {
    1
}

/// Default confidence threshold for security review.
fn default_sec_confidence_threshold() -> f64 {
    0.5
}

/// Default report formats for security review.
fn default_sec_report_formats() -> Vec<String> {
    vec![
        "markdown".to_string(),
        "json".to_string(),
        "sarif".to_string(),
    ]
}

/// Configuration for the built-in security review plugin.
///
/// Controls which checks run, how findings are thresholded, and what output
/// formats are generated.  SARIF output is enabled by default when the
/// security review plugin runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReviewConfig {
    /// Whether the security review plugin is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Directory to load custom security review prompt templates from.
    #[serde(default)]
    pub prompt_dir: String,
    /// Maximum number of findings to include in the report.
    #[serde(default = "default_sec_max_findings")]
    pub max_findings: u32,
    /// Minimum severity level for a finding to be included.
    #[serde(default = "default_sec_severity_threshold")]
    pub severity_threshold: String,
    /// Whether to include a SARIF-format report alongside Markdown and JSON.
    #[serde(default = "default_true")]
    pub include_sarif: bool,
    /// Output formats for the generated report.
    #[serde(default = "default_sec_report_formats")]
    pub report_formats: Vec<String>,
    /// Run secret pattern scanning.
    #[serde(default = "default_true")]
    pub secret_scanning: bool,
    /// Scan dependency manifests for known-vulnerable packages.
    #[serde(default = "default_true")]
    pub dependency_scanning: bool,
    /// Check for unsafe Rust code blocks.
    #[serde(default = "default_true")]
    pub check_unsafe_code: bool,
    /// Check authentication and authorization code.
    #[serde(default = "default_true")]
    pub check_auth: bool,
    /// Check for hardcoded or suspicious endpoints.
    #[serde(default = "default_true")]
    pub check_endpoints: bool,
    /// Check for OS command execution patterns.
    #[serde(default = "default_true")]
    pub check_command_execution: bool,
    /// Check for unsafe deserialization patterns.
    #[serde(default = "default_true")]
    pub check_deserialization: bool,
    /// Check for weak or misused cryptography.
    #[serde(default = "default_true")]
    pub check_cryptography: bool,
    /// When true, the plugin returns a failure exit code if any critical
    /// finding is present.
    #[serde(default)]
    pub fail_on_critical: bool,
    /// Number of files to include in each AI analysis batch.
    #[serde(default = "default_sec_batch_size")]
    pub batch_size: u32,
    /// Optional provider model override for the security review step.
    #[serde(default)]
    pub model_override: Option<String>,
    /// Number of verification turns to request from the AI provider.
    #[serde(default = "default_sec_verification_turns")]
    pub verification_turns: u32,
    /// Minimum AI confidence score `[0.0, 1.0]` for a finding to be included.
    #[serde(default = "default_sec_confidence_threshold")]
    pub confidence_threshold: f64,
    /// Maximum number of agent turns allowed in the AI analysis loop.
    #[serde(default = "default_sec_agent_max_turns")]
    pub agent_max_turns: u32,
    /// Weight of the AI confidence score in the final blended confidence
    /// computation.  Clamped to `[0.0, 1.0]` at use time.
    ///
    /// Default: `0.5`.
    #[serde(default = "default_ai_confidence_weight")]
    pub ai_confidence_weight: f64,
    /// Master switch for the AI confidence blend step.
    ///
    /// When `false`, only the static signal score is used to gate findings;
    /// the AI's self-reported confidence value is ignored entirely.  Setting
    /// this to `false` produces identical filtering behaviour to a pipeline
    /// configured with no AI provider.
    ///
    /// Default: `true`.
    #[serde(default = "default_true")]
    pub ai_analysis_enabled: bool,
    /// Maximum number of matched files before the investigation switches to
    /// batched sessions. When `None`, the default of `20` is used.
    #[serde(default)]
    pub investigation_threshold_files: Option<u64>,
    /// Maximum total repository size in bytes before investigation switches to
    /// batched sessions. When `None`, the default of `10_000_000` (10 MB) is used.
    #[serde(default)]
    pub investigation_threshold_bytes: Option<u64>,
    /// Number of concurrent investigation batches.
    /// When `None`, the default of `4` is used.
    #[serde(default)]
    pub investigation_batch_count: Option<u32>,
    /// Whether to query the OSV vulnerability database when dependency scanning
    /// is enabled. The OSV endpoint is unauthenticated and requires no API key.
    ///
    /// Default: `true`.
    #[serde(default = "default_true")]
    pub osv_enabled: bool,
}

/// Default maximum agent turns for security review.
fn default_sec_agent_max_turns() -> u32 {
    15
}

impl Default for SecurityReviewConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prompt_dir: String::new(),
            max_findings: default_sec_max_findings(),
            severity_threshold: default_sec_severity_threshold(),
            include_sarif: true,
            report_formats: default_sec_report_formats(),
            secret_scanning: true,
            dependency_scanning: true,
            check_unsafe_code: true,
            check_auth: true,
            check_endpoints: true,
            check_command_execution: true,
            check_deserialization: true,
            check_cryptography: true,
            fail_on_critical: false,
            batch_size: default_sec_batch_size(),
            model_override: None,
            verification_turns: default_sec_verification_turns(),
            confidence_threshold: default_sec_confidence_threshold(),
            agent_max_turns: default_sec_agent_max_turns(),
            ai_confidence_weight: default_ai_confidence_weight(),
            ai_analysis_enabled: default_true(),
            investigation_threshold_files: None,
            investigation_threshold_bytes: None,
            investigation_batch_count: None,
            osv_enabled: default_true(),
        }
    }
}

/// Governance rule enforcement configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
    /// Whether governance checking is active.
    #[serde(default)]
    pub enabled: bool,
    /// Path to the rules file (e.g. `AGENTS.md`).
    #[serde(default)]
    pub rules_path: String,
    /// Whether a governance violation should abort the run.
    #[serde(default)]
    pub fail_on_violation: bool,
}

impl Default for GovernanceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rules_path: "AGENTS.md".to_string(),
            fail_on_violation: true,
        }
    }
}

/// Kafka watcher configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherConfig {
    /// Whether the watcher is active.
    #[serde(default)]
    pub enabled: bool,
    /// Maximum number of pipeline tasks to run concurrently.
    #[serde(default)]
    pub max_concurrent_tasks: u32,
    /// Whether to publish results back to Kafka after each run.
    #[serde(default)]
    pub result_publish_enabled: bool,
    /// When `true`, process one batch of tasks then exit.
    #[serde(default)]
    pub once: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_concurrent_tasks: 2,
            result_publish_enabled: true,
            once: false,
        }
    }
}

/// Kafka broker and security configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaConfig {
    /// List of Kafka broker addresses.
    #[serde(default)]
    pub brokers: Vec<String>,
    /// Consumer group identifier.
    #[serde(default)]
    pub group_id: String,
    /// Security protocol (`PLAINTEXT`, `SSL`, `SASL_PLAINTEXT`, `SASL_SSL`).
    #[serde(default)]
    pub security_protocol: String,
    /// SASL mechanism (e.g. `PLAIN`, `SCRAM-SHA-256`).
    #[serde(default)]
    pub sasl_mechanism: Option<String>,
    /// Environment variable holding the SASL username.
    #[serde(default)]
    pub sasl_username_env: Option<String>,
    /// Environment variable holding the SASL password.
    #[serde(default)]
    pub sasl_password_env: Option<String>,
    /// Path to the SSL CA certificate file.
    #[serde(default)]
    pub ssl_ca_location: Option<String>,
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            brokers: vec!["localhost:9092".to_string()],
            group_id: "xzardgz-workflow-harness".to_string(),
            security_protocol: "PLAINTEXT".to_string(),
            sasl_mechanism: None,
            sasl_username_env: None,
            sasl_password_env: None,
            ssl_ca_location: None,
        }
    }
}

/// Kafka topic names used by the watcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicsConfig {
    /// Topic on which task messages are consumed.
    #[serde(default)]
    pub task: String,
    /// Topic to which result messages are published.
    #[serde(default)]
    pub result: String,
}

impl Default for TopicsConfig {
    fn default() -> Self {
        Self {
            task: "xzardgz.tasks".to_string(),
            result: "xzardgz.results".to_string(),
        }
    }
}

/// Event and repository routing matcher configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatcherConfig {
    /// Event type strings that this instance should handle.
    #[serde(default)]
    pub event_types: Vec<String>,
    /// Repository name filters (empty means all repositories).
    #[serde(default)]
    pub repositories: Vec<String>,
    /// Plugin name filters applied to incoming tasks.
    #[serde(default)]
    pub plugins: Vec<String>,
    /// Platform filters (empty means all platforms).
    #[serde(default)]
    pub platforms: Vec<String>,
    /// Arbitrary key/value metadata filters.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl Default for MatcherConfig {
    fn default() -> Self {
        Self {
            event_types: vec![
                "xzardgz.technical_review.requested".to_string(),
                "xzardgz.security_review.requested".to_string(),
            ],
            repositories: vec![],
            plugins: vec![
                "technical-review".to_string(),
                "security-review".to_string(),
            ],
            platforms: vec![],
            metadata: HashMap::new(),
        }
    }
}

impl MatcherConfig {
    /// Returns `true` when `event_types`, `repositories`, and `plugins` are all empty.
    ///
    /// An empty matcher acts as a wildcard that accepts every incoming event.
    pub fn is_empty(&self) -> bool {
        self.event_types.is_empty() && self.repositories.is_empty() && self.plugins.is_empty()
    }
}

/// Prompt discovery and override configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptsConfig {
    /// Directories to search for prompt template files.
    #[serde(default)]
    pub directories: Vec<String>,
    /// Whether user-supplied prompt overrides are permitted.
    #[serde(default)]
    pub allow_overrides: bool,
}

impl Default for PromptsConfig {
    fn default() -> Self {
        Self {
            directories: vec![".xzardgz/prompts".to_string()],
            allow_overrides: true,
        }
    }
}

/// Sub-agent delegation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    /// Whether sub-agent delegation is active.
    #[serde(default)]
    pub enabled: bool,
    /// Maximum recursion depth for sub-agent chains.
    #[serde(default)]
    pub max_depth: u32,
}

impl Default for SubagentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_depth: 1,
        }
    }
}

/// Trace transcript recording configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceTranscriptConfig {
    /// Whether transcript recording is active.
    #[serde(default)]
    pub enabled: bool,
    /// Whether secret values are redacted in the transcript.
    #[serde(default)]
    pub redact_secrets: bool,
    /// Directory where transcript files are written.
    #[serde(default)]
    pub output_dir: String,
}

impl Default for TraceTranscriptConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            redact_secrets: true,
            output_dir: ".xzardgz/transcripts".to_string(),
        }
    }
}

/// Scan output file configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanOutputConfig {
    /// File path for the scan output.
    #[serde(default)]
    pub path: String,
    /// Output format identifier (e.g. `"json"`).
    #[serde(default)]
    pub format: String,
    /// Whether to overwrite an existing output file.
    #[serde(default)]
    pub overwrite: bool,
}

impl Default for ScanOutputConfig {
    fn default() -> Self {
        Self {
            path: ".xzardgz/scan/scan.json".to_string(),
            format: "json".to_string(),
            overwrite: true,
        }
    }
}

/// Report generation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportsConfig {
    /// Directory where generated reports are written.
    #[serde(default)]
    pub output_dir: String,
    /// Output formats to generate (e.g. `["markdown", "json"]`).
    #[serde(default)]
    pub formats: Vec<String>,
    /// Whether to overwrite existing reports.
    #[serde(default)]
    pub overwrite: bool,
    /// Whether to embed pipeline diagnostic information in reports.
    #[serde(default)]
    pub include_diagnostics: bool,
}

impl Default for ReportsConfig {
    fn default() -> Self {
        Self {
            output_dir: ".xzardgz/reports".to_string(),
            formats: vec!["markdown".to_string(), "json".to_string()],
            overwrite: true,
            include_diagnostics: true,
        }
    }
}

/// MCP server authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpAuthConfig {
    /// Environment variable holding the bearer token.
    #[serde(default)]
    pub token_env: Option<String>,
    /// Authentication method identifier (e.g. `"bearer"`).
    #[serde(default)]
    pub method: Option<String>,
}

/// Configuration for a single MCP server instance.
///
/// `name` and `command` do not have meaningful defaults; they must always be
/// specified in the config file.  [`Default`] is implemented with empty strings
/// solely to satisfy the `#[serde(default)]` constraint on
/// `Vec<McpServerConfig>`.  In practice, users always specify these fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Unique name for this MCP server instance.
    pub name: String,
    /// Executable command used to launch the server process.
    pub command: String,
    /// Command-line arguments passed to the server command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Additional environment variables injected into the server process.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Per-server request timeout in seconds.
    #[serde(default)]
    pub timeout_seconds: u64,
    /// Transport type identifier (`"stdio"`, `"http"`, etc.).
    #[serde(default)]
    pub transport: String,
    /// Allowlist of tool names the server may expose.
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Optional authentication configuration for this server.
    #[serde(default)]
    pub auth: Option<McpAuthConfig>,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            timeout_seconds: 30,
            transport: "stdio".to_string(),
            allowed_tools: vec![],
            auth: None,
        }
    }
}

/// Model Context Protocol (MCP) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    /// List of MCP server configurations.
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
    /// Global tool allowlist keyed by server name.
    #[serde(default)]
    pub allowed_tools: HashMap<String, Vec<String>>,
    /// Default request timeout for all MCP servers in seconds.
    #[serde(default)]
    pub timeout_seconds: u64,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            servers: vec![],
            allowed_tools: HashMap::new(),
            timeout_seconds: 30,
        }
    }
}

/// Model metadata caching configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadataConfig {
    /// File path for the cached model metadata.
    #[serde(default)]
    pub cache_path: String,
    /// Whether to refresh model metadata when the pipeline starts.
    #[serde(default)]
    pub refresh_on_start: bool,
    /// Whether to continue when model metadata is incomplete or stale.
    #[serde(default)]
    pub allow_degraded_metadata: bool,
}

impl Default for ModelMetadataConfig {
    fn default() -> Self {
        Self {
            cache_path: ".xzardgz/model_metadata.json".to_string(),
            refresh_on_start: false,
            allow_degraded_metadata: true,
        }
    }
}

/// Automatic model selection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSelectionConfig {
    /// Whether automatic model selection is active.
    #[serde(default)]
    pub enabled: bool,
    /// Whether to fall back to the next model when the preferred one fails.
    #[serde(default)]
    pub auto_fallback: bool,
    /// Whether to require tool-calling support when selecting a model.
    #[serde(default)]
    pub require_tools: bool,
    /// Whether to require structured output support when selecting a model.
    #[serde(default)]
    pub require_structured_output: bool,
    /// Minimum context window size in tokens required of a candidate model.
    #[serde(default)]
    pub min_context_tokens: u32,
    /// Ordered list of preferred model identifiers.
    #[serde(default)]
    pub preferred_models: Vec<String>,
    /// Ordered list of fallback model identifiers.
    #[serde(default)]
    pub fallback_models: Vec<String>,
}

impl Default for ModelSelectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_fallback: true,
            require_tools: true,
            require_structured_output: true,
            min_context_tokens: 16000,
            preferred_models: vec!["gpt-4.1-mini".to_string()],
            fallback_models: vec!["gpt-4.1".to_string()],
        }
    }
}

/// Project identity metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Project name.
    #[serde(default)]
    pub name: String,
    /// Project owner identifier.
    #[serde(default)]
    pub owner: String,
    /// Tags associated with this project.
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: "xzardgz".to_string(),
            owner: "local".to_string(),
            tags: vec!["workflow-harness".to_string()],
        }
    }
}

// ---------------------------------------------------------------------------
// Override types
// ---------------------------------------------------------------------------

/// CLI and programmatic configuration overrides.
///
/// Each field is `Option<T>`; only `Some` values are applied when passed to
/// [`Config::apply_overrides`].
#[derive(Debug, Clone, Default)]
pub struct ConfigOverrides {
    /// Override the active provider name.
    pub provider: Option<String>,
    /// Override the OpenAI model identifier.
    pub openai_model: Option<String>,
    /// Override the Ollama model identifier.
    pub ollama_model: Option<String>,
    /// Override the Ollama host URL.
    pub ollama_host: Option<String>,
    /// Override the workspace root directory.
    pub workspace_root: Option<String>,
}

/// Model selection overrides for CLI or programmatic adjustment.
///
/// Each field is `Option<T>`; only `Some` values are applied when passed to
/// [`Config::merge_model_selection`].
#[derive(Debug, Clone, Default)]
pub struct ModelSelectionOverrides {
    /// Override the preferred model list.
    pub preferred_models: Option<Vec<String>>,
    /// Override the fallback model list.
    pub fallback_models: Option<Vec<String>>,
    /// Override the auto-fallback flag.
    pub auto_fallback: Option<bool>,
    /// Override the require-tools flag.
    pub require_tools: Option<bool>,
    /// Override the require-structured-output flag.
    pub require_structured_output: Option<bool>,
    /// Override the minimum context token requirement.
    pub min_context_tokens: Option<u32>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_uses_openai() {
        assert_eq!(Config::default().provider.default, "openai");
    }

    #[test]
    fn test_default_model_selection_enabled() {
        assert!(Config::default().model_selection.enabled);
    }

    #[test]
    fn test_default_model_selection_auto_fallback() {
        assert!(Config::default().model_selection.auto_fallback);
    }

    #[test]
    fn test_default_min_context_tokens() {
        assert_eq!(Config::default().model_selection.min_context_tokens, 16000);
    }

    #[test]
    fn test_load_from_str_rejects_documentation_field() {
        let yaml = "documentation:\n  foo: bar\n";
        let result = Config::load_from_str(yaml);
        assert!(
            result.is_err(),
            "expected Err for legacy 'documentation' field"
        );
    }

    #[test]
    fn test_load_from_str_rejects_export_scan_field() {
        let yaml = "export_scan:\n  enabled: true\n";
        let result = Config::load_from_str(yaml);
        assert!(
            result.is_err(),
            "expected Err for legacy 'export_scan' field"
        );
    }

    #[test]
    fn test_validate_rejects_insecure_endpoint() {
        let mut config = Config::default();
        config.openai.allow_insecure_endpoint = false;
        config.openai.endpoint = "http://example.com".to_string();
        assert!(
            config.validate().is_err(),
            "expected Err for insecure endpoint"
        );
    }

    #[test]
    fn test_validate_allows_https_endpoint() {
        let mut config = Config::default();
        config.openai.allow_insecure_endpoint = false;
        config.openai.endpoint = "https://api.openai.com/v1".to_string();
        assert!(config.validate().is_ok(), "expected Ok for HTTPS endpoint");
    }

    #[test]
    fn test_matcher_is_empty_when_all_lists_empty() {
        let matcher = MatcherConfig {
            event_types: vec![],
            repositories: vec![],
            plugins: vec![],
            platforms: vec![],
            metadata: HashMap::new(),
        };
        assert!(matcher.is_empty());
    }

    #[test]
    fn test_matcher_is_not_empty_when_has_event_types() {
        let mut matcher = MatcherConfig {
            event_types: vec![],
            repositories: vec![],
            plugins: vec![],
            platforms: vec![],
            metadata: HashMap::new(),
        };
        matcher.event_types = vec!["xzardgz.technical_review.requested".to_string()];
        assert!(!matcher.is_empty());
    }

    #[test]
    fn test_apply_overrides_sets_provider() {
        let mut config = Config::default();
        let overrides = ConfigOverrides {
            provider: Some("ollama".to_string()),
            ..ConfigOverrides::default()
        };
        config.apply_overrides(&overrides);
        assert_eq!(config.provider.default, "ollama");
    }

    #[test]
    fn test_merge_model_selection_updates_preferred_models() {
        let mut config = Config::default();
        let overrides = ModelSelectionOverrides {
            preferred_models: Some(vec!["gpt-4o".to_string()]),
            ..ModelSelectionOverrides::default()
        };
        config.merge_model_selection(&overrides);
        assert_eq!(config.model_selection.preferred_models, vec!["gpt-4o"]);
    }

    // ------------------------------------------------------------------
    // TechnicalReviewConfig investigation thresholds
    // ------------------------------------------------------------------

    #[test]
    fn test_technical_review_config_investigation_threshold_files_defaults_to_none() {
        let cfg = TechnicalReviewConfig::default();
        assert!(cfg.investigation_threshold_files.is_none());
    }

    #[test]
    fn test_technical_review_config_investigation_threshold_bytes_defaults_to_none() {
        let cfg = TechnicalReviewConfig::default();
        assert!(cfg.investigation_threshold_bytes.is_none());
    }

    #[test]
    fn test_technical_review_config_investigation_batch_count_defaults_to_none() {
        let cfg = TechnicalReviewConfig::default();
        assert!(cfg.investigation_batch_count.is_none());
    }

    #[test]
    fn test_technical_review_config_scorecard_enabled_defaults_to_true() {
        let cfg = TechnicalReviewConfig::default();
        assert!(cfg.scorecard_enabled);
    }

    #[test]
    fn test_technical_review_config_repodata_enabled_defaults_to_true() {
        let cfg = TechnicalReviewConfig::default();
        assert!(cfg.repodata_enabled);
    }

    #[test]
    fn test_technical_review_config_investigation_thresholds_can_be_set() {
        let cfg = TechnicalReviewConfig {
            investigation_threshold_files: Some(50),
            investigation_threshold_bytes: Some(5_000_000),
            investigation_batch_count: Some(8),
            ..TechnicalReviewConfig::default()
        };
        assert_eq!(cfg.investigation_threshold_files, Some(50));
        assert_eq!(cfg.investigation_threshold_bytes, Some(5_000_000));
        assert_eq!(cfg.investigation_batch_count, Some(8));
    }

    // ------------------------------------------------------------------
    // SecurityReviewConfig investigation thresholds
    // ------------------------------------------------------------------

    #[test]
    fn test_security_review_config_investigation_threshold_files_defaults_to_none() {
        let cfg = SecurityReviewConfig::default();
        assert!(cfg.investigation_threshold_files.is_none());
    }

    #[test]
    fn test_security_review_config_investigation_threshold_bytes_defaults_to_none() {
        let cfg = SecurityReviewConfig::default();
        assert!(cfg.investigation_threshold_bytes.is_none());
    }

    #[test]
    fn test_security_review_config_investigation_batch_count_defaults_to_none() {
        let cfg = SecurityReviewConfig::default();
        assert!(cfg.investigation_batch_count.is_none());
    }

    #[test]
    fn test_security_review_config_osv_enabled_defaults_to_true() {
        let cfg = SecurityReviewConfig::default();
        assert!(cfg.osv_enabled);
    }

    #[test]
    fn test_security_review_config_osv_enabled_can_be_set_false() {
        let cfg = SecurityReviewConfig {
            osv_enabled: false,
            ..SecurityReviewConfig::default()
        };
        assert!(!cfg.osv_enabled);
    }

    #[test]
    fn test_security_review_config_investigation_thresholds_can_be_set() {
        let cfg = SecurityReviewConfig {
            investigation_threshold_files: Some(30),
            investigation_threshold_bytes: Some(8_000_000),
            investigation_batch_count: Some(6),
            ..SecurityReviewConfig::default()
        };
        assert_eq!(cfg.investigation_threshold_files, Some(30));
        assert_eq!(cfg.investigation_threshold_bytes, Some(8_000_000));
        assert_eq!(cfg.investigation_batch_count, Some(6));
    }
}
