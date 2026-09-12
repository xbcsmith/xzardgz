//! Output projection modules for the SAST scanner.
//!
//! This module contains the two output projections defined in Phase 5:
//!
//! | Submodule      | Format              | Specification              |
//! |----------------|---------------------|----------------------------|
//! | [`sarif`]      | SARIF 2.1.0 JSON    | OASIS SARIF v2.1.0         |
//! | [`cyclonedx`]  | CycloneDX 1.7 JSON  | CycloneDX Specification    |
//!
//! Both submodules export a single public projection function that accepts a
//! [`SastScanReport`] reference and returns a serialisable value:
//!
//! ```ignore
//! // SARIF
//! let log = sarif::render_sarif(&report);
//! let json = serde_json::to_string_pretty(&log)?;
//!
//! // CycloneDX
//! let bom = cyclonedx::render_cyclonedx(&report);
//! let json = serde_json::to_string_pretty(&bom)?;
//! ```
//!
//! [`SastScanReport`]: crate::scanner::sast::SastScanReport

pub mod cyclonedx;
pub mod sarif;
