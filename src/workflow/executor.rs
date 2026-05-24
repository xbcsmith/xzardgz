//! Workflow plan executor for plugin-first plans.
//!
//! This module provides [`WorkflowExecutor`], which runs a [`WorkflowPlan`]
//! step by step while respecting declared inter-step dependencies. Steps are
//! executed in topological order: a step becomes eligible once all of its
//! dependencies are recorded as complete.

use crate::agent::core::Agent;
use crate::error::{PipelineError, Result};
use crate::workflow::plan::{PluginStep, WorkflowPlan};
use std::collections::HashSet;
use std::sync::Arc;

/// Executes a [`WorkflowPlan`] step by step, respecting declared dependencies.
///
/// The executor iterates over the plan's steps in dependency order. On each
/// iteration it collects all steps whose dependencies have already completed
/// and executes them. If no progress can be made before all steps complete,
/// the executor returns a [`PipelineError::Workflow`] describing the deadlock.
pub struct WorkflowExecutor {
    agent: Arc<Agent>,
    plan: WorkflowPlan,
    completed_steps: HashSet<String>,
}

impl WorkflowExecutor {
    /// Creates a new [`WorkflowExecutor`] bound to the given agent and plan.
    ///
    /// # Arguments
    ///
    /// * `agent` - The agent used for any LLM-backed step execution.
    /// * `plan` - The validated [`WorkflowPlan`] to execute.
    pub fn new(agent: Arc<Agent>, plan: WorkflowPlan) -> Self {
        Self {
            agent,
            plan,
            completed_steps: HashSet::new(),
        }
    }

    /// Runs the workflow plan to completion, executing steps in dependency
    /// order.
    ///
    /// The executor loops until all steps have been marked complete or a
    /// deadlock is detected. A deadlock occurs when unfinished steps remain
    /// but none of them are currently eligible (i.e., they all have at least
    /// one incomplete dependency).
    ///
    /// When the plan's `dry_run` flag is set, steps are logged but their
    /// plugin logic is not invoked.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workflow`] if a deadlock is detected (circular
    /// or missing dependencies prevent all steps from completing), or if any
    /// individual step fails.
    pub async fn execute(&mut self) -> Result<()> {
        loop {
            let executable_steps = self.get_executable_steps();
            if executable_steps.is_empty() {
                if self.completed_steps.len() == self.plan.steps.len() {
                    break;
                } else {
                    return Err(PipelineError::Workflow(
                        "deadlock or missing dependencies detected in workflow plan".to_string(),
                    ));
                }
            }

            for step in executable_steps {
                println!("Executing step: {} (plugin: {})", step.id, step.plugin);
                self.execute_step(&step).await?;
                self.completed_steps.insert(step.id.clone());
            }
        }
        Ok(())
    }

    /// Returns the list of steps that are currently eligible for execution.
    ///
    /// A step is eligible when it has not yet been completed and all of its
    /// declared dependency step IDs appear in `completed_steps`.
    fn get_executable_steps(&self) -> Vec<PluginStep> {
        self.plan
            .steps
            .iter()
            .filter(|step| !self.completed_steps.contains(&step.id))
            .filter(|step| {
                step.dependencies
                    .iter()
                    .all(|dep| self.completed_steps.contains(dep))
            })
            .cloned()
            .collect()
    }

    /// Executes a single plugin step.
    ///
    /// In dry-run mode the step is only logged; no plugin logic is invoked.
    /// Otherwise, a placeholder message is printed. Actual plugin dispatch
    /// will be wired to the plugin registry in a future phase.
    ///
    /// # Arguments
    ///
    /// * `step` - The [`PluginStep`] to execute.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Workflow`] if step execution fails.
    async fn execute_step(&self, step: &PluginStep) -> Result<()> {
        if self.plan.is_dry_run() {
            println!(
                "[dry-run] step '{}': would invoke plugin '{}'",
                step.id, step.plugin
            );
            return Ok(());
        }

        // Placeholder: actual plugin dispatch is handled by the plugin
        // registry in a future phase. The agent is available for LLM-backed
        // plugins via self.agent.
        println!(
            "step '{}': invoking plugin '{}' on repository '{}'",
            step.id, step.plugin, self.plan.repository
        );

        // Suppress the unused-field warning until the real dispatch is wired.
        let _ = &self.agent;

        Ok(())
    }
}
