use crate::error::{PipelineError, Result};
use serde::{Deserialize, Serialize};

/// Top-level application configuration loaded from file and environment variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Configuration for the AI provider backend.
    pub provider: ProviderConfig,
    /// Configuration for the autonomous agent behaviour.
    pub agent: AgentConfig,
    /// Configuration for repository scanning behaviour.
    pub repository: RepositoryConfig,
}

/// Configuration for the AI provider backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// The provider type identifier, e.g. `"copilot"` or `"ollama"`.
    pub provider_type: String,
    /// An optional model name override for the selected provider.
    pub model: Option<String>,
}

/// Configuration for the autonomous agent behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Maximum number of turns the agent may take before stopping.
    pub max_turns: u32,
    /// Maximum elapsed time in seconds before the agent is considered timed out.
    pub timeout_seconds: u64,
}

/// Configuration for repository scanning behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryConfig {
    /// Glob patterns for paths to ignore during repository scanning.
    pub ignore_patterns: Vec<String>,
}

impl Config {
    /// Loads configuration from `config.yaml` if it exists, then applies
    /// environment variable overrides.
    ///
    /// Falls back to [`Config::default`] when no config file is present.
    /// Returns `PipelineError::Config` on file read or parse failures.
    pub fn load() -> Result<Self> {
        let mut config = Config::default();

        if std::path::Path::new("config.yaml").exists() {
            let content = std::fs::read_to_string("config.yaml")
                .map_err(|e| PipelineError::Config(format!("failed to read config file: {}", e)))?;
            let file_config: Config = serde_yaml::from_str(&content).map_err(|e| {
                PipelineError::Config(format!("failed to parse config file: {}", e))
            })?;
            config = file_config;
        }

        if let Ok(provider) = std::env::var("XZARDGZ_PROVIDER") {
            config.provider.provider_type = provider;
        }

        Ok(config)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: ProviderConfig {
                provider_type: "ollama".to_string(),
                model: Some("qwen2.5-coder".to_string()),
            },
            agent: AgentConfig {
                max_turns: 10,
                timeout_seconds: 600,
            },
            repository: RepositoryConfig {
                ignore_patterns: vec!["target".to_string(), ".git".to_string()],
            },
        }
    }
}
