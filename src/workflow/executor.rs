use crate::agent::core::Agent;
use crate::error::{PipelineError, Result};
use crate::workflow::plan::{Action, Plan, WorkflowStep};
use std::collections::HashSet;
use std::sync::Arc;

/// Executes a workflow plan step by step, respecting declared dependencies between steps.
pub struct WorkflowExecutor {
    agent: Arc<Agent>,
    plan: Plan,
    completed_steps: HashSet<String>,
}

impl WorkflowExecutor {
    /// Creates a new `WorkflowExecutor` with the given agent and plan.
    pub fn new(agent: Arc<Agent>, plan: Plan) -> Self {
        Self {
            agent,
            plan,
            completed_steps: HashSet::new(),
        }
    }

    /// Runs the workflow plan to completion, executing steps in dependency order.
    ///
    /// Returns `PipelineError::Workflow` if a deadlock is detected (circular or
    /// missing dependencies prevent all steps from completing).
    pub async fn execute(&mut self) -> Result<()> {
        loop {
            let executable_steps = self.get_executable_steps();
            if executable_steps.is_empty() {
                if self.completed_steps.len() == self.plan.steps.len() {
                    break;
                } else {
                    return Err(PipelineError::Workflow(
                        "deadlock or missing dependencies detected".to_string(),
                    ));
                }
            }

            for step in executable_steps {
                println!("Executing step: {}", step.id);
                self.execute_step(&step).await?;
                self.completed_steps.insert(step.id.clone());
            }
        }
        Ok(())
    }

    fn get_executable_steps(&self) -> Vec<WorkflowStep> {
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

    async fn execute_step(&self, step: &WorkflowStep) -> Result<()> {
        match &step.action {
            Action::ScanRepository => {
                println!("Scanning repository...");
                Ok(())
            }
            Action::AnalyzeCode => {
                println!("Analyzing code...");
                Ok(())
            }
            Action::RunPlugin { plugin } => {
                println!("Running plugin: {}", plugin);
                Ok(())
            }
            Action::ExecuteCommand { command } => {
                println!("Executing command: {}", command);
                Ok(())
            }
            Action::AgentTask { prompt } => {
                println!("Agent task: {}", prompt);
                self.agent
                    .run(prompt)
                    .await
                    .map_err(|e| PipelineError::Workflow(e.to_string()))?;
                Ok(())
            }
        }
    }
}
