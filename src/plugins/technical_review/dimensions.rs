//! Review dimensions evaluated by the technical review plugin.
//!
//! Each [`ReviewDimension`] represents a distinct aspect of codebase quality
//! that the plugin assesses.  Dimensions are selected via the `focus_areas`
//! field in [`TechnicalReviewConfig`][crate::config::TechnicalReviewConfig].
//! Unknown focus area strings are silently ignored; an empty list defaults to
//! all 14 dimensions.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ReviewDimension
// ---------------------------------------------------------------------------

/// A code-quality dimension evaluated by the technical review plugin.
///
/// The 14 variants span the full software-engineering lifecycle, from
/// high-level architecture down to operational readiness.  Variants are listed
/// in a canonical order used by [`ReviewDimension::all`].
///
/// # Examples
///
/// ```
/// use xzardgz::plugins::technical_review::dimensions::ReviewDimension;
///
/// assert_eq!(ReviewDimension::Architecture.as_str(), "architecture");
/// assert_eq!(ReviewDimension::ErrorHandling.display_name(), "Error Handling");
/// assert_eq!(ReviewDimension::all().len(), 14);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDimension {
    /// High-level structural design of the codebase.
    Architecture,
    /// Degree to which responsibilities are separated into cohesive units.
    Modularity,
    /// Ease of understanding, modifying, and extending the code.
    Maintainability,
    /// Correctness and completeness of error propagation and handling.
    ErrorHandling,
    /// Coverage, quality, and organisation of automated tests.
    TestingPosture,
    /// Currency, correctness, and security of third-party dependencies.
    DependencyHygiene,
    /// Usability and discoverability of command-line interfaces.
    CliUsability,
    /// Design and ergonomics of public API surfaces.
    ApiUsability,
    /// Usability and safety of configuration mechanisms.
    ConfigurationErgonomics,
    /// Presence and quality of logging, metrics, and tracing.
    Observability,
    /// Completeness and accuracy of inline and external documentation.
    DocumentationCoverage,
    /// Algorithmic complexity, allocation patterns, and latency risks.
    PerformanceRisks,
    /// CI/CD pipeline quality, release automation, and artefact hygiene.
    BuildAndReleaseHygiene,
    /// Readiness for production deployment, runbooks, and on-call support.
    OperationalReadiness,
}

impl ReviewDimension {
    /// Returns all 14 review dimensions in canonical order.
    ///
    /// The canonical order is the order in which variants are declared in the
    /// enum definition.
    ///
    /// # Returns
    ///
    /// A `Vec<ReviewDimension>` with all 14 variants.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::dimensions::ReviewDimension;
    ///
    /// let all = ReviewDimension::all();
    /// assert_eq!(all.len(), 14);
    /// assert_eq!(all[0], ReviewDimension::Architecture);
    /// assert_eq!(all[13], ReviewDimension::OperationalReadiness);
    /// ```
    pub fn all() -> Vec<Self> {
        vec![
            Self::Architecture,
            Self::Modularity,
            Self::Maintainability,
            Self::ErrorHandling,
            Self::TestingPosture,
            Self::DependencyHygiene,
            Self::CliUsability,
            Self::ApiUsability,
            Self::ConfigurationErgonomics,
            Self::Observability,
            Self::DocumentationCoverage,
            Self::PerformanceRisks,
            Self::BuildAndReleaseHygiene,
            Self::OperationalReadiness,
        ]
    }

    /// Returns the snake_case identifier string for this dimension.
    ///
    /// The identifier is used as the `category` field in
    /// [`TechnicalReviewFinding`][super::finding::TechnicalReviewFinding] and
    /// as a valid value for `focus_areas` in the plugin configuration.
    ///
    /// # Returns
    ///
    /// A static `&str` in lowercase snake_case.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::dimensions::ReviewDimension;
    ///
    /// assert_eq!(ReviewDimension::Architecture.as_str(), "architecture");
    /// assert_eq!(ReviewDimension::BuildAndReleaseHygiene.as_str(), "build_and_release_hygiene");
    /// assert_eq!(ReviewDimension::ApiUsability.as_str(), "api_usability");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Architecture => "architecture",
            Self::Modularity => "modularity",
            Self::Maintainability => "maintainability",
            Self::ErrorHandling => "error_handling",
            Self::TestingPosture => "testing_posture",
            Self::DependencyHygiene => "dependency_hygiene",
            Self::CliUsability => "cli_usability",
            Self::ApiUsability => "api_usability",
            Self::ConfigurationErgonomics => "configuration_ergonomics",
            Self::Observability => "observability",
            Self::DocumentationCoverage => "documentation_coverage",
            Self::PerformanceRisks => "performance_risks",
            Self::BuildAndReleaseHygiene => "build_and_release_hygiene",
            Self::OperationalReadiness => "operational_readiness",
        }
    }

    /// Returns a human-readable display name for this dimension.
    ///
    /// Display names use title case and are intended for report headings.
    ///
    /// # Returns
    ///
    /// A static `&str` in title case.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::dimensions::ReviewDimension;
    ///
    /// assert_eq!(ReviewDimension::ErrorHandling.display_name(), "Error Handling");
    /// assert_eq!(ReviewDimension::ApiUsability.display_name(), "API Usability");
    /// assert_eq!(ReviewDimension::CliUsability.display_name(), "CLI Usability");
    /// ```
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Architecture => "Architecture",
            Self::Modularity => "Modularity",
            Self::Maintainability => "Maintainability",
            Self::ErrorHandling => "Error Handling",
            Self::TestingPosture => "Testing Posture",
            Self::DependencyHygiene => "Dependency Hygiene",
            Self::CliUsability => "CLI Usability",
            Self::ApiUsability => "API Usability",
            Self::ConfigurationErgonomics => "Configuration Ergonomics",
            Self::Observability => "Observability",
            Self::DocumentationCoverage => "Documentation Coverage",
            Self::PerformanceRisks => "Performance Risks",
            Self::BuildAndReleaseHygiene => "Build and Release Hygiene",
            Self::OperationalReadiness => "Operational Readiness",
        }
    }

    /// Returns a brief description of what this dimension covers.
    ///
    /// Descriptions are used in prompt engineering and report footers.
    ///
    /// # Returns
    ///
    /// A static `&str` with a one-sentence description.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::dimensions::ReviewDimension;
    ///
    /// let desc = ReviewDimension::Architecture.description();
    /// assert!(!desc.is_empty());
    /// assert!(desc.contains("structural") || desc.contains("design"));
    /// ```
    pub fn description(&self) -> &'static str {
        match self {
            Self::Architecture => "High-level structural design of the codebase.",
            Self::Modularity => "Separation of responsibilities into cohesive, decoupled units.",
            Self::Maintainability => "Ease of understanding, modifying, and extending the code.",
            Self::ErrorHandling => {
                "Correctness and completeness of error propagation and handling."
            }
            Self::TestingPosture => "Coverage, quality, and organisation of automated tests.",
            Self::DependencyHygiene => {
                "Currency, correctness, and security of third-party dependencies."
            }
            Self::CliUsability => "Usability and discoverability of command-line interfaces.",
            Self::ApiUsability => "Design and ergonomics of public API surfaces.",
            Self::ConfigurationErgonomics => "Usability and safety of configuration mechanisms.",
            Self::Observability => "Presence and quality of logging, metrics, and tracing.",
            Self::DocumentationCoverage => {
                "Completeness and accuracy of inline and external documentation."
            }
            Self::PerformanceRisks => {
                "Algorithmic complexity, allocation patterns, and latency risks."
            }
            Self::BuildAndReleaseHygiene => {
                "CI/CD pipeline quality, release automation, and artefact hygiene."
            }
            Self::OperationalReadiness => {
                "Readiness for production deployment, runbooks, and on-call support."
            }
        }
    }

    /// Parses a [`ReviewDimension`] from a snake_case string (case-insensitive).
    ///
    /// Matches against the values returned by [`as_str`][Self::as_str].
    ///
    /// # Arguments
    ///
    /// * `s` - Dimension identifier string to parse.
    ///
    /// # Returns
    ///
    /// `Some(ReviewDimension)` when `s` matches a known identifier,
    /// `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::dimensions::ReviewDimension;
    ///
    /// assert_eq!(ReviewDimension::from_str("architecture"), Some(ReviewDimension::Architecture));
    /// assert_eq!(ReviewDimension::from_str("ERROR_HANDLING"), Some(ReviewDimension::ErrorHandling));
    /// assert_eq!(ReviewDimension::from_str("api_usability"), Some(ReviewDimension::ApiUsability));
    /// assert!(ReviewDimension::from_str("unknown").is_none());
    /// assert!(ReviewDimension::from_str("").is_none());
    /// ```
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        let lower = s.to_lowercase();
        Self::all()
            .into_iter()
            .find(|d| d.as_str() == lower.as_str())
    }

    /// Returns the dimensions matching a list of focus area strings.
    ///
    /// Unknown strings are silently ignored.  If `areas` is empty, all 14
    /// dimensions are returned.
    ///
    /// # Arguments
    ///
    /// * `areas` - Focus area identifiers, typically from
    ///   `TechnicalReviewConfig.focus_areas`.
    ///
    /// # Returns
    ///
    /// A `Vec<ReviewDimension>` with one entry per recognised area, in the
    /// order they appear in `areas`, or [`all`][Self::all] when `areas` is
    /// empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::plugins::technical_review::dimensions::ReviewDimension;
    ///
    /// let areas = vec!["architecture".to_string(), "error_handling".to_string()];
    /// let dims = ReviewDimension::from_focus_areas(&areas);
    /// assert_eq!(dims.len(), 2);
    /// assert_eq!(dims[0], ReviewDimension::Architecture);
    /// assert_eq!(dims[1], ReviewDimension::ErrorHandling);
    ///
    /// // Unknown strings are silently dropped.
    /// let mixed = vec!["architecture".to_string(), "unknown_area".to_string()];
    /// assert_eq!(ReviewDimension::from_focus_areas(&mixed).len(), 1);
    ///
    /// // Empty list returns all dimensions.
    /// let empty: Vec<String> = vec![];
    /// assert_eq!(ReviewDimension::from_focus_areas(&empty).len(), 14);
    /// ```
    pub fn from_focus_areas(areas: &[String]) -> Vec<Self> {
        if areas.is_empty() {
            return Self::all();
        }
        areas.iter().filter_map(|a| Self::from_str(a)).collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // all()
    // ------------------------------------------------------------------

    #[test]
    fn test_review_dimension_all_returns_14_variants() {
        assert_eq!(ReviewDimension::all().len(), 14);
    }

    #[test]
    fn test_review_dimension_all_first_is_architecture() {
        assert_eq!(ReviewDimension::all()[0], ReviewDimension::Architecture);
    }

    #[test]
    fn test_review_dimension_all_last_is_operational_readiness() {
        let all = ReviewDimension::all();
        assert_eq!(all[all.len() - 1], ReviewDimension::OperationalReadiness);
    }

    #[test]
    fn test_review_dimension_all_contains_no_duplicates() {
        let all = ReviewDimension::all();
        let mut seen = std::collections::HashSet::new();
        for d in &all {
            assert!(seen.insert(d.clone()), "duplicate dimension: {:?}", d);
        }
    }

    // ------------------------------------------------------------------
    // as_str()
    // ------------------------------------------------------------------

    #[test]
    fn test_review_dimension_as_str_architecture_is_lowercase() {
        assert_eq!(ReviewDimension::Architecture.as_str(), "architecture");
    }

    #[test]
    fn test_review_dimension_as_str_error_handling_uses_snake_case() {
        assert_eq!(ReviewDimension::ErrorHandling.as_str(), "error_handling");
    }

    #[test]
    fn test_review_dimension_as_str_build_and_release_hygiene_is_correct() {
        assert_eq!(
            ReviewDimension::BuildAndReleaseHygiene.as_str(),
            "build_and_release_hygiene"
        );
    }

    #[test]
    fn test_review_dimension_as_str_api_usability_is_correct() {
        assert_eq!(ReviewDimension::ApiUsability.as_str(), "api_usability");
    }

    #[test]
    fn test_review_dimension_as_str_all_variants_return_non_empty() {
        for d in ReviewDimension::all() {
            assert!(!d.as_str().is_empty(), "{:?} returned empty as_str", d);
        }
    }

    // ------------------------------------------------------------------
    // display_name()
    // ------------------------------------------------------------------

    #[test]
    fn test_review_dimension_display_name_error_handling_has_spaces() {
        assert_eq!(
            ReviewDimension::ErrorHandling.display_name(),
            "Error Handling"
        );
    }

    #[test]
    fn test_review_dimension_display_name_api_usability_is_uppercase() {
        assert_eq!(
            ReviewDimension::ApiUsability.display_name(),
            "API Usability"
        );
    }

    #[test]
    fn test_review_dimension_display_name_cli_usability_is_uppercase() {
        assert_eq!(
            ReviewDimension::CliUsability.display_name(),
            "CLI Usability"
        );
    }

    #[test]
    fn test_review_dimension_display_name_all_variants_non_empty() {
        for d in ReviewDimension::all() {
            assert!(
                !d.display_name().is_empty(),
                "{:?} returned empty display_name",
                d
            );
        }
    }

    // ------------------------------------------------------------------
    // description()
    // ------------------------------------------------------------------

    #[test]
    fn test_review_dimension_description_all_variants_non_empty() {
        for d in ReviewDimension::all() {
            assert!(
                !d.description().is_empty(),
                "{:?} returned empty description",
                d
            );
        }
    }

    #[test]
    fn test_review_dimension_description_architecture_mentions_design() {
        let desc = ReviewDimension::Architecture.description();
        assert!(desc.contains("design") || desc.contains("structural"));
    }

    // ------------------------------------------------------------------
    // from_str()
    // ------------------------------------------------------------------

    #[test]
    fn test_review_dimension_from_str_lowercase_architecture_returns_some() {
        assert_eq!(
            ReviewDimension::from_str("architecture"),
            Some(ReviewDimension::Architecture)
        );
    }

    #[test]
    fn test_review_dimension_from_str_uppercase_is_case_insensitive() {
        assert_eq!(
            ReviewDimension::from_str("ERROR_HANDLING"),
            Some(ReviewDimension::ErrorHandling)
        );
    }

    #[test]
    fn test_review_dimension_from_str_all_as_str_values_round_trip() {
        for dim in ReviewDimension::all() {
            let parsed = ReviewDimension::from_str(dim.as_str());
            assert_eq!(parsed, Some(dim.clone()), "round-trip failed for {:?}", dim);
        }
    }

    #[test]
    fn test_review_dimension_from_str_unknown_returns_none() {
        assert!(ReviewDimension::from_str("unknown").is_none());
    }

    #[test]
    fn test_review_dimension_from_str_empty_returns_none() {
        assert!(ReviewDimension::from_str("").is_none());
    }

    #[test]
    fn test_review_dimension_from_str_reliability_returns_none() {
        // "reliability" is not a valid dimension identifier.
        assert!(ReviewDimension::from_str("reliability").is_none());
    }

    // ------------------------------------------------------------------
    // from_focus_areas()
    // ------------------------------------------------------------------

    #[test]
    fn test_review_dimension_from_focus_areas_empty_returns_all() {
        let empty: Vec<String> = vec![];
        assert_eq!(ReviewDimension::from_focus_areas(&empty).len(), 14);
    }

    #[test]
    fn test_review_dimension_from_focus_areas_known_strings_are_parsed() {
        let areas = vec!["architecture".to_string(), "error_handling".to_string()];
        let dims = ReviewDimension::from_focus_areas(&areas);
        assert_eq!(dims.len(), 2);
        assert!(dims.contains(&ReviewDimension::Architecture));
        assert!(dims.contains(&ReviewDimension::ErrorHandling));
    }

    #[test]
    fn test_review_dimension_from_focus_areas_unknown_strings_are_ignored() {
        let areas = vec![
            "architecture".to_string(),
            "reliability".to_string(),
            "unknown".to_string(),
        ];
        let dims = ReviewDimension::from_focus_areas(&areas);
        assert_eq!(dims.len(), 1);
        assert_eq!(dims[0], ReviewDimension::Architecture);
    }

    #[test]
    fn test_review_dimension_from_focus_areas_all_unknown_returns_empty() {
        let areas = vec!["foo".to_string(), "bar".to_string()];
        let dims = ReviewDimension::from_focus_areas(&areas);
        assert!(dims.is_empty());
    }

    #[test]
    fn test_review_dimension_from_focus_areas_preserves_input_order() {
        let areas = vec![
            "observability".to_string(),
            "architecture".to_string(),
            "error_handling".to_string(),
        ];
        let dims = ReviewDimension::from_focus_areas(&areas);
        assert_eq!(dims[0], ReviewDimension::Observability);
        assert_eq!(dims[1], ReviewDimension::Architecture);
        assert_eq!(dims[2], ReviewDimension::ErrorHandling);
    }

    // ------------------------------------------------------------------
    // Serde round-trip
    // ------------------------------------------------------------------

    #[test]
    fn test_review_dimension_serde_roundtrip_architecture() {
        let dim = ReviewDimension::Architecture;
        // SAFETY: ReviewDimension is a known-valid enum; serialization cannot fail.
        let json = serde_json::to_string(&dim).unwrap();
        // SAFETY: we just serialized this value so it is valid JSON.
        let restored: ReviewDimension = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, dim);
    }

    #[test]
    fn test_review_dimension_serde_all_variants_roundtrip() {
        for dim in ReviewDimension::all() {
            // SAFETY: ReviewDimension variants are always serializable.
            let json = serde_json::to_string(&dim).unwrap();
            // SAFETY: we just serialized this value.
            let restored: ReviewDimension = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, dim, "serde round-trip failed for {:?}", dim);
        }
    }
}
