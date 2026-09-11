//! Rule model for the SAST pipeline.
//!
//! | Submodule    | Purpose                                                 |
//! |--------------|---------------------------------------------------------|
//! | [`compat`]   | Compatibility gate for unsupported rule constructs      |
//! | [`ir`]       | Compiled intermediate representation (`RuleIr`, etc.)  |
//! | [`metadata`] | Structured metadata types (`Severity`, `Confidence`)   |
//! | [`parse`]    | Rule parser: schema to IR compilation                  |
//! | [`schema`]   | Serde types for the Semgrep YAML rule surface           |

pub mod compat;
pub mod ir;
pub mod metadata;
pub mod parse;
pub mod schema;

pub use ir::{CompileOutcome, Condition, Formula, Leaf, MetavarId, RuleIr};
pub use metadata::{Confidence, RuleMetadata, Severity, StringOrVec};
pub use schema::{RuleFile, RuleSchema};
