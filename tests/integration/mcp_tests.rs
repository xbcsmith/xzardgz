//! Integration tests for the MCP client with [`MockTransport`].
//!
//! Verifies protocol initialization, tool discovery, tool invocation, and
//! error-handling paths of [`McpClient`] without spawning real subprocesses.
//! All transport I/O is satisfied by a pre-loaded [`MockTransport`] response
//! queue.

use serde_json::json;
use xzardgz::error::PipelineError;
use xzardgz::mcp::client::McpClient;
use xzardgz::mcp::transport::MockTransport;
use xzardgz::mcp::types::{JsonRpcError, JsonRpcResponse, MCP_PROTOCOL_VERSION};

// ---------------------------------------------------------------------------
// Test response builders
// ---------------------------------------------------------------------------

/// Returns a well-formed `initialize` response carrying the current protocol
/// version and a server identity of `"test-server"`.
fn make_init_response() -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: 1,
        result: Some(json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "serverInfo": {
                "name": "test-server",
                "version": "1.0.0"
            },
            "capabilities": {}
        })),
        error: None,
    }
}

/// Returns a `tools/list` response containing two tool definitions:
/// `read_file` and `list_directory`.
fn make_list_tools_response() -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: 2,
        result: Some(json!({
            "tools": [
                {
                    "name": "read_file",
                    "description": "Read a file from the filesystem",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"}
                        }
                    }
                },
                {
                    "name": "list_directory",
                    "description": "List files in a directory",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"}
                        }
                    }
                }
            ]
        })),
        error: None,
    }
}

/// Returns a successful `tools/call` response with a single text content item.
fn make_call_tool_response() -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: 3,
        result: Some(json!({
            "content": [{"type": "text", "text": "file contents here"}],
            "isError": false
        })),
        error: None,
    }
}

/// Returns an `initialize` response that carries an intentionally wrong
/// protocol version to trigger a mismatch error.
fn make_bad_protocol_response() -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: 1,
        result: Some(json!({
            "protocolVersion": "1999-01-01",
            "serverInfo": {"name": "bad-server", "version": "0.1"},
            "capabilities": {}
        })),
        error: None,
    }
}

/// Returns a JSON-RPC error response simulating a server-side internal error
/// (code -32603).
fn make_server_error_response() -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: 1,
        result: None,
        error: Some(JsonRpcError {
            code: -32603,
            message: "Internal error".to_string(),
            data: None,
        }),
    }
}

// ---------------------------------------------------------------------------
// initialize tests
// ---------------------------------------------------------------------------

/// Tests that [`McpClient::initialize`] succeeds when the mock transport
/// returns a valid `initialize` response, and that the parsed server identity
/// is accessible on the returned result.
#[tokio::test]
async fn test_mcp_client_initializes_with_mock_transport() {
    let transport = MockTransport::with_responses(vec![Ok(make_init_response())]);
    let mut client = McpClient::new("test-server", Box::new(transport));

    let result = client.initialize().await;

    assert!(
        result.is_ok(),
        "initialize must succeed with a valid mock response, got: {:?}",
        result.err()
    );

    // SAFETY: asserted is_ok() on the line above.
    let init_result = result.unwrap();

    assert_eq!(
        init_result.server_info.name, "test-server",
        "server_info.name must match the value from the initialize response"
    );
}

// ---------------------------------------------------------------------------
// list_tools tests
// ---------------------------------------------------------------------------

/// Tests that [`McpClient::list_tools`] returns the tool definitions from the
/// `tools/list` mock response, including the correct number of tools and the
/// expected names, after a successful initialization.
#[tokio::test]
async fn test_mcp_client_lists_tools_with_mock_transport() {
    let transport = MockTransport::with_responses(vec![
        Ok(make_init_response()),
        Ok(make_list_tools_response()),
    ]);
    let mut client = McpClient::new("test-server", Box::new(transport));

    client.initialize().await.expect("initialize must succeed");

    let result = client.list_tools().await;

    assert!(
        result.is_ok(),
        "list_tools must succeed with a valid mock response, got: {:?}",
        result.err()
    );

    // SAFETY: asserted is_ok() on the line above.
    let tools = result.unwrap();

    assert_eq!(tools.len(), 2, "tool list must contain exactly 2 entries");
    assert_eq!(tools[0].name, "read_file", "first tool must be read_file");
}

// ---------------------------------------------------------------------------
// call_tool tests
// ---------------------------------------------------------------------------

/// Tests that [`McpClient::call_tool`] returns a non-error result when the
/// mock transport returns a successful `tools/call` response, without
/// requiring a prior `list_tools` call.
#[tokio::test]
async fn test_mcp_client_calls_tool_with_mock_transport() {
    let transport = MockTransport::with_responses(vec![
        Ok(make_init_response()),
        Ok(make_call_tool_response()),
    ]);
    let mut client = McpClient::new("test-server", Box::new(transport));

    client.initialize().await.expect("initialize must succeed");

    let result = client
        .call_tool("read_file", json!({"path": "/tmp/test.txt"}))
        .await;

    assert!(
        result.is_ok(),
        "call_tool must succeed with a valid mock response, got: {:?}",
        result.err()
    );

    // SAFETY: asserted is_ok() on the line above.
    let call_result = result.unwrap();

    assert!(
        !call_result.is_tool_error(),
        "call result must not report a tool-level error"
    );
}

// ---------------------------------------------------------------------------
// Error-path tests
// ---------------------------------------------------------------------------

/// Tests that [`McpClient::initialize`] returns
/// [`PipelineError::McpProtocolVersionMismatch`] when the server reports a
/// `protocolVersion` that does not match the client's [`MCP_PROTOCOL_VERSION`].
#[tokio::test]
async fn test_mcp_client_rejects_mismatched_protocol_version() {
    let transport = MockTransport::with_responses(vec![Ok(make_bad_protocol_response())]);
    let mut client = McpClient::new("test-server", Box::new(transport));

    let result = client.initialize().await;

    assert!(
        result.is_err(),
        "initialize must fail when the server reports an incompatible protocol version"
    );

    // SAFETY: asserted is_err() on the line above.
    let err = result.unwrap_err();

    assert!(
        matches!(err, PipelineError::McpProtocolVersionMismatch { .. }),
        "error must be McpProtocolVersionMismatch, got: {:?}",
        err
    );

    let err_str = err.to_string();
    assert!(
        err_str.contains("protocol") || err_str.contains("1999-01-01"),
        "error message must reference the version mismatch, got: {err_str}"
    );
}

/// Tests that [`McpClient::initialize`] returns an error when the mock
/// transport queue is empty, simulating a transport-level failure before any
/// communication with the server.
#[tokio::test]
async fn test_mcp_client_returns_error_when_transport_exhausted() {
    let transport = MockTransport::empty();
    let mut client = McpClient::new("test-server", Box::new(transport));

    let result = client.initialize().await;

    assert!(
        result.is_err(),
        "initialize must fail when the transport queue has no queued responses"
    );
}

/// Tests that [`McpClient::initialize`] propagates a JSON-RPC server error
/// response (error field set, result absent) as a [`PipelineError::Mcp`],
/// with the server's error message preserved in the error string.
#[tokio::test]
async fn test_mcp_client_handles_server_error_response() {
    let transport = MockTransport::with_responses(vec![Ok(make_server_error_response())]);
    let mut client = McpClient::new("test-server", Box::new(transport));

    let result = client.initialize().await;

    assert!(
        result.is_err(),
        "initialize must fail when the server returns a JSON-RPC error object"
    );

    // SAFETY: asserted is_err() on the line above.
    let err = result.unwrap_err();

    assert!(
        matches!(err, PipelineError::Mcp(_)),
        "error must be PipelineError::Mcp for a JSON-RPC server error, got: {:?}",
        err
    );

    let err_str = err.to_string();
    assert!(
        err_str.contains("Internal error"),
        "error message must preserve the server error text, got: {err_str}"
    );
}
