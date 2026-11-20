//! Licence classification for ruleset sources.
//!
//! Provides a lightweight heuristic classifier that maps SPDX identifiers or
//! the text of a `LICENSE` / `COPYING` file into a broad [`LicenseClass`].
//! The default policy built into [`LicenseClass::is_allowed_by_default_policy`]
//! and the free-function [`default_policy_allows`] permits `Permissive` and
//! `WeakCopyleft` classes and blocks everything else.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// LicenseClass
// ---------------------------------------------------------------------------

/// Broad classification of a software licence for policy enforcement.
///
/// The ordering from most to least permissive is:
/// `Permissive` > `WeakCopyleft` > `StrongCopyleft` > `NetworkCopyleft`
/// > `NonCommercial` > `Restricted` > `Unknown`.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::rulesets::license::LicenseClass;
///
/// assert!(LicenseClass::Permissive.is_allowed_by_default_policy());
/// assert!(LicenseClass::WeakCopyleft.is_allowed_by_default_policy());
/// assert!(!LicenseClass::StrongCopyleft.is_allowed_by_default_policy());
/// assert!(!LicenseClass::Unknown.is_allowed_by_default_policy());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LicenseClass {
    /// MIT, Apache-2.0, BSD-*, ISC, 0BSD, and similar open licences.
    Permissive,
    /// LGPL-*, MPL-2.0, EPL, CDDL, and similar file-level copyleft licences.
    WeakCopyleft,
    /// GPL-2.0, GPL-3.0, and similar project-level copyleft licences.
    StrongCopyleft,
    /// AGPL-3.0 and similar network-use copyleft licences.
    NetworkCopyleft,
    /// CC-BY-NC and similar licences that prohibit commercial use.
    NonCommercial,
    /// Proprietary, "Commons Clause", "All Rights Reserved", SSPL.
    Restricted,
    /// The licence could not be determined from available information.
    Unknown,
}

impl LicenseClass {
    /// Returns `true` when the **default policy** allows this licence class.
    ///
    /// Default policy: allow `Permissive` and `WeakCopyleft`; deny everything
    /// else, including `Unknown` (to prevent use of unreviewed licences).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::clients::rulesets::license::LicenseClass;
    ///
    /// assert!(LicenseClass::Permissive.is_allowed_by_default_policy());
    /// assert!(LicenseClass::WeakCopyleft.is_allowed_by_default_policy());
    /// assert!(!LicenseClass::StrongCopyleft.is_allowed_by_default_policy());
    /// assert!(!LicenseClass::NetworkCopyleft.is_allowed_by_default_policy());
    /// assert!(!LicenseClass::NonCommercial.is_allowed_by_default_policy());
    /// assert!(!LicenseClass::Restricted.is_allowed_by_default_policy());
    /// assert!(!LicenseClass::Unknown.is_allowed_by_default_policy());
    /// ```
    pub fn is_allowed_by_default_policy(&self) -> bool {
        matches!(self, LicenseClass::Permissive | LicenseClass::WeakCopyleft)
    }

    /// Returns a human-readable policy decision string for this class.
    ///
    /// Returns `"allowed"` when [`is_allowed_by_default_policy`] is `true`,
    /// otherwise returns `"denied"`.
    ///
    /// [`is_allowed_by_default_policy`]: LicenseClass::is_allowed_by_default_policy
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::clients::rulesets::license::LicenseClass;
    ///
    /// assert_eq!(LicenseClass::Permissive.policy_verdict(), "allowed");
    /// assert_eq!(LicenseClass::StrongCopyleft.policy_verdict(), "denied");
    /// assert_eq!(LicenseClass::Unknown.policy_verdict(), "denied");
    /// ```
    pub fn policy_verdict(&self) -> &'static str {
        if self.is_allowed_by_default_policy() {
            "allowed"
        } else {
            "denied"
        }
    }
}

// ---------------------------------------------------------------------------
// classify_license
// ---------------------------------------------------------------------------

/// Classify a licence from its SPDX identifier or free-text licence string.
///
/// The comparison is case-insensitive.  Classification uses substring matching
/// with the following precedence (checked top-to-bottom; first match wins):
///
/// 1. `Restricted`:      "commons clause", "proprietary", "all rights reserved"
/// 2. `NonCommercial`:   "noncommercial", "non-commercial", "nc"
/// 3. `NetworkCopyleft`: "agpl", "affero"
/// 4. `StrongCopyleft`:  "gpl" (but NOT "lgpl")
/// 5. `WeakCopyleft`:    "lgpl", "mpl", "mozilla", "epl", "eclipse", "cddl"
/// 6. `Permissive`:    "mit", "apache", "bsd", "isc", "zlib", "boost",
///    "0bsd", "unlicense", "cc0"
/// 7. `Unknown`          (fallback)
///
/// # Arguments
///
/// * `text` - An SPDX licence identifier or free-text string such as the
///   content of a `LICENSE` file.
///
/// # Returns
///
/// The [`LicenseClass`] that best describes the licence.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::rulesets::license::{LicenseClass, classify_license};
///
/// assert_eq!(classify_license("MIT"), LicenseClass::Permissive);
/// assert_eq!(classify_license("GPL-3.0"), LicenseClass::StrongCopyleft);
/// assert_eq!(classify_license("AGPL-3.0"), LicenseClass::NetworkCopyleft);
/// assert_eq!(classify_license("UNKNOWN-9.9"), LicenseClass::Unknown);
/// ```
pub fn classify_license(text: &str) -> LicenseClass {
    let lower = text.to_lowercase();
    let lower = lower.trim();

    // 1. Restricted: must be checked before "nc" to avoid "commons clause"
    //    matching a later pattern.
    if lower.contains("commons clause")
        || lower.contains("proprietary")
        || lower.contains("all rights reserved")
    {
        return LicenseClass::Restricted;
    }

    // 2. NonCommercial: "nc" is a common substring in SPDX (CC-BY-NC-*).
    if lower.contains("noncommercial")
        || lower.contains("non-commercial")
        || lower.contains("-nc-")
        || lower.ends_with("-nc")
    {
        return LicenseClass::NonCommercial;
    }

    // 3. NetworkCopyleft: must precede StrongCopyleft because AGPL text also
    //    contains "gpl".
    if lower.contains("agpl") || lower.contains("affero") {
        return LicenseClass::NetworkCopyleft;
    }

    // 4. StrongCopyleft: match "gpl" but reject "lgpl".
    if lower.contains("gpl") && !lower.contains("lgpl") {
        return LicenseClass::StrongCopyleft;
    }

    // 5. WeakCopyleft.
    if lower.contains("lgpl")
        || lower.contains("mpl")
        || lower.contains("mozilla")
        || lower.contains("epl")
        || lower.contains("eclipse")
        || lower.contains("cddl")
    {
        return LicenseClass::WeakCopyleft;
    }

    // 6. Permissive.
    if lower.contains("mit")
        || lower.contains("apache")
        || lower.contains("bsd")
        || lower.contains("isc")
        || lower.contains("zlib")
        || lower.contains("boost")
        || lower.contains("0bsd")
        || lower.contains("unlicense")
        || lower.contains("cc0")
    {
        return LicenseClass::Permissive;
    }

    // 7. Fallback.
    LicenseClass::Unknown
}

// ---------------------------------------------------------------------------
// classify_license_file
// ---------------------------------------------------------------------------

/// Classify a licence from the raw text of a `LICENSE` or `COPYING` file.
///
/// Uses keyword heuristics.  For precise SPDX matching, provide the identifier
/// via [`classify_license`] instead.
///
/// # Arguments
///
/// * `content` - The full text of a licence file.
///
/// # Returns
///
/// The [`LicenseClass`] inferred from the file content.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::rulesets::license::{LicenseClass, classify_license_file};
///
/// let mit_text = "Permission is hereby granted, free of charge";
/// assert_eq!(classify_license_file(mit_text), LicenseClass::Permissive);
/// ```
pub fn classify_license_file(content: &str) -> LicenseClass {
    let upper = content.to_uppercase();

    // Network copyleft must be tested before strong copyleft because AGPL
    // text also contains "GNU GENERAL PUBLIC LICENSE".
    if upper.contains("GNU AFFERO GENERAL PUBLIC LICENSE")
        || upper.contains("AFFERO GENERAL PUBLIC")
        || upper.contains("SSPL")
        || upper.contains("SERVER SIDE PUBLIC LICENSE")
    {
        return LicenseClass::NetworkCopyleft;
    }

    if upper.contains("GNU GENERAL PUBLIC LICENSE")
        || upper.contains("GENERAL PUBLIC LICENSE")
        || upper.contains("GPL")
    {
        if upper.contains("LESSER") || upper.contains("LIBRARY") {
            return LicenseClass::WeakCopyleft;
        }
        return LicenseClass::StrongCopyleft;
    }

    if upper.contains("MOZILLA PUBLIC LICENSE") || upper.contains("MPL") {
        return LicenseClass::WeakCopyleft;
    }

    if upper.contains("LESSER GENERAL PUBLIC LICENSE") || upper.contains("LGPL") {
        return LicenseClass::WeakCopyleft;
    }

    if upper.contains("EUPL") || upper.contains("EUROPEAN UNION PUBLIC LICENCE") {
        return LicenseClass::WeakCopyleft;
    }

    if upper.contains("NONCOMMERCIAL") || upper.contains("NON-COMMERCIAL") {
        return LicenseClass::NonCommercial;
    }

    if upper.contains("ALL RIGHTS RESERVED") || upper.contains("PROPRIETARY") {
        return LicenseClass::Restricted;
    }

    // Permissive heuristics -- checked last because they use common words.
    if upper.contains("MIT LICENSE")
        || upper.contains("PERMISSION IS HEREBY GRANTED")
        || upper.contains("APACHE LICENSE")
        || upper.contains("APACHE SOFTWARE LICENSE")
        || upper.contains("BSD LICENSE")
        || upper.contains("BSD 2-CLAUSE")
        || upper.contains("BSD 3-CLAUSE")
        || upper.contains("ISC LICENSE")
        || upper.contains("BOOST SOFTWARE LICENSE")
        || upper.contains("PUBLIC DOMAIN")
        || upper.contains("CREATIVE COMMONS ZERO")
        || upper.contains("CC0")
    {
        return LicenseClass::Permissive;
    }

    LicenseClass::Unknown
}

// ---------------------------------------------------------------------------
// default_policy_allows (free function, kept for backward compatibility)
// ---------------------------------------------------------------------------

/// Returns `true` when the given licence class is permitted by the default policy.
///
/// The default policy allows [`LicenseClass::Permissive`] and
/// [`LicenseClass::WeakCopyleft`]. All other classes, including
/// [`LicenseClass::Unknown`], are denied to avoid accidental use of
/// incompatible or unreviewed licences.
///
/// Prefer [`LicenseClass::is_allowed_by_default_policy`] on new code.
///
/// # Arguments
///
/// * `class` - The licence class to evaluate.
///
/// # Returns
///
/// `true` if the class is allowed, `false` if it is denied.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::rulesets::license::{LicenseClass, default_policy_allows};
///
/// assert!(default_policy_allows(&LicenseClass::Permissive));
/// assert!(default_policy_allows(&LicenseClass::WeakCopyleft));
/// assert!(!default_policy_allows(&LicenseClass::StrongCopyleft));
/// assert!(!default_policy_allows(&LicenseClass::NetworkCopyleft));
/// assert!(!default_policy_allows(&LicenseClass::Unknown));
/// ```
pub fn default_policy_allows(class: &LicenseClass) -> bool {
    class.is_allowed_by_default_policy()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // classify_license: spec-required tests
    // ------------------------------------------------------------------

    #[test]
    fn test_classify_license_mit_is_permissive() {
        assert_eq!(classify_license("MIT"), LicenseClass::Permissive);
    }

    #[test]
    fn test_classify_license_apache_is_permissive() {
        assert_eq!(classify_license("Apache-2.0"), LicenseClass::Permissive);
    }

    #[test]
    fn test_classify_license_bsd_is_permissive() {
        assert_eq!(classify_license("BSD"), LicenseClass::Permissive);
    }

    #[test]
    fn test_classify_license_lgpl_is_weak_copyleft() {
        assert_eq!(classify_license("LGPL-3.0"), LicenseClass::WeakCopyleft);
    }

    #[test]
    fn test_classify_license_mpl_is_weak_copyleft() {
        assert_eq!(classify_license("MPL-2.0"), LicenseClass::WeakCopyleft);
    }

    #[test]
    fn test_classify_license_gpl_is_strong_copyleft() {
        assert_eq!(classify_license("GPL-3.0"), LicenseClass::StrongCopyleft);
    }

    #[test]
    fn test_classify_license_agpl_is_network_copyleft() {
        assert_eq!(classify_license("AGPL-3.0"), LicenseClass::NetworkCopyleft);
    }

    #[test]
    fn test_classify_license_commons_clause_is_restricted() {
        // "Commons Clause" is a licence restriction, not a copyleft.
        // It must NOT be classified as WeakCopyleft.
        assert_eq!(
            classify_license("MIT WITH Commons Clause"),
            LicenseClass::Restricted
        );
        assert_eq!(classify_license("Commons Clause"), LicenseClass::Restricted);
    }

    #[test]
    fn test_classify_license_proprietary_is_restricted() {
        assert_eq!(classify_license("Proprietary"), LicenseClass::Restricted);
    }

    #[test]
    fn test_classify_license_non_commercial_is_nonccommercial() {
        assert_eq!(
            classify_license("CC-BY-NC-4.0"),
            LicenseClass::NonCommercial
        );
        assert_eq!(
            classify_license("non-commercial"),
            LicenseClass::NonCommercial
        );
    }

    #[test]
    fn test_classify_license_empty_string_is_unknown() {
        assert_eq!(classify_license(""), LicenseClass::Unknown);
    }

    #[test]
    fn test_classify_license_unrecognised_string_is_unknown() {
        assert_eq!(classify_license("CUSTOM-9.9"), LicenseClass::Unknown);
        assert_eq!(
            classify_license("my-special-licence"),
            LicenseClass::Unknown
        );
    }

    // ------------------------------------------------------------------
    // LicenseClass::is_allowed_by_default_policy: spec-required tests
    // ------------------------------------------------------------------

    #[test]
    fn test_license_class_permissive_is_allowed_by_default_policy() {
        assert!(LicenseClass::Permissive.is_allowed_by_default_policy());
    }

    #[test]
    fn test_license_class_weak_copyleft_is_allowed_by_default_policy() {
        assert!(LicenseClass::WeakCopyleft.is_allowed_by_default_policy());
    }

    #[test]
    fn test_license_class_strong_copyleft_is_denied_by_default_policy() {
        assert!(!LicenseClass::StrongCopyleft.is_allowed_by_default_policy());
    }

    #[test]
    fn test_license_class_network_copyleft_is_denied_by_default_policy() {
        assert!(!LicenseClass::NetworkCopyleft.is_allowed_by_default_policy());
    }

    #[test]
    fn test_license_class_nonccommercial_is_denied_by_default_policy() {
        assert!(!LicenseClass::NonCommercial.is_allowed_by_default_policy());
    }

    #[test]
    fn test_license_class_restricted_is_denied_by_default_policy() {
        assert!(!LicenseClass::Restricted.is_allowed_by_default_policy());
    }

    #[test]
    fn test_license_class_unknown_is_denied_by_default_policy() {
        assert!(!LicenseClass::Unknown.is_allowed_by_default_policy());
    }

    // ------------------------------------------------------------------
    // LicenseClass::policy_verdict
    // ------------------------------------------------------------------

    #[test]
    fn test_policy_verdict_permissive_returns_allowed() {
        assert_eq!(LicenseClass::Permissive.policy_verdict(), "allowed");
    }

    #[test]
    fn test_policy_verdict_strong_copyleft_returns_denied() {
        assert_eq!(LicenseClass::StrongCopyleft.policy_verdict(), "denied");
    }

    #[test]
    fn test_policy_verdict_unknown_returns_denied() {
        assert_eq!(LicenseClass::Unknown.policy_verdict(), "denied");
    }

    // ------------------------------------------------------------------
    // classify_license: additional SPDX coverage
    // ------------------------------------------------------------------

    #[test]
    fn test_classify_license_bsd3_returns_permissive() {
        assert_eq!(classify_license("BSD-3-Clause"), LicenseClass::Permissive);
    }

    #[test]
    fn test_classify_license_gpl2_returns_strong_copyleft() {
        assert_eq!(classify_license("GPL-2.0"), LicenseClass::StrongCopyleft);
    }

    #[test]
    fn test_classify_license_lgpl3_returns_weak_copyleft() {
        assert_eq!(classify_license("LGPL-3.0"), LicenseClass::WeakCopyleft);
    }

    #[test]
    fn test_classify_license_mpl2_returns_weak_copyleft() {
        assert_eq!(classify_license("MPL-2.0"), LicenseClass::WeakCopyleft);
    }

    #[test]
    fn test_classify_license_agpl3_returns_network_copyleft() {
        assert_eq!(classify_license("AGPL-3.0"), LicenseClass::NetworkCopyleft);
    }

    #[test]
    fn test_classify_license_cc_by_nc_returns_non_commercial() {
        assert_eq!(
            classify_license("CC-BY-NC-4.0"),
            LicenseClass::NonCommercial
        );
    }

    #[test]
    fn test_classify_license_unknown_spdx_returns_unknown() {
        assert_eq!(classify_license("CUSTOM-9.9"), LicenseClass::Unknown);
    }

    #[test]
    fn test_classify_license_is_case_insensitive() {
        assert_eq!(classify_license("mit"), LicenseClass::Permissive);
        assert_eq!(classify_license("gpl-3.0"), LicenseClass::StrongCopyleft);
    }

    // ------------------------------------------------------------------
    // classify_license_file: text heuristics
    // ------------------------------------------------------------------

    #[test]
    fn test_classify_license_file_mit_text_returns_permissive() {
        let text = "MIT License\nPermission is hereby granted, free of charge, to any person";
        assert_eq!(classify_license_file(text), LicenseClass::Permissive);
    }

    #[test]
    fn test_classify_license_file_apache_text_returns_permissive() {
        let text = "Apache License\nVersion 2.0, January 2004";
        assert_eq!(classify_license_file(text), LicenseClass::Permissive);
    }

    #[test]
    fn test_classify_license_file_gpl3_text_returns_strong_copyleft() {
        let text = "GNU GENERAL PUBLIC LICENSE\nVersion 3, 29 June 2007";
        assert_eq!(classify_license_file(text), LicenseClass::StrongCopyleft);
    }

    #[test]
    fn test_classify_license_file_agpl3_text_returns_network_copyleft() {
        let text = "GNU AFFERO GENERAL PUBLIC LICENSE\nVersion 3, 19 November 2007";
        assert_eq!(classify_license_file(text), LicenseClass::NetworkCopyleft);
    }

    #[test]
    fn test_classify_license_file_mpl2_text_returns_weak_copyleft() {
        let text = "Mozilla Public License Version 2.0";
        assert_eq!(classify_license_file(text), LicenseClass::WeakCopyleft);
    }

    #[test]
    fn test_classify_license_file_empty_returns_unknown() {
        assert_eq!(classify_license_file(""), LicenseClass::Unknown);
    }

    // ------------------------------------------------------------------
    // default_policy_allows (free function)
    // ------------------------------------------------------------------

    #[test]
    fn test_default_policy_allows_permissive_returns_true() {
        assert!(default_policy_allows(&LicenseClass::Permissive));
    }

    #[test]
    fn test_default_policy_allows_weak_copyleft_returns_true() {
        assert!(default_policy_allows(&LicenseClass::WeakCopyleft));
    }

    #[test]
    fn test_default_policy_allows_strong_copyleft_returns_false() {
        assert!(!default_policy_allows(&LicenseClass::StrongCopyleft));
    }

    #[test]
    fn test_default_policy_allows_network_copyleft_returns_false() {
        assert!(!default_policy_allows(&LicenseClass::NetworkCopyleft));
    }

    #[test]
    fn test_default_policy_allows_non_commercial_returns_false() {
        assert!(!default_policy_allows(&LicenseClass::NonCommercial));
    }

    #[test]
    fn test_default_policy_allows_restricted_returns_false() {
        assert!(!default_policy_allows(&LicenseClass::Restricted));
    }

    #[test]
    fn test_default_policy_allows_unknown_returns_false() {
        assert!(!default_policy_allows(&LicenseClass::Unknown));
    }
}
