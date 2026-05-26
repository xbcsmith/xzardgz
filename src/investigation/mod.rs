//! Investigation module for the XZardgz pipeline.
//!
//! Provides the types and utilities that govern the investigation phase of
//! plugin execution. During this phase, a set of candidate files is selected,
//! grouped, and dispatched to AI-driven plugin sessions.
//!
//! # Module Layout
//!
//! | Sub-module | Purpose |
//! |---|---|
//! | [`scope`] | [`FileMatchEntry`] and [`InvestigationScope`] - the file set |
//! | [`batch`] | [`BatchConfig`], [`InvestigationBatch`], and splitting utilities |
//! | [`strategy`] | [`InvestigationStrategy`] - single vs. batched session selection |
//!
//! # Typical Workflow
//!
//! 1. The scanner preselection layer produces a set of candidate files.
//! 2. Each candidate is wrapped into a [`FileMatchEntry`] and inserted into
//!    an [`InvestigationScope`].
//! 3. [`InvestigationStrategy::default_for_scope`] selects either
//!    [`InvestigationStrategy::SingleSession`] or
//!    [`InvestigationStrategy::BatchedSession`].
//! 4. For batched strategies, [`split_into_batches`] partitions the scope
//!    into deterministically ordered [`InvestigationBatch`] slices.
//! 5. Each batch is dispatched to a plugin for AI-driven analysis.

pub mod batch;
pub mod scope;
pub mod strategy;

pub use batch::{BatchConfig, InvestigationBatch, compute_investigation_turns, split_into_batches};
pub use scope::{FileMatchEntry, InvestigationScope};
pub use strategy::InvestigationStrategy;
