use xzardgz::error::{ConfigError, PipelineError, XzardgzError};

#[test]
fn test_error_conversion() {
    let config_err = ConfigError::Load("test failure".to_string());
    let app_err: XzardgzError = config_err.into();

    match app_err {
        PipelineError::Config(msg) => assert!(msg.contains("test failure")),
        _ => panic!("Wrong error type"),
    }
}

#[test]
fn test_error_display() {
    let err = ConfigError::Validation("invalid field".to_string());
    assert_eq!(err.to_string(), "Validation error: invalid field");

    let app_err: XzardgzError = err.into();
    assert!(app_err.to_string().contains("invalid field"));
}
