# Technical Review External Signals Integration

## Overview

This document explains the integration of the `src/clients/` supply-chain signal
module into the `technical_review` plugin. The change wires OpenSSF Scorecard
results and GitHub repository metadata into both the AI analysis prompt and the
Markdown report output.

## Motivation

The `technical_review` plugin previously evaluated a repository using only local
scan data (file structure, language statistics, existing findings). External
signals such as an OpenSSF Scorecard score or GitHub metadata can meaningfully
influence the review - for example, a low Scorecard score for binary artifacts
or a repository that is archived or forked warrants explicit AI attention.
Including this context in the prompt and the written report improves the quality
and traceability of findings.

## Components Changed

### `src/plugins/technical_review/report.rs`

Two new public methods were added to `TechnicalReviewMarkdownReport`:

- `render_with_signals` - Calls the existing `render` method and, when `signals`
  is `Some` and non-empty, appends a "Supply Chain Signals" Markdown section via
  the private `render_signals_section` helper.
- `write_with_signals` - Validates the path, creates parent directories, calls
  `render_with_signals`, and writes the result to disk.

The existing `render` and `write` methods are unchanged; the new methods are
additive and preserve all existing call sites.

A new private helper `render_signals_section` appends two optional subsections:

- `### OpenSSF Scorecard` - Overall score and a Markdown table of individual
  checks when check data is present.
- `### Repository Metadata` - Key metadata fields (stars, forks, topics,
  license, archived/fork flags) sourced from `RepoMetadata`.

### `src/plugins/technical_review/plugin.rs`

The `run` method was updated to resolve external signals immediately after the
enabled check and before file prioritization:

1. The workspace root for local override file resolution is taken from
   `ctx.state.local_repository_path` when present, falling back to the pipeline
   workspace root (`ctx.workspace.paths.root`).
2. The repository slug is resolved from `scan_result.repository_url` with a
   fallback to `scan_result.repository_name`. Both values are cloned into an
   owned `Option<String>` to avoid holding a borrow on `ctx` while diagnostic
   mutations are applied.
3. `resolve_scorecard` and `resolve_repodata` are called when the corresponding
   `scorecard_enabled` / `repodata_enabled` config flags are true and a slug is
   available. Failures are recorded as `Diagnostic::warning` entries and the
   field is set to `None`; the plugin continues normally.
4. The resolved `ExternalSignals` value is threaded through to:
   - `build_user_prompt` (new `signals` parameter) - appends Scorecard score,
     weak checks, and relevant repo metadata to the AI context before the final
     "Provide your technical review findings." line.
   - `TechnicalReviewMarkdownReport::write_with_signals` - replaces the previous
     `write` call so the report includes the supply-chain section.
   - `TechnicalReviewBatchSession` - carries a cloned `ExternalSignals` so each
     batch's user prompt receives the same context.

## Design Decisions

### Owned slug string

The `repo_slug` variable is an `Option<String>` rather than an `Option<&str>`
referencing `ctx.scan_result`. This avoids a split-borrow error: holding an
immutable borrow of `ctx.scan_result` across the `ctx.add_diagnostic(...)` calls
(which require a mutable borrow of `ctx`) is rejected by the borrow checker.

### Additive methods only

`render` and `write` retain their original signatures. The `_with_signals`
variants are purely additive; this ensures all existing tests and call sites
continue to compile without modification (they are tested separately and the
existing tests were explicitly preserved).

### Clone for batch session

`ExternalSignals` does not implement `Copy`. The batch session receives a struct
literal with each field cloned individually, which is equivalent to calling
`.clone()` on the whole struct and makes the field origin explicit.

### Non-fatal resolution failures

Both `resolve_scorecard` and `resolve_repodata` failures are demoted to
`Diagnostic::warning`. This keeps the plugin resilient in air-gapped or
network-restricted environments where remote scorecard fetches would always
fail.

## Tests Added

### `report.rs` (6 new tests)

| Test                                                             | What it verifies                                 |
| ---------------------------------------------------------------- | ------------------------------------------------ |
| `test_render_with_signals_none_produces_same_as_render`          | `None` signals leaves output unchanged           |
| `test_render_with_signals_empty_signals_produces_same_as_render` | Empty `ExternalSignals` leaves output unchanged  |
| `test_render_with_signals_scorecard_appends_section`             | Scorecard data produces correct Markdown section |
| `test_render_with_signals_repodata_appends_metadata`             | Repo metadata produces correct Markdown section  |
| `test_write_with_signals_none_creates_file`                      | File is created when signals is `None`           |
| `test_write_with_signals_with_scorecard_includes_section`        | Written file contains scorecard section          |

### `plugin.rs` (existing tests updated)

The five existing `test_build_user_prompt_*` tests received a `None` fourth
argument to match the updated `build_user_prompt` signature. No test logic was
changed.
