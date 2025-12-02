use crate::agent::core::Agent;
use crate::error::WorkflowError;
use crate::workflow::plan::{Action, Plan, WorkflowStep};
use std::collections::HashSet;
use std::sync::Arc;

pub struct WorkflowExecutor {
    agent: Arc<Agent>,
    plan: Plan,
    completed_steps: HashSet<String>,
}

impl WorkflowExecutor {
    pub fn new(agent: Arc<Agent>, plan: Plan) -> Self {
        Self {
            agent,
            plan,
            completed_steps: HashSet::new(),
        }
    }

    pub async fn execute(&mut self) -> Result<(), WorkflowError> {
        // Simple execution loop: find executable steps, execute them, repeat.
        loop {
            let executable_steps = self.get_executable_steps();
            if executable_steps.is_empty() {
                if self.completed_steps.len() == self.plan.steps.len() {
                    break; // All done
                } else {
                    return Err(WorkflowError::Execution(
                        "Deadlock or missing dependencies detected".to_string(),
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

    async fn execute_step(&self, step: &WorkflowStep) -> Result<(), WorkflowError> {
        match &step.action {
            Action::ScanRepository => {
                println!("Scanning repository...");
                // TODO: Integrate RepositoryScanner
                Ok(())
            }
            Action::AnalyzeCode => {
                println!("Analyzing code...");
                Ok(())
            }
            Action::GenerateDocumentation { category } => {
                println!("Generating documentation for category: {:?}", category);
                Ok(())
            }
            Action::ExecuteCommand { command } => {
                println!("Executing command: {}", command);
                // TODO: Execute command safely
                Ok(())
            }
            Action::AgentTask { prompt } => {
                println!("Agent task: {}", prompt);
                self.agent
                    .run(prompt)
                    .await
                    .map_err(|e| WorkflowError::Execution(e.to_string()))?;
                Ok(())
            }
        }
    }
}
