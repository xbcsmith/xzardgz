use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xzardgz")]
#[command(about = "Autonomous AI agent for documentation generation")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a workflow
    Run {
        /// Path to the plan file
        #[arg(required = true)]
        plan: String,
    },
    /// Start an interactive chat session
    Chat {
        /// Optional initial message
        #[arg(short, long)]
        message: Option<String>,
    },
    /// Authenticate with providers
    Auth {
        #[command(subcommand)]
        command: AuthCommands,
    },
    /// Generate documentation
    Generate {
        /// Repository path
        #[arg(short, long, default_value = ".")]
        repository: String,

        /// Documentation category
        #[arg(short, long, value_enum)]
        category: crate::docgen::diataxis::DocCategory,

        /// Topic to generate documentation for
        #[arg(short, long)]
        topic: String,

        /// Output directory
        #[arg(short, long, default_value = ".")]
        output: String,

        /// Overwrite existing files
        #[arg(long)]
        overwrite: bool,
    },
}

#[derive(Subcommand)]
pub enum AuthCommands {
    /// Login to GitHub Copilot
    Login,
}
