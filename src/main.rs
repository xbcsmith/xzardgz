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
        Some(Commands::Chat { message }) => commands::chat::run(message).await,
        Some(Commands::Auth { command }) => match command {
            AuthCommands::Login => commands::auth::login().await,
        },
        Some(Commands::Generate {
            repository,
            category,
            topic,
            output,
            overwrite,
        }) => commands::generate::execute(repository, category, topic, output, overwrite).await,
        None => {
            println!("No command specified. Use --help for usage.");
            Ok(())
        }
    }
}
