use clap::{CommandFactory, Parser, error::ErrorKind};
use xzardgz::cli::Cli;

const PHASE_1_COMMANDS: [&str; 7] = ["run", "scan", "plugin", "watch", "auth", "prompts", "mcp"];

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

#[test]
fn test_cli_rejects_legacy_chat_command() {
    let result = Cli::try_parse_from(["xzardgz", "chat", "--message", "hello"]);

    assert!(
        result.is_err(),
        "legacy `chat` command should be rejected by the Phase 1 CLI surface"
    );
}

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
