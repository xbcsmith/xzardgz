//! Model Context Protocol (MCP) client layer.
//!
//! Implements MCP server configuration parsing, transport initialization,
//! protocol version negotiation, tool discovery, and tool invocation.
//! MCP tools can be added to agent tool registries when explicitly configured.
//!
//! # Module structure
//!
//! - [`types`]: JSON-RPC 2.0 wire types and MCP protocol domain types.
//! - [`transport`]: [`transport::Transport`] trait plus [`transport::StdioTransport`]
//!   and [`transport::MockTransport`] implementations.
//! - [`client`]: [`client::McpClient`] managing a single server session.
//! - [`registry`]: [`registry::McpRegistry`] connecting to configured servers
//!   and constructing [`crate::tools::ToolExecutor`] instances.

pub mod client;
pub mod registry;
pub mod transport;
pub mod types;
