use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Missing required configuration: {0}")]
    MissingConfig(String),

    #[error("Invalid security protocol: {0}")]
    InvalidSecurityProtocol(String),

    #[error("Invalid SASL mechanism: {0}")]
    InvalidSaslMechanism(String),
}

/// Security protocol for Kafka connection
#[derive(Debug, Clone, Default)]
pub enum SecurityProtocol {
    #[default]
    Plaintext,
    Ssl,
    SaslPlaintext,
    SaslSsl,
}

impl SecurityProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plaintext => "PLAINTEXT",
            Self::Ssl => "SSL",
            Self::SaslPlaintext => "SASL_PLAINTEXT",
            Self::SaslSsl => "SASL_SSL",
        }
    }
}

/// SASL authentication mechanism
#[derive(Debug, Clone, Default)]
pub enum SaslMechanism {
    Plain,
    #[default]
    ScramSha256,
    ScramSha512,
}

impl SaslMechanism {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plain => "PLAIN",
            Self::ScramSha256 => "SCRAM-SHA-256",
            Self::ScramSha512 => "SCRAM-SHA-512",
        }
    }
}

/// SASL authentication configuration
#[derive(Debug, Clone)]
pub struct SaslConfig {
    pub mechanism: SaslMechanism,
    pub username: String,
    pub password: String,
}

/// SSL/TLS configuration
#[derive(Debug, Clone)]
pub struct SslConfig {
    pub ca_location: Option<String>,
    pub certificate_location: Option<String>,
    pub key_location: Option<String>,
}

/// Kafka consumer configuration
#[derive(Debug, Clone)]
pub struct KafkaConsumerConfig {
    /// Kafka broker addresses (comma-separated)
    pub brokers: String,

    /// Topic to consume from
    pub topic: String,

    /// Consumer group ID (defaults to xzepr-consumer-{service_name})
    pub group_id: String,

    /// Service name for identification
    pub service_name: String,

    /// Security protocol
    pub security_protocol: SecurityProtocol,

    /// SASL configuration (required for SASL protocols)
    pub sasl_config: Option<SaslConfig>,

    /// SSL configuration (required for SSL protocols)
    pub ssl_config: Option<SslConfig>,

    /// Auto offset reset policy
    pub auto_offset_reset: String,

    /// Enable auto commit
    pub enable_auto_commit: bool,

    /// Session timeout
    pub session_timeout: Duration,
}

impl KafkaConsumerConfig {
    /// Create new configuration with defaults
    pub fn new(brokers: &str, topic: &str, service_name: &str) -> Self {
        let group_id = format!("xzepr-consumer-{}", service_name);
        Self {
            brokers: brokers.to_string(),
            topic: topic.to_string(),
            group_id,
            service_name: service_name.to_string(),
            security_protocol: SecurityProtocol::default(),
            sasl_config: None,
            ssl_config: None,
            auto_offset_reset: "earliest".to_string(),
            enable_auto_commit: true,
            session_timeout: Duration::from_secs(30),
        }
    }

    /// Set custom consumer group ID
    pub fn with_group_id(mut self, group_id: &str) -> Self {
        self.group_id = group_id.to_string();
        self
    }

    /// Configure SASL/SCRAM-SHA-256 authentication
    pub fn with_sasl_scram_sha256(mut self, username: &str, password: &str) -> Self {
        self.security_protocol = SecurityProtocol::SaslSsl;
        self.sasl_config = Some(SaslConfig {
            mechanism: SaslMechanism::ScramSha256,
            username: username.to_string(),
            password: password.to_string(),
        });
        self
    }

    /// Configure SSL/TLS
    pub fn with_ssl(mut self, ca_location: &str) -> Self {
        self.ssl_config = Some(SslConfig {
            ca_location: Some(ca_location.to_string()),
            certificate_location: None,
            key_location: None,
        });
        self
    }

    /// Load configuration from environment variables
    pub fn from_env(service_name: &str) -> Result<Self, ConfigError> {
        let brokers =
            std::env::var("XZEPR_KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string());

        let topic =
            std::env::var("XZEPR_KAFKA_TOPIC").unwrap_or_else(|_| "xzepr.dev.events".to_string());

        let group_id = std::env::var("XZEPR_KAFKA_GROUP_ID")
            .unwrap_or_else(|_| format!("xzepr-consumer-{}", service_name));

        let mut config = Self::new(&brokers, &topic, service_name).with_group_id(&group_id);

        // Load security protocol
        let protocol = std::env::var("XZEPR_KAFKA_SECURITY_PROTOCOL")
            .unwrap_or_else(|_| "PLAINTEXT".to_string());

        config.security_protocol = match protocol.to_uppercase().as_str() {
            "PLAINTEXT" => SecurityProtocol::Plaintext,
            "SSL" => SecurityProtocol::Ssl,
            "SASL_PLAINTEXT" => SecurityProtocol::SaslPlaintext,
            "SASL_SSL" => SecurityProtocol::SaslSsl,
            _ => return Err(ConfigError::InvalidSecurityProtocol(protocol)),
        };

        // Load SASL config if needed
        if matches!(
            config.security_protocol,
            SecurityProtocol::SaslPlaintext | SecurityProtocol::SaslSsl
        ) {
            let username = std::env::var("XZEPR_KAFKA_SASL_USERNAME")
                .map_err(|_| ConfigError::MissingConfig("XZEPR_KAFKA_SASL_USERNAME".to_string()))?;
            let password = std::env::var("XZEPR_KAFKA_SASL_PASSWORD")
                .map_err(|_| ConfigError::MissingConfig("XZEPR_KAFKA_SASL_PASSWORD".to_string()))?;

            let mechanism = std::env::var("XZEPR_KAFKA_SASL_MECHANISM")
                .unwrap_or_else(|_| "SCRAM-SHA-256".to_string());

            let sasl_mechanism = match mechanism.to_uppercase().as_str() {
                "PLAIN" => SaslMechanism::Plain,
                "SCRAM-SHA-256" => SaslMechanism::ScramSha256,
                "SCRAM-SHA-512" => SaslMechanism::ScramSha512,
                _ => return Err(ConfigError::InvalidSaslMechanism(mechanism)),
            };

            config.sasl_config = Some(SaslConfig {
                mechanism: sasl_mechanism,
                username,
                password,
            });
        }

        // Load SSL config if needed
        if matches!(
            config.security_protocol,
            SecurityProtocol::Ssl | SecurityProtocol::SaslSsl
        ) {
            let ca_location = std::env::var("XZEPR_KAFKA_SSL_CA_LOCATION").ok();
            if ca_location.is_some() {
                config.ssl_config = Some(SslConfig {
                    ca_location,
                    certificate_location: std::env::var("XZEPR_KAFKA_SSL_CERT_LOCATION").ok(),
                    key_location: std::env::var("XZEPR_KAFKA_SSL_KEY_LOCATION").ok(),
                });
            }
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_env_defaults() {
        temp_env::with_vars(
            [
                ("XZEPR_KAFKA_BROKERS", None::<&str>),
                ("XZEPR_KAFKA_TOPIC", None::<&str>),
                ("XZEPR_KAFKA_GROUP_ID", None::<&str>),
                ("XZEPR_KAFKA_SECURITY_PROTOCOL", None::<&str>),
                ("XZEPR_KAFKA_SASL_USERNAME", None::<&str>),
                ("XZEPR_KAFKA_SASL_PASSWORD", None::<&str>),
            ],
            || {
                let config = KafkaConsumerConfig::from_env("test-service").unwrap();
                assert_eq!(config.brokers, "localhost:9092");
                assert_eq!(config.topic, "xzepr.dev.events");
                assert_eq!(config.group_id, "xzepr-consumer-test-service");
                assert!(matches!(
                    config.security_protocol,
                    SecurityProtocol::Plaintext
                ));
                assert!(config.sasl_config.is_none());
            },
        );
    }

    #[test]
    fn test_config_from_env_full() {
        temp_env::with_vars(
            [
                ("XZEPR_KAFKA_BROKERS", Some("kafka:9092")),
                ("XZEPR_KAFKA_TOPIC", Some("my.topic")),
                ("XZEPR_KAFKA_GROUP_ID", Some("my-group")),
                ("XZEPR_KAFKA_SECURITY_PROTOCOL", Some("SASL_SSL")),
                ("XZEPR_KAFKA_SASL_USERNAME", Some("user")),
                ("XZEPR_KAFKA_SASL_PASSWORD", Some("pass")),
                ("XZEPR_KAFKA_SASL_MECHANISM", Some("SCRAM-SHA-512")),
            ],
            || {
                let config = KafkaConsumerConfig::from_env("test-service").unwrap();
                assert_eq!(config.brokers, "kafka:9092");
                assert_eq!(config.topic, "my.topic");
                assert_eq!(config.group_id, "my-group");
                assert!(matches!(
                    config.security_protocol,
                    SecurityProtocol::SaslSsl
                ));

                let sasl = config.sasl_config.unwrap();
                assert_eq!(sasl.username, "user");
                assert_eq!(sasl.password, "pass");
                assert!(matches!(sasl.mechanism, SaslMechanism::ScramSha512));
            },
        );
    }

    #[test]
    fn test_config_missing_sasl_creds() {
        temp_env::with_vars(
            [
                ("XZEPR_KAFKA_SECURITY_PROTOCOL", Some("SASL_PLAINTEXT")),
                ("XZEPR_KAFKA_SASL_USERNAME", None::<&str>),
            ],
            || {
                let result = KafkaConsumerConfig::from_env("test-service");
                assert!(result.is_err());
                assert!(matches!(result.unwrap_err(), ConfigError::MissingConfig(_)));
            },
        );
    }
}
