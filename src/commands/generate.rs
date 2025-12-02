use crate::agent::core::Agent;
use crate::config::Config;
use crate::docgen::diataxis::DocCategory;
use crate::docgen::generator::DocGenerator;
use crate::docgen::templates::TemplateRegistry;
use crate::docgen::writer::DocumentWriter;
use crate::error::Result;
use crate::providers::factory::ProviderFactory;
use crate::tools::registry::ToolRegistry;
use std::path::PathBuf;
use std::sync::Arc;

pub async fn execute(
    repository: String,
    category: DocCategory,
    topic: String,
    output: String,
    overwrite: bool,
) -> Result<()> {
    println!("Generating {} documentation for '{}'...", category, topic);

    let config = Config::load()?;
    let provider = ProviderFactory::create(&config.provider)?;
    let tools = ToolRegistry::new();
    let agent = Arc::new(Agent::new(
        provider,
        "You are a documentation expert.".to_string(),
        tools,
    ));
    let templates = Arc::new(TemplateRegistry::new()?);
    let generator = DocGenerator::new(agent, templates);
    let writer = DocumentWriter::new(PathBuf::from(output), overwrite);

    // TODO: Scan repository to get context
    // For now, we'll just use a placeholder or read a summary file if it exists
    let context = format!("Repository at {}", repository);

    let content = generator.generate(category, &topic, &context).await?;
    let path = writer
        .write(
            category,
            &format!("{}.md", topic.replace(" ", "_").to_lowercase()),
            &content,
        )
        .await?;

    println!("Documentation generated at: {:?}", path);
    Ok(())
}
