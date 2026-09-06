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
//! | [`scope`] | [`FileMatchEntry`], [`InvestigationScope`], and [`ScopeMetrics`] |
//! | [`batch`] | [`BatchConfig`], [`InvestigationBatch`], and splitting utilities |
//! | [`strategy`] | [`InvestigationStrategy`], [`compute_turn_budget`], [`decide_investigation_strategy`] |
//!
//! # Typical Workflow
//!
//! 1. The scanner preselection layer produces a set of candidate files.
//! 2. Each candidate is wrapped into a [`FileMatchEntry`] and inserted into
//!    an [`InvestigationScope`].
//! 3. Build a [`ScopeMetrics`] from a [`crate::scanner::result::ScanResult`] via
//!    [`ScopeMetrics::from_scan_result`] to obtain repository-level totals.
//! 4. Call [`compute_turn_budget`] to determine how many AI turns to allow, and
//!    [`decide_investigation_strategy`] (or [`InvestigationStrategy::default_for_scope`])
//!    to pick [`InvestigationStrategy::SingleSession`] or
//!    [`InvestigationStrategy::BatchedSession`].
//! 5. For batched strategies, [`split_into_batches`] partitions the scope
//!    into deterministically ordered [`InvestigationBatch`] slices.
//! 6. Each batch is dispatched to a plugin for AI-driven analysis.

pub mod batch;
pub mod scope;
pub mod strategy;

pub use batch::{BatchConfig, InvestigationBatch, compute_investigation_turns, split_into_batches};
pub use scope::{FileMatchEntry, InvestigationScope, ScopeMetrics};
pub use strategy::{InvestigationStrategy, compute_turn_budget, decide_investigation_strategy};
