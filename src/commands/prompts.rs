//! Prompt management command handler.
//!
//! Implements the `prompts` family of subcommands: exporting embedded templates
//! to disk, validating configured override directories, displaying the runtime
//! resolution order, listing per-plugin template keys, and rendering a template
//! with an optional JSON context for debugging.

use std::path::PathBuf;

use crate::cli::PromptsCommands;
use crate::error::{PipelineError, Result};
use crate::prompts::PromptLoader;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Executes a prompts subcommand.
///
/// Dispatches to the appropriate handler based on the `command` variant.
/// Template resolution and rendering are performed via [`PromptLoader`].
///
/// # Arguments
///
/// * `command` - The prompts subcommand to execute.
///
/// # Errors
///
/// Returns [`PipelineError::Io`] if a filesystem operation fails during
/// `Export`. Returns [`PipelineError::Prompt`] for an unknown plugin name, a
/// missing template, or a malformed JSON context string supplied to `Render`.
pub async fn execute(command: PromptsCommands) -> Result<()> {
    match command {
        // ------------------------------------------------------------------
        // Export
        // ------------------------------------------------------------------
        PromptsCommands::Export { output_dir } => {
            let dir = output_dir.as_deref().unwrap_or(".xzardgz/prompts");
            for plugin in PromptLoader::known_plugins() {
                for key in PromptLoader::known_keys(plugin) {
                    let plugin_dir = PathBuf::from(dir).join(plugin);
                    std::fs::create_dir_all(&plugin_dir)?;
                    let file_path = plugin_dir.join(format!("{key}.tera"));
                    // SAFETY: known_keys() only returns keys that have embedded
                    // templates, so embedded_raw() is guaranteed to return Some.
                    let content = PromptLoader::embedded_raw(plugin, key).unwrap();
                    std::fs::write(&file_path, content)?;
                    println!("exported: {}", file_path.display());
                }
            }
        }

        // ------------------------------------------------------------------
        // Validate
        // ------------------------------------------------------------------
        PromptsCommands::Validate => {
            println!("Validating prompt directories: checking configured override paths.");
            let config = crate::config::PromptsConfig::default();
            if config.allow_overrides {
                for dir in &config.directories {
                    let path = std::path::Path::new(dir);
                    if path.exists() {
                        println!("  {} (found)", path.display());
                    } else {
                        println!(
                            "  {} (not found - will use embedded defaults)",
                            path.display()
                        );
                    }
                }
            }
            println!("Embedded defaults are always available regardless of override directories.");
        }

        // ------------------------------------------------------------------
        // ShowOrder
        // ------------------------------------------------------------------
        PromptsCommands::ShowOrder => {
            let plugins = PromptLoader::known_plugins().join(", ");
            println!("Prompt template resolution order (highest to lowest priority):");
            println!();
            println!("  1. In-memory overrides (programmatic only)");
            println!();
            println!("  2. File-based overrides");
            println!("     Enabled: true (PromptsConfig.allow_overrides)");
            println!("     Search directories (first match wins):");
            println!("       .xzardgz/prompts  (default)");
            println!("     File pattern: {{directory}}/{{plugin}}/{{key}}.tera");
            println!();
            println!("  3. Compiled-in embedded defaults (always available)");
            println!("     Embedded plugins: {plugins}");
            println!("     Each plugin provides a 'system' template.");
            println!();
            println!("To override a template, export the defaults and edit:");
            println!("  xzardgz prompts export");
            println!("  # then edit .xzardgz/prompts/{{plugin}}/{{key}}.tera");
        }

        // ------------------------------------------------------------------
        // ListTemplates
        // ------------------------------------------------------------------
        PromptsCommands::ListTemplates { plugin } => {
            let normalized = plugin.replace('-', "_");
            let keys = PromptLoader::known_keys(&normalized);
            if keys.is_empty() {
                eprintln!("error: unknown plugin '{}'. Known plugins:", plugin);
                for p in PromptLoader::known_plugins() {
                    eprintln!("  {}", p);
                }
                return Err(PipelineError::Prompt(format!("unknown plugin: {plugin}")));
            }
            println!("Templates for plugin '{}':", normalized);
            for key in keys {
                println!("  {}", key);
            }
        }

        // ------------------------------------------------------------------
        // Render
        // ------------------------------------------------------------------
        PromptsCommands::Render {
            plugin,
            key,
            context,
        } => {
            let normalized_plugin = plugin.replace('-', "_");

            let mut ctx = tera::Context::new();
            if let Some(ref json_str) = context {
                let val: serde_json::Value = serde_json::from_str(json_str)
                    .map_err(|e| PipelineError::Prompt(format!("invalid JSON context: {e}")))?;
                if let serde_json::Value::Object(map) = val {
                    for (k, v) in map {
                        ctx.insert(&k, &v);
                    }
                }
            }

            let loader = PromptLoader::default();
            let result = loader.render(&normalized_plugin, &key, &ctx);
            if result.is_empty() {
                eprintln!(
                    "error: no template found for plugin '{}', key '{}'",
                    normalized_plugin, key
                );
                return Err(PipelineError::Prompt(format!(
                    "no template found for plugin '{}', key '{}'",
                    normalized_plugin, key
                )));
            }
            println!("{}", result);
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

    // ------------------------------------------------------------------
    // Export
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_export_writes_template_files_to_directory() {
        let tmp = tempfile::tempdir().expect("failed to create tempdir");
        let tmp_path = tmp.path().to_string_lossy().to_string();
        let result = execute(PromptsCommands::Export {
            output_dir: Some(tmp_path),
        })
        .await;
        assert!(
            result.is_ok(),
            "export should succeed, got: {:?}",
            result.err()
        );
        assert!(
            tmp.path()
                .join("security_review")
                .join("system.tera")
                .exists(),
            "security_review/system.tera should exist after export"
        );
        assert!(
            tmp.path()
                .join("technical_review")
                .join("system.tera")
                .exists(),
            "technical_review/system.tera should exist after export"
        );
    }

    #[tokio::test]
    async fn test_execute_export_files_are_valid_tera_templates() {
        let tmp = tempfile::tempdir().expect("failed to create tempdir");
        let tmp_path = tmp.path().to_string_lossy().to_string();
        execute(PromptsCommands::Export {
            output_dir: Some(tmp_path),
        })
        .await
        // SAFETY: previous test already validates this path succeeds.
        .unwrap();

        let content =
            std::fs::read_to_string(tmp.path().join("security_review").join("system.tera"))
                .expect("should be able to read exported template");
        assert!(
            !content.is_empty(),
            "exported template content should not be empty"
        );

        let render_result = tera::Tera::one_off(&content, &tera::Context::new(), false);
        assert!(
            render_result.is_ok(),
            "exported template should be valid Tera, got: {:?}",
            render_result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_export_exports_both_plugins() {
        let tmp = tempfile::tempdir().expect("failed to create tempdir");
        let tmp_path = tmp.path().to_string_lossy().to_string();
        let result = execute(PromptsCommands::Export {
            output_dir: Some(tmp_path),
        })
        .await;
        assert!(
            result.is_ok(),
            "export should succeed, got: {:?}",
            result.err()
        );
        assert!(
            tmp.path()
                .join("security_review")
                .join("system.tera")
                .exists(),
            "security_review/system.tera should exist"
        );
        assert!(
            tmp.path()
                .join("technical_review")
                .join("system.tera")
                .exists(),
            "technical_review/system.tera should exist"
        );
    }

    // ------------------------------------------------------------------
    // Validate
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_validate_returns_ok() {
        let result = execute(PromptsCommands::Validate).await;
        assert!(
            result.is_ok(),
            "prompts validate should succeed, got: {:?}",
            result.err()
        );
    }

    // ------------------------------------------------------------------
    // ShowOrder
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_show_order_returns_ok() {
        let result = execute(PromptsCommands::ShowOrder).await;
        assert!(
            result.is_ok(),
            "prompts show-order should succeed, got: {:?}",
            result.err()
        );
    }

    // ------------------------------------------------------------------
    // ListTemplates
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_list_templates_known_plugin_returns_ok() {
        let result = execute(PromptsCommands::ListTemplates {
            plugin: "security_review".to_string(),
        })
        .await;
        assert!(
            result.is_ok(),
            "list-templates with known plugin should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_list_templates_with_kebab_case_plugin_normalizes_and_returns_ok() {
        let result = execute(PromptsCommands::ListTemplates {
            plugin: "security-review".to_string(),
        })
        .await;
        assert!(
            result.is_ok(),
            "list-templates with kebab-case plugin should succeed after normalization, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_list_templates_unknown_plugin_returns_err() {
        let result = execute(PromptsCommands::ListTemplates {
            plugin: "nonexistent".to_string(),
        })
        .await;
        assert!(
            result.is_err(),
            "list-templates with unknown plugin should return an error"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("unknown plugin"),
            "error message should mention 'unknown plugin', got: {msg}"
        );
    }

    // ------------------------------------------------------------------
    // Render
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_render_with_default_config_returns_ok() {
        let result = execute(PromptsCommands::Render {
            plugin: "security_review".to_string(),
            key: "system".to_string(),
            context: None,
        })
        .await;
        assert!(
            result.is_ok(),
            "render security_review/system should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_execute_render_embedded_output_contains_security_keyword() {
        let output =
            PromptLoader::default().render("security_review", "system", &tera::Context::new());
        assert!(
            output.to_lowercase().contains("security"),
            "security_review template should contain 'security', got: {output}"
        );
    }

    #[tokio::test]
    async fn test_execute_render_technical_review_system_returns_ok() {
        let result = execute(PromptsCommands::Render {
            plugin: "technical_review".to_string(),
            key: "system".to_string(),
            context: None,
        })
        .await;
        assert!(
            result.is_ok(),
            "render technical_review/system should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_render_with_empty_json_context_returns_ok() {
        let result = execute(PromptsCommands::Render {
            plugin: "security_review".to_string(),
            key: "system".to_string(),
            context: Some("{}".to_string()),
        })
        .await;
        assert!(
            result.is_ok(),
            "render with empty JSON context should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_render_with_invalid_json_context_returns_err() {
        let result = execute(PromptsCommands::Render {
            plugin: "security_review".to_string(),
            key: "system".to_string(),
            context: Some("not-json".to_string()),
        })
        .await;
        assert!(
            result.is_err(),
            "render with invalid JSON should return an error"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("invalid JSON"),
            "error message should mention 'invalid JSON', got: {msg}"
        );
    }

    #[tokio::test]
    async fn test_execute_render_unknown_plugin_returns_err() {
        let result = execute(PromptsCommands::Render {
            plugin: "nonexistent".to_string(),
            key: "system".to_string(),
            context: None,
        })
        .await;
        assert!(
            result.is_err(),
            "render with unknown plugin should return an error"
        );
    }

    #[tokio::test]
    async fn test_execute_render_unknown_key_returns_err() {
        let result = execute(PromptsCommands::Render {
            plugin: "security_review".to_string(),
            key: "nonexistent_key".to_string(),
            context: None,
        })
        .await;
        assert!(
            result.is_err(),
            "render with unknown key should return an error"
        );
    }

    #[tokio::test]
    async fn test_execute_render_with_kebab_plugin_normalizes_correctly() {
        let result = execute(PromptsCommands::Render {
            plugin: "security-review".to_string(),
            key: "system".to_string(),
            context: None,
        })
        .await;
        assert!(
            result.is_ok(),
            "render with kebab-case plugin should succeed after normalization, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_execute_render_embedded_output_contains_architect_for_technical_review() {
        let output =
            PromptLoader::default().render("technical_review", "system", &tera::Context::new());
        assert!(
            output.to_lowercase().contains("architect"),
            "technical_review template should contain 'architect', got: {output}"
        );
    }

    #[tokio::test]
    async fn test_execute_export_technical_review_template_is_valid_tera() {
        // The exported technical_review/system.tera must also be parseable by Tera.
        // SAFETY: tempdir() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::tempdir().unwrap();
        execute(PromptsCommands::Export {
            output_dir: Some(tmp.path().to_str().unwrap().to_string()),
        })
        .await
        .unwrap();
        let content =
            std::fs::read_to_string(tmp.path().join("technical_review").join("system.tera"))
                // SAFETY: the export step above writes this file; read failure means export is broken.
                .unwrap();
        assert!(
            !content.is_empty(),
            "exported technical_review/system.tera must not be empty"
        );
        let render_result = tera::Tera::one_off(&content, &tera::Context::new(), false);
        assert!(
            render_result.is_ok(),
            "exported technical_review/system.tera must be valid Tera: {:?}",
            render_result.err()
        );
    }

    #[test]
    fn test_render_with_fixed_context_produces_exact_output() {
        // Asserts rendered output matches an exact expected string for a fixed context.
        // This is the 2.4c requirement: a precise, not just keyword-presence, assertion.
        use crate::config::PromptsConfig;
        use crate::prompts::PromptLoader;
        let mut overrides = std::collections::HashMap::new();
        overrides.insert(
            "security_review/system".to_string(),
            "Plugin: {{ plugin_name }}.".to_string(),
        );
        let loader =
            PromptLoader::new(PromptsConfig::default()).with_in_memory_overrides(overrides);
        let mut ctx = tera::Context::new();
        ctx.insert("plugin_name", &"security-review");
        let result = loader.render("security_review", "system", &ctx);
        assert_eq!(
            result, "Plugin: security-review.",
            "render with a fixed context must produce an exact expected string"
        );
    }

    #[tokio::test]
    async fn test_execute_export_then_edit_then_render_reflects_change() {
        // 2.6b success criterion: exporting, editing, and re-running against the
        // exported system prompt observably changes the output -- proving that the
        // file-based override path is live end-to-end.
        use crate::config::PromptsConfig;
        use crate::prompts::PromptLoader;

        // SAFETY: tempdir() only fails if the OS cannot create a temp dir.
        let tmp = tempfile::tempdir().unwrap();
        let output_dir = tmp.path().to_str().unwrap().to_string();

        // Step 1: Export embedded templates to disk.
        execute(PromptsCommands::Export {
            output_dir: Some(output_dir.clone()),
        })
        .await
        // SAFETY: export to a writable tempdir cannot fail.
        .unwrap();

        // Confirm the file was written before editing.
        let template_path = tmp.path().join("security_review").join("system.tera");
        assert!(
            template_path.exists(),
            "exported file must exist before editing"
        );

        // Step 2: Edit the exported template.
        let edited_content =
            "EDITED SYSTEM PROMPT: this content differs from the embedded default.";
        // SAFETY: template_path was just confirmed to exist.
        std::fs::write(&template_path, edited_content).unwrap();

        // Step 3: Create a PromptLoader pointing at the export directory.
        let config = PromptsConfig {
            directories: vec![output_dir],
            allow_overrides: true,
        };
        let loader = PromptLoader::new(config);

        // Step 4: Render -- must return the EDITED content, not the embedded default.
        let result = loader.render("security_review", "system", &tera::Context::new());
        assert_eq!(
            result, edited_content,
            "render must return the edited on-disk template, not the embedded default"
        );

        // Step 5: Confirm the result differs from the embedded default.
        let embedded = PromptLoader::embedded_raw("security_review", "system")
            // SAFETY: security_review/system is a known registered template.
            .unwrap();
        assert_ne!(
            result.as_str(),
            embedded,
            "edited template must differ from the compiled-in embedded default"
        );
    }
}
