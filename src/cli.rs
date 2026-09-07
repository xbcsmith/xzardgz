//! Command-line interface definitions for the XZardgz workflow harness.
//!
//! This module declares all [`clap`]-derived structs and enums that make up
//! the first-release CLI surface. Every subcommand, flag, and positional
//! argument is documented here. Parse with [`Cli::parse`] in `main` or
//! [`Cli::try_parse_from`] in tests.

use clap::{Args, Parser, Subcommand, ValueEnum};

// ---------------------------------------------------------------------------
// Top-level entry point
// ---------------------------------------------------------------------------

/// Top-level CLI entry point for the XZardgz workflow harness.
///
/// If no subcommand is supplied, clap prints the help text and exits. Use
/// `--verbose` / `-v` (repeatable) to increase log verbosity and `--config`
/// to supply a non-default configuration file path before any subcommand.
#[derive(Debug, Clone, Parser)]
#[command(name = "xzardgz")]
#[command(version)]
#[command(about = "Generic AI workflow harness for repository review automation")]
#[command(arg_required_else_help = true)]
pub struct Cli {
    /// Increase log verbosity. Repeat to raise the level (-v, -vv, -vvv).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Path to the configuration file. Overrides the default search path.
    #[arg(short = 'c', long, global = true)]
    pub config: Option<String>,

    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Commands,
}

// ---------------------------------------------------------------------------
// Top-level command dispatch
// ---------------------------------------------------------------------------

/// Top-level workflow harness subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// Execute a workflow plan or invoke a plugin directly against a repository.
    Run(RunArgs),

    /// Scan a repository and produce a structured scan artifact.
    Scan(ScanArgs),

    /// Manage and execute workflow plugins.
    Plugin {
        /// Plugin subcommand to execute.
        #[command(subcommand)]
        command: PluginCommands,
    },

    /// Start watcher mode for event-driven workflow execution via Kafka.
    Watch(WatchArgs),

    /// Authenticate with AI providers and manage stored credentials.
    Auth {
        /// Authentication subcommand to execute.
        #[command(subcommand)]
        command: AuthCommands,
    },

    /// Manage and inspect workflow prompt templates.
    Prompts {
        /// Prompt management subcommand to execute.
        #[command(subcommand)]
        command: PromptsCommands,
    },

    /// Manage MCP server configuration and test tool discovery.
    Mcp {
        /// MCP management subcommand to execute.
        #[command(subcommand)]
        command: McpCommands,
    },
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

/// Arguments for the `run` subcommand.
///
/// Executes a workflow plan file or performs a direct plugin invocation against
/// a repository. At least one of `--plan` or `--plugin` must be specified;
/// this constraint is validated by the command handler rather than clap.
#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    /// Path to the workflow plan file. Required unless --plugin is specified.
    #[arg(short = 'p', long)]
    pub plan: Option<String>,

    /// Repository path or URL to operate on. Defaults to the current directory.
    #[arg(short = 'r', long)]
    pub repository: Option<String>,

    /// Target branch for the operation.
    #[arg(short = 'b', long)]
    pub branch: Option<String>,

    /// Plugin name for direct invocation (e.g. technical-review, security-review).
    /// Required when --plan is not specified.
    #[arg(long)]
    pub plugin: Option<String>,

    /// Provider override (openai, anthropic, ollama, copilot).
    #[arg(long)]
    pub provider: Option<String>,

    /// Model override.
    #[arg(short = 'm', long)]
    pub model: Option<String>,

    /// Perform a dry run without executing any actions or making API calls.
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Workspace directory override for intermediate files.
    #[arg(short = 'w', long)]
    pub workspace: Option<String>,

    /// Output directory for generated reports.
    #[arg(short = 'o', long)]
    pub output_dir: Option<String>,

    /// OpenAI-compatible API endpoint URL override.
    #[arg(long)]
    pub openai_endpoint: Option<String>,

    /// Ollama host URL override (e.g. http://localhost:11434).
    #[arg(long)]
    pub ollama_host: Option<String>,

    /// Allow insecure HTTP endpoints (disables TLS certificate verification).
    #[arg(long)]
    pub insecure: bool,

    /// Path to an existing scan artifact; skips the repository scan phase.
    #[arg(long)]
    pub scan_artifact: Option<String>,

    /// Enable transcript tracing to a file for debugging AI interactions.
    #[arg(long)]
    pub trace_transcript: bool,

    /// Maximum number of findings to include in the generated report.
    #[arg(long)]
    pub max_findings: Option<u32>,

    /// Report output formats. Accepts comma-separated values (json, markdown, sarif).
    #[arg(short = 'f', long, value_delimiter = ',')]
    pub report_format: Vec<String>,

    /// Resume execution from an existing workspace state rather than starting fresh.
    #[arg(long)]
    pub resume: bool,
}

// ---------------------------------------------------------------------------
// scan
// ---------------------------------------------------------------------------

/// Arguments for the `scan` subcommand.
///
/// Scans a repository and writes a structured scan artifact to disk for later
/// consumption by `run` or `plugin run`.
#[derive(Debug, Clone, Args)]
pub struct ScanArgs {
    /// Repository path or URL to scan.
    #[arg(short = 'r', long, default_value = ".")]
    pub repository: String,

    /// Target branch to scan. Uses the repository default branch if omitted.
    #[arg(short = 'b', long)]
    pub branch: Option<String>,

    /// Workspace directory for intermediate scan files.
    #[arg(short = 'w', long)]
    pub workspace: Option<String>,

    /// Output path for the produced scan artifact.
    #[arg(short = 'o', long)]
    pub output: Option<String>,

    /// Output format for the scan artifact (json, yaml).
    #[arg(long)]
    pub format: Option<String>,

    /// Overwrite an existing scan artifact at the output path.
    #[arg(long)]
    pub overwrite: bool,

    /// Resume execution from an existing workspace state rather than starting fresh.
    #[arg(long)]
    pub resume: bool,
}

// ---------------------------------------------------------------------------
// plugin
// ---------------------------------------------------------------------------

/// Subcommands for the `plugin` command.
///
/// Provides introspection and execution capabilities for workflow plugins.
#[derive(Debug, Clone, Subcommand)]
pub enum PluginCommands {
    /// List all available plugins registered in the harness.
    List,

    /// Display the configuration schema for a plugin.
    Schema {
        /// Name of the plugin whose schema to display.
        plugin: String,
    },

    /// Run a plugin against a workspace directory or existing scan artifact.
    Run(PluginRunArgs),

    /// Validate a plugin configuration file against the plugin schema.
    Validate {
        /// Name of the plugin to validate configuration for.
        plugin: String,

        /// Path to the plugin configuration file to validate.
        #[arg(short = 'c', long)]
        config: Option<String>,
    },

    /// Show the report output formats supported by a plugin.
    Formats {
        /// Name of the plugin whose supported formats to display.
        plugin: String,
    },
}

/// Arguments for the `plugin run` subcommand.
///
/// Runs a specific plugin against an existing workspace or scan artifact,
/// bypassing the full `run` pipeline.
#[derive(Debug, Clone, Args)]
pub struct PluginRunArgs {
    /// Name of the plugin to run (e.g. technical-review, security-review).
    pub plugin: String,

    /// Workspace directory to run the plugin against.
    #[arg(short = 'w', long)]
    pub workspace: Option<String>,

    /// Path to an existing scan artifact to use as plugin input.
    #[arg(short = 's', long)]
    pub scan_artifact: Option<String>,

    /// Path to the plugin configuration file.
    #[arg(short = 'c', long)]
    pub config: Option<String>,

    /// Provider override (openai, anthropic, ollama, copilot).
    #[arg(long)]
    pub provider: Option<String>,

    /// Model override.
    #[arg(short = 'm', long)]
    pub model: Option<String>,

    /// Perform a dry run without executing any actions or making API calls.
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Output directory for generated reports.
    #[arg(short = 'o', long)]
    pub output_dir: Option<String>,

    /// Report output formats. Accepts comma-separated values (json, markdown, sarif).
    #[arg(short = 'f', long, value_delimiter = ',')]
    pub report_format: Vec<String>,
}

// ---------------------------------------------------------------------------
// watch
// ---------------------------------------------------------------------------

/// Arguments for the `watch` subcommand.
///
/// Starts the event-driven watcher loop that consumes workflow tasks from a
/// Kafka topic and publishes results back to an output topic.
#[derive(Debug, Clone, Args)]
pub struct WatchArgs {
    /// Provider override (openai, anthropic, ollama, copilot).
    #[arg(long)]
    pub provider: Option<String>,

    /// Model override.
    #[arg(short = 'm', long)]
    pub model: Option<String>,

    /// Workspace directory for task execution and intermediate files.
    #[arg(short = 'w', long)]
    pub workspace: Option<String>,

    /// Kafka broker list override. Accepts comma-separated host:port pairs.
    #[arg(long)]
    pub brokers: Option<String>,

    /// Kafka input topic override.
    #[arg(long)]
    pub input_topic: Option<String>,

    /// Kafka output topic override.
    #[arg(long)]
    pub output_topic: Option<String>,

    /// Path to the matcher configuration file.
    #[arg(long)]
    pub matcher_config: Option<String>,

    /// Perform a dry run: consume tasks but do not execute or publish results.
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Process exactly one task then exit. Useful for tests and batch jobs.
    #[arg(long)]
    pub once: bool,

    /// Maximum number of tasks to process concurrently.
    #[arg(long)]
    pub max_concurrent: Option<usize>,

    /// Disable publishing results back to the Kafka output topic.
    #[arg(long)]
    pub no_publish: bool,
}

// ---------------------------------------------------------------------------
// auth
// ---------------------------------------------------------------------------

/// Subcommands for the `auth` command.
///
/// Manages authentication credentials for supported AI providers stored in
/// the system credential store.
#[derive(Debug, Clone, Subcommand)]
pub enum AuthCommands {
    /// Authenticate and store credentials for a provider.
    Login {
        /// Provider to authenticate with.
        provider: AuthProvider,
    },

    /// Remove cached credentials for a provider.
    Logout {
        /// Provider to log out from.
        provider: AuthProvider,
    },

    /// Display authentication status for all configured providers.
    Status,

    /// Validate all stored provider credentials without modifying them.
    Validate,

    /// Store an API key for a provider in the system credential store.
    SetKey {
        /// Provider to store the API key for.
        provider: AuthProvider,
    },

    /// Remove the stored API key for a provider from the system credential store.
    RemoveKey {
        /// Provider to remove the API key for.
        provider: AuthProvider,
    },
}

/// Supported AI provider targets for authentication operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AuthProvider {
    /// OpenAI (GPT series models).
    Openai,

    /// Anthropic (Claude series models).
    Anthropic,

    /// GitHub Copilot.
    Copilot,

    /// Ollama locally-hosted models.
    Ollama,
}

// ---------------------------------------------------------------------------
// prompts
// ---------------------------------------------------------------------------

/// Subcommands for the `prompts` command.
///
/// Provides management and introspection of workflow prompt templates used
/// by plugins and the planner.
#[derive(Debug, Clone, Subcommand)]
pub enum PromptsCommands {
    /// Export built-in prompt templates to a directory for local customization.
    Export {
        /// Destination directory to export templates into.
        #[arg(short = 'o', long)]
        output_dir: Option<String>,
    },

    /// Validate all configured prompt template directories for correctness.
    Validate,

    /// Display the prompt template resolution order used at runtime.
    ShowOrder,

    /// List the prompt template names available for a specific plugin.
    ListTemplates {
        /// Name of the plugin whose templates to list.
        plugin: String,
    },

    /// Render a prompt template with a test context for debugging.
    Render {
        /// Plugin identifier, e.g. `security_review` or `security-review`.
        plugin: String,

        /// Template key within the plugin, e.g. `system`.
        key: String,

        /// JSON object string supplying Tera context variables.
        /// Note: the short flag -c is reserved by the global --config option.
        #[arg(long)]
        context: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// mcp
// ---------------------------------------------------------------------------

/// Subcommands for the `mcp` command.
///
/// Provides configuration validation and discovery testing for MCP servers
/// registered with the harness.
#[derive(Debug, Clone, Subcommand)]
pub enum McpCommands {
    /// Validate the MCP server configuration file.
    Validate,

    /// List all MCP servers defined in the configuration.
    ListServers,

    /// List all tools exposed by a specific MCP server.
    ListTools {
        /// Name or identifier of the MCP server to query.
        server: String,
    },

    /// Test tool discovery for a specific MCP server.
    TestDiscovery {
        /// Name or identifier of the MCP server to test.
        server: String,
    },

    /// Test tool invocation on an MCP server using a safe sample input.
    TestInvoke {
        /// Name or identifier of the MCP server.
        server: String,

        /// Name of the tool to invoke on the server.
        tool: String,
    },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse CLI arguments from a string slice, returning a `clap::Error` on failure.
    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(args)
    }

    // -----------------------------------------------------------------------
    // run subcommand
    // -----------------------------------------------------------------------

    #[test]
    fn test_run_with_plan_file_parses_correctly() {
        let cli = parse(&["xzardgz", "run", "--plan", "sample_plan.yaml"]).unwrap();
        match cli.command {
            Commands::Run(args) => {
                assert_eq!(args.plan, Some("sample_plan.yaml".to_string()));
                assert_eq!(args.repository, None);
                assert!(!args.dry_run);
                assert!(args.report_format.is_empty());
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn test_run_with_direct_plugin_invocation_parses_correctly() {
        let cli = parse(&[
            "xzardgz",
            "run",
            "--plugin",
            "security-review",
            "--repository",
            "/tmp/repo",
            "--provider",
            "openai",
            "--model",
            "gpt-4o",
            "--dry-run",
        ])
        .unwrap();
        match cli.command {
            Commands::Run(args) => {
                assert_eq!(args.plugin, Some("security-review".to_string()));
                assert_eq!(args.repository, Some("/tmp/repo".to_string()));
                assert_eq!(args.provider, Some("openai".to_string()));
                assert_eq!(args.model, Some("gpt-4o".to_string()));
                assert!(args.dry_run);
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn test_run_report_format_comma_separated_parses_correctly() {
        let cli = parse(&[
            "xzardgz",
            "run",
            "--plan",
            "plan.yaml",
            "--report-format",
            "json,markdown,sarif",
        ])
        .unwrap();
        match cli.command {
            Commands::Run(args) => {
                assert_eq!(
                    args.report_format,
                    vec![
                        "json".to_string(),
                        "markdown".to_string(),
                        "sarif".to_string()
                    ]
                );
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn test_run_all_flags_parse_correctly() {
        let cli = parse(&[
            "xzardgz",
            "run",
            "--plan",
            "plan.yaml",
            "--repository",
            ".",
            "--branch",
            "main",
            "--provider",
            "anthropic",
            "--model",
            "claude-3-5-sonnet",
            "--dry-run",
            "--workspace",
            "/tmp/ws",
            "--output-dir",
            "/tmp/out",
            "--openai-endpoint",
            "https://api.example.com",
            "--ollama-host",
            "http://localhost:11434",
            "--insecure",
            "--scan-artifact",
            "/tmp/scan.json",
            "--trace-transcript",
            "--max-findings",
            "50",
            "--resume",
        ])
        .unwrap();
        match cli.command {
            Commands::Run(args) => {
                assert_eq!(args.branch, Some("main".to_string()));
                assert_eq!(args.provider, Some("anthropic".to_string()));
                assert_eq!(args.workspace, Some("/tmp/ws".to_string()));
                assert_eq!(args.output_dir, Some("/tmp/out".to_string()));
                assert_eq!(
                    args.openai_endpoint,
                    Some("https://api.example.com".to_string())
                );
                assert_eq!(args.ollama_host, Some("http://localhost:11434".to_string()));
                assert!(args.insecure);
                assert_eq!(args.scan_artifact, Some("/tmp/scan.json".to_string()));
                assert!(args.trace_transcript);
                assert_eq!(args.max_findings, Some(50));
                assert!(args.resume);
            }
            _ => panic!("expected Run command"),
        }
    }

    // -----------------------------------------------------------------------
    // scan subcommand
    // -----------------------------------------------------------------------

    #[test]
    fn test_scan_defaults_parse_correctly() {
        let cli = parse(&["xzardgz", "scan"]).unwrap();
        match cli.command {
            Commands::Scan(args) => {
                assert_eq!(args.repository, ".");
                assert_eq!(args.branch, None);
                assert_eq!(args.workspace, None);
                assert_eq!(args.output, None);
                assert_eq!(args.format, None);
                assert!(!args.overwrite);
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn test_scan_with_all_options_parses_correctly() {
        let cli = parse(&[
            "xzardgz",
            "scan",
            "--repository",
            "https://github.com/example/repo",
            "--branch",
            "develop",
            "--workspace",
            "/tmp/ws",
            "--output",
            "/tmp/scan.json",
            "--format",
            "json",
            "--overwrite",
        ])
        .unwrap();
        match cli.command {
            Commands::Scan(args) => {
                assert_eq!(args.repository, "https://github.com/example/repo");
                assert_eq!(args.branch, Some("develop".to_string()));
                assert_eq!(args.workspace, Some("/tmp/ws".to_string()));
                assert_eq!(args.output, Some("/tmp/scan.json".to_string()));
                assert_eq!(args.format, Some("json".to_string()));
                assert!(args.overwrite);
            }
            _ => panic!("expected Scan command"),
        }
    }

    // -----------------------------------------------------------------------
    // plugin subcommand
    // -----------------------------------------------------------------------

    #[test]
    fn test_plugin_list_parses_correctly() {
        let cli = parse(&["xzardgz", "plugin", "list"]).unwrap();
        match cli.command {
            Commands::Plugin { command } => {
                assert!(matches!(command, PluginCommands::List));
            }
            _ => panic!("expected Plugin command"),
        }
    }

    #[test]
    fn test_plugin_schema_parses_correctly() {
        let cli = parse(&["xzardgz", "plugin", "schema", "technical-review"]).unwrap();
        match cli.command {
            Commands::Plugin { command } => match command {
                PluginCommands::Schema { plugin } => {
                    assert_eq!(plugin, "technical-review");
                }
                _ => panic!("expected Schema subcommand"),
            },
            _ => panic!("expected Plugin command"),
        }
    }

    #[test]
    fn test_plugin_run_parses_correctly() {
        let cli = parse(&[
            "xzardgz",
            "plugin",
            "run",
            "security-review",
            "--workspace",
            "/tmp/ws",
            "--dry-run",
        ])
        .unwrap();
        match cli.command {
            Commands::Plugin { command } => match command {
                PluginCommands::Run(args) => {
                    assert_eq!(args.plugin, "security-review");
                    assert_eq!(args.workspace, Some("/tmp/ws".to_string()));
                    assert!(args.dry_run);
                }
                _ => panic!("expected Run subcommand"),
            },
            _ => panic!("expected Plugin command"),
        }
    }

    #[test]
    fn test_plugin_validate_parses_correctly() {
        let cli = parse(&[
            "xzardgz",
            "plugin",
            "validate",
            "my-plugin",
            "--config",
            "plugin.yaml",
        ])
        .unwrap();
        match cli.command {
            Commands::Plugin { command } => match command {
                PluginCommands::Validate { plugin, config } => {
                    assert_eq!(plugin, "my-plugin");
                    assert_eq!(config, Some("plugin.yaml".to_string()));
                }
                _ => panic!("expected Validate subcommand"),
            },
            _ => panic!("expected Plugin command"),
        }
    }

    #[test]
    fn test_plugin_formats_parses_correctly() {
        let cli = parse(&["xzardgz", "plugin", "formats", "technical-review"]).unwrap();
        match cli.command {
            Commands::Plugin { command } => match command {
                PluginCommands::Formats { plugin } => {
                    assert_eq!(plugin, "technical-review");
                }
                _ => panic!("expected Formats subcommand"),
            },
            _ => panic!("expected Plugin command"),
        }
    }

    // -----------------------------------------------------------------------
    // watch subcommand
    // -----------------------------------------------------------------------

    #[test]
    fn test_watch_once_flag_parses_correctly() {
        let cli = parse(&["xzardgz", "watch", "--once"]).unwrap();
        match cli.command {
            Commands::Watch(args) => {
                assert!(args.once);
                assert!(!args.dry_run);
                assert!(!args.no_publish);
            }
            _ => panic!("expected Watch command"),
        }
    }

    #[test]
    fn test_watch_all_options_parse_correctly() {
        let cli = parse(&[
            "xzardgz",
            "watch",
            "--provider",
            "openai",
            "--model",
            "gpt-4o",
            "--workspace",
            "/tmp/ws",
            "--brokers",
            "broker1:9092,broker2:9092",
            "--input-topic",
            "tasks-in",
            "--output-topic",
            "tasks-out",
            "--matcher-config",
            "matcher.yaml",
            "--dry-run",
            "--once",
            "--max-concurrent",
            "4",
            "--no-publish",
        ])
        .unwrap();
        match cli.command {
            Commands::Watch(args) => {
                assert_eq!(args.provider, Some("openai".to_string()));
                assert_eq!(args.model, Some("gpt-4o".to_string()));
                assert_eq!(args.workspace, Some("/tmp/ws".to_string()));
                assert_eq!(args.brokers, Some("broker1:9092,broker2:9092".to_string()));
                assert_eq!(args.input_topic, Some("tasks-in".to_string()));
                assert_eq!(args.output_topic, Some("tasks-out".to_string()));
                assert_eq!(args.matcher_config, Some("matcher.yaml".to_string()));
                assert!(args.dry_run);
                assert!(args.once);
                assert_eq!(args.max_concurrent, Some(4));
                assert!(args.no_publish);
            }
            _ => panic!("expected Watch command"),
        }
    }

    // -----------------------------------------------------------------------
    // auth subcommand + AuthProvider value enum
    // -----------------------------------------------------------------------

    #[test]
    fn test_auth_login_openai_parses_correctly() {
        let cli = parse(&["xzardgz", "auth", "login", "openai"]).unwrap();
        match cli.command {
            Commands::Auth { command } => match command {
                AuthCommands::Login { provider } => {
                    assert_eq!(provider, AuthProvider::Openai);
                }
                _ => panic!("expected Login subcommand"),
            },
            _ => panic!("expected Auth command"),
        }
    }

    #[test]
    fn test_auth_login_anthropic_parses_correctly() {
        let cli = parse(&["xzardgz", "auth", "login", "anthropic"]).unwrap();
        match cli.command {
            Commands::Auth { command } => match command {
                AuthCommands::Login { provider } => {
                    assert_eq!(provider, AuthProvider::Anthropic);
                }
                _ => panic!("expected Login subcommand"),
            },
            _ => panic!("expected Auth command"),
        }
    }

    #[test]
    fn test_auth_login_copilot_parses_correctly() {
        let cli = parse(&["xzardgz", "auth", "login", "copilot"]).unwrap();
        match cli.command {
            Commands::Auth { command } => match command {
                AuthCommands::Login { provider } => {
                    assert_eq!(provider, AuthProvider::Copilot);
                }
                _ => panic!("expected Login subcommand"),
            },
            _ => panic!("expected Auth command"),
        }
    }

    #[test]
    fn test_auth_login_ollama_parses_correctly() {
        let cli = parse(&["xzardgz", "auth", "login", "ollama"]).unwrap();
        match cli.command {
            Commands::Auth { command } => match command {
                AuthCommands::Login { provider } => {
                    assert_eq!(provider, AuthProvider::Ollama);
                }
                _ => panic!("expected Login subcommand"),
            },
            _ => panic!("expected Auth command"),
        }
    }

    #[test]
    fn test_auth_provider_all_variants_parse_correctly() {
        let cases = [
            ("openai", AuthProvider::Openai),
            ("anthropic", AuthProvider::Anthropic),
            ("copilot", AuthProvider::Copilot),
            ("ollama", AuthProvider::Ollama),
        ];
        for (input, expected) in cases {
            let cli = parse(&["xzardgz", "auth", "login", input]).unwrap();
            match cli.command {
                Commands::Auth {
                    command: AuthCommands::Login { provider },
                } => assert_eq!(provider, expected, "failed for provider '{input}'"),
                _ => panic!("unexpected command for provider '{input}'"),
            }
        }
    }

    #[test]
    fn test_auth_logout_parses_correctly() {
        let cli = parse(&["xzardgz", "auth", "logout", "anthropic"]).unwrap();
        match cli.command {
            Commands::Auth { command } => match command {
                AuthCommands::Logout { provider } => {
                    assert_eq!(provider, AuthProvider::Anthropic);
                }
                _ => panic!("expected Logout subcommand"),
            },
            _ => panic!("expected Auth command"),
        }
    }

    #[test]
    fn test_auth_status_parses_correctly() {
        let cli = parse(&["xzardgz", "auth", "status"]).unwrap();
        match cli.command {
            Commands::Auth { command } => {
                assert!(matches!(command, AuthCommands::Status));
            }
            _ => panic!("expected Auth command"),
        }
    }

    #[test]
    fn test_auth_validate_parses_correctly() {
        let cli = parse(&["xzardgz", "auth", "validate"]).unwrap();
        match cli.command {
            Commands::Auth { command } => {
                assert!(matches!(command, AuthCommands::Validate));
            }
            _ => panic!("expected Auth command"),
        }
    }

    #[test]
    fn test_auth_set_key_parses_correctly() {
        let cli = parse(&["xzardgz", "auth", "set-key", "openai"]).unwrap();
        match cli.command {
            Commands::Auth { command } => match command {
                AuthCommands::SetKey { provider } => {
                    assert_eq!(provider, AuthProvider::Openai);
                }
                _ => panic!("expected SetKey subcommand"),
            },
            _ => panic!("expected Auth command"),
        }
    }

    #[test]
    fn test_auth_remove_key_parses_correctly() {
        let cli = parse(&["xzardgz", "auth", "remove-key", "ollama"]).unwrap();
        match cli.command {
            Commands::Auth { command } => match command {
                AuthCommands::RemoveKey { provider } => {
                    assert_eq!(provider, AuthProvider::Ollama);
                }
                _ => panic!("expected RemoveKey subcommand"),
            },
            _ => panic!("expected Auth command"),
        }
    }

    // -----------------------------------------------------------------------
    // prompts subcommand
    // -----------------------------------------------------------------------

    #[test]
    fn test_prompts_export_parses_correctly() {
        let cli = parse(&[
            "xzardgz",
            "prompts",
            "export",
            "--output-dir",
            "/tmp/prompts",
        ])
        .unwrap();
        match cli.command {
            Commands::Prompts { command } => match command {
                PromptsCommands::Export { output_dir } => {
                    assert_eq!(output_dir, Some("/tmp/prompts".to_string()));
                }
                _ => panic!("expected Export subcommand"),
            },
            _ => panic!("expected Prompts command"),
        }
    }

    #[test]
    fn test_prompts_export_defaults_parse_correctly() {
        let cli = parse(&["xzardgz", "prompts", "export"]).unwrap();
        match cli.command {
            Commands::Prompts { command } => match command {
                PromptsCommands::Export { output_dir } => {
                    assert_eq!(output_dir, None);
                }
                _ => panic!("expected Export subcommand"),
            },
            _ => panic!("expected Prompts command"),
        }
    }

    #[test]
    fn test_prompts_validate_parses_correctly() {
        let cli = parse(&["xzardgz", "prompts", "validate"]).unwrap();
        match cli.command {
            Commands::Prompts { command } => {
                assert!(matches!(command, PromptsCommands::Validate));
            }
            _ => panic!("expected Prompts command"),
        }
    }

    #[test]
    fn test_prompts_show_order_parses_correctly() {
        let cli = parse(&["xzardgz", "prompts", "show-order"]).unwrap();
        match cli.command {
            Commands::Prompts { command } => {
                assert!(matches!(command, PromptsCommands::ShowOrder));
            }
            _ => panic!("expected Prompts command"),
        }
    }

    #[test]
    fn test_prompts_list_templates_parses_correctly() {
        let cli = parse(&["xzardgz", "prompts", "list-templates", "security-review"]).unwrap();
        match cli.command {
            Commands::Prompts { command } => match command {
                PromptsCommands::ListTemplates { plugin } => {
                    assert_eq!(plugin, "security-review");
                }
                _ => panic!("expected ListTemplates subcommand"),
            },
            _ => panic!("expected Prompts command"),
        }
    }

    #[test]
    fn test_prompts_render_parses_correctly() {
        let cli = parse(&[
            "xzardgz",
            "prompts",
            "render",
            "security_review",
            "system",
            "--context",
            r#"{"key":"value"}"#,
        ])
        .unwrap();
        match cli.command {
            Commands::Prompts { command } => match command {
                PromptsCommands::Render {
                    plugin,
                    key,
                    context,
                } => {
                    assert_eq!(plugin, "security_review");
                    assert_eq!(key, "system");
                    assert_eq!(context, Some(r#"{"key":"value"}"#.to_string()));
                }
                _ => panic!("expected Render subcommand"),
            },
            _ => panic!("expected Prompts command"),
        }
    }

    // -----------------------------------------------------------------------
    // mcp subcommand
    // -----------------------------------------------------------------------

    #[test]
    fn test_mcp_validate_parses_correctly() {
        let cli = parse(&["xzardgz", "mcp", "validate"]).unwrap();
        match cli.command {
            Commands::Mcp { command } => {
                assert!(matches!(command, McpCommands::Validate));
            }
            _ => panic!("expected Mcp command"),
        }
    }

    #[test]
    fn test_mcp_list_servers_parses_correctly() {
        let cli = parse(&["xzardgz", "mcp", "list-servers"]).unwrap();
        match cli.command {
            Commands::Mcp { command } => {
                assert!(matches!(command, McpCommands::ListServers));
            }
            _ => panic!("expected Mcp command"),
        }
    }

    #[test]
    fn test_mcp_list_tools_with_server_arg_parses_correctly() {
        let cli = parse(&["xzardgz", "mcp", "list-tools", "my-server"]).unwrap();
        match cli.command {
            Commands::Mcp { command } => match command {
                McpCommands::ListTools { server } => {
                    assert_eq!(server, "my-server");
                }
                _ => panic!("expected ListTools subcommand"),
            },
            _ => panic!("expected Mcp command"),
        }
    }

    #[test]
    fn test_mcp_test_discovery_parses_correctly() {
        let cli = parse(&["xzardgz", "mcp", "test-discovery", "my-server"]).unwrap();
        match cli.command {
            Commands::Mcp { command } => match command {
                McpCommands::TestDiscovery { server } => {
                    assert_eq!(server, "my-server");
                }
                _ => panic!("expected TestDiscovery subcommand"),
            },
            _ => panic!("expected Mcp command"),
        }
    }

    #[test]
    fn test_mcp_test_invoke_parses_correctly() {
        let cli = parse(&["xzardgz", "mcp", "test-invoke", "my-server", "my-tool"]).unwrap();
        match cli.command {
            Commands::Mcp { command } => match command {
                McpCommands::TestInvoke { server, tool } => {
                    assert_eq!(server, "my-server");
                    assert_eq!(tool, "my-tool");
                }
                _ => panic!("expected TestInvoke subcommand"),
            },
            _ => panic!("expected Mcp command"),
        }
    }

    // -----------------------------------------------------------------------
    // global flags
    // -----------------------------------------------------------------------

    #[test]
    fn test_verbose_flag_increments_correctly() {
        let cli = parse(&["xzardgz", "-vvv", "scan"]).unwrap();
        assert_eq!(cli.verbose, 3);
    }

    #[test]
    fn test_config_flag_parses_correctly() {
        let cli = parse(&["xzardgz", "--config", "custom.yaml", "scan"]).unwrap();
        assert_eq!(cli.config, Some("custom.yaml".to_string()));
    }

    #[test]
    fn test_global_verbose_after_subcommand_parses_correctly() {
        let cli = parse(&["xzardgz", "scan", "-vv"]).unwrap();
        assert_eq!(cli.verbose, 2);
    }

    // -----------------------------------------------------------------------
    // legacy and invalid commands are rejected
    // -----------------------------------------------------------------------

    #[test]
    fn test_legacy_chat_command_is_rejected() {
        let result = parse(&["xzardgz", "chat"]);
        assert!(result.is_err(), "legacy 'chat' command should be rejected");
    }

    #[test]
    fn test_legacy_generate_command_is_rejected() {
        let result = parse(&["xzardgz", "generate"]);
        assert!(
            result.is_err(),
            "legacy 'generate' command should be rejected"
        );
    }

    #[test]
    fn test_unknown_provider_is_rejected() {
        let result = parse(&["xzardgz", "auth", "login", "unknown-provider"]);
        assert!(
            result.is_err(),
            "unknown provider value should be rejected by clap"
        );
    }

    // -----------------------------------------------------------------------
    // missing subcommand triggers help error
    // -----------------------------------------------------------------------

    #[test]
    fn test_missing_subcommand_returns_error() {
        let result = parse(&["xzardgz"]);
        assert!(
            result.is_err(),
            "missing subcommand should produce an error"
        );
    }
}
