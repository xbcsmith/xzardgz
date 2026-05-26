//! MCP management command handler.
//!
//! Implements the `mcp` family of subcommands for managing and introspecting
//! Model Context Protocol server configurations.
//!
//! # Entry points
//!
//! - [`execute`]: preserves the original call signature used by `main.rs`.
//!   Constructs a [`Config::default()`] internally and delegates to
//!   [`execute_with_config`].
//! - [`execute_with_config`]: full implementation; accepts an explicit
//!   [`Config`] for programmatic use and testing.

use crate::cli::McpCommands;
use crate::config::Config;
use crate::error::{PipelineError, Result};
use crate::mcp::registry::McpRegistry;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Executes an MCP subcommand using a default [`Config`].
///
/// This function preserves the call signature expected by `main.rs`.
/// For programmatic use with a specific config, prefer [`execute_with_config`].
///
/// # Arguments
///
/// * `command` - The MCP subcommand variant to execute.
///
/// # Errors
///
/// Returns [`PipelineError::Config`] when MCP configuration validation fails,
/// or transport/protocol errors when connecting to live servers.
pub async fn execute(command: McpCommands) -> Result<()> {
    let config = Config::default();
    execute_with_config(command, &config).await
}

/// Executes an MCP subcommand using the provided [`Config`].
///
/// # Arguments
///
/// * `command` - The MCP subcommand variant to execute.
/// * `config`  - Pipeline configuration supplying the MCP registry.
///
/// # Errors
///
/// Returns [`PipelineError::Config`] when validation finds issues, or
/// [`PipelineError::McpServerNotFound`] / transport errors when the requested
/// server is absent or unreachable.
pub async fn execute_with_config(command: McpCommands, config: &Config) -> Result<()> {
    let registry = McpRegistry::new(config.mcp.clone());

    match command {
        // ------------------------------------------------------------------
        // mcp validate
        // ------------------------------------------------------------------
        McpCommands::Validate => {
            let issues = registry.validate_config();
            if issues.is_empty() {
                let names = registry.server_names();
                if names.is_empty() {
                    println!("MCP configuration is valid. No servers configured.");
                } else {
                    println!("MCP configuration is valid.");
                    println!("Configured servers ({}):", names.len());
                    for name in &names {
                        println!("  - {}", name);
                    }
                }
            } else {
                println!("MCP configuration has {} issue(s):", issues.len());
                for issue in &issues {
                    println!("  - {}", issue);
                }
                return Err(PipelineError::Config(format!(
                    "MCP configuration validation failed with {} issue(s)",
                    issues.len()
                )));
            }
        }

        // ------------------------------------------------------------------
        // mcp list-servers
        // ------------------------------------------------------------------
        McpCommands::ListServers => {
            let names = registry.server_names();
            if names.is_empty() {
                println!("No MCP servers configured.");
            } else {
                println!("Configured MCP servers ({}):", names.len());
                for name in &names {
                    let server_cfg = registry.get_server_config(name);
                    if let Some(cfg) = server_cfg {
                        println!(
                            "  - {} (transport: {}, timeout: {}s)",
                            name, cfg.transport, cfg.timeout_seconds
                        );
                    } else {
                        println!("  - {}", name);
                    }
                }
            }
        }

        // ------------------------------------------------------------------
        // mcp list-tools <server>
        // ------------------------------------------------------------------
        McpCommands::ListTools { server } => {
            let tools = registry.list_tools(&server).await?;
            if tools.is_empty() {
                println!("No tools exposed by server '{}'.", server);
            } else {
                println!("Tools on server '{}' ({}):", server, tools.len());
                for tool in &tools {
                    println!(
                        "  - {} - {}",
                        tool.name,
                        tool.description.as_deref().unwrap_or("(no description)")
                    );
                }
            }
        }

        // ------------------------------------------------------------------
        // mcp test-discovery <server>
        // ------------------------------------------------------------------
        McpCommands::TestDiscovery { server } => {
            let tools = registry.list_tools(&server).await?;
            println!("Tool discovery succeeded for server '{}'.", server);
            println!("Discovered {} tool(s):", tools.len());
            for tool in &tools {
                println!("  - {}", tool.name);
            }
        }

        // ------------------------------------------------------------------
        // mcp test-invoke <server> <tool>
        // ------------------------------------------------------------------
        McpCommands::TestInvoke { server, tool } => {
            let client = registry.connect(&server).await?;
            let result = {
                let mut guard = client.lock().await;
                guard
                    .call_tool(&tool, serde_json::Value::Object(Default::default()))
                    .await?
            };
            if result.is_tool_error() {
                println!("Tool '{}' returned an error: {}", tool, result.text());
            } else {
                println!("Tool '{}' invocation succeeded.", tool);
                println!("Result: {}", result.text());
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::McpCommands;
    use crate::config::{McpConfig, McpServerConfig};

    // ------------------------------------------------------------------
    // execute tests (use Config::default() which has no MCP servers)
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_validate_with_empty_config_returns_ok() {
        let result = execute(McpCommands::Validate).await;
        assert!(
            result.is_ok(),
            "mcp validate with empty config should succeed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_list_servers_with_empty_config_returns_ok() {
        let result = execute(McpCommands::ListServers).await;
        assert!(
            result.is_ok(),
            "mcp list-servers with empty config should succeed: {:?}",
            result.err()
        );
    }

    // ------------------------------------------------------------------
    // execute_with_config tests
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_validate_with_invalid_config_returns_err() {
        let config = Config {
            mcp: McpConfig {
                servers: vec![McpServerConfig {
                    name: "test-server".to_string(),
                    command: String::new(), // empty command triggers validation failure
                    ..McpServerConfig::default()
                }],
                ..McpConfig::default()
            },
            ..Config::default()
        };
        let result = execute_with_config(McpCommands::Validate, &config).await;
        assert!(
            result.is_err(),
            "mcp validate with invalid config should fail"
        );
        assert!(matches!(result.unwrap_err(), PipelineError::Config(_)));
    }

    #[tokio::test]
    async fn test_execute_list_tools_returns_error_for_missing_server() {
        let config = Config::default(); // no servers configured
        let result = execute_with_config(
            McpCommands::ListTools {
                server: "nonexistent".to_string(),
            },
            &config,
        )
        .await;
        assert!(
            result.is_err(),
            "list-tools for missing server must return error"
        );
        assert!(matches!(
            result.unwrap_err(),
            PipelineError::McpServerNotFound { .. }
        ));
    }

    #[tokio::test]
    async fn test_execute_test_discovery_returns_error_for_missing_server() {
        let config = Config::default(); // no servers configured
        let result = execute_with_config(
            McpCommands::TestDiscovery {
                server: "nonexistent".to_string(),
            },
            &config,
        )
        .await;
        assert!(
            result.is_err(),
            "test-discovery for missing server must return error"
        );
        assert!(matches!(
            result.unwrap_err(),
            PipelineError::McpServerNotFound { .. }
        ));
    }
}
