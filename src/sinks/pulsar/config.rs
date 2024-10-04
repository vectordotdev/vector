use crate::{
    schema,
    sinks::{
        prelude::*,
        pulsar::sink::{healthcheck, PulsarSink},
    },
};
use futures_util::FutureExt;
use pulsar::{
    authentication::oauth2::{OAuth2Authentication, OAuth2Params},
    compression,
    message::proto,
    Authentication, ConnectionRetryOptions, Error as PulsarError, ProducerOptions, Pulsar,
    TokioExecutor,
};
use pulsar::{error::AuthenticationError, OperationRetryOptions};
use snafu::ResultExt;
use vector_lib::codecs::{encoding::SerializerConfig, TextSerializerConfig};
use vector_lib::config::DataType;
use vector_lib::lookup::lookup_v2::OptionalTargetPath;
use vector_lib::sensitive_string::SensitiveString;
use vrl::value::Kind;

/// Configuration for the `pulsar` sink.
#[configurable_component(sink("pulsar", "Publish observability events to Apache Pulsar topics."))]
#[derive(Clone, Debug)]
pub struct PulsarSinkConfig {
    /// The endpoint to which the Pulsar client should connect to.
    ///
    /// The endpoint should specify the pulsar protocol and port.
    #[serde(alias = "address")]
    #[configurable(metadata(docs::examples = "pulsar://127.0.0.1:6650"))]
    pub(crate) endpoint: String,

    /// The Pulsar topic name to write events to.
    #[configurable(metadata(docs::examples = "topic-1234"))]
    pub(crate) topic: Template,

    /// The name of the producer. If not specified, the default name assigned by Pulsar is used.
    #[configurable(metadata(docs::examples = "producer-name"))]
    pub(crate) producer_name: Option<String>,

    /// The log field name or tags key to use for the partition key.
    ///
    /// If the field does not exist in the log event or metric tags, a blank value will be used.
    ///
    /// If omitted, the key is not sent.
    ///
    /// Pulsar uses a hash of the key to choose the topic-partition or uses round-robin if the record has no key.
    #[configurable(metadata(docs::examples = "message"))]
    #[configurable(metadata(docs::examples = "my_field"))]
    pub(crate) partition_key_field: Option<OptionalTargetPath>,

    /// The log field name to use for the Pulsar properties key.
    ///
    /// If omitted, no properties will be written.
    pub properties_key: Option<OptionalTargetPath>,

    #[configurable(derived)]
    #[serde(default)]
    pub(crate) batch: PulsarBatchConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: PulsarCompression,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    pub(crate) auth: Option<PulsarAuthConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

/// Event batching behavior.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PulsarBatchConfig {
    /// The maximum amount of events in a batch before it is flushed.
    ///
    /// Note this is an unsigned 32 bit integer which is a smaller capacity than
    /// many of the other sink batch settings.
    #[configurable(metadata(docs::type_unit = "events"))]
    #[configurable(metadata(docs::examples = 1000))]
    pub max_events: Option<u32>,

    /// The maximum size of a batch before it is flushed.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub max_bytes: Option<usize>,
}

/// Authentication configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub(crate) struct PulsarAuthConfig {
    /// Basic authentication name/username.
    ///
    /// This can be used either for basic authentication (username/password) or JWT authentication.
    /// When used for JWT, the value should be `token`.
    #[configurable(metadata(docs::examples = "${PULSAR_NAME}"))]
    #[configurable(metadata(docs::examples = "name123"))]
    name: Option<String>,

    /// Basic authentication password/token.
    ///
    /// This can be used either for basic authentication (username/password) or JWT authentication.
    /// When used for JWT, the value should be the signed JWT, in the compact representation.
    #[configurable(metadata(docs::examples = "${PULSAR_TOKEN}"))]
    #[configurable(metadata(docs::examples = "123456789"))]
    token: Option<SensitiveString>,

    #[configurable(derived)]
    oauth2: Option<OAuth2Config>,
}

/// OAuth2-specific authentication configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct OAuth2Config {
    /// The issuer URL.
    #[configurable(metadata(docs::examples = "${OAUTH2_ISSUER_URL}"))]
    #[configurable(metadata(docs::examples = "https://oauth2.issuer"))]
    issuer_url: String,

    /// The credentials URL.
    ///
    /// A data URL is also supported.
    #[configurable(metadata(docs::examples = "{OAUTH2_CREDENTIALS_URL}"))]
    #[configurable(metadata(docs::examples = "file:///oauth2_credentials"))]
    #[configurable(metadata(docs::examples = "data:application/json;base64,cHVsc2FyCg=="))]
    credentials_url: String,

    /// The OAuth2 audience.
    #[configurable(metadata(docs::examples = "${OAUTH2_AUDIENCE}"))]
    #[configurable(metadata(docs::examples = "pulsar"))]
    audience: Option<String>,

    /// The OAuth2 scope.
    #[configurable(metadata(docs::examples = "${OAUTH2_SCOPE}"))]
    #[configurable(metadata(docs::examples = "admin"))]
    scope: Option<String>,
}

/// Supported compression types for Pulsar.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum PulsarCompression {
    /// No compression.
    #[derivative(Default)]
    None,

    /// LZ4.
    Lz4,

    /// Zlib.
    Zlib,

    /// Zstandard.
    Zstd,

    /// Snappy.
    Snappy,
}

impl Default for PulsarSinkConfig {
    fn default() -> Self {
        Self {
            endpoint: "pulsar://127.0.0.1:6650".to_string(),
            topic: Template::try_from("topic-1234")
                .expect("Unable to parse default template topic"),
            producer_name: None,
            properties_key: None,
            partition_key_field: None,
            batch: Default::default(),
            compression: Default::default(),
            encoding: TextSerializerConfig::default().into(),
            auth: None,
            acknowledgements: Default::default(),
        }
    }
}

impl PulsarSinkConfig {
    pub(crate) async fn create_pulsar_client(&self) -> Result<Pulsar<TokioExecutor>, PulsarError> {
        let mut builder = Pulsar::builder(&self.endpoint, TokioExecutor);
        if let Some(auth) = &self.auth {
            builder = match (
                auth.name.as_ref(),
                auth.token.as_ref(),
                auth.oauth2.as_ref(),
            ) {
                (Some(name), Some(token), None) => builder.with_auth(Authentication {
                    name: name.clone(),
                    data: token.inner().as_bytes().to_vec(),
                }),
                (None, None, Some(oauth2)) => builder.with_auth_provider(
                    OAuth2Authentication::client_credentials(OAuth2Params {
                        issuer_url: oauth2.issuer_url.clone(),
                        credentials_url: oauth2.credentials_url.clone(),
                        audience: oauth2.audience.clone(),
                        scope: oauth2.scope.clone(),
                    }),
                ),
                _ => return Err(PulsarError::Authentication(AuthenticationError::Custom(
                    "Invalid auth config: can only specify name and token or oauth2 configuration"
                        .to_string(),
                ))),
            };
        }

        // Apply configuration for reconnection exponential backoff.
        let retry_opts = ConnectionRetryOptions::default();
        builder = builder.with_connection_retry_options(retry_opts);

        // Apply configuration for retrying Pulsar operations.
        let operation_retry_opts = OperationRetryOptions::default();
        builder = builder.with_operation_retry_options(operation_retry_opts);

        builder.build().await
    }

    pub(crate) fn build_producer_options(&self) -> ProducerOptions {
        let mut opts = ProducerOptions {
            encrypted: None,
            access_mode: Some(0),
            metadata: Default::default(),
            schema: None,
            batch_size: self.batch.max_events,
            batch_byte_size: self.batch.max_bytes,
            compression: None,
        };

        match &self.compression {
            PulsarCompression::None => opts.compression = Some(compression::Compression::None),
            PulsarCompression::Lz4 => {
                opts.compression = Some(compression::Compression::Lz4(
                    compression::CompressionLz4::default(),
                ))
            }
            PulsarCompression::Zlib => {
                opts.compression = Some(compression::Compression::Zlib(
                    compression::CompressionZlib::default(),
                ))
            }
            PulsarCompression::Zstd => {
                opts.compression = Some(compression::Compression::Zstd(
                    compression::CompressionZstd::default(),
                ))
            }
            PulsarCompression::Snappy => {
                opts.compression = Some(compression::Compression::Snappy(
                    compression::CompressionSnappy::default(),
                ))
            }
        }

        if let SerializerConfig::Avro { avro } = self.encoding.config() {
            opts.schema = Some(proto::Schema {
                schema_data: avro.schema.as_bytes().into(),
                r#type: proto::schema::Type::Avro as i32,
                ..Default::default()
            });
        }
        opts
    }
}

impl GenerateConfig for PulsarSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::default()).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "pulsar")]
impl SinkConfig for PulsarSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self
            .create_pulsar_client()
            .await
            .context(super::sink::CreatePulsarSinkSnafu)?;

        let sink = PulsarSink::new(client, self.clone())?;

        let hc = healthcheck(self.clone()).boxed();

        Ok((VectorSink::from_event_streamsink(sink), hc))
    }

    fn input(&self) -> Input {
        let requirement =
            schema::Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::new(self.encoding.config().input_type() & (DataType::Log | DataType::Metric))
            .with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}
