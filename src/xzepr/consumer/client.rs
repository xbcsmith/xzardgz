use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;
use tracing::{debug, error, info};

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Request to create an event receiver
#[derive(Debug, Serialize)]
pub struct CreateEventReceiverRequest {
    pub name: String,
    #[serde(rename = "type")]
    pub receiver_type: String,
    pub version: String,
    pub description: String,
    pub schema: JsonValue,
}

/// Response from creating an event receiver
#[derive(Debug, Deserialize)]
pub struct CreateEventReceiverResponse {
    pub data: String,
}

/// Event receiver entity from list response
#[derive(Debug, Deserialize)]
pub struct EventReceiverResponse {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub receiver_type: String,
    pub version: String,
    pub description: String,
    pub schema: JsonValue,
    pub fingerprint: String,
    pub created_at: String,
}

/// Paginated response wrapper
#[derive(Debug, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationMeta,
}

/// Pagination metadata
#[derive(Debug, Deserialize)]
pub struct PaginationMeta {
    pub limit: usize,
    pub offset: usize,
    pub total: usize,
    pub has_more: bool,
}

/// Request to create an event
#[derive(Debug, Serialize)]
pub struct CreateEventRequest {
    pub name: String,
    pub version: String,
    pub release: String,
    pub platform_id: String,
    pub package: String,
    pub description: String,
    pub payload: JsonValue,
    pub success: bool,
    pub event_receiver_id: String,
}

/// Response from creating an event
#[derive(Debug, Deserialize)]
pub struct CreateEventResponse {
    pub data: String,
}

/// XZepr API client configuration
#[derive(Debug, Clone)]
pub struct XzeprClientConfig {
    /// Base URL of the XZepr API
    pub base_url: String,

    /// JWT token for authentication
    pub token: String,

    /// Request timeout in seconds
    pub timeout_secs: u64,
}

impl XzeprClientConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> Result<Self, ClientError> {
        let base_url =
            std::env::var("XZEPR_API_URL").unwrap_or_else(|_| "http://localhost:8042".to_string());

        let token = std::env::var("XZEPR_API_TOKEN")
            .map_err(|_| ClientError::Authentication("XZEPR_API_TOKEN not set".to_string()))?;

        Ok(Self {
            base_url,
            token,
            timeout_secs: 30,
        })
    }
}

/// XZepr API client for downstream services
pub struct XzeprClient {
    client: Client,
    config: XzeprClientConfig,
}

impl XzeprClient {
    /// Create a new XZepr client
    pub fn new(config: XzeprClientConfig) -> Result<Self, ClientError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()?;

        Ok(Self { client, config })
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self, ClientError> {
        let config = XzeprClientConfig::from_env()?;
        Self::new(config)
    }

    /// Build a request with authentication
    fn build_request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.config.base_url, path);
        self.client
            .request(method, &url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Content-Type", "application/json")
    }

    /// Create a new event receiver
    pub async fn create_event_receiver(
        &self,
        request: CreateEventReceiverRequest,
    ) -> Result<String, ClientError> {
        let response = self
            .build_request(reqwest::Method::POST, "/api/v1/receivers")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            let result: CreateEventReceiverResponse = response.json().await?;
            info!(
                receiver_id = %result.data,
                name = %request.name,
                "Created event receiver"
            );
            Ok(result.data)
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(ClientError::Api {
                status: status.as_u16(),
                message: body,
            })
        }
    }

    /// List event receivers with optional filters
    pub async fn list_event_receivers(
        &self,
        name_filter: Option<&str>,
        receiver_type: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<PaginatedResponse<EventReceiverResponse>, ClientError> {
        let mut query = vec![("limit", limit.to_string()), ("offset", offset.to_string())];

        if let Some(name) = name_filter {
            query.push(("name", name.to_string()));
        }
        if let Some(rtype) = receiver_type {
            query.push(("type", rtype.to_string()));
        }

        let response = self
            .build_request(reqwest::Method::GET, "/api/v1/receivers")
            .query(&query)
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            let result: PaginatedResponse<EventReceiverResponse> = response.json().await?;
            Ok(result)
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(ClientError::Api {
                status: status.as_u16(),
                message: body,
            })
        }
    }

    /// Get an event receiver by ID
    pub async fn get_event_receiver(&self, id: &str) -> Result<EventReceiverResponse, ClientError> {
        let response = self
            .build_request(reqwest::Method::GET, &format!("/api/v1/receivers/{}", id))
            .send()
            .await?;

        let status = response.status();
        match status {
            StatusCode::OK => {
                let result: EventReceiverResponse = response.json().await?;
                Ok(result)
            }
            StatusCode::NOT_FOUND => Err(ClientError::NotFound(format!("Event receiver {}", id))),
            _ => {
                let body = response.text().await.unwrap_or_default();
                Err(ClientError::Api {
                    status: status.as_u16(),
                    message: body,
                })
            }
        }
    }

    /// Discover an existing event receiver by name, or create if not found
    pub async fn discover_or_create_event_receiver(
        &self,
        name: &str,
        receiver_type: &str,
        version: &str,
        description: &str,
        schema: JsonValue,
    ) -> Result<String, ClientError> {
        // First, try to find existing receiver by name
        let receivers = self
            .list_event_receivers(Some(name), Some(receiver_type), 10, 0)
            .await?;

        // Check for exact match
        for receiver in &receivers.data {
            if receiver.name == name && receiver.receiver_type == receiver_type {
                info!(
                    receiver_id = %receiver.id,
                    name = %name,
                    "Discovered existing event receiver"
                );
                return Ok(receiver.id.clone());
            }
        }

        // Not found, create new receiver
        info!(name = %name, "Event receiver not found, creating new one");
        let request = CreateEventReceiverRequest {
            name: name.to_string(),
            receiver_type: receiver_type.to_string(),
            version: version.to_string(),
            description: description.to_string(),
            schema,
        };

        self.create_event_receiver(request).await
    }

    /// Create a new event
    pub async fn create_event(&self, request: CreateEventRequest) -> Result<String, ClientError> {
        let response = self
            .build_request(reqwest::Method::POST, "/api/v1/events")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            let result: CreateEventResponse = response.json().await?;
            debug!(
                event_id = %result.data,
                event_name = %request.name,
                "Created event"
            );
            Ok(result.data)
        } else {
            let body = response.text().await.unwrap_or_default();
            error!(
                status = status.as_u16(),
                body = %body,
                "Failed to create event"
            );
            Err(ClientError::Api {
                status: status.as_u16(),
                message: body,
            })
        }
    }

    /// Post a work started event
    #[allow(clippy::too_many_arguments)]
    pub async fn post_work_started(
        &self,
        receiver_id: &str,
        work_id: &str,
        work_name: &str,
        version: &str,
        platform_id: &str,
        package: &str,
        payload: JsonValue,
    ) -> Result<String, ClientError> {
        let request = CreateEventRequest {
            name: format!("{}.started", work_name),
            version: version.to_string(),
            release: version.to_string(),
            platform_id: platform_id.to_string(),
            package: package.to_string(),
            description: format!("Work started: {} ({})", work_name, work_id),
            payload: serde_json::json!({
                "work_id": work_id,
                "status": "started",
                "started_at": chrono::Utc::now().to_rfc3339(),
                "details": payload
            }),
            success: true,
            event_receiver_id: receiver_id.to_string(),
        };

        self.create_event(request).await
    }

    /// Post a work completed event
    #[allow(clippy::too_many_arguments)]
    pub async fn post_work_completed(
        &self,
        receiver_id: &str,
        work_id: &str,
        work_name: &str,
        version: &str,
        platform_id: &str,
        package: &str,
        success: bool,
        payload: JsonValue,
    ) -> Result<String, ClientError> {
        let status_suffix = if success { "completed" } else { "failed" };
        let request = CreateEventRequest {
            name: format!("{}.{}", work_name, status_suffix),
            version: version.to_string(),
            release: version.to_string(),
            platform_id: platform_id.to_string(),
            package: package.to_string(),
            description: format!("Work {}: {} ({})", status_suffix, work_name, work_id),
            payload: serde_json::json!({
                "work_id": work_id,
                "status": status_suffix,
                "completed_at": chrono::Utc::now().to_rfc3339(),
                "success": success,
                "details": payload
            }),
            success,
            event_receiver_id: receiver_id.to_string(),
        };

        self.create_event(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config_from_env() {
        temp_env::with_vars(
            [
                ("XZEPR_API_URL", Some("http://test.api")),
                ("XZEPR_API_TOKEN", Some("test-token")),
            ],
            || {
                let config = XzeprClientConfig::from_env().unwrap();
                assert_eq!(config.base_url, "http://test.api");
                assert_eq!(config.token, "test-token");
            },
        );
    }
}
