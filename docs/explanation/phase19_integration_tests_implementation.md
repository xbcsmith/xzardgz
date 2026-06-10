# Phase 19 Integration Tests Implementation

## Overview

This document explains the design and implementation of the Phase 19 integration
test suite for the XZardgz workflow executor, covering two test files:

- `tests/integration/workflow_tests.rs` -- five executor end-to-end scenarios
- `tests/integration/sarif_tests.rs` -- three SARIF report generation scenarios

Both files are compiled via the `tests/integration.rs` harness using Cargo's
standard `#[path]` module inclusion mechanism.

## Files

| File                                                           | Description                        |
| -------------------------------------------------------------- | ---------------------------------- |
| `tests/integration.rs`                                         | Cargo integration-test harness     |
| `tests/integration/workflow_tests.rs`                          | Workflow executor end-to-end tests |
| `tests/integration/sarif_tests.rs`                             | SARIF report generation tests      |
| `docs/explanation/phase19_integration_tests_implementation.md` | This document                      |

## Architecture

### Test Harness

Cargo discovers integration tests from files at the root of `tests/`. The
`tests/integration.rs` file acts as the entry point for the `integration` test
binary and includes each sub-module using `#[path = "integration/<file>.rs"]`
declarations, following the same pattern used by `tests/unit.rs`.

### Wiremock Setup

Tests that exercise provider-backed plugins (TechnicalReviewPlugin,
SecurityReviewPlugin) use `wiremock` to stub the OpenAI chat-completions
endpoint. Each test:

1. Starts a `MockServer` bound to a random port.
2. Mounts a `Mock` that responds to all `POST /v1/chat/completions` requests.
3. Sets `config.openai.endpoint` to the mock server URI.
4. Sets `config.openai.allow_insecure_endpoint = true` (required for HTTP).
5. Registers a unique `api_key_env` name and sets it via `unsafe { set_var }`.
6. Cleans up the env var after the test via `unsafe { remove_var }`.

### Environment Variable Safety

Each test uses a globally unique environment variable name to avoid races when
tests run in parallel. The unsafe blocks carry SAFETY comments explaining the
invariant (no concurrent readers or writers of the same variable).

### Config Pattern

All tests call a local `make_test_config(workspace_root)` helper that:

- Sets `config.workspace.root` to the temp workspace directory.
- Sets `config.reports.formats` to `["json"]` (minimal output).
- Disables governance (`enabled = false`, `rules_path = ""`).

This prevents the governance loader from attempting to parse `AGENTS.md` as a
YAML rules file, which would cause a parse error.

### TempDir Usage

Each test creates isolated directories using `tempfile::TempDir`. The `TempDir`
handle is kept alive for the duration of the test to prevent premature cleanup.

## workflow_tests.rs

### Test 1: test_scan_only_workflow_produces_scan_artifact

Exercises `ExecutionInput::ScanOnly`. Creates a temp repo with a single
`main.rs` file, builds a `WorkflowExecutor` with an empty plugin registry, and
asserts that the result is successful and `scan_artifact_path` is `Some`.

### Test 2: test_dry_run_plan_skips_plugin_execution

Registers a `PanicPlugin` whose `run` method panics unconditionally. Sets
`dry_run = true` on the plan. Asserts that `result.is_dry_run == true` and
`result.success == true`, proving the executor short-circuits before Stage 12.

### Test 3: test_local_technical_review_with_mock_openai

Full end-to-end run of `TechnicalReviewPlugin` against a wiremock server
returning `{"findings":[]}`. Asserts success, no errors, and a scan artifact.
The mock responds to all POST requests to `/v1/chat/completions` with a valid
OpenAI response shape, allowing the plugin to complete without network access.

### Test 4: test_local_plan_with_unknown_plugin_records_error

Creates a plan with `plugin: "nonexistent-plugin"` against an empty registry.
Asserts that the executor returns `Ok(result)` with `success = false` and an
error message referencing the plugin name, rather than panicking or returning
`Err`.

### Test 5: test_direct_success_plugin_execution

Registers a custom `SuccessPlugin` that returns `PluginOutput::success` without
calling `provider.complete()`. Sets a dummy API key env var so provider
construction at Stage 11 succeeds. Asserts `result.success == true`.

## sarif_tests.rs

### SARIF Generation Chain

The SARIF output path involves two layers:

1. The `SecurityReviewPlugin` writes its own internal SARIF file via
   `SecurityReviewSarifReport::write`.
2. The `WorkflowExecutor::write_step_reports` writes a separate SARIF file from
   `output.findings` using `SarifReportWriter`.

The `result.report_paths` map (keyed by step ID) contains only the paths written
by layer 2. The helper `find_sarif_path` scans all values in `report_paths` for
paths ending in `.sarif.json`.

### SARIF Severity Mapping

`SarifReportWriter` maps `FindingSeverity` to SARIF levels as follows:

| Severity | SARIF level |
| -------- | ----------- |
| Critical | `"error"`   |
| High     | `"error"`   |
| Medium   | `"warning"` |
| Low      | `"note"`    |
| Info     | `"note"`    |

### Test 1: test_security_review_generates_sarif_file

Mocks a high-severity finding. Asserts the executor produces a `.sarif.json`
file in `report_paths`, the file is valid JSON, `version` is `"2.1.0"`, and
`runs` is an array.

### Test 2: test_security_review_sarif_severity_mapping

Mocks a critical-severity finding with `severity_threshold = "info"` to prevent
filtering. Asserts that at least one SARIF result has `"level": "error"`.
`fail_on_critical` is disabled so the executor marks the run as successful.

### Test 3: test_security_review_without_sarif_does_not_create_sarif_file

Sets `include_sarif = false` and restricts `report_formats` to markdown and JSON
at both the config and step levels. Asserts that `find_sarif_path` returns
`None`, confirming no `.sarif.json` path was written by the executor.

## Design Decisions

### Unique Env Var Names

Each test uses a distinct env var name (`XZARDGZ_IT_WF_KEY_TECH`,
`XZARDGZ_IT_WF_KEY_SUCCESS`, `XZARDGZ_IT_SARIF_KEY_GEN`, etc.) to avoid data
races when tests run in parallel under `cargo test`.

### fail_on_critical = false

Security review tests with findings always set `fail_on_critical = false` so the
executor reports success. Tests that verify SARIF content do not test exit-code
behavior.

### Step report_formats vs. Config formats

The executor's `get_report_formats_for_step` prefers the step's `report_formats`
field over `config.reports.formats`. Tests that need SARIF output set
`report_formats: Some(vec!["markdown", "json", "sarif"])` at the step level,
making the intent explicit and independent of config defaults.
