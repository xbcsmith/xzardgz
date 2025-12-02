use std::sync::Arc;
use tempfile::NamedTempFile;
use xzardgz::providers::types::{FunctionCall, ToolCall};
use xzardgz::tools::executor::ToolExecutionDispatcher;
use xzardgz::tools::file_ops::{ReadFileTool, WriteFileTool};
use xzardgz::tools::registry::ToolRegistry;

#[tokio::test]
async fn test_tool_execution() {
    let mut registry = ToolRegistry::new();

    // Register tools
    registry.register(ReadFileTool::definition(), Arc::new(ReadFileTool));
    registry.register(WriteFileTool::definition(), Arc::new(WriteFileTool));

    let dispatcher = ToolExecutionDispatcher::new(Arc::new(registry));

    // Test Write
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap().to_string();

    let write_call = ToolCall {
        id: "call_1".to_string(),
        function: FunctionCall {
            name: "write_file".to_string(),
            arguments: format!(r#"{{"path": "{}", "content": "Hello Tool"}}"#, path),
        },
    };

    let result = dispatcher.execute(&write_call).await.unwrap();
    assert!(result.error.is_none());
    assert!(result.output.contains("Successfully wrote"));

    // Test Read
    let read_call = ToolCall {
        id: "call_2".to_string(),
        function: FunctionCall {
            name: "read_file".to_string(),
            arguments: format!(r#"{{"path": "{}"}}"#, path),
        },
    };

    let result = dispatcher.execute(&read_call).await.unwrap();
    assert!(result.error.is_none());
    assert_eq!(result.output, "Hello Tool");
}

#[tokio::test]
async fn test_unknown_tool() {
    let registry = ToolRegistry::new();
    let dispatcher = ToolExecutionDispatcher::new(Arc::new(registry));

    let call = ToolCall {
        id: "call_3".to_string(),
        function: FunctionCall {
            name: "unknown".to_string(),
            arguments: "{}".to_string(),
        },
    };

    let result = dispatcher.execute(&call).await;
    assert!(result.is_err());
}
