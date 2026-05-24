use crate::agent::core::Agent;
use crate::config::Config;
use crate::error::Result;
use crate::providers::factory::ProviderFactory;
use crate::tools::file_ops::{ReadFileTool, WriteFileTool};
use crate::tools::git_ops::GitStatusTool;
use crate::tools::registry::ToolRegistry;
use crate::workflow::executor::WorkflowExecutor;
use crate::workflow::parser::parse_plan;
use std::path::Path;
use std::sync::Arc;

/// Reads, parses, and executes the workflow plan located at `plan_path`.
///
/// The plan format is inferred from the file extension (`yaml`, `json`, `md`).
/// Falls back to `yaml` when no extension is present.
pub async fn execute(plan_path: String) -> Result<()> {
    println!("Executing plan from: {}", plan_path);

    let content = std::fs::read_to_string(&plan_path).map_err(crate::error::PipelineError::Io)?;

    let extension = Path::new(&plan_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("yaml");

    let plan = parse_plan(&content, extension)?;
    println!("Plan: {}", plan.name);

    let config = Config::load()?;
    let provider = ProviderFactory::create_from_config(&config)?;

    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool::definition(), Arc::new(ReadFileTool));
    registry.register(WriteFileTool::definition(), Arc::new(WriteFileTool));
    registry.register(GitStatusTool::definition(), Arc::new(GitStatusTool));

    let system_prompt = "You are an autonomous agent executing a workflow plan.".to_string();
    let agent = Arc::new(Agent::new(provider, system_prompt, registry));

    let mut executor = WorkflowExecutor::new(agent, plan);

    executor.execute().await?;

    println!("Plan execution completed successfully.");
    Ok(())
}
