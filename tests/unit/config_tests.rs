use xzardgz::config::Config;

#[test]
fn test_default_config() {
    let config = Config::default();
    assert_eq!(config.provider.provider_type, "ollama");
    assert_eq!(config.agent.max_turns, 10);
}

#[test]
fn test_load_config_env() {
    temp_env::with_var("XZARDGZ_PROVIDER", Some("copilot"), || {
        let config = Config::load().unwrap();
        assert_eq!(config.provider.provider_type, "copilot");
    });
}
