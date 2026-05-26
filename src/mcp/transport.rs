//! MCP transport abstraction over stdio subprocess channels and mock testing.
//!
//! The [`Transport`] trait provides a uniform interface for sending JSON-RPC
//! requests and notifications.  [`StdioTransport`] implements it by spawning a
//! child process and communicating over its stdin/stdout pipes.
//! [`MockTransport`] implements it with a pre-populated response queue suitable
//! for unit testing client logic without real processes.

use crate::error::{PipelineError, Result};
use crate::mcp::types::{JsonRpcRequest, JsonRpcResponse};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::{Duration, timeout};

// ---------------------------------------------------------------------------
// Transport trait
// ---------------------------------------------------------------------------

/// Asynchronous transport for JSON-RPC 2.0 messages to an MCP server.
///
/// Implementations must be `Send + Sync` so they can be stored behind an
/// `Arc` and used from multiple async tasks.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Sends a JSON-RPC request and waits for the corresponding response.
    ///
    /// # Arguments
    ///
    /// * `request` - The JSON-RPC request to send.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::McpTransport`] on serialization or IO failures,
    /// or [`PipelineError::McpTimeout`] when no response arrives within the
    /// configured deadline.
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse>;

    /// Sends a JSON-RPC notification (fire-and-forget; no response expected).
    ///
    /// # Arguments
    ///
    /// * `method` - The notification method name.
    /// * `params` - Optional notification parameters.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::McpTransport`] on serialization or IO failures.
    async fn send_notification(&self, method: &str, params: Option<Value>) -> Result<()>;
}

// ---------------------------------------------------------------------------
// StdioTransport
// ---------------------------------------------------------------------------

/// Internal mutable state owned by a [`StdioTransport`].
struct StdioState {
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    /// Keeps the child process alive for the lifetime of the transport.
    _child: tokio::process::Child,
}

/// Transport implementation that communicates with an MCP server via the
/// server process's stdin/stdout pipes.
///
/// The child process is spawned with `kill_on_drop(true)` so it is
/// automatically terminated when this transport is dropped.
pub struct StdioTransport {
    state: Arc<AsyncMutex<StdioState>>,
    timeout_secs: u64,
    server_name: String,
}

impl StdioTransport {
    /// Spawns an MCP server subprocess and wraps its stdio as a transport.
    ///
    /// # Arguments
    ///
    /// * `server_name`  - Logical name used in error messages.
    /// * `command`      - Executable to run.
    /// * `args`         - Command-line arguments.
    /// * `env`          - Extra environment variables for the child process.
    /// * `timeout_secs` - Per-request timeout in seconds.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::McpTransport`] if the process cannot be spawned
    /// or if its stdio pipes are unavailable.
    pub async fn spawn(
        server_name: impl Into<String>,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        timeout_secs: u64,
    ) -> Result<Self> {
        let mut child = tokio::process::Command::new(command)
            .args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                PipelineError::McpTransport(format!("failed to spawn '{}': {e}", command))
            })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            PipelineError::McpTransport("child process has no stdin pipe".to_string())
        })?;
        let child_stdout = child.stdout.take().ok_or_else(|| {
            PipelineError::McpTransport("child process has no stdout pipe".to_string())
        })?;

        let state = StdioState {
            stdin,
            stdout: BufReader::new(child_stdout),
            _child: child,
        };

        Ok(Self {
            state: Arc::new(AsyncMutex::new(state)),
            timeout_secs,
            server_name: server_name.into(),
        })
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        timeout(Duration::from_secs(self.timeout_secs), async {
            let mut state = self.state.lock().await;

            let line = serde_json::to_string(&request).map_err(|e| {
                PipelineError::McpTransport(format!("request serialize error: {e}"))
            })?;

            let framed = format!("{line}\n");
            state
                .stdin
                .write_all(framed.as_bytes())
                .await
                .map_err(|e| PipelineError::McpTransport(format!("stdin write error: {e}")))?;
            state
                .stdin
                .flush()
                .await
                .map_err(|e| PipelineError::McpTransport(format!("stdin flush error: {e}")))?;

            let mut response_line = String::new();
            state
                .stdout
                .read_line(&mut response_line)
                .await
                .map_err(|e| PipelineError::McpTransport(format!("stdout read error: {e}")))?;

            serde_json::from_str::<JsonRpcResponse>(&response_line)
                .map_err(|e| PipelineError::McpTransport(format!("response parse error: {e}")))
        })
        .await
        .map_err(|_| PipelineError::McpTimeout {
            server: self.server_name.clone(),
            timeout_ms: self.timeout_secs * 1000,
        })?
    }

    async fn send_notification(&self, method: &str, params: Option<Value>) -> Result<()> {
        let notif = serde_json::json!({"jsonrpc": "2.0", "method": method, "params": params});
        let line = serde_json::to_string(&notif).map_err(|e| {
            PipelineError::McpTransport(format!("notification serialize error: {e}"))
        })?;

        let framed = format!("{line}\n");
        let mut state = self.state.lock().await;
        state
            .stdin
            .write_all(framed.as_bytes())
            .await
            .map_err(|e| PipelineError::McpTransport(format!("notification write error: {e}")))?;
        state
            .stdin
            .flush()
            .await
            .map_err(|e| PipelineError::McpTransport(format!("notification flush error: {e}")))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MockTransport
// ---------------------------------------------------------------------------

/// Mock transport for unit testing MCP client logic without real processes.
///
/// Pre-load a queue of responses with [`MockTransport::with_responses`]; each
/// call to [`send_request`] dequeues and returns the next entry. If the queue
/// is exhausted the request returns [`PipelineError::McpTransport`].
/// Notifications always succeed without consuming a queued entry.
pub struct MockTransport {
    responses: Mutex<VecDeque<Result<JsonRpcResponse>>>,
}

impl MockTransport {
    /// Creates a mock transport pre-loaded with the given response sequence.
    ///
    /// # Arguments
    ///
    /// * `responses` - Ordered list of results to return from `send_request`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::mcp::transport::MockTransport;
    /// use xzardgz::mcp::types::JsonRpcResponse;
    /// use serde_json::json;
    ///
    /// let transport = MockTransport::with_responses(vec![
    ///     Ok(JsonRpcResponse { jsonrpc: "2.0".to_string(), id: 1, result: Some(json!({})), error: None }),
    /// ]);
    /// ```
    pub fn with_responses(responses: Vec<Result<JsonRpcResponse>>) -> Self {
        Self {
            responses: Mutex::new(VecDeque::from(responses)),
        }
    }

    /// Creates an empty mock transport that returns an exhausted error for
    /// every request.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::mcp::transport::MockTransport;
    ///
    /// let transport = MockTransport::empty();
    /// ```
    pub fn empty() -> Self {
        Self {
            responses: Mutex::new(VecDeque::new()),
        }
    }

    /// Appends a response to the back of the queue.
    ///
    /// # Arguments
    ///
    /// * `response` - The result to enqueue.
    pub fn push_response(&self, response: Result<JsonRpcResponse>) {
        // SAFETY: This mutex is only poisoned if a thread panics while holding
        // the lock. In unit tests a panic is a test failure; the mock is never
        // used across poisoned-mutex boundaries.
        self.responses.lock().unwrap().push_back(response);
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn send_request(&self, _request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        // SAFETY: Same poisoning rationale as push_response.
        let entry = self.responses.lock().unwrap().pop_front();
        match entry {
            Some(response) => response,
            None => Err(PipelineError::McpTransport(
                "mock transport exhausted: no more queued responses".to_string(),
            )),
        }
    }

    async fn send_notification(&self, _method: &str, _params: Option<Value>) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_ok_response(id: u64) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({"ok": true})),
            error: None,
        }
    }

    #[tokio::test]
    async fn test_mock_transport_returns_queued_responses_in_order() {
        let transport =
            MockTransport::with_responses(vec![Ok(make_ok_response(1)), Ok(make_ok_response(2))]);
        let req1 = JsonRpcRequest::new(1, "ping", None);
        let req2 = JsonRpcRequest::new(2, "ping", None);

        let r1 = transport
            .send_request(req1)
            .await
            .expect("first response must succeed");
        let r2 = transport
            .send_request(req2)
            .await
            .expect("second response must succeed");

        assert_eq!(r1.id, 1);
        assert_eq!(r2.id, 2);
    }

    #[tokio::test]
    async fn test_mock_transport_returns_error_when_exhausted() {
        let transport = MockTransport::empty();
        let req = JsonRpcRequest::new(1, "ping", None);
        let result = transport.send_request(req).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("exhausted"));
    }

    #[tokio::test]
    async fn test_mock_transport_push_response_adds_to_queue() {
        let transport = MockTransport::empty();
        transport.push_response(Ok(make_ok_response(99)));

        let req = JsonRpcRequest::new(99, "ping", None);
        let result = transport
            .send_request(req)
            .await
            .expect("pushed response must be returned");
        assert_eq!(result.id, 99);
    }
}
