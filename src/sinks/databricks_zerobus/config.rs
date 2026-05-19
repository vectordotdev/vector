//! Configuration for the Zerobus sink.

use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;

use crate::config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext};
use crate::sinks::{
    prelude::*,
    util::{BatchConfig, RealtimeSizeBasedDefaultBatchSettings},
};

use vector_lib::codecs::encoding::{
    BatchEncoder, BatchSerializerConfig, ProtoBatchSerializerConfig,
};

use super::{
    error::ZerobusSinkError,
    service::{StreamMode, ZerobusService},
    sink::ZerobusSink,
};

/// Authentication configuration for Databricks.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "strategy", rename_all = "snake_case")]
#[configurable(metadata(
    docs::enum_tag_description = "The authentication strategy to use for Databricks."
))]
pub enum DatabricksAuthentication {
    /// Authenticate using OAuth 2.0 client credentials.
    #[serde(rename = "oauth")]
    OAuth {
        /// OAuth 2.0 client ID.
        #[configurable(metadata(docs::examples = "${DATABRICKS_CLIENT_ID}"))]
        #[configurable(metadata(docs::examples = "abc123..."))]
        client_id: SensitiveString,

        /// OAuth 2.0 client secret.
        #[configurable(metadata(docs::examples = "${DATABRICKS_CLIENT_SECRET}"))]
        #[configurable(metadata(docs::examples = "secret123..."))]
        client_secret: SensitiveString,
    },
}

impl DatabricksAuthentication {
    /// Extract the client ID and client secret as string references.
    pub fn credentials(&self) -> (&str, &str) {
        match self {
            DatabricksAuthentication::OAuth {
                client_id,
                client_secret,
            } => (client_id.inner(), client_secret.inner()),
        }
    }
}

/// Zerobus stream configuration options.
///
/// This is a thin wrapper around the SDK's `StreamConfigurationOptions` with Vector-specific
/// configuration attributes and custom defaults suitable for Vector's use case.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ZerobusStreamOptions {
    /// Timeout in milliseconds for flush operations.
    #[serde(default = "default_flush_timeout_ms")]
    #[configurable(metadata(docs::examples = 30000))]
    pub flush_timeout_ms: u64,

    /// Timeout in milliseconds for server acknowledgements.
    #[serde(default = "default_server_ack_timeout_ms")]
    #[configurable(metadata(docs::examples = 60000))]
    pub server_lack_of_ack_timeout_ms: u64,
}

impl Default for ZerobusStreamOptions {
    fn default() -> Self {
        Self {
            flush_timeout_ms: default_flush_timeout_ms(),
            server_lack_of_ack_timeout_ms: default_server_ack_timeout_ms(),
        }
    }
}

/// Configuration for the Databricks Zerobus sink.
#[configurable_component(sink(
    "databricks_zerobus",
    "Stream observability data to Databricks Unity Catalog via Zerobus."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ZerobusSinkConfig {
    /// The Zerobus ingestion endpoint URL.
    ///
    /// This should be the full URL to the Zerobus ingestion service.
    #[configurable(metadata(docs::examples = "https://ingest.dev.databricks.com"))]
    #[configurable(metadata(docs::examples = "https://ingest.prod.databricks.com"))]
    pub ingestion_endpoint: String,

    /// The Unity Catalog table name to write to.
    ///
    /// This should be in the format `catalog.schema.table`.
    #[configurable(metadata(docs::examples = "logging_platform.my_team.logs"))]
    #[configurable(metadata(docs::examples = "main.default.vector_logs"))]
    pub table_name: String,

    /// The Unity Catalog endpoint URL.
    ///
    /// This is used for authentication and table metadata.
    #[configurable(metadata(
        docs::examples = "https://dbc-e2f0eb31-2b0e.staging.cloud.databricks.com"
    ))]
    #[configurable(metadata(docs::examples = "https://your-workspace.cloud.databricks.com"))]
    pub unity_catalog_endpoint: String,

    /// Databricks authentication configuration.
    #[configurable(derived)]
    pub auth: DatabricksAuthentication,

    #[configurable(derived)]
    #[serde(default)]
    pub stream_options: ZerobusStreamOptions,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for ZerobusSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            ingestion_endpoint: "https://ingest.dev.databricks.com".to_string(),
            table_name: "catalog.schema.table".to_string(),
            unity_catalog_endpoint: "https://your-workspace.cloud.databricks.com".to_string(),
            auth: DatabricksAuthentication::OAuth {
                client_id: SensitiveString::from("${DATABRICKS_CLIENT_ID}".to_string()),
                client_secret: SensitiveString::from("${DATABRICKS_CLIENT_SECRET}".to_string()),
            },
            stream_options: ZerobusStreamOptions::default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            acknowledgements: AcknowledgementsConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "databricks_zerobus")]
impl SinkConfig for ZerobusSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        self.validate()?;

        let descriptor = ZerobusService::resolve_descriptor(self, cx.proxy()).await?;

        // The zerobus sink always encodes in proto_batch form — the stream
        // descriptor is the one we just resolved from Unity Catalog.
        let descriptor_proto = std::sync::Arc::new(descriptor.descriptor_proto().clone());
        let stream_mode = StreamMode::Proto { descriptor_proto };

        let proto_config = ProtoBatchSerializerConfig {
            descriptor: Some(descriptor),
        };
        let batch_serializer = BatchSerializerConfig::ProtoBatch(proto_config)
            .build_batch_serializer()
            .map_err(|e| format!("Failed to build batch serializer: {}", e))?;
        let encoder = BatchEncoder::new(batch_serializer);

        let service = ZerobusService::new(self.clone(), stream_mode, cx.proxy()).await?;
        let healthcheck_service = service.clone();

        let request_limits = self.request.into_settings();

        let sink = ZerobusSink::new(service, request_limits, self.batch, encoder)?;

        let healthcheck = async move {
            healthcheck_service
                .ensure_stream()
                .await
                .map_err(|e| e.into())
        };

        Ok((
            VectorSink::from_event_streamsink(sink),
            Box::pin(healthcheck),
        ))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl ZerobusSinkConfig {
    pub fn validate(&self) -> Result<(), ZerobusSinkError> {
        if self.ingestion_endpoint.is_empty() {
            return Err(ZerobusSinkError::ConfigError {
                message: "ingestion_endpoint cannot be empty".to_string(),
            });
        }

        if self.table_name.is_empty() {
            return Err(ZerobusSinkError::ConfigError {
                message: "table_name cannot be empty".to_string(),
            });
        }

        let parts: Vec<&str> = self.table_name.split('.').collect();
        if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
            return Err(ZerobusSinkError::ConfigError {
                message: "table_name must be in format 'catalog.schema.table' (exactly 3 non-empty parts)"
                    .to_string(),
            });
        }

        if self.unity_catalog_endpoint.is_empty() {
            return Err(ZerobusSinkError::ConfigError {
                message: "unity_catalog_endpoint cannot be empty".to_string(),
            });
        }

        // Validate authentication credentials
        match &self.auth {
            DatabricksAuthentication::OAuth {
                client_id,
                client_secret,
            } => {
                if client_id.inner().is_empty() {
                    return Err(ZerobusSinkError::ConfigError {
                        message: "OAuth client_id cannot be empty".to_string(),
                    });
                }
                if client_secret.inner().is_empty() {
                    return Err(ZerobusSinkError::ConfigError {
                        message: "OAuth client_secret cannot be empty".to_string(),
                    });
                }
            }
        }

        if let Some(max_bytes) = self.batch.max_bytes {
            // Zerobus SDK limits max bytes to 10MB. This cap is a conservative safety limit:
            // it's measured against Vector's pre-serialization sizing, not the protobuf bytes
            // the SDK actually sends. Vector's pre-serialization size is generally larger than
            // the SDK's protobuf-encoded size, so enforcing the 10MB cap here ensures the SDK's
            // 10MB limit cannot be exceeded at runtime.
            if max_bytes > 10_000_000 {
                return Err(ZerobusSinkError::ConfigError {
                    message: "max_bytes must be less than or equal to 10MB".to_string(),
                });
            }
        }

        Ok(())
    }
}

// Default value functions
const fn default_flush_timeout_ms() -> u64 {
    30000
}

const fn default_server_ack_timeout_ms() -> u64 {
    60000
}

#[cfg(test)]
mod tests {
    use super::*;
    use vector_lib::sensitive_string::SensitiveString;

    fn create_test_config() -> ZerobusSinkConfig {
        ZerobusSinkConfig {
            ingestion_endpoint: "https://test.databricks.com".to_string(),
            table_name: "test.default.logs".to_string(),
            unity_catalog_endpoint: "https://test-workspace.databricks.com".to_string(),
            auth: DatabricksAuthentication::OAuth {
                client_id: SensitiveString::from("test-client-id".to_string()),
                client_secret: SensitiveString::from("test-client-secret".to_string()),
            },
            stream_options: ZerobusStreamOptions::default(),
            batch: Default::default(),
            request: Default::default(),
            acknowledgements: Default::default(),
        }
    }

    #[test]
    fn test_config_validation_success() {
        let config = create_test_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_empty_endpoint() {
        let mut config = create_test_config();
        config.ingestion_endpoint = "".to_string();

        let result = config.validate();
        assert!(result.is_err());

        if let Err(crate::sinks::databricks_zerobus::error::ZerobusSinkError::ConfigError {
            message,
        }) = result
        {
            assert!(message.contains("ingestion_endpoint cannot be empty"));
        } else {
            panic!("Expected ConfigError for empty ingestion_endpoint");
        }
    }

    #[test]
    fn test_config_validation_empty_table_name() {
        let mut config = create_test_config();
        config.table_name = "".to_string();

        let result = config.validate();
        assert!(result.is_err());

        if let Err(crate::sinks::databricks_zerobus::error::ZerobusSinkError::ConfigError {
            message,
        }) = result
        {
            assert!(message.contains("table_name cannot be empty"));
        } else {
            panic!("Expected ConfigError for empty table_name");
        }
    }

    #[test]
    fn test_config_validation_invalid_table_name() {
        let mut config = create_test_config();
        config.table_name = "invalid_table".to_string(); // Missing dots

        let result = config.validate();
        assert!(result.is_err());

        if let Err(crate::sinks::databricks_zerobus::error::ZerobusSinkError::ConfigError {
            message,
        }) = result
        {
            assert!(message.contains("catalog.schema.table"));
        } else {
            panic!("Expected ConfigError for invalid table_name format");
        }
    }

    #[test]
    fn test_config_validation_table_name_empty_segments() {
        for bad in [
            "catalog..table",
            ".schema.table",
            "catalog.schema.",
            "..",
            "catalog.schema.table.extra",
        ] {
            let mut config = create_test_config();
            config.table_name = bad.to_string();
            let result = config.validate();
            assert!(result.is_err(), "expected error for table_name={bad:?}");
            if let Err(crate::sinks::databricks_zerobus::error::ZerobusSinkError::ConfigError {
                message,
            }) = result
            {
                assert!(message.contains("catalog.schema.table"));
            } else {
                panic!("Expected ConfigError for table_name={bad:?}");
            }
        }
    }

    #[test]
    fn test_config_validation_empty_unity_catalog_endpoint() {
        let mut config = create_test_config();
        config.unity_catalog_endpoint = "".to_string();

        let result = config.validate();
        assert!(result.is_err());

        if let Err(crate::sinks::databricks_zerobus::error::ZerobusSinkError::ConfigError {
            message,
        }) = result
        {
            assert!(message.contains("unity_catalog_endpoint cannot be empty"));
        } else {
            panic!("Expected ConfigError for empty unity_catalog_endpoint");
        }
    }

    #[test]
    fn test_config_validation_empty_oauth_credentials() {
        let mut config = create_test_config();
        config.auth = DatabricksAuthentication::OAuth {
            client_id: SensitiveString::from("".to_string()),
            client_secret: SensitiveString::from("test-secret".to_string()),
        };

        let result = config.validate();
        assert!(result.is_err());

        if let Err(crate::sinks::databricks_zerobus::error::ZerobusSinkError::ConfigError {
            message,
        }) = result
        {
            assert!(message.contains("OAuth client_id cannot be empty"));
        } else {
            panic!("Expected ConfigError for empty OAuth client_id");
        }
    }

    /// When `batch.max_bytes` is `None` (user omitted the field or set it to `null`),
    /// `into_batcher_settings()` must merge it against
    /// `RealtimeSizeBasedDefaultBatchSettings::MAX_BYTES` (10MB) — never unbounded.
    /// This guarantees the Zerobus SDK's 10MB limit cannot be exceeded at runtime
    /// even without an explicit user cap.
    #[test]
    fn test_batch_max_bytes_none_defaults_to_10mb() {
        let mut config = create_test_config();
        config.batch.max_bytes = None;

        let settings = config
            .batch
            .into_batcher_settings()
            .expect("batch settings should build");

        assert_eq!(settings.size_limit, 10_000_000);
    }
}
