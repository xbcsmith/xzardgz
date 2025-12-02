use crate::error::ConfigError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: ProviderConfig,
    pub agent: AgentConfig,
    pub repository: RepositoryConfig,
    pub documentation: DocumentationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider_type: String, // "copilot" or "ollama"
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub max_turns: u32,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryConfig {
    pub ignore_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentationConfig {
    pub output_dir: String,
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let mut config = Config::default();

        // 1. Load from file (config.yaml) if exists
        if std::path::Path::new("config.yaml").exists() {
            let content = std::fs::read_to_string("config.yaml")
                .map_err(|e| ConfigError::Load(format!("Failed to read config file: {}", e)))?;
            let file_config: Config = serde_yaml::from_str(&content)
                .map_err(|e| ConfigError::Load(format!("Failed to parse config file: {}", e)))?;
            config = file_config;
        }

        // 2. Override with Env Vars (Basic implementation)
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
            documentation: DocumentationConfig {
                output_dir: "docs".to_string(),
            },
        }
    }
}
