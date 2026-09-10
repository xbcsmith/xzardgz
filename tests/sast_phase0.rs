//! Phase 0 SAST implementation tests.
//!
//! Validates the Phase 0 deliverables:
//! - Vendored JSON Schema files (CycloneDX 1.7 and SARIF 2.1.0) parse as valid
//!   JSON Schema documents.
//! - The `sast_corpus` helper behaves correctly with and without the
//!   `XZARDGZ_SEMGREP_RULES_DIR` environment variable.

#[path = "helpers/sast_corpus.rs"]
mod sast_corpus;

#[cfg(test)]
mod schema_tests {
    /// Assert that the vendored CycloneDX 1.7 JSON Schema is well-formed and
    /// is a valid JSON Schema document according to the `jsonschema` crate.
    ///
    /// The schema contains external `$ref`s (`spdx.schema.json`,
    /// `jsf-0.82.schema.json`, `cryptography-defs.schema.json`) that are not
    /// vendored alongside it. `jsonschema::meta::validate` checks structural
    /// validity without resolving external references, which is the correct
    /// scope for this test.
    #[test]
    fn test_cyclonedx_schema_is_valid_json_schema() {
        let schema_str = include_str!("../testdata/cyclonedx/bom-1.7.schema.json");
        let schema_value: serde_json::Value = serde_json::from_str(schema_str)
            .expect("testdata/cyclonedx/bom-1.7.schema.json must be valid JSON");
        jsonschema::meta::validate(&schema_value).unwrap_or_else(|e| {
            panic!("testdata/cyclonedx/bom-1.7.schema.json must be a valid JSON Schema: {e}")
        });
    }

    /// Assert that the vendored SARIF 2.1.0 JSON Schema is well-formed and
    /// is a valid JSON Schema document according to the `jsonschema` crate.
    #[test]
    fn test_sarif_schema_is_valid_json_schema() {
        let schema_str = include_str!("../testdata/sarif/sarif-2.1.0.schema.json");
        let schema_value: serde_json::Value = serde_json::from_str(schema_str)
            .expect("testdata/sarif/sarif-2.1.0.schema.json must be valid JSON");
        jsonschema::meta::validate(&schema_value).unwrap_or_else(|e| {
            panic!("testdata/sarif/sarif-2.1.0.schema.json must be a valid JSON Schema: {e}")
        });
    }
}
