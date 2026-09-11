//! AST parsing and analysis layer for the SAST engine.
//!
//! Provides language detection, concurrency-safe parse caching, and error
//! node density accounting for tree-sitter parse trees.
//!
//! ## Submodule layout
//!
//! | Submodule       | Purpose                                                     |
//! |-----------------|-------------------------------------------------------------|
//! | [`diagnostics`] | `ERROR`/`MISSING` node density accounting                  |
//! | [`lang`]        | Language detection from extension, path, and Semgrep names |
//! | [`parse`]       | Concurrency-safe parse cache                                |

pub mod diagnostics;
pub mod lang;
pub mod parse;

pub use diagnostics::ErrorNodeDensity;
pub use lang::Language;
pub use parse::{CachedRoot, ParseCache};
