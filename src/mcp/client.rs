//! MCP client implementing protocol initialization, tool discovery, and invocation.
//!
//! [`McpClient`] wraps a [`Transport`] and provides the three-phase MCP
//! interaction: `initialize` (protocol negotiation), `tools/list` (tool
//! discovery), and `tools/call` (tool invocation).  All state—server info,
//! protocol version, and a monotonic request counter—lives on the client
//! struct and is mutated during the session lifetime.

use crate::error::{PipelineError, Result};
use crate::mcp::transport::Transport;
use crate::mcp::types::{
    JsonRpcRequest, MCP_PROTOCOL_VERSION, McpInitializeResult, McpServerInfo, McpToolCallResult,
    McpToolDefinition,
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// McpClient
// ---------------------------------------------------------------------------

/// Stateful client for a single MCP server session.
///
/// The client must be initialized with [`McpClient::initialize`] before any
/// tool operations are performed.  [`McpClient::list_tools`] queries available
/// tools; [`McpClient::call_tool`] invokes a named tool.
///
/// The struct is not `Clone` because [`AtomicU64`] does not implement `Clone`
/// and because the transport itself is uniquely owned.  Share clients across
/// tasks by wrapping in `Arc<tokio::sync::Mutex<McpClient>>`.
pub struct McpClient {
    transport: Box<dyn Transport>,
    server_name: String,
    next_id: AtomicU64,
    server_info: Option<McpServerInfo>,
    protocol_version: Option<String>,
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("server_name", &self.server_name)
            .field("server_info", &self.server_info)
            .field("protocol_version", &self.protocol_version)
            .finish_non_exhaustive()
    }
}

impl McpClient {
    /// Creates a new, uninitialized MCP client backed by the given transport.
    ///
    /// Call [`McpClient::initialize`] before making any tool requests.
    ///
    /// # Arguments
    ///
    /// * `server_name` - Logical name of the server, used in error messages.
    /// * `transport`   - The underlying message transport.
    pub fn new(server_name: impl Into<String>, transport: Box<dyn Transport>) -> Self {
        Self {
            transport,
            server_name: server_name.into(),
            next_id: AtomicU64::new(1),
            server_info: None,
            protocol_version: None,
        }
    }

    /// Returns the next monotonically increasing request identifier.
    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Performs the MCP `initialize` handshake with the server.
    ///
    /// Sends client capabilities, verifies the server's reported protocol
    /// version matches [`MCP_PROTOCOL_VERSION`], stores the server identity,
    /// and sends the `notifications/initialized` notification.
    ///
    /// # Errors
    ///
    /// - [`PipelineError::McpTransport`] if the request cannot be delivered.
    /// - [`PipelineError::Mcp`] if the server returns a JSON-RPC error or an
    ///   unrecognizable response body.
    /// - [`PipelineError::McpProtocolVersionMismatch`] if the server's
    ///   `protocolVersion` does not match [`MCP_PROTOCOL_VERSION`].
    pub async fn initialize(&mut self) -> Result<McpInitializeResult> {
        let params = json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "xzardgz",
                // SAFETY: Set at compile time by Cargo, cannot fail.
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        let request = JsonRpcRequest::new(self.next_id(), "initialize", Some(params));
        let response = self.transport.send_request(request).await?;
        let result_value = response.into_result()?;

        let init_result: McpInitializeResult = serde_json::from_value(result_value)
            .map_err(|e| PipelineError::Mcp(format!("failed to parse initialize result: {e}")))?;

        if init_result.protocol_version != MCP_PROTOCOL_VERSION {
            return Err(PipelineError::McpProtocolVersionMismatch {
                expected: MCP_PROTOCOL_VERSION.to_string(),
                got: init_result.protocol_version.clone(),
            });
        }

        self.server_info = Some(init_result.server_info.clone());
        self.protocol_version = Some(init_result.protocol_version.clone());

        if let Err(e) = self
            .transport
            .send_notification("notifications/initialized", None)
            .await
        {
            tracing::warn!(
                server = %self.server_name,
                error = %e,
                "failed to send initialized notification; continuing"
            );
        }

        Ok(init_result)
    }

    /// Queries the server for its list of available tools.
    ///
    /// # Errors
    ///
    /// - [`PipelineError::McpTransport`] on communication failures.
    /// - [`PipelineError::Mcp`] if the response cannot be parsed.
    pub async fn list_tools(&mut self) -> Result<Vec<McpToolDefinition>> {
        let request = JsonRpcRequest::new(self.next_id(), "tools/list", None);
        let response = self.transport.send_request(request).await?;
        let result_value = response.into_result()?;

        let tools_json = result_value["tools"].clone();
        let tools: Vec<McpToolDefinition> = serde_json::from_value(tools_json)
            .map_err(|e| PipelineError::Mcp(format!("failed to parse tools/list response: {e}")))?;

        Ok(tools)
    }

    /// Invokes a named tool on the server with the given arguments.
    ///
    /// # Arguments
    ///
    /// * `name`      - The tool name as returned by [`McpClient::list_tools`].
    /// * `arguments` - A JSON object of arguments matching the tool's input schema.
    ///
    /// # Errors
    ///
    /// - [`PipelineError::McpTransport`] on communication failures.
    /// - [`PipelineError::Mcp`] if the response cannot be parsed.
    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<McpToolCallResult> {
        let params = json!({"name": name, "arguments": arguments});
        let request = JsonRpcRequest::new(self.next_id(), "tools/call", Some(params));
        let response = self.transport.send_request(request).await?;
        let result_value = response.into_result()?;

        let call_result: McpToolCallResult = serde_json::from_value(result_value)
            .map_err(|e| PipelineError::Mcp(format!("failed to parse tools/call response: {e}")))?;

        Ok(call_result)
    }

    /// Returns the server identity information received during initialization,
    /// or `None` if the client has not yet been initialized.
    pub fn server_info(&self) -> Option<&McpServerInfo> {
        self.server_info.as_ref()
    }

    /// Returns the protocol version string negotiated during initialization,
    /// or `None` if the client has not yet been initialized.
    pub fn protocol_version(&self) -> Option<&str> {
        self.protocol_version.as_deref()
    }

    /// Returns the logical server name provided at construction time.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::transport::MockTransport;
    use crate::mcp::types::{JsonRpcError, JsonRpcResponse};
    use serde_json::json;

    // ------------------------------------------------------------------
    // Test helpers
    // ------------------------------------------------------------------

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

    fn make_error_response(id: u64, message: &str) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32000,
                message: message.to_string(),
                data: None,
            }),
        }
    }

    // ------------------------------------------------------------------
    // initialize tests
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_initialize_succeeds_with_valid_response() {
        let mock = MockTransport::with_responses(vec![Ok(make_init_response(1))]);
        let mut client = McpClient::new("test-server", Box::new(mock));

        let result = client.initialize().await;
        assert!(
            result.is_ok(),
            "initialize must succeed: {:?}",
            result.err()
        );

        let info = client
            .server_info()
            .expect("server_info must be set after init");
        assert_eq!(info.name, "test-server");
        assert_eq!(info.version, "1.0.0");
        assert_eq!(client.protocol_version(), Some("2024-11-05"));
        assert_eq!(client.server_name(), "test-server");
    }

    #[tokio::test]
    async fn test_initialize_fails_on_protocol_version_mismatch() {
        let mock = MockTransport::with_responses(vec![Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: 1,
            result: Some(json!({
                "protocolVersion": "2099-01-01",
                "capabilities": {},
                "serverInfo": {"name": "future-server", "version": "99.0"}
            })),
            error: None,
        })]);
        let mut client = McpClient::new("test-server", Box::new(mock));

        let result = client.initialize().await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PipelineError::McpProtocolVersionMismatch { .. }
        ));
    }

    #[tokio::test]
    async fn test_initialize_fails_on_server_error_response() {
        let mock =
            MockTransport::with_responses(vec![Ok(make_error_response(1, "server not ready"))]);
        let mut client = McpClient::new("test-server", Box::new(mock));

        let result = client.initialize().await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("server not ready"));
    }

    // ------------------------------------------------------------------
    // list_tools tests
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_tools_returns_tool_definitions() {
        let mock = MockTransport::with_responses(vec![
            Ok(make_init_response(1)),
            Ok(make_tools_response(2)),
        ]);
        let mut client = McpClient::new("test-server", Box::new(mock));

        client.initialize().await.expect("initialize must succeed");
        let tools = client.list_tools().await.expect("list_tools must succeed");

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");
        assert_eq!(tools[0].description.as_deref(), Some("Echo tool"));
    }

    // ------------------------------------------------------------------
    // call_tool tests
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_call_tool_returns_result() {
        let mock = MockTransport::with_responses(vec![
            Ok(make_init_response(1)),
            Ok(make_tools_response(2)),
            Ok(make_call_result_response(3)),
        ]);
        let mut client = McpClient::new("test-server", Box::new(mock));

        client.initialize().await.expect("initialize must succeed");
        let _tools = client.list_tools().await.expect("list_tools must succeed");
        let result = client
            .call_tool("echo", json!({"text": "hello"}))
            .await
            .expect("call_tool must succeed");

        assert!(!result.is_tool_error());
        assert_eq!(result.text(), "hello");
    }

    #[tokio::test]
    async fn test_call_tool_returns_error_on_server_error() {
        let mock = MockTransport::with_responses(vec![
            Ok(make_init_response(1)),
            Ok(make_error_response(2, "tool not found")),
        ]);
        let mut client = McpClient::new("test-server", Box::new(mock));

        client.initialize().await.expect("initialize must succeed");
        let result = client.call_tool("missing", json!({})).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("tool not found"));
    }
}
