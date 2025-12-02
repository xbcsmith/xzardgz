use thiserror::Error;

pub type Result<T> = std::result::Result<T, XzardgzError>;

#[derive(Debug, Error)]
pub enum XzardgzError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("Workflow error: {0}")]
    Workflow(#[from] WorkflowError),

    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Documentation generation error: {0}")]
    DocGen(#[from] DocGenError),
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to load config: {0}")]
    Load(String),
    #[error("Validation error: {0}")]
    Validation(String),
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("Plan parsing failed: {0}")]
    Parse(String),
    #[error("Execution failed: {0}")]
    Execution(String),
}

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("Git operation failed: {0}")]
    Git(String),
    #[error("Scan failed: {0}")]
    Scan(String),
}

#[derive(Debug, Error)]
pub enum DocGenError {
    #[error("Template error: {0}")]
    Template(String),
    #[error("Generation error: {0}")]
    Generation(String),
    #[error("IO error: {0}")]
    Io(String),
}
