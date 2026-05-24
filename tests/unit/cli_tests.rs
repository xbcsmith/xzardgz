//! CLI argument parsing integration tests.
//!
//! These tests verify that the Phase 1 command surface is correctly accepted
//! or rejected by the clap parser, and that all new subcommand forms parse
//! into the expected argument types.

use clap::{CommandFactory, Parser, error::ErrorKind};
use xzardgz::cli::{AuthProvider, Cli, Commands};

const PHASE_1_COMMANDS: [&str; 7] = ["run", "scan", "plugin", "watch", "auth", "prompts", "mcp"];

/// Verifies that all Phase 1 product commands are accepted by clap's help
/// system. Passing `--help` after any recognised subcommand must yield a
/// `DisplayHelp` error kind, confirming the command is registered.
#[test]
fn test_cli_accepts_phase_1_product_commands() {
    for command_name in PHASE_1_COMMANDS {
        let result = Cli::command().try_get_matches_from(["xzardgz", command_name, "--help"]);

        match result {
            Err(error) if error.kind() == ErrorKind::DisplayHelp => {}
            Err(error) => panic!(
                "expected `{}` to be accepted by clap help parsing, got {:?}: {}",
                command_name,
                error.kind(),
                error
            ),
            Ok(_) => panic!(
                "expected `{}` help parsing to return DisplayHelp, but parsing succeeded",
                command_name
            ),
        }
    }
}

/// Verifies that the legacy `chat` command is not present in the Phase 1
/// command surface.
#[test]
fn test_cli_rejects_legacy_chat_command() {
    let result = Cli::try_parse_from(["xzardgz", "chat", "--message", "hello"]);

    assert!(
        result.is_err(),
        "legacy `chat` command should be rejected by the Phase 1 CLI surface"
    );
}

/// Verifies that the legacy `generate` command is not present in the Phase 1
/// command surface.
#[test]
fn test_cli_rejects_legacy_generate_command() {
    let result = Cli::try_parse_from([
        "xzardgz",
        "generate",
        "--repository",
        ".",
        "--category",
        "tutorial",
        "--topic",
        "getting-started",
        "--output",
        "docs",
    ]);

    assert!(
        result.is_err(),
        "legacy `generate` command should be rejected by the Phase 1 CLI surface"
    );
}

/// Verifies that `run` with no arguments parses successfully at the clap
/// level. The validation error (neither plan nor plugin supplied) is raised
/// at runtime inside `execute`, not during argument parsing.
#[test]
fn test_run_command_requires_plan_or_plugin() {
    let result = Cli::try_parse_from(["xzardgz", "run"]);
    assert!(
        result.is_ok(),
        "xzardgz run with no args should parse at the clap level, got: {:?}",
        result.err()
    );
}

/// Verifies that `scan` with only the default repository argument parses
/// successfully.
#[test]
fn test_scan_command_with_defaults_parses_correctly() {
    let result = Cli::try_parse_from(["xzardgz", "scan"]);
    assert!(
        result.is_ok(),
        "xzardgz scan with defaults should parse, got: {:?}",
        result.err()
    );
    if let Ok(cli) = result {
        if let Commands::Scan(args) = cli.command {
            assert_eq!(args.repository, ".", "default repository should be '.'");
        } else {
            panic!("expected Scan command");
        }
    }
}

/// Verifies that `watch` with no arguments (all optional) parses correctly.
#[test]
fn test_watch_command_with_defaults_parses_correctly() {
    let result = Cli::try_parse_from(["xzardgz", "watch"]);
    assert!(
        result.is_ok(),
        "xzardgz watch with no args should parse, got: {:?}",
        result.err()
    );
}

/// Verifies that `auth login openai` produces the expected argument structure.
#[test]
fn test_auth_login_openai_parses_correctly() {
    let result = Cli::try_parse_from(["xzardgz", "auth", "login", "openai"]);
    assert!(
        result.is_ok(),
        "xzardgz auth login openai should parse, got: {:?}",
        result.err()
    );
    if let Ok(cli) = result {
        if let Commands::Auth { command } = cli.command {
            use xzardgz::cli::AuthCommands;
            if let AuthCommands::Login { provider } = command {
                assert_eq!(provider, AuthProvider::Openai);
            } else {
                panic!("expected Login subcommand");
            }
        } else {
            panic!("expected Auth command");
        }
    }
}

/// Verifies that `plugin list` produces a `PluginCommands::List` variant.
#[test]
fn test_plugin_list_parses_correctly() {
    let result = Cli::try_parse_from(["xzardgz", "plugin", "list"]);
    assert!(
        result.is_ok(),
        "xzardgz plugin list should parse, got: {:?}",
        result.err()
    );
    if let Ok(cli) = result {
        if let Commands::Plugin { command } = cli.command {
            use xzardgz::cli::PluginCommands;
            assert!(
                matches!(command, PluginCommands::List),
                "expected PluginCommands::List"
            );
        } else {
            panic!("expected Plugin command");
        }
    }
}

/// Verifies that `mcp validate` produces a `McpCommands::Validate` variant.
#[test]
fn test_mcp_validate_parses_correctly() {
    let result = Cli::try_parse_from(["xzardgz", "mcp", "validate"]);
    assert!(
        result.is_ok(),
        "xzardgz mcp validate should parse, got: {:?}",
        result.err()
    );
    if let Ok(cli) = result {
        if let Commands::Mcp { command } = cli.command {
            use xzardgz::cli::McpCommands;
            assert!(
                matches!(command, McpCommands::Validate),
                "expected McpCommands::Validate"
            );
        } else {
            panic!("expected Mcp command");
        }
    }
}

/// Verifies that `prompts show-order` produces a `PromptsCommands::ShowOrder`
/// variant.
#[test]
fn test_prompts_show_order_parses_correctly() {
    let result = Cli::try_parse_from(["xzardgz", "prompts", "show-order"]);
    assert!(
        result.is_ok(),
        "xzardgz prompts show-order should parse, got: {:?}",
        result.err()
    );
    if let Ok(cli) = result {
        if let Commands::Prompts { command } = cli.command {
            use xzardgz::cli::PromptsCommands;
            assert!(
                matches!(command, PromptsCommands::ShowOrder),
                "expected PromptsCommands::ShowOrder"
            );
        } else {
            panic!("expected Prompts command");
        }
    }
}
