use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub repository: Option<String>,
    pub steps: Vec<WorkflowStep>,
    #[serde(default)]
    pub deliverables: Vec<Deliverable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: String,
    pub description: String,
    pub action: Action,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "params")]
pub enum Action {
    #[serde(rename = "scan_repository")]
    ScanRepository,
    #[serde(rename = "analyze_code")]
    AnalyzeCode,
    #[serde(rename = "generate_docs")]
    GenerateDocumentation { category: DocCategory },
    #[serde(rename = "execute_command")]
    ExecuteCommand { command: String },
    #[serde(rename = "agent_task")]
    AgentTask { prompt: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocCategory {
    Tutorial,
    HowTo,
    Explanation,
    Reference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deliverable {
    pub name: String,
    pub description: String,
    pub path: String,
}

impl Plan {
    pub fn validate(&self) -> Result<(), String> {
        // 1. Check for duplicate step IDs
        let mut ids = std::collections::HashSet::new();
        for step in &self.steps {
            if !ids.insert(&step.id) {
                return Err(format!("Duplicate step ID: {}", step.id));
            }
        }

        // 2. Check dependencies exist
        for step in &self.steps {
            for dep in &step.dependencies {
                if !ids.contains(dep) {
                    return Err(format!("Step {} depends on unknown step {}", step.id, dep));
                }
            }
        }

        Ok(())
    }
}
