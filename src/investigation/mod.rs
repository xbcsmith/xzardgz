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
//! | [`scope`] | [`FileMatchEntry`], [`InvestigationScope`], [`ScopeMetrics`] |
//! | [`batch`] | [`BatchConfig`], [`InvestigationBatch`], [`BatchSession`], [`BatchedInvestigationRunner`], error and outcome types |
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
//! 5. For batched strategies, construct a [`BatchedInvestigationRunner`] with a
//!    [`BatchSession`] implementation and call [`BatchedInvestigationRunner::run`].
//! 6. The runner dispatches batches concurrently (or sequentially when
//!    [`BatchConfig::sequential_batches`] is `true`), degrades gracefully on
//!    [`InvestigationError::TurnBudgetExceeded`], and returns an
//!    [`InvestigationOutcome`] that aggregates all findings and diagnostics.
//! 7. Call [`InvestigationOutcome::into_watcher_diagnostics`] to drain the
//!    diagnostics into a `WatcherResultMessage.diagnostics` field so they are
//!    visible in downstream watcher consumers.

pub mod batch;
pub mod scope;
pub mod strategy;

pub use batch::{
    BatchConfig, BatchOutcome, BatchSession, BatchedInvestigationRunner, InvestigationBatch,
    InvestigationError, InvestigationOutcome, compute_investigation_turns, split_into_batches,
};
pub use scope::{FileMatchEntry, InvestigationScope, ScopeMetrics};
pub use strategy::{InvestigationStrategy, compute_turn_budget, decide_investigation_strategy};
