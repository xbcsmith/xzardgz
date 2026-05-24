//! Prompt management command handler.
//!
//! This module implements the `prompts` family of subcommands for managing,
//! validating, and inspecting prompt templates used by workflow plugins.

use crate::cli::PromptsCommands;
use crate::error::Result;

/// Executes a prompts subcommand.
///
/// Dispatches to the appropriate print stub based on the variant of `command`.
/// Full prompt management (directory discovery, template rendering, override
/// resolution) is implemented in a later phase.
///
/// # Arguments
///
/// * `command` - The prompts subcommand to execute.
///
/// # Errors
///
/// This implementation does not currently return errors.
pub async fn execute(command: PromptsCommands) -> Result<()> {
    match command {
        PromptsCommands::Export { output_dir } => {
            let dir = output_dir.as_deref().unwrap_or(".xzardgz/prompts");
            println!(
                "Exporting built-in prompts to: {}. Implemented in a later phase.",
                dir
            );
        }
        PromptsCommands::Validate => {
            println!("Validating prompt directories: implemented in a later phase.");
        }
        PromptsCommands::ShowOrder => {
            println!(
                "Prompt resolution order: 1. CLI override, 2. Config directories, 3. Built-in defaults."
            );
        }
        PromptsCommands::ListTemplates { plugin } => {
            println!(
                "Prompt templates for plugin '{}': implemented in a later phase.",
                plugin
            );
        }
        PromptsCommands::Render { template, context } => {
            println!(
                "Rendering template '{}' with context: {:?}.",
                template, context
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::PromptsCommands;

    #[tokio::test]
    async fn test_execute_export_returns_ok() {
        let result = execute(PromptsCommands::Export { output_dir: None }).await;
        assert!(
            result.is_ok(),
            "prompts export should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_validate_returns_ok() {
        let result = execute(PromptsCommands::Validate).await;
        assert!(
            result.is_ok(),
            "prompts validate should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_show_order_returns_ok() {
        let result = execute(PromptsCommands::ShowOrder).await;
        assert!(
            result.is_ok(),
            "prompts show-order should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_list_templates_returns_ok() {
        let result = execute(PromptsCommands::ListTemplates {
            plugin: "technical-review".to_string(),
        })
        .await;
        assert!(
            result.is_ok(),
            "prompts list-templates should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_render_returns_ok() {
        let result = execute(PromptsCommands::Render {
            template: "review-summary".to_string(),
            context: None,
        })
        .await;
        assert!(
            result.is_ok(),
            "prompts render should succeed, got: {:?}",
            result.err()
        );
    }
}
