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
    message::proto, Authentication, Error as PulsarError, ProducerOptions, Pulsar, TokioExecutor,
};
use snafu::ResultExt;
use vector_config::configurable_component;
use vector_core::config::DataType;

/// Configuration for the `pulsar` sink.
#[configurable_component(sink("pulsar"))]
#[derive(Clone, Debug)]
pub struct PulsarSinkConfig {
    /// The endpoint to which the Pulsar client should connect to.
    #[serde(alias = "address")]
    pub endpoint: String,

    /// The Pulsar topic name to write events to.
    #[configurable(metadata(templateable))]
    pub topic: String,

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

    /// Determines the batch size.
    ///
    /// Defaults to 1000.
    #[configurable(derived)]
    #[serde(default)]
    pub batch_size: Option<u32>,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Option<PulsarCompression>,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    pub auth: Option<AuthConfig>,

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

/// Identifies the compression options that are available within Pulsar.
#[configurable_component]
#[derive(Clone, Debug)]
pub enum PulsarCompression {
    /// No compression.
    None,

    /// [LZ4][lz4] compression.
    ///
    /// [lz4]: https://lz4.github.io/lz4/
    Lz4,

    /// [Zlib][zlib] compression.
    ///
    /// [zlib]: https://www.zlib.net
    Zlib,

    /// [Zstd][zstd] compression.
    ///
    /// [zstd]: https://zstd.net
    Zstd,

    /// [Snappy][snappy] compression.
    ///
    /// [snappy]: https://google.github.io/snappy/
    Snappy,
}

/// Authentication configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct AuthConfig {
    /// Basic authentication name/username.
    ///
    /// This can be used either for basic authentication (username/password) or JWT authentication.
    /// When used for JWT, the value should be `token`.
    name: Option<String>,

    /// Basic authentication password/token.
    ///
    /// This can be used either for basic authentication (username/password) or JWT authentication.
    /// When used for JWT, the value should be the signed JWT, in the compact representation.
    token: Option<String>,

    #[configurable(derived)]
    oauth2: Option<OAuth2Config>,
}

/// OAuth2-specific authenticatgion configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct OAuth2Config {
    /// The issuer URL.
    issuer_url: String,

    /// The credentials URL.
    ///
    /// A data URL is also supported.
    credentials_url: String,

    /// The OAuth2 audience.
    audience: Option<String>,

    /// The OAuth2 scope.
    scope: Option<String>,
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
                    data: token.as_bytes().to_vec(),
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
        if let Some(compression) = &self.compression {
            match compression {
                PulsarCompression::None => opts.compression = Some(proto::CompressionType::None),
                PulsarCompression::Lz4 => opts.compression = Some(proto::CompressionType::Lz4),
                PulsarCompression::Zlib => opts.compression = Some(proto::CompressionType::Zlib),
                PulsarCompression::Zstd => opts.compression = Some(proto::CompressionType::Zstd),
                PulsarCompression::Snappy => {
                    opts.compression = Some(proto::CompressionType::Snappy)
                }
            }
        }
        opts.batch_size = self.batch_size.to_owned();
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
            key_field: None,
            properties_key: None,
            batch_size: None,
            compression: None,
            encoding: TextSerializerConfig::new().into(),
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
