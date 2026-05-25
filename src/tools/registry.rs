//! Tool registry for discovering and dispatching named tools.
//!
//! The registry maps tool names to their definitions and executors. Use
//! [`build_read_only_registry`], [`build_read_write_registry`], or
//! [`build_subagent_registry`] to construct a pre-populated registry for
//! common use cases.

use crate::providers::types::Tool;
use crate::tools::ToolExecutor;
use crate::tools::file_ops::{
    AppendDiagnosticTool, CreateDirectoryTool, FindFilesByGlobTool, ListDirectoryTool,
    ReadFileTool, ReadScanArtifactTool, ReadWorkspaceMetadataTool, SearchFileContentsTool,
    WriteFileTool, WriteReportArtifactTool,
};
use crate::tools::sandbox::PathValidator;
use std::collections::HashMap;
use std::sync::Arc;

/// A registry mapping tool names to their definitions and executors.
///
/// Tools are registered by name. Duplicate registrations overwrite the
/// previous entry.
pub struct ToolRegistry {
    tools: HashMap<String, Tool>,
    executors: HashMap<String, Arc<dyn ToolExecutor>>,
}

impl ToolRegistry {
    /// Creates a new empty `ToolRegistry`.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            executors: HashMap::new(),
        }
    }

    /// Registers a tool definition and its executor under the tool's name.
    ///
    /// If a tool with the same name already exists, it is replaced.
    ///
    /// # Arguments
    ///
    /// * `tool` - The tool definition.
    /// * `executor` - The executor that handles calls to this tool.
    pub fn register(&mut self, tool: Tool, executor: Arc<dyn ToolExecutor>) {
        self.executors.insert(tool.name.clone(), executor);
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Convenience method that registers a tool using its own definition.
    ///
    /// Calls [`executor.tool_definition()`] to obtain the tool metadata, then
    /// delegates to [`Self::register`].
    ///
    /// # Arguments
    ///
    /// * `executor` - The executor whose [`ToolExecutor::tool_definition`] will
    ///   be used as the registration key.
    pub fn register_executor(&mut self, executor: Arc<dyn ToolExecutor>) {
        let tool = executor.tool_definition();
        self.register(tool, executor);
    }

    /// Returns the tool definition for the given name, or `None` if not found.
    pub fn get_tool(&self, name: &str) -> Option<&Tool> {
        self.tools.get(name)
    }

    /// Returns an executor for the given tool name, or `None` if not found.
    pub fn get_executor(&self, name: &str) -> Option<Arc<dyn ToolExecutor>> {
        self.executors.get(name).cloned()
    }

    /// Returns all registered tool definitions.
    pub fn list_tools(&self) -> Vec<Tool> {
        self.tools.values().cloned().collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Registry builder functions
// ---------------------------------------------------------------------------

/// Builds a read-only tool registry for review plugins.
///
/// Only read-only file tools are registered. Write tools are excluded.
/// The registry includes: `read_file`, `list_directory`, `search_file_contents`,
/// `find_files_by_glob`, `read_scan_artifact`, `read_workspace_metadata`.
///
/// # Arguments
///
/// * `validator` - The sandbox validator enforcing path restrictions.
pub fn build_read_only_registry(validator: Arc<PathValidator>) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register_executor(Arc::new(ReadFileTool::new(validator.clone())));
    registry.register_executor(Arc::new(ListDirectoryTool::new(validator.clone())));
    registry.register_executor(Arc::new(SearchFileContentsTool::new(validator.clone())));
    registry.register_executor(Arc::new(FindFilesByGlobTool::new(validator.clone())));
    registry.register_executor(Arc::new(ReadScanArtifactTool::new(validator.clone())));
    registry.register_executor(Arc::new(ReadWorkspaceMetadataTool::new(validator)));
    registry
}

/// Builds a read-write tool registry with full file access within sandbox zones.
///
/// Includes all tools from [`build_read_only_registry`] plus: `write_file`,
/// `create_directory`, `write_report_artifact`, `append_diagnostic`.
///
/// # Arguments
///
/// * `validator` - The sandbox validator enforcing path restrictions.
pub fn build_read_write_registry(validator: Arc<PathValidator>) -> ToolRegistry {
    let mut registry = build_read_only_registry(validator.clone());
    registry.register_executor(Arc::new(WriteFileTool::new(validator.clone())));
    registry.register_executor(Arc::new(CreateDirectoryTool::new(validator.clone())));
    registry.register_executor(Arc::new(WriteReportArtifactTool::new(validator.clone())));
    registry.register_executor(Arc::new(AppendDiagnosticTool::new(validator)));
    registry
}

/// Builds a subagent registry. Currently identical to the read-write registry.
///
/// Subagent delegation tools will be added when subagent support ships.
///
/// # Arguments
///
/// * `validator` - The sandbox validator enforcing path restrictions.
pub fn build_subagent_registry(validator: Arc<PathValidator>) -> ToolRegistry {
    build_read_write_registry(validator)
}

/// Builds an MCP-augmented registry by adding externally provided MCP tools.
///
/// Starts from a full read-write registry and appends each provided MCP tool.
///
/// # Arguments
///
/// * `validator` - The sandbox validator enforcing path restrictions.
/// * `mcp_tools` - Pairs of (tool definition, executor) to add to the registry.
pub fn build_mcp_augmented_registry(
    validator: Arc<PathValidator>,
    mcp_tools: Vec<(Tool, Arc<dyn ToolExecutor>)>,
) -> ToolRegistry {
    let mut registry = build_read_write_registry(validator);
    for (tool, executor) in mcp_tools {
        registry.register(tool, executor);
    }
    registry
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolResult;
    use async_trait::async_trait;
    use serde_json::Value;
    use tempfile::TempDir;

    fn make_rw_validator(dir: &TempDir) -> Arc<PathValidator> {
        Arc::new(PathValidator::new(
            vec![dir.path().to_path_buf()],
            vec![dir.path().to_path_buf()],
        ))
    }

    /// Minimal tool executor used in MCP augmentation tests.
    struct MockTool {
        name: String,
    }

    #[async_trait]
    impl ToolExecutor for MockTool {
        fn tool_definition(&self) -> Tool {
            Tool {
                name: self.name.clone(),
                description: "Mock tool for testing".to_string(),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            }
        }

        async fn execute(&self, _params: Value) -> crate::error::Result<ToolResult> {
            Ok(ToolResult::success("mock"))
        }
    }

    #[test]
    fn test_build_read_only_registry_has_read_tools() {
        let dir = TempDir::new().unwrap();
        let registry = build_read_only_registry(make_rw_validator(&dir));
        assert!(registry.get_tool("read_file").is_some());
        assert!(registry.get_tool("list_directory").is_some());
        assert!(registry.get_tool("search_file_contents").is_some());
        assert!(registry.get_tool("find_files_by_glob").is_some());
        assert!(registry.get_tool("read_scan_artifact").is_some());
        assert!(registry.get_tool("read_workspace_metadata").is_some());
    }

    #[test]
    fn test_build_read_only_registry_does_not_have_write_tools() {
        let dir = TempDir::new().unwrap();
        let registry = build_read_only_registry(make_rw_validator(&dir));
        assert!(registry.get_tool("write_file").is_none());
        assert!(registry.get_tool("create_directory").is_none());
        assert!(registry.get_tool("write_report_artifact").is_none());
        assert!(registry.get_tool("append_diagnostic").is_none());
    }

    #[test]
    fn test_build_read_write_registry_has_both_read_and_write_tools() {
        let dir = TempDir::new().unwrap();
        let registry = build_read_write_registry(make_rw_validator(&dir));
        // Read tools
        assert!(registry.get_tool("read_file").is_some());
        assert!(registry.get_tool("list_directory").is_some());
        // Write tools
        assert!(registry.get_tool("write_file").is_some());
        assert!(registry.get_tool("create_directory").is_some());
        assert!(registry.get_tool("write_report_artifact").is_some());
        assert!(registry.get_tool("append_diagnostic").is_some());
    }

    #[test]
    fn test_build_mcp_augmented_registry_includes_mcp_tools() {
        let dir = TempDir::new().unwrap();
        let mock: Arc<dyn ToolExecutor> = Arc::new(MockTool {
            name: "mcp_custom".to_string(),
        });
        let mcp_tool = mock.tool_definition();
        let registry =
            build_mcp_augmented_registry(make_rw_validator(&dir), vec![(mcp_tool, mock)]);
        assert!(registry.get_tool("mcp_custom").is_some());
        // Standard tools are also present
        assert!(registry.get_tool("read_file").is_some());
        assert!(registry.get_tool("write_file").is_some());
    }

    #[test]
    fn test_register_executor_registers_tool() {
        let mut registry = ToolRegistry::new();
        let executor: Arc<dyn ToolExecutor> = Arc::new(MockTool {
            name: "my_tool".to_string(),
        });
        registry.register_executor(executor);
        assert!(registry.get_tool("my_tool").is_some());
        assert!(registry.get_executor("my_tool").is_some());
    }
}
