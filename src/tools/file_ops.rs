//! Sandboxed filesystem operation tools for agent file access.
//!
//! All tools in this module accept an [`Arc<PathValidator>`] and validate
//! every path through the sandbox before performing any I/O. Write tools
//! additionally require at least one write zone to be configured.

use crate::error::{PipelineError, Result};
use crate::providers::types::Tool;
use crate::tools::sandbox::PathValidator;
use crate::tools::{ToolExecutor, ToolResult};
use async_trait::async_trait;
use chrono::Utc;
use ignore::WalkBuilder;
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helper: simple glob pattern matching applied to a filename string
// ---------------------------------------------------------------------------

/// Returns `true` if `name` matches the glob `pattern`.
///
/// Supports `*` as a wildcard matching any sequence of characters (including
/// the empty sequence). `**` collapses to the same behaviour as `*` for
/// filename matching. Exact strings match exactly.
fn matches_pattern(pattern: &str, name: &str) -> bool {
    if !pattern.contains('*') {
        return name == pattern;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    // SAFETY: split always returns at least one element
    let first = parts[0];
    let last = *parts.last().unwrap();

    if !first.is_empty() && !name.starts_with(first) {
        return false;
    }
    if !last.is_empty() && !name.ends_with(last) {
        return false;
    }

    // Validate that all middle literal parts appear in order between the
    // prefix and suffix that were already anchored above.
    let start_offset = first.len();
    let end_offset = if last.is_empty() {
        name.len()
    } else {
        // Guard against underflow when prefix + suffix > total length
        match name.len().checked_sub(last.len()) {
            Some(v) if v >= start_offset => v,
            _ => return false,
        }
    };

    let middle = &name[start_offset..end_offset];
    let mut pos = 0usize;
    let middle_parts = if parts.len() >= 2 {
        &parts[1..parts.len() - 1]
    } else {
        &[][..]
    };
    for part in middle_parts {
        if part.is_empty() {
            continue;
        }
        match middle[pos..].find(part) {
            Some(found) => pos += found + part.len(),
            None => return false,
        }
    }

    true
}

// ---------------------------------------------------------------------------
// ReadFileTool
// ---------------------------------------------------------------------------

/// Tool that reads the full contents of a file within a sandboxed read zone.
pub struct ReadFileTool {
    validator: Arc<PathValidator>,
}

impl ReadFileTool {
    /// Creates a new `ReadFileTool` backed by the given [`PathValidator`].
    pub fn new(validator: Arc<PathValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl ToolExecutor for ReadFileTool {
    /// Returns the tool definition for `read_file`.
    fn tool_definition(&self) -> Tool {
        Tool {
            name: "read_file".to_string(),
            description: "Read the full contents of a file".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    /// Validates the path for read access, then returns the file contents.
    ///
    /// Returns [`ToolResult::failure`] if the file cannot be read after
    /// validation. Returns [`Err`] if sandbox validation fails.
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing path parameter".to_string()))?;
        let validated = self.validator.validate_read(Path::new(path_str))?;
        match fs::read_to_string(&validated) {
            Ok(content) => Ok(ToolResult::success(content)),
            Err(e) => Ok(ToolResult::failure(format!("Failed to read file: {}", e))),
        }
    }
}

// ---------------------------------------------------------------------------
// ListDirectoryTool
// ---------------------------------------------------------------------------

/// Tool that lists the entries of a directory within a sandboxed read zone.
pub struct ListDirectoryTool {
    validator: Arc<PathValidator>,
}

impl ListDirectoryTool {
    /// Creates a new `ListDirectoryTool` backed by the given [`PathValidator`].
    pub fn new(validator: Arc<PathValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl ToolExecutor for ListDirectoryTool {
    /// Returns the tool definition for `list_directory`.
    fn tool_definition(&self) -> Tool {
        Tool {
            name: "list_directory".to_string(),
            description: "List the entries of a directory".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the directory"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    /// Validates the path for read access, then lists directory entries.
    ///
    /// Each entry is formatted as `"type:name"` where `type` is either
    /// `"file"` or `"dir"`. Entries are joined by newline.
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing path parameter".to_string()))?;
        let validated = self.validator.validate_read(Path::new(path_str))?;
        let read_dir = match fs::read_dir(&validated) {
            Ok(rd) => rd,
            Err(e) => {
                return Ok(ToolResult::failure(format!(
                    "Failed to list directory: {}",
                    e
                )));
            }
        };
        let mut entries = Vec::new();
        for entry in read_dir.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let kind = if entry.path().is_dir() { "dir" } else { "file" };
            entries.push(format!("{}:{}", kind, name));
        }
        entries.sort();
        Ok(ToolResult::success(entries.join("\n")))
    }
}

// ---------------------------------------------------------------------------
// SearchFileContentsTool
// ---------------------------------------------------------------------------

/// Tool that searches a file's contents for lines containing a pattern.
pub struct SearchFileContentsTool {
    validator: Arc<PathValidator>,
}

impl SearchFileContentsTool {
    /// Creates a new `SearchFileContentsTool` backed by the given [`PathValidator`].
    pub fn new(validator: Arc<PathValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl ToolExecutor for SearchFileContentsTool {
    /// Returns the tool definition for `search_file_contents`.
    fn tool_definition(&self) -> Tool {
        Tool {
            name: "search_file_contents".to_string(),
            description: "Search a file for lines containing a pattern".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to search"
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Substring to search for in each line"
                    }
                },
                "required": ["path", "pattern"]
            }),
        }
    }

    /// Validates the path, reads the file, and returns matching lines.
    ///
    /// Each match is formatted as `"L{n}: {line}"` where `n` is the 1-based
    /// line number. Returns an empty string if no lines match.
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing path parameter".to_string()))?;
        let pattern = params["pattern"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing pattern parameter".to_string()))?;
        let validated = self.validator.validate_read(Path::new(path_str))?;
        let content = match fs::read_to_string(&validated) {
            Ok(c) => c,
            Err(e) => return Ok(ToolResult::failure(format!("Failed to read file: {}", e))),
        };
        let matches: Vec<String> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains(pattern))
            .map(|(i, line)| format!("L{}: {}", i + 1, line))
            .collect();
        Ok(ToolResult::success(matches.join("\n")))
    }
}

// ---------------------------------------------------------------------------
// FindFilesByGlobTool
// ---------------------------------------------------------------------------

/// Tool that walks a directory tree and returns files matching a glob pattern.
pub struct FindFilesByGlobTool {
    validator: Arc<PathValidator>,
}

impl FindFilesByGlobTool {
    /// Creates a new `FindFilesByGlobTool` backed by the given [`PathValidator`].
    pub fn new(validator: Arc<PathValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl ToolExecutor for FindFilesByGlobTool {
    /// Returns the tool definition for `find_files_by_glob`.
    fn tool_definition(&self) -> Tool {
        Tool {
            name: "find_files_by_glob".to_string(),
            description: "Walk a directory and return files whose name matches a glob pattern"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "root": {
                        "type": "string",
                        "description": "Root directory to walk"
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern to match against file names (e.g. *.rs)"
                    }
                },
                "required": ["root", "pattern"]
            }),
        }
    }

    /// Validates the root for read access, walks the tree, and returns
    /// matching file paths one per line.
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let root_str = params["root"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing root parameter".to_string()))?;
        let pattern = params["pattern"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing pattern parameter".to_string()))?;
        let validated_root = self.validator.validate_read(Path::new(root_str))?;

        let mut matches = Vec::new();
        for entry in WalkBuilder::new(&validated_root).build().flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if matches_pattern(pattern, &file_name) {
                matches.push(path.to_string_lossy().into_owned());
            }
        }
        matches.sort();
        Ok(ToolResult::success(matches.join("\n")))
    }
}

// ---------------------------------------------------------------------------
// ReadScanArtifactTool
// ---------------------------------------------------------------------------

/// Tool that reads a scan artifact YAML file and returns its raw contents.
pub struct ReadScanArtifactTool {
    validator: Arc<PathValidator>,
}

impl ReadScanArtifactTool {
    /// Creates a new `ReadScanArtifactTool` backed by the given [`PathValidator`].
    pub fn new(validator: Arc<PathValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl ToolExecutor for ReadScanArtifactTool {
    /// Returns the tool definition for `read_scan_artifact`.
    fn tool_definition(&self) -> Tool {
        Tool {
            name: "read_scan_artifact".to_string(),
            description: "Read the contents of a scan artifact YAML file".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the scan artifact YAML file"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    /// Validates the path for read access and returns the raw file contents.
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing path parameter".to_string()))?;
        let validated = self.validator.validate_read(Path::new(path_str))?;
        match fs::read_to_string(&validated) {
            Ok(content) => Ok(ToolResult::success(content)),
            Err(e) => Ok(ToolResult::failure(format!(
                "Failed to read scan artifact: {}",
                e
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// ReadWorkspaceMetadataTool
// ---------------------------------------------------------------------------

/// Tool that reads a workspace `state.yaml` file and returns it as pretty JSON.
pub struct ReadWorkspaceMetadataTool {
    validator: Arc<PathValidator>,
}

impl ReadWorkspaceMetadataTool {
    /// Creates a new `ReadWorkspaceMetadataTool` backed by the given [`PathValidator`].
    pub fn new(validator: Arc<PathValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl ToolExecutor for ReadWorkspaceMetadataTool {
    /// Returns the tool definition for `read_workspace_metadata`.
    fn tool_definition(&self) -> Tool {
        Tool {
            name: "read_workspace_metadata".to_string(),
            description: "Read a workspace state.yaml file and return it as JSON".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to workspace state.yaml file"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    /// Validates the path, parses the YAML, and returns a pretty-printed JSON string.
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing path parameter".to_string()))?;
        let validated = self.validator.validate_read(Path::new(path_str))?;
        let content = match fs::read_to_string(&validated) {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolResult::failure(format!(
                    "Failed to read workspace metadata: {}",
                    e
                )));
            }
        };
        let parsed: serde_json::Value = match serde_yaml::from_str(&content) {
            Ok(v) => v,
            Err(e) => return Ok(ToolResult::failure(format!("Failed to parse YAML: {}", e))),
        };
        match serde_json::to_string_pretty(&parsed) {
            Ok(json) => Ok(ToolResult::success(json)),
            Err(e) => Ok(ToolResult::failure(format!(
                "Failed to serialize as JSON: {}",
                e
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// WriteFileTool
// ---------------------------------------------------------------------------

/// Tool that writes content to a file within a sandboxed write zone.
pub struct WriteFileTool {
    validator: Arc<PathValidator>,
}

impl WriteFileTool {
    /// Creates a new `WriteFileTool` backed by the given [`PathValidator`].
    pub fn new(validator: Arc<PathValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl ToolExecutor for WriteFileTool {
    /// Returns the tool definition for `write_file`.
    fn tool_definition(&self) -> Tool {
        Tool {
            name: "write_file".to_string(),
            description: "Write content to a file within a sandboxed write zone".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Destination file path"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    /// Validates the path for write access, creates parent directories if
    /// needed, then writes the content.
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing path parameter".to_string()))?;
        let content = params["content"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing content parameter".to_string()))?;
        let validated = self.validator.validate_write(Path::new(path_str))?;
        if let Some(parent) = validated.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            return Ok(ToolResult::failure(format!(
                "Failed to create parent directories: {}",
                e
            )));
        }
        match fs::write(&validated, content) {
            Ok(_) => Ok(ToolResult::success(format!(
                "Successfully wrote to {}",
                validated.display()
            ))),
            Err(e) => Ok(ToolResult::failure(format!("Failed to write file: {}", e))),
        }
    }
}

// ---------------------------------------------------------------------------
// CreateDirectoryTool
// ---------------------------------------------------------------------------

/// Tool that creates a directory (and all parents) within a sandboxed write zone.
pub struct CreateDirectoryTool {
    validator: Arc<PathValidator>,
}

impl CreateDirectoryTool {
    /// Creates a new `CreateDirectoryTool` backed by the given [`PathValidator`].
    pub fn new(validator: Arc<PathValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl ToolExecutor for CreateDirectoryTool {
    /// Returns the tool definition for `create_directory`.
    fn tool_definition(&self) -> Tool {
        Tool {
            name: "create_directory".to_string(),
            description: "Create a directory (and all parents) within a sandboxed write zone"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to create"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    /// Validates the path for write access and creates the directory.
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing path parameter".to_string()))?;
        let validated = self.validator.validate_write(Path::new(path_str))?;
        match fs::create_dir_all(&validated) {
            Ok(_) => Ok(ToolResult::success(format!(
                "Created directory {}",
                validated.display()
            ))),
            Err(e) => Ok(ToolResult::failure(format!(
                "Failed to create directory: {}",
                e
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// WriteReportArtifactTool
// ---------------------------------------------------------------------------

/// Tool that writes a report artifact file within a sandboxed write zone.
pub struct WriteReportArtifactTool {
    validator: Arc<PathValidator>,
}

impl WriteReportArtifactTool {
    /// Creates a new `WriteReportArtifactTool` backed by the given [`PathValidator`].
    pub fn new(validator: Arc<PathValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl ToolExecutor for WriteReportArtifactTool {
    /// Returns the tool definition for `write_report_artifact`.
    fn tool_definition(&self) -> Tool {
        Tool {
            name: "write_report_artifact".to_string(),
            description: "Write a report artifact file within a sandboxed write zone".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Destination path for the report artifact"
                    },
                    "content": {
                        "type": "string",
                        "description": "Report content to write"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    /// Validates the path for write access, creates parent directories if
    /// needed, then writes the report content.
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing path parameter".to_string()))?;
        let content = params["content"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing content parameter".to_string()))?;
        let validated = self.validator.validate_write(Path::new(path_str))?;
        if let Some(parent) = validated.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            return Ok(ToolResult::failure(format!(
                "Failed to create parent directories: {}",
                e
            )));
        }
        match fs::write(&validated, content) {
            Ok(_) => Ok(ToolResult::success(format!(
                "Successfully wrote report artifact to {}",
                validated.display()
            ))),
            Err(e) => Ok(ToolResult::failure(format!(
                "Failed to write report artifact: {}",
                e
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// AppendDiagnosticTool
// ---------------------------------------------------------------------------

/// Tool that appends a structured YAML diagnostic entry to a log file.
pub struct AppendDiagnosticTool {
    validator: Arc<PathValidator>,
}

impl AppendDiagnosticTool {
    /// Creates a new `AppendDiagnosticTool` backed by the given [`PathValidator`].
    pub fn new(validator: Arc<PathValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl ToolExecutor for AppendDiagnosticTool {
    /// Returns the tool definition for `append_diagnostic`.
    fn tool_definition(&self) -> Tool {
        Tool {
            name: "append_diagnostic".to_string(),
            description: "Append a structured YAML diagnostic entry to a log file".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the diagnostics log file"
                    },
                    "message": {
                        "type": "string",
                        "description": "Diagnostic message"
                    },
                    "level": {
                        "type": "string",
                        "description": "Severity level (default: warning)"
                    }
                },
                "required": ["path", "message"]
            }),
        }
    }

    /// Validates the path for write access, then appends a YAML diagnostic
    /// entry with the given message, level, and a UTC timestamp.
    ///
    /// The level parameter defaults to `"warning"` if not provided.
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing path parameter".to_string()))?;
        let message = params["message"]
            .as_str()
            .ok_or_else(|| PipelineError::Tool("missing message parameter".to_string()))?;
        let level = params["level"].as_str().unwrap_or("warning");
        let timestamp = Utc::now().to_rfc3339();

        let validated = self.validator.validate_write(Path::new(path_str))?;
        if let Some(parent) = validated.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            return Ok(ToolResult::failure(format!(
                "Failed to create parent directories: {}",
                e
            )));
        }

        let entry = format!(
            "- message: \"{}\"\n  level: \"{}\"\n  timestamp: \"{}\"\n",
            message.replace('"', "\\\""),
            level,
            timestamp
        );

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&validated);

        match file {
            Ok(mut f) => match f.write_all(entry.as_bytes()) {
                Ok(_) => Ok(ToolResult::success(format!(
                    "Appended diagnostic to {}",
                    validated.display()
                ))),
                Err(e) => Ok(ToolResult::failure(format!(
                    "Failed to write diagnostic: {}",
                    e
                ))),
            },
            Err(e) => Ok(ToolResult::failure(format!(
                "Failed to open diagnostic file: {}",
                e
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn make_rw_validator(dir: &TempDir) -> Arc<PathValidator> {
        Arc::new(PathValidator::new(
            vec![dir.path().to_path_buf()],
            vec![dir.path().to_path_buf()],
        ))
    }

    fn make_ro_validator(dir: &TempDir) -> Arc<PathValidator> {
        Arc::new(PathValidator::read_only(vec![dir.path().to_path_buf()]))
    }

    // -----------------------------------------------------------------------
    // ReadFileTool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_read_file_tool_returns_file_contents() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("hello.txt");
        fs::write(&file, "hello world").unwrap();
        let tool = ReadFileTool::new(make_ro_validator(&dir));
        let result = tool
            .execute(json!({"path": file.to_str().unwrap()}))
            .await
            .unwrap();
        assert_eq!(result.output, "hello world");
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_read_file_tool_returns_failure_for_missing_file() {
        let dir = TempDir::new().unwrap();
        // File does not exist; canonicalize_for_read will fail, propagating Err
        let nonexistent = dir.path().join("nonexistent.txt");
        let tool = ReadFileTool::new(make_ro_validator(&dir));
        let result = tool
            .execute(json!({"path": nonexistent.to_str().unwrap()}))
            .await;
        assert!(
            result.is_err(),
            "expected Err for missing file, got {:?}",
            result
        );
    }

    // -----------------------------------------------------------------------
    // WriteFileTool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_write_file_tool_writes_content() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("output.txt");
        let tool = WriteFileTool::new(make_rw_validator(&dir));
        let result = tool
            .execute(json!({"path": file.to_str().unwrap(), "content": "test content"}))
            .await
            .unwrap();
        assert!(result.error.is_none());
        assert_eq!(fs::read_to_string(&file).unwrap(), "test content");
    }

    #[tokio::test]
    async fn test_write_file_tool_is_rejected_by_read_only_validator() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("output.txt");
        let tool = WriteFileTool::new(make_ro_validator(&dir));
        let result = tool
            .execute(json!({"path": file.to_str().unwrap(), "content": "data"}))
            .await;
        assert!(result.is_err(), "read-only validator should reject writes");
    }

    // -----------------------------------------------------------------------
    // ListDirectoryTool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_directory_tool_lists_entries() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), "").unwrap();
        fs::write(dir.path().join("b.txt"), "").unwrap();
        let tool = ListDirectoryTool::new(make_ro_validator(&dir));
        let result = tool
            .execute(json!({"path": dir.path().to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.output.contains("file:a.txt"));
        assert!(result.output.contains("file:b.txt"));
    }

    // -----------------------------------------------------------------------
    // SearchFileContentsTool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_search_file_contents_finds_matching_lines() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("log.txt");
        fs::write(&file, "line one\nerror here\nline three\n").unwrap();
        let tool = SearchFileContentsTool::new(make_ro_validator(&dir));
        let result = tool
            .execute(json!({"path": file.to_str().unwrap(), "pattern": "error"}))
            .await
            .unwrap();
        assert!(result.output.contains("L2: error here"));
    }

    #[tokio::test]
    async fn test_search_file_contents_returns_no_matches() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("clean.txt");
        fs::write(&file, "all good\nno issues\n").unwrap();
        let tool = SearchFileContentsTool::new(make_ro_validator(&dir));
        let result = tool
            .execute(json!({"path": file.to_str().unwrap(), "pattern": "error"}))
            .await
            .unwrap();
        assert!(result.output.is_empty());
    }

    // -----------------------------------------------------------------------
    // CreateDirectoryTool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_directory_tool_creates_directory() {
        let dir = TempDir::new().unwrap();
        let new_dir = dir.path().join("subdir");
        let tool = CreateDirectoryTool::new(make_rw_validator(&dir));
        let result = tool
            .execute(json!({"path": new_dir.to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.error.is_none());
        assert!(new_dir.exists());
        assert!(new_dir.is_dir());
    }

    // -----------------------------------------------------------------------
    // FindFilesByGlobTool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_find_files_by_glob_finds_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("alpha.txt"), "").unwrap();
        fs::write(dir.path().join("beta.rs"), "").unwrap();
        fs::write(dir.path().join("gamma.txt"), "").unwrap();
        let tool = FindFilesByGlobTool::new(make_ro_validator(&dir));
        let result = tool
            .execute(json!({"root": dir.path().to_str().unwrap(), "pattern": "*.txt"}))
            .await
            .unwrap();
        assert!(result.output.contains("alpha.txt"));
        assert!(result.output.contains("gamma.txt"));
        assert!(!result.output.contains("beta.rs"));
    }

    // -----------------------------------------------------------------------
    // ReadScanArtifactTool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_read_scan_artifact_returns_file_contents() {
        let dir = TempDir::new().unwrap();
        let artifact = dir.path().join("scan.yaml");
        fs::write(&artifact, "findings:\n  - id: 1\n").unwrap();
        let tool = ReadScanArtifactTool::new(make_ro_validator(&dir));
        let result = tool
            .execute(json!({"path": artifact.to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.output.contains("findings"));
        assert!(result.error.is_none());
    }

    // -----------------------------------------------------------------------
    // AppendDiagnosticTool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_append_diagnostic_appends_entry() {
        let dir = TempDir::new().unwrap();
        let log = dir.path().join("diagnostics.yaml");
        let tool = AppendDiagnosticTool::new(make_rw_validator(&dir));
        let result = tool
            .execute(json!({
                "path": log.to_str().unwrap(),
                "message": "something went wrong",
                "level": "error"
            }))
            .await
            .unwrap();
        assert!(result.error.is_none());
        let contents = fs::read_to_string(&log).unwrap();
        assert!(contents.contains("something went wrong"));
        assert!(contents.contains("error"));
        assert!(contents.contains("timestamp"));
    }
}
