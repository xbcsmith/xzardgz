//! Backward-compatible re-export of git repository operations.
//!
//! The canonical implementation lives in [`crate::git::ops`].
//! This module preserves the original `crate::repository::git::GitRepository`
//! path for any code that imported it from the old location.

pub use crate::git::ops::GitRepository;
