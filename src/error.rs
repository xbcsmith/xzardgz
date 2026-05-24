//! Unified error types for the XZardgz pipeline.
//!
//! All public-facing functions return [`Result<T>`], which is aliased to
//! `std::result::Result<T, PipelineError>`.  The legacy sub-error types
//! (`ConfigError`, `ProviderError`, `WorkflowError`, `RepositoryError`) are
//! retained for backward compatibility; each has a manual `From`
//! implementation that maps it into the appropriate `PipelineError` variant.

use thiserror::Error;

// ---------------------------------------------------------------------------
// Primary error type
// ---------------------------------------------------------------------------

/// Unified, flat error type used throughout the XZardgz pipeline.
///
/// Every variant carries either a descriptive [`String`] message or, for
/// structured cases, named fields that let callers pattern-match on specific
/// details without string parsing.
#[derive(Debug, Error)]
pub enum PipelineError {
    /// Configuration loading or validation error.
    #[error("configuration error: {0}")]
    Config(String),

    /// Git operation error.
    #[error("git error: {0}")]
    Git(String),

    /// Repository scanner error.
    #[error("scanner error: {0}")]
    Scanner(String),

    /// AI provider interaction error.
    #[error("provider error: {0}")]
    Provider(String),

    /// Authentication error.
    #[error("authentication error: {0}")]
    Auth(String),

    /// Governance rule violation.
    #[error("governance error: {0}")]
    Governance(String),

    /// Agent execution error.
    #[error("agent error: {0}")]
    Agent(String),

    /// Tool execution error.
    #[error("tool error: {0}")]
    Tool(String),

    /// Named plugin not registered in the plugin registry.
    #[error("plugin not found: {name}")]
    PluginNotFound {
        /// The name of the missing plugin.
        name: String,
    },

    /// Plugin execution error.
    #[error("plugin error: {0}")]
    Plugin(String),

    /// Prompt resolution or rendering error.
    #[error("prompt error: {0}")]
    Prompt(String),

    /// Report generation error.
    #[error("report error: {0}")]
    Report(String),

    /// Workspace operation error.
    #[error("workspace error: {0}")]
    Workspace(String),

    /// Workflow execution error.
    #[error("workflow error: {0}")]
    Workflow(String),

    /// Watcher operation or routing error.
    #[error("watcher error: {0}")]
    Watcher(String),

    /// Kafka publish error.
    #[error("kafka error: {0}")]
    Kafka(String),

    /// Generic MCP error.
    #[error("mcp error: {0}")]
    Mcp(String),

    /// MCP transport error (connection or framing level).
    #[error("mcp transport error: {0}")]
    McpTransport(String),

    /// Named MCP server is not present in the registry.
    #[error("mcp server not found: {server}")]
    McpServerNotFound {
        /// The name of the missing MCP server.
        server: String,
    },

    /// Requested tool is not registered on the named MCP server.
    #[error("mcp tool not found: server={server}, tool={tool}")]
    McpToolNotFound {
        /// The MCP server that was queried.
        server: String,
        /// The tool that was not found on that server.
        tool: String,
    },

    /// Incompatible MCP protocol versions between client and server.
    #[error("mcp protocol version mismatch: expected {expected}, got {got}")]
    McpProtocolVersionMismatch {
        /// The protocol version the client expected.
        expected: String,
        /// The protocol version the server reported.
        got: String,
    },

    /// MCP server did not respond within the allotted time.
    #[error("mcp timeout: server={server}, timeout_ms={timeout_ms}")]
    McpTimeout {
        /// The MCP server that timed out.
        server: String,
        /// The timeout threshold in milliseconds.
        timeout_ms: u64,
    },

    /// MCP credential or authorization error.
    #[error("mcp auth error: {0}")]
    McpAuth(String),

    /// MCP sampling or prompting (elicitation) error.
    #[error("mcp elicitation error: {0}")]
    McpElicitation(String),

    /// MCP background task error.
    #[error("mcp task error: {0}")]
    McpTask(String),

    /// Standard IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Arbitrary error not covered by another variant.
    #[error("error: {0}")]
    Custom(String),
}

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

/// Convenience alias so callers can write `Result<T>` instead of
/// `std::result::Result<T, PipelineError>`.
pub type Result<T> = std::result::Result<T, PipelineError>;

/// Backward-compatibility alias.  New code should prefer [`PipelineError`].
pub type XzardgzError = PipelineError;

// ---------------------------------------------------------------------------
// Legacy sub-error types
// ---------------------------------------------------------------------------

/// Configuration-related errors (legacy; prefer [`PipelineError::Config`]).
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Failed to load a configuration file or source.
    #[error("Failed to load config: {0}")]
    Load(String),

    /// Configuration value failed validation.
    #[error("Validation error: {0}")]
    Validation(String),
}

/// AI-provider errors (legacy; prefer [`PipelineError::Provider`]).
#[derive(Debug, Error)]
pub enum ProviderError {
    /// Provider authentication failed.
    #[error("Authentication failed: {0}")]
    Auth(String),

    /// Provider returned an API-level error.
    #[error("API error: {0}")]
    Api(String),

    /// Network-level failure communicating with the provider.
    #[error("Network error: {0}")]
    Network(String),

    /// Failed to serialize or deserialize provider data.
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Workflow errors (legacy; prefer [`PipelineError::Workflow`]).
#[derive(Debug, Error)]
pub enum WorkflowError {
    /// Failed to parse a workflow plan.
    #[error("Plan parsing failed: {0}")]
    Parse(String),

    /// Workflow execution step failed.
    #[error("Execution failed: {0}")]
    Execution(String),
}

/// Repository errors (legacy; prefer [`PipelineError::Git`]).
#[derive(Debug, Error)]
pub enum RepositoryError {
    /// A git operation failed.
    #[error("Git operation failed: {0}")]
    Git(String),

    /// A repository scan failed.
    #[error("Scan failed: {0}")]
    Scan(String),
}

// ---------------------------------------------------------------------------
// From impls for legacy sub-error types
// ---------------------------------------------------------------------------

impl From<ConfigError> for PipelineError {
    fn from(e: ConfigError) -> Self {
        PipelineError::Config(e.to_string())
    }
}

impl From<ProviderError> for PipelineError {
    fn from(e: ProviderError) -> Self {
        PipelineError::Provider(e.to_string())
    }
}

impl From<WorkflowError> for PipelineError {
    fn from(e: WorkflowError) -> Self {
        PipelineError::Workflow(e.to_string())
    }
}

impl From<RepositoryError> for PipelineError {
    fn from(e: RepositoryError) -> Self {
        PipelineError::Git(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Display string tests
    // ------------------------------------------------------------------

    #[test]
    fn test_config_display_contains_message() {
        let err = PipelineError::Config("missing field".to_string());
        assert_eq!(err.to_string(), "configuration error: missing field");
    }

    #[test]
    fn test_git_display_contains_message() {
        let err = PipelineError::Git("ref not found".to_string());
        assert_eq!(err.to_string(), "git error: ref not found");
    }

    #[test]
    fn test_plugin_not_found_display_contains_name() {
        let err = PipelineError::PluginNotFound {
            name: "my_plugin".to_string(),
        };
        assert_eq!(err.to_string(), "plugin not found: my_plugin");
    }

    #[test]
    fn test_mcp_server_not_found_display_contains_server() {
        let err = PipelineError::McpServerNotFound {
            server: "main_server".to_string(),
        };
        assert_eq!(err.to_string(), "mcp server not found: main_server");
    }

    #[test]
    fn test_mcp_timeout_display_contains_server_and_ms() {
        let err = PipelineError::McpTimeout {
            server: "slow_server".to_string(),
            timeout_ms: 5000,
        };
        assert_eq!(
            err.to_string(),
            "mcp timeout: server=slow_server, timeout_ms=5000"
        );
    }

    // ------------------------------------------------------------------
    // From<std::io::Error>
    // ------------------------------------------------------------------

    #[test]
    fn test_io_error_from_converts_correctly() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: PipelineError = io_err.into();
        assert!(matches!(err, PipelineError::Io(_)));
        assert!(err.to_string().contains("file missing"));
    }

    // ------------------------------------------------------------------
    // From<ConfigError>
    // ------------------------------------------------------------------

    #[test]
    fn test_config_error_load_from_converts_to_pipeline_config() {
        let ce = ConfigError::Load("bad path".to_string());
        let err: PipelineError = ce.into();
        assert!(matches!(err, PipelineError::Config(_)));
        assert!(err.to_string().contains("bad path"));
    }

    #[test]
    fn test_config_error_validation_from_converts_to_pipeline_config() {
        let ce = ConfigError::Validation("invalid value".to_string());
        let err: PipelineError = ce.into();
        assert!(matches!(err, PipelineError::Config(_)));
        assert!(err.to_string().contains("invalid value"));
    }

    // ------------------------------------------------------------------
    // From<ProviderError>
    // ------------------------------------------------------------------

    #[test]
    fn test_provider_error_from_converts_to_pipeline_provider() {
        let pe = ProviderError::Api("rate limited".to_string());
        let err: PipelineError = pe.into();
        assert!(matches!(err, PipelineError::Provider(_)));
        assert!(err.to_string().contains("rate limited"));
    }

    // ------------------------------------------------------------------
    // From<WorkflowError>
    // ------------------------------------------------------------------

    #[test]
    fn test_workflow_error_from_converts_to_pipeline_workflow() {
        let we = WorkflowError::Execution("step failed".to_string());
        let err: PipelineError = we.into();
        assert!(matches!(err, PipelineError::Workflow(_)));
        assert!(err.to_string().contains("step failed"));
    }

    // ------------------------------------------------------------------
    // From<RepositoryError>
    // ------------------------------------------------------------------

    #[test]
    fn test_repository_error_from_converts_to_pipeline_git() {
        let re = RepositoryError::Git("merge conflict".to_string());
        let err: PipelineError = re.into();
        assert!(matches!(err, PipelineError::Git(_)));
        assert!(err.to_string().contains("merge conflict"));
    }

    // ------------------------------------------------------------------
    // Structured variant field access
    // ------------------------------------------------------------------

    #[test]
    fn test_plugin_not_found_fields_are_accessible() {
        let err = PipelineError::PluginNotFound {
            name: "acme".to_string(),
        };
        if let PipelineError::PluginNotFound { name } = err {
            assert_eq!(name, "acme");
        } else {
            panic!("unexpected variant");
        }
    }

    #[test]
    fn test_mcp_server_not_found_fields_are_accessible() {
        let err = PipelineError::McpServerNotFound {
            server: "srv1".to_string(),
        };
        if let PipelineError::McpServerNotFound { server } = err {
            assert_eq!(server, "srv1");
        } else {
            panic!("unexpected variant");
        }
    }

    #[test]
    fn test_mcp_tool_not_found_fields_are_accessible() {
        let err = PipelineError::McpToolNotFound {
            server: "srv1".to_string(),
            tool: "echo".to_string(),
        };
        if let PipelineError::McpToolNotFound { server, tool } = err {
            assert_eq!(server, "srv1");
            assert_eq!(tool, "echo");
        } else {
            panic!("unexpected variant");
        }
    }

    #[test]
    fn test_mcp_protocol_version_mismatch_fields_are_accessible() {
        let err = PipelineError::McpProtocolVersionMismatch {
            expected: "1.0".to_string(),
            got: "2.0".to_string(),
        };
        if let PipelineError::McpProtocolVersionMismatch { expected, got } = err {
            assert_eq!(expected, "1.0");
            assert_eq!(got, "2.0");
        } else {
            panic!("unexpected variant");
        }
    }

    #[test]
    fn test_mcp_timeout_fields_are_accessible() {
        let err = PipelineError::McpTimeout {
            server: "srv1".to_string(),
            timeout_ms: 3000,
        };
        if let PipelineError::McpTimeout { server, timeout_ms } = err {
            assert_eq!(server, "srv1");
            assert_eq!(timeout_ms, 3000);
        } else {
            panic!("unexpected variant");
        }
    }
}
