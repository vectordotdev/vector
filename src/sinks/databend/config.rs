use std::{collections::BTreeMap, sync::Arc};

use databend_client::APIClient as DatabendAPIClient;
use futures::future::FutureExt;
use tower::ServiceBuilder;
use vector_lib::{
    codecs::encoding::{Framer, FramingConfig},
    configurable::{component::GenerateConfig, configurable_component},
};

use super::{
    compression::DatabendCompression,
    encoding::{DatabendEncodingConfig, DatabendMissingFieldAS, DatabendSerializerConfig},
    request_builder::DatabendRequestBuilder,
    service::{DatabendRetryLogic, DatabendService, DatabendServiceSettings},
    sink::DatabendSink,
};
use crate::{
    codecs::{Encoder, EncodingConfig},
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    http::{Auth, MaybeAuth},
    sinks::{
        Healthcheck, VectorSink,
        util::{
            BatchConfig, Compression, RealtimeSizeBasedDefaultBatchSettings, ServiceBuilderExt,
            TowerRequestConfig, UriSerde,
        },
    },
    tls::TlsConfig,
    vector_version,
};

fn default_stage() -> String {
    "~".to_string()
}

fn default_stage_path_prefix() -> String {
    "vector".to_string()
}

fn default_raw_message_key() -> String {
    "message".to_string()
}

/// Databend load API to use for this sink.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum DatabendLoadMode {
    /// Upload each batch to a stage, then load it into the table.
    #[default]
    Staged,

    /// Stream each batch directly through Databend's `/v1/streaming_load` endpoint.
    Streaming,
}

/// Metadata options for raw ingest records.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(default)]
pub struct DatabendRawMetadataOptions {
    /// Vector metadata paths to include in `record_metadata`.
    ///
    /// If unset, all supported metadata fields are included. If set to an empty array, no
    /// metadata fields are included.
    pub includes: Vec<String>,
}

impl Default for DatabendRawMetadataOptions {
    fn default() -> Self {
        Self {
            includes: vec!["*".to_string()],
        }
    }
}

fn default_raw_metadata() -> DatabendRawMetadataOptions {
    DatabendRawMetadataOptions::default()
}

/// COPY options used by staged Databend loads.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(default)]
pub struct DatabendCopyOptions {
    /// Whether to remove loaded files from the stage after a successful load.
    pub purge: bool,

    /// Whether to load files even if Databend has loaded them before.
    pub force: bool,

    /// Whether to disable variant type checks while loading.
    pub disable_variant_check: bool,

    /// How Databend handles errors while loading staged files.
    pub on_error: DatabendCopyOnError,
}

impl Default for DatabendCopyOptions {
    fn default() -> Self {
        Self {
            purge: true,
            force: false,
            disable_variant_check: false,
            on_error: DatabendCopyOnError::Abort,
        }
    }
}

/// COPY `ON_ERROR` behavior.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum DatabendCopyOnError {
    /// Abort the load when an error is encountered.
    #[default]
    Abort,

    /// Continue loading other rows when an error is encountered.
    Continue,
}

impl DatabendCopyOnError {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Abort => "abort",
            Self::Continue => "continue",
        }
    }
}

/// Raw Kafka ingest compatibility mode.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(default)]
pub struct DatabendRawOptions {
    /// Enable bend-ingest-kafka compatible raw records.
    pub enabled: bool,

    /// Controls which Vector metadata fields are copied into `record_metadata`.
    #[serde(default = "default_raw_metadata")]
    pub metadata: DatabendRawMetadataOptions,

    /// Create the raw target table during sink startup if it does not exist.
    pub create_table: bool,

    /// Event field containing the raw Kafka payload.
    #[serde(default = "default_raw_message_key")]
    pub message_key: String,
}

impl Default for DatabendRawOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            metadata: default_raw_metadata(),
            create_table: false,
            message_key: default_raw_message_key(),
        }
    }
}

/// Configuration for the `databend` sink.
#[configurable_component(sink("databend", "Deliver log data to a Databend database."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DatabendConfig {
    /// The DSN of the Databend server.
    #[configurable(metadata(
        docs::examples = "databend://localhost:8000/default?sslmode=disable"
    ))]
    pub endpoint: UriSerde,

    /// The TLS configuration to use when connecting to the Databend server.
    #[configurable(
        deprecated = "This option has been deprecated, use arguments in the DSN instead."
    )]
    pub tls: Option<TlsConfig>,

    /// The database that contains the table that data is inserted into. Overrides the database in DSN.
    #[configurable(metadata(docs::examples = "mydatabase"))]
    pub database: Option<String>,

    /// The username and password to authenticate with. Overrides the username and password in DSN.
    #[configurable(derived)]
    pub auth: Option<Auth>,

    /// The table that data is inserted into.
    #[configurable(metadata(docs::examples = "mytable"))]
    pub table: String,

    #[configurable(derived)]
    #[serde(default)]
    pub missing_field_as: DatabendMissingFieldAS,

    #[configurable(derived)]
    #[serde(default)]
    pub encoding: DatabendEncodingConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: DatabendCompression,

    /// The Databend load API to use.
    #[serde(default)]
    pub load_mode: DatabendLoadMode,

    /// The user stage to upload staged batch files into.
    #[serde(default = "default_stage")]
    pub stage: String,

    /// Path prefix used under `stage` for staged batch files.
    #[serde(default = "default_stage_path_prefix")]
    pub stage_path_prefix: String,

    /// COPY options for staged loads.
    #[serde(default)]
    pub copy_options: DatabendCopyOptions,

    /// Columns used for `REPLACE INTO ... ON (...)`.
    ///
    /// When empty, Databend uses normal insert mode. When non-empty, Databend uses replace mode.
    /// This is independent of raw mode. Configure it to match the target table's logical key.
    #[serde(default)]
    pub primary_key: Vec<String>,

    /// bend-ingest-kafka compatible raw Kafka ingest options.
    #[serde(default)]
    pub raw: DatabendRawOptions,

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

impl GenerateConfig for DatabendConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"endpoint = "databend://localhost:8000/default?sslmode=disable"
            table = "default"
        "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "databend")]
impl SinkConfig for DatabendConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let ua = format!("vector/{}", vector_version());
        let auth = self.auth.choose_one(&self.endpoint.auth)?;
        let authority = self
            .endpoint
            .uri
            .authority()
            .ok_or("Endpoint missing authority")?;
        let endpoint = match self.endpoint.uri.scheme().map(|s| s.as_str()) {
            Some("databend") => self.endpoint.to_string(),
            // for backward compatibility, build DSN from endpoint
            Some("http") => format!("databend://{authority}/?sslmode=disable"),
            Some("https") => format!("databend://{authority}"),
            None => {
                return Err("Missing scheme for Databend endpoint. Expected `databend`.".into());
            }
            Some(s) => {
                return Err(format!("Unsupported scheme for Databend endpoint: {s}").into());
            }
        };
        let mut endpoint = url::Url::parse(&endpoint)?;
        match auth {
            Some(Auth::Basic { user, password }) => {
                let _ = endpoint.set_username(&user);
                let _ = endpoint.set_password(Some(password.inner()));
            }
            Some(Auth::Bearer { .. }) => {
                return Err("Bearer authentication is not supported currently".into());
            }
            Some(Auth::Custom { .. }) => {
                return Err("Custom authentication is not supported currently".into());
            }
            None => {}
            #[cfg(feature = "aws-core")]
            _ => {}
        }
        if let Some(database) = &self.database {
            endpoint.set_path(&format!("/{database}"));
        }
        let endpoint = endpoint.to_string();
        let health_client = DatabendAPIClient::new(&endpoint, Some(ua.clone())).await?;
        let healthcheck = select_one(health_client).boxed();

        let request_settings = self.request.into_settings();
        let batch_settings = self.batch.into_batcher_settings()?;

        let mut file_format_options = BTreeMap::new();
        let compression = match self.compression {
            DatabendCompression::Gzip => {
                file_format_options.insert("compression", "GZIP");
                Compression::gzip_default()
            }
            DatabendCompression::None => {
                file_format_options.insert("compression", "NONE");
                Compression::None
            }
            DatabendCompression::Zstd => {
                file_format_options.insert("compression", "ZSTD");
                Compression::zstd_default()
            }
        };
        let encoding: EncodingConfig = self.encoding.clone().into();
        let serializer = match self.encoding.config() {
            DatabendSerializerConfig::Json(_) => {
                file_format_options.insert("type", "NDJSON");
                file_format_options.insert("missing_field_as", self.missing_field_as.as_str());
                encoding.build()?
            }
            DatabendSerializerConfig::Csv(_) => {
                file_format_options.insert("type", "CSV");
                file_format_options.insert("field_delimiter", ",");
                file_format_options.insert("record_delimiter", "\n");
                file_format_options.insert("skip_header", "0");
                encoding.build()?
            }
        };
        let framer = FramingConfig::NewlineDelimited.build();
        let transformer = encoding.transformer();

        if matches!(self.load_mode, DatabendLoadMode::Streaming) {
            file_format_options.insert("compression", "AUTO");
        }

        let mut copy_options = BTreeMap::new();
        copy_options.insert(
            "purge",
            if self.copy_options.purge {
                "true"
            } else {
                "false"
            },
        );
        copy_options.insert(
            "force",
            if self.copy_options.force {
                "true"
            } else {
                "false"
            },
        );
        copy_options.insert(
            "disable_variant_check",
            if self.copy_options.disable_variant_check {
                "true"
            } else {
                "false"
            },
        );
        copy_options.insert("on_error", self.copy_options.on_error.as_str());

        let client = DatabendAPIClient::new(&endpoint, Some(ua)).await?;
        if self.raw.create_table {
            let sql = format!(
                "CREATE TABLE IF NOT EXISTS {} (uuid String, koffset BIGINT, kpartition INT, raw_data JSON, record_metadata JSON, add_time TIMESTAMP)",
                self.table
            );
            client.query_all(&sql).await?;
        }
        let service = DatabendService::new(
            client,
            DatabendServiceSettings {
                table: self.table.clone(),
                load_mode: self.load_mode,
                stage: self.stage.clone(),
                stage_path_prefix: self.stage_path_prefix.clone(),
                file_format_options,
                copy_options,
                primary_key: self.primary_key.clone(),
            },
        )?;
        let service = ServiceBuilder::new()
            .settings(request_settings, DatabendRetryLogic)
            .service(service);

        let encoder = Encoder::<Framer>::new(framer, serializer);
        let request_builder =
            DatabendRequestBuilder::new(compression, (transformer, encoder), self.raw.clone());

        let sink = DatabendSink::new(batch_settings, request_builder, service);

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

async fn select_one(client: Arc<DatabendAPIClient>) -> crate::Result<()> {
    client.query_all("SELECT 1").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatabendConfig>();
    }

    #[test]
    fn parse_config() {
        let cfg = toml::from_str::<DatabendConfig>(
            r#"
            endpoint = "databend://localhost:8000/mydatabase?sslmode=disable"
            table = "mytable"
        "#,
        )
        .unwrap();
        assert_eq!(
            cfg.endpoint.uri,
            "databend://localhost:8000/mydatabase?sslmode=disable"
        );
        assert_eq!(cfg.table, "mytable");
        assert!(matches!(
            cfg.encoding.config(),
            DatabendSerializerConfig::Json(_)
        ));
        assert!(matches!(cfg.compression, DatabendCompression::None));
    }

    #[test]
    fn parse_config_with_encoding_compression() {
        let cfg = toml::from_str::<DatabendConfig>(
            r#"
            endpoint = "databend://localhost:8000/mydatabase?sslmode=disable"
            table = "mytable"
            encoding.codec = "csv"
            encoding.csv.fields = ["host", "timestamp", "message"]
            compression = "gzip"
        "#,
        )
        .unwrap();
        assert_eq!(
            cfg.endpoint.uri,
            "databend://localhost:8000/mydatabase?sslmode=disable"
        );
        assert_eq!(cfg.table, "mytable");
        assert!(matches!(
            cfg.encoding.config(),
            DatabendSerializerConfig::Csv(_)
        ));
        assert!(matches!(cfg.compression, DatabendCompression::Gzip));
    }

    #[test]
    fn parse_config_with_ingest_options() {
        let cfg = toml::from_str::<DatabendConfig>(
            r#"
            endpoint = "databend://localhost:8000/mydatabase?sslmode=disable"
            table = "mytable"
            compression = "zstd"
            load_mode = "streaming"
            stage = "ingest_stage"
            stage_path_prefix = "kafka"
            copy_options.purge = false
            copy_options.force = true
            copy_options.disable_variant_check = true
            copy_options.on_error = "continue"
            primary_key = ["id", "source"]
            raw.enabled = true
            raw.create_table = true
            raw.message_key = "message"
            raw.metadata.includes = ["%kafka.topic", "%kafka.partition", "%kafka.offset", "%kafka.message_key"]
        "#,
        )
        .unwrap();

        assert!(matches!(cfg.compression, DatabendCompression::Zstd));
        assert!(matches!(cfg.load_mode, DatabendLoadMode::Streaming));
        assert_eq!(cfg.stage, "ingest_stage");
        assert_eq!(cfg.stage_path_prefix, "kafka");
        assert!(!cfg.copy_options.purge);
        assert!(cfg.copy_options.force);
        assert!(cfg.copy_options.disable_variant_check);
        assert!(matches!(
            cfg.copy_options.on_error,
            DatabendCopyOnError::Continue
        ));
        assert_eq!(cfg.primary_key, ["id", "source"]);
        assert!(cfg.raw.enabled);
        assert!(cfg.raw.create_table);
        assert_eq!(
            cfg.raw.metadata.includes,
            [
                "%kafka.topic",
                "%kafka.partition",
                "%kafka.offset",
                "%kafka.message_key",
            ]
        );
    }

    #[test]
    fn parse_config_with_default_raw_metadata_includes_all() {
        let cfg = toml::from_str::<DatabendConfig>(
            r#"
            endpoint = "databend://localhost:8000/mydatabase?sslmode=disable"
            table = "mytable"
            raw.enabled = true
        "#,
        )
        .unwrap();

        assert_eq!(cfg.raw.metadata.includes, [String::from("*")]);
    }

    #[test]
    fn parse_config_with_empty_raw_metadata_config_includes_all() {
        let cfg = toml::from_str::<DatabendConfig>(
            r#"
            endpoint = "databend://localhost:8000/mydatabase?sslmode=disable"
            table = "mytable"
            raw.enabled = true
            raw.metadata = {}
        "#,
        )
        .unwrap();

        assert_eq!(cfg.raw.metadata.includes, [String::from("*")]);
    }

    #[test]
    fn parse_config_with_empty_raw_metadata_includes_none() {
        let cfg = toml::from_str::<DatabendConfig>(
            r#"
            endpoint = "databend://localhost:8000/mydatabase?sslmode=disable"
            table = "mytable"
            raw.enabled = true
            raw.metadata.includes = []
        "#,
        )
        .unwrap();

        assert!(cfg.raw.metadata.includes.is_empty());
    }

    #[test]
    fn parse_config_with_default_primary_key_empty() {
        let cfg = toml::from_str::<DatabendConfig>(
            r#"
            endpoint = "databend://localhost:8000/mydatabase?sslmode=disable"
            table = "mytable"
        "#,
        )
        .unwrap();

        assert!(cfg.primary_key.is_empty());
    }
}
