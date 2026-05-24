//! Repository scan command handler.
//!
//! This module implements the `scan` subcommand, which triggers a structured
//! scan of a target repository and writes the resulting artifact to disk for
//! later consumption by the `run` or `plugin run` commands.

use crate::cli::ScanArgs;
use crate::config::Config;
use crate::error::Result;

/// Executes the repository scan command.
///
/// Loads global configuration, prints information about what would be scanned,
/// and returns `Ok(())`. Full scan execution is implemented in a later phase.
///
/// # Arguments
///
/// * `args` - Parsed CLI arguments from the `scan` subcommand.
///
/// # Errors
///
/// Returns [`crate::error::PipelineError::Config`] if configuration loading
/// fails.
pub async fn execute(args: ScanArgs) -> Result<()> {
    let _config = Config::load()?;

    println!("Scanning repository: {}", args.repository);

    if let Some(ref branch) = args.branch {
        println!("Branch: {}", branch);
    }

    let format = args.format.as_deref().unwrap_or("json");
    println!("Output format: {}", format);

    if let Some(ref workspace) = args.workspace {
        println!("Workspace: {}", workspace);
    }

    if let Some(ref output) = args.output {
        println!("Output path: {}", output);
    }

    if args.overwrite {
        println!("Overwrite: enabled");
    }

    println!("Scan execution is implemented in a later phase.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ScanArgs;

    #[tokio::test]
    async fn test_execute_prints_repository_info_without_error() {
        let args = ScanArgs {
            repository: ".".to_string(),
            branch: None,
            workspace: None,
            output: None,
            format: None,
            overwrite: false,
        };
        let result = execute(args).await;
        assert!(
            result.is_ok(),
            "scan should succeed for a valid repository, got: {:?}",
            result.err()
        );
    }
}
