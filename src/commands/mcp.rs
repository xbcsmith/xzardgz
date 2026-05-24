//! MCP management command handler.
//!
//! This module implements the `mcp` family of subcommands for managing and
//! introspecting Model Context Protocol server configurations.

use crate::cli::McpCommands;
use crate::error::Result;

/// Executes an MCP subcommand.
///
/// Dispatches to the appropriate print stub based on the variant of `command`.
/// Full MCP management (server discovery, tool invocation, transport handling)
/// is implemented in a later phase.
///
/// # Arguments
///
/// * `command` - The MCP subcommand to execute.
///
/// # Errors
///
/// This implementation does not currently return errors.
pub async fn execute(command: McpCommands) -> Result<()> {
    match command {
        McpCommands::Validate => {
            println!("Validating MCP server configuration: implemented in a later phase.");
        }
        McpCommands::ListServers => {
            println!("Configured MCP servers: implemented in a later phase.");
        }
        McpCommands::ListTools { server } => {
            println!(
                "Tools exposed by server '{}': implemented in a later phase.",
                server
            );
        }
        McpCommands::TestDiscovery { server } => {
            println!(
                "Testing tool discovery for server '{}': implemented in a later phase.",
                server
            );
        }
        McpCommands::TestInvoke { server, tool } => {
            println!(
                "Testing tool invocation: server='{}', tool='{}': implemented in a later phase.",
                server, tool
            );
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

    #[tokio::test]
    async fn test_execute_validate_returns_ok() {
        let result = execute(McpCommands::Validate).await;
        assert!(
            result.is_ok(),
            "mcp validate should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_list_servers_returns_ok() {
        let result = execute(McpCommands::ListServers).await;
        assert!(
            result.is_ok(),
            "mcp list-servers should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_list_tools_returns_ok() {
        let result = execute(McpCommands::ListTools {
            server: "main-server".to_string(),
        })
        .await;
        assert!(
            result.is_ok(),
            "mcp list-tools should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_test_discovery_returns_ok() {
        let result = execute(McpCommands::TestDiscovery {
            server: "main-server".to_string(),
        })
        .await;
        assert!(
            result.is_ok(),
            "mcp test-discovery should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_test_invoke_returns_ok() {
        let result = execute(McpCommands::TestInvoke {
            server: "main-server".to_string(),
            tool: "echo".to_string(),
        })
        .await;
        assert!(
            result.is_ok(),
            "mcp test-invoke should succeed, got: {:?}",
            result.err()
        );
    }
}
