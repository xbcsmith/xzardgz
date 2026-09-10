# SARIF 2.1.0 JSON Schema

## Provenance

| Field           | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| Schema version  | SARIF 2.1.0                                                     |
| Upstream URL    | `https://json.schemastore.org/sarif-2.1.0.json`                 |
| Upstream source | JSON Schema Store (sourced from oasis-tcs/sarif-spec)           |
| Retrieved       | 2026-09-10                                                      |
| Purpose         | Golden-file JSON Schema validation in Phase 0 and Phase 5 tests |

## Usage

`sarif-2.1.0.schema.json` is vendored here for offline, reproducible test
execution. It is used by `tests/sast_phase0.rs` to verify the schema is valid
JSON Schema, and will be used by `output/sarif.rs` tests in Phase 5 to validate
generated SARIF 2.1.0 output.

## Notes

The SARIF 2.1.0 schema is maintained by OASIS and the Microsoft SARIF SDK. The
`$id` in the vendored file is
`https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json`.

The schema is self-contained and does not reference external sub-schemas.
