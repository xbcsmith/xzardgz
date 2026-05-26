//! MCP server registry managing connections and tool executor construction.
//!
//! [`McpRegistry`] holds a snapshot of the MCP configuration and offers
//! methods to validate it, enumerate configured servers, connect to a server
//! (spawning its subprocess), list its tools, and build [`ToolExecutor`]
//! instances for integration into the agent tool registry.

use crate::config::{McpConfig, McpServerConfig};
use crate::error::{PipelineError, Result};
use crate::mcp::client::McpClient;
use crate::mcp::transport::StdioTransport;
#[cfg(test)]
use crate::mcp::transport::Transport;
use crate::mcp::types::McpToolDefinition;
use crate::providers::types::Tool;
use crate::tools::{ToolExecutor, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

// ---------------------------------------------------------------------------
// McpToolExecutor
// ---------------------------------------------------------------------------

/// Tool executor that delegates invocations to an MCP server.
///
/// Each instance corresponds to a single tool on a single server.  Multiple
/// `McpToolExecutor` instances may share the same client `Arc` so that all
/// tools on the same server reuse a single initialized session.
pub struct McpToolExecutor {
    server_name: String,
    tool_name: String,
    definition: McpToolDefinition,
    client: Arc<AsyncMutex<McpClient>>,
}

impl McpToolExecutor {
    /// Creates a new executor for a specific tool on a specific server.
    ///
    /// # Arguments
    ///
    /// * `server_name` - The logical name of the MCP server.
    /// * `tool_name`   - The canonical name of the tool.
    /// * `definition`  - The tool definition received from the server.
    /// * `client`      - A shared, initialized client for the server.
    pub fn new(
        server_name: String,
        tool_name: String,
        definition: McpToolDefinition,
        client: Arc<AsyncMutex<McpClient>>,
    ) -> Self {
        Self {
            server_name,
            tool_name,
            definition,
            client,
        }
    }
}

#[async_trait]
impl ToolExecutor for McpToolExecutor {
    /// Returns the provider-level [`Tool`] descriptor constructed from the
    /// MCP tool definition.
    fn tool_definition(&self) -> Tool {
        Tool {
            name: self.definition.name.clone(),
            description: self.definition.description.clone().unwrap_or_default(),
            parameters: self.definition.input_schema.clone(),
        }
    }

    /// Invokes the tool on the MCP server and maps the result to a
    /// [`ToolResult`].
    ///
    /// Returns [`ToolResult::failure`] when the server signals a tool-level
    /// error via `isError: true`; otherwise returns [`ToolResult::success`].
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::McpTransport`] or [`PipelineError::Mcp`] on
    /// transport or protocol failures.
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        tracing::debug!(
            server = %self.server_name,
            tool = %self.tool_name,
            "executing MCP tool"
        );
        let mut guard = self.client.lock().await;
        let result = guard.call_tool(&self.tool_name, params).await?;
        if result.is_tool_error() {
            Ok(ToolResult::failure(result.text()))
        } else {
            Ok(ToolResult::success(result.text()))
        }
    }
}

// ---------------------------------------------------------------------------
// McpRegistry
// ---------------------------------------------------------------------------

/// Manages MCP server configurations and creates client connections on demand.
///
/// The registry is constructed from [`McpConfig`] and operates statelessly:
/// each call to [`McpRegistry::connect`] spawns a fresh subprocess and
/// initializes a new session.  Callers that need long-lived sessions should
/// store the returned `Arc<AsyncMutex<McpClient>>`.
pub struct McpRegistry {
    config: McpConfig,
}

impl McpRegistry {
    /// Creates a new registry from the provided MCP configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - The MCP configuration block from the pipeline config.
    pub fn new(config: McpConfig) -> Self {
        Self { config }
    }

    /// Returns the names of all servers defined in the configuration.
    pub fn server_names(&self) -> Vec<String> {
        self.config.servers.iter().map(|s| s.name.clone()).collect()
    }

    /// Looks up a server's configuration by name.
    ///
    /// Returns `None` when no server with the given name is configured.
    pub fn get_server_config(&self, name: &str) -> Option<&McpServerConfig> {
        self.config.servers.iter().find(|s| s.name == name)
    }

    /// Connects to the named server, spawning its subprocess and performing
    /// the MCP `initialize` handshake.
    ///
    /// # Arguments
    ///
    /// * `server_name` - The name of the server to connect to.
    ///
    /// # Errors
    ///
    /// - [`PipelineError::McpServerNotFound`] if no server with that name is
    ///   configured.
    /// - [`PipelineError::McpTransport`] if the transport type is unsupported,
    ///   if the subprocess cannot be spawned, or if IO fails.
    /// - [`PipelineError::McpProtocolVersionMismatch`] if the server's
    ///   protocol version differs from the client's.
    pub async fn connect(&self, server_name: &str) -> Result<Arc<AsyncMutex<McpClient>>> {
        let server_config = self.get_server_config(server_name).ok_or_else(|| {
            PipelineError::McpServerNotFound {
                server: server_name.to_string(),
            }
        })?;

        if !server_config.transport.is_empty() && server_config.transport != "stdio" {
            return Err(PipelineError::McpTransport(format!(
                "unsupported transport: {}",
                server_config.transport
            )));
        }

        let timeout_secs = if server_config.timeout_seconds > 0 {
            server_config.timeout_seconds
        } else {
            self.config.timeout_seconds
        };

        let mut env: HashMap<String, String> = server_config.env.clone();

        if let Some(ref auth) = server_config.auth
            && let (Some(method), Some(env_var)) = (&auth.method, &auth.token_env)
            && method == "bearer"
        {
            match std::env::var(env_var) {
                Ok(token) => {
                    env.insert(env_var.clone(), token);
                }
                Err(_) => {
                    tracing::warn!(
                        server = %server_config.name,
                        env_var = %env_var,
                        "MCP auth bearer token env var not set; \
                         proceeding without it"
                    );
                }
            }
        }

        let transport = StdioTransport::spawn(
            &server_config.name,
            &server_config.command,
            &server_config.args,
            &env,
            timeout_secs,
        )
        .await?;

        let mut client = McpClient::new(server_name, Box::new(transport));
        client.initialize().await?;

        Ok(Arc::new(AsyncMutex::new(client)))
    }

    /// Connects to the named server and returns its list of available tools,
    /// filtered by the server's `allowed_tools` allowlist.
    ///
    /// An empty allowlist means all tools are allowed.
    ///
    /// # Errors
    ///
    /// Propagates errors from [`McpRegistry::connect`] and the `tools/list`
    /// RPC call.
    pub async fn list_tools(&self, server_name: &str) -> Result<Vec<McpToolDefinition>> {
        let client_arc = self.connect(server_name).await?;

        let tools = {
            let mut guard = client_arc.lock().await;
            guard.list_tools().await?
        };

        let allowlist: Vec<String> = self
            .get_server_config(server_name)
            .map(|c| c.allowed_tools.clone())
            .unwrap_or_default();

        if allowlist.is_empty() {
            Ok(tools)
        } else {
            Ok(tools
                .into_iter()
                .filter(|t| allowlist.contains(&t.name))
                .collect())
        }
    }

    /// Connects to the named server and builds a [`ToolExecutor`] for each
    /// available tool (after allowlist filtering).
    ///
    /// All executors share the same initialized client session.
    ///
    /// # Returns
    ///
    /// A vector of `(Tool, Arc<dyn ToolExecutor>)` pairs ready for
    /// registration in the agent tool registry.
    ///
    /// # Errors
    ///
    /// Propagates errors from [`McpRegistry::connect`] and `tools/list`.
    pub async fn build_tool_executors(
        &self,
        server_name: &str,
    ) -> Result<Vec<(Tool, Arc<dyn ToolExecutor>)>> {
        let client_arc = self.connect(server_name).await?;

        let tools = {
            let mut guard = client_arc.lock().await;
            guard.list_tools().await?
        };

        let allowlist: Vec<String> = self
            .get_server_config(server_name)
            .map(|c| c.allowed_tools.clone())
            .unwrap_or_default();

        let filtered: Vec<McpToolDefinition> = if allowlist.is_empty() {
            tools
        } else {
            tools
                .into_iter()
                .filter(|t| allowlist.contains(&t.name))
                .collect()
        };

        let mut result: Vec<(Tool, Arc<dyn ToolExecutor>)> = Vec::new();
        for tool_def in filtered {
            let executor = McpToolExecutor::new(
                server_name.to_string(),
                tool_def.name.clone(),
                tool_def,
                Arc::clone(&client_arc),
            );
            let tool = executor.tool_definition();
            result.push((tool, Arc::new(executor) as Arc<dyn ToolExecutor>));
        }

        Ok(result)
    }

    /// Validates the MCP configuration and returns a list of issue strings.
    ///
    /// An empty return value means the configuration is valid.  Issues are
    /// informational strings suitable for display to the user.
    ///
    /// Checks performed:
    /// - Each server must have a non-empty `name`.
    /// - Each server must have a non-empty `command`.
    /// - Each server's `transport` must be `""` or `"stdio"`.
    /// - A global `timeout_seconds` of `0` is flagged as informational.
    pub fn validate_config(&self) -> Vec<String> {
        let mut issues = Vec::new();

        for (i, server) in self.config.servers.iter().enumerate() {
            if server.name.is_empty() {
                issues.push(format!("server at index {} has empty name", i));
            }
            if server.command.is_empty() {
                issues.push(format!("server '{}' has empty command", server.name));
            }
            if !server.transport.is_empty() && server.transport != "stdio" {
                issues.push(format!(
                    "server '{}' uses transport '{}' which is not yet supported \
                     (only stdio is supported)",
                    server.name, server.transport
                ));
            }
        }

        if self.config.timeout_seconds == 0 {
            issues
                .push("global timeout_seconds is 0; using server-level timeouts only".to_string());
        }

        issues
    }
}

// ---------------------------------------------------------------------------
// Test-only helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
impl McpRegistry {
    /// Connects to the named server using an externally-supplied transport.
    ///
    /// Intended for unit tests that want to inject a [`MockTransport`] instead
    /// of spawning a real subprocess.
    pub(crate) async fn connect_with_transport(
        &self,
        server_name: &str,
        transport: Box<dyn Transport>,
    ) -> Result<Arc<AsyncMutex<McpClient>>> {
        let server_config = self.get_server_config(server_name).ok_or_else(|| {
            PipelineError::McpServerNotFound {
                server: server_name.to_string(),
            }
        })?;
        // Acknowledge that we have the config; we use the injected transport.
        let _ = server_config;
        let mut client = McpClient::new(server_name, transport);
        client.initialize().await?;
        Ok(Arc::new(AsyncMutex::new(client)))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{McpConfig, McpServerConfig};
    use crate::mcp::transport::MockTransport;
    use crate::mcp::types::JsonRpcResponse;
    use serde_json::json;

    // ------------------------------------------------------------------
    // Test helpers
    // ------------------------------------------------------------------

    fn make_test_config() -> McpConfig {
        McpConfig {
            servers: vec![McpServerConfig {
                name: "test-server".to_string(),
                command: "echo".to_string(),
                ..McpServerConfig::default()
            }],
            ..McpConfig::default()
        }
    }

    fn make_init_response(id: u64) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "serverInfo": {"name": "test-server", "version": "1.0.0"}
            })),
            error: None,
        }
    }

    fn make_tools_response(id: u64) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({
                "tools": [
                    {
                        "name": "echo",
                        "description": "Echo tool",
                        "inputSchema": {"type": "object", "properties": {}}
                    }
                ]
            })),
            error: None,
        }
    }

    fn make_call_result_response(id: u64) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({
                "content": [{"type": "text", "text": "hello"}],
                "isError": false
            })),
            error: None,
        }
    }

    fn make_error_call_result_response(id: u64) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({
                "content": [{"type": "text", "text": "Tool failed"}],
                "isError": true
            })),
            error: None,
        }
    }

    // ------------------------------------------------------------------
    // validate_config tests
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_config_passes_for_empty_config() {
        let registry = McpRegistry::new(McpConfig::default());
        let issues = registry.validate_config();
        assert!(
            issues.is_empty(),
            "empty config should have no issues, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_validate_config_fails_for_server_with_empty_name() {
        let config = McpConfig {
            servers: vec![McpServerConfig {
                name: String::new(),
                command: "echo".to_string(),
                ..McpServerConfig::default()
            }],
            ..McpConfig::default()
        };
        let registry = McpRegistry::new(config);
        let issues = registry.validate_config();
        assert!(
            issues.iter().any(|s| s.contains("empty name")),
            "expected empty name issue, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_validate_config_fails_for_server_with_empty_command() {
        let config = McpConfig {
            servers: vec![McpServerConfig {
                name: "test-server".to_string(),
                command: String::new(),
                ..McpServerConfig::default()
            }],
            ..McpConfig::default()
        };
        let registry = McpRegistry::new(config);
        let issues = registry.validate_config();
        assert!(
            issues.iter().any(|s| s.contains("empty command")),
            "expected empty command issue, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_validate_config_fails_for_unsupported_transport() {
        let config = McpConfig {
            servers: vec![McpServerConfig {
                name: "test-server".to_string(),
                command: "echo".to_string(),
                transport: "http".to_string(),
                ..McpServerConfig::default()
            }],
            ..McpConfig::default()
        };
        let registry = McpRegistry::new(config);
        let issues = registry.validate_config();
        assert!(
            issues.iter().any(|s| s.contains("http")),
            "expected unsupported transport issue, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_validate_config_passes_for_stdio_transport() {
        let config = McpConfig {
            servers: vec![McpServerConfig {
                name: "test-server".to_string(),
                command: "echo".to_string(),
                transport: "stdio".to_string(),
                ..McpServerConfig::default()
            }],
            ..McpConfig::default()
        };
        let registry = McpRegistry::new(config);
        let issues = registry.validate_config();
        // Stdio is explicitly supported; no transport-related issue expected.
        assert!(
            !issues.iter().any(|s| s.contains("transport")),
            "stdio transport should not produce issues, got: {:?}",
            issues
        );
    }

    // ------------------------------------------------------------------
    // server_names / get_server_config tests
    // ------------------------------------------------------------------

    #[test]
    fn test_server_names_returns_configured_names() {
        let config = McpConfig {
            servers: vec![
                McpServerConfig {
                    name: "server-1".to_string(),
                    command: "cmd1".to_string(),
                    ..McpServerConfig::default()
                },
                McpServerConfig {
                    name: "server-2".to_string(),
                    command: "cmd2".to_string(),
                    ..McpServerConfig::default()
                },
            ],
            ..McpConfig::default()
        };
        let registry = McpRegistry::new(config);
        let names = registry.server_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"server-1".to_string()));
        assert!(names.contains(&"server-2".to_string()));
    }

    #[test]
    fn test_get_server_config_returns_none_for_unknown_server() {
        let registry = McpRegistry::new(McpConfig::default());
        assert!(registry.get_server_config("nonexistent").is_none());
    }

    // ------------------------------------------------------------------
    // connect tests
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_connect_returns_error_for_missing_server() {
        let registry = McpRegistry::new(McpConfig::default());
        let result = registry.connect("nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PipelineError::McpServerNotFound { .. }
        ));
    }

    #[tokio::test]
    async fn test_connect_with_mock_transport_succeeds() {
        let registry = McpRegistry::new(make_test_config());
        let mock = MockTransport::with_responses(vec![Ok(make_init_response(1))]);

        let result = registry
            .connect_with_transport("test-server", Box::new(mock))
            .await;
        assert!(
            result.is_ok(),
            "connect with mock must succeed: {:?}",
            result.err()
        );
    }

    // ------------------------------------------------------------------
    // list_tools tests
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_tools_with_mock_returns_definitions() {
        let registry = McpRegistry::new(make_test_config());
        let mock = MockTransport::with_responses(vec![
            Ok(make_init_response(1)),
            Ok(make_tools_response(2)),
        ]);

        let client_arc = registry
            .connect_with_transport("test-server", Box::new(mock))
            .await
            .expect("connect must succeed");

        let tools = {
            let mut guard = client_arc.lock().await;
            guard.list_tools().await.expect("list_tools must succeed")
        };

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");
    }

    // ------------------------------------------------------------------
    // McpToolExecutor tests
    // ------------------------------------------------------------------

    #[test]
    fn test_mcp_tool_executor_tool_definition_matches_definition() {
        let tool_def = McpToolDefinition {
            name: "echo".to_string(),
            description: Some("Echo tool".to_string()),
            input_schema: json!({"type": "object"}),
        };
        let mock = MockTransport::empty();
        let client = McpClient::new("test-server", Box::new(mock));
        let client_arc = Arc::new(AsyncMutex::new(client));

        let executor = McpToolExecutor::new(
            "test-server".to_string(),
            "echo".to_string(),
            tool_def,
            client_arc,
        );

        let tool = executor.tool_definition();
        assert_eq!(tool.name, "echo");
        assert_eq!(tool.description, "Echo tool");
    }

    #[tokio::test]
    async fn test_mcp_tool_executor_execute_returns_success_result() {
        let mock = MockTransport::with_responses(vec![
            Ok(make_init_response(1)),
            Ok(make_call_result_response(2)),
        ]);
        let mut client = McpClient::new("test-server", Box::new(mock));
        client.initialize().await.expect("initialize must succeed");
        let client_arc = Arc::new(AsyncMutex::new(client));

        let tool_def = McpToolDefinition {
            name: "echo".to_string(),
            description: Some("Echo tool".to_string()),
            input_schema: json!({"type": "object"}),
        };
        let executor = McpToolExecutor::new(
            "test-server".to_string(),
            "echo".to_string(),
            tool_def,
            client_arc,
        );

        let result = executor
            .execute(json!({"text": "hello"}))
            .await
            .expect("execute must succeed");

        assert!(result.error.is_none());
        assert_eq!(result.output, "hello");
    }

    #[tokio::test]
    async fn test_mcp_tool_executor_execute_returns_failure_for_error_result() {
        let mock = MockTransport::with_responses(vec![
            Ok(make_init_response(1)),
            Ok(make_error_call_result_response(2)),
        ]);
        let mut client = McpClient::new("test-server", Box::new(mock));
        client.initialize().await.expect("initialize must succeed");
        let client_arc = Arc::new(AsyncMutex::new(client));

        let tool_def = McpToolDefinition {
            name: "failing-tool".to_string(),
            description: None,
            input_schema: json!({"type": "object"}),
        };
        let executor = McpToolExecutor::new(
            "test-server".to_string(),
            "failing-tool".to_string(),
            tool_def,
            client_arc,
        );

        let result = executor
            .execute(json!({}))
            .await
            .expect("execute itself must not error");

        assert!(
            result.error.is_some(),
            "tool error result must populate error field"
        );
        assert!(result.error.as_deref().unwrap().contains("Tool failed"));
    }
}
