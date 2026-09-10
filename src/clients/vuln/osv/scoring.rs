//! CVSS scoring for OSV severity entries.
//!
//! Converts raw CVSS vector strings found in [`OsvSeverityEntry`] records
//! into numeric scores and categorical [`CvssBand`] values.
//!
//! Severity-band mapping from a numeric score is delegated to the
//! [`cvss_rs`] crate via `cvss_rs::score_to_severity`, which follows the
//! NVD / FIRST CVSS v3 and v4 band boundaries.  Custom vector-string scoring
//! (`score_cvss3`, `score_cvss4`) remains implemented locally because
//! `cvss-rs` is a JSON deserializer rather than a vector-string scorer.
//!
//! # Primary-score rule
//!
//! When a record contains both CVSS v3 and CVSS v4 entries, the v3 score is
//! always used as `primary_score`.  v4 is used only when no valid v3 entry is
//! present.  A malformed or unrecognised vector string degrades that specific
//! entry to `0.0` without failing the whole call.
//!
//! # Supported vector formats
//!
//! | Prefix       | Formula           |
//! |--------------|-------------------|
//! | `CVSS:3.0/`  | CVSS v3 base-score formula (identical for 3.0 and 3.1) |
//! | `CVSS:3.1/`  | CVSS v3 base-score formula                              |
//! | `CVSS:4.0/`  | CVSS v4 EQ-level lookup table                           |

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::clients::vuln::OsvSeverityEntry;
use crate::scanner::scoring::ScoringSignal;

// ---------------------------------------------------------------------------
// CvssBand
// ---------------------------------------------------------------------------

/// Categorical CVSS severity band mapped from a numeric score.
///
/// Boundaries follow the NVD / FIRST CVSS v3 and v4 conventions:
///
/// | Band     | Score range |
/// |----------|-------------|
/// | Low      | 0.1 - 3.9   |
/// | Medium   | 4.0 - 6.9   |
/// | High     | 7.0 - 8.9   |
/// | Critical | 9.0 - 10.0  |
///
/// A score of `0.0` or below produces `None` from [`CvssBand::from_score`].
///
/// # Examples
///
/// ```
/// use xzardgz::clients::vuln::CvssBand;
///
/// assert_eq!(CvssBand::from_score(7.5), Some(CvssBand::High));
/// assert_eq!(CvssBand::from_score(9.5), Some(CvssBand::Critical));
/// assert_eq!(CvssBand::from_score(0.0), None);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CvssBand {
    /// Score in the range 0.1 - 3.9.
    Low,
    /// Score in the range 4.0 - 6.9.
    Medium,
    /// Score in the range 7.0 - 8.9.
    High,
    /// Score in the range 9.0 - 10.0.
    Critical,
}

impl fmt::Display for CvssBand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            CvssBand::Low => "low",
            CvssBand::Medium => "medium",
            CvssBand::High => "high",
            CvssBand::Critical => "critical",
        };
        f.write_str(label)
    }
}

impl CvssBand {
    /// Maps a numeric CVSS score to a [`CvssBand`].
    ///
    /// Returns `None` when `score <= 0.0`.
    ///
    /// # Arguments
    ///
    /// * `score` - A CVSS numeric score, nominally in `[0.0, 10.0]`.
    ///
    /// # Returns
    ///
    /// `Some(band)` for scores above `0.0`, or `None` for `0.0` and below.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::clients::vuln::CvssBand;
    ///
    /// assert_eq!(CvssBand::from_score(2.0),  Some(CvssBand::Low));
    /// assert_eq!(CvssBand::from_score(5.5),  Some(CvssBand::Medium));
    /// assert_eq!(CvssBand::from_score(7.5),  Some(CvssBand::High));
    /// assert_eq!(CvssBand::from_score(9.5),  Some(CvssBand::Critical));
    /// assert_eq!(CvssBand::from_score(0.0),  None);
    /// assert_eq!(CvssBand::from_score(-1.0), None);
    /// ```
    pub fn from_score(score: f64) -> Option<CvssBand> {
        match cvss_rs::score_to_severity(score) {
            None | Some(cvss_rs::Severity::None) => None,
            Some(cvss_rs::Severity::Low) => Some(CvssBand::Low),
            Some(cvss_rs::Severity::Medium) => Some(CvssBand::Medium),
            Some(cvss_rs::Severity::High) => Some(CvssBand::High),
            Some(cvss_rs::Severity::Critical) => Some(CvssBand::Critical),
        }
    }
}

// ---------------------------------------------------------------------------
// OsvScore
// ---------------------------------------------------------------------------

/// Computed CVSS scores and severity band derived from an OSV severity list.
///
/// `primary_score` is the authoritative numeric value: CVSS v3 when present,
/// CVSS v4 when v3 is absent, and `0.0` when neither is available or valid.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::vuln::{CvssBand, OsvSeverityEntry};
/// use xzardgz::clients::vuln::osv::scoring::score_severity;
///
/// let entries = vec![OsvSeverityEntry {
///     r#type: "CVSS_V3".to_string(),
///     score: "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N".to_string(),
/// }];
/// let result = score_severity(&entries);
/// assert!((result.primary_score - 8.6).abs() < 0.05);
/// assert_eq!(result.band, Some(CvssBand::High));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvScore {
    /// Computed CVSS v3.x score, or `None` when no valid `CVSS_V3` entry is
    /// present.
    pub cvss_v3_score: Option<f64>,
    /// Computed CVSS v4.0 score, or `None` when no valid `CVSS_V4` entry is
    /// present.
    pub cvss_v4_score: Option<f64>,
    /// Authoritative score: v3 when available, else v4, else `0.0`.
    pub primary_score: f64,
    /// Severity band derived from `primary_score`.
    pub band: Option<CvssBand>,
}

// ---------------------------------------------------------------------------
// score_severity
// ---------------------------------------------------------------------------

/// Computes CVSS scores from a slice of [`OsvSeverityEntry`] records.
///
/// Iterates over the entries, scoring each `CVSS_V3` or `CVSS_V4` vector.
/// Malformed vectors degrade to `0.0` for that entry without failing the call.
/// The highest valid score per type is retained.
///
/// # Arguments
///
/// * `severity` - Slice of severity entries from an OSV vulnerability record.
///
/// # Returns
///
/// An [`OsvScore`] containing per-type scores, the primary score (v3
/// preferred), and the corresponding [`CvssBand`].
///
/// # Examples
///
/// ```
/// use xzardgz::clients::vuln::{CvssBand, OsvSeverityEntry};
/// use xzardgz::clients::vuln::osv::scoring::score_severity;
///
/// let entries = vec![OsvSeverityEntry {
///     r#type: "CVSS_V3".to_string(),
///     score: "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N".to_string(),
/// }];
/// let result = score_severity(&entries);
/// assert!((result.primary_score - 8.6).abs() < 0.05);
/// assert_eq!(result.band, Some(CvssBand::High));
/// ```
pub fn score_severity(severity: &[OsvSeverityEntry]) -> OsvScore {
    let mut v3_score: Option<f64> = None;
    let mut v4_score: Option<f64> = None;

    for entry in severity {
        match entry.r#type.as_str() {
            "CVSS_V3" => {
                let s = if entry.score.starts_with("CVSS:3.0/")
                    || entry.score.starts_with("CVSS:3.1/")
                {
                    score_cvss3(&entry.score)
                } else {
                    0.0
                };
                if s > 0.0 {
                    v3_score = Some(v3_score.unwrap_or(0.0_f64).max(s));
                }
            }
            "CVSS_V4" => {
                let s = if entry.score.starts_with("CVSS:4.0/") {
                    score_cvss4(&entry.score)
                } else {
                    0.0
                };
                if s > 0.0 {
                    v4_score = Some(v4_score.unwrap_or(0.0_f64).max(s));
                }
            }
            _ => {}
        }
    }

    let primary_score = v3_score.or(v4_score).unwrap_or(0.0);
    let band = CvssBand::from_score(primary_score);

    OsvScore {
        cvss_v3_score: v3_score,
        cvss_v4_score: v4_score,
        primary_score,
        band,
    }
}

// ---------------------------------------------------------------------------
// osv_score_to_signal
// ---------------------------------------------------------------------------

/// Maps an [`OsvScore`]'s band to a [`ScoringSignal::Negative`].
///
/// Returns `None` when `score.band` is `None` (i.e. `primary_score` is
/// `0.0`).
///
/// # Band to weight mapping
///
/// | Band     | Weight |
/// |----------|--------|
/// | Low      | 0.10   |
/// | Medium   | 0.25   |
/// | High     | 0.45   |
/// | Critical | 0.65   |
///
/// # Arguments
///
/// * `score` - A computed [`OsvScore`] from [`score_severity`].
///
/// # Returns
///
/// `Some(ScoringSignal::Negative { label, weight })` when a band is present,
/// or `None` when `primary_score` is `0.0`.
pub fn osv_score_to_signal(score: &OsvScore) -> Option<ScoringSignal> {
    let band = score.band.as_ref()?;
    let (label, weight): (&str, f64) = match band {
        CvssBand::Low => ("osv-vulnerability-low", 0.10),
        CvssBand::Medium => ("osv-vulnerability-medium", 0.25),
        CvssBand::High => ("osv-vulnerability-high", 0.45),
        CvssBand::Critical => ("osv-vulnerability-critical", 0.65),
    };
    Some(ScoringSignal::Negative {
        label: label.to_string(),
        weight,
    })
}

// ---------------------------------------------------------------------------
// CVSS v3.x scorer (private)
// ---------------------------------------------------------------------------

/// Computes a CVSS 3.0 / 3.1 base score from a vector string.
///
/// Uses the same formula for both 3.0 and 3.1.  Returns `0.0` for any
/// unrecognised or malformed metric value.
fn score_cvss3(vector: &str) -> f64 {
    let metrics: HashMap<&str, &str> = vector
        .split('/')
        .skip(1) // skip "CVSS:3.x"
        .filter_map(|s| s.split_once(':'))
        .collect();

    let av = match metrics.get("AV").copied().unwrap_or("") {
        "N" => 0.85_f64,
        "A" => 0.62,
        "L" => 0.55,
        "P" => 0.20,
        _ => return 0.0,
    };

    let ac = match metrics.get("AC").copied().unwrap_or("") {
        "L" => 0.77_f64,
        "H" => 0.44,
        _ => return 0.0,
    };

    let scope = metrics.get("S").copied().unwrap_or("");

    let pr = match (scope, metrics.get("PR").copied().unwrap_or("")) {
        ("U", "N") => 0.85_f64,
        ("U", "L") => 0.62,
        ("U", "H") => 0.27,
        ("C", "N") => 0.85,
        ("C", "L") => 0.50,
        ("C", "H") => 0.50,
        _ => return 0.0,
    };

    let ui = match metrics.get("UI").copied().unwrap_or("") {
        "N" => 0.85_f64,
        "R" => 0.62,
        _ => return 0.0,
    };

    let c_val = match metrics.get("C").copied().unwrap_or("") {
        "N" => 0.00_f64,
        "L" => 0.22,
        "H" => 0.56,
        _ => return 0.0,
    };
    let i_val = match metrics.get("I").copied().unwrap_or("") {
        "N" => 0.00_f64,
        "L" => 0.22,
        "H" => 0.56,
        _ => return 0.0,
    };
    let a_val = match metrics.get("A").copied().unwrap_or("") {
        "N" => 0.00_f64,
        "L" => 0.22,
        "H" => 0.56,
        _ => return 0.0,
    };

    // Impact sub-score
    let isc = 1.0 - (1.0 - c_val) * (1.0 - i_val) * (1.0 - a_val);

    let iss = if scope == "U" {
        6.42 * isc
    } else {
        // Scope == Changed
        7.52 * (isc - 0.029) - 3.25 * (isc - 0.02_f64).powi(15)
    };

    if iss <= 0.0 {
        return 0.0;
    }

    // Exploitability sub-score
    let ess = 8.22 * av * ac * pr * ui;

    let raw = if scope == "U" {
        (iss + ess).min(10.0)
    } else {
        (1.08 * (iss + ess)).min(10.0)
    };

    roundup(raw)
}

// ---------------------------------------------------------------------------
// CVSS v4.0 scorer (private)
// ---------------------------------------------------------------------------

/// Computes a CVSS 4.0 score from a vector string using EQ-level lookups.
///
/// Returns `0.0` for any unrecognised or malformed metric value.
fn score_cvss4(vector: &str) -> f64 {
    let metrics: HashMap<&str, &str> = vector
        .split('/')
        .skip(1) // skip "CVSS:4.0"
        .filter_map(|s| s.split_once(':'))
        .collect();

    // EQ1: Attack Vector  (N=0 most dangerous, P=3 least)
    let eq1: usize = match metrics.get("AV").copied().unwrap_or("") {
        "N" => 0,
        "A" => 1,
        "L" => 2,
        "P" => 3,
        _ => return 0.0,
    };

    let ac = metrics.get("AC").copied().unwrap_or("");
    let at = metrics.get("AT").copied().unwrap_or("");
    let pr = metrics.get("PR").copied().unwrap_or("");
    let ui = metrics.get("UI").copied().unwrap_or("");

    if !matches!(ac, "L" | "H") {
        return 0.0;
    }
    if !matches!(at, "N" | "P") {
        return 0.0;
    }
    if !matches!(pr, "N" | "L" | "H") {
        return 0.0;
    }
    if !matches!(ui, "N" | "P" | "A") {
        return 0.0;
    }

    // EQ2: AC=L AND AT=N → 0, else 1
    let eq2: usize = if ac == "L" && at == "N" { 0 } else { 1 };

    // EQ3: (PR=N AND UI=N) → 0, exactly one N → 1, neither N → 2
    let eq3: usize = if pr == "N" && ui == "N" {
        0
    } else if pr == "N" || ui == "N" {
        1
    } else {
        2
    };

    let vc = metrics.get("VC").copied().unwrap_or("");
    let vi = metrics.get("VI").copied().unwrap_or("");
    let va = metrics.get("VA").copied().unwrap_or("");
    let sc = metrics.get("SC").copied().unwrap_or("");
    let si = metrics.get("SI").copied().unwrap_or("");
    let sa = metrics.get("SA").copied().unwrap_or("");

    if !matches!(vc, "N" | "L" | "H") {
        return 0.0;
    }
    if !matches!(vi, "N" | "L" | "H") {
        return 0.0;
    }
    if !matches!(va, "N" | "L" | "H") {
        return 0.0;
    }
    if !matches!(sc, "N" | "L" | "H") {
        return 0.0;
    }
    if !matches!(si, "N" | "L" | "H") {
        return 0.0;
    }
    if !matches!(sa, "N" | "L" | "H") {
        return 0.0;
    }

    // EQ4: Vulnerable component impact (any H → 0, any L no H → 1, all N → 2)
    let eq4: usize = if vc == "H" || vi == "H" || va == "H" {
        0
    } else if vc == "L" || vi == "L" || va == "L" {
        1
    } else {
        2
    };

    // EQ5: Subsequent system impact (any H → 0, any L no H → 1, all N → 2)
    let eq5: usize = if sc == "H" || si == "H" || sa == "H" {
        0
    } else if sc == "L" || si == "L" || sa == "L" {
        1
    } else {
        2
    };

    const EQ1_W: [f64; 4] = [0.0, 1.5, 2.5, 4.0];
    const EQ2_W: [f64; 2] = [0.0, 1.0];
    const EQ3_W: [f64; 3] = [0.0, 0.5, 1.0];
    const EQ4_W: [f64; 3] = [0.0, 2.0, 4.0];
    const EQ5_W: [f64; 3] = [0.0, 2.0, 4.0];

    let base = 10.0 - EQ1_W[eq1] - EQ2_W[eq2] - EQ3_W[eq3];
    let impact = 10.0 - (EQ4_W[eq4] + EQ5_W[eq5]).min(8.0);

    let raw = (base + impact) / 2.0;
    roundup(raw.clamp(0.0, 10.0))
}

// ---------------------------------------------------------------------------
// roundup helper (private)
// ---------------------------------------------------------------------------

/// Rounds a CVSS score up to the nearest 0.1 (ceiling to one decimal place).
///
/// The CVSS specification requires ceiling rounding, not standard rounding.
#[inline]
fn roundup(x: f64) -> f64 {
    (x * 10.0).ceil() / 10.0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clients::vuln::OsvSeverityEntry;
    use crate::scanner::scoring::ScoringSignal;

    // Helper: build a single-entry severity slice.
    fn v3_entry(score: &str) -> Vec<OsvSeverityEntry> {
        vec![OsvSeverityEntry {
            r#type: "CVSS_V3".to_string(),
            score: score.to_string(),
        }]
    }

    fn v4_entry(score: &str) -> Vec<OsvSeverityEntry> {
        vec![OsvSeverityEntry {
            r#type: "CVSS_V4".to_string(),
            score: score.to_string(),
        }]
    }

    // ------------------------------------------------------------------
    // CvssBand::from_score
    // ------------------------------------------------------------------

    #[test]
    fn test_cvss_band_from_score_with_zero_returns_none() {
        assert_eq!(CvssBand::from_score(0.0), None);
    }

    #[test]
    fn test_cvss_band_from_score_with_negative_returns_none() {
        assert_eq!(CvssBand::from_score(-1.0), None);
    }

    #[test]
    fn test_cvss_band_from_score_with_low_range_returns_low() {
        assert_eq!(CvssBand::from_score(2.0), Some(CvssBand::Low));
    }

    #[test]
    fn test_cvss_band_from_score_with_medium_range_returns_medium() {
        assert_eq!(CvssBand::from_score(5.5), Some(CvssBand::Medium));
    }

    #[test]
    fn test_cvss_band_from_score_with_high_range_returns_high() {
        assert_eq!(CvssBand::from_score(7.5), Some(CvssBand::High));
    }

    #[test]
    fn test_cvss_band_from_score_with_critical_range_returns_critical() {
        assert_eq!(CvssBand::from_score(9.5), Some(CvssBand::Critical));
    }

    // Boundary edges
    #[test]
    fn test_cvss_band_from_score_at_low_upper_boundary_returns_low() {
        assert_eq!(CvssBand::from_score(3.9), Some(CvssBand::Low));
    }

    #[test]
    fn test_cvss_band_from_score_at_medium_lower_boundary_returns_medium() {
        assert_eq!(CvssBand::from_score(4.0), Some(CvssBand::Medium));
    }

    #[test]
    fn test_cvss_band_from_score_at_high_lower_boundary_returns_high() {
        assert_eq!(CvssBand::from_score(7.0), Some(CvssBand::High));
    }

    #[test]
    fn test_cvss_band_from_score_at_critical_lower_boundary_returns_critical() {
        assert_eq!(CvssBand::from_score(9.0), Some(CvssBand::Critical));
    }

    // ------------------------------------------------------------------
    // CvssBand Display
    // ------------------------------------------------------------------

    #[test]
    fn test_cvss_band_display_low_returns_lowercase_string() {
        assert_eq!(CvssBand::Low.to_string(), "low");
    }

    #[test]
    fn test_cvss_band_display_critical_returns_lowercase_string() {
        assert_eq!(CvssBand::Critical.to_string(), "critical");
    }

    // ------------------------------------------------------------------
    // score_severity: empty
    // ------------------------------------------------------------------

    #[test]
    fn test_score_severity_with_empty_slice_returns_zero() {
        let result = score_severity(&[]);
        assert_eq!(result.primary_score, 0.0);
        assert_eq!(result.band, None);
        assert!(result.cvss_v3_score.is_none());
        assert!(result.cvss_v4_score.is_none());
    }

    #[test]
    fn test_score_pysec_entry_with_no_severity_returns_zero() {
        // Simulates a PYSEC record that has no severity array at all.
        let result = score_severity(&[]);
        assert_eq!(result.primary_score, 0.0);
        assert_eq!(result.band, None);
    }

    // ------------------------------------------------------------------
    // score_severity: CVSS v3 only
    // ------------------------------------------------------------------

    #[test]
    fn test_score_severity_with_only_cvss_v3_uses_v3() {
        let entries = v3_entry("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N");
        let result = score_severity(&entries);
        assert!(result.cvss_v3_score.is_some());
        assert!(result.cvss_v4_score.is_none());
        assert!((result.primary_score - 8.6).abs() < 0.05);
        assert_eq!(result.band, Some(CvssBand::High));
    }

    #[test]
    fn test_score_cvss3_vector_gives_expected_score() {
        // Hand-computed: CVSS:3.0/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N → 8.6
        let entries = vec![OsvSeverityEntry {
            r#type: "CVSS_V3".to_string(),
            score: "CVSS:3.0/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N".to_string(),
        }];
        let result = score_severity(&entries);
        assert!(
            (result.primary_score - 8.6).abs() < 0.05,
            "expected ~8.6, got {}",
            result.primary_score
        );
        assert_eq!(result.band, Some(CvssBand::High));
    }

    // ------------------------------------------------------------------
    // score_severity: CVSS v4 only
    // ------------------------------------------------------------------

    #[test]
    fn test_score_severity_with_only_cvss_v4_uses_v4() {
        let entries = v4_entry("CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:N/VI:N/VA:N/SC:H/SI:N/SA:N");
        let result = score_severity(&entries);
        assert!(result.cvss_v3_score.is_none());
        assert!(result.cvss_v4_score.is_some());
        assert!((result.primary_score - 8.0).abs() < 0.05);
        assert_eq!(result.band, Some(CvssBand::High));
    }

    #[test]
    fn test_score_cvss4_vector_with_subsequent_impact_only_gives_high_band() {
        // eq1=0, eq2=0, eq3=0 → base=10; eq4=2 (all N), eq5=0 (SC=H) → impact=6; score=8.0
        let entries = vec![OsvSeverityEntry {
            r#type: "CVSS_V4".to_string(),
            score: "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:N/VI:N/VA:N/SC:H/SI:N/SA:N".to_string(),
        }];
        let result = score_severity(&entries);
        assert!(
            (result.primary_score - 8.0).abs() < 0.05,
            "expected ~8.0, got {}",
            result.primary_score
        );
        assert_eq!(result.band, Some(CvssBand::High));
    }

    // ------------------------------------------------------------------
    // score_severity: v3 preferred over v4
    // ------------------------------------------------------------------

    #[test]
    fn test_score_severity_with_both_cvss_v3_and_v4_prefers_v3() {
        let entries = vec![
            OsvSeverityEntry {
                r#type: "CVSS_V3".to_string(),
                score: "CVSS:3.0/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N".to_string(),
            },
            OsvSeverityEntry {
                r#type: "CVSS_V4".to_string(),
                score: "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:N/VI:N/VA:N/SC:H/SI:N/SA:N"
                    .to_string(),
            },
        ];
        let result = score_severity(&entries);
        assert!(result.cvss_v3_score.is_some());
        assert!(result.cvss_v4_score.is_some());
        // v3 is preferred as primary
        let v3 = result.cvss_v3_score.unwrap();
        assert!(
            (result.primary_score - v3).abs() < 1e-10,
            "primary_score should equal v3 score"
        );
        assert_eq!(result.band, Some(CvssBand::High));
    }

    #[test]
    fn test_both_cvss_versions_primary_is_v3_v4_retained() {
        // GHSA-462w-v97r-4m45 has both CVSS_V3 and CVSS_V4
        let entries = vec![
            OsvSeverityEntry {
                r#type: "CVSS_V3".to_string(),
                score: "CVSS:3.0/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N".to_string(),
            },
            OsvSeverityEntry {
                r#type: "CVSS_V4".to_string(),
                score: "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:N/VI:N/VA:N/SC:H/SI:N/SA:N"
                    .to_string(),
            },
        ];
        let result = score_severity(&entries);
        assert!(result.cvss_v3_score.is_some(), "v3 score should be present");
        assert!(result.cvss_v4_score.is_some(), "v4 score should be present");
        assert_eq!(
            result.primary_score,
            result.cvss_v3_score.unwrap(),
            "v3 score should be primary"
        );
    }

    // ------------------------------------------------------------------
    // score_severity: malformed vector degrades gracefully
    // ------------------------------------------------------------------

    #[test]
    fn test_score_severity_with_malformed_vector_returns_zero_for_that_entry() {
        // One valid entry and one malformed entry; the valid one should win.
        let entries = vec![
            OsvSeverityEntry {
                r#type: "CVSS_V3".to_string(),
                score: "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N".to_string(),
            },
            OsvSeverityEntry {
                r#type: "CVSS_V3".to_string(),
                score: "INVALID_VECTOR_STRING".to_string(),
            },
        ];
        let result = score_severity(&entries);
        assert!(
            (result.primary_score - 8.6).abs() < 0.05,
            "valid entry should win; expected ~8.6, got {}",
            result.primary_score
        );
        assert_eq!(result.band, Some(CvssBand::High));
    }

    #[test]
    fn test_score_empty_severity_returns_zero_with_no_band() {
        let result = score_severity(&[]);
        assert_eq!(result.primary_score, 0.0);
        assert_eq!(result.band, None);
    }

    // ------------------------------------------------------------------
    // osv_score_to_signal
    // ------------------------------------------------------------------

    #[test]
    fn test_osv_score_to_signal_with_no_band_returns_none() {
        let score = OsvScore {
            cvss_v3_score: None,
            cvss_v4_score: None,
            primary_score: 0.0,
            band: None,
        };
        assert!(osv_score_to_signal(&score).is_none());
    }

    #[test]
    fn test_osv_score_to_signal_with_low_band_returns_negative_signal() {
        let score = OsvScore {
            cvss_v3_score: Some(2.0),
            cvss_v4_score: None,
            primary_score: 2.0,
            band: Some(CvssBand::Low),
        };
        let signal = osv_score_to_signal(&score);
        assert!(signal.is_some());
        if let ScoringSignal::Negative { label, weight } = signal.unwrap() {
            assert_eq!(label, "osv-vulnerability-low");
            assert!((weight - 0.10).abs() < 1e-10);
        } else {
            panic!("expected ScoringSignal::Negative");
        }
    }

    #[test]
    fn test_osv_score_to_signal_with_high_band_returns_negative_signal() {
        let score = OsvScore {
            cvss_v3_score: Some(7.5),
            cvss_v4_score: None,
            primary_score: 7.5,
            band: Some(CvssBand::High),
        };
        let signal = osv_score_to_signal(&score);
        assert!(signal.is_some());
        if let ScoringSignal::Negative { label, weight } = signal.unwrap() {
            assert_eq!(label, "osv-vulnerability-high");
            assert!((weight - 0.45).abs() < 1e-10);
        } else {
            panic!("expected ScoringSignal::Negative");
        }
    }

    #[test]
    fn test_osv_score_to_signal_with_critical_band_returns_negative_signal() {
        let score = OsvScore {
            cvss_v3_score: Some(9.5),
            cvss_v4_score: None,
            primary_score: 9.5,
            band: Some(CvssBand::Critical),
        };
        let signal = osv_score_to_signal(&score);
        assert!(signal.is_some());
        if let ScoringSignal::Negative { label, weight } = signal.unwrap() {
            assert_eq!(label, "osv-vulnerability-critical");
            assert!((weight - 0.65).abs() < 1e-10);
        } else {
            panic!("expected ScoringSignal::Negative");
        }
    }

    // ------------------------------------------------------------------
    // CVSS v4 second hand-computed case
    // ------------------------------------------------------------------

    #[test]
    fn test_score_cvss4_medium_vector_gives_expected_score() {
        // AV:L/AC:L/AT:P/PR:L/UI:P/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N
        // eq1=2,eq2=1,eq3=2 → base=5.5; eq4=0,eq5=2 → impact=6; score=5.8
        let entries = vec![OsvSeverityEntry {
            r#type: "CVSS_V4".to_string(),
            score: "CVSS:4.0/AV:L/AC:L/AT:P/PR:L/UI:P/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N".to_string(),
        }];
        let result = score_severity(&entries);
        assert!(
            (result.primary_score - 5.8).abs() < 0.05,
            "expected ~5.8, got {}",
            result.primary_score
        );
        assert_eq!(result.band, Some(CvssBand::Medium));
    }
}
