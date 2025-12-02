use serde_json::json;
use xzardgz::docgen::diataxis::DocCategory;
use xzardgz::docgen::templates::TemplateRegistry;

#[test]
fn test_doc_category_as_str() {
    assert_eq!(DocCategory::Tutorial.as_str(), "tutorial");
    assert_eq!(DocCategory::HowTo.as_str(), "how-to");
    assert_eq!(DocCategory::Explanation.as_str(), "explanation");
    assert_eq!(DocCategory::Reference.as_str(), "reference");
}

#[test]
fn test_doc_category_directory() {
    assert_eq!(DocCategory::Tutorial.directory(), "docs/tutorials");
    assert_eq!(DocCategory::HowTo.directory(), "docs/how_to");
    assert_eq!(DocCategory::Explanation.directory(), "docs/explanation");
    assert_eq!(DocCategory::Reference.directory(), "docs/reference");
}

#[test]
fn test_template_registry_creation() {
    let registry = TemplateRegistry::new();
    assert!(registry.is_ok());
}

#[test]
fn test_template_render_tutorial() {
    let registry = TemplateRegistry::new().unwrap();
    let data = json!({
        "title": "Test Tutorial",
        "introduction": "This is a test",
        "prerequisites": "None",
        "steps": [
            {"number": 1, "title": "First Step", "content": "Do this"}
        ],
        "conclusion": "Done"
    });

    let result = registry.render(DocCategory::Tutorial, &data);
    assert!(result.is_ok());
    let content = result.unwrap();
    assert!(content.contains("Test Tutorial"));
    assert!(content.contains("First Step"));
}

#[test]
fn test_template_render_how_to() {
    let registry = TemplateRegistry::new().unwrap();
    let data = json!({
        "title": "Do Something",
        "problem": "Need to do X",
        "solution": "Use Y",
        "steps": ["Step 1", "Step 2"],
        "discussion": "This works because..."
    });

    let result = registry.render(DocCategory::HowTo, &data);
    assert!(result.is_ok());
    let content = result.unwrap();
    assert!(content.contains("How to Do Something"));
}
