use crate::{
    schema,
    sinks::{
        prelude::*,
        pulsar::sink::{healthcheck, PulsarSink},
    },
};
use futures_util::{FutureExt, TryFutureExt};
use pulsar::{
    authentication::oauth2::{OAuth2Authentication, OAuth2Params},
    compression,
    message::proto,
    Authentication, ConnectionRetryOptions, Error as PulsarError, ProducerOptions, Pulsar,
    TokioExecutor,
};
use pulsar::{error::AuthenticationError, OperationRetryOptions};
use std::path::Path;
use std::time::Duration;
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

    #[configurable(derived)]
    #[serde(default)]
    pub connection_retry_options: Option<CustomConnectionRetryOptions>,

    #[configurable(derived)]
    #[serde(default)]
    pub(crate) tls: Option<PulsarTlsOptions>,
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

#[configurable_component]
#[configurable(
    description = "Custom connection retry options configuration for the Pulsar client."
)]
#[derive(Clone, Debug)]
pub struct CustomConnectionRetryOptions {
    /// Minimum delay between connection retries.
    #[configurable(metadata(docs::type_unit = "milliseconds"))]
    pub min_backoff_ms: Option<u64>,

    /// Maximum delay between reconnection retries.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::examples = 30))]
    pub max_backoff_secs: Option<u64>,

    /// Maximum number of connection retries.
    #[configurable(metadata(docs::examples = 12))]
    pub max_retries: Option<u32>,

    /// Time limit to establish a connection.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::examples = 10))]
    pub connection_timeout_secs: Option<u64>,

    /// Keep-alive interval for each broker connection.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::examples = 60))]
    pub keep_alive_secs: Option<u64>,
}

#[configurable_component]
#[configurable(description = "TLS options configuration for the Pulsar client.")]
#[derive(Clone, Debug)]
pub struct PulsarTlsOptions {
    /// File path containing a list of PEM encoded certificates.
    #[configurable(metadata(docs::examples = "/etc/certs/chain.pem"))]
    pub ca_file: String,

    /// Enables certificate verification.
    ///
    /// Do NOT set this to `false` unless you understand the risks of not verifying the validity of certificates.
    pub verify_certificate: Option<bool>,

    /// Whether hostname verification is enabled when verify_certificate is false.
    ///
    /// Set to true if not specified.
    pub verify_hostname: Option<bool>,
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
            connection_retry_options: None,
            tls: None,
        }
    }
}

impl PulsarSinkConfig {
    pub(crate) async fn create_pulsar_client(&self) -> crate::Result<Pulsar<TokioExecutor>> {
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
                _ => return Err(Box::new(PulsarError::Authentication(AuthenticationError::Custom(
                    "Invalid auth config: can only specify name and token or oauth2 configuration"
                        .to_string(),
                ))))?,
            };
        }

        // Apply configuration for reconnection exponential backoff.
        let default_retry_options = ConnectionRetryOptions::default();
        let retry_options =
            self.connection_retry_options
                .as_ref()
                .map_or(default_retry_options.clone(), |opts| {
                    ConnectionRetryOptions {
                        min_backoff: opts
                            .min_backoff_ms
                            .map_or(default_retry_options.min_backoff, |ms| {
                                Duration::from_millis(ms)
                            }),
                        max_backoff: opts
                            .max_backoff_secs
                            .map_or(default_retry_options.max_backoff, |secs| {
                                Duration::from_secs(secs)
                            }),
                        max_retries: opts
                            .max_retries
                            .unwrap_or(default_retry_options.max_retries),
                        connection_timeout: opts
                            .connection_timeout_secs
                            .map_or(default_retry_options.connection_timeout, |secs| {
                                Duration::from_secs(secs)
                            }),
                        keep_alive: opts
                            .keep_alive_secs
                            .map_or(default_retry_options.keep_alive, |secs| {
                                Duration::from_secs(secs)
                            }),
                    }
                });

        builder = builder.with_connection_retry_options(retry_options);

        // Apply configuration for retrying Pulsar operations.
        let operation_retry_opts = OperationRetryOptions::default();
        builder = builder.with_operation_retry_options(operation_retry_opts);

        if let Some(options) = &self.tls {
            builder = builder.with_certificate_chain_file(Path::new(&options.ca_file))?;
            builder =
                builder.with_allow_insecure_connection(!options.verify_certificate.unwrap_or(true));
            builder = builder
                .with_tls_hostname_verification_enabled(options.verify_hostname.unwrap_or(true));
        }
        builder.build().map_err(|e| e.into()).await
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
            .map_err(|e| super::sink::BuildError::CreatePulsarSink { source: e })?;

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
