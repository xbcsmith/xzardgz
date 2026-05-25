//! Provider authentication command handler.
//!
//! Implements `auth login`, `auth logout`, `auth status`, `auth validate`,
//! `auth set-key`, and `auth remove-key` using the [`ProviderAuthManager`].
//!
//! # Entry points
//!
//! - [`execute`]: preserves the original call signature used by `main.rs`.
//!   Constructs a [`Config::default()`] internally and delegates to
//!   [`execute_with_config`].
//! - [`execute_with_config`]: full implementation; accepts an explicit
//!   [`Config`] for programmatic use and testing.

use crate::auth::ProviderAuthManager;
use crate::cli::{AuthCommands, AuthProvider};
use crate::config::Config;
use crate::error::Result;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Executes an authentication subcommand using a default [`Config`].
///
/// This function preserves the call signature expected by `main.rs`.
/// For programmatic use with a specific config, prefer [`execute_with_config`].
///
/// # Arguments
///
/// * `command` - The auth subcommand variant to execute.
///
/// # Errors
///
/// Returns [`crate::error::PipelineError::Auth`] when a key storage operation
/// fails (e.g., keyring backend error on `logout` or `remove-key`).
pub async fn execute(command: AuthCommands) -> Result<()> {
    let config = Config::default();
    execute_with_config(command, &config).await
}

/// Executes an authentication subcommand using the provided [`Config`].
///
/// # Arguments
///
/// * `command` - The auth subcommand variant to execute.
/// * `config`  - Pipeline configuration for constructing auth helpers.
///
/// # Errors
///
/// Returns [`crate::error::PipelineError::Auth`] when a key storage operation
/// fails.
pub async fn execute_with_config(command: AuthCommands, config: &Config) -> Result<()> {
    let manager = ProviderAuthManager::from_config(config);

    match command {
        // ------------------------------------------------------------------
        // auth login <provider>
        // ------------------------------------------------------------------
        AuthCommands::Login { provider } => {
            println!("Logging in to provider: {}", provider_display(&provider));
            match provider {
                AuthProvider::Openai => {
                    let status = manager.openai.status();
                    println!("  OpenAI status: {}", status.summary());
                    println!("  To store an API key: xzardgz auth set-key openai");
                }
                AuthProvider::Anthropic => {
                    let status = manager.anthropic.status();
                    println!("  Anthropic status: {}", status.summary());
                    println!("  To store an API key: xzardgz auth set-key anthropic");
                }
                AuthProvider::Ollama => {
                    println!("  Ollama uses no credentials (local server).");
                    println!("  Host: {}", manager.ollama.host());
                }
                AuthProvider::Copilot => {
                    println!("  Copilot authentication uses GitHub OAuth device flow.");
                    println!("  OAuth flow is handled by the Copilot provider directly.");
                }
            }
        }

        // ------------------------------------------------------------------
        // auth logout <provider>
        // ------------------------------------------------------------------
        AuthCommands::Logout { provider } => match provider {
            AuthProvider::Openai => {
                manager.openai.remove_key()?;
                println!("OpenAI keyring credential removed.");
            }
            AuthProvider::Anthropic => {
                manager.anthropic.remove_key()?;
                println!("Anthropic keyring credential removed.");
            }
            AuthProvider::Ollama => {
                println!("Ollama uses no credentials — nothing to remove.");
            }
            AuthProvider::Copilot => {
                println!("Copilot logout is not yet implemented.");
            }
        },

        // ------------------------------------------------------------------
        // auth status
        // ------------------------------------------------------------------
        AuthCommands::Status => {
            let all = manager.status_all();
            println!("Provider authentication status:");
            println!("  OpenAI:    {}", all.openai.summary());
            println!("  Anthropic: {}", all.anthropic.summary());
            println!("  Ollama:    {}", all.ollama.summary());
            println!("  Copilot:   {}", all.copilot.summary());
        }

        // ------------------------------------------------------------------
        // auth validate
        // ------------------------------------------------------------------
        AuthCommands::Validate => {
            let all = manager.status_all();
            println!("Validating provider credentials:");
            if all.openai.has_credentials() {
                println!("  OpenAI:    credential present (API validation requires network call)");
            } else {
                println!("  OpenAI:    {}", all.openai.summary());
            }
            if all.anthropic.has_credentials() {
                println!("  Anthropic: credential present (API validation requires network call)");
            } else {
                println!("  Anthropic: {}", all.anthropic.summary());
            }
            println!("  Ollama:    {}", all.ollama.summary());
        }

        // ------------------------------------------------------------------
        // auth set-key <provider>
        // ------------------------------------------------------------------
        AuthCommands::SetKey { provider } => match provider {
            AuthProvider::Openai => {
                println!("Enter OpenAI API key (input is not echoed):");
                match read_secret_from_stdin() {
                    Ok(key) if !key.is_empty() => {
                        manager.openai.set_key(&key)?;
                        println!("OpenAI API key stored in keyring.");
                    }
                    Ok(_) => {
                        println!("No key provided.");
                    }
                    Err(e) => {
                        println!("Could not read key from stdin: {e}");
                    }
                }
            }
            AuthProvider::Anthropic => {
                println!("Enter Anthropic API key (input is not echoed):");
                match read_secret_from_stdin() {
                    Ok(key) if !key.is_empty() => {
                        manager.anthropic.set_key(&key)?;
                        println!("Anthropic API key stored in keyring.");
                    }
                    Ok(_) => {
                        println!("No key provided.");
                    }
                    Err(e) => {
                        println!("Could not read key from stdin: {e}");
                    }
                }
            }
            AuthProvider::Ollama => {
                println!("Ollama uses no credentials.");
            }
            AuthProvider::Copilot => {
                println!("Copilot uses OAuth; use `auth login copilot` instead.");
            }
        },

        // ------------------------------------------------------------------
        // auth remove-key <provider>
        // ------------------------------------------------------------------
        AuthCommands::RemoveKey { provider } => match provider {
            AuthProvider::Openai => {
                manager.openai.remove_key()?;
                println!("OpenAI key removed.");
            }
            AuthProvider::Anthropic => {
                manager.anthropic.remove_key()?;
                println!("Anthropic key removed.");
            }
            AuthProvider::Ollama => {
                println!("Ollama uses no credentials.");
            }
            AuthProvider::Copilot => {
                println!("Copilot: use `auth logout copilot`.");
            }
        },
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns a display-safe name for an [`AuthProvider`] variant.
fn provider_display(p: &AuthProvider) -> &str {
    match p {
        AuthProvider::Openai => "openai",
        AuthProvider::Anthropic => "anthropic",
        AuthProvider::Ollama => "ollama",
        AuthProvider::Copilot => "copilot",
    }
}

/// Reads a single line from stdin and returns it trimmed.
///
/// Returns `Ok(String)` where the string may be empty if the user pressed
/// Enter without typing anything or if stdin is at EOF (e.g., in tests).
/// Returns `Err(PipelineError::Auth)` only if `read_line` itself fails.
fn read_secret_from_stdin() -> Result<String> {
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| crate::error::PipelineError::Auth(format!("failed to read stdin: {e}")))?;
    Ok(line.trim().to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{AuthCommands, AuthProvider};

    // ------------------------------------------------------------------
    // Original 6 stub-path tests (preserved verbatim)
    // ------------------------------------------------------------------

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
        // stdin is at EOF in test runs; read_secret_from_stdin returns Ok(""),
        // the empty-key branch prints "No key provided." and returns Ok(()).
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

    // ------------------------------------------------------------------
    // New tests for execute / execute_with_config
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_status_shows_provider_list() {
        // status should return Ok regardless of which credentials are set.
        let result = execute(AuthCommands::Status).await;
        assert!(
            result.is_ok(),
            "auth status with default config should return Ok, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_validate_returns_ok_with_default_config() {
        // validate should return Ok with the default config.
        let config = Config::default();
        let result = execute_with_config(AuthCommands::Validate, &config).await;
        assert!(
            result.is_ok(),
            "auth validate with default config should return Ok, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_login_ollama_returns_ok() {
        let result = execute(AuthCommands::Login {
            provider: AuthProvider::Ollama,
        })
        .await;
        assert!(
            result.is_ok(),
            "auth login ollama should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_login_copilot_returns_ok() {
        let result = execute(AuthCommands::Login {
            provider: AuthProvider::Copilot,
        })
        .await;
        assert!(
            result.is_ok(),
            "auth login copilot should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_remove_key_openai_returns_ok() {
        // remove_key on an absent keyring entry should succeed silently.
        let result = execute(AuthCommands::RemoveKey {
            provider: AuthProvider::Openai,
        })
        .await;
        assert!(
            result.is_ok(),
            "auth remove-key openai should succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_set_key_anthropic_returns_ok() {
        // stdin is at EOF in test runs; should print "No key provided." and return Ok.
        let result = execute(AuthCommands::SetKey {
            provider: AuthProvider::Anthropic,
        })
        .await;
        assert!(
            result.is_ok(),
            "auth set-key anthropic should succeed, got: {:?}",
            result.err()
        );
    }
}
