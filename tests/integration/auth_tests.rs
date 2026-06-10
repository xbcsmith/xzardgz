//! Integration tests for OpenAI authentication flows and provider behavior.
//!
//! Covers credential lookup via [`xzardgz::auth::OpenAiAuth`] and the full
//! HTTP request/response cycle for [`xzardgz::providers::openai::OpenAiProvider`]
//! using a wiremock mock server, so no real network calls are made during the
//! test run.

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use xzardgz::auth::OpenAiAuth;
use xzardgz::auth::types::AuthStatus;
use xzardgz::config::{OpenAiConfig, ProviderDefaultsConfig};
use xzardgz::error::PipelineError;
use xzardgz::providers::Message;
use xzardgz::providers::base::Provider;
use xzardgz::providers::openai::OpenAiProvider;

// ---------------------------------------------------------------------------
// OpenAiAuth::status tests
// ---------------------------------------------------------------------------

/// Tests that [`OpenAiAuth::status`] returns [`AuthStatus::CredentialPresent`]
/// when the configured environment variable holds a non-empty value.
///
/// Uses [`temp_env::with_var`] to scope the variable to the closure lifetime,
/// guaranteeing restoration even on assertion failure.
#[test]
fn test_openai_auth_status_present_when_env_var_set() {
    temp_env::with_var("OPENAI_API_KEY", Some("test-key"), || {
        let auth = OpenAiAuth::new("OPENAI_API_KEY");
        let status = auth.status();
        assert!(
            matches!(status, AuthStatus::CredentialPresent { .. }),
            "expected CredentialPresent when env var is set, got: {:?}",
            status
        );
    });
}

/// Tests that [`OpenAiAuth::status`] returns [`AuthStatus::NotAuthenticated`]
/// when the configured environment variable is absent and no keyring entry
/// exists under the default service name.
///
/// Uses [`temp_env::with_var`] with `None` to unset the variable for the
/// duration of the closure, restoring any previous value afterward.
#[test]
fn test_openai_auth_status_missing_when_env_var_absent() {
    temp_env::with_var("OPENAI_API_KEY", None::<&str>, || {
        let auth = OpenAiAuth::new("OPENAI_API_KEY");
        let status = auth.status();
        assert!(
            matches!(status, AuthStatus::NotAuthenticated { .. }),
            "expected NotAuthenticated when env var is absent and keyring is empty, got: {:?}",
            status
        );
    });
}

// ---------------------------------------------------------------------------
// OpenAiAuth::get_key tests
// ---------------------------------------------------------------------------

/// Tests that [`OpenAiAuth::get_key`] returns the exact string value stored in
/// the configured environment variable.
///
/// Uses a unique environment variable name to avoid interference with any
/// other test running in the same process.
#[test]
fn test_openai_auth_get_key_returns_value_from_env() {
    // SAFETY: "XZARDGZ_IT_AUTH_GETKEY_TEST" is a unique name used only in
    // this test; no other test in the suite reads or writes this variable,
    // eliminating data-race risk across concurrent test threads.
    unsafe {
        std::env::set_var("XZARDGZ_IT_AUTH_GETKEY_TEST", "my-test-key-value");
    }

    let auth = OpenAiAuth::new("XZARDGZ_IT_AUTH_GETKEY_TEST");
    let key = auth.get_key();

    // SAFETY: mirrors the set_var above; no concurrent access to this variable.
    unsafe {
        std::env::remove_var("XZARDGZ_IT_AUTH_GETKEY_TEST");
    }

    assert_eq!(
        key,
        Some("my-test-key-value".to_string()),
        "get_key must return the exact string stored in the environment variable"
    );
}

// ---------------------------------------------------------------------------
// OpenAiProvider + wiremock tests
// ---------------------------------------------------------------------------

/// Tests that [`OpenAiProvider::list_models`] fetches and parses the `/models`
/// endpoint response from a wiremock mock server, returning entries that
/// include the expected `gpt-4.1-mini` model identifier.
///
/// The mock server is HTTP-only; `allow_insecure_endpoint` is set to `true` to
/// permit the connection.
#[tokio::test]
async fn test_openai_provider_list_models_with_mock_server() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {"id": "gpt-4.1-mini"},
                {"id": "gpt-4.1"}
            ]
        })))
        .mount(&server)
        .await;

    // SAFETY: "XZARDGZ_TEST_LIST_MODELS_KEY" is a unique env var name used
    // only in this async test; no other test in the suite reads or writes it,
    // preventing concurrent-access races.
    unsafe {
        std::env::set_var("XZARDGZ_TEST_LIST_MODELS_KEY", "test-list-key");
    }

    let config = OpenAiConfig {
        endpoint: format!("{}/v1", server.uri()),
        allow_insecure_endpoint: true,
        api_key_env: "XZARDGZ_TEST_LIST_MODELS_KEY".to_string(),
        model: "gpt-4.1-mini".to_string(),
    };

    // SAFETY: config is structurally valid; allow_insecure_endpoint explicitly
    // permits the HTTP-only wiremock server address.
    let provider = OpenAiProvider::new(config, ProviderDefaultsConfig::default())
        .expect("provider construction must succeed with a valid config");

    let result = provider.list_models().await;

    // Remove the env var before assertions so cleanup occurs even on panic.
    // SAFETY: mirrors the set_var above; no concurrent access to this variable.
    unsafe {
        std::env::remove_var("XZARDGZ_TEST_LIST_MODELS_KEY");
    }

    assert!(
        result.is_ok(),
        "list_models must succeed against the mock server, got: {:?}",
        result.err()
    );

    let models = result.unwrap();
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();

    assert!(
        ids.contains(&"gpt-4.1-mini"),
        "model list must contain gpt-4.1-mini; got: {:?}",
        ids
    );
}

/// Tests that [`OpenAiProvider::complete`] returns a [`PipelineError::Auth`]
/// error when the upstream chat completions endpoint responds with HTTP 401
/// Unauthorized, exercising the authentication-failure detection path.
///
/// The API key environment variable is set to a non-empty dummy value so the
/// provider attempts the request; the wiremock server then rejects it with 401.
#[tokio::test]
async fn test_openai_provider_complete_returns_auth_error_on_401() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&server)
        .await;

    // SAFETY: "XZARDGZ_TEST_COMPLETE_401_KEY" is a unique env var name used
    // only in this async test; no other test in the suite reads or writes it.
    unsafe {
        std::env::set_var("XZARDGZ_TEST_COMPLETE_401_KEY", "test-invalid-key");
    }

    let config = OpenAiConfig {
        endpoint: format!("{}/v1", server.uri()),
        allow_insecure_endpoint: true,
        api_key_env: "XZARDGZ_TEST_COMPLETE_401_KEY".to_string(),
        model: "gpt-4.1-mini".to_string(),
    };

    // SAFETY: config is structurally valid; allow_insecure_endpoint explicitly
    // permits the HTTP-only wiremock server address.
    let provider = OpenAiProvider::new(config, ProviderDefaultsConfig::default())
        .expect("provider construction must succeed with a valid config");

    let result = provider.complete(&[Message::user("test")], &[]).await;

    // Remove the env var before assertions so cleanup occurs even on panic.
    // SAFETY: mirrors the set_var above; no concurrent access to this variable.
    unsafe {
        std::env::remove_var("XZARDGZ_TEST_COMPLETE_401_KEY");
    }

    assert!(
        result.is_err(),
        "complete must return Err when the server responds with 401 Unauthorized"
    );

    // SAFETY: asserted is_err() on the line above.
    let err = result.unwrap_err();

    assert!(
        matches!(err, PipelineError::Auth(_)),
        "error must be PipelineError::Auth on a 401 response, got: {:?}",
        err
    );

    let err_str = err.to_string();
    assert!(
        err_str.contains("401") || err_str.contains("Unauthorized") || err_str.contains("auth"),
        "error message must reference the authentication failure, got: {err_str}"
    );
}
