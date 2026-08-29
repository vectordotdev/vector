//! Configuration for the Zerobus sink.

use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;

use crate::config::{
    AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
};
use crate::sinks::{
    prelude::*,
    util::{
        BatchConfig, HttpEndpoint, RealtimeSizeBasedDefaultBatchSettings, TowerRequestSettings,
    },
};

use super::{error::ZerobusSinkError, service::ZerobusService, sink::ZerobusSink};

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

/// Arrow IPC compression codec for Zerobus Arrow Flight payloads.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    /// No compression.
    #[default]
    None,
    /// LZ4 frame compression.
    Lz4Frame,
    /// Zstandard compression.
    Zstd,
}

impl From<Compression> for Option<arrow::ipc::CompressionType> {
    fn from(value: Compression) -> Self {
        match value {
            Compression::None => None,
            Compression::Lz4Frame => Some(arrow::ipc::CompressionType::LZ4_FRAME),
            Compression::Zstd => Some(arrow::ipc::CompressionType::ZSTD),
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

    /// Arrow IPC compression for Flight payloads. Defaults to no compression.
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub compression: Compression,
}

impl Default for ZerobusStreamOptions {
    fn default() -> Self {
        Self {
            flush_timeout_ms: default_flush_timeout_ms(),
            server_lack_of_ack_timeout_ms: default_server_ack_timeout_ms(),
            compression: Compression::None,
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
    ///
    /// See the [Databricks Zerobus documentation][zerobus_endpoint] to find your workspace URL and
    /// Zerobus ingest endpoint.
    ///
    /// [zerobus_endpoint]: https://docs.databricks.com/aws/en/ingestion/zerobus-ingest#get-your-workspace-url-and-zerobus-ingest-endpoint
    #[configurable(metadata(
        docs::examples = "https://1234567890123456.zerobus.us-west-2.cloud.databricks.com"
    ))]
    #[configurable(metadata(
        docs::examples = "https://6543210987654321.zerobus.us-east-1.cloud.databricks.com"
    ))]
    pub ingestion_endpoint: HttpEndpoint,

    /// The Unity Catalog table name to write to.
    ///
    /// This should be in the format `catalog.schema.table`.
    ///
    /// See the [Databricks Zerobus documentation][zerobus_table] to create or identify the target
    /// table.
    ///
    /// [zerobus_table]: https://docs.databricks.com/aws/en/ingestion/zerobus-ingest#create-or-identify-the-target-table
    #[configurable(metadata(docs::examples = "main.default.logs"))]
    #[configurable(metadata(docs::examples = "main.default.vector_logs"))]
    pub table_name: String,

    /// The Unity Catalog endpoint URL.
    ///
    /// This is used for authentication and table metadata.
    ///
    /// See the [Databricks Zerobus documentation][zerobus_endpoint] to find your workspace URL and
    /// Zerobus ingest endpoint.
    ///
    /// [zerobus_endpoint]: https://docs.databricks.com/aws/en/ingestion/zerobus-ingest#get-your-workspace-url-and-zerobus-ingest-endpoint
    #[configurable(metadata(docs::examples = "https://dbc-a1b2c3d4-e5f6.cloud.databricks.com"))]
    #[configurable(metadata(docs::examples = "https://dbc-f6e5d4c3-b2a1.cloud.databricks.com"))]
    pub unity_catalog_endpoint: HttpEndpoint,

    /// Databricks authentication configuration.
    ///
    /// See the [Databricks Zerobus documentation][zerobus_service_principal] to create a service
    /// principal and grant it permissions to write to the target table.
    ///
    /// [zerobus_service_principal]: https://docs.databricks.com/aws/en/ingestion/zerobus-ingest#create-a-service-principal-and-grant-permissions
    pub auth: DatabricksAuthentication,

    /// Custom identifier appended to the `user-agent` header sent to Databricks.
    ///
    /// The header always includes `Vector/<version>`; when set, this value is
    /// appended after it (e.g. `my-service/1.2`).
    #[serde(default)]
    #[configurable(metadata(docs::examples = "my-service/1.2"))]
    pub user_agent: Option<String>,

    #[serde(default)]
    pub stream_options: ZerobusStreamOptions,

    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[serde(default)]
    pub request: TowerRequestConfig,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for ZerobusSinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            ingestion_endpoint: HttpEndpoint::parse(
                "https://1234567890123456.zerobus.us-west-2.cloud.databricks.com",
            )
            .expect("valid example ingestion endpoint"),
            table_name: "main.default.logs".to_string(),
            unity_catalog_endpoint: HttpEndpoint::parse(
                "https://dbc-a1b2c3d4-e5f6.cloud.databricks.com",
            )
            .expect("valid example unity catalog endpoint"),
            auth: DatabricksAuthentication::OAuth {
                client_id: SensitiveString::from("${DATABRICKS_CLIENT_ID}".to_string()),
                client_secret: SensitiveString::from("${DATABRICKS_CLIENT_SECRET}".to_string()),
            },
            user_agent: None,
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
    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedZerobus {
    batch_settings: BatcherSettings,
    request_limits: TowerRequestSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for ZerobusSinkConfig {
    type Validated = ValidatedZerobus;

    fn validate(&self) -> crate::Result<ValidatedZerobus> {
        // Pure structural validation (no network/filesystem/async).
        self.validate()?;

        let batch_settings = self.batch.into_batcher_settings()?;

        let request_limits = self.request.into_settings();

        Ok(ValidatedZerobus {
            batch_settings,
            request_limits,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedZerobus,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let service = ZerobusService::new(self.clone(), cx.proxy()).await?;
        let healthcheck_service = service.clone();

        let sink = ZerobusSink::new(
            service,
            validated.request_limits.clone(),
            validated.batch_settings,
        );

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
}

impl ZerobusSinkConfig {
    pub fn validate(&self) -> Result<(), ZerobusSinkError> {
        // `ingestion_endpoint` is an `HttpEndpoint`: deserialization already
        // guarantees it is an absolute http(s) URL with a host and valid port,
        // so no further validation is needed here.

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

        // `unity_catalog_endpoint` is an `HttpEndpoint`: deserialization already
        // guarantees it is an absolute http(s) URL with a nonempty host and a
        // valid numeric port (the healthcheck builds `{endpoint}/oidc/v1/token`
        // and `{endpoint}/api/2.1/unity-catalog/tables/{name}` from it), so no
        // further validation is needed here.

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
            // Zerobus SDK limits max bytes to 10MB. This cap is a coarse safety
            // limit: it's measured against Vector's pre-serialization (estimated
            // JSON) sizing, not the encoded Arrow bytes the SDK actually sends.
            // The two differ — for numeric-heavy schemas the encoded Arrow batch
            // can be larger than the source events — so a batch configured right
            // at the boundary may still exceed the SDK's limit; lower max_bytes to
            // leave headroom if you see SDK-side size errors.
            if max_bytes > 10_000_000 {
                return Err(ZerobusSinkError::ConfigError {
                    message: "max_bytes must be less than or equal to 10MB".to_string(),
                });
            }
        }

        Ok(())
    }

    /// The user-agent suffix to hand the Zerobus SDK: `Vector/<version>`
    /// alone, or with the user's configured `user_agent` appended. The SDK
    /// prepends its own `zerobus-sdk-rs/<version>` prefix to this value.
    pub fn user_agent_suffix(&self) -> String {
        let vector = format!("Vector/{}", crate::vector_version());
        match self.user_agent.as_deref().filter(|s| !s.is_empty()) {
            Some(ua) => format!("{vector} {ua}"),
            None => vector,
        }
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
            ingestion_endpoint: HttpEndpoint::parse("https://test.databricks.com").unwrap(),
            table_name: "test.default.logs".to_string(),
            unity_catalog_endpoint: HttpEndpoint::parse("https://test-workspace.databricks.com")
                .unwrap(),
            auth: DatabricksAuthentication::OAuth {
                client_id: SensitiveString::from("test-client-id".to_string()),
                client_secret: SensitiveString::from("test-client-secret".to_string()),
            },
            user_agent: None,
            stream_options: ZerobusStreamOptions::default(),
            batch: Default::default(),
            request: Default::default(),
            acknowledgements: Default::default(),
        }
    }

    #[test]
    fn validate_produces_request_limits_and_batch() {
        use crate::config::ValidatedSink;
        let config = create_test_config();
        // The inherent `validate` (structural checks) shadows the trait method in
        // method-call syntax, so call the trait method via UFCS.
        let validated = <ZerobusSinkConfig as ValidatedSink>::validate(&config)
            .expect("validation should succeed");
        // Default request settings resolve to an unbounded concurrency.
        assert_eq!(validated.request_limits.concurrency, None);
        assert!(validated.request_limits.timeout.as_secs() > 0);
        // Default batch settings resolve to the 10MB size limit.
        assert_eq!(validated.batch_settings.size_limit, 10_000_000);
    }

    #[test]
    fn validate_rejects_invalid_batch_settings() {
        use crate::config::ValidatedSink;
        // `batch.max_events = 0` is a pure config error that `vector validate
        // --no-environment` must catch rather than deferring to build.
        let mut config = create_test_config();
        config.batch.max_events = Some(0);
        assert!(
            <ZerobusSinkConfig as ValidatedSink>::validate(&config).is_err(),
            "max_events = 0 should fail validation"
        );

        // Same for a non-positive timeout.
        let mut config = create_test_config();
        config.batch.timeout_secs = Some(0.0);
        assert!(
            <ZerobusSinkConfig as ValidatedSink>::validate(&config).is_err(),
            "timeout_secs <= 0 should fail validation"
        );
    }

    #[test]
    fn test_config_validation_success() {
        let config = create_test_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_rejects_non_http_ingestion_endpoint() {
        // `ingestion_endpoint` is an `HttpEndpoint`, so a non-http scheme is
        // rejected at deserialization time rather than at validate time.
        let config = r#"
ingestion_endpoint: "ftp://test.databricks.com"
table_name: "test.default.logs"
unity_catalog_endpoint: "https://test-workspace.databricks.com"
auth:
  strategy: oauth
  client_id: "test-client-id"
  client_secret: "test-client-secret"
"#;
        let result: Result<ZerobusSinkConfig, _> = serde_yaml::from_str(config);
        assert!(
            result.is_err(),
            "non-http ingestion_endpoint should fail deserialization"
        );

        // The same holds for a relative endpoint with no host.
        let config = r#"
ingestion_endpoint: "/some/path"
table_name: "test.default.logs"
unity_catalog_endpoint: "https://test-workspace.databricks.com"
auth:
  strategy: oauth
  client_id: "test-client-id"
  client_secret: "test-client-secret"
"#;
        let result: Result<ZerobusSinkConfig, _> = serde_yaml::from_str(config);
        assert!(
            result.is_err(),
            "host-less ingestion_endpoint should fail deserialization"
        );
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
    fn test_config_rejects_invalid_unity_catalog_endpoint() {
        // `unity_catalog_endpoint` is an `HttpEndpoint`, so a malformed endpoint
        // is rejected at deserialization time (config load) rather than at
        // validate time. `http::Uri` alone would accept an empty host
        // (`http://:8080`) and a non-numeric port (`http://localhost:notaport`),
        // both of which `HttpEndpoint` rejects.
        for bad in [
            "",
            "not a uri",
            "//workspace.databricks.com",
            "ftp://workspace.databricks.com",
            "http://:8080",
            "http://localhost:notaport",
        ] {
            let config = format!(
                r#"
ingestion_endpoint: "https://test.databricks.com"
table_name: "test.default.logs"
unity_catalog_endpoint: {bad:?}
auth:
  strategy: oauth
  client_id: "test-client-id"
  client_secret: "test-client-secret"
"#
            );
            let result: Result<ZerobusSinkConfig, _> = serde_yaml::from_str(&config);
            assert!(
                result.is_err(),
                "expected deserialization error for endpoint={bad:?}"
            );
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

    #[test]
    fn test_stream_options_compression_deserializes() {
        let opts: ZerobusStreamOptions =
            serde_json::from_str(r#"{"compression":"zstd"}"#).expect("should parse zstd");
        assert_eq!(opts.compression, Compression::Zstd);

        let opts: ZerobusStreamOptions =
            serde_json::from_str(r#"{"compression":"lz4_frame"}"#).expect("should parse lz4_frame");
        assert_eq!(opts.compression, Compression::Lz4Frame);

        let opts: ZerobusStreamOptions =
            serde_json::from_str(r#"{"compression":"none"}"#).expect("should parse none");
        assert_eq!(opts.compression, Compression::None);

        // Omitting the field leaves compression disabled.
        let opts: ZerobusStreamOptions = serde_json::from_str("{}").expect("should parse empty");
        assert_eq!(opts.compression, Compression::None);
    }

    #[test]
    fn test_compression_maps_to_arrow_ipc() {
        assert_eq!(
            Option::<arrow::ipc::CompressionType>::from(Compression::None),
            None,
        );
        assert_eq!(
            Option::<arrow::ipc::CompressionType>::from(Compression::Lz4Frame),
            Some(arrow::ipc::CompressionType::LZ4_FRAME),
        );
        assert_eq!(
            Option::<arrow::ipc::CompressionType>::from(Compression::Zstd),
            Some(arrow::ipc::CompressionType::ZSTD),
        );
    }

    /// Guards the `arrow/ipc_compression` feature: lz4/zstd error at runtime unless
    /// arrow is built with the codecs. arrow-ipc only validates when writing a
    /// compressed buffer, so this round-trips a batch through each codec.
    #[test]
    fn test_arrow_ipc_compression_codecs_are_enabled() {
        use std::sync::Arc;

        use arrow::array::Int32Array;
        use arrow::datatypes::{DataType, Field, Schema};
        use arrow::ipc::writer::{IpcWriteOptions, StreamWriter};
        use arrow::record_batch::RecordBatch;

        let schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int32, false)]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int32Array::from((0..1024).collect::<Vec<_>>()))],
        )
        .expect("batch should build");

        for codec in [Compression::Lz4Frame, Compression::Zstd] {
            let compression: Option<arrow::ipc::CompressionType> = codec.into();
            let options = IpcWriteOptions::default()
                .try_with_compression(compression)
                .unwrap_or_else(|e| panic!("{codec:?} not enabled in arrow build: {e}"));

            let mut buf = Vec::new();
            let mut writer = StreamWriter::try_new_with_options(&mut buf, &schema, options)
                .unwrap_or_else(|e| panic!("writer for {codec:?} should build: {e}"));
            writer
                .write(&batch)
                .unwrap_or_else(|e| panic!("writing compressed batch for {codec:?} failed: {e}"));
            writer
                .finish()
                .unwrap_or_else(|e| panic!("finishing stream for {codec:?} failed: {e}"));

            assert!(!buf.is_empty(), "{codec:?} produced no output");
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

    #[test]
    fn test_user_agent_suffix_without_user_value() {
        let config = create_test_config();
        let suffix = config.user_agent_suffix();
        assert!(
            suffix.starts_with("Vector/"),
            "expected Vector/<version> prefix, got {suffix:?}"
        );
        // No user value configured, so nothing is appended.
        assert!(
            !suffix.contains(' '),
            "unexpected appended value in {suffix:?}"
        );
    }

    #[test]
    fn test_user_agent_suffix_with_user_value() {
        let mut config = create_test_config();
        config.user_agent = Some("my-service/1.2".to_string());
        let suffix = config.user_agent_suffix();
        assert!(
            suffix.starts_with("Vector/"),
            "expected Vector/<version> prefix, got {suffix:?}"
        );
        assert!(
            suffix.ends_with(" my-service/1.2"),
            "expected user value appended, got {suffix:?}"
        );
    }

    #[test]
    fn test_user_agent_suffix_empty_user_value_ignored() {
        let mut config = create_test_config();
        config.user_agent = Some(String::new());
        let suffix = config.user_agent_suffix();
        // An empty string is treated the same as no value: no trailing space.
        assert!(
            !suffix.contains(' '),
            "empty user_agent should be ignored, got {suffix:?}"
        );
    }
}
