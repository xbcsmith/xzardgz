use xzardgz::workflow::parser::{JsonPlanParser, MarkdownPlanParser, PlanParser, YamlPlanParser};
use xzardgz::workflow::plan::Action;

#[test]
fn test_yaml_parsing() {
    let yaml = r#"
name: Test Plan
description: A test plan
steps:
  - id: step1
    description: Scan repo
    action:
      type: scan_repository
      params: null
"#;
    let plan = YamlPlanParser.parse(yaml).unwrap();
    assert_eq!(plan.name, "Test Plan");
    assert_eq!(plan.steps.len(), 1);
    match plan.steps[0].action {
        Action::ScanRepository => (),
        _ => panic!("Wrong action type"),
    }
}

#[test]
fn test_json_parsing() {
    let json = r#"{
    "name": "Test Plan",
    "description": "A test plan",
    "steps": [
        {
            "id": "step1",
            "description": "Scan repo",
            "action": {
                "type": "scan_repository",
                "params": null
            }
        }
    ],
    "deliverables": []
}"#;
    let plan = JsonPlanParser.parse(json).unwrap();
    assert_eq!(plan.name, "Test Plan");
}

#[test]
fn test_markdown_parsing() {
    let md = r#"
# My Plan

Here is the plan:

```yaml
name: Test Plan
description: A test plan
steps:
  - id: step1
    description: Scan repo
    action:
      type: scan_repository
      params: null
```
"#;
    let plan = MarkdownPlanParser.parse(md).unwrap();
    assert_eq!(plan.name, "Test Plan");
}
