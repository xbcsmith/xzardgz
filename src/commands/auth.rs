//! Provider authentication command handler.
//!
//! This module implements the `auth` family of subcommands for managing
//! provider credentials (login, logout, status, validation, key management).

use crate::cli::AuthCommands;
use crate::error::Result;

/// Executes an authentication subcommand.
///
/// Dispatches to the appropriate print stub based on the variant of `command`.
/// Full provider authentication (OAuth flows, keyring integration) is
/// implemented in a later phase.
///
/// # Arguments
///
/// * `command` - The auth subcommand to execute.
///
/// # Errors
///
/// This implementation does not currently return errors.
pub async fn execute(command: AuthCommands) -> Result<()> {
    match command {
        AuthCommands::Login { provider } => {
            println!(
                "Logging in to provider: {:?}. Provider authentication is implemented in a later phase.",
                provider
            );
        }
        AuthCommands::Logout { provider } => {
            println!("Logging out from provider: {:?}.", provider);
        }
        AuthCommands::Status => {
            println!("Authentication status: implemented in a later phase.");
        }
        AuthCommands::Validate => {
            println!("Validating credentials: implemented in a later phase.");
        }
        AuthCommands::SetKey { provider } => {
            println!("Setting API key for provider: {:?}.", provider);
        }
        AuthCommands::RemoveKey { provider } => {
            println!("Removing API key for provider: {:?}.", provider);
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
    use crate::cli::{AuthCommands, AuthProvider};

    #[tokio::test]
    async fn test_execute_login_openai_returns_ok() {
        let result = execute(AuthCommands::Login {
            provider: AuthProvider::Openai,
        })
        .await;
        assert!(
            result.is_ok(),
            "auth login openai should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_logout_returns_ok() {
        let result = execute(AuthCommands::Logout {
            provider: AuthProvider::Anthropic,
        })
        .await;
        assert!(
            result.is_ok(),
            "auth logout should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_status_returns_ok() {
        let result = execute(AuthCommands::Status).await;
        assert!(
            result.is_ok(),
            "auth status should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_validate_returns_ok() {
        let result = execute(AuthCommands::Validate).await;
        assert!(
            result.is_ok(),
            "auth validate should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_set_key_returns_ok() {
        let result = execute(AuthCommands::SetKey {
            provider: AuthProvider::Openai,
        })
        .await;
        assert!(
            result.is_ok(),
            "auth set-key should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_remove_key_returns_ok() {
        let result = execute(AuthCommands::RemoveKey {
            provider: AuthProvider::Ollama,
        })
        .await;
        assert!(
            result.is_ok(),
            "auth remove-key should succeed, got: {:?}",
            result.err()
        );
    }
}
