use clap::Parser;
use xzardgz::cli::{AuthCommands, Cli, Commands};
use xzardgz::commands;
use xzardgz::error::XzardgzError;

#[tokio::main]
async fn main() -> Result<(), XzardgzError> {
    xzardgz::telemetry::init_logging("info")?;
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Run { plan }) => commands::run::execute(plan).await,
        Some(Commands::Scan { repository }) => commands::scan::execute(repository).await,
        Some(Commands::Plugin { name }) => commands::plugin::execute(name).await,
        Some(Commands::Watch) => commands::watch::execute().await,
        Some(Commands::Auth { command }) => match command {
            AuthCommands::Login => commands::auth::login().await,
        },
        Some(Commands::Prompts) => commands::prompts::execute().await,
        Some(Commands::Mcp) => commands::mcp::execute().await,
        None => {
            println!("No command specified. Use --help for usage.");
            Ok(())
        }
    }
}
