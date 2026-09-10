# CycloneDX 1.7 JSON Schema

## Provenance

| Field           | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| Schema version  | CycloneDX 1.7                                                   |
| Upstream URL    | `https://cyclonedx.org/schema/bom-1.7.schema.json`              |
| Upstream source | CycloneDX specification repository (Apache-2.0)                 |
| Retrieved       | 2026-09-10                                                      |
| Purpose         | Golden-file JSON Schema validation in Phase 0 and Phase 5 tests |

## Usage

`bom-1.7.schema.json` is vendored here for offline, reproducible test execution.
It is used by `tests/sast_phase0.rs` to verify the schema is valid JSON Schema,
and will be used by `output/cyclonedx.rs` tests in Phase 5 to validate generated
CycloneDX 1.7 output.

## Notes

This schema contains external `$ref` entries for sub-schemas
(`spdx.schema.json`, `jsf-0.82.schema.json`, `cryptography-defs.schema.json`)
that are not vendored alongside it. Tests that need to validate CycloneDX BOM
documents (not the schema itself) must provide those sub-schemas or use a
permissive validator configuration. Tests that only validate the schema
structure use `jsonschema::meta::validate`, which does not resolve external
references.

The schema and its license are governed by the Apache License 2.0.
