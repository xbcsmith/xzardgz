use xzardgz::workflow::parser::{
    JsonPlanParser, MarkdownPlanParser, PlanParser, YamlPlanParser, parse_plan,
};

// ---------------------------------------------------------------------------
// YAML parser tests
// ---------------------------------------------------------------------------

#[test]
fn test_yaml_parser_parses_plugin_first_plan_correctly() {
    let yaml = concat!(
        "version: \"1\"\n",
        "name: Test Plan\n",
        "repository: \".\"\n",
        "steps:\n",
        "  - id: step1\n",
        "    plugin: technical-review\n",
    );
    let result = YamlPlanParser.parse(yaml);
    assert!(
        result.is_ok(),
        "valid plugin-first YAML plan should parse without errors: {:?}",
        result.err()
    );
    let plan = result.unwrap();
    assert_eq!(plan.name, "Test Plan");
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].id, "step1");
    assert_eq!(plan.steps[0].plugin, "technical-review");
}

#[test]
fn test_yaml_parser_rejects_legacy_scan_repository_action() {
    let yaml = concat!(
        "name: Legacy Plan\n",
        "description: A plan using the old scan_repository action type\n",
        "steps:\n",
        "  - id: step1\n",
        "    description: Scan repo\n",
        "    action:\n",
        "      type: scan_repository\n",
        "      params: null\n",
    );
    let result = YamlPlanParser.parse(yaml);
    assert!(
        result.is_err(),
        "legacy scan_repository action type must be rejected"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("scan_repository"),
        "error message should name the rejected legacy type, got: {err_msg}"
    );
}

#[test]
fn test_yaml_parser_rejects_legacy_generate_docs_action() {
    let yaml = concat!(
        "name: Legacy Doc Gen Plan\n",
        "description: A plan using the removed documentation generation action\n",
        "steps:\n",
        "  - id: step1\n",
        "    description: Generate docs\n",
        "    action:\n",
        "      type: generate_docs\n",
        "      params:\n",
        "        category: tutorial\n",
    );
    let result = YamlPlanParser.parse(yaml);
    assert!(
        result.is_err(),
        "legacy generate_docs workflow action should be rejected"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("generate_docs"),
        "error message should name the rejected legacy type, got: {err_msg}"
    );
}

#[test]
fn test_yaml_parser_rejects_wrong_version() {
    let yaml = concat!(
        "version: \"2\"\n",
        "name: Test Plan\n",
        "repository: \".\"\n",
        "steps:\n",
        "  - id: step1\n",
        "    plugin: technical-review\n",
    );
    let result = YamlPlanParser.parse(yaml);
    assert!(
        result.is_err(),
        "plan declaring schema version 2 must be rejected"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("is not supported"),
        "error message should describe the version rejection, got: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// JSON parser tests
// ---------------------------------------------------------------------------

#[test]
fn test_json_parser_parses_plugin_first_plan_correctly() {
    let json = r#"{
        "version": "1",
        "name": "Test Plan",
        "repository": ".",
        "steps": [
            {
                "id": "step1",
                "plugin": "technical-review"
            }
        ]
    }"#;
    let result = JsonPlanParser.parse(json);
    assert!(
        result.is_ok(),
        "valid plugin-first JSON plan should parse without errors: {:?}",
        result.err()
    );
    let plan = result.unwrap();
    assert_eq!(plan.name, "Test Plan");
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].plugin, "technical-review");
}

// ---------------------------------------------------------------------------
// Markdown parser tests
// ---------------------------------------------------------------------------

#[test]
fn test_markdown_parser_parses_plan_from_yaml_fenced_block() {
    let md = concat!(
        "# My Workflow Plan\n",
        "\n",
        "This plan runs a technical review plugin.\n",
        "\n",
        "```yaml\n",
        "version: \"1\"\n",
        "name: Test Plan\n",
        "repository: \".\"\n",
        "steps:\n",
        "  - id: step1\n",
        "    plugin: technical-review\n",
        "```\n",
    );
    let result = MarkdownPlanParser.parse(md);
    assert!(
        result.is_ok(),
        "valid plan embedded in markdown should parse: {:?}",
        result.err()
    );
    let plan = result.unwrap();
    assert_eq!(plan.name, "Test Plan");
    assert_eq!(plan.steps[0].plugin, "technical-review");
}

// ---------------------------------------------------------------------------
// parse_plan format dispatch tests
// ---------------------------------------------------------------------------

#[test]
fn test_parse_plan_rejects_unsupported_format() {
    let result = parse_plan("anything", "toml");
    assert!(
        result.is_err(),
        "an unsupported format string must produce an error"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("unsupported format"),
        "error message should describe the format problem, got: {err_msg}"
    );
}
