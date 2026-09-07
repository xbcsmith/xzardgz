# Phase 1: Client Boundary, Scorecard, and Repo-Metadata Resolution

This document summarises the implementation of Phase 1 from the external data
clients plan. It covers the new `src/clients/` module, the two resolver
functions, the `TechnicalReviewConfig` additions, and the integration into
`technical_review`.

## Motivation

Before this phase, `technical_review` had no path to real OpenSSF Scorecard or
GitHub repository metadata. Supply-chain signal was absent from every report.
Phase 1 adds a dedicated, clearly-bounded client layer and wires it into the
plugin so that reports reflect real external data when it is available, while
degrading gracefully when it is not.

## Module boundary

`src/clients/` is a new top-level module with an explicit component-boundary
contract documented in its module-level doc comment:

| Rule          | Detail                          |
| ------------- | ------------------------------- |
| May depend on | `auth`, `config`                |
| Must NOT call | `scanner`, `providers`, `agent` |
| Must NOT be   | called from `tools/`            |

The boundary keeps the data-fetching layer a pure leaf in the dependency graph
and prevents circular imports.

## New code

### `src/clients/mod.rs`

- `parse_github_slug(repo: &str) -> Option<(String, String)>` parses GitHub
  repository identifiers from five formats: `owner/repo`,
  `github.com/owner/repo`, `https://github.com/owner/repo`,
  `https://github.com/owner/repo.git`, and `git@github.com:owner/repo.git`.
  Non-GitHub hosts return `None`.
- `ExternalSignals { scorecard: Option<ScorecardResult>, repodata: Option<RepoMetadata> }`
  aggregates resolved signals for consumption by reports and prompts.

### `src/clients/scorecard.rs`

Provides OpenSSF Scorecard integration.

- `fetch_scorecard(repo)` - fetches from the public
  `https://api.securityscorecards.dev/projects/github.com/{owner}/{repo}`
  endpoint via `reqwest`. No authentication required.
- `resolve_scorecard(repo, workspace_root)` - two-level fallback chain:
  1. Reads `{workspace_root}/scorecard.json` when the file exists (local
     override, no network call).
  2. Calls `fetch_scorecard` as the remote fallback.
  3. Returns `ScorecardResolveError::AllSourcesExhausted` when both sources
     fail.

The internal `fetch_scorecard_from(repo, base_url)` function accepts a
configurable base URL, enabling wiremock-based tests without network access.

### `src/clients/repodata.rs`

Provides GitHub repository metadata integration.

- `resolve_repodata(repo, workspace_root)` - two-level fallback chain:
  1. Reads `{workspace_root}/repodata.json` when the file exists.
  2. Calls the GitHub REST API `GET /repos/{owner}/{repo}`. The `GITHUB_TOKEN`
     environment variable is resolved via `EnvVarStore::new("GITHUB_")` for
     authenticated requests; unauthenticated requests are used when the variable
     is absent.
  3. Returns `RepoDataResolveError::AllSourcesExhausted` when both sources fail.

### `TechnicalReviewConfig` additions

Two new fields with `default = true`:

- `scorecard_enabled: bool` - enables Scorecard resolution per run.
- `repodata_enabled: bool` - enables repo metadata resolution per run.

Both are `serde(default = "default_true")` so existing configuration files
require no change.

## Integration into `technical_review`

### Signal resolution in the plugin

`TechnicalReviewPlugin::run` resolves external signals immediately after the
enabled-flag check (before file prioritisation). The workspace root for local
override files is `WorkspaceState::local_repository_path` when set, falling back
to the pipeline workspace directory.

Resolution failures become `Diagnostic::warning` entries so the plugin continues
rather than failing the whole run. Both single-session and batched-session
execution paths receive the resolved signals.

### Prompt enrichment

`build_user_prompt` now accepts `signals: Option<&ExternalSignals>`. When
signals are present it appends:

- OpenSSF Scorecard score and a list of checks scoring below 5.
- Repository flags (archived, fork) and topics from GitHub metadata.

This gives the AI model real supply-chain context during analysis.

### Report enrichment

`TechnicalReviewMarkdownReport::render_with_signals` and `write_with_signals`
append a `## Supply Chain Signals` section to the Markdown report when signals
are non-empty. The section contains:

- An `### OpenSSF Scorecard` subsection with the numeric score and a per-check
  table.
- A `### Repository Metadata` subsection with stars, forks, topics, license, and
  archival/fork status.

The existing `render` and `write` methods are unchanged; `write_with_signals` is
used by the plugin.

## Error handling

Every error in the client layer is a typed enum variant via `thiserror`. The
plugin demotes all resolution failures to diagnostics, satisfying the success
criterion: "degrades gracefully (typed resolve error, plugin continues rather
than failing the whole run) when every source fails."

## Testing

All tests are in the same file as the code they cover.

| Test group                     | Coverage                                          |
| ------------------------------ | ------------------------------------------------- |
| `clients::mod` - 16 tests      | All `parse_github_slug` formats and edge cases    |
| `clients::scorecard` - 7 tests | Mock server success/404, local file, invalid repo |
| `clients::repodata` - 6 tests  | Mock server success/404, local file, invalid repo |
| `report::render_with_signals`  | None/empty/scorecard/repodata cases               |
| `report::write_with_signals`   | File creation with and without signals            |
| `config` - 2 tests             | Default values for new fields                     |

Wiremock tests use `fetch_scorecard_from` and `fetch_repodata_from` with the
mock server URI as the base URL, so no live network calls are made during
`cargo test`.

## What is NOT in Phase 1

Phase 2 (local Scorecard generation via direct GitHub API calls) is explicitly
excluded and requires a separate planning pass before implementation. The
`resolve_scorecard` fallback chain has exactly two levels; a third level for
local generation is not wired.
