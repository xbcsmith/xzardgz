use crate::agent::core::Agent;
use crate::config::Config;
use crate::error::XzardgzError;
use crate::providers::factory::ProviderFactory;
use crate::tools::file_ops::{ReadFileTool, WriteFileTool};
use crate::tools::git_ops::GitStatusTool;
use crate::tools::registry::ToolRegistry;
use crate::workflow::executor::WorkflowExecutor;
use crate::workflow::parser::parse_plan;
use std::path::Path;
use std::sync::Arc;

pub async fn execute(plan_path: String) -> Result<(), XzardgzError> {
    println!("Executing plan from: {}", plan_path);

    // 1. Read plan file
    let content = std::fs::read_to_string(&plan_path).map_err(XzardgzError::Io)?;

    let extension = Path::new(&plan_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("yaml");

    // 2. Parse plan
    let plan = parse_plan(&content, extension)?;
    println!("Plan: {}", plan.name);

    // 3. Initialize Agent (needed for executor)
    let config = Config::load()?;
    let provider = ProviderFactory::create(&config.provider)?;

    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool::definition(), Arc::new(ReadFileTool));
    registry.register(WriteFileTool::definition(), Arc::new(WriteFileTool));
    registry.register(GitStatusTool::definition(), Arc::new(GitStatusTool));

    let system_prompt = "You are an autonomous agent executing a workflow plan.".to_string();
    let agent = Arc::new(Agent::new(provider, system_prompt, registry));

    // 4. Initialize Executor
    let mut executor = WorkflowExecutor::new(agent, plan);

    // 5. Execute
    executor.execute().await?;

    println!("Plan execution completed successfully.");
    Ok(())
}
