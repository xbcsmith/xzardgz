use super::diataxis::DocCategory;
use crate::error::{DocGenError, Result};
use handlebars::Handlebars;

pub struct TemplateRegistry {
    registry: Handlebars<'static>,
}

impl TemplateRegistry {
    pub fn new() -> Result<Self> {
        let mut registry = Handlebars::new();

        registry
            .register_template_string(
                DocCategory::Tutorial.as_str(),
                include_str!("templates/tutorial.hbs"),
            )
            .map_err(|e| DocGenError::Template(e.to_string()))?;

        registry
            .register_template_string(
                DocCategory::HowTo.as_str(),
                include_str!("templates/how_to.hbs"),
            )
            .map_err(|e| DocGenError::Template(e.to_string()))?;

        registry
            .register_template_string(
                DocCategory::Explanation.as_str(),
                include_str!("templates/explanation.hbs"),
            )
            .map_err(|e| DocGenError::Template(e.to_string()))?;

        registry
            .register_template_string(
                DocCategory::Reference.as_str(),
                include_str!("templates/reference.hbs"),
            )
            .map_err(|e| DocGenError::Template(e.to_string()))?;

        Ok(Self { registry })
    }

    pub fn render(&self, category: DocCategory, data: &serde_json::Value) -> Result<String> {
        self.registry
            .render(category.as_str(), data)
            .map_err(|e| DocGenError::Template(e.to_string()).into())
    }
}
