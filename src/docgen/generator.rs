use super::diataxis::DocCategory;
use super::templates::TemplateRegistry;
use crate::agent::core::Agent;
use crate::error::Result;
use std::sync::Arc;

pub struct DocGenerator {
    agent: Arc<Agent>,
    templates: Arc<TemplateRegistry>,
}

impl DocGenerator {
    pub fn new(agent: Arc<Agent>, templates: Arc<TemplateRegistry>) -> Self {
        Self { agent, templates }
    }

    pub async fn generate(
        &self,
        category: DocCategory,
        topic: &str,
        context: &str,
    ) -> Result<String> {
        // 1. Build prompt
        let prompt = self.build_prompt(category, topic, context);

        // 2. Call agent
        let content_json = self.agent.run(&prompt).await?;

        // 3. Clean up potential markdown code blocks from response
        let clean_json = self.clean_json_response(&content_json);

        // 4. Parse JSON content
        let data: serde_json::Value = serde_json::from_str(&clean_json).map_err(|e| {
            crate::error::DocGenError::Generation(format!(
                "Failed to parse agent response as JSON: {}. Response: {}",
                e, clean_json
            ))
        })?;

        // 5. Render template
        self.templates.render(category, &data)
    }

    fn clean_json_response(&self, response: &str) -> String {
        let response = response.trim();
        if response.starts_with("```json") {
            response
                .trim_start_matches("```json")
                .trim_end_matches("```")
                .trim()
                .to_string()
        } else if response.starts_with("```") {
            response
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim()
                .to_string()
        } else {
            response.to_string()
        }
    }

    fn build_prompt(&self, category: DocCategory, topic: &str, context: &str) -> String {
        format!(
            r#"You are a technical documentation writer. Generate a {category} document for "{topic}".

            Repository Context:
            {context}

            Output Format:
            You must return a valid JSON object that matches the structure required for the {category} template.
            Do not include any markdown formatting (like ```json) around the output. Just the raw JSON.

            Template Structure for {category}:
            {structure_hint}

            Guidelines:
            - Clear and concise language
            - Appropriate technical depth for {category}
            - Code examples where relevant
            "#,
            category = category.as_str(),
            topic = topic,
            context = context,
            structure_hint = self.get_structure_hint(category)
        )
    }

    fn get_structure_hint(&self, category: DocCategory) -> &'static str {
        match category {
            DocCategory::Tutorial => {
                r#"{
                "title": "Tutorial Title",
                "introduction": "...",
                "prerequisites": "...",
                "steps": [
                    { "number": 1, "title": "Step 1", "content": "..." }
                ],
                "conclusion": "..."
            }"#
            }
            DocCategory::HowTo => {
                r#"{
                "title": "How to ...",
                "problem": "...",
                "solution": "...",
                "steps": ["Step 1", "Step 2"],
                "discussion": "..."
            }"#
            }
            DocCategory::Explanation => {
                r#"{
                "title": "Topic",
                "overview": "...",
                "concepts": "...",
                "architecture": "...",
                "design_decisions": "..."
            }"#
            }
            DocCategory::Reference => {
                r#"{
                "title": "Reference",
                "description": "...",
                "usage": "...",
                "api": "...",
                "examples": "..."
            }"#
            }
        }
    }
}
