use crate::error::ProviderError;
use keyring::Entry;
use oauth2::basic::BasicClient;
use oauth2::{AuthUrl, ClientId, DeviceAuthorizationUrl, Scope, TokenResponse, TokenUrl};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use tokio::time::sleep;

const CLIENT_ID: &str = "Iv1.b507a08c87ecfe98"; // GitHub CLI Client ID
const AUTH_URL: &str = "https://github.com/login/oauth/authorize";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const DEVICE_AUTH_URL: &str = "https://github.com/login/device/code";
const KEYRING_SERVICE: &str = "xzardgz-copilot";
const KEYRING_USER: &str = "oauth-token";

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct TokenCache {
    access_token: String,
    expires_at: Option<SystemTime>,
    refresh_token: Option<String>,
}

pub struct CopilotAuth {
    client: BasicClient,
}

impl CopilotAuth {
    pub fn new() -> Result<Self, ProviderError> {
        let auth_url =
            AuthUrl::new(AUTH_URL.to_string()).map_err(|e| ProviderError::Auth(e.to_string()))?;
        let token_url =
            TokenUrl::new(TOKEN_URL.to_string()).map_err(|e| ProviderError::Auth(e.to_string()))?;
        let device_auth_url = DeviceAuthorizationUrl::new(DEVICE_AUTH_URL.to_string())
            .map_err(|e| ProviderError::Auth(e.to_string()))?;

        let client = BasicClient::new(
            ClientId::new(CLIENT_ID.to_string()),
            None, // No client secret for public clients
            auth_url,
            Some(token_url),
        )
        .set_device_authorization_url(device_auth_url);

        Ok(Self { client })
    }

    pub async fn get_token(&self) -> Result<String, ProviderError> {
        // 1. Try to load from keyring
        if let Ok(token) = self.load_token() {
            return Ok(token);
        }

        // 2. Start device flow
        let details: oauth2::StandardDeviceAuthorizationResponse = self
            .client
            .exchange_device_code()
            .map_err(|e| ProviderError::Auth(e.to_string()))?
            .add_scope(Scope::new("read:user".to_string()))
            .add_scope(Scope::new("copilot".to_string()))
            .request_async(oauth2::reqwest::async_http_client)
            .await
            .map_err(|e| ProviderError::Auth(format!("Device flow init failed: {}", e)))?;

        println!(
            "Open this URL to authenticate: {}",
            details.verification_uri().as_str()
        );
        println!("Enter code: {}", details.user_code().secret());

        // 3. Poll for token
        let token_result = self
            .client
            .exchange_device_access_token(&details)
            .request_async(oauth2::reqwest::async_http_client, sleep, None)
            .await
            .map_err(|e| ProviderError::Auth(format!("Token polling failed: {}", e)))?;

        let access_token = token_result.access_token().secret().to_string();

        // 4. Cache token
        self.save_token(&access_token)?;

        Ok(access_token)
    }

    fn load_token(&self) -> Result<String, ProviderError> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| ProviderError::Auth(e.to_string()))?;

        let token = entry
            .get_password()
            .map_err(|_| ProviderError::Auth("No cached token".to_string()))?;

        Ok(token)
    }

    fn save_token(&self, token: &str) -> Result<(), ProviderError> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| ProviderError::Auth(e.to_string()))?;

        entry
            .set_password(token)
            .map_err(|e| ProviderError::Auth(e.to_string()))?;

        Ok(())
    }
}
