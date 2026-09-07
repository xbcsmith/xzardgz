//! Prompt template management for workflow plugins.
//!
//! This module provides [`loader::PromptLoader`], a three-level prompt
//! resolution engine that serves embedded defaults, file-based overrides, and
//! in-memory overrides to workflow plugins via a unified
//! [`PromptLoader::render`][loader::PromptLoader::render] call.
//!
//! ## Resolution order
//!
//! 1. In-memory override (set via
//!    [`PromptLoader::with_in_memory_overrides`][loader::PromptLoader::with_in_memory_overrides]).
//! 2. File-based override from directories listed in
//!    [`PromptsConfig::directories`][crate::config::PromptsConfig::directories]
//!    (only when
//!    [`PromptsConfig::allow_overrides`][crate::config::PromptsConfig::allow_overrides]
//!    is `true`).
//! 3. Compiled-in embedded default from
//!    `src/prompts/templates/{plugin}/{key}.tera`.
//!
//! Override failures fall back silently to the next priority level; the loader
//! is infallible.

pub mod loader;

pub use loader::PromptLoader;
