//! SAST matching engine modules.
//!
//! The engine modules implement the formula evaluation and scanning logic.
//! Additional engine modules (pattern compilation, range algebra, formula
//! evaluation, conditions, prefiltering) are added in later phases.

pub mod compare;
pub mod conditions;
pub mod formula;
pub mod pattern;
pub mod range;
pub mod regex_mode;

pub use compare::{CompareError, CompareOptions, eval_comparison, validate_comparison};
pub use formula::{TruncationReason, eval_formula, scan_rule};
pub use pattern::PatternCompiler;
pub use range::{MetavarBindings, MetavarValue, RangeWithMetavars};
pub use regex_mode::{RegexMatch, RegexModeScanner};
