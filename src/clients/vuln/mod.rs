//! Vulnerability intelligence types and the [`VulnerabilitySource`] async abstraction.
//!
//! Defines the data model for vulnerability queries and records following the
//! [OSV schema](https://osv.dev/docs/), a custom error type, and the
//! [`VulnerabilitySource`] async trait implemented by all back-end clients.
//!
//! # Component-Boundary Contract
//!
//! | Rule           | Detail                                               |
//! |----------------|------------------------------------------------------|
//! | May depend on  | `auth`, `config`                                     |
//! | Must NOT call  | `scanner`, `providers`, `agent`                      |
//! | Must NOT be    | called from `tools/`                                 |
//!
//! These rules keep the vulnerability-fetching layer as a pure leaf in the
//! dependency graph and prevent circular imports.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod osv;

pub use osv::OsvClient;
pub use osv::scoring::{CvssBand, OsvScore, score_severity};

// ---------------------------------------------------------------------------
// VulnerabilityQuery
// ---------------------------------------------------------------------------

/// A query submitted to a vulnerability source.
///
/// At least one of `purl`, the `name`+`ecosystem` pair, or `commit` must be
/// populated for a useful lookup.  The [`OsvClient`] falls back through each
/// strategy in that order.
///
/// # Examples
///
/// ```
/// use xzardgz::clients::vuln::VulnerabilityQuery;
///
/// let q = VulnerabilityQuery {
///     name: "jinja2".to_string(),
///     version: Some("2.9.6".to_string()),
///     ecosystem: Some("PyPI".to_string()),
///     purl: Some("pkg:pypi/jinja2@2.9.6".to_string()),
///     commit: None,
/// };
/// assert_eq!(q.name, "jinja2");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityQuery {
    /// Package name (e.g. `jinja2`, `express`).
    pub name: String,
    /// Optional version string (e.g. `2.9.6`).
    pub version: Option<String>,
    /// Optional ecosystem identifier (e.g. `PyPI`, `npm`, `crates.io`).
    pub ecosystem: Option<String>,
    /// Optional PURL (e.g. `pkg:pypi/jinja2@2.9.6`).
    pub purl: Option<String>,
    /// Optional Git commit hash for commit-level vulnerability matching.
    pub commit: Option<String>,
}

// ---------------------------------------------------------------------------
// VulnerabilityRecord and nested OSV schema types
// ---------------------------------------------------------------------------

/// A single vulnerability record returned by an OSV-compatible source.
///
/// Fields mirror the [OSV schema](https://osv.dev/docs/).  Unknown JSON
/// fields are silently ignored during deserialization so that new schema
/// versions do not break existing code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityRecord {
    /// OSV or advisory database identifier (e.g. `GHSA-462w-v97r-4m45`).
    pub id: String,
    /// Short one-line summary of the vulnerability.
    pub summary: Option<String>,
    /// Full human-readable description.
    pub details: Option<String>,
    /// Alternate identifiers from other databases (e.g. `CVE-2019-10906`).
    #[serde(default)]
    pub aliases: Vec<String>,
    /// URLs and advisory references related to this vulnerability.
    #[serde(default)]
    pub references: Vec<OsvReference>,
    /// CVSS and other severity scores attached to this record.
    #[serde(default)]
    pub severity: Vec<OsvSeverityEntry>,
    /// Package and version-range information for affected components.
    #[serde(default)]
    pub affected: Vec<OsvAffected>,
    /// Source-database-specific metadata as an opaque JSON value.
    pub database_specific: Option<serde_json::Value>,
}

/// A reference (URL) associated with a vulnerability record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvReference {
    /// Reference category (e.g. `ADVISORY`, `WEB`, `FIX`).
    #[serde(rename = "type")]
    pub r#type: String,
    /// The reference URL.
    pub url: String,
}

/// A severity score entry attached to a vulnerability record.
///
/// The `r#type` field identifies the scoring system (`CVSS_V3`, `CVSS_V4`),
/// and `score` holds the raw CVSS vector string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvSeverityEntry {
    /// Scoring system identifier (e.g. `CVSS_V3`, `CVSS_V4`).
    #[serde(rename = "type")]
    pub r#type: String,
    /// Raw CVSS vector string (e.g. `CVSS:3.1/AV:N/AC:L/...`).
    pub score: String,
}

/// Affected package and version-range information within a vulnerability record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvAffected {
    /// The affected package.
    pub package: OsvPackage,
    /// Specific affected versions, if enumerated.
    #[serde(default)]
    pub versions: Vec<String>,
    /// Affected version ranges, stored as raw JSON to accommodate schema
    /// variations across databases.
    #[serde(default)]
    pub ranges: Vec<serde_json::Value>,
}

/// Package identity within an affected-packages entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvPackage {
    /// Package name within the ecosystem.
    pub name: String,
    /// Ecosystem identifier (e.g. `PyPI`, `npm`, `crates.io`).
    pub ecosystem: String,
    /// Optional PURL for this package.
    pub purl: Option<String>,
}

// ---------------------------------------------------------------------------
// VulnClientError
// ---------------------------------------------------------------------------

/// Errors that can occur when querying a vulnerability source.
#[derive(Debug, Error)]
pub enum VulnClientError {
    /// HTTP-level error (network failure or non-success HTTP status code).
    #[error("HTTP error querying '{url}': {message}")]
    Http {
        /// The URL that was being queried when the error occurred.
        url: String,
        /// A description of the HTTP error.
        message: String,
    },
    /// JSON deserialization failure for the vulnerability response.
    #[error("failed to parse vulnerability response: {0}")]
    Parse(String),
    /// No vulnerabilities were found across all query strategies.
    #[error("no vulnerability results found")]
    NoResults,
}

// ---------------------------------------------------------------------------
// VulnerabilitySource
// ---------------------------------------------------------------------------

/// Async trait for vulnerability data sources.
///
/// Each implementation wraps a specific upstream database and translates its
/// response format into [`VulnerabilityRecord`]s.
///
/// # Errors
///
/// Returns [`VulnClientError`] on network failure or deserialization errors.
#[async_trait]
pub trait VulnerabilitySource: Send + Sync {
    /// Queries the vulnerability source for records matching the given dependency.
    ///
    /// # Arguments
    ///
    /// * `dep` - The dependency query containing package identifiers and version
    ///   information.
    ///
    /// # Returns
    ///
    /// A `Vec<VulnerabilityRecord>` of matching records; may be empty when none
    /// are found.
    ///
    /// # Errors
    ///
    /// - [`VulnClientError::Http`] on network failures or non-2xx HTTP status.
    /// - [`VulnClientError::Parse`] when the response body cannot be
    ///   deserialized.
    async fn query(
        &self,
        dep: &VulnerabilityQuery,
    ) -> Result<Vec<VulnerabilityRecord>, VulnClientError>;
}
