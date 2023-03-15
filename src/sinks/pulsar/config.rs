use crate::sinks::util::TowerRequestConfig;
use crate::{
    codecs::EncodingConfig,
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::{
        pulsar::sink::{healthcheck, PulsarSink},
        Healthcheck, VectorSink,
    },
};
use codecs::{encoding::SerializerConfig, TextSerializerConfig};
use futures_util::FutureExt;
use pulsar::authentication::oauth2::{OAuth2Authentication, OAuth2Params};
use pulsar::error::AuthenticationError;
use pulsar::{
    compression, message::proto, Authentication, Error as PulsarError, ProducerOptions, Pulsar,
    TokioExecutor,
};
use snafu::ResultExt;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;
use vector_core::config::DataType;

/// Configuration for the `pulsar` sink.
#[configurable_component(sink("pulsar"))]
#[derive(Clone, Debug)]
pub struct PulsarSinkConfig {
    /// The endpoint to which the Pulsar client should connect to.
    ///
    /// The endpoint should specify the pulsar protocol and port.
    #[serde(alias = "address")]
    #[configurable(metadata(docs::examples = "pulsar://127.0.0.1:6650"))]
    endpoint: String,

    /// The Pulsar topic name to write events to.
    #[configurable(metadata(docs::examples = "topic-1234"))]
    pub(crate) topic: String,

    /// The name of the producer. If not specified, the default name assigned by Pulsar will be used.
    #[configurable(metadata(docs::examples = "producer-name"))]
    producer_name: Option<String>,

    /// The log field name or tags key to use for the topic key.
    ///
    /// If the field does not exist in the log or in tags, a blank value will be used. If unspecified, the key is not sent.
    ///
    /// Pulsar uses a hash of the key to choose the topic-partition or uses round-robin if the record has no key.
    pub key_field: Option<String>,

    /// The log field name to use for the Pulsar properties.
    ///
    /// If omitted, no properties will be written.
    pub properties_key: Option<String>,

    /// Log field to use as Pulsar message key.
    #[configurable(metadata(docs::examples = "message"))]
    #[configurable(metadata(docs::examples = "my_field"))]
    partition_key_field: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    batch: BatchConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Option<PulsarCompression>,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    auth: Option<AuthConfig>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

/// Event batching behavior.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
struct BatchConfig {
    /// The maximum size of a batch before it is flushed.
    #[configurable(metadata(docs::type_unit = "events"))]
    #[configurable(metadata(docs::examples = 1000))]
    pub max_events: Option<u32>,
}

/// Authentication configuration.
#[configurable_component]
#[derive(Clone, Debug)]
struct AuthConfig {
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

        builder.build().await
    }

    pub(crate) fn build_producer_options(&self) -> ProducerOptions {
        let mut opts = ProducerOptions {
            encrypted: None,
            access_mode: Some(0),
            metadata: Default::default(),
            schema: None,
            batch_size: None,
            compression: None,
        };
        if let Some(config_compression) = &self.compression {
            match config_compression {
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
        }
        opts.batch_size = self.batch.max_events;
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
        toml::Value::try_from(Self {
            endpoint: "pulsar://127.0.0.1:6650".to_string(),
            request: TowerRequestConfig::default(),
            topic: "topic-1234".to_string(),
            producer_name: None,
            key_field: None,
            properties_key: None,
            partition_key_field: None,
            batch: Default::default(),
            compression: None,
            encoding: TextSerializerConfig::default().into(),
            auth: None,
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
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
        Input::new(self.encoding.config().input_type() & (DataType::Log | DataType::Metric))
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}
