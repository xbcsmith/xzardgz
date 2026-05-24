# Phase 4 CLI Surface Implementation

## Overview

Phase 4 rewrites `src/cli.rs` to deliver the complete first-release command-line
surface for the XZardgz workflow harness. The previous file contained a minimal
skeleton with flat, struct-literal variants and only a single `auth login`
subcommand. This rewrite replaces every part of that skeleton with a fully
documented, production-ready API using `clap` 4's derive macros.

`src/main.rs` is updated in lock-step so that the dispatch table compiles and
routes correctly against the new enum shapes.

---

## Design Decisions

### Required subcommand (non-Option)

The top-level `command` field is typed as `Commands` rather than
`Option<Commands>`. Combined with `#[command(arg_required_else_help = true)]` on
`Cli`, clap prints the help text and exits whenever the binary is invoked with
no arguments. This removes the `None` branch from `main.rs` and avoids silent
no-ops.

### Tuple variants for argument-heavy subcommands

`run`, `scan`, and `watch` each carry many flags and options. Using dedicated
`RunArgs`, `ScanArgs`, and `WatchArgs` structs (with
`#[derive(Debug, Clone, Args)]`) rather than inline struct bodies keeps the
`Commands` enum readable and makes it trivial to pass the full arg struct to a
handler function in a later phase.

### Inline struct variants for simple subcommands

`plugin schema`, `plugin validate`, `plugin formats`, `auth login`,
`auth logout`, `auth set-key`, `auth remove-key`, `prompts export`,
`prompts list-templates`, `prompts render`, `mcp list-tools`,
`mcp test-discovery`, and `mcp test-invoke` each carry only one or two fields.
For these, inline struct bodies inside the enum variant are cleaner than
inventing named structs for every case.

### Global flags

`--verbose` (`-v`, count action) and `--config` (`-c`) carry `global = true` so
they are accepted before or after any subcommand. The verbose counter returns a
`u8` for use with `tracing_subscriber`'s level filter.

### Short-flag conflict resolution

The global `--config` flag owns the short letter `-c`. The
`prompts render --context` option would normally also claim `-c`. Since a global
flag's short letter cannot be reused for a different long name in any descendant
subcommand, `--context` is exposed as a long-only option. The doc comment on the
field explains the reservation.

All other uses of `-c` in subcommand arg structs (`plugin run --config`,
`plugin validate --config`) reuse the same long name (`config`) and therefore
shadow the global correctly without triggering a conflict assertion.

### AuthProvider value enum

`AuthProvider` derives `clap::ValueEnum` so clap handles completion, error
messages, and case-normalization automatically. The four variants map to the
string values `openai`, `anthropic`, `copilot`, and `ollama`.

---

## Public Types Added

| Type              | Kind              | Purpose                              |
| ----------------- | ----------------- | ------------------------------------ |
| `Cli`             | `Parser` struct   | Top-level entry point                |
| `Commands`        | `Subcommand` enum | Dispatches the 7 top-level commands  |
| `RunArgs`         | `Args` struct     | Flags and options for `run`          |
| `ScanArgs`        | `Args` struct     | Flags and options for `scan`         |
| `PluginCommands`  | `Subcommand` enum | Subcommands for `plugin`             |
| `PluginRunArgs`   | `Args` struct     | Flags and options for `plugin run`   |
| `WatchArgs`       | `Args` struct     | Flags and options for `watch`        |
| `AuthCommands`    | `Subcommand` enum | Subcommands for `auth`               |
| `AuthProvider`    | `ValueEnum` enum  | Provider targets for auth operations |
| `PromptsCommands` | `Subcommand` enum | Subcommands for `prompts`            |
| `McpCommands`     | `Subcommand` enum | Subcommands for `mcp`                |

---

## main.rs Changes

The dispatch table is updated to match the new enum shapes:

- `Commands::Run(args)` and `Commands::Scan(args)` replace the old inline struct
  variant patterns. The relevant scalar fields are extracted for the Phase 1
  handler signatures that remain in place.
- `Commands::Plugin { command }` introduces a second-level match over
  `PluginCommands` variants, routing each to `commands::plugin::execute`.
- `Commands::Watch(_args)` drops the `None` branch from the old `Option`-based
  match.
- `Commands::Auth { command }` routes `Login` to the existing Copilot handler
  and returns a placeholder message for the remaining subcommands pending Phase
  5 implementation.
- `Commands::Prompts { .. }` and `Commands::Mcp { .. }` continue to delegate to
  their placeholder handlers.

---

## Test Coverage

The `#[cfg(test)]` module in `src/cli.rs` contains 40 tests covering:

- All 7 top-level subcommands
- Every subcommand of `plugin`, `auth`, `prompts`, and `mcp`
- All flags on `RunArgs`, `ScanArgs`, and `WatchArgs`
- Comma-separated `--report-format` parsing
- All four `AuthProvider` variants individually and in a parameterised loop
- Global `--verbose` count and `--config` path, including placement after a
  subcommand
- Rejection of unknown commands (`chat`, `generate`) and unknown provider values
- Missing subcommand returning an error

---

## Quality Gate Results

All four gates passed after the implementation:

```text
cargo fmt --all                                  -- clean
cargo check --all-targets --all-features         -- Finished (0 warnings)
cargo clippy --all-targets --all-features        -- Finished (0 warnings)
cargo test --all-features                        -- 110 passed (unit)
                                                    43 passed (integration)
                                                     9 passed (doc)
```
