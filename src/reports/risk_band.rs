//! Risk band classification for plugin report findings.
//!
//! [`RiskBand`] is a four-tier enum that represents the overall risk exposure
//! derived from a set of findings. It can be obtained programmatically from an
//! AI confidence score via [`RiskBand::from_confidence`] or mapped from the
//! highest [`crate::scanner::findings::FindingSeverity`] across all findings.

use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// RiskBand
// ---------------------------------------------------------------------------

/// Overall risk classification for a plugin report.
///
/// Variants are declared in ascending order so that the derived [`Ord`]
/// implementation produces `Low < Medium < High < Critical`.
///
/// # Examples
///
/// ```
/// use xzardgz::reports::risk_band::RiskBand;
///
/// assert!(RiskBand::Low < RiskBand::Critical);
/// assert_eq!(RiskBand::High.as_str(), "high");
/// assert_eq!(RiskBand::Critical.label(), "CRITICAL");
/// assert_eq!(RiskBand::from_confidence(0.9), RiskBand::Critical);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskBand {
    /// Low risk: informational or minor issues only.
    Low,
    /// Moderate risk: issues that warrant review.
    Medium,
    /// High risk: issues requiring prompt attention.
    High,
    /// Critical risk: immediately actionable issues.
    Critical,
}

impl RiskBand {
    /// Returns the lowercase string representation of the risk band.
    ///
    /// # Returns
    ///
    /// One of `"low"`, `"medium"`, `"high"`, `"critical"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::risk_band::RiskBand;
    ///
    /// assert_eq!(RiskBand::Medium.as_str(), "medium");
    /// assert_eq!(RiskBand::High.as_str(), "high");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    /// Returns the uppercase label of the risk band.
    ///
    /// # Returns
    ///
    /// One of `"LOW"`, `"MEDIUM"`, `"HIGH"`, `"CRITICAL"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::risk_band::RiskBand;
    ///
    /// assert_eq!(RiskBand::Low.label(), "LOW");
    /// assert_eq!(RiskBand::Critical.label(), "CRITICAL");
    /// ```
    pub fn label(&self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }

    /// Derives a `RiskBand` from a confidence score in the range `[0.0, 1.0]`.
    ///
    /// # Arguments
    ///
    /// * `score` - AI or scanner confidence in the range `[0.0, 1.0]`.
    ///
    /// # Returns
    ///
    /// - `score < 0.25` => [`RiskBand::Low`]
    /// - `score < 0.50` => [`RiskBand::Medium`]
    /// - `score < 0.75` => [`RiskBand::High`]
    /// - `score >= 0.75` => [`RiskBand::Critical`]
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::reports::risk_band::RiskBand;
    ///
    /// assert_eq!(RiskBand::from_confidence(0.1),  RiskBand::Low);
    /// assert_eq!(RiskBand::from_confidence(0.25), RiskBand::Medium);
    /// assert_eq!(RiskBand::from_confidence(0.50), RiskBand::High);
    /// assert_eq!(RiskBand::from_confidence(0.75), RiskBand::Critical);
    /// assert_eq!(RiskBand::from_confidence(1.0),  RiskBand::Critical);
    /// ```
    pub fn from_confidence(score: f64) -> RiskBand {
        if score < 0.25 {
            RiskBand::Low
        } else if score < 0.50 {
            RiskBand::Medium
        } else if score < 0.75 {
            RiskBand::High
        } else {
            RiskBand::Critical
        }
    }
}

impl fmt::Display for RiskBand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // from_confidence threshold tests
    // ------------------------------------------------------------------

    #[test]
    fn test_risk_band_from_confidence_zero_returns_low() {
        assert_eq!(RiskBand::from_confidence(0.0), RiskBand::Low);
    }

    #[test]
    fn test_risk_band_from_confidence_below_0_25_returns_low() {
        assert_eq!(RiskBand::from_confidence(0.10), RiskBand::Low);
        assert_eq!(RiskBand::from_confidence(0.24), RiskBand::Low);
    }

    #[test]
    fn test_risk_band_from_confidence_at_0_25_returns_medium() {
        assert_eq!(RiskBand::from_confidence(0.25), RiskBand::Medium);
    }

    #[test]
    fn test_risk_band_from_confidence_between_0_25_and_0_50_returns_medium() {
        assert_eq!(RiskBand::from_confidence(0.30), RiskBand::Medium);
        assert_eq!(RiskBand::from_confidence(0.49), RiskBand::Medium);
    }

    #[test]
    fn test_risk_band_from_confidence_at_0_50_returns_high() {
        assert_eq!(RiskBand::from_confidence(0.50), RiskBand::High);
    }

    #[test]
    fn test_risk_band_from_confidence_between_0_50_and_0_75_returns_high() {
        assert_eq!(RiskBand::from_confidence(0.60), RiskBand::High);
        assert_eq!(RiskBand::from_confidence(0.74), RiskBand::High);
    }

    #[test]
    fn test_risk_band_from_confidence_at_0_75_returns_critical() {
        assert_eq!(RiskBand::from_confidence(0.75), RiskBand::Critical);
    }

    #[test]
    fn test_risk_band_from_confidence_above_0_75_returns_critical() {
        assert_eq!(RiskBand::from_confidence(0.80), RiskBand::Critical);
        assert_eq!(RiskBand::from_confidence(1.0), RiskBand::Critical);
    }

    // ------------------------------------------------------------------
    // Ordering tests
    // ------------------------------------------------------------------

    #[test]
    fn test_risk_band_ordering_low_lt_medium() {
        assert!(RiskBand::Low < RiskBand::Medium);
    }

    #[test]
    fn test_risk_band_ordering_medium_lt_high() {
        assert!(RiskBand::Medium < RiskBand::High);
    }

    #[test]
    fn test_risk_band_ordering_high_lt_critical() {
        assert!(RiskBand::High < RiskBand::Critical);
    }

    #[test]
    fn test_risk_band_ordering_full_chain_low_lt_critical() {
        assert!(RiskBand::Low < RiskBand::Critical);
        assert!(RiskBand::Medium < RiskBand::Critical);
    }

    // ------------------------------------------------------------------
    // as_str tests
    // ------------------------------------------------------------------

    #[test]
    fn test_risk_band_as_str_low() {
        assert_eq!(RiskBand::Low.as_str(), "low");
    }

    #[test]
    fn test_risk_band_as_str_medium() {
        assert_eq!(RiskBand::Medium.as_str(), "medium");
    }

    #[test]
    fn test_risk_band_as_str_high() {
        assert_eq!(RiskBand::High.as_str(), "high");
    }

    #[test]
    fn test_risk_band_as_str_critical() {
        assert_eq!(RiskBand::Critical.as_str(), "critical");
    }

    // ------------------------------------------------------------------
    // label tests
    // ------------------------------------------------------------------

    #[test]
    fn test_risk_band_label_low() {
        assert_eq!(RiskBand::Low.label(), "LOW");
    }

    #[test]
    fn test_risk_band_label_medium() {
        assert_eq!(RiskBand::Medium.label(), "MEDIUM");
    }

    #[test]
    fn test_risk_band_label_high() {
        assert_eq!(RiskBand::High.label(), "HIGH");
    }

    #[test]
    fn test_risk_band_label_critical() {
        assert_eq!(RiskBand::Critical.label(), "CRITICAL");
    }

    // ------------------------------------------------------------------
    // Display tests
    // ------------------------------------------------------------------

    #[test]
    fn test_risk_band_display_matches_as_str() {
        assert_eq!(RiskBand::Low.to_string(), "low");
        assert_eq!(RiskBand::Medium.to_string(), "medium");
        assert_eq!(RiskBand::High.to_string(), "high");
        assert_eq!(RiskBand::Critical.to_string(), "critical");
    }

    // ------------------------------------------------------------------
    // Serde round-trip
    // ------------------------------------------------------------------

    #[test]
    fn test_risk_band_serde_roundtrip() {
        let original = RiskBand::Critical;
        // SAFETY: RiskBand is a known-valid enum; serialization cannot fail.
        let json = serde_json::to_string(&original).unwrap();
        // SAFETY: we serialized the string ourselves so it is valid JSON.
        let restored: RiskBand = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, original);
    }
}
