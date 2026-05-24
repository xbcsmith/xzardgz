use clap::Parser;
use xzardgz::cli::{Cli, Commands};
use xzardgz::commands;
use xzardgz::error::XzardgzError;

#[tokio::main]
async fn main() -> Result<(), XzardgzError> {
    xzardgz::telemetry::init_logging("info")?;
    let cli = Cli::parse();

    match cli.command {
        Commands::Run(args) => commands::run::execute(args).await,
        Commands::Scan(args) => commands::scan::execute(args).await,
        Commands::Plugin { command } => commands::plugin::execute(command).await,
        Commands::Watch(args) => commands::watch::execute(args).await,
        Commands::Auth { command } => commands::auth::execute(command).await,
        Commands::Prompts { command } => commands::prompts::execute(command).await,
        Commands::Mcp { command } => commands::mcp::execute(command).await,
    }
}
