use xzardgz::error::{ConfigError, XzardgzError};

#[test]
fn test_error_conversion() {
    let config_err = ConfigError::Load("test failure".to_string());
    let app_err: XzardgzError = config_err.into();

    match app_err {
        XzardgzError::Config(ConfigError::Load(msg)) => assert_eq!(msg, "test failure"),
        _ => panic!("Wrong error type"),
    }
}

#[test]
fn test_error_display() {
    let err = ConfigError::Validation("invalid field".to_string());
    assert_eq!(err.to_string(), "Validation error: invalid field");

    let app_err: XzardgzError = err.into();
    assert_eq!(
        app_err.to_string(),
        "Configuration error: Validation error: invalid field"
    );
}
