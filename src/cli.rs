use clap::{Parser, Subcommand};

/// Command-line interface for the XZardgz workflow harness.
#[derive(Parser)]
#[command(name = "xzardgz")]
#[command(about = "Generic AI workflow harness for repository review automation")]
pub struct Cli {
    /// Command to execute.
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Top-level workflow harness commands.
#[derive(Subcommand)]
pub enum Commands {
    /// Run a workflow plan.
    Run {
        /// Path to the plan file.
        #[arg(required = true)]
        plan: String,
    },
    /// Scan a repository and emit a scan artifact.
    Scan {
        /// Repository path or URL to scan.
        #[arg(short, long, default_value = ".")]
        repository: String,
    },
    /// Inspect or execute workflow plugins.
    Plugin {
        /// Optional plugin name.
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Start watcher mode for event-driven workflow execution.
    Watch,
    /// Authenticate with providers.
    Auth {
        /// Authentication action to perform.
        #[command(subcommand)]
        command: AuthCommands,
    },
    /// Manage prompt templates.
    Prompts,
    /// Manage MCP server and tool configuration.
    Mcp,
}

/// Provider authentication commands.
#[derive(Subcommand)]
pub enum AuthCommands {
    /// Login to the default provider.
    Login,
}
