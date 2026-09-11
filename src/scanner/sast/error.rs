//! Error types for the SAST scanning pipeline.
//!
//! All SAST errors are represented by [`SastError`]. Lower-level parsing
//! failures are broken out into [`RuleParseError`] so callers can
//! pattern-match on the specific cause without string parsing.

use thiserror::Error;

// ---------------------------------------------------------------------------
// Rule parse errors
// ---------------------------------------------------------------------------

/// Structured error produced while parsing a SAST rule file.
#[derive(Error, Debug)]
pub enum RuleParseError {
    /// A required field is absent from the rule definition.
    #[error("missing required field '{field}'")]
    MissingField {
        /// The name of the absent field.
        field: String,
    },

    /// A field value did not satisfy a structural or semantic invariant.
    #[error("invalid value: {message}")]
    InvalidValue {
        /// Human-readable description of the violation.
        message: String,
    },

    /// The rule id does not match the required `^[a-zA-Z0-9._-]+$` pattern.
    #[error("rule id '{id}' contains disallowed characters: {reason}")]
    InvalidId {
        /// The offending rule id.
        id: String,
        /// Human-readable explanation of the constraint that was violated.
        reason: String,
    },

    /// An unsupported formula construct was used.
    #[error("unsupported construct '{construct}' in rule '{rule_id}'")]
    UnsupportedConstruct {
        /// The unsupported construct name (e.g. `pattern-propagators`).
        construct: String,
        /// The rule that contained the construct.
        rule_id: String,
    },

    /// A structural invariant of the rule formula was violated.
    ///
    /// Used when a `patterns:` list is empty or has no positive term.
    #[error("formula invariant violated in rule '{rule_id}': {reason}")]
    Invariant {
        /// The rule whose formula violated an invariant.
        rule_id: String,
        /// Human-readable description of the invariant that was violated.
        reason: String,
    },

    /// The rule's formula root is absent or ambiguous.
    ///
    /// Raised when zero or more than one of `pattern`, `patterns`,
    /// `pattern-either`, and `pattern-regex` are present at the rule root.
    #[error("schema error in rule '{rule_id}': {reason}")]
    Schema {
        /// The rule whose schema is invalid.
        rule_id: String,
        /// Human-readable description of the schema violation.
        reason: String,
    },

    /// The YAML input could not be deserialized into a [`RuleFile`].
    ///
    /// [`RuleFile`]: crate::scanner::sast::rule::schema::RuleFile
    #[error("YAML parse error: {0}")]
    Yaml(String),
}

// ---------------------------------------------------------------------------
// SkipReason
// ---------------------------------------------------------------------------

/// The reason a rule was not compiled into a [`RuleIr`] by the compatibility gate.
///
/// Rules that reference unsupported engine constructs are excluded from
/// compilation rather than silently dropped. Each exclusion carries one or more
/// `SkipReason` values so callers can report exactly which constructs blocked
/// evaluation.
///
/// [`RuleIr`]: crate::scanner::sast::rule::ir::RuleIr
#[derive(Debug, Clone)]
pub enum SkipReason {
    /// Rule declares `mode: taint`.
    TaintMode,
    /// Rule declares `mode: join`.
    JoinMode,
    /// Rule declares `mode: extract`.
    ExtractMode,
    /// Rule declares `mode: step`.
    StepMode,
    /// Rule uses `fix-regex`, which requires regex-based autofix not yet supported.
    FixRegex,
    /// Rule or pattern term uses `pattern-propagators`.
    PatternPropagators,
    /// A pattern string contains deep-expression syntax (`<... ... ...>`).
    DeepExpression,
    /// A pattern string contains typed-metavariable syntax (`(TypeName $X)`).
    TypedMetavariable,
    /// A pattern term uses `metavariable-analysis`.
    MetavariableAnalysis,
    /// None of the rule's declared languages are supported by this engine.
    NoSupportedLanguage(Vec<String>),
}

// ---------------------------------------------------------------------------
// SAST pipeline error
// ---------------------------------------------------------------------------

/// Top-level error type for the SAST scanning pipeline.
///
/// Every public function in `scanner::sast` returns
/// `Result<T, SastError>`. Callers that need to surface SAST errors in
/// the wider pipeline can convert via the `From<SastError> for
/// crate::error::PipelineError` implementation.
#[derive(Error, Debug)]
pub enum SastError {
    /// A rule file could not be read from disk.
    #[error("failed to read rule file '{path}': {cause}")]
    RuleRead {
        /// Filesystem path that could not be read.
        path: String,
        /// Underlying IO error description.
        cause: String,
    },

    /// A rule file was read but could not be parsed.
    #[error("failed to parse rule file '{path}': {source}")]
    RuleParse {
        /// Filesystem path of the malformed rule file.
        path: String,
        /// Structured parse failure.
        #[source]
        source: RuleParseError,
    },

    /// A source file could not be read for scanning.
    #[error("failed to read file '{path}': {cause}")]
    FileRead {
        /// Filesystem path that could not be read.
        path: String,
        /// Underlying IO error description.
        cause: String,
    },

    /// A source file exceeded the configured byte-size limit and was skipped.
    #[error("file '{path}' is too large: {bytes} bytes (limit: {limit} bytes)")]
    FileTooLarge {
        /// Filesystem path of the oversized file.
        path: String,
        /// Actual size of the file in bytes.
        bytes: u64,
        /// Configured maximum size in bytes.
        limit: u64,
    },

    /// A rule-supplied regex failed to compile.
    ///
    /// This includes patterns that exceed the 10 MiB NFA/DFA size limit
    /// enforced by [`regex::RegexBuilder::size_limit`] and
    /// [`regex::RegexBuilder::dfa_size_limit`].
    #[error("regex compile error for rule '{rule_id}': {cause}")]
    RegexCompile {
        /// ID of the rule whose `pattern-regex` failed to compile.
        rule_id: String,
        /// Error message from the regex engine.
        cause: String,
    },

    /// Rule evaluation timed out while scanning a file.
    #[error("scan timed out for rule '{rule_id}' on '{path}'")]
    ScanTimeout {
        /// ID of the rule that timed out.
        rule_id: String,
        /// Filesystem path that was being scanned.
        path: String,
    },

    /// An unexpected internal condition was reached.
    ///
    /// This variant is used for logic errors that should not occur under
    /// normal operation, such as a formula tree containing no scannable leaf.
    #[error("internal sast error: {0}")]
    Internal(String),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sast_error_rule_read_display_contains_path_and_cause() {
        let err = SastError::RuleRead {
            path: "/rules/foo.yaml".to_string(),
            cause: "permission denied".to_string(),
        };
        let s = err.to_string();
        assert!(s.contains("/rules/foo.yaml"));
        assert!(s.contains("permission denied"));
    }

    #[test]
    fn test_sast_error_regex_compile_display_contains_rule_id_and_cause() {
        let err = SastError::RegexCompile {
            rule_id: "my-rule".to_string(),
            cause: "unclosed group".to_string(),
        };
        let s = err.to_string();
        assert!(s.contains("my-rule"));
        assert!(s.contains("unclosed group"));
    }

    #[test]
    fn test_sast_error_file_too_large_display_contains_bytes_and_limit() {
        let err = SastError::FileTooLarge {
            path: "/src/big.rs".to_string(),
            bytes: 20_000_000,
            limit: 5_242_880,
        };
        let s = err.to_string();
        assert!(s.contains("20000000"));
        assert!(s.contains("5242880"));
    }

    #[test]
    fn test_sast_error_internal_display_contains_message() {
        let err = SastError::Internal("no pattern-regex leaf in formula".to_string());
        assert!(err.to_string().contains("no pattern-regex leaf in formula"));
    }

    #[test]
    fn test_rule_parse_error_missing_field_display_contains_field() {
        let err = RuleParseError::MissingField {
            field: "languages".to_string(),
        };
        assert!(err.to_string().contains("languages"));
    }

    #[test]
    fn test_sast_error_rule_parse_wraps_source() {
        let source = RuleParseError::MissingField {
            field: "id".to_string(),
        };
        let err = SastError::RuleParse {
            path: "/rules/bad.yaml".to_string(),
            source,
        };
        let s = err.to_string();
        assert!(s.contains("/rules/bad.yaml"));
    }
}
