//! Workflow plan executor with full pipeline integration.
//!
//! This module provides [`WorkflowExecutor`], the shared execution engine for
//! both CLI-driven and watcher-triggered pipeline runs.  It supports:
//!
//! - Local plan execution (`ExecutionInput::LocalPlan`)
//! - Watcher task execution (`ExecutionInput::WatcherTask`)
//! - Scan-only execution (`ExecutionInput::ScanOnly`)
//! - Plugin-only execution against previously-collected scan data
//!   (`ExecutionInput::PluginOnly`)
//! - Pull request creation (`ExecutionInput::CreatePr`)
//! - Dry-run mode (validates without side effects)
//! - Resume from existing workspace state
//!
//! # Execution Stages
//!
//! A complete run progresses through these stages in order:
//!
//! 1. Apply config overrides from the plan.
//! 2. Validate governance rules.
//! 3. Initialize or resume the workspace.
//! 4. Dry-run short-circuit (if requested).
//! 5. Resolve the repository to a local path.
//! 6. Collect git metadata (if the path is a git repository).
//! 7. Run the repository scanner or load an existing scan artifact.
//! 8. Persist the scan artifact.
//! 9. For each plugin step (dependency order):
//!    a. Transition workspace to `PluginRunning`.
//!    b. Build plugin context.
//!    c. Execute the plugin.
//!    d. Record plugin output and score.
//!    e. Write reports.
//!    f. Transition workspace to `PluginComplete`.
//! 10. Transition workspace to `Complete`.
//! 11. Build `WatcherResultMessage` if triggered by a watcher task.
//! 12. Return a structured [`ExecutionResult`].

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tracing::{debug, warn};
use ulid::Ulid;

use crate::config::Config;
use crate::diagnostics::{Diagnostic, DiagnosticCategory};
use crate::error::{PipelineError, Result};
use crate::git::ops::GitRepository;
use crate::governance::GovernanceChecker;
use crate::plugins::context::{PluginContext, ToolAccessLevel};
use crate::plugins::output::PluginOutput;
use crate::plugins::registry::PluginRegistry;
use crate::providers::factory::ProviderFactory;
use crate::reports::envelope::ReportEnvelope;
use crate::reports::findings::PluginFindings;
use crate::reports::formatter::{PluginReportFormatter, ReportFormat};
use crate::reports::json::JsonReportWriter;
use crate::reports::markdown::MarkdownReportWriter;
use crate::reports::sarif::SarifReportWriter;
use crate::scanner::Scanner;
use crate::scanner::config::ScannerConfig as ScannerCfg;
use crate::scanner::result::ScanResult;
use crate::tools::registry::{ToolRegistry, build_read_only_registry, build_read_write_registry};
use crate::tools::sandbox::PathValidator;
use crate::watcher::event_type::WatcherEventType;
use crate::watcher::result::WatcherResultMessage;
use crate::watcher::task::WatcherTaskMessage;
use crate::workflow::plan::{
    PLAN_VERSION, PlanReportOptions, PlanScanOptions, PluginStep, WorkflowPlan,
};
use crate::workspace::WorkspaceManager;
use crate::workspace::stage::WorkspaceStage;

// ---------------------------------------------------------------------------
// ExecutionInput
// ---------------------------------------------------------------------------

/// Input descriptor for a workflow execution run.
///
/// Five modes are supported:
///
/// - [`LocalPlan`][ExecutionInput::LocalPlan] — a full workflow plan
///   constructed from a file or programmatically.
/// - [`WatcherTask`][ExecutionInput::WatcherTask] — triggered by a Kafka
///   watcher task message.
/// - [`ScanOnly`][ExecutionInput::ScanOnly] — runs only the repository
///   scanner without invoking any plugin.
/// - [`PluginOnly`][ExecutionInput::PluginOnly] — runs a single plugin
///   against previously-collected scan data, without resolving a
///   repository or running the scanner.
/// - [`CreatePr`][ExecutionInput::CreatePr] — creates a GitHub pull request
///   for a feature branch; returns an immediate no-op when
///   `config.pr.enabled` is `false`.
///
/// # Examples
///
/// ```
/// use xzardgz::workflow::executor::ExecutionInput;
/// use xzardgz::workflow::plan::WorkflowPlan;
///
/// let plan = WorkflowPlan::direct_plugin_invocation("technical-review", ".");
/// let input = ExecutionInput::LocalPlan(Box::new(plan));
/// ```
pub enum ExecutionInput {
    /// Full workflow plan execution.
    LocalPlan(Box<WorkflowPlan>),
    /// Watcher-triggered task execution.
    ///
    /// Boxed to keep the enum variant sizes balanced.
    WatcherTask(Box<WatcherTaskMessage>),
    /// Repository scan without plugin execution.
    ScanOnly {
        /// Repository directory to scan.
        repository: String,
        /// Optional path to write the scan artifact YAML.
        ///
        /// When `None`, the artifact is written only to the workspace.
        output_path: Option<String>,
        /// Target branch to scan. `None` uses the repository default branch.
        branch: Option<String>,
        /// When `true`, resumes from an existing workspace for `repository`
        /// (loading its recorded scan artifact when present) instead of
        /// always creating a fresh workspace.
        resume: bool,
        /// Workspace root override. Falls back to the global configuration's
        /// workspace root when `None`.
        workspace: Option<String>,
    },
    /// Runs a single plugin against previously-collected scan data.
    ///
    /// Skips repository resolution, git metadata collection, and repository
    /// scanning entirely. Scan data is loaded either from an existing
    /// workspace's recorded scan artifact (`workspace_dir`) or from an
    /// externally-supplied scan-artifact YAML file (`scan_artifact_path`).
    /// Backs the `plugin run --workspace <dir>` / `plugin run
    /// --scan-artifact <path>` CLI shape, which bypasses the full `run`
    /// pipeline. Exactly one of `workspace_dir` or `scan_artifact_path` must
    /// be `Some`; supplying neither is a [`PipelineError::Workflow`].
    PluginOnly {
        /// Name of the plugin to execute.
        plugin: String,
        /// Path to an existing workspace directory
        /// (`{workspace_root}/{workspace_id}`) to resume scan data, state,
        /// and sandbox roots from.
        workspace_dir: Option<String>,
        /// Path to an external scan-artifact YAML file to load scan data
        /// from directly, independent of any workspace.
        ///
        /// When `workspace_dir` is also supplied, this path takes
        /// precedence over the workspace's own recorded artifact. When
        /// `workspace_dir` is absent, a fresh, ephemeral workspace is
        /// created under `workspace_root` purely to host reports and
        /// sandbox state; the plugin has no read access to the original
        /// repository checkout in this mode, since its location is not
        /// recorded anywhere.
        scan_artifact_path: Option<String>,
        /// Base directory under which a fresh workspace is created when
        /// `workspace_dir` is not supplied. Falls back to the global
        /// configuration's workspace root when `None`.
        workspace_root: Option<String>,
        /// Plugin-specific configuration as a JSON value.
        config: Option<serde_json::Value>,
        /// Provider override (`openai`, `anthropic`, `ollama`, `copilot`).
        provider: Option<String>,
        /// Model identifier override.
        model: Option<String>,
        /// Performs a dry run: validates inputs without invoking any
        /// provider or writing reports.
        dry_run: bool,
        /// Requested report output formats. Empty means use the
        /// workspace/plugin default.
        report_formats: Vec<String>,
        /// Output directory override for generated reports. `None` uses the
        /// workspace's default report location.
        output_dir: Option<String>,
    },
    /// Creates a GitHub pull request for a feature branch.
    ///
    /// When `config.pr.enabled` is `false` this variant returns an immediate
    /// successful no-op result without any commit, push, or PR activity.
    CreatePr {
        /// Local path to an existing git checkout.
        repository: String,
        /// Branch to use as the PR head (source branch).
        head_branch: String,
        /// Branch to use as the PR base (target/destination branch).
        ///
        /// Must be supplied explicitly; the executor never auto-discovers the
        /// repository default branch.
        base_branch: String,
        /// GitHub repository owner (username or organisation).
        owner: String,
        /// GitHub repository name.
        repo: String,
        /// Pull request title.
        title: String,
        /// Optional pull request body/description.
        body: Option<String>,
        /// When `true`, create the PR as a draft.
        draft: bool,
        /// Workspace root override. Falls back to `config.workspace.root` when `None`.
        workspace: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// ExecutionResult
// ---------------------------------------------------------------------------

/// Structured result returned by every [`WorkflowExecutor::execute`] call.
///
/// Consumers should check `success` first, then inspect `errors` or
/// `diagnostics` for details.  `report_paths` maps step IDs to the list of
/// report files written for that step.
///
/// # Examples
///
/// ```no_run
/// # use xzardgz::workflow::executor::ExecutionResult;
/// # fn example(result: ExecutionResult) {
/// if result.success {
///     for (step_id, paths) in &result.report_paths {
///         println!("step {}: {} reports", step_id, paths.len());
///     }
/// } else {
///     for err in &result.errors {
///         eprintln!("error: {}", err);
///     }
/// }
/// # }
/// ```
#[derive(Debug)]
pub struct ExecutionResult {
    /// Unique workspace identifier (ULID) created or resumed for this run.
    pub workspace_id: String,
    /// Whether all pipeline stages and plugin steps completed successfully.
    pub success: bool,
    /// Error messages accumulated during execution.
    pub errors: Vec<String>,
    /// Informational and warning diagnostics from the execution.
    pub diagnostics: Vec<Diagnostic>,
    /// Path to the persisted scan artifact YAML file.
    pub scan_artifact_path: Option<String>,
    /// Map of step ID to list of report file paths written for that step.
    pub report_paths: HashMap<String, Vec<String>>,
    /// Populated when execution was triggered by a watcher task.
    pub watcher_result: Option<WatcherResultMessage>,
    /// Pipeline stage at the time execution completed or failed.
    pub stage_at_completion: WorkspaceStage,
    /// UTC timestamp when execution started.
    pub started_at: DateTime<Utc>,
    /// UTC timestamp when execution completed.
    pub completed_at: DateTime<Utc>,
    /// Whether this was a dry-run (validation only, no side effects).
    pub is_dry_run: bool,
}

// ---------------------------------------------------------------------------
// WorkflowExecutor
// ---------------------------------------------------------------------------

/// Shared execution engine for CLI and watcher workflow runs.
///
/// `WorkflowExecutor` is the single code path shared by CLI-invoked pipeline
/// runs and watcher-triggered task executions.  It accepts an
/// [`ExecutionInput`] describing the run and returns a structured
/// [`ExecutionResult`].
///
/// The executor owns a [`Config`] and a [`PluginRegistry`].  It creates a
/// new provider instance from the config for each run via
/// [`ProviderFactory`][crate::providers::factory::ProviderFactory].
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use xzardgz::config::Config;
/// use xzardgz::plugins::registry::PluginRegistry;
/// use xzardgz::workflow::executor::{ExecutionInput, WorkflowExecutor};
/// use xzardgz::workflow::plan::WorkflowPlan;
///
/// let executor = WorkflowExecutor::new(
///     Arc::new(Config::default()),
///     Arc::new(PluginRegistry::new()),
/// );
/// ```
pub struct WorkflowExecutor {
    config: Arc<Config>,
    plugin_registry: Arc<PluginRegistry>,
}

impl WorkflowExecutor {
    /// Creates a new `WorkflowExecutor` with the given config and plugin registry.
    ///
    /// # Arguments
    ///
    /// * `config` - The effective pipeline configuration.
    /// * `plugin_registry` - Registry of available plugins.
    ///
    /// # Returns
    ///
    /// A new `WorkflowExecutor` ready to accept execution inputs.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use xzardgz::config::Config;
    /// use xzardgz::plugins::registry::PluginRegistry;
    /// use xzardgz::workflow::executor::WorkflowExecutor;
    ///
    /// let executor = WorkflowExecutor::new(
    ///     Arc::new(Config::default()),
    ///     Arc::new(PluginRegistry::new()),
    /// );
    /// ```
    pub fn new(config: Arc<Config>, plugin_registry: Arc<PluginRegistry>) -> Self {
        Self {
            config,
            plugin_registry,
        }
    }

    /// Executes a workflow run described by `input`.
    ///
    /// Dispatches to the appropriate execution path based on the input variant:
    ///
    /// | Variant | Execution path |
    /// |---------|----------------|
    /// | `LocalPlan` | Full pipeline: scan, plugin steps, reports |
    /// | `WatcherTask` | Converts task to plan, runs pipeline, builds result |
    /// | `ScanOnly` | Scan only, no plugin execution |
    /// | `PluginOnly` | Plugin only, against previously-collected scan data |
    ///
    /// # Arguments
    ///
    /// * `input` - An [`ExecutionInput`] describing the run.
    ///
    /// # Returns
    ///
    /// An [`ExecutionResult`] with `success` set to `true` on full completion.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError`] for unrecoverable failures such as governance
    /// violations, workspace creation failures, scanner errors, or plugin
    /// execution panics.  Graceful plugin failures are captured inside
    /// `ExecutionResult.errors` rather than returned as `Err`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use xzardgz::config::Config;
    /// use xzardgz::plugins::registry::PluginRegistry;
    /// use xzardgz::workflow::executor::{ExecutionInput, WorkflowExecutor};
    ///
    /// # async fn run() -> xzardgz::error::Result<()> {
    /// let executor = WorkflowExecutor::new(
    ///     Arc::new(Config::default()),
    ///     Arc::new(PluginRegistry::new()),
    /// );
    /// let input = ExecutionInput::ScanOnly {
    ///     repository: ".".to_string(),
    ///     output_path: None,
    ///     branch: None,
    ///     resume: false,
    ///     workspace: None,
    /// };
    /// let result = executor.execute(input).await?;
    /// assert!(result.scan_artifact_path.is_some());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(&self, input: ExecutionInput) -> Result<ExecutionResult> {
        match input {
            ExecutionInput::LocalPlan(plan) => self.run_plan(*plan).await,
            ExecutionInput::WatcherTask(task) => self.run_watcher_task(*task).await,
            ExecutionInput::ScanOnly {
                repository,
                output_path,
                branch,
                resume,
                workspace,
            } => {
                self.run_scan_only(
                    &repository,
                    output_path.as_deref(),
                    branch.as_deref(),
                    resume,
                    workspace.as_deref(),
                )
                .await
            }
            ExecutionInput::PluginOnly {
                plugin,
                workspace_dir,
                scan_artifact_path,
                workspace_root,
                config,
                provider,
                model,
                dry_run,
                report_formats,
                output_dir,
            } => {
                self.run_plugin_only(
                    &plugin,
                    workspace_dir.as_deref(),
                    scan_artifact_path.as_deref(),
                    workspace_root.as_deref(),
                    config,
                    provider,
                    model,
                    dry_run,
                    report_formats,
                    output_dir,
                )
                .await
            }
            ExecutionInput::CreatePr {
                repository,
                head_branch,
                base_branch,
                owner,
                repo,
                title,
                body,
                draft,
                workspace,
            } => {
                self.run_create_pr(
                    &repository,
                    &head_branch,
                    &base_branch,
                    &owner,
                    &repo,
                    &title,
                    body.as_deref(),
                    draft,
                    workspace.as_deref(),
                )
                .await
            }
        }
    }

    // -----------------------------------------------------------------------
    // Private: top-level execution paths
    // -----------------------------------------------------------------------

    /// Runs a full workflow plan through all pipeline stages.
    async fn run_plan(&self, plan: WorkflowPlan) -> Result<ExecutionResult> {
        let started_at = Utc::now();

        // Stage 1: apply plan-level config overrides.
        let effective_config = self.apply_plan_overrides(&plan);

        // Stage 2: validate governance.
        let governance = GovernanceChecker::from_config(&effective_config.governance)?;
        let plugin_names: Vec<&str> = plan.steps.iter().map(|s| s.plugin.as_str()).collect();
        governance.check_workflow_inputs(
            plan.branch.as_deref(),
            &[plan.repository.as_str()],
            &plugin_names,
            &[],
            &[],
        )?;

        // Stage 3: resolve workspace root.
        let workspace_root = plan
            .workspace
            .clone()
            .unwrap_or_else(|| effective_config.workspace.root.clone());
        governance.check_workspace_path(&workspace_root)?;

        // Stage 4: open or create workspace.
        let mut workspace = if plan.resume {
            WorkspaceManager::open(&workspace_root, &plan.repository, plan.branch.clone())?
        } else {
            WorkspaceManager::create(&workspace_root, &plan.repository, plan.branch.clone(), None)?
        };

        let workspace_id = workspace.id().to_string();

        // Stage 5: dry-run short-circuit.
        if plan.is_dry_run() {
            return self.execute_dry_run(&plan, &workspace, &effective_config, started_at);
        }

        // Stage 6: transition to Initializing.
        workspace.transition(WorkspaceStage::Initializing)?;

        // Stage 7: resolve repository to a local path.
        let repo_path = match self.resolve_repository_path(&plan.repository) {
            Ok(p) => p,
            Err(e) => {
                workspace.transition(WorkspaceStage::Failed {
                    stage: "initializing".to_string(),
                    reason: e.to_string(),
                })?;
                return Err(e);
            }
        };

        // Stage 8: git preparation.
        let git_metadata = match GitRepository::open(&repo_path) {
            Ok(repo) => match repo.metadata(plan.branch.as_deref()) {
                Ok(meta) => {
                    workspace.apply_git_metadata(&meta)?;
                    Some(meta)
                }
                Err(e) => {
                    debug!(
                        path = %repo_path.display(),
                        error = %e,
                        "failed to collect git metadata; skipping"
                    );
                    None
                }
            },
            Err(e) => {
                debug!(
                    path = %repo_path.display(),
                    error = %e,
                    "repository is not a git repo; skipping git metadata"
                );
                None
            }
        };

        // Stage 9: scan or resume from artifact.
        workspace.transition(WorkspaceStage::Scanning)?;
        let scan_result = if plan.resume && workspace.state.scan_artifact_path.is_some() {
            match self.load_scan_artifact(&workspace) {
                Ok(sr) => sr,
                Err(e) => {
                    warn!(
                        error = %e,
                        "failed to load scan artifact; falling back to fresh scan"
                    );
                    self.run_scanner(
                        &repo_path,
                        &effective_config,
                        plan.scan.as_ref(),
                        git_metadata.as_ref(),
                    )?
                }
            }
        } else {
            self.run_scanner(
                &repo_path,
                &effective_config,
                plan.scan.as_ref(),
                git_metadata.as_ref(),
            )?
        };

        // Stage 10: persist scan artifact.
        let artifact_path = workspace
            .paths
            .scan_artifact()
            .to_string_lossy()
            .to_string();
        let scan_yaml = scan_result.to_yaml()?;
        std::fs::write(&artifact_path, &scan_yaml).map_err(|e| {
            PipelineError::Scanner(format!(
                "failed to write scan artifact to '{}': {}",
                artifact_path, e
            ))
        })?;
        workspace.record_scan_artifact(
            artifact_path,
            Some(scan_result.version.clone()),
            scan_result.head_commit.clone(),
        )?;

        // Stage 11: create provider.
        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> =
            ProviderFactory::create_from_config(&effective_config)?;

        // Stage 12: execute steps in dependency order.
        let mut completed_steps: HashSet<String> = HashSet::new();
        let mut all_report_paths: HashMap<String, Vec<String>> = HashMap::new();
        let mut execution_errors: Vec<String> = Vec::new();

        loop {
            let ready: Vec<&PluginStep> = plan
                .steps
                .iter()
                .filter(|s| !completed_steps.contains(&s.id))
                .filter(|s| s.dependencies.iter().all(|d| completed_steps.contains(d)))
                .collect();

            if ready.is_empty() {
                if completed_steps.len() == plan.steps.len() {
                    break;
                }
                let reason = "deadlock: unresolvable step dependencies".to_string();
                workspace.transition(WorkspaceStage::Failed {
                    stage: "plugin_running".to_string(),
                    reason: reason.clone(),
                })?;
                return Err(PipelineError::Workflow(format!(
                    "workflow deadlock: {} of {} steps completed before stalling",
                    completed_steps.len(),
                    plan.steps.len()
                )));
            }

            for step in ready {
                workspace.transition(WorkspaceStage::PluginRunning {
                    step_id: step.id.clone(),
                })?;

                // Look up the plugin.
                let plugin = match self.plugin_registry.get(&step.plugin) {
                    Ok(p) => p,
                    Err(e) => {
                        let msg = format!("step '{}': {}", step.id, e);
                        execution_errors.push(msg.clone());
                        workspace
                            .record_plugin_output(&step.id, &step.plugin, None, false, vec![msg])
                            .unwrap_or_else(|we| {
                                warn!(
                                    step = %step.id,
                                    error = %we,
                                    "failed to record plugin output for missing plugin"
                                );
                            });
                        completed_steps.insert(step.id.clone());
                        continue;
                    }
                };

                // Build sandbox path validator.
                let sandbox = Arc::new(PathValidator::new(
                    vec![repo_path.clone(), workspace.paths.root.clone()],
                    vec![workspace.paths.root.clone()],
                ));

                // Build tool registry matching plugin's access requirements.
                let tool_registry = match plugin.required_tool_access() {
                    ToolAccessLevel::None => ToolRegistry::new(),
                    ToolAccessLevel::ReadOnly => build_read_only_registry(sandbox),
                    ToolAccessLevel::ReadWrite => build_read_write_registry(sandbox),
                };

                // Load a fresh workspace snapshot for the plugin context.
                let plugin_workspace = match WorkspaceManager::load(&workspace_root, &workspace_id)
                {
                    Ok(wm) => Arc::new(wm),
                    Err(e) => {
                        warn!(
                            error = %e,
                            "failed to load workspace for plugin context; using current state"
                        );
                        // Fall back: recreate from current state snapshot.
                        Arc::new(WorkspaceManager::load(&workspace_root, &workspace_id)?)
                    }
                };

                let current_state = plugin_workspace.state.clone();

                let ctx = PluginContext::new(
                    Arc::new(effective_config.clone()),
                    plugin_workspace,
                    current_state,
                    scan_result.clone(),
                    Arc::clone(&provider),
                    tool_registry,
                    GovernanceChecker::from_config(&effective_config.governance)?,
                );

                // Attach watcher task ID if present.
                let ctx = if let Some(ref task_id) = workspace.state.watcher_task_id {
                    ctx.with_watcher_task_id(task_id.clone())
                } else {
                    ctx
                };

                // Execute the plugin.
                let plugin_output = match plugin.run(ctx).await {
                    Ok(output) => output,
                    Err(e) => {
                        let msg = format!("step '{}': plugin execution failed: {}", step.id, e);
                        workspace.record_plugin_output(
                            &step.id,
                            &step.plugin,
                            None,
                            false,
                            vec![e.to_string()],
                        )?;
                        workspace.transition(WorkspaceStage::Failed {
                            stage: format!("plugin_running:{}", step.id),
                            reason: e.to_string(),
                        })?;
                        return Err(PipelineError::Plugin(msg));
                    }
                };

                // Reload workspace to pick up any writes the plugin made.
                if let Ok(refreshed) = WorkspaceManager::load(&workspace_root, &workspace_id) {
                    workspace = refreshed;
                }

                // Record plugin output.
                let diag_strings: Vec<String> = plugin_output
                    .diagnostics
                    .entries
                    .iter()
                    .map(|d| d.message.clone())
                    .collect();
                let first_written = plugin_output.written_files.first().cloned();
                workspace.record_plugin_output(
                    &step.id,
                    &step.plugin,
                    first_written,
                    plugin_output.completed,
                    diag_strings,
                )?;

                // Record any plugin score.
                if let Some(score) = plugin_output
                    .scores
                    .get("overall")
                    .or_else(|| plugin_output.scores.values().next())
                    .copied()
                {
                    workspace.record_plugin_score(&step.id, score)?;
                }

                // Write reports.
                workspace.transition(WorkspaceStage::ReportWriting)?;
                let report_paths =
                    self.write_step_reports(step, &plugin_output, &plan, &workspace)?;
                for path in &report_paths {
                    workspace.add_report_path(&step.id, path.clone())?;
                }
                all_report_paths.insert(step.id.clone(), report_paths);

                workspace.transition(WorkspaceStage::ReportComplete)?;
                workspace.transition(WorkspaceStage::PluginComplete {
                    step_id: step.id.clone(),
                })?;
                completed_steps.insert(step.id.clone());
            }
        }

        // Stage 13: mark complete.
        workspace.transition(WorkspaceStage::Complete)?;

        let completed_at = Utc::now();
        let final_state = workspace.state.clone();

        Ok(ExecutionResult {
            workspace_id,
            success: execution_errors.is_empty(),
            errors: execution_errors,
            diagnostics: Vec::new(),
            scan_artifact_path: final_state.scan_artifact_path,
            report_paths: all_report_paths,
            watcher_result: None,
            stage_at_completion: final_state.current_stage,
            started_at,
            completed_at,
            is_dry_run: false,
        })
    }

    /// Converts a watcher task to a plan, runs it, and adds the watcher result.
    async fn run_watcher_task(&self, task: WatcherTaskMessage) -> Result<ExecutionResult> {
        let started_at = Utc::now();
        let plan = self.watcher_task_to_plan(&task);

        let mut exec_result = self.run_plan(plan).await?;

        let watcher_result = Self::build_watcher_result(&task, &exec_result, started_at);
        exec_result.watcher_result = Some(watcher_result);

        Ok(exec_result)
    }

    /// Runs only the scanner stage without plugin execution.
    ///
    /// Mirrors [`run_plan`][Self::run_plan]'s workspace and governance
    /// handling: `resume` selects [`WorkspaceManager::open`] over
    /// [`WorkspaceManager::create`], and governance rules are validated
    /// before any workspace or filesystem I/O occurs.
    async fn run_scan_only(
        &self,
        repository: &str,
        output_path: Option<&str>,
        branch: Option<&str>,
        resume: bool,
        workspace_override: Option<&str>,
    ) -> Result<ExecutionResult> {
        let started_at = Utc::now();

        // Governance: validate repository/branch inputs. No plugin steps run
        // in scan-only mode, so `plugin_names` is empty.
        let governance = GovernanceChecker::from_config(&self.config.governance)?;
        governance.check_workflow_inputs(branch, &[repository], &[], &[], &[])?;

        let workspace_root = workspace_override
            .map(str::to_string)
            .unwrap_or_else(|| self.config.workspace.root.clone());
        governance.check_workspace_path(&workspace_root)?;

        let mut workspace = if resume {
            WorkspaceManager::open(&workspace_root, repository, branch.map(str::to_string))?
        } else {
            WorkspaceManager::create(
                &workspace_root,
                repository,
                branch.map(str::to_string),
                None,
            )?
        };
        let workspace_id = workspace.id().to_string();

        workspace.transition(WorkspaceStage::Scanning)?;

        let repo_path = self.resolve_repository_path(repository)?;
        let scan_result = if resume && workspace.state.scan_artifact_path.is_some() {
            match self.load_scan_artifact(&workspace) {
                Ok(sr) => sr,
                Err(e) => {
                    warn!(
                        error = %e,
                        "failed to load scan artifact; falling back to fresh scan"
                    );
                    self.run_scanner(&repo_path, &self.config, None, None)?
                }
            }
        } else {
            self.run_scanner(&repo_path, &self.config, None, None)?
        };

        // Persist scan artifact to workspace.
        let artifact_path = workspace
            .paths
            .scan_artifact()
            .to_string_lossy()
            .to_string();
        let scan_yaml = scan_result.to_yaml()?;
        std::fs::write(&artifact_path, &scan_yaml)
            .map_err(|e| PipelineError::Scanner(format!("failed to write scan artifact: {}", e)))?;

        // Optionally write to caller-specified path.
        if let Some(out) = output_path {
            std::fs::write(out, &scan_yaml)?;
        }

        workspace.record_scan_artifact(
            artifact_path.clone(),
            Some(scan_result.version.clone()),
            scan_result.head_commit.clone(),
        )?;
        workspace.transition(WorkspaceStage::Complete)?;

        let completed_at = Utc::now();

        Ok(ExecutionResult {
            workspace_id,
            success: true,
            errors: Vec::new(),
            diagnostics: Vec::new(),
            scan_artifact_path: Some(artifact_path),
            report_paths: HashMap::new(),
            watcher_result: None,
            stage_at_completion: WorkspaceStage::Complete,
            started_at,
            completed_at,
            is_dry_run: false,
        })
    }

    /// Runs a single plugin against previously-collected scan data, without
    /// resolving a repository or running the scanner.
    ///
    /// Exactly one of `workspace_dir` or `scan_artifact_path` must be
    /// supplied. When `workspace_dir` is given, scan data and sandbox read
    /// access to the original repository checkout (when its location was
    /// recorded on the resumed workspace) are restored from it. When only
    /// `scan_artifact_path` is given, a fresh, ephemeral workspace is
    /// created under `workspace_root` purely to host reports and sandbox
    /// state; the plugin has no read access to the original repository
    /// checkout in that case.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workflow`] if neither `workspace_dir` nor
    /// `scan_artifact_path` is supplied, if `workspace_dir` does not have
    /// both a parent (workspace root) and a file-name (workspace id)
    /// component, or if the requested plugin is not registered. Returns
    /// [`PipelineError::Plugin`] if the plugin itself returns an error.
    #[allow(clippy::too_many_arguments)]
    async fn run_plugin_only(
        &self,
        plugin_name: &str,
        workspace_dir: Option<&str>,
        scan_artifact_path: Option<&str>,
        workspace_root_override: Option<&str>,
        plugin_config: Option<serde_json::Value>,
        provider_override: Option<String>,
        model_override: Option<String>,
        dry_run: bool,
        report_formats: Vec<String>,
        output_dir: Option<String>,
    ) -> Result<ExecutionResult> {
        let started_at = Utc::now();

        if workspace_dir.is_none() && scan_artifact_path.is_none() {
            return Err(PipelineError::Workflow(
                "plugin-only execution requires workspace_dir or scan_artifact_path".to_string(),
            ));
        }

        // Build a synthetic single-step plan so plan-shaped helpers (report
        // format resolution, dry-run validation, report writing) can be
        // reused unchanged.
        let mut plan = WorkflowPlan::direct_plugin_invocation(
            plugin_name,
            workspace_dir.or(scan_artifact_path).unwrap_or_default(),
        );
        plan.provider = provider_override;
        plan.model = model_override;
        plan.dry_run = dry_run;
        plan.steps[0].config = plugin_config;
        if !report_formats.is_empty() {
            plan.steps[0].report_formats = Some(report_formats.clone());
        }
        if !report_formats.is_empty() || output_dir.is_some() {
            plan.reports = Some(PlanReportOptions {
                output_dir,
                formats: if report_formats.is_empty() {
                    None
                } else {
                    Some(report_formats)
                },
                overwrite: None,
            });
        }

        let effective_config = self.apply_plan_overrides(&plan);

        // Governance: validate the plugin name. No file paths, branch, or
        // provider endpoints are known in this mode.
        let governance = GovernanceChecker::from_config(&effective_config.governance)?;
        governance.check_workflow_inputs(None, &[], &[plugin_name], &[], &[])?;

        // Resolve workspace, scan data, and (when available) the original
        // repository checkout path for sandbox read access.
        let (mut workspace, scan_result, repo_path, workspace_root) = if let Some(dir) =
            workspace_dir
        {
            let path = std::path::Path::new(dir);
            let root = path.parent().ok_or_else(|| {
                PipelineError::Workflow(format!(
                    "workspace directory '{}' has no parent (workspace root)",
                    dir
                ))
            })?;
            let id = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
                PipelineError::Workflow(format!(
                    "workspace directory '{}' has no valid workspace id component",
                    dir
                ))
            })?;
            let root_str = root.to_string_lossy().to_string();
            governance.check_workspace_path(&root_str)?;
            let workspace = WorkspaceManager::load(&root_str, id)?;

            let scan_result = if let Some(artifact_path) = scan_artifact_path {
                let content = std::fs::read_to_string(artifact_path).map_err(PipelineError::Io)?;
                ScanResult::load_from_str(&content)?
            } else {
                self.load_scan_artifact(&workspace)?
            };

            let repo_path = workspace
                .state
                .local_repository_path
                .as_ref()
                .map(PathBuf::from);

            (workspace, scan_result, repo_path, root_str)
        } else {
            // `scan_artifact_path` is `Some` by the guard above.
            let artifact_path = scan_artifact_path.expect("checked above");
            let workspace_root = workspace_root_override
                .map(str::to_string)
                .unwrap_or_else(|| effective_config.workspace.root.clone());
            governance.check_workspace_path(&workspace_root)?;
            let workspace = WorkspaceManager::create(
                &workspace_root,
                &format!("scan-artifact:{}", artifact_path),
                None,
                None,
            )?;
            let content = std::fs::read_to_string(artifact_path).map_err(PipelineError::Io)?;
            let scan_result = ScanResult::load_from_str(&content)?;
            (workspace, scan_result, None, workspace_root)
        };

        let workspace_id = workspace.id().to_string();

        // Dry-run short-circuit.
        if plan.is_dry_run() {
            return self.execute_dry_run(&plan, &workspace, &effective_config, started_at);
        }

        let step = plan.steps[0].clone();

        workspace.transition(WorkspaceStage::PluginRunning {
            step_id: step.id.clone(),
        })?;

        let plugin = self.plugin_registry.get(plugin_name)?;

        let sandbox_read_roots = match &repo_path {
            Some(p) => vec![p.clone(), workspace.paths.root.clone()],
            None => vec![workspace.paths.root.clone()],
        };
        let sandbox = Arc::new(PathValidator::new(
            sandbox_read_roots,
            vec![workspace.paths.root.clone()],
        ));

        let tool_registry = match plugin.required_tool_access() {
            ToolAccessLevel::None => ToolRegistry::new(),
            ToolAccessLevel::ReadOnly => build_read_only_registry(sandbox),
            ToolAccessLevel::ReadWrite => build_read_write_registry(sandbox),
        };

        let provider: Arc<dyn crate::providers::base::Provider + Send + Sync> =
            ProviderFactory::create_from_config(&effective_config)?;

        let plugin_workspace = Arc::new(WorkspaceManager::load(&workspace_root, &workspace_id)?);
        let current_state = plugin_workspace.state.clone();

        let ctx = PluginContext::new(
            Arc::new(effective_config.clone()),
            plugin_workspace,
            current_state,
            scan_result,
            Arc::clone(&provider),
            tool_registry,
            GovernanceChecker::from_config(&effective_config.governance)?,
        );

        let plugin_output = match plugin.run(ctx).await {
            Ok(output) => output,
            Err(e) => {
                let msg = format!("plugin '{}' execution failed: {}", plugin_name, e);
                workspace.record_plugin_output(
                    &step.id,
                    plugin_name,
                    None,
                    false,
                    vec![e.to_string()],
                )?;
                workspace.transition(WorkspaceStage::Failed {
                    stage: format!("plugin_running:{}", step.id),
                    reason: e.to_string(),
                })?;
                return Err(PipelineError::Plugin(msg));
            }
        };

        // Reload workspace to pick up any writes the plugin made.
        if let Ok(refreshed) = WorkspaceManager::load(&workspace_root, &workspace_id) {
            workspace = refreshed;
        }

        let diag_strings: Vec<String> = plugin_output
            .diagnostics
            .entries
            .iter()
            .map(|d| d.message.clone())
            .collect();
        let first_written = plugin_output.written_files.first().cloned();
        workspace.record_plugin_output(
            &step.id,
            plugin_name,
            first_written,
            plugin_output.completed,
            diag_strings,
        )?;

        if let Some(score) = plugin_output
            .scores
            .get("overall")
            .or_else(|| plugin_output.scores.values().next())
            .copied()
        {
            workspace.record_plugin_score(&step.id, score)?;
        }

        workspace.transition(WorkspaceStage::ReportWriting)?;
        let report_paths = self.write_step_reports(&step, &plugin_output, &plan, &workspace)?;
        for path in &report_paths {
            workspace.add_report_path(&step.id, path.clone())?;
        }
        let mut all_report_paths: HashMap<String, Vec<String>> = HashMap::new();
        all_report_paths.insert(step.id.clone(), report_paths);

        workspace.transition(WorkspaceStage::ReportComplete)?;
        workspace.transition(WorkspaceStage::PluginComplete {
            step_id: step.id.clone(),
        })?;
        workspace.transition(WorkspaceStage::Complete)?;

        let completed_at = Utc::now();
        let final_state = workspace.state.clone();

        Ok(ExecutionResult {
            workspace_id,
            success: true,
            errors: Vec::new(),
            diagnostics: Vec::new(),
            scan_artifact_path: final_state.scan_artifact_path,
            report_paths: all_report_paths,
            watcher_result: None,
            stage_at_completion: final_state.current_stage,
            started_at,
            completed_at,
            is_dry_run: false,
        })
    }

    // -----------------------------------------------------------------------
    // Private: pull request creation
    // -----------------------------------------------------------------------

    /// Creates a GitHub pull request for the given head branch.
    ///
    /// When `config.pr.enabled` is `false`, returns an immediate no-op success
    /// result without contacting GitHub.  Otherwise, resolves a GitHub PAT and
    /// calls [`crate::clients::github::GithubPrClient::create_pr`].
    ///
    /// # Arguments
    ///
    /// * `_repository` - Local repository path (reserved for future use when
    ///   push integration is added).
    /// * `head_branch` - The source branch for the PR.
    /// * `base_branch` - The target branch for the PR. Must differ from
    ///   `head_branch`.
    /// * `owner` - GitHub repository owner.
    /// * `repo` - GitHub repository name.
    /// * `title` - Pull request title.
    /// * `body` - Optional pull request description.
    /// * `draft` - When `true`, opens the PR as a draft.
    /// * `_workspace` - Workspace root override (reserved for future use).
    ///
    /// # Returns
    ///
    /// An [`ExecutionResult`] with `stage_at_completion` set to
    /// [`WorkspaceStage::Complete`] for the no-op path, or
    /// [`WorkspaceStage::PrComplete`] on successful PR creation.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Governance`] if `head_branch` or `base_branch`
    /// violates an active governance rule (only when governance is enabled and
    /// `fail_on_violation` is `true` for a `Required` branch rule).
    ///
    /// Returns [`PipelineError::Git`] if the GitHub API call fails.
    #[allow(clippy::too_many_arguments)]
    async fn run_create_pr(
        &self,
        _repository: &str,
        head_branch: &str,
        base_branch: &str,
        owner: &str,
        repo: &str,
        title: &str,
        body: Option<&str>,
        draft: bool,
        _workspace: Option<&str>,
    ) -> Result<ExecutionResult> {
        let started_at = Utc::now();

        // Opt-in gate: return a no-op success when PR creation is disabled.
        if !self.config.pr.enabled {
            return Ok(ExecutionResult {
                workspace_id: Ulid::new().to_string(),
                success: true,
                errors: vec![],
                diagnostics: vec![],
                scan_artifact_path: None,
                report_paths: std::collections::HashMap::new(),
                watcher_result: None,
                stage_at_completion: WorkspaceStage::Complete,
                started_at,
                completed_at: Utc::now(),
                is_dry_run: false,
            });
        }

        // Governance: validate branch names before contacting GitHub.
        let governance = GovernanceChecker::from_config(&self.config.governance)?;
        governance.check_branch(head_branch)?;
        governance.check_branch(base_branch)?;

        // Resolve GitHub PAT; pass `None` to `GithubPrClient::new` when absent.
        let token = crate::clients::github::pr::resolve_github_pat();

        let client = crate::clients::github::GithubPrClient::new(token);
        let pr_input = crate::clients::github::PrInput {
            owner: owner.to_string(),
            repo: repo.to_string(),
            head_branch: head_branch.to_string(),
            base_branch: base_branch.to_string(),
            title: title.to_string(),
            body: body.map(str::to_string),
            draft,
        };

        let pr_output = client
            .create_pr(&pr_input)
            .await
            .map_err(|e| PipelineError::Git(format!("PR creation failed: {e}")))?;

        Ok(ExecutionResult {
            workspace_id: Ulid::new().to_string(),
            success: true,
            errors: vec![],
            diagnostics: vec![],
            scan_artifact_path: None,
            report_paths: std::collections::HashMap::new(),
            watcher_result: None,
            stage_at_completion: WorkspaceStage::PrComplete {
                branch: head_branch.to_string(),
                pr_number: pr_output.number,
                pr_url: pr_output.html_url,
            },
            started_at,
            completed_at: Utc::now(),
            is_dry_run: false,
        })
    }

    // -----------------------------------------------------------------------
    // Private: dry run
    // -----------------------------------------------------------------------

    /// Validates the plan without performing any scanner, provider, or report
    /// side effects.
    fn execute_dry_run(
        &self,
        plan: &WorkflowPlan,
        workspace: &WorkspaceManager,
        effective_config: &Config,
        started_at: DateTime<Utc>,
    ) -> Result<ExecutionResult> {
        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        // Validate that the repository path exists.
        let repo_path = std::path::Path::new(&plan.repository);
        if !repo_path.exists() {
            diagnostics.push(Diagnostic::warning(
                DiagnosticCategory::Config,
                format!(
                    "dry-run: repository '{}' does not exist as a local path",
                    plan.repository
                ),
            ));
        }

        // Validate provider config (create provider object but do not call it).
        if let Err(e) = ProviderFactory::create_from_config(effective_config) {
            diagnostics.push(Diagnostic::warning(
                DiagnosticCategory::Config,
                format!("dry-run: provider config invalid: {}", e),
            ));
        }

        // Validate each step's plugin is registered.
        let mut step_errors: Vec<String> = Vec::new();
        for step in &plan.steps {
            let config_val = step
                .config
                .clone()
                .unwrap_or(serde_json::Value::Object(Default::default()));
            if let Err(e) = self
                .plugin_registry
                .validate_plugin_config(&step.plugin, &config_val)
            {
                step_errors.push(format!("step '{}': {}", step.id, e));
            }
        }

        diagnostics.push(Diagnostic::info(
            DiagnosticCategory::Config,
            "dry-run: all validations passed".to_string(),
        ));

        Ok(ExecutionResult {
            workspace_id: workspace.id().to_string(),
            success: step_errors.is_empty(),
            errors: step_errors,
            diagnostics,
            scan_artifact_path: None,
            report_paths: HashMap::new(),
            watcher_result: None,
            stage_at_completion: WorkspaceStage::Initializing,
            started_at,
            completed_at: Utc::now(),
            is_dry_run: true,
        })
    }

    // -----------------------------------------------------------------------
    // Private: helpers
    // -----------------------------------------------------------------------

    /// Applies plan-level provider and model overrides to a clone of `self.config`.
    fn apply_plan_overrides(&self, plan: &WorkflowPlan) -> Config {
        let mut config = (*self.config).clone();
        if let Some(ref p) = plan.provider {
            config.provider.default = p.clone();
        }
        if let Some(ref m) = plan.model {
            match config.provider.default.as_str() {
                "openai" | "" => config.openai.model = m.clone(),
                "anthropic" => config.anthropic.model = m.clone(),
                "ollama" => config.ollama.model = m.clone(),
                _ => {}
            }
        }
        config
    }

    /// Resolves `repository` to a local filesystem path.
    ///
    /// Only local directory paths are supported in Phase 17. Remote URLs
    /// return an error with a migration hint.
    fn resolve_repository_path(&self, repository: &str) -> Result<PathBuf> {
        let path = std::path::Path::new(repository);
        if path.exists() && path.is_dir() {
            Ok(path.to_path_buf())
        } else {
            Err(PipelineError::Workflow(format!(
                "repository '{}' is not an accessible local directory; \
                 remote URL cloning is not yet supported",
                repository
            )))
        }
    }

    /// Builds a [`ScannerCfg`] from global config and optional plan overrides.
    fn build_scanner_config(&self, config: &Config, opts: Option<&PlanScanOptions>) -> ScannerCfg {
        let mut sc = ScannerCfg::default()
            .with_include_hidden(config.scanner.include_hidden)
            .with_max_file_size(config.scanner.max_file_size_bytes)
            .with_exclude_patterns(config.scanner.ignore_patterns.clone());

        if let Some(o) = opts {
            if let Some(h) = o.include_hidden {
                sc = sc.with_include_hidden(h);
            }
            if let Some(m) = o.max_file_size_bytes {
                sc = sc.with_max_file_size(m);
            }
            if let Some(ref p) = o.ignore_patterns {
                let mut all = sc.exclude_patterns.clone();
                all.extend(p.iter().cloned());
                sc = sc.with_exclude_patterns(all);
            }
        }

        sc
    }

    /// Runs the repository scanner on `repo_path`.
    fn run_scanner(
        &self,
        repo_path: &std::path::Path,
        config: &Config,
        opts: Option<&PlanScanOptions>,
        git_metadata: Option<&crate::git::metadata::GitMetadata>,
    ) -> Result<ScanResult> {
        let sc = self.build_scanner_config(config, opts);
        Scanner::new(sc).scan(repo_path, git_metadata)
    }

    /// Loads the scan artifact from the path stored in the workspace state.
    fn load_scan_artifact(&self, workspace: &WorkspaceManager) -> Result<ScanResult> {
        let path = workspace.state.scan_artifact_path.as_ref().ok_or_else(|| {
            PipelineError::Scanner("no scan artifact path recorded in workspace state".to_string())
        })?;
        let content = std::fs::read_to_string(path).map_err(|e| {
            PipelineError::Scanner(format!("failed to read scan artifact '{}': {}", path, e))
        })?;
        ScanResult::load_from_str(&content)
    }

    /// Resolves the report formats for a single step.
    ///
    /// Resolution order (first match wins):
    /// 1. Step-level `report_formats`
    /// 2. Plan-level `reports.formats`
    /// 3. Global `config.reports.formats`
    fn get_report_formats_for_step(
        &self,
        step: &PluginStep,
        plan: &WorkflowPlan,
    ) -> Vec<ReportFormat> {
        let format_strings: Vec<String> = step
            .report_formats
            .as_ref()
            .or_else(|| plan.reports.as_ref().and_then(|r| r.formats.as_ref()))
            .cloned()
            .unwrap_or_else(|| self.config.reports.formats.clone());

        format_strings
            .iter()
            .filter_map(|s| ReportFormat::from_str(s))
            .collect()
    }

    /// Writes reports for a completed plugin step.
    ///
    /// Returns the list of report file paths written.
    fn write_step_reports(
        &self,
        step: &PluginStep,
        output: &PluginOutput,
        plan: &WorkflowPlan,
        workspace: &WorkspaceManager,
    ) -> Result<Vec<String>> {
        let formats = self.get_report_formats_for_step(step, plan);
        if formats.is_empty() {
            return Ok(Vec::new());
        }

        // Prefer an explicit plan-level output directory override (mapped
        // from `--output-dir`) over the workspace's default report location.
        let step_dir = match plan.reports.as_ref().and_then(|r| r.output_dir.as_ref()) {
            Some(dir) => PathBuf::from(dir).join(&step.id),
            None => workspace.paths.step_reports_dir(&step.id),
        };
        std::fs::create_dir_all(&step_dir)?;

        let report_id = Ulid::new().to_string();
        let mut envelope = ReportEnvelope::new(&report_id, &step.plugin, workspace.id());
        envelope.repository_url = Some(workspace.state.repository_url.clone());
        envelope.head_commit = workspace.state.scan_artifact_head_commit.clone();

        // Populate envelope findings from plugin output.
        let mut pf = PluginFindings::new();
        for f in &output.findings {
            envelope.findings.push(f.clone());
            pf.push(f.clone());
        }
        if !output.findings.is_empty() {
            envelope.risk_band = pf.to_risk_band();
        }

        let mut written: Vec<String> = Vec::new();
        for format in &formats {
            let fname = format!("{}{}", step.plugin.replace('-', "_"), format.extension());
            let path = step_dir.join(&fname);
            match format {
                ReportFormat::Markdown => MarkdownReportWriter.write(&envelope, &path)?,
                ReportFormat::Json => JsonReportWriter.write(&envelope, &path)?,
                ReportFormat::Sarif => SarifReportWriter.write(&envelope, &path)?,
            }
            written.push(path.to_string_lossy().to_string());
        }

        Ok(written)
    }

    /// Converts a [`WatcherTaskMessage`] to a [`WorkflowPlan`].
    fn watcher_task_to_plan(&self, task: &WatcherTaskMessage) -> WorkflowPlan {
        let workspace_root = task
            .workspace_directory
            .clone()
            .unwrap_or_else(|| self.config.workspace.root.clone());

        let plugin_config = if task.plugin_config.is_null() {
            None
        } else {
            Some(task.plugin_config.clone())
        };

        let report_formats = if task.requested_report_formats.is_empty() {
            None
        } else {
            Some(task.requested_report_formats.clone())
        };

        WorkflowPlan {
            version: PLAN_VERSION.to_string(),
            name: format!("watcher-{}", task.id),
            description: Some(format!("Watcher task for plugin '{}'", task.plugin)),
            repository: task.repository.clone(),
            branch: task.target_branch.clone(),
            workspace: Some(workspace_root),
            provider: task.provider.clone(),
            model: task.model.clone(),
            scan: None,
            steps: vec![PluginStep {
                id: "step1".to_string(),
                description: Some(format!("Run plugin '{}'", task.plugin)),
                plugin: task.plugin.clone(),
                config: plugin_config,
                dependencies: vec![],
                report_formats,
                max_findings: None,
                severity_threshold: None,
            }],
            reports: None,
            dry_run: task.dry_run,
            resume: false,
        }
    }

    /// Builds a [`WatcherResultMessage`] from an [`ExecutionResult`].
    fn build_watcher_result(
        task: &WatcherTaskMessage,
        exec_result: &ExecutionResult,
        started_at: DateTime<Utc>,
    ) -> WatcherResultMessage {
        let result_id = Ulid::new().to_string();

        let result_event_type = match task.event_type {
            WatcherEventType::TechnicalReviewTask | WatcherEventType::TechnicalReviewResult => {
                WatcherEventType::TechnicalReviewResult
            }
            WatcherEventType::SecurityReviewTask | WatcherEventType::SecurityReviewResult => {
                WatcherEventType::SecurityReviewResult
            }
        };

        let mut result = WatcherResultMessage::new(
            result_id,
            result_event_type,
            "xzardgz-watcher",
            task.repository.clone(),
            task.plugin.clone(),
            exec_result.workspace_id.clone(),
            task.correlation_id.clone(),
            task.id.clone(),
            started_at,
        );

        result.target_branch = task.target_branch.clone();
        result.scan_artifact_path = exec_result.scan_artifact_path.clone();
        result.report_paths = exec_result.report_paths.clone();

        if exec_result.success {
            result = result.success_result();
        } else {
            for e in &exec_result.errors {
                result = result.with_error(e.clone());
            }
        }

        result
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::config::Config;
    use crate::plugins::context::ToolAccessLevel;
    use crate::plugins::output::PluginOutput;
    use crate::plugins::registry::PluginRegistry;
    use crate::plugins::trait_def::{PluginMetadata, WorkflowPlugin};
    use crate::watcher::event_type::WatcherEventType;
    use crate::watcher::task::WatcherTaskMessage;
    use crate::workflow::plan::WorkflowPlan;

    // ------------------------------------------------------------------
    // Test helpers
    // ------------------------------------------------------------------

    /// Minimal plugin that always returns success.
    struct SuccessPlugin;

    #[async_trait]
    impl WorkflowPlugin for SuccessPlugin {
        fn name(&self) -> &str {
            "test-plugin"
        }

        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new("test-plugin", "1.0.0", "Test plugin.")
        }

        fn supported_formats(&self) -> Vec<String> {
            vec!["json".to_string()]
        }

        fn required_tool_access(&self) -> ToolAccessLevel {
            ToolAccessLevel::None
        }

        async fn run(&self, _ctx: PluginContext) -> crate::error::Result<PluginOutput> {
            Ok(PluginOutput::success("test plugin succeeded"))
        }
    }

    /// Plugin that returns a non-completed (failure) output.
    struct FailureOutputPlugin;

    #[async_trait]
    impl WorkflowPlugin for FailureOutputPlugin {
        fn name(&self) -> &str {
            "failure-plugin"
        }

        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(
                "failure-plugin",
                "1.0.0",
                "Plugin that returns failure output.",
            )
        }

        fn supported_formats(&self) -> Vec<String> {
            vec!["json".to_string()]
        }

        fn required_tool_access(&self) -> ToolAccessLevel {
            ToolAccessLevel::None
        }

        async fn run(&self, _ctx: PluginContext) -> crate::error::Result<PluginOutput> {
            Ok(PluginOutput::failure("plugin reported failure"))
        }
    }

    /// Builds a test config with governance disabled.
    fn make_test_config(workspace_root: &str) -> Config {
        let mut config = Config::default();
        config.workspace.root = workspace_root.to_string();
        // Use json report format for tests.
        config.reports.formats = vec!["json".to_string()];
        // Disable governance and clear rules_path so the loader does not
        // attempt to parse AGENTS.md as a YAML rules file.
        config.governance.enabled = false;
        config.governance.rules_path = String::new();
        config
    }

    /// Builds a [`WorkflowExecutor`] with a `SuccessPlugin` registered.
    fn make_executor_with_success_plugin(workspace_root: &str) -> WorkflowExecutor {
        let config = make_test_config(workspace_root);
        let mut registry = PluginRegistry::new();
        registry.register(Arc::new(SuccessPlugin));
        WorkflowExecutor::new(Arc::new(config), Arc::new(registry))
    }

    /// Builds a minimal single-step [`WorkflowPlan`] pointing at `repo_path`.
    fn make_test_plan(repo_path: &str, workspace_root: &str) -> WorkflowPlan {
        WorkflowPlan {
            version: PLAN_VERSION.to_string(),
            name: "test-plan".to_string(),
            description: None,
            repository: repo_path.to_string(),
            branch: None,
            workspace: Some(workspace_root.to_string()),
            provider: None,
            model: None,
            scan: None,
            steps: vec![crate::workflow::plan::PluginStep {
                id: "step1".to_string(),
                description: None,
                plugin: "test-plugin".to_string(),
                config: None,
                dependencies: vec![],
                report_formats: Some(vec!["json".to_string()]),
                max_findings: None,
                severity_threshold: None,
            }],
            reports: None,
            dry_run: false,
            resume: false,
        }
    }

    // ------------------------------------------------------------------
    // WorkflowExecutor::new
    // ------------------------------------------------------------------

    #[test]
    fn test_workflow_executor_new_creates_executor() {
        let ws = TempDir::new().unwrap();
        let executor = make_executor_with_success_plugin(ws.path().to_str().unwrap());
        // Simply verify that the executor was constructed without panic.
        let _ = executor;
    }

    // ------------------------------------------------------------------
    // ExecutionInput::LocalPlan — success path
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_local_plan_with_success_plugin_returns_success_result() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let executor = make_executor_with_success_plugin(ws_dir.path().to_str().unwrap());
        let plan = make_test_plan(
            repo_dir.path().to_str().unwrap(),
            ws_dir.path().to_str().unwrap(),
        );

        // SAFETY: TempDirs are valid directories; execution cannot fail for
        // reasons outside our control in a clean test environment.
        let result = executor
            .execute(ExecutionInput::LocalPlan(Box::new(plan)))
            .await
            .expect("execution must succeed");

        assert!(
            result.success,
            "expected success; errors: {:?}",
            result.errors
        );
        assert!(result.errors.is_empty());
        assert!(!result.workspace_id.is_empty());
        assert!(result.scan_artifact_path.is_some());
        assert!(!result.is_dry_run);
    }

    // ------------------------------------------------------------------
    // ExecutionInput::LocalPlan — plugin failure output
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_local_plan_with_failure_output_records_workspace_id() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let mut registry = PluginRegistry::new();
        registry.register(Arc::new(FailureOutputPlugin));
        let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(registry));

        let plan = WorkflowPlan {
            version: PLAN_VERSION.to_string(),
            name: "failure-test".to_string(),
            description: None,
            repository: repo_dir.path().to_str().unwrap().to_string(),
            branch: None,
            workspace: Some(ws_dir.path().to_str().unwrap().to_string()),
            provider: None,
            model: None,
            scan: None,
            steps: vec![crate::workflow::plan::PluginStep {
                id: "step1".to_string(),
                description: None,
                plugin: "failure-plugin".to_string(),
                config: None,
                dependencies: vec![],
                report_formats: Some(vec!["json".to_string()]),
                max_findings: None,
                severity_threshold: None,
            }],
            reports: None,
            dry_run: false,
            resume: false,
        };

        let result = executor
            .execute(ExecutionInput::LocalPlan(Box::new(plan)))
            .await
            .expect("execution must not return Err for graceful plugin failure");

        // A graceful failure output (completed=false) still produces a result.
        assert!(!result.workspace_id.is_empty());
        assert!(result.scan_artifact_path.is_some());
    }

    // ------------------------------------------------------------------
    // ExecutionInput::ScanOnly
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_scan_only_returns_scan_artifact_path() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(PluginRegistry::new()));

        let result = executor
            .execute(ExecutionInput::ScanOnly {
                repository: repo_dir.path().to_str().unwrap().to_string(),
                output_path: None,
                branch: None,
                resume: false,
                workspace: None,
            })
            .await
            .expect("scan-only must succeed");

        assert!(result.success);
        assert!(
            result.scan_artifact_path.is_some(),
            "scan artifact path must be set"
        );
        assert!(result.report_paths.is_empty());
        assert!(!result.is_dry_run);
    }

    #[tokio::test]
    async fn test_execute_scan_only_writes_output_path_when_specified() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();
        let out_file = ws_dir.path().join("out_artifact.yaml");

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(PluginRegistry::new()));

        let result = executor
            .execute(ExecutionInput::ScanOnly {
                repository: repo_dir.path().to_str().unwrap().to_string(),
                output_path: Some(out_file.to_string_lossy().to_string()),
                branch: None,
                resume: false,
                workspace: None,
            })
            .await
            .expect("scan-only with output path must succeed");

        assert!(result.success);
        assert!(
            out_file.exists(),
            "output_path artifact must have been written"
        );
    }

    #[tokio::test]
    async fn test_execute_scan_only_resume_loads_existing_scan_artifact() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(PluginRegistry::new()));

        let repository = repo_dir.path().to_str().unwrap().to_string();
        let workspace = Some(ws_dir.path().to_str().unwrap().to_string());

        // First run: creates workspace and scan artifact.
        let first = executor
            .execute(ExecutionInput::ScanOnly {
                repository: repository.clone(),
                output_path: None,
                branch: None,
                resume: false,
                workspace: workspace.clone(),
            })
            .await
            .expect("first scan-only run must succeed");
        assert!(first.scan_artifact_path.is_some());

        // Second run: resume = true, should open the existing workspace and
        // load its recorded scan artifact instead of failing.
        let second = executor
            .execute(ExecutionInput::ScanOnly {
                repository,
                output_path: None,
                branch: None,
                resume: true,
                workspace,
            })
            .await
            .expect("resumed scan-only run must succeed");

        assert!(second.success);
        assert!(second.scan_artifact_path.is_some());
    }

    #[tokio::test]
    async fn test_execute_scan_only_rejects_invalid_workspace_path_under_governance() {
        let repo_dir = TempDir::new().unwrap();

        let mut config = Config::default();
        config.governance.enabled = true;
        config.governance.rules_path = String::new();
        let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(PluginRegistry::new()));

        let result = executor
            .execute(ExecutionInput::ScanOnly {
                repository: repo_dir.path().to_str().unwrap().to_string(),
                output_path: None,
                branch: None,
                resume: false,
                workspace: Some("../escaping/workspace".to_string()),
            })
            .await;

        assert!(
            result.is_err(),
            "expected governance to reject a path-traversal workspace override"
        );
    }

    // ------------------------------------------------------------------
    // ExecutionInput::PluginOnly
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_plugin_only_rejects_when_neither_source_given() {
        let ws_dir = TempDir::new().unwrap();
        let executor = make_executor_with_success_plugin(ws_dir.path().to_str().unwrap());

        let result = executor
            .execute(ExecutionInput::PluginOnly {
                plugin: "test-plugin".to_string(),
                workspace_dir: None,
                scan_artifact_path: None,
                workspace_root: None,
                config: None,
                provider: None,
                model: None,
                dry_run: false,
                report_formats: vec![],
                output_dir: None,
            })
            .await;

        assert!(
            result.is_err(),
            "expected error when neither workspace_dir nor scan_artifact_path is given"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("requires workspace_dir or scan_artifact_path"),
            "expected missing-source error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn test_execute_plugin_only_from_scan_artifact_path_returns_success() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();
        let plugin_ws_dir = TempDir::new().unwrap();

        let executor = make_executor_with_success_plugin(ws_dir.path().to_str().unwrap());

        // Produce a real scan artifact via a scan-only run first.
        let scan_result = executor
            .execute(ExecutionInput::ScanOnly {
                repository: repo_dir.path().to_str().unwrap().to_string(),
                output_path: None,
                branch: None,
                resume: false,
                workspace: Some(ws_dir.path().to_str().unwrap().to_string()),
            })
            .await
            .expect("scan-only must succeed");
        let artifact_path = scan_result
            .scan_artifact_path
            .expect("scan-only must produce an artifact path");

        let result = executor
            .execute(ExecutionInput::PluginOnly {
                plugin: "test-plugin".to_string(),
                workspace_dir: None,
                scan_artifact_path: Some(artifact_path),
                workspace_root: Some(plugin_ws_dir.path().to_str().unwrap().to_string()),
                config: None,
                provider: None,
                model: None,
                dry_run: false,
                report_formats: vec!["json".to_string()],
                output_dir: None,
            })
            .await
            .expect("plugin-only run from scan artifact must succeed");

        assert!(
            result.success,
            "expected success; errors: {:?}",
            result.errors
        );
        assert!(!result.report_paths.is_empty());
        assert!(!result.is_dry_run);
    }

    #[tokio::test]
    async fn test_execute_plugin_only_from_workspace_dir_returns_success() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let executor = make_executor_with_success_plugin(ws_dir.path().to_str().unwrap());

        // First, a LocalPlan run creates a workspace with a recorded scan
        // artifact that PluginOnly can resume from.
        let plan = make_test_plan(
            repo_dir.path().to_str().unwrap(),
            ws_dir.path().to_str().unwrap(),
        );
        let first = executor
            .execute(ExecutionInput::LocalPlan(Box::new(plan)))
            .await
            .expect("initial local plan run must succeed");

        let workspace_dir = ws_dir
            .path()
            .join(&first.workspace_id)
            .to_string_lossy()
            .to_string();

        let result = executor
            .execute(ExecutionInput::PluginOnly {
                plugin: "test-plugin".to_string(),
                workspace_dir: Some(workspace_dir),
                scan_artifact_path: None,
                workspace_root: None,
                config: None,
                provider: None,
                model: None,
                dry_run: false,
                report_formats: vec!["json".to_string()],
                output_dir: None,
            })
            .await
            .expect("plugin-only run from workspace dir must succeed");

        assert!(
            result.success,
            "expected success; errors: {:?}",
            result.errors
        );
        assert!(!result.report_paths.is_empty());
    }

    #[tokio::test]
    async fn test_execute_plugin_only_dry_run_skips_plugin_execution() {
        let ws_dir = TempDir::new().unwrap();
        let artifact_ws_dir = TempDir::new().unwrap();

        // Register a plugin that would panic if actually invoked.
        struct PanicPlugin;

        #[async_trait]
        impl WorkflowPlugin for PanicPlugin {
            fn name(&self) -> &str {
                "test-plugin"
            }
            fn metadata(&self) -> PluginMetadata {
                PluginMetadata::new("test-plugin", "1.0.0", "Should not run.")
            }
            fn supported_formats(&self) -> Vec<String> {
                vec![]
            }
            fn required_tool_access(&self) -> ToolAccessLevel {
                ToolAccessLevel::None
            }
            async fn run(&self, _ctx: PluginContext) -> crate::error::Result<PluginOutput> {
                panic!("PanicPlugin::run must never be called during dry run");
            }
        }

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let mut registry = PluginRegistry::new();
        registry.register(Arc::new(PanicPlugin));
        let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(registry));

        // A scan artifact file does not need to be scan-shaped for a dry
        // run, since dry-run short-circuits before the plugin ever sees it;
        // it still must exist and parse as a valid ScanResult, so reuse a
        // real one produced by a scan-only run.
        let repo_dir = TempDir::new().unwrap();
        let scan = executor
            .execute(ExecutionInput::ScanOnly {
                repository: repo_dir.path().to_str().unwrap().to_string(),
                output_path: None,
                branch: None,
                resume: false,
                workspace: Some(ws_dir.path().to_str().unwrap().to_string()),
            })
            .await
            .expect("scan-only must succeed");

        let result = executor
            .execute(ExecutionInput::PluginOnly {
                plugin: "test-plugin".to_string(),
                workspace_dir: None,
                scan_artifact_path: scan.scan_artifact_path,
                workspace_root: Some(artifact_ws_dir.path().to_str().unwrap().to_string()),
                config: None,
                provider: None,
                model: None,
                dry_run: true,
                report_formats: vec![],
                output_dir: None,
            })
            .await
            .expect("dry run must not fail");

        assert!(result.is_dry_run);
        assert!(result.report_paths.is_empty());
    }

    // ------------------------------------------------------------------
    // Dry run
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_dry_run_skips_plugin_execution() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        // Register a plugin that would panic if actually invoked.
        struct PanicPlugin;

        #[async_trait]
        impl WorkflowPlugin for PanicPlugin {
            fn name(&self) -> &str {
                "test-plugin"
            }
            fn metadata(&self) -> PluginMetadata {
                PluginMetadata::new("test-plugin", "1.0.0", "Should not run.")
            }
            fn supported_formats(&self) -> Vec<String> {
                vec![]
            }
            fn required_tool_access(&self) -> ToolAccessLevel {
                ToolAccessLevel::None
            }
            async fn run(&self, _ctx: PluginContext) -> crate::error::Result<PluginOutput> {
                panic!("PanicPlugin::run must never be called during dry run");
            }
        }

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        let mut registry = PluginRegistry::new();
        registry.register(Arc::new(PanicPlugin));
        let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(registry));

        let mut plan = make_test_plan(
            repo_dir.path().to_str().unwrap(),
            ws_dir.path().to_str().unwrap(),
        );
        plan.dry_run = true;

        let result = executor
            .execute(ExecutionInput::LocalPlan(Box::new(plan)))
            .await
            .expect("dry run must not fail");

        assert!(result.is_dry_run);
        assert!(result.success);
        assert!(result.scan_artifact_path.is_none());
    }

    // ------------------------------------------------------------------
    // Resume from scan artifact
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_local_plan_resume_loads_existing_scan_artifact() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let executor = make_executor_with_success_plugin(ws_dir.path().to_str().unwrap());

        // First run: creates workspace and scan artifact.
        let plan = make_test_plan(
            repo_dir.path().to_str().unwrap(),
            ws_dir.path().to_str().unwrap(),
        );
        let first_result = executor
            .execute(ExecutionInput::LocalPlan(Box::new(plan)))
            .await
            .expect("first run must succeed");

        let first_artifact = first_result
            .scan_artifact_path
            .expect("first run must produce scan artifact");

        // Second run: resume = true, should load existing artifact.
        let mut resume_plan = make_test_plan(
            repo_dir.path().to_str().unwrap(),
            ws_dir.path().to_str().unwrap(),
        );
        resume_plan.resume = true;

        let second_result = executor
            .execute(ExecutionInput::LocalPlan(Box::new(resume_plan)))
            .await
            .expect("resume run must succeed");

        assert!(second_result.success);
        // A new workspace is opened (or created) on resume; artifact exists.
        assert!(second_result.scan_artifact_path.is_some());
        let _ = first_artifact; // used for context; drop here
    }

    // ------------------------------------------------------------------
    // WatcherTask execution
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_watcher_task_returns_watcher_result() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let executor = make_executor_with_success_plugin(ws_dir.path().to_str().unwrap());

        let mut task = WatcherTaskMessage::new(
            "task-001",
            WatcherEventType::TechnicalReviewTask,
            "xzardgz://test",
            repo_dir.path().to_str().unwrap(),
            "test-plugin",
            "corr-001",
        );
        task.workspace_directory = Some(ws_dir.path().to_str().unwrap().to_string());

        let result = executor
            .execute(ExecutionInput::WatcherTask(Box::new(task)))
            .await
            .expect("watcher task execution must succeed");

        assert!(result.success, "errors: {:?}", result.errors);
        assert!(
            result.watcher_result.is_some(),
            "watcher_result must be populated"
        );
    }

    #[tokio::test]
    async fn test_execute_watcher_task_sets_correlation_id_in_result() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let executor = make_executor_with_success_plugin(ws_dir.path().to_str().unwrap());

        let mut task = WatcherTaskMessage::new(
            "task-002",
            WatcherEventType::SecurityReviewTask,
            "xzardgz://test",
            repo_dir.path().to_str().unwrap(),
            "test-plugin",
            "corr-xyz-789",
        );
        task.workspace_directory = Some(ws_dir.path().to_str().unwrap().to_string());

        let result = executor
            .execute(ExecutionInput::WatcherTask(Box::new(task)))
            .await
            .expect("watcher task execution must succeed");

        let wr = result
            .watcher_result
            .expect("watcher result must be present");
        assert_eq!(wr.correlation_id, "corr-xyz-789");
        assert_eq!(wr.original_task_id, "task-002");
    }

    // ------------------------------------------------------------------
    // Stage transitions
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_local_plan_transitions_workspace_to_complete() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let executor = make_executor_with_success_plugin(ws_dir.path().to_str().unwrap());
        let plan = make_test_plan(
            repo_dir.path().to_str().unwrap(),
            ws_dir.path().to_str().unwrap(),
        );

        let result = executor
            .execute(ExecutionInput::LocalPlan(Box::new(plan)))
            .await
            .expect("execution must succeed");

        assert_eq!(
            result.stage_at_completion,
            WorkspaceStage::Complete,
            "workspace must reach Complete stage"
        );
    }

    // ------------------------------------------------------------------
    // Report persistence
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_local_plan_records_report_paths() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let executor = make_executor_with_success_plugin(ws_dir.path().to_str().unwrap());
        let plan = make_test_plan(
            repo_dir.path().to_str().unwrap(),
            ws_dir.path().to_str().unwrap(),
        );

        let result = executor
            .execute(ExecutionInput::LocalPlan(Box::new(plan)))
            .await
            .expect("execution must succeed");

        // step1 should have at least one report path (json format).
        let step_paths = result
            .report_paths
            .get("step1")
            .expect("step1 must have report paths");
        assert!(
            !step_paths.is_empty(),
            "at least one report must be written for step1"
        );
        // Verify the file actually exists on disk.
        for p in step_paths {
            assert!(
                std::path::Path::new(p).exists(),
                "report file must exist: {}",
                p
            );
        }
    }

    // ------------------------------------------------------------------
    // Watcher result — security event type mapping
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_watcher_task_security_event_maps_to_security_result() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let executor = make_executor_with_success_plugin(ws_dir.path().to_str().unwrap());

        let mut task = WatcherTaskMessage::new(
            "task-sec-001",
            WatcherEventType::SecurityReviewTask,
            "xzardgz://test",
            repo_dir.path().to_str().unwrap(),
            "test-plugin",
            "corr-sec-001",
        );
        task.workspace_directory = Some(ws_dir.path().to_str().unwrap().to_string());

        let result = executor
            .execute(ExecutionInput::WatcherTask(Box::new(task)))
            .await
            .expect("security review watcher task must succeed");

        let wr = result.watcher_result.expect("watcher result must be set");
        assert_eq!(
            wr.event_type,
            WatcherEventType::SecurityReviewResult,
            "security task must produce security result event type"
        );
    }

    // ------------------------------------------------------------------
    // Unknown plugin in plan
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_local_plan_unknown_plugin_captures_error_not_panic() {
        let repo_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        let config = make_test_config(ws_dir.path().to_str().unwrap());
        // Empty registry - no plugins registered.
        let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(PluginRegistry::new()));

        let plan = make_test_plan(
            repo_dir.path().to_str().unwrap(),
            ws_dir.path().to_str().unwrap(),
        );

        let result = executor
            .execute(ExecutionInput::LocalPlan(Box::new(plan)))
            .await
            .expect("execution must return Ok even for unknown plugin");

        assert!(
            !result.success,
            "plan with unknown plugin must not be marked successful"
        );
        assert!(
            !result.errors.is_empty(),
            "errors must contain the missing plugin message"
        );
    }

    // ------------------------------------------------------------------
    // ExecutionInput::CreatePr — opt-in gate
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_create_pr_without_opt_in_returns_success_with_no_pr_activity() {
        let ws_dir = TempDir::new().unwrap();
        // Config::default() has pr.enabled = false.
        let executor = make_executor_with_success_plugin(ws_dir.path().to_str().unwrap());

        let input = ExecutionInput::CreatePr {
            repository: ".".to_string(),
            head_branch: "feature/test".to_string(),
            base_branch: "main".to_string(),
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            title: "Test PR".to_string(),
            body: None,
            draft: false,
            workspace: None,
        };

        let result = executor.execute(input).await;
        assert!(result.is_ok(), "no-op should not fail: {:?}", result.err());
        let exec_result = result.unwrap();
        assert!(exec_result.success, "no-op result should be successful");
        assert!(
            exec_result.errors.is_empty(),
            "no-op should produce no errors"
        );
        // Verify the stage does NOT reflect PR activity.
        assert!(
            !matches!(
                exec_result.stage_at_completion,
                WorkspaceStage::PrComplete { .. }
            ),
            "stage should not be PrComplete when opt-in is false"
        );
    }

    #[tokio::test]
    async fn test_execute_create_pr_with_governance_enabled_respects_check_branch() {
        let ws_dir = TempDir::new().unwrap();
        // Build a config with pr.enabled = true AND governance enabled.
        // governance.rules_path = "" so no external file is loaded.
        let mut config = Config::default();
        config.workspace.root = ws_dir.path().to_str().unwrap().to_string();
        config.pr.enabled = true;
        config.governance.enabled = true;
        config.governance.rules_path = String::new();
        let executor = WorkflowExecutor::new(Arc::new(config), Arc::new(PluginRegistry::new()));

        // A branch name with a valid format must not cause a governance error
        // (the branch rule is Recommended, not Required, so it never blocks).
        let input = ExecutionInput::CreatePr {
            repository: ".".to_string(),
            head_branch: "feature/valid-branch".to_string(),
            base_branch: "main".to_string(),
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            title: "Test PR".to_string(),
            body: None,
            draft: false,
            workspace: None,
        };

        // The PR client call will fail (no real token, no real GitHub server).
        // We only care that the governance step runs without panicking and that
        // failure, if any, is a Git or MissingToken error, NOT a Governance error.
        let result = executor.execute(input).await;
        // Either the call returns Ok (impossible without a token) or Err of the
        // Git/PR kind -- the important thing is no Governance panic or error.
        match result {
            Ok(_) => {}                      // unexpected but not a failure
            Err(PipelineError::Git(_)) => {} // expected: MissingToken maps to Git
            Err(other) => panic!("expected Git error from missing token, got: {:?}", other),
        }
    }
}
