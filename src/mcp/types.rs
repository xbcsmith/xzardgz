//! MCP protocol types for JSON-RPC 2.0 communication.
//!
//! Defines the wire-format structs used to exchange messages with MCP servers
//! over a JSON-RPC 2.0 framing layer, plus the higher-level MCP domain types
//! for tool definitions, tool call results, and protocol initialization.

use crate::error::{PipelineError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Protocol version
// ---------------------------------------------------------------------------

/// The MCP protocol version this client implements.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 types
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 request object.
///
/// # Examples
///
/// ```
/// use xzardgz::mcp::types::JsonRpcRequest;
///
/// let req = JsonRpcRequest::new(1, "tools/list", None);
/// assert_eq!(req.jsonrpc, "2.0");
/// assert_eq!(req.id, 1);
/// assert_eq!(req.method, "tools/list");
/// assert!(req.params.is_none());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// The JSON-RPC protocol version string, always `"2.0"`.
    pub jsonrpc: String,
    /// The unique identifier for this request.
    pub id: u64,
    /// The RPC method name to invoke.
    pub method: String,
    /// Optional parameters to pass to the method.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    /// Creates a new JSON-RPC 2.0 request with the given id, method, and params.
    ///
    /// The `jsonrpc` field is always set to `"2.0"`.
    ///
    /// # Arguments
    ///
    /// * `id`     - The request identifier. Must be unique within a session.
    /// * `method` - The RPC method name.
    /// * `params` - Optional JSON parameters for the method.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::mcp::types::JsonRpcRequest;
    /// use serde_json::json;
    ///
    /// let req = JsonRpcRequest::new(42, "tools/call", Some(json!({"name": "echo"})));
    /// assert_eq!(req.jsonrpc, "2.0");
    /// assert_eq!(req.id, 42);
    /// ```
    pub fn new(id: u64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC 2.0 error object embedded in a failed response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// The numeric error code.
    pub code: i64,
    /// A short, human-readable description of the error.
    pub message: String,
    /// Optional additional error data.
    #[serde(default)]
    pub data: Option<Value>,
}

/// A JSON-RPC 2.0 response object received from an MCP server.
///
/// Exactly one of `result` or `error` should be present in a valid response.
/// [`JsonRpcResponse::into_result`] enforces this invariant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// The JSON-RPC protocol version string, always `"2.0"`.
    pub jsonrpc: String,
    /// The identifier matching the original request.
    pub id: u64,
    /// The success result, present when the call succeeded.
    #[serde(default)]
    pub result: Option<Value>,
    /// The error object, present when the call failed.
    #[serde(default)]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Converts this response into a `Result<Value>`, returning the result
    /// value on success or a [`PipelineError::Mcp`] on failure.
    ///
    /// Returns `Err` when:
    /// - `error` is present (the server returned an error).
    /// - Both `result` and `error` are absent (malformed response).
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Mcp`] when the response contains an error or
    /// is missing both `result` and `error`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::mcp::types::JsonRpcResponse;
    /// use serde_json::json;
    ///
    /// let resp = JsonRpcResponse {
    ///     jsonrpc: "2.0".to_string(),
    ///     id: 1,
    ///     result: Some(json!({"tools": []})),
    ///     error: None,
    /// };
    /// let val = resp.into_result().unwrap();
    /// assert!(val.get("tools").is_some());
    /// ```
    pub fn into_result(self) -> Result<Value> {
        if let Some(e) = self.error {
            return Err(PipelineError::Mcp(format!(
                "JSON-RPC error {}: {}",
                e.code, e.message
            )));
        }
        self.result.ok_or_else(|| {
            PipelineError::Mcp("JSON-RPC response has neither result nor error".to_string())
        })
    }
}

// ---------------------------------------------------------------------------
// MCP domain types
// ---------------------------------------------------------------------------

/// The definition of a callable tool advertised by an MCP server.
///
/// Received as part of the `tools/list` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDefinition {
    /// The canonical name of the tool.
    pub name: String,
    /// A human-readable description of what the tool does.
    pub description: Option<String>,
    /// JSON Schema describing the tool's accepted input parameters.
    pub input_schema: Value,
}

/// A single content item returned by an MCP tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpContent {
    /// The content type identifier (e.g. `"text"`, `"image"`).
    #[serde(rename = "type")]
    pub content_type: String,
    /// The text payload, present when `content_type` is `"text"`.
    #[serde(default)]
    pub text: Option<String>,
}

/// The result returned by an MCP `tools/call` invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallResult {
    /// The content items produced by the tool.
    pub content: Vec<McpContent>,
    /// Whether the tool itself reported an error condition.
    #[serde(default)]
    pub is_error: Option<bool>,
}

impl McpToolCallResult {
    /// Returns all text content joined with newlines.
    ///
    /// Content items without text (e.g. images) are skipped.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::mcp::types::{McpContent, McpToolCallResult};
    ///
    /// let result = McpToolCallResult {
    ///     content: vec![
    ///         McpContent { content_type: "text".to_string(), text: Some("hello".to_string()) },
    ///         McpContent { content_type: "text".to_string(), text: Some("world".to_string()) },
    ///     ],
    ///     is_error: None,
    /// };
    /// assert_eq!(result.text(), "hello\nworld");
    /// ```
    pub fn text(&self) -> String {
        let parts: Vec<&str> = self
            .content
            .iter()
            .filter_map(|c| c.text.as_deref())
            .collect();
        parts.join("\n")
    }

    /// Returns `true` if the tool reported an error result.
    ///
    /// Defaults to `false` when the `isError` field is absent from the
    /// server's response.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::mcp::types::McpToolCallResult;
    ///
    /// let ok_result = McpToolCallResult { content: vec![], is_error: None };
    /// assert!(!ok_result.is_tool_error());
    ///
    /// let err_result = McpToolCallResult { content: vec![], is_error: Some(true) };
    /// assert!(err_result.is_tool_error());
    /// ```
    pub fn is_tool_error(&self) -> bool {
        self.is_error.unwrap_or(false)
    }
}

/// Client identity information sent to the server during MCP initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpClientInfo {
    /// The client application name.
    pub name: String,
    /// The client application version string.
    pub version: String,
}

/// Server identity information received during MCP initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    /// The server application name.
    pub name: String,
    /// The server application version string.
    pub version: String,
}

/// The parsed result of a successful MCP `initialize` exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInitializeResult {
    /// The protocol version the server reported.
    pub protocol_version: String,
    /// The server's capability declarations as a free-form JSON object.
    pub capabilities: Value,
    /// The server's identity information.
    pub server_info: McpServerInfo,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_json_rpc_request_new_sets_jsonrpc_version() {
        let req = JsonRpcRequest::new(1, "tools/list", None);
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.id, 1);
        assert_eq!(req.method, "tools/list");
        assert!(req.params.is_none());
    }

    #[test]
    fn test_json_rpc_response_into_result_returns_ok_when_result_present() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: 1,
            result: Some(json!({"answer": 42})),
            error: None,
        };
        let val = resp.into_result();
        assert!(val.is_ok());
        assert_eq!(val.unwrap()["answer"], 42);
    }

    #[test]
    fn test_json_rpc_response_into_result_returns_err_when_error_present() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: 1,
            result: None,
            error: Some(JsonRpcError {
                code: -32600,
                message: "Invalid request".to_string(),
                data: None,
            }),
        };
        let val = resp.into_result();
        assert!(val.is_err());
        let err_str = val.unwrap_err().to_string();
        assert!(err_str.contains("Invalid request"));
    }

    #[test]
    fn test_json_rpc_response_into_result_returns_err_when_both_none() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: 1,
            result: None,
            error: None,
        };
        let val = resp.into_result();
        assert!(val.is_err());
        let err_str = val.unwrap_err().to_string();
        assert!(err_str.contains("neither result nor error"));
    }

    #[test]
    fn test_mcp_tool_call_result_text_joins_content() {
        let result = McpToolCallResult {
            content: vec![
                McpContent {
                    content_type: "text".to_string(),
                    text: Some("hello".to_string()),
                },
                McpContent {
                    content_type: "text".to_string(),
                    text: Some("world".to_string()),
                },
            ],
            is_error: None,
        };
        assert_eq!(result.text(), "hello\nworld");
    }

    #[test]
    fn test_mcp_tool_call_result_is_tool_error_returns_false_by_default() {
        let result = McpToolCallResult {
            content: vec![],
            is_error: None,
        };
        assert!(!result.is_tool_error());
    }

    #[test]
    fn test_mcp_tool_definition_serializes_correctly() {
        let def = McpToolDefinition {
            name: "echo".to_string(),
            description: Some("Echoes input".to_string()),
            input_schema: json!({"type": "object", "properties": {"text": {"type": "string"}}}),
        };
        let serialized = serde_json::to_string(&def).expect("serialization must succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&serialized).expect("must be valid JSON");
        assert_eq!(parsed["name"], "echo");
        assert_eq!(parsed["description"], "Echoes input");
        // Verify camelCase rename on the wire
        assert!(parsed.get("inputSchema").is_some());
        assert!(parsed.get("input_schema").is_none());
    }
}
