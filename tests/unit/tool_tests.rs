use std::sync::Arc;
use tempfile::TempDir;
use xzardgz::providers::types::{FunctionCall, ToolCall};
use xzardgz::tools::executor::ToolExecutionDispatcher;
use xzardgz::tools::file_ops::{ReadFileTool, WriteFileTool};
use xzardgz::tools::registry::ToolRegistry;
use xzardgz::tools::sandbox::PathValidator;

#[tokio::test]
async fn test_tool_execution() {
    let dir = TempDir::new().unwrap();
    let validator = Arc::new(PathValidator::new(
        vec![dir.path().to_path_buf()],
        vec![dir.path().to_path_buf()],
    ));

    let mut registry = ToolRegistry::new();
    registry.register_executor(Arc::new(ReadFileTool::new(validator.clone())));
    registry.register_executor(Arc::new(WriteFileTool::new(validator.clone())));

    let dispatcher = ToolExecutionDispatcher::new(Arc::new(registry));

    // Write a file inside the sandbox zone
    let file_path = dir.path().join("test_output.txt");
    let path = file_path.to_str().unwrap().to_string();

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

    // Read the file back
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
