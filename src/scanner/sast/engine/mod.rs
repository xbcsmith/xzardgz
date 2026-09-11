//! SAST matching engine modules.
//!
//! The engine modules implement the formula evaluation and scanning logic.
//! Additional engine modules (pattern compilation, range algebra, formula
//! evaluation, conditions, prefiltering) are added in later phases.

pub mod regex_mode;

pub use regex_mode::{RegexMatch, RegexModeScanner};
