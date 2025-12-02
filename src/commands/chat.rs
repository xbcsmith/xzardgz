use crate::agent::core::Agent;
use crate::config::Config;
use crate::error::XzardgzError;
use crate::providers::factory::ProviderFactory;
use crate::tools::file_ops::{ReadFileTool, WriteFileTool};
use crate::tools::git_ops::GitStatusTool;
use crate::tools::registry::ToolRegistry;
use std::io::{self, Write};
use std::sync::Arc;

pub async fn run(initial_message: Option<String>) -> Result<(), XzardgzError> {
    // 1. Load Config
    let config = Config::load()?;
    println!("Loaded config: Provider={}", config.provider.provider_type);

    // 2. Create Provider
    let provider = ProviderFactory::create(&config.provider)?;

    // 3. Create Tool Registry
    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool::definition(), Arc::new(ReadFileTool));
    registry.register(WriteFileTool::definition(), Arc::new(WriteFileTool));
    registry.register(GitStatusTool::definition(), Arc::new(GitStatusTool));

    // 4. Create Agent
    let system_prompt =
        "You are XZardgz, an autonomous AI agent. You can read/write files and check git status."
            .to_string();
    let agent = Agent::new(provider, system_prompt, registry);

    // 5. Run Loop
    if let Some(msg) = initial_message {
        println!("User: {}", msg);
        let response = agent.run(&msg).await?;
        println!("Agent: {}", response);
    } else {
        println!("Starting interactive chat. Type 'exit' or 'quit' to leave.");
        loop {
            print!("> ");
            io::stdout().flush().map_err(XzardgzError::Io)?;

            let mut input = String::new();
            io::stdin()
                .read_line(&mut input)
                .map_err(XzardgzError::Io)?;
            let input = input.trim();

            if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
                break;
            }

            if input.is_empty() {
                continue;
            }

            match agent.run(input).await {
                Ok(response) => println!("Agent: {}", response),
                Err(e) => println!("Error: {}", e),
            }
        }
    }

    Ok(())
}
