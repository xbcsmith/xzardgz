# SARIF Output Implementation

## Overview

Phase 5 adds SARIF 2.1.0 projection to the SAST scanner output pipeline. The
`render_sarif` function in `src/scanner/sast/output/sarif.rs` converts a
`SastScanReport` containing rich Phase 5 `SastMatch` objects into a SARIF log
document suitable for consumption by GitHub Advanced Security, VS Code SARIF
Viewer, and any other SARIF-aware tooling.

## Design Decisions

### Single Run Per Log

Each call to `render_sarif` produces a `SarifLog` with exactly one `SarifRun`.
This simplifies the model: one scanner invocation corresponds to one run.

### Rule Deduplication

The SARIF driver lists every distinct rule that produced at least one result.
Rules are deduplicated by `rule_id` in first-seen order (the order determined by
the sorted `SastScanReport.matches` vector). The first match for each `rule_id`
supplies the rule descriptor.

### Column Numbering Conversion

The `Position` type in `match_model` stores 0-based byte columns (`col`). SARIF
requires 1-based columns. The projection adds 1 to every `Position.col` value
when populating `SarifRegion.startColumn` and `SarifRegion.endColumn`.

### Severity Mapping

| `Severity` variant | SARIF `level` |
| ------------------ | ------------- |
| `Info`             | `"note"`      |
| `Warning`          | `"warning"`   |
| `Error`            | `"error"`     |

### Tag Construction

The `properties.tags` array in each driver rule is assembled as follows:

1. All values from `metadata.cwe` (e.g. `"CWE-326"`).
2. The literal `"security"` tag, added whenever at least one CWE is present.
3. All values from `metadata.owasp` (e.g. `"A02:2021"`).

Rules with neither CWE nor OWASP metadata produce an empty tags array.

### Fingerprint Key

Each SARIF result's `fingerprints` map contains exactly one entry:

```text
"xzardgz/v1": "<fingerprint>"
```

The fingerprint value comes from `SastMatch.fingerprint`, which is the
BLAKE2b-256 content-addressed fingerprint computed by
`crate::scanner::sast::fingerprint::compute_fingerprint`.

### Short Description Fallback

The rule `shortDescription.text` is taken from `metadata.description` when
present. When `metadata.description` is `None`, the match `message` is used as
the short description. The `help.text` field always uses the match `message`.

## File Layout

```text
src/scanner/sast/output/sarif.rs       - SARIF renderer and type definitions
testdata/sarif/sarif-2.1.0.schema.json - SARIF JSON Schema (for test validation)
testdata/sast/golden/sarif_empty.json  - Golden file: empty report output
testdata/sast/golden/sarif_one_match.json - Golden file: one-match report output
```

## Type Hierarchy

```text
SarifLog
  version: &'static str          ("2.1.0")
  $schema: &'static str          (OASIS URI)
  runs: Vec<SarifRun>
    tool: SarifTool
      driver: SarifDriver
        name: String             ("xzardgz-sast")
        version: String          ("0.1.0")
        informationUri: String
        rules: Vec<SarifRule>    (deduplicated by rule_id)
          id: String
          name: String
          shortDescription: SarifMessage
          help: SarifMessage
          properties: SarifRuleProperties
            tags: Vec<String>    (CWE, "security", OWASP)
    results: Vec<SarifResult>
      ruleId: String
      message: SarifMessage
      level: String              ("note" | "warning" | "error")
      locations: Vec<SarifLocation>
        physicalLocation: SarifPhysicalLocation
          artifactLocation: SarifArtifactLocation
            uri: String          (repo-relative path)
            uriBaseId: String    ("%SRCROOT%")
          region: SarifRegion
            startLine: u32       (1-based)
            startColumn: u32     (1-based; Position.col + 1)
            endLine: u32
            endColumn: u32
      fingerprints: BTreeMap<String, String>
        "xzardgz/v1" -> fingerprint
```

## Serialisation Notes

All structs use `#[serde(rename_all = "camelCase")]` where needed to match the
SARIF JSON key naming convention. The `$schema` key on `SarifLog` is produced
with `#[serde(rename = "$schema")]` on the `schema` field, since `$schema` is
not a valid Rust identifier.

## Tests

The test suite in `sarif.rs` covers:

- Schema validation against the official SARIF 2.1.0 JSON Schema for both
  single-match and empty reports.
- Severity mapping for all three variants.
- Tag construction including CWE, OWASP, and the `"security"` synthetic tag.
- Fingerprint key presence and value correctness.
- SARIF column conversion from 0-based `Position.col` to 1-based SARIF columns.
- Rule deduplication: two matches with the same `rule_id` produce one driver
  rule; two distinct `rule_id` values produce two driver rules.
- Tool metadata: driver name, version string, schema URI.
- Golden file comparison for both the empty and one-match cases.
