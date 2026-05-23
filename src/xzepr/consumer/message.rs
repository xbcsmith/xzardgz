use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// CloudEvents 1.0.1 message from XZepr
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudEventMessage {
    /// Indicates if the event represents success
    pub success: bool,

    /// Unique event identifier (ULID)
    pub id: String,

    /// CloudEvents specification version
    pub specversion: String,

    /// Event type/name
    #[serde(rename = "type")]
    pub event_type: String,

    /// Event source URI
    pub source: String,

    /// XZepr API version
    pub api_version: String,

    /// Event name
    pub name: String,

    /// Event version
    pub version: String,

    /// Release identifier
    pub release: String,

    /// Platform identifier
    pub platform_id: String,

    /// Package name
    pub package: String,

    /// Event payload data
    pub data: CloudEventData,
}

/// Data payload containing entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudEventData {
    /// Events in this message
    pub events: Vec<EventEntity>,

    /// Event receivers in this message
    pub event_receivers: Vec<EventReceiverEntity>,

    /// Event receiver groups in this message
    pub event_receiver_groups: Vec<EventReceiverGroupEntity>,
}

/// Event entity from XZepr
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEntity {
    pub id: String,
    pub name: String,
    pub version: String,
    pub release: String,
    pub platform_id: String,
    pub package: String,
    pub description: String,
    pub payload: JsonValue,
    pub success: bool,
    pub event_receiver_id: String,
    pub created_at: DateTime<Utc>,
}

/// Event receiver entity from XZepr
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventReceiverEntity {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub receiver_type: String,
    pub version: String,
    pub description: String,
    pub schema: JsonValue,
    pub fingerprint: String,
    pub created_at: DateTime<Utc>,
}

/// Event receiver group entity from XZepr
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventReceiverGroupEntity {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub group_type: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    pub event_receiver_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_cloud_event() {
        let json_data = json!({
            "success": true,
            "id": "01JXXXXXXXXXXXXXXXXXXXXXXX",
            "specversion": "1.0.1",
            "type": "deployment.success",
            "source": "xzepr.event.receiver.01JXXXXXXXXXXXXXXXXXXXXXXX",
            "api_version": "v1",
            "name": "deployment.success",
            "version": "1.0.0",
            "release": "1.0.0-rc.1",
            "platform_id": "kubernetes",
            "package": "myapp",
            "data": {
                "events": [],
                "event_receivers": [],
                "event_receiver_groups": []
            }
        });

        let event: CloudEventMessage = serde_json::from_value(json_data).unwrap();
        assert_eq!(event.id, "01JXXXXXXXXXXXXXXXXXXXXXXX");
        assert_eq!(event.event_type, "deployment.success");
        assert_eq!(event.platform_id, "kubernetes");
        assert!(event.success);
    }
}
