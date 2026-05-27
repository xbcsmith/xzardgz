# Phase 16: Security Review Plugin with SARIF

This document describes the implementation of the built-in `security-review`
plugin for the XZardgz workflow harness. The plugin performs AI-assisted
security analysis of a repository and produces Markdown, JSON, and SARIF 2.1.0
reports.

---

## Architecture

The `SecurityReviewPlugin` is a concrete implementation of the `WorkflowPlugin`
trait defined in `src/plugins/trait_def.rs`. It is registered in
`PluginRegistry` under the name `"security-review"` and can be invoked from two
entry points:

- **CLI**: `xzardgz run --plugin security-review`
- **Watcher**: a `WatcherTaskMessage` with `plugin` set to `"security-review"`
  routes through `WatcherExecutor` to the registry and then to the plugin.

The plugin is entirely self-contained within `src/plugins/security_review/`. All
output is produced through the standard `PluginOutput` and `PluginContext`
interfaces established in Phase 13. No shared infrastructure module is modified.

Sub-modules are arranged in a strict layered dependency order to prevent
circular imports:

```text
config.rs  -> (standalone: serde, config crate types)
finding.rs -> config.rs
scope.rs   -> finding.rs
report.rs  -> finding.rs, scope.rs
plugin.rs  -> config.rs, finding.rs, scope.rs, report.rs,
              plugins::PluginContext, plugins::PluginOutput,
              reports::ReportEnvelope, providers::Provider
```

No sub-module imports from a module above it in this dependency order.

### Module Layout

The plugin is implemented across six focused files:

| File                                     | Purpose                                         |
| ---------------------------------------- | ----------------------------------------------- |
| `src/plugins/security_review/mod.rs`     | Module root and public API re-exports           |
| `src/plugins/security_review/config.rs`  | `SecurityReviewConfig` and validation logic     |
| `src/plugins/security_review/finding.rs` | `SecurityReviewFinding` data type and severity  |
| `src/plugins/security_review/scope.rs`   | `SecurityCategory` enum and file prioritization |
| `src/plugins/security_review/report.rs`  | Markdown, JSON, and SARIF 2.1.0 report writers  |
| `src/plugins/security_review/plugin.rs`  | `SecurityReviewPlugin` `WorkflowPlugin` impl    |

`src/plugins/mod.rs` is updated to declare `pub mod security_review;` and
re-export `SecurityReviewPlugin` so callers can register it without importing
sub-paths directly.

---

## Configuration

`SecurityReviewConfig` controls all aspects of the security review pass. It
derives `serde::Deserialize`, `serde::Serialize`, `Clone`, and `Debug`.

| Field                     | Type             | Default                         | Purpose                                         |
| ------------------------- | ---------------- | ------------------------------- | ----------------------------------------------- |
| `enabled`                 | `bool`           | `true`                          | Master gate; disables the plugin when false     |
| `prompt_dir`              | `String`         | `""`                            | Override directory for prompt template files    |
| `max_findings`            | `u32`            | `50`                            | Cap on total findings retained in the report    |
| `severity_threshold`      | `String`         | `"medium"`                      | Minimum severity level to include in output     |
| `include_sarif`           | `bool`           | `true`                          | Whether to emit the SARIF 2.1.0 report file     |
| `report_formats`          | `Vec<String>`    | `["markdown", "json", "sarif"]` | Output formats to generate                      |
| `secret_scanning`         | `bool`           | `true`                          | Activates secret exposure category checks       |
| `dependency_scanning`     | `bool`           | `true`                          | Activates insecure dependency category checks   |
| `check_unsafe_code`       | `bool`           | `true`                          | Activates unsafe code usage category checks     |
| `check_auth`              | `bool`           | `true`                          | Activates broken authentication category checks |
| `check_endpoints`         | `bool`           | `true`                          | Activates insecure endpoint category checks     |
| `check_command_execution` | `bool`           | `true`                          | Activates command execution category checks     |
| `check_deserialization`   | `bool`           | `true`                          | Activates insecure deserialization checks       |
| `check_cryptography`      | `bool`           | `true`                          | Activates cryptography weakness checks          |
| `fail_on_critical`        | `bool`           | `false`                         | Exit non-zero when critical findings are found  |
| `batch_size`              | `u32`            | `10`                            | Files per AI request batch                      |
| `model_override`          | `Option<String>` | `None`                          | Override the provider default model             |
| `verification_turns`      | `u32`            | `1`                             | AI verification passes after initial analysis   |
| `confidence_threshold`    | `f64`            | `0.5`                           | Minimum AI confidence to retain a finding       |

All boolean category flags default to `true`, meaning the full 19-category scan
runs unless flags are explicitly disabled in the configuration file. Setting
`enabled` to `false` bypasses the entire plugin execution and returns an empty
`PluginOutput` immediately.

---

## Security Scope

`SecurityCategory` is an enum that enumerates the 19 security concern areas the
plugin inspects. Each variant maps to one or more CWE identifiers and, where
applicable, an OWASP Top Ten category.

| Variant                    | CWE      | OWASP    | Description                                      |
| -------------------------- | -------- | -------- | ------------------------------------------------ |
| `SecretExposure`           | CWE-312  | A02:2021 | Cleartext storage of sensitive information       |
| `HardcodedCredential`      | CWE-798  | A02:2021 | Use of hard-coded credentials                    |
| `InsecureDependency`       | CWE-1035 | A06:2021 | Use of components with known vulnerabilities     |
| `UnsafeCodeUsage`          | CWE-676  | -        | Use of potentially dangerous functions           |
| `BrokenAuthentication`     | CWE-287  | A07:2021 | Improper authentication mechanisms               |
| `AccessControl`            | CWE-284  | A01:2021 | Improper access control enforcement              |
| `InsecureEndpoint`         | CWE-284  | A01:2021 | Endpoints lacking authorization or rate limiting |
| `CommandExecution`         | CWE-78   | A03:2021 | OS command injection via unsanitized input       |
| `InsecureDeserialization`  | CWE-502  | A08:2021 | Deserialization of untrusted data                |
| `CryptographyWeakness`     | CWE-327  | A02:2021 | Use of broken or weak cryptographic algorithms   |
| `InjectionVulnerability`   | CWE-89   | A03:2021 | SQL or other structured query injection          |
| `PathTraversal`            | CWE-22   | A01:2021 | Path traversal via unsanitized file paths        |
| `ServerSideRequestForgery` | CWE-918  | A10:2021 | SSRF via unvalidated outbound requests           |
| `SensitiveDataExposure`    | CWE-200  | A02:2021 | Inadvertent exposure of sensitive information    |
| `SecurityMisconfiguration` | CWE-16   | A05:2021 | Insecure default or runtime configuration        |
| `LoggingAndMonitoring`     | CWE-223  | A09:2021 | Insufficient logging of security-relevant events |
| `PrivilegeEscalation`      | CWE-269  | A01:2021 | Improper management of privilege assignment      |
| `RaceCondition`            | CWE-362  | -        | Concurrent execution with shared resource use    |
| `MemorySafety`             | CWE-119  | -        | Buffer errors and memory corruption vectors      |

The active category list is constructed at runtime from the config flags. Eight
categories map directly to boolean config fields. The remaining eleven are
always active when the plugin is enabled. `SecurityFilePrioritizer` ranks
repository files for inclusion in the analysis set, prioritizing authentication
handlers, endpoint definitions, and files touching external input or
cryptographic operations.

---

## Finding Model

`SecurityReviewFinding` is the plugin-specific finding type. Each instance
represents one confirmed or suspected security issue located during analysis.

| Field                  | Type               | Description                                            |
| ---------------------- | ------------------ | ------------------------------------------------------ |
| `category`             | `SecurityCategory` | The security concern category for this finding         |
| `severity`             | `FindingSeverity`  | Critical, High, Medium, Low, or Info                   |
| `cwe`                  | `Option<String>`   | CWE identifier, for example `"CWE-312"`                |
| `owasp`                | `Option<String>`   | OWASP category reference, for example `"A02:2021"`     |
| `file`                 | `String`           | Repository-relative path to the affected file          |
| `line`                 | `Option<u32>`      | Line number within the file, if determinable           |
| `symbol`               | `Option<String>`   | Function, variable, or type name where issue resides   |
| `evidence`             | `String`           | Redacted code snippet supporting the finding           |
| `exploitability`       | `String`           | How and under what conditions the issue is exploitable |
| `impact`               | `String`           | Potential business or technical impact if exploited    |
| `remediation`          | `String`           | Recommended remediation steps                          |
| `confidence`           | `f64`              | AI confidence score from 0.0 to 1.0                    |
| `false_positive_notes` | `Option<String>`   | Notes on false positive likelihood or known patterns   |
| `sarif_rule_id`        | `String`           | Stable rule identifier used in SARIF output            |
| `sarif_help_uri`       | `Option<String>`   | URI pointing to extended rule documentation            |

`FindingSeverity` is shared with the technical review and governance plugins and
is defined in `src/reports/finding.rs`. The `sarif_rule_id` is derived
deterministically from the `SecurityCategory` variant name to ensure stable
cross-run deduplication.

---

## Secret Handling

Evidence fields are automatically redacted before a `SecurityReviewFinding` is
stored in memory, written to any report file, or published in a Kafka result
message.

The redaction pass scans the raw evidence string for patterns that indicate the
presence of a credential value. When a match is detected, the value portion is
replaced with `[REDACTED]` while preserving enough surrounding context to locate
the source line.

Patterns that trigger redaction include, but are not limited to:

- `password=`
- `secret=`
- `api_key=`
- `token=`
- `auth=`
- `private_key=`
- `access_key=`
- `bearer`
- Hex or base64 sequences of 20 or more characters adjacent to any of the above
  keywords

The redaction function operates case-insensitively and handles assignment
operator forms (`=`, `:`, `:=`) as well as whitespace-separated forms.

Redaction is applied in `SecurityReviewFinding::new` before the struct is
returned to the caller, ensuring that raw credential values never appear in heap
memory beyond the construction point. Tests verify that the redaction function
preserves the keyword while replacing the value and that multiple occurrences
within a single evidence string are all handled.

---

## SARIF Output

When `include_sarif` is `true` (the default), the plugin writes a
`security_review.sarif` file in the workspace output directory. The file
conforms to SARIF 2.1.0 as defined by the
[OASIS SARIF specification](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html).

The top-level SARIF document structure produced by the plugin:

```text
{
  "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
  "version": "2.1.0",
  "runs": [
    {
      "tool": { "driver": { "name", "version", "informationUri", "rules": [...] } },
      "results": [ ... ]
    }
  ]
}
```

**Tool metadata**: The `driver.name` is `"xzardgz-security-review"` and the
version is set from `CARGO_PKG_VERSION` at compile time.

**Rules**: One SARIF rule entry is emitted per unique `sarif_rule_id` present in
the filtered finding list. Each rule entry includes:

- `id` - the `sarif_rule_id` value
- `name` - a human-readable name derived from the `SecurityCategory` variant
- `shortDescription.text` - one-sentence description of the security concern
- `help.text` - combined CWE and OWASP references for the rule
- `helpUri` - the `sarif_help_uri` value when present

**Results**: Each `SecurityReviewFinding` maps to one SARIF result entry.
Severity is mapped as follows:

| Finding Severity | SARIF Level |
| ---------------- | ----------- |
| Critical         | `"error"`   |
| High             | `"error"`   |
| Medium           | `"warning"` |
| Low              | `"note"`    |
| Info             | `"note"`    |

**Locations**: The `physicalLocation.artifactLocation.uri` is set to the `file`
field of the finding. When `line` is present, `region.startLine` is populated.

**Fingerprints**: Each result carries a `partialFingerprints` entry keyed
`"securityReviewV1"` whose value is a SHA-256 hex digest of
`{sarif_rule_id}:{file}:{line}`. This enables CI/CD tooling to deduplicate
findings across successive runs.

---

## Reports

The plugin produces three output files in the workspace `output/` directory.

### security_review.md

A human-readable Markdown report. Findings are grouped by `SecurityCategory`
with a summary table at the top showing total counts by severity. Within each
category section, findings are listed in descending severity order. Each finding
entry shows the file path, line number, symbol, severity, CWE, OWASP mapping,
evidence snippet, exploitability, impact, and remediation.

A risk band derived from the highest severity present among all retained
findings appears in the report header: `CRITICAL`, `HIGH`, `MEDIUM`, `LOW`, or
`CLEAR`.

### security_review.json

A machine-readable JSON file serialized from a
`ReportEnvelope<SecurityReviewFinding>`. The envelope carries:

- `plugin` - `"security-review"`
- `version` - crate version string
- `workspace_id` - from `PluginContext`
- `timestamp` - ISO 8601 UTC timestamp of report generation
- `findings` - the filtered and capped `Vec<SecurityReviewFinding>`
- `metadata` - map of additional key-value pairs including risk band and
  category counts

Loading the file with `ReportEnvelope::load_from_json` produces a typed value
that can be processed by downstream pipeline stages.

### security_review.sarif

The SARIF 2.1.0 file described in the SARIF Output section above. Written only
when `include_sarif` is `true`. CI/CD integrations such as GitHub Advanced
Security, GitLab SAST, and Azure DevOps can ingest this file directly to
annotate pull requests with inline security findings.

---

## CI Behavior

The `fail_on_critical` flag controls whether the presence of critical severity
findings causes the plugin to signal failure to the calling process.

When `fail_on_critical` is `true`:

- The plugin returns a `PluginOutput` with `completed` set to `false` if any
  retained finding has `severity == Critical`.
- The CLI propagates this as a non-zero exit code, allowing pipeline stages to
  gate on security outcomes.
- All three report files are still written before returning; the failure signal
  does not suppress the reports.

The `severity_threshold` field controls which findings are included in reports
and in the failure signal. Findings below the threshold are discarded after
confidence filtering. The threshold values from most to least restrictive are:
`"critical"`, `"high"`, `"medium"`, `"low"`, `"info"`.

In watcher mode, when `fail_on_critical` triggers, the `WatcherResultMessage` is
published with `success: false`. The message still carries the findings summary
and report paths so the downstream consumer can act on the data.
`PublishFailureState` persists the message to disk if the Kafka publish itself
fails.

---

## Execution Flow

The following steps describe the full execution sequence of
`SecurityReviewPlugin::run`:

1. Call `SecurityReviewConfig::validate` and return `Err` if validation fails.
1. Check `config.enabled`; return an empty `PluginOutput` immediately if
   `false`.
1. Select security-relevant files using `SecurityFilePrioritizer` from the
   `ScanResult`.
1. Determine the active `Vec<SecurityCategory>` list from the enabled config
   flags.
1. Build system and user prompts via `PromptBuilder`.
1. Call `Provider::complete` for AI analysis; repeat for `verification_turns`
   passes.
1. Parse the JSON response into a `Vec<SecurityReviewFinding>`, applying
   evidence redaction in `SecurityReviewFinding::new` during construction.
1. Filter findings where `confidence < confidence_threshold`.
1. Filter findings where severity is below `severity_threshold`.
1. Cap the finding list at `max_findings`, retaining higher severity findings
   first.
1. Compute the risk band from the maximum severity present in the retained list.
1. Write `security_review.md`, `security_review.json`, and
   `security_review.sarif`.
1. Return `PluginOutput` with report paths, risk band, and the
   `fail_on_critical` signal.

---

## Watcher Integration

When a `WatcherTaskMessage` arrives with `plugin` set to `"security-review"`,
`WatcherExecutor` routes the task through the plugin registry to
`SecurityReviewPlugin`. `WatcherExecutor::process_task` builds a `PluginContext`
from the incoming task fields and calls `plugin.run(ctx)`. On completion it
constructs a `WatcherResultMessage` containing:

- `success` - set from `output.completed`
- `findings_summary` - total finding count and `by_severity` breakdown
- `report_paths` - paths from `output.report_paths`
- `sarif_path` - path to `security_review.sarif` when present
- `risk_band` - from `output.risk_band`
- `diagnostics` - merged from `output.diagnostics`
- `workspace_id` - from the workspace state
- `provider_metadata` - from `output.provider_metadata`

The result message is published to the Kafka result topic by
`KafkaResultPublisher`. If publication fails, `PublishFailureState` records the
failure to disk so the finding data is not lost. Local CLI execution and
watcher-triggered execution use identical code paths through the plugin; there
is no separate code branch for each entry point.

---

## Testing

Each sub-module has a `#[cfg(test)] mod tests` block with tests following the
`test_<function>_<condition>_<expected>` naming convention.

| Module       | Key Test Areas                                                                      |
| ------------ | ----------------------------------------------------------------------------------- |
| `config.rs`  | Default values, serde roundtrip, threshold validation, `enabled` gate               |
| `finding.rs` | Construction, evidence redaction, serialization roundtrip, `sarif_rule_id` format   |
| `scope.rs`   | `SecurityCategory` variant count, CWE accessor, OWASP accessor, file prioritization |
| `report.rs`  | Markdown grouping, JSON `ReportEnvelope` structure, SARIF schema fields             |
| `plugin.rs`  | `enabled=false` skip, full run with `MockProvider`, finding cap, `fail_on_critical` |

A `MockProvider` from `crate::providers::base` is used to inject canned JSON
responses without making network calls:

```rust
let mut mock = MockProvider::new();
mock.expect_complete()
    .returning(|_, _| Ok(FIXTURE_FINDINGS_JSON.to_string()));
```

An integration test in `tests/security_review_integration.rs` exercises the
plugin against a fixture repository scan, verifies all three output files are
written, checks that the SARIF document parses as valid JSON, and confirms that
`ReportEnvelope::load_from_json` succeeds on the JSON report.
