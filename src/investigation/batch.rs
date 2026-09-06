//! Batch splitting utilities for investigation scopes.
//!
//! Provides [`BatchConfig`] for configuring batch parameters,
//! [`InvestigationBatch`] as a slice of a larger scope, and
//! [`split_into_batches`] for deterministically partitioning an
//! [`InvestigationScope`] into bounded chunks.

use async_trait::async_trait;
use thiserror::Error;

use crate::diagnostics::{Diagnostic, DiagnosticCategory, Diagnostics};
use crate::investigation::scope::{FileMatchEntry, InvestigationScope};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// BatchConfig
// ---------------------------------------------------------------------------

/// Configuration for batched investigation.
///
/// Controls how many batches are produced and how many files each batch holds.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::batch::BatchConfig;
///
/// let config = BatchConfig::new(5, 10, 2);
/// assert_eq!(config.max_batches, 5);
/// assert_eq!(config.batch_size, 10);
/// assert_eq!(config.clean_verification_turns, 2);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    /// Maximum number of batches to produce.
    pub max_batches: usize,
    /// Maximum number of files per batch.
    pub batch_size: usize,
    /// Number of verification turns to run after each batch.
    pub clean_verification_turns: usize,
    /// Whether to run investigation batches sequentially rather than concurrently.
    ///
    /// Set to `true` for constrained inference backends that cannot handle
    /// concurrent sessions. Defaults to `false` (concurrent by default).
    pub sequential_batches: bool,
}

impl BatchConfig {
    /// Creates a new [`BatchConfig`] with the specified parameters.
    ///
    /// # Arguments
    ///
    /// * `max_batches` - Maximum number of batches to produce.
    /// * `batch_size` - Maximum number of files per batch.
    /// * `clean_verification_turns` - Number of verification turns per batch.
    ///
    /// # Returns
    ///
    /// A new [`BatchConfig`] with the given values.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::batch::BatchConfig;
    ///
    /// let config = BatchConfig::new(5, 10, 2);
    /// assert_eq!(config.max_batches, 5);
    /// assert_eq!(config.batch_size, 10);
    /// assert_eq!(config.clean_verification_turns, 2);
    /// ```
    pub fn new(max_batches: usize, batch_size: usize, clean_verification_turns: usize) -> Self {
        Self {
            max_batches,
            batch_size,
            clean_verification_turns,
            sequential_batches: false,
        }
    }

    /// Sets the `sequential_batches` flag (builder pattern).
    ///
    /// When `true`, batches run one-at-a-time rather than concurrently.
    /// Use this for inference backends that cannot handle concurrent sessions.
    ///
    /// # Arguments
    ///
    /// * `sequential` - `true` to run sequentially, `false` to run concurrently.
    ///
    /// # Returns
    ///
    /// The updated [`BatchConfig`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::batch::BatchConfig;
    ///
    /// let config = BatchConfig::default().with_sequential(true);
    /// assert!(config.sequential_batches);
    /// ```
    pub fn with_sequential(mut self, sequential: bool) -> Self {
        self.sequential_batches = sequential;
        self
    }
}

impl Default for BatchConfig {
    /// Returns a [`BatchConfig`] with `max_batches=10`, `batch_size=20`,
    /// and `clean_verification_turns=1`.
    ///
    /// These defaults are suitable for most mid-sized repositories.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::batch::BatchConfig;
    ///
    /// let config = BatchConfig::default();
    /// assert_eq!(config.max_batches, 10);
    /// assert_eq!(config.batch_size, 20);
    /// assert_eq!(config.clean_verification_turns, 1);
    /// ```
    fn default() -> Self {
        Self {
            max_batches: 10,
            batch_size: 20,
            clean_verification_turns: 1,
            sequential_batches: false,
        }
    }
}

// ---------------------------------------------------------------------------
// InvestigationBatch
// ---------------------------------------------------------------------------

/// A single investigation batch: a sorted slice of the full scope.
///
/// Produced by [`split_into_batches`] and consumed by the plugin execution
/// layer. Each batch carries a 0-based index and the total number of batches
/// so that callers can report progress.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
/// use xzardgz::investigation::batch::{BatchConfig, split_into_batches};
///
/// let mut scope = InvestigationScope::new();
/// for i in 0..3 {
///     scope.insert(FileMatchEntry::new(format!("file_{}.rs", i)));
/// }
/// let config = BatchConfig::new(10, 2, 1);
/// let batches = split_into_batches(&scope, &config);
/// assert_eq!(batches[0].index, 0);
/// assert_eq!(batches[0].total_batches, 2);
/// ```
#[derive(Debug, Clone)]
pub struct InvestigationBatch {
    /// 0-based batch index.
    pub index: usize,
    /// Total number of batches in this run.
    pub total_batches: usize,
    /// The files assigned to this batch.
    pub scope: InvestigationScope,
}

// ---------------------------------------------------------------------------
// split_into_batches
// ---------------------------------------------------------------------------

/// Splits an [`InvestigationScope`] into batches according to [`BatchConfig`].
///
/// Files are sorted by path before splitting to ensure deterministic output.
/// Returns at most `config.max_batches` batches of at most `config.batch_size`
/// files each. Returns an empty `Vec` if the scope is empty or if
/// `config.batch_size` is zero.
///
/// # Arguments
///
/// * `scope` - The investigation scope to split.
/// * `config` - Batch configuration controlling chunk size and batch cap.
///
/// # Returns
///
/// A `Vec<InvestigationBatch>` ordered by ascending index. Each batch's
/// `total_batches` field reflects the actual number of batches created.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
/// use xzardgz::investigation::batch::{BatchConfig, split_into_batches};
///
/// let mut scope = InvestigationScope::new();
/// for i in 0..5 {
///     scope.insert(FileMatchEntry::new(format!("file_{}.rs", i)));
/// }
/// let config = BatchConfig::new(10, 3, 1);
/// let batches = split_into_batches(&scope, &config);
/// assert_eq!(batches.len(), 2);
/// assert_eq!(batches[0].scope.len(), 3);
/// assert_eq!(batches[1].scope.len(), 2);
/// ```
pub fn split_into_batches(
    scope: &InvestigationScope,
    config: &BatchConfig,
) -> Vec<InvestigationBatch> {
    if scope.is_empty() || config.batch_size == 0 {
        return Vec::new();
    }

    // entries() already returns entries sorted by path, giving deterministic
    // chunk membership across independent invocations.
    let sorted_entries: Vec<&FileMatchEntry> = scope.entries();

    let owned_chunks: Vec<Vec<FileMatchEntry>> = sorted_entries
        .chunks(config.batch_size)
        .take(config.max_batches)
        .map(|chunk| chunk.iter().map(|e| (*e).clone()).collect())
        .collect();

    let total_batches = owned_chunks.len();

    owned_chunks
        .into_iter()
        .enumerate()
        .map(|(index, files)| {
            let mut batch_scope = InvestigationScope::new();
            for entry in files {
                batch_scope.insert(entry);
            }
            InvestigationBatch {
                index,
                total_batches,
                scope: batch_scope,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// compute_investigation_turns
// ---------------------------------------------------------------------------

/// Computes the number of investigation turns needed given a scope and config.
///
/// Returns the number of batches that would be created (capped by
/// `config.max_batches`). Returns `0` for an empty scope or a
/// `config.batch_size` of zero.
///
/// # Arguments
///
/// * `scope` - The investigation scope to evaluate.
/// * `config` - Batch configuration.
///
/// # Returns
///
/// The number of turns (batches), in the range `[0, config.max_batches]`.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
/// use xzardgz::investigation::batch::{BatchConfig, compute_investigation_turns};
///
/// let mut scope = InvestigationScope::new();
/// for i in 0..25 {
///     scope.insert(FileMatchEntry::new(format!("file_{}.rs", i)));
/// }
/// let config = BatchConfig::new(10, 20, 1);
/// let turns = compute_investigation_turns(&scope, &config);
/// assert_eq!(turns, 2);
/// ```
pub fn compute_investigation_turns(scope: &InvestigationScope, config: &BatchConfig) -> usize {
    if scope.is_empty() || config.batch_size == 0 {
        return 0;
    }
    let raw = scope.len().div_ceil(config.batch_size);
    raw.min(config.max_batches)
}

// ---------------------------------------------------------------------------
// InvestigationError
// ---------------------------------------------------------------------------

/// Errors that can occur when running an investigation batch session.
///
/// On [`InvestigationError::TurnBudgetExceeded`] the calling
/// [`BatchedInvestigationRunner`] degrades gracefully: it contributes the
/// `partial_findings` to the aggregate [`InvestigationOutcome`] and records a
/// [`Diagnostic`] rather than failing the whole plugin run.
///
/// # Examples
///
/// ```
/// use xzardgz::investigation::batch::InvestigationError;
///
/// let err = InvestigationError::TurnBudgetExceeded {
///     batch_index: 0,
///     turn_limit: 10,
///     partial_findings: vec!["partial finding".to_string()],
/// };
/// assert!(err.to_string().contains("batch 0"));
/// assert!(err.to_string().contains("10"));
/// ```
#[derive(Error, Debug)]
pub enum InvestigationError {
    /// A session consumed all its allocated turns without completing analysis.
    ///
    /// Partial findings collected up to the point of exhaustion are preserved
    /// in `partial_findings` so they can be contributed to the aggregate result.
    #[error("batch {batch_index} exhausted its turn budget of {turn_limit} turns")]
    TurnBudgetExceeded {
        /// 0-based index of the batch that exhausted its budget.
        batch_index: usize,
        /// Turn limit that was reached.
        turn_limit: u32,
        /// Findings collected before the budget was exhausted.
        partial_findings: Vec<String>,
    },
    /// A session failed for a reason unrelated to the turn budget.
    #[error("batch {batch_index} failed: {message}")]
    BatchFailed {
        /// 0-based index of the batch that failed.
        batch_index: usize,
        /// Human-readable error description.
        message: String,
    },
}

// ---------------------------------------------------------------------------
// BatchOutcome
// ---------------------------------------------------------------------------

/// The result of a single investigation batch session.
///
/// Produced by a [`BatchSession`] implementation and consumed by
/// [`BatchedInvestigationRunner`] to build the aggregate [`InvestigationOutcome`].
///
/// # Examples
///
/// ```
/// use xzardgz::diagnostics::Diagnostics;
/// use xzardgz::investigation::batch::BatchOutcome;
///
/// let outcome = BatchOutcome {
///     batch_index: 0,
///     total_batches: 2,
///     findings: vec!["finding one".to_string()],
///     diagnostics: Diagnostics::new(),
///     turn_budget_exhausted: false,
/// };
/// assert!(!outcome.turn_budget_exhausted);
/// assert_eq!(outcome.findings.len(), 1);
/// ```
#[derive(Debug, Clone)]
pub struct BatchOutcome {
    /// 0-based index of the batch that produced this outcome.
    pub batch_index: usize,
    /// Total number of batches in the run.
    pub total_batches: usize,
    /// Findings collected during this batch (may be partial on exhaustion).
    pub findings: Vec<String>,
    /// Diagnostics produced during this batch (e.g. budget-exhaustion warnings).
    pub diagnostics: Diagnostics,
    /// `true` when this batch exhausted its turn budget before completing.
    pub turn_budget_exhausted: bool,
}

// ---------------------------------------------------------------------------
// InvestigationOutcome
// ---------------------------------------------------------------------------

/// Aggregate outcome of a multi-batch investigation run.
///
/// Produced by [`BatchedInvestigationRunner::run`]. Collects findings and
/// diagnostics from all batches. When one or more batches exhausted their
/// turn budget, the outcome is considered *partial*; the diagnostics
/// collection contains a [`DiagnosticCategory::Plugin`]-level warning for
/// each exhausted batch.
///
/// # Examples
///
/// ```
/// use xzardgz::diagnostics::{DiagnosticCategory, DiagnosticLevel, Diagnostics};
/// use xzardgz::investigation::batch::{BatchOutcome, InvestigationOutcome};
///
/// let mut outcome = InvestigationOutcome::new();
/// assert!(outcome.is_empty());
/// assert!(!outcome.is_partial());
/// ```
#[derive(Debug, Clone, Default)]
pub struct InvestigationOutcome {
    /// All findings collected across every batch.
    pub findings: Vec<String>,
    /// All diagnostics produced across every batch (warnings for exhausted budgets, etc.)
    pub diagnostics: Diagnostics,
    /// Number of batches that completed without budget exhaustion.
    pub batches_completed: usize,
    /// Number of batches that exhausted their turn budget.
    pub batches_exhausted: usize,
    /// Total number of batches in the run.
    pub total_batches: usize,
}

impl InvestigationOutcome {
    /// Creates an empty [`InvestigationOutcome`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::batch::InvestigationOutcome;
    ///
    /// let outcome = InvestigationOutcome::new();
    /// assert!(outcome.is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Merges a [`BatchOutcome`] into this aggregate outcome.
    ///
    /// Findings and diagnostics are appended.  The batch counters and
    /// `total_batches` are updated accordingly.
    ///
    /// # Arguments
    ///
    /// * `batch` - The batch result to merge.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::diagnostics::Diagnostics;
    /// use xzardgz::investigation::batch::{BatchOutcome, InvestigationOutcome};
    ///
    /// let mut outcome = InvestigationOutcome::new();
    /// let batch = BatchOutcome {
    ///     batch_index: 0,
    ///     total_batches: 1,
    ///     findings: vec!["f1".to_string()],
    ///     diagnostics: Diagnostics::new(),
    ///     turn_budget_exhausted: false,
    /// };
    /// outcome.merge_batch_outcome(batch);
    /// assert_eq!(outcome.findings.len(), 1);
    /// assert_eq!(outcome.batches_completed, 1);
    /// ```
    pub fn merge_batch_outcome(&mut self, batch: BatchOutcome) {
        self.total_batches = batch.total_batches;
        self.findings.extend(batch.findings);
        self.diagnostics.merge(batch.diagnostics);
        if batch.turn_budget_exhausted {
            self.batches_exhausted += 1;
        } else {
            self.batches_completed += 1;
        }
    }

    /// Returns `true` when at least one batch exhausted its turn budget.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::diagnostics::Diagnostics;
    /// use xzardgz::investigation::batch::{BatchOutcome, InvestigationOutcome};
    ///
    /// let mut outcome = InvestigationOutcome::new();
    /// let exhausted = BatchOutcome {
    ///     batch_index: 0,
    ///     total_batches: 1,
    ///     findings: vec![],
    ///     diagnostics: Diagnostics::new(),
    ///     turn_budget_exhausted: true,
    /// };
    /// outcome.merge_batch_outcome(exhausted);
    /// assert!(outcome.is_partial());
    /// ```
    pub fn is_partial(&self) -> bool {
        self.batches_exhausted > 0
    }

    /// Returns `true` when no findings have been collected across any batch.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::batch::InvestigationOutcome;
    ///
    /// let outcome = InvestigationOutcome::new();
    /// assert!(outcome.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }

    /// Consumes this outcome and returns the diagnostics entries as a
    /// `Vec<Diagnostic>` suitable for extending a
    /// `WatcherResultMessage.diagnostics` field.
    ///
    /// This is the integration point between investigation-level diagnostics
    /// and the watcher message layer: callers can drain all diagnostics into
    /// the watcher result so they appear in `WatcherResultMessage.diagnostics`
    /// and surface in downstream consumers.
    ///
    /// # Returns
    ///
    /// A `Vec<Diagnostic>` containing every diagnostic produced during the run.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::investigation::batch::InvestigationOutcome;
    ///
    /// let outcome = InvestigationOutcome::new();
    /// let diags = outcome.into_watcher_diagnostics();
    /// assert!(diags.is_empty());
    /// ```
    pub fn into_watcher_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics.entries
    }
}

// ---------------------------------------------------------------------------
// BatchSession trait
// ---------------------------------------------------------------------------

/// Abstracts a single AI session that analyzes one [`InvestigationBatch`].
///
/// Implementors wrap the actual AI provider call (or a mock for testing).
/// The [`BatchedInvestigationRunner`] calls [`BatchSession::run`] for each
/// batch and handles the two failure modes defined by [`InvestigationError`].
///
/// # Implementor contract
///
/// - Return `Ok(BatchOutcome)` on success (even partial success).
/// - Return `Err(InvestigationError::TurnBudgetExceeded { .. })` when the
///   session consumed all allocated turns; include any partial findings
///   collected before exhaustion in the error's `partial_findings` field.
/// - Return `Err(InvestigationError::BatchFailed { .. })` for any other
///   non-recoverable error.
///
/// # Examples
///
/// ```
/// use async_trait::async_trait;
/// use xzardgz::diagnostics::Diagnostics;
/// use xzardgz::investigation::batch::{
///     BatchOutcome, BatchSession, InvestigationBatch, InvestigationError,
/// };
///
/// struct AlwaysSucceeds;
///
/// #[async_trait]
/// impl BatchSession for AlwaysSucceeds {
///     async fn run(
///         &self,
///         batch: &InvestigationBatch,
///         _turn_budget: u32,
///     ) -> Result<BatchOutcome, InvestigationError> {
///         Ok(BatchOutcome {
///             batch_index: batch.index,
///             total_batches: batch.total_batches,
///             findings: vec!["ok".to_string()],
///             diagnostics: Diagnostics::new(),
///             turn_budget_exhausted: false,
///         })
///     }
/// }
/// ```
#[async_trait]
pub trait BatchSession: Send + Sync {
    /// Runs AI analysis on `batch` within the given `turn_budget`.
    ///
    /// # Arguments
    ///
    /// * `batch` - The investigation batch to analyze.
    /// * `turn_budget` - Maximum number of AI turns to consume.
    ///
    /// # Errors
    ///
    /// Returns [`InvestigationError::TurnBudgetExceeded`] when all turns are
    /// consumed.  Returns [`InvestigationError::BatchFailed`] on any other
    /// non-recoverable error.
    async fn run(
        &self,
        batch: &InvestigationBatch,
        turn_budget: u32,
    ) -> Result<BatchOutcome, InvestigationError>;
}

// ---------------------------------------------------------------------------
// BatchedInvestigationRunner
// ---------------------------------------------------------------------------

/// Orchestrates a multi-batch investigation run over an [`InvestigationScope`].
///
/// Given a scope, a [`BatchConfig`], and a [`BatchSession`] implementation,
/// this runner:
///
/// 1. Splits the scope into batches via [`split_into_batches`].
/// 2. Dispatches each batch to the session — concurrently by default, or
///    sequentially when [`BatchConfig::sequential_batches`] is `true`.
/// 3. Degrades gracefully on [`InvestigationError::TurnBudgetExceeded`]:
///    contributes partial findings, emits a [`DiagnosticCategory::Plugin`]
///    warning diagnostic, and logs with [`tracing::warn!`].
/// 4. Returns an [`InvestigationOutcome`] that aggregates all findings and
///    diagnostics from every batch.
///
/// # Concurrency
///
/// When `config.sequential_batches == false` (the default), all batch futures
/// are driven to completion concurrently via [`futures::future::join_all`].
/// No additional Tokio tasks are spawned; all work runs within the caller's
/// task.
///
/// # Examples
///
/// ```
/// use async_trait::async_trait;
/// use xzardgz::diagnostics::Diagnostics;
/// use xzardgz::investigation::batch::{
///     BatchConfig, BatchOutcome, BatchSession, BatchedInvestigationRunner,
///     InvestigationBatch, InvestigationError,
/// };
/// use xzardgz::investigation::scope::{FileMatchEntry, InvestigationScope};
///
/// struct NoOpSession;
///
/// #[async_trait]
/// impl BatchSession for NoOpSession {
///     async fn run(
///         &self,
///         batch: &InvestigationBatch,
///         _budget: u32,
///     ) -> Result<BatchOutcome, InvestigationError> {
///         Ok(BatchOutcome {
///             batch_index: batch.index,
///             total_batches: batch.total_batches,
///             findings: vec![],
///             diagnostics: Diagnostics::new(),
///             turn_budget_exhausted: false,
///         })
///     }
/// }
/// ```
pub struct BatchedInvestigationRunner<S> {
    session: std::sync::Arc<S>,
    config: BatchConfig,
}

impl<S: BatchSession + 'static> BatchedInvestigationRunner<S> {
    /// Creates a new runner wrapping the given `session` and `config`.
    ///
    /// # Arguments
    ///
    /// * `session` - The [`BatchSession`] implementation to use for each batch.
    /// * `config` - Batch configuration (size, count, sequencing).
    ///
    /// # Returns
    ///
    /// A new [`BatchedInvestigationRunner`].
    ///
    /// # Examples
    ///
    /// ```
    /// use async_trait::async_trait;
    /// use xzardgz::diagnostics::Diagnostics;
    /// use xzardgz::investigation::batch::{
    ///     BatchConfig, BatchOutcome, BatchSession, BatchedInvestigationRunner,
    ///     InvestigationBatch, InvestigationError,
    /// };
    ///
    /// struct NoOpSession;
    ///
    /// #[async_trait]
    /// impl BatchSession for NoOpSession {
    ///     async fn run(
    ///         &self,
    ///         batch: &InvestigationBatch,
    ///         _budget: u32,
    ///     ) -> Result<BatchOutcome, InvestigationError> {
    ///         Ok(BatchOutcome {
    ///             batch_index: batch.index,
    ///             total_batches: batch.total_batches,
    ///             findings: vec![],
    ///             diagnostics: Diagnostics::new(),
    ///             turn_budget_exhausted: false,
    ///         })
    ///     }
    /// }
    ///
    /// let runner = BatchedInvestigationRunner::new(NoOpSession, BatchConfig::default());
    /// ```
    pub fn new(session: S, config: BatchConfig) -> Self {
        Self {
            session: std::sync::Arc::new(session),
            config,
        }
    }

    /// Runs investigation batches over `scope` within `turn_budget` turns per batch.
    ///
    /// Dispatches concurrently (or sequentially if
    /// [`BatchConfig::sequential_batches`] is `true`).  Exhausted batches
    /// degrade gracefully: their partial findings are included and a
    /// [`DiagnosticCategory::Plugin`] warning is appended to the returned
    /// [`InvestigationOutcome`].
    ///
    /// Returns immediately with an empty outcome when the scope is empty or
    /// when no batches are produced (e.g., `batch_size == 0`).
    ///
    /// # Arguments
    ///
    /// * `scope` - The investigation scope to analyze.
    /// * `turn_budget` - Maximum turns allowed per batch session.
    ///
    /// # Returns
    ///
    /// An [`InvestigationOutcome`] aggregating all batch results.  Never
    /// returns an error — failures are surfaced as diagnostics.
    ///
    /// # Examples
    ///
    /// ```
    /// // See struct-level doc for a complete example.
    /// ```
    pub async fn run(&self, scope: &InvestigationScope, turn_budget: u32) -> InvestigationOutcome {
        let batches = split_into_batches(scope, &self.config);
        if batches.is_empty() {
            return InvestigationOutcome::new();
        }
        if self.config.sequential_batches {
            self.run_sequential(batches, turn_budget).await
        } else {
            self.run_concurrent(batches, turn_budget).await
        }
    }

    async fn run_sequential(
        &self,
        batches: Vec<InvestigationBatch>,
        turn_budget: u32,
    ) -> InvestigationOutcome {
        let mut outcome = InvestigationOutcome::new();
        for batch in batches {
            let batch_result = resolve_batch_outcome(&*self.session, batch, turn_budget).await;
            outcome.merge_batch_outcome(batch_result);
        }
        outcome
    }

    async fn run_concurrent(
        &self,
        batches: Vec<InvestigationBatch>,
        turn_budget: u32,
    ) -> InvestigationOutcome {
        let futures: Vec<
            std::pin::Pin<Box<dyn std::future::Future<Output = BatchOutcome> + Send>>,
        > = batches
            .into_iter()
            .map(|batch| {
                let session = std::sync::Arc::clone(&self.session);
                let fut: std::pin::Pin<Box<dyn std::future::Future<Output = BatchOutcome> + Send>> =
                    Box::pin(
                        async move { resolve_batch_outcome(&*session, batch, turn_budget).await },
                    );
                fut
            })
            .collect();

        let results = futures::future::join_all(futures).await;
        let mut outcome = InvestigationOutcome::new();
        for batch_result in results {
            outcome.merge_batch_outcome(batch_result);
        }
        outcome
    }
}

// ---------------------------------------------------------------------------
// resolve_batch_outcome
// ---------------------------------------------------------------------------

/// Executes one batch session and converts any [`InvestigationError`] into a
/// [`BatchOutcome`] with an attached warning [`Diagnostic`].
///
/// This is the degrade-and-warn integration point:
/// - On [`InvestigationError::TurnBudgetExceeded`]: contributes partial
///   findings, records a `Plugin`-category warning diagnostic, and emits a
///   `tracing::warn!` log message.
/// - On [`InvestigationError::BatchFailed`]: records an empty outcome with a
///   `Plugin`-category warning diagnostic, and emits a `tracing::warn!`.
/// - On `Ok(BatchOutcome)`: passes the outcome through unchanged.
async fn resolve_batch_outcome<S: BatchSession>(
    session: &S,
    batch: InvestigationBatch,
    turn_budget: u32,
) -> BatchOutcome {
    let total_batches = batch.total_batches;
    match session.run(&batch, turn_budget).await {
        Ok(outcome) => outcome,
        Err(InvestigationError::TurnBudgetExceeded {
            batch_index,
            turn_limit,
            partial_findings,
        }) => {
            let msg = format!(
                "investigation batch {} exhausted its turn budget of {} turns; \
                 findings from this batch may be incomplete",
                batch_index, turn_limit
            );
            tracing::warn!("{}", msg);
            let mut diagnostics = Diagnostics::new();
            diagnostics.push_warning_with_context(
                DiagnosticCategory::Plugin,
                msg,
                format!("batch_{}", batch_index),
            );
            BatchOutcome {
                batch_index,
                total_batches,
                findings: partial_findings,
                diagnostics,
                turn_budget_exhausted: true,
            }
        }
        Err(InvestigationError::BatchFailed {
            batch_index,
            message,
        }) => {
            let msg = format!("investigation batch {} failed: {}", batch_index, message);
            tracing::warn!("{}", msg);
            let mut diagnostics = Diagnostics::new();
            diagnostics.push_warning_with_context(
                DiagnosticCategory::Plugin,
                msg,
                format!("batch_{}", batch_index),
            );
            BatchOutcome {
                batch_index,
                total_batches,
                findings: vec![],
                diagnostics,
                turn_budget_exhausted: false,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use crate::diagnostics::{DiagnosticCategory, DiagnosticLevel};
    use crate::watcher::event_type::WatcherEventType;
    use crate::watcher::result::{WATCHER_RESULT_VERSION, WatcherResultMessage};

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Builds a scope with `n` entries named `file_0.rs` .. `file_{n-1}.rs`.
    fn make_scope(n: usize) -> InvestigationScope {
        let mut scope = InvestigationScope::new();
        for i in 0..n {
            scope.insert(FileMatchEntry::new(format!("file_{i}.rs")));
        }
        scope
    }

    // ------------------------------------------------------------------
    // BatchConfig
    // ------------------------------------------------------------------

    #[test]
    fn test_batch_config_new_sets_all_fields() {
        let config = BatchConfig::new(5, 10, 2);
        assert_eq!(config.max_batches, 5);
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.clean_verification_turns, 2);
    }

    #[test]
    fn test_batch_config_default_has_expected_values() {
        let config = BatchConfig::default();
        assert_eq!(config.max_batches, 10);
        assert_eq!(config.batch_size, 20);
        assert_eq!(config.clean_verification_turns, 1);
    }

    #[test]
    fn test_batch_config_new_with_zeros_accepts_edge_values() {
        let config = BatchConfig::new(0, 0, 0);
        assert_eq!(config.max_batches, 0);
        assert_eq!(config.batch_size, 0);
        assert_eq!(config.clean_verification_turns, 0);
    }

    // ------------------------------------------------------------------
    // split_into_batches - empty / guard cases
    // ------------------------------------------------------------------

    #[test]
    fn test_split_into_batches_empty_scope_returns_empty_vec() {
        let scope = InvestigationScope::new();
        let config = BatchConfig::default();
        let batches = split_into_batches(&scope, &config);
        assert!(batches.is_empty());
    }

    #[test]
    fn test_split_into_batches_batch_size_zero_returns_empty_vec() {
        let scope = make_scope(5);
        let config = BatchConfig::new(10, 0, 1);
        let batches = split_into_batches(&scope, &config);
        assert!(batches.is_empty());
    }

    // ------------------------------------------------------------------
    // split_into_batches - single batch
    // ------------------------------------------------------------------

    #[test]
    fn test_split_into_batches_single_batch_when_files_lte_batch_size() {
        let scope = make_scope(5);
        let config = BatchConfig::new(10, 20, 1);
        let batches = split_into_batches(&scope, &config);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].scope.len(), 5);
    }

    #[test]
    fn test_split_into_batches_exact_batch_size_boundary_produces_one_batch() {
        let scope = make_scope(20);
        let config = BatchConfig::new(10, 20, 1);
        let batches = split_into_batches(&scope, &config);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].scope.len(), 20);
    }

    // ------------------------------------------------------------------
    // split_into_batches - multiple batches
    // ------------------------------------------------------------------

    #[test]
    fn test_split_into_batches_multiple_batches_when_files_gt_batch_size() {
        let scope = make_scope(7);
        let config = BatchConfig::new(10, 3, 1);
        let batches = split_into_batches(&scope, &config);
        // ceil(7/3) = 3
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].scope.len(), 3);
        assert_eq!(batches[1].scope.len(), 3);
        assert_eq!(batches[2].scope.len(), 1);
    }

    #[test]
    fn test_split_into_batches_caps_at_max_batches() {
        // 100 files, batch_size=5 would normally give 20 batches, but max=4
        let scope = make_scope(100);
        let config = BatchConfig::new(4, 5, 1);
        let batches = split_into_batches(&scope, &config);
        assert_eq!(batches.len(), 4);
        // Each of the 4 batches should have 5 files
        for batch in &batches {
            assert_eq!(batch.scope.len(), 5);
        }
    }

    // ------------------------------------------------------------------
    // split_into_batches - index / total_batches fields
    // ------------------------------------------------------------------

    #[test]
    fn test_split_into_batches_index_and_total_batches_are_correct() {
        let scope = make_scope(5);
        let config = BatchConfig::new(10, 3, 1);
        let batches = split_into_batches(&scope, &config);
        // ceil(5/3) = 2 batches
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].index, 0);
        assert_eq!(batches[0].total_batches, 2);
        assert_eq!(batches[1].index, 1);
        assert_eq!(batches[1].total_batches, 2);
    }

    // ------------------------------------------------------------------
    // split_into_batches - determinism
    // ------------------------------------------------------------------

    #[test]
    fn test_split_into_batches_files_are_sorted_deterministically() {
        let mut scope = InvestigationScope::new();
        // Insert in non-alphabetical order.
        scope.insert(FileMatchEntry::new("c.rs"));
        scope.insert(FileMatchEntry::new("a.rs"));
        scope.insert(FileMatchEntry::new("b.rs"));
        let config = BatchConfig::new(10, 2, 1);
        let batches = split_into_batches(&scope, &config);
        // First batch should contain a.rs and b.rs (sorted).
        assert_eq!(batches.len(), 2);
        // SAFETY: batch is non-empty by construction
        let first_batch_paths = batches[0].scope.paths();
        assert_eq!(first_batch_paths, vec!["a.rs", "b.rs"]);
        let second_batch_paths = batches[1].scope.paths();
        assert_eq!(second_batch_paths, vec!["c.rs"]);
    }

    #[test]
    fn test_split_into_batches_two_calls_produce_identical_batches() {
        let scope = make_scope(10);
        let config = BatchConfig::new(10, 3, 1);
        let batches_a = split_into_batches(&scope, &config);
        let batches_b = split_into_batches(&scope, &config);
        assert_eq!(batches_a.len(), batches_b.len());
        for (a, b) in batches_a.iter().zip(batches_b.iter()) {
            assert_eq!(a.scope.paths(), b.scope.paths());
        }
    }

    // ------------------------------------------------------------------
    // compute_investigation_turns
    // ------------------------------------------------------------------

    #[test]
    fn test_compute_investigation_turns_empty_scope_returns_zero() {
        let scope = InvestigationScope::new();
        let config = BatchConfig::default();
        assert_eq!(compute_investigation_turns(&scope, &config), 0);
    }

    #[test]
    fn test_compute_investigation_turns_batch_size_zero_returns_zero() {
        let scope = make_scope(10);
        let config = BatchConfig::new(10, 0, 1);
        assert_eq!(compute_investigation_turns(&scope, &config), 0);
    }

    #[test]
    fn test_compute_investigation_turns_small_scope_returns_one() {
        let scope = make_scope(5);
        let config = BatchConfig::new(10, 20, 1);
        assert_eq!(compute_investigation_turns(&scope, &config), 1);
    }

    #[test]
    fn test_compute_investigation_turns_exact_boundary_returns_one() {
        let scope = make_scope(20);
        let config = BatchConfig::new(10, 20, 1);
        assert_eq!(compute_investigation_turns(&scope, &config), 1);
    }

    #[test]
    fn test_compute_investigation_turns_one_over_boundary_returns_two() {
        let scope = make_scope(21);
        let config = BatchConfig::new(10, 20, 1);
        assert_eq!(compute_investigation_turns(&scope, &config), 2);
    }

    #[test]
    fn test_compute_investigation_turns_large_scope_is_capped_by_max_batches() {
        // 100 files / batch_size 5 = 20 raw, capped to 10
        let scope = make_scope(100);
        let config = BatchConfig::new(10, 5, 1);
        assert_eq!(compute_investigation_turns(&scope, &config), 10);
    }

    #[test]
    fn test_compute_investigation_turns_matches_split_into_batches_len() {
        let scope = make_scope(37);
        let config = BatchConfig::new(8, 7, 1);
        let turns = compute_investigation_turns(&scope, &config);
        let batches = split_into_batches(&scope, &config);
        assert_eq!(turns, batches.len());
    }

    // ------------------------------------------------------------------
    // BatchConfig sequential_batches
    // ------------------------------------------------------------------

    #[test]
    fn test_batch_config_sequential_batches_defaults_to_false_via_new() {
        let config = BatchConfig::new(5, 10, 2);
        assert!(!config.sequential_batches);
    }

    #[test]
    fn test_batch_config_sequential_batches_defaults_to_false_via_default() {
        let config = BatchConfig::default();
        assert!(!config.sequential_batches);
    }

    #[test]
    fn test_batch_config_with_sequential_true_sets_flag() {
        let config = BatchConfig::default().with_sequential(true);
        assert!(config.sequential_batches);
    }

    #[test]
    fn test_batch_config_with_sequential_false_clears_flag() {
        let config = BatchConfig::default()
            .with_sequential(true)
            .with_sequential(false);
        assert!(!config.sequential_batches);
    }

    // ------------------------------------------------------------------
    // InvestigationError
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_error_turn_budget_exceeded_display_contains_batch_and_limit() {
        let err = InvestigationError::TurnBudgetExceeded {
            batch_index: 2,
            turn_limit: 15,
            partial_findings: vec![],
        };
        let msg = err.to_string();
        assert!(msg.contains("2"), "message must contain batch_index: {msg}");
        assert!(msg.contains("15"), "message must contain turn_limit: {msg}");
    }

    #[test]
    fn test_investigation_error_batch_failed_display_contains_message() {
        let err = InvestigationError::BatchFailed {
            batch_index: 0,
            message: "provider timeout".to_string(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("provider timeout"),
            "display must contain message: {msg}"
        );
    }

    #[test]
    fn test_investigation_error_turn_budget_exceeded_preserves_partial_findings() {
        let err = InvestigationError::TurnBudgetExceeded {
            batch_index: 0,
            turn_limit: 5,
            partial_findings: vec!["p1".to_string(), "p2".to_string()],
        };
        if let InvestigationError::TurnBudgetExceeded {
            partial_findings, ..
        } = err
        {
            assert_eq!(partial_findings.len(), 2);
            assert_eq!(partial_findings[0], "p1");
        } else {
            panic!("wrong variant");
        }
    }

    // ------------------------------------------------------------------
    // BatchOutcome
    // ------------------------------------------------------------------

    #[test]
    fn test_batch_outcome_turn_budget_exhausted_false_by_default_construction() {
        use crate::diagnostics::Diagnostics;
        let outcome = BatchOutcome {
            batch_index: 0,
            total_batches: 1,
            findings: vec![],
            diagnostics: Diagnostics::new(),
            turn_budget_exhausted: false,
        };
        assert!(!outcome.turn_budget_exhausted);
    }

    #[test]
    fn test_batch_outcome_with_findings_stores_all_findings() {
        use crate::diagnostics::Diagnostics;
        let outcome = BatchOutcome {
            batch_index: 1,
            total_batches: 3,
            findings: vec!["a".to_string(), "b".to_string()],
            diagnostics: Diagnostics::new(),
            turn_budget_exhausted: false,
        };
        assert_eq!(outcome.findings.len(), 2);
    }

    // ------------------------------------------------------------------
    // InvestigationOutcome
    // ------------------------------------------------------------------

    #[test]
    fn test_investigation_outcome_new_is_empty_and_not_partial() {
        let outcome = InvestigationOutcome::new();
        assert!(outcome.is_empty());
        assert!(!outcome.is_partial());
        assert_eq!(outcome.batches_completed, 0);
        assert_eq!(outcome.batches_exhausted, 0);
    }

    #[test]
    fn test_investigation_outcome_merge_non_exhausted_batch_increments_completed() {
        use crate::diagnostics::Diagnostics;
        let mut outcome = InvestigationOutcome::new();
        outcome.merge_batch_outcome(BatchOutcome {
            batch_index: 0,
            total_batches: 2,
            findings: vec!["f1".to_string()],
            diagnostics: Diagnostics::new(),
            turn_budget_exhausted: false,
        });
        assert_eq!(outcome.batches_completed, 1);
        assert_eq!(outcome.batches_exhausted, 0);
        assert_eq!(outcome.findings.len(), 1);
        assert!(!outcome.is_partial());
    }

    #[test]
    fn test_investigation_outcome_merge_exhausted_batch_increments_exhausted_counter() {
        use crate::diagnostics::Diagnostics;
        let mut outcome = InvestigationOutcome::new();
        outcome.merge_batch_outcome(BatchOutcome {
            batch_index: 0,
            total_batches: 1,
            findings: vec![],
            diagnostics: Diagnostics::new(),
            turn_budget_exhausted: true,
        });
        assert_eq!(outcome.batches_exhausted, 1);
        assert_eq!(outcome.batches_completed, 0);
        assert!(outcome.is_partial());
    }

    #[test]
    fn test_investigation_outcome_merge_multiple_batches_accumulates_findings() {
        use crate::diagnostics::Diagnostics;
        let mut outcome = InvestigationOutcome::new();
        for i in 0..3_usize {
            outcome.merge_batch_outcome(BatchOutcome {
                batch_index: i,
                total_batches: 3,
                findings: vec![format!("finding_{i}")],
                diagnostics: Diagnostics::new(),
                turn_budget_exhausted: false,
            });
        }
        assert_eq!(outcome.findings.len(), 3);
        assert_eq!(outcome.batches_completed, 3);
        assert!(!outcome.is_partial());
    }

    #[test]
    fn test_investigation_outcome_into_watcher_diagnostics_returns_entries() {
        use crate::diagnostics::Diagnostics;
        let mut outcome = InvestigationOutcome::new();
        let mut diags = Diagnostics::new();
        diags.push_warning(DiagnosticCategory::Plugin, "batch exhausted");
        outcome.merge_batch_outcome(BatchOutcome {
            batch_index: 0,
            total_batches: 1,
            findings: vec![],
            diagnostics: diags,
            turn_budget_exhausted: true,
        });
        let watcher_diags = outcome.into_watcher_diagnostics();
        assert_eq!(watcher_diags.len(), 1);
        assert_eq!(watcher_diags[0].category, DiagnosticCategory::Plugin);
    }

    // ------------------------------------------------------------------
    // BatchedInvestigationRunner — mock sessions
    // ------------------------------------------------------------------

    struct SuccessSession;

    #[async_trait::async_trait]
    impl BatchSession for SuccessSession {
        async fn run(
            &self,
            batch: &InvestigationBatch,
            _turn_budget: u32,
        ) -> Result<BatchOutcome, InvestigationError> {
            use crate::diagnostics::Diagnostics;
            Ok(BatchOutcome {
                batch_index: batch.index,
                total_batches: batch.total_batches,
                findings: vec![format!("finding_from_batch_{}", batch.index)],
                diagnostics: Diagnostics::new(),
                turn_budget_exhausted: false,
            })
        }
    }

    struct ExhaustingSession;

    #[async_trait::async_trait]
    impl BatchSession for ExhaustingSession {
        async fn run(
            &self,
            batch: &InvestigationBatch,
            turn_budget: u32,
        ) -> Result<BatchOutcome, InvestigationError> {
            Err(InvestigationError::TurnBudgetExceeded {
                batch_index: batch.index,
                turn_limit: turn_budget,
                partial_findings: vec![format!("partial_from_batch_{}", batch.index)],
            })
        }
    }

    struct FailingSession;

    #[async_trait::async_trait]
    impl BatchSession for FailingSession {
        async fn run(
            &self,
            batch: &InvestigationBatch,
            _turn_budget: u32,
        ) -> Result<BatchOutcome, InvestigationError> {
            Err(InvestigationError::BatchFailed {
                batch_index: batch.index,
                message: "simulated provider failure".to_string(),
            })
        }
    }

    // ------------------------------------------------------------------
    // BatchedInvestigationRunner — concurrent (default)
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_batched_runner_with_success_session_all_batches_complete() {
        let scope = make_scope(6);
        let config = BatchConfig::new(10, 3, 1); // 2 batches
        let runner = BatchedInvestigationRunner::new(SuccessSession, config);
        let outcome = runner.run(&scope, 10).await;
        assert_eq!(outcome.batches_completed, 2);
        assert_eq!(outcome.batches_exhausted, 0);
        assert!(!outcome.is_partial());
        assert_eq!(outcome.findings.len(), 2); // one finding per batch
        assert!(outcome.diagnostics.is_empty());
    }

    #[tokio::test]
    async fn test_batched_runner_with_exhausting_session_completes_without_error() {
        // The run must succeed (no panic, no Err) even when batches exhaust budget.
        let scope = make_scope(6);
        let config = BatchConfig::new(10, 3, 1);
        let runner = BatchedInvestigationRunner::new(ExhaustingSession, config);
        let outcome = runner.run(&scope, 5).await;
        // Run completed — not an Err.
        assert_eq!(outcome.batches_exhausted, 2);
        assert_eq!(outcome.batches_completed, 0);
        assert!(outcome.is_partial());
    }

    #[tokio::test]
    async fn test_batched_runner_exhausted_batch_emits_plugin_category_diagnostic() {
        let scope = make_scope(3);
        let config = BatchConfig::new(10, 3, 1); // 1 batch
        let runner = BatchedInvestigationRunner::new(ExhaustingSession, config);
        let outcome = runner.run(&scope, 5).await;
        assert_eq!(outcome.diagnostics.len(), 1);
        let diag = &outcome.diagnostics.entries[0];
        assert_eq!(diag.category, DiagnosticCategory::Plugin);
        assert_eq!(diag.level, DiagnosticLevel::Warning);
    }

    #[tokio::test]
    async fn test_batched_runner_exhausted_batch_diagnostic_message_contains_batch_index_and_limit()
    {
        let scope = make_scope(3);
        let config = BatchConfig::new(10, 3, 1);
        let runner = BatchedInvestigationRunner::new(ExhaustingSession, config);
        let outcome = runner.run(&scope, 7).await;
        let msg = &outcome.diagnostics.entries[0].message;
        assert!(msg.contains("0"), "message must reference batch 0: {msg}");
        assert!(
            msg.contains("7"),
            "message must reference turn_limit 7: {msg}"
        );
    }

    #[tokio::test]
    async fn test_batched_runner_exhausted_batch_partial_findings_are_included() {
        let scope = make_scope(3);
        let config = BatchConfig::new(10, 3, 1);
        let runner = BatchedInvestigationRunner::new(ExhaustingSession, config);
        let outcome = runner.run(&scope, 5).await;
        // ExhaustingSession includes partial findings even on exhaustion.
        assert!(!outcome.findings.is_empty());
        assert!(outcome.findings[0].starts_with("partial_from_batch_"));
    }

    #[tokio::test]
    async fn test_batched_runner_failing_session_completes_with_warning_diagnostic() {
        let scope = make_scope(3);
        let config = BatchConfig::new(10, 3, 1);
        let runner = BatchedInvestigationRunner::new(FailingSession, config);
        let outcome = runner.run(&scope, 10).await;
        // BatchFailed does NOT count as exhausted.
        assert_eq!(outcome.batches_exhausted, 0);
        assert!(!outcome.diagnostics.is_empty());
        assert_eq!(
            outcome.diagnostics.entries[0].category,
            DiagnosticCategory::Plugin
        );
    }

    #[tokio::test]
    async fn test_batched_runner_empty_scope_returns_empty_outcome() {
        let scope = InvestigationScope::new();
        let runner = BatchedInvestigationRunner::new(SuccessSession, BatchConfig::default());
        let outcome = runner.run(&scope, 10).await;
        assert!(outcome.is_empty());
        assert_eq!(outcome.batches_completed, 0);
    }

    // ------------------------------------------------------------------
    // BatchedInvestigationRunner — sequential mode
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_batched_runner_sequential_mode_completes_all_batches() {
        let scope = make_scope(6);
        let config = BatchConfig::new(10, 3, 1).with_sequential(true);
        let runner = BatchedInvestigationRunner::new(SuccessSession, config);
        let outcome = runner.run(&scope, 10).await;
        assert_eq!(outcome.batches_completed, 2);
        assert!(!outcome.is_partial());
    }

    #[tokio::test]
    async fn test_batched_runner_sequential_exhausted_produces_same_diagnostics_as_concurrent() {
        let scope = make_scope(3);
        let seq_config = BatchConfig::new(10, 3, 1).with_sequential(true);
        let con_config = BatchConfig::new(10, 3, 1);

        let seq_runner = BatchedInvestigationRunner::new(ExhaustingSession, seq_config);
        let con_runner = BatchedInvestigationRunner::new(ExhaustingSession, con_config);

        let seq_outcome = seq_runner.run(&scope, 5).await;
        let con_outcome = con_runner.run(&scope, 5).await;

        assert_eq!(seq_outcome.batches_exhausted, con_outcome.batches_exhausted);
        assert_eq!(seq_outcome.diagnostics.len(), con_outcome.diagnostics.len());
    }

    // ------------------------------------------------------------------
    // Phase 2.4 watcher integration test
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_batched_runner_diagnostics_flow_into_watcher_result_message() {
        // This test demonstrates the full degrade-and-warn path:
        // 1. Runner produces an InvestigationOutcome with a budget-exhaustion diagnostic.
        // 2. The diagnostic drains into WatcherResultMessage.diagnostics via
        //    into_watcher_diagnostics().
        // 3. The message's diagnostics vec contains the expected Plugin-category warning.
        let scope = make_scope(3);
        let config = BatchConfig::new(10, 3, 1);
        let runner = BatchedInvestigationRunner::new(ExhaustingSession, config);
        let outcome = runner.run(&scope, 5).await;

        // Verify the outcome itself carries the diagnostic.
        assert!(!outcome.diagnostics.is_empty());
        assert_eq!(
            outcome.diagnostics.entries[0].category,
            DiagnosticCategory::Plugin
        );
        assert_eq!(
            outcome.diagnostics.entries[0].level,
            DiagnosticLevel::Warning
        );

        // Drain diagnostics into a WatcherResultMessage.
        let watcher_diags = outcome.into_watcher_diagnostics();

        let started = chrono::Utc::now();
        let mut result_message = WatcherResultMessage::new(
            "result-001",
            WatcherEventType::SecurityReviewResult,
            "xzardgz://test",
            "https://github.com/org/repo",
            "security_review",
            "ws-test",
            "corr-test",
            "task-test",
            started,
        );
        result_message.diagnostics.extend(watcher_diags);

        // Verify the version constant is accessible from the result module.
        assert!(!result_message.version.is_empty());
        let _ = WATCHER_RESULT_VERSION;

        // The watcher result message now carries the budget-exhaustion diagnostic.
        assert!(
            result_message
                .diagnostics
                .iter()
                .any(|d| d.category == DiagnosticCategory::Plugin
                    && d.level == DiagnosticLevel::Warning),
            "WatcherResultMessage.diagnostics must contain the plugin-category warning"
        );
    }
}
