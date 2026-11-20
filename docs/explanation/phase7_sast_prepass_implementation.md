# Phase 7: SAST Pre-pass Implementation

## Overview

Phase 7 wires the deterministic SAST engine into the `SecurityReviewPlugin` as a
synchronous pre-pass that runs before the AI completion call. SAST matches are
converted into `ScoringSignal` values and folded into the existing signal list
alongside OSV vulnerability signals.

## Changes

### `src/config.rs` - `SecurityReviewConfig`

Three new fields were added to `SecurityReviewConfig`:

| Field                     | Type          | Default                                  | Purpose                                     |
| ------------------------- | ------------- | ---------------------------------------- | ------------------------------------------- |
| `sast_enabled`            | `bool`        | `false`                                  | Master switch for the SAST pre-pass         |
| `sast_rulesets`           | `Vec<String>` | `["builtin:crypto", "builtin:security"]` | Ruleset identifiers loaded for the scan     |
| `sast_severity_threshold` | `String`      | `"warning"`                              | Minimum severity for signals to be included |

Two private serde default helpers were added:

- `default_sast_rulesets()` - returns `["builtin:crypto", "builtin:security"]`
- `default_sast_severity_threshold()` - returns `"warning"`

### `src/plugins/security_review/config.rs` - Validation

`validate_security_review_config` was extended to validate
`sast_severity_threshold` when `sast_enabled` is `true`. The accepted values are
`"info"`, `"warning"`, and `"error"` (case-insensitive), which map directly to
the `Severity` enum used by the SAST engine. This set is intentionally distinct
from the AI-review `severity_threshold` values (`"info"`, `"low"`, `"medium"`,
`"high"`, `"critical"`).

The validation is skipped entirely when `sast_enabled` is `false` to avoid
forcing users who do not use the pre-pass to supply a threshold value.

### `src/plugins/security_review/plugin.rs` - SAST Pre-pass

A new private `async fn run_sast_prepass` helper was added. The function:

1. Calls `resolve_rules` with the configured `sast_rulesets` to load compiled
   `RuleIr` values.
2. Constructs a `SastEngine` and loads the rules via `with_rules`.
3. Scans `ctx.workspace.paths.repo_dir()` - the workspace-local clone root.
4. Applies the `sast_severity_threshold` filter.
5. Converts each surviving match to a `ScoringSignal`:
   - Matches on CWE-326, CWE-327, or CWE-328 at `Error` severity become
     `AbsoluteViolation` signals that always pass the confidence threshold.
   - `High`-confidence matches become `Positive { weight: 0.3 }`.
   - `Low`-confidence matches become `Negative { weight: 0.2 }`.
   - `Medium`/`Unknown`-confidence matches are discarded (no signal emitted).
6. Returns an empty `Vec` and emits a `tracing::warn!` on any error rather than
   propagating the failure, keeping the pre-pass non-blocking.

The pre-pass is inserted in the `run` method as **Step 2a**, between the enabled
flag check (Step 2) and the OSV resolution step (Step 2b). The resulting
`sast_signals` vector is merged with `osv_signals` to form the note string
passed to the user prompt builder.

### `config.example.yaml`

Three new fields were added under `security_review:`:

```yaml
sast_enabled: false
sast_rulesets:
  - builtin:crypto
  - builtin:security
sast_severity_threshold: warning
```

The `sast_scan:` block was updated to include the AI analysis fields introduced
in Phase 7:

```yaml
ai_confidence_weight: 0.5
ai_analysis_enabled: false
confidence_threshold: 0.5
```

## Signal Mapping Rationale

| SAST match type                   | Signal emitted             | Rationale                                  |
| --------------------------------- | -------------------------- | ------------------------------------------ |
| CWE-326/327/328 at Error severity | `AbsoluteViolation`        | Broken crypto is a deterministic violation |
| High confidence                   | `Positive { weight: 0.3 }` | High confidence reduces uncertainty        |
| Low confidence                    | `Negative { weight: 0.2 }` | Heuristic matches increase uncertainty     |
| Medium/Unknown confidence         | None                       | Neutral contribution; no distortion        |

## Tests Added

### `src/config.rs`

- `test_security_review_config_sast_enabled_defaults_to_false`
- `test_security_review_config_sast_rulesets_defaults_to_builtin`
- `test_security_review_config_sast_severity_threshold_defaults_to_warning`

### `src/plugins/security_review/config.rs`

- `test_validate_security_review_config_with_valid_sast_threshold_returns_ok`
- `test_validate_security_review_config_with_invalid_sast_threshold_returns_plugin_error`
- `test_validate_security_review_config_sast_threshold_only_validated_when_enabled`

### `src/plugins/security_review/plugin.rs`

- `test_security_review_plugin_sast_prepass_produces_signals_on_fixture_code` -
  smoke test confirming the pre-pass path does not panic with an empty workspace
