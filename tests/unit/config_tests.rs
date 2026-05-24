// Allow unused imports: the spec mandates this import list for documentation and
// forward-compatibility; not all types are used as explicit type annotations in
// every test, but they form the public API surface under test.
#![allow(unused_imports)]

use std::collections::HashMap;

use xzardgz::config::{
    Config, ConfigOverrides, KafkaConfig, MatcherConfig, McpConfig, McpServerConfig,
    ModelSelectionConfig, ModelSelectionOverrides, PluginsConfig, TechnicalReviewConfig,
};

// ---------------------------------------------------------------------------
// Default value tests
// ---------------------------------------------------------------------------

/// The default provider must be "openai".
#[test]
fn test_default_config_uses_openai() {
    assert_eq!(Config::default().provider.default, "openai");
}

/// Model selection must be enabled by default.
#[test]
fn test_default_model_selection_enabled() {
    assert!(Config::default().model_selection.enabled);
}

/// auto_fallback must default to true.
#[test]
fn test_default_model_selection_auto_fallback() {
    assert!(Config::default().model_selection.auto_fallback);
}

/// require_tools must default to true.
#[test]
fn test_default_model_selection_require_tools() {
    assert!(Config::default().model_selection.require_tools);
}

/// require_structured_output must default to true.
#[test]
fn test_default_model_selection_require_structured_output() {
    assert!(Config::default().model_selection.require_structured_output);
}

/// min_context_tokens must default to 16 000.
#[test]
fn test_default_model_selection_min_context_tokens() {
    assert_eq!(Config::default().model_selection.min_context_tokens, 16000);
}

// ---------------------------------------------------------------------------
// load_from_str tests
// ---------------------------------------------------------------------------

/// A minimal valid YAML document should parse successfully and reflect the
/// supplied provider while defaulting every other field.
#[test]
fn test_load_from_str_with_valid_yaml() {
    let yaml = "provider:\n  default: \"ollama\"\n";
    // SAFETY: the YAML is syntactically valid and contains only recognised
    // fields, so load_from_str is expected to succeed.
    let config = Config::load_from_str(yaml).unwrap();
    assert_eq!(config.provider.default, "ollama");
    // model_selection was not specified; the default (enabled = true) must be
    // preserved through field-level merging.
    assert!(config.model_selection.enabled);
}

/// Fields absent from the YAML document must retain their default values.
#[test]
fn test_field_level_merging_uses_defaults_for_missing_fields() {
    let yaml = "provider:\n  default: \"ollama\"\n";
    // SAFETY: the YAML is syntactically valid and should parse successfully.
    let config = Config::load_from_str(yaml).unwrap();
    assert_eq!(config.model_selection.min_context_tokens, 16000);
    assert_eq!(config.openai.endpoint, "https://api.openai.com/v1");
}

/// An invalid YAML document must return an error, not a default config.
#[test]
fn test_load_from_str_with_invalid_yaml() {
    let result = Config::load_from_str("invalid: : yaml: :");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Legacy field rejection tests
// ---------------------------------------------------------------------------

/// The removed `documentation` top-level key must be rejected on load.
#[test]
fn test_load_from_str_rejects_legacy_documentation_field() {
    let result = Config::load_from_str("documentation:\n  something: true\n");
    assert!(result.is_err());
    // SAFETY: we just asserted result.is_err(), so unwrap_err cannot panic.
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("documentation"),
        "expected 'documentation' in error message, got: {}",
        err_msg,
    );
}

/// The removed `export_scan` top-level key must be rejected on load.
#[test]
fn test_load_from_str_rejects_legacy_export_scan_field() {
    let result = Config::load_from_str("export_scan:\n  path: /tmp\n");
    assert!(result.is_err());
    // SAFETY: we just asserted result.is_err(), so unwrap_err cannot panic.
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("export_scan"),
        "expected 'export_scan' in error message, got: {}",
        err_msg,
    );
}

// ---------------------------------------------------------------------------
// Environment variable override tests
// ---------------------------------------------------------------------------

/// The XZARDGZ_PROVIDER environment variable must override the value from the
/// YAML document when load_from_str is called.
#[test]
fn test_env_override_sets_provider() {
    temp_env::with_var("XZARDGZ_PROVIDER", Some("ollama"), || {
        let yaml = "provider:\n  default: \"openai\"\n";
        // SAFETY: the YAML is syntactically valid and the function is expected
        // to succeed; the env var override is also well-formed.
        let config = Config::load_from_str(yaml).unwrap();
        assert_eq!(
            config.provider.default, "ollama",
            "XZARDGZ_PROVIDER env var should win over YAML value",
        );
    });
}

/// The XZARDGZ_OLLAMA_MODEL environment variable must override the Ollama
/// model field when load_from_str is called.
#[test]
fn test_env_override_sets_ollama_model() {
    temp_env::with_var("XZARDGZ_OLLAMA_MODEL", Some("llama3"), || {
        let yaml = "provider:\n  default: \"ollama\"\n";
        // SAFETY: the YAML is syntactically valid and the function is expected
        // to succeed; the env var override is also well-formed.
        let config = Config::load_from_str(yaml).unwrap();
        assert_eq!(
            config.ollama.model, "llama3",
            "XZARDGZ_OLLAMA_MODEL env var should override the default model",
        );
    });
}

// ---------------------------------------------------------------------------
// CLI / apply_overrides tests
// ---------------------------------------------------------------------------

/// apply_overrides must set provider.default when ConfigOverrides::provider is
/// Some.
#[test]
fn test_cli_override_sets_provider() {
    let mut config = Config::default();
    let overrides = ConfigOverrides {
        provider: Some("copilot".to_string()),
        ..Default::default()
    };
    config.apply_overrides(&overrides);
    assert_eq!(config.provider.default, "copilot");
}

/// apply_overrides must set workspace.root when ConfigOverrides::workspace_root
/// is Some.
#[test]
fn test_cli_override_sets_workspace_root() {
    let mut config = Config::default();
    let overrides = ConfigOverrides {
        workspace_root: Some("/tmp/ws".to_string()),
        ..Default::default()
    };
    config.apply_overrides(&overrides);
    assert_eq!(config.workspace.root, "/tmp/ws");
}

// ---------------------------------------------------------------------------
// validate() — endpoint security
// ---------------------------------------------------------------------------

/// validate() must reject a plain-HTTP endpoint when allow_insecure_endpoint
/// is false.
#[test]
fn test_validate_rejects_insecure_openai_endpoint() {
    let mut config = Config::default();
    config.openai.allow_insecure_endpoint = false;
    config.openai.endpoint = "http://evil.com/v1".to_string();
    let result = config.validate();
    assert!(result.is_err());
    // SAFETY: we just asserted result.is_err(), so unwrap_err cannot panic.
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("HTTPS"),
        "error should mention 'HTTPS', got: {}",
        err_msg,
    );
}

/// validate() must accept the default config whose endpoint already uses HTTPS.
#[test]
fn test_validate_allows_https_openai_endpoint() {
    // Default endpoint is "https://api.openai.com/v1" with insecure = false.
    let result = Config::default().validate();
    assert!(result.is_ok());
}

/// validate() must accept a plain-HTTP endpoint when allow_insecure_endpoint
/// is explicitly set to true.
#[test]
fn test_validate_allows_insecure_endpoint_when_opted_in() {
    let mut config = Config::default();
    config.openai.allow_insecure_endpoint = true;
    config.openai.endpoint = "http://localhost:8080/v1".to_string();
    let result = config.validate();
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// MatcherConfig tests
// ---------------------------------------------------------------------------

/// A MatcherConfig with all-empty fields must report is_empty() == true.
#[test]
fn test_empty_matcher_is_empty() {
    let matcher = MatcherConfig {
        event_types: vec![],
        repositories: vec![],
        plugins: vec![],
        platforms: vec![],
        metadata: HashMap::new(),
    };
    assert!(matcher.is_empty());
}

/// A MatcherConfig with at least one event_type must report is_empty() == false.
#[test]
fn test_non_empty_matcher_is_not_empty() {
    let matcher = MatcherConfig {
        event_types: vec!["push".to_string()],
        repositories: vec![],
        plugins: vec![],
        platforms: vec![],
        metadata: HashMap::new(),
    };
    assert!(!matcher.is_empty());
}

// ---------------------------------------------------------------------------
// Plugin config tests
// ---------------------------------------------------------------------------

/// The default plugin list must include both "technical-review" and
/// "security-review".
#[test]
fn test_plugins_config_enabled_list() {
    let plugins: PluginsConfig = Config::default().plugins;
    assert!(
        plugins.enabled.contains(&"technical-review".to_string()),
        "expected 'technical-review' in default enabled plugins, got: {:?}",
        plugins.enabled,
    );
    assert!(
        plugins.enabled.contains(&"security-review".to_string()),
        "expected 'security-review' in default enabled plugins, got: {:?}",
        plugins.enabled,
    );
}

// ---------------------------------------------------------------------------
// Model selection config validation tests
// ---------------------------------------------------------------------------

/// validate() must reject a config where min_context_tokens is zero.
#[test]
fn test_model_selection_validation_rejects_zero_min_context_tokens() {
    let mut config = Config::default();
    config.model_selection.min_context_tokens = 0;
    let result = config.validate();
    assert!(
        result.is_err(),
        "validate() should reject min_context_tokens == 0",
    );
}

// ---------------------------------------------------------------------------
// Model selection override merging tests
// ---------------------------------------------------------------------------

/// merge_model_selection must replace preferred_models while leaving all other
/// model selection fields at their defaults.
#[test]
fn test_model_selection_override_merging() {
    let mut config = Config::default();
    let overrides = ModelSelectionOverrides {
        preferred_models: Some(vec!["custom-model".to_string()]),
        ..Default::default()
    };
    config.merge_model_selection(&overrides);
    assert_eq!(
        config.model_selection.preferred_models,
        vec!["custom-model".to_string()],
    );
    // Unset fields must remain at their defaults.
    assert!(
        config.model_selection.auto_fallback,
        "auto_fallback should remain true after partial override",
    );
}

// ---------------------------------------------------------------------------
// Kafka config tests
// ---------------------------------------------------------------------------

/// The default Kafka config must have at least one broker and must use the
/// PLAINTEXT security protocol.
#[test]
fn test_kafka_config_defaults() {
    let kafka: KafkaConfig = Config::default().kafka;
    assert!(
        !kafka.brokers.is_empty(),
        "default kafka config should have at least one broker",
    );
    assert_eq!(
        kafka.security_protocol, "PLAINTEXT",
        "default security_protocol should be 'PLAINTEXT'",
    );
}

// ---------------------------------------------------------------------------
// MCP config tests
// ---------------------------------------------------------------------------

/// The default MCP config must have an empty server list and a 30-second
/// global timeout.
#[test]
fn test_mcp_config_defaults_to_empty_servers() {
    let mcp: McpConfig = Config::default().mcp;
    assert!(
        mcp.servers.is_empty(),
        "default MCP servers list must be empty"
    );
    assert_eq!(
        mcp.timeout_seconds, 30,
        "default MCP timeout must be 30 seconds"
    );
}

/// validate() must reject an MCP server config where timeout_seconds is zero.
#[test]
fn test_mcp_server_timeout_validation() {
    let mut config = Config::default();
    config.mcp.servers.push(McpServerConfig {
        name: "test".to_string(),
        command: "echo".to_string(),
        timeout_seconds: 0,
        ..Default::default()
    });
    let result = config.validate();
    assert!(
        result.is_err(),
        "validate() should reject timeout_seconds == 0"
    );
    // SAFETY: we just asserted result.is_err(), so unwrap_err cannot panic.
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("timeout"),
        "error should mention 'timeout', got: {}",
        err_msg,
    );
}
