# Phase 12: MCP Client Layer Implementation

## Overview

Phase 12 implements the Model Context Protocol (MCP) client layer for XZardgz.
MCP is a JSON-RPC 2.0 based protocol that allows the agent to discover and
invoke tools exposed by external server processes over stdio pipes. This phase
wires the client into the command surface and lays the foundation for
integrating MCP tools into the agent tool registry.

## Files Created

| File                   | Purpose                                                  |
| ---------------------- | -------------------------------------------------------- |
| `src/mcp/types.rs`     | JSON-RPC 2.0 wire types and MCP protocol domain structs  |
| `src/mcp/transport.rs` | Transport trait, StdioTransport, and MockTransport       |
| `src/mcp/client.rs`    | McpClient implementing initialize, list_tools, call_tool |
| `src/mcp/registry.rs`  | McpRegistry, McpToolExecutor, configuration validation   |
| `src/mcp/mod.rs`       | Module declarations and crate-level documentation        |

## Files Modified

| File                  | Change                                                  |
| --------------------- | ------------------------------------------------------- |
| `src/lib.rs`          | Added `pub mod mcp;`                                    |
| `src/commands/mcp.rs` | Replaced stub with real execute and execute_with_config |

## Architecture

### Protocol Types (types.rs)

The types module defines the JSON-RPC 2.0 framing layer plus the MCP-specific
structs:

- `JsonRpcRequest` / `JsonRpcResponse` / `JsonRpcError` - wire format types with
  serde serialization matching the JSON-RPC 2.0 specification.
- `McpToolDefinition` - a tool advertisement from a server's `tools/list`
  response. Uses `#[serde(rename_all = "camelCase")]` to handle the MCP wire
  format (`inputSchema` becomes `input_schema` in Rust).
- `McpToolCallResult` / `McpContent` - the result of a `tools/call` invocation.
  `isError` maps to `is_error` via the same camelCase rename.
- `McpInitializeResult` - the protocol negotiation result including `serverInfo`
  and `protocolVersion`.
- `MCP_PROTOCOL_VERSION = "2024-11-05"` - the version constant used in all
  handshakes.

### Transport Abstraction (transport.rs)

The `Transport` trait abstracts over all communication channels:

```rust
pub trait Transport: Send + Sync {
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse>;
    async fn send_notification(&self, method: &str, params: Option<Value>) -> Result<()>;
}
```

`StdioTransport` implements the trait by:

1. Spawning a child process with `kill_on_drop(true)`.
2. Taking stdin and stdout from the child before moving the child into the
   internal state struct (the child is kept alive by the struct field).
3. Wrapping all IO in `tokio::time::timeout` to enforce per-server deadlines.
4. Using `Arc<AsyncMutex<StdioState>>` to allow the transport to be shared.

`MockTransport` holds a `Mutex<VecDeque<Result<JsonRpcResponse>>>` and serves
responses sequentially. Notifications always succeed silently. This is the only
transport used in unit tests.

### MCP Client (client.rs)

`McpClient` is a stateful single-session client:

- `new(server_name, transport)` - constructs an uninitialized client.
- `initialize()` - performs the three-step handshake: send `initialize`,
  validate the protocol version, send `notifications/initialized`. Stores
  `server_info` and `protocol_version` for later introspection. If the
  notification delivery fails, a warning is logged but the error is not
  propagated (the server may not require it).
- `list_tools()` - sends `tools/list` and deserializes the result.
- `call_tool(name, arguments)` - sends `tools/call` with the tool name and
  argument JSON object.
- A monotonic `AtomicU64` request counter is used for request IDs, enabling
  `next_id()` to be called from `&self` even though other methods take
  `&mut self`.

The struct implements a manual `Debug` because `Box<dyn Transport>` does not
implement `Debug` and a derived impl is not possible.

### Registry (registry.rs)

`McpRegistry` is the main integration point:

- `new(config)` - takes a snapshot of the `McpConfig`.
- `server_names()` / `get_server_config(name)` - synchronous lookups.
- `connect(server_name)` - spawns the server subprocess, initializes the client,
  and returns `Arc<AsyncMutex<McpClient>>`. Handles auth token injection via
  bearer token env-var lookup.
- `list_tools(server_name)` - connects and calls `list_tools`, then applies the
  per-server `allowed_tools` allowlist filter.
- `build_tool_executors(server_name)` - connects, lists tools (with filtering),
  and builds `McpToolExecutor` instances that share the same client Arc.
- `validate_config()` - synchronously checks all configured servers for missing
  names, missing commands, and unsupported transports.

`McpToolExecutor` implements `ToolExecutor` from `crate::tools`:

- `tool_definition()` maps the `McpToolDefinition` to a
  `providers::types::Tool`.
- `execute(params)` locks the client, calls `call_tool`, and maps
  `isError: true` responses to `ToolResult::failure`.

### Command Handler (commands/mcp.rs)

The command handler follows the same two-function pattern used by
`commands/auth.rs`:

- `execute(command)` loads `Config::default()` and calls `execute_with_config`.
- `execute_with_config(command, config)` constructs an `McpRegistry` and
  dispatches on the `McpCommands` variant.

## Serde Field Mapping

MCP uses camelCase JSON field names that differ from Rust's snake_case
convention. The mapping is handled via `#[serde(rename_all = "camelCase")]` on
struct definitions:

| Rust field         | JSON wire name         |
| ------------------ | ---------------------- |
| `input_schema`     | `inputSchema`          |
| `is_error`         | `isError`              |
| `protocol_version` | `protocolVersion`      |
| `server_info`      | `serverInfo`           |
| `content_type`     | `type` (manual rename) |

## Error Handling

All error conditions map to named `PipelineError` variants defined in
`src/error.rs`:

| Condition                 | Variant                                        |
| ------------------------- | ---------------------------------------------- |
| Server not in config      | `McpServerNotFound { server }`                 |
| Transport or IO failure   | `McpTransport(String)`                         |
| Request timeout           | `McpTimeout { server, timeout_ms }`            |
| JSON-RPC error response   | `Mcp(String)`                                  |
| Protocol version mismatch | `McpProtocolVersionMismatch { expected, got }` |
| Auth credential missing   | Warning only; not an error by default          |

## Testing Strategy

All logic is tested without real processes using `MockTransport`. The mock holds
a `VecDeque` of pre-queued `Result<JsonRpcResponse>` values and pops one per
`send_request` call. Notifications consume nothing from the queue.

Test-only methods are gated with `#[cfg(test)]`:

- `McpRegistry::connect_with_transport` injects a transport directly, bypassing
  subprocess spawning. This enables full registry logic testing (init plus tool
  list) in a single coherent flow.

### Test Count by Module

| Module           | Test count |
| ---------------- | ---------- |
| `mcp::types`     | 7          |
| `mcp::transport` | 3          |
| `mcp::client`    | 6          |
| `mcp::registry`  | 13         |
| `commands::mcp`  | 5          |
| Total new        | 34         |

## Quality Gate Results

All four quality gates passed:

- `cargo fmt --all` - clean
- `cargo check --all-targets --all-features` - clean
- `cargo clippy --all-targets --all-features -- -D warnings` - clean
- `cargo test --all-features` - 763 tests, 0 failed
