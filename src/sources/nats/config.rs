use async_nats::jetstream::{
    consumer::{PullConsumer, StreamError as ConsumerStreamError},
    context::GetStreamError,
};
use snafu::{ResultExt, Snafu};
use vector_lib::{
    codecs::decoding::{DeserializerConfig, FramingConfig},
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    lookup::{lookup_v2::OptionalValuePath, owned_value_path},
};
use vrl::value::Kind;

use crate::{
    codecs::DecodingConfig,
    config::{GenerateConfig, SourceConfig, SourceContext, SourceOutput},
    nats::{NatsAuthConfig, NatsConfigError, from_tls_auth_config},
    serde::{default_decoding, default_framing_message_based},
    sources::{
        Source,
        nats::source::{create_subscription, run_nats_core, run_nats_jetstream},
    },
    tls::TlsEnableableConfig,
};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum BuildError {
    #[snafu(display("NATS Config Error: {}", source))]
    Config { source: NatsConfigError },
    #[snafu(display("NATS Connect Error: {}", source))]
    Connect { source: async_nats::ConnectError },
    #[snafu(display("NATS Subscribe Error: {}", source))]
    Subscribe { source: async_nats::SubscribeError },
    #[snafu(display("NATS stream not found: {}", source))]
    Stream { source: GetStreamError },
    #[snafu(display("Failed to get NATS consumer: {}", source))]
    Consumer { source: async_nats::Error },
    #[snafu(display("Failed to retrieve messages from NATS consumer: {}", source))]
    Messages { source: ConsumerStreamError },
}

/// Batch settings for a JetStream pull consumer.
///
/// By default, messages are pulled in batches of up to 200.
/// Each pull request expires after 30 seconds if not fulfilled.
/// There is no explicit maximum byte size per batch unless specified.
///
/// **Note:** These defaults follow the `async-nats` crate’s `StreamBuilder`.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct BatchConfig {
    /// The maximum number of messages to pull in a single batch.
    #[serde(default = "default_batch")]
    batch: usize,

    /// The maximum total byte size for a batch. The pull request will be
    /// fulfilled when either `size` or `max_bytes` is reached.
    #[serde(default = "default_max_bytes")]
    max_bytes: usize,
}

const fn default_batch() -> usize {
    200
}

const fn default_max_bytes() -> usize {
    0
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            batch: default_batch(),
            max_bytes: default_max_bytes(),
        }
    }
}

/// Configuration for NATS JetStream.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct JetStreamConfig {
    /// The name of the stream to bind to.
    pub stream: String,
    /// The name of the durable consumer to pull from.
    pub consumer: String,

    #[serde(default)]
    #[configurable(derived)]
    pub batch_config: BatchConfig,
}

/// Configuration for the `nats` source.
#[configurable_component(source(
    "nats",
    "Read observability data from subjects on the NATS messaging system."
))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct NatsSourceConfig {
    /// The NATS URL to connect to.
    ///
    /// The URL takes the form of `nats://server:port`.
    /// If the port is not specified it defaults to 4222.
    #[configurable(metadata(docs::examples = "nats://demo.nats.io"))]
    #[configurable(metadata(docs::examples = "nats://127.0.0.1:4242"))]
    #[configurable(metadata(
        docs::examples = "nats://localhost:4222,nats://localhost:5222,nats://localhost:6222"
    ))]
    pub url: String,

    /// A [name][nats_connection_name] assigned to the NATS connection.
    ///
    /// [nats_connection_name]: https://docs.nats.io/using-nats/developer/connecting/name
    #[serde(alias = "name")]
    #[configurable(metadata(docs::examples = "vector"))]
    pub connection_name: String,

    /// The NATS [subject][nats_subject] to pull messages from.
    ///
    /// [nats_subject]: https://docs.nats.io/nats-concepts/subjects
    #[configurable(metadata(docs::examples = "foo"))]
    #[configurable(metadata(docs::examples = "time.us.east"))]
    #[configurable(metadata(docs::examples = "time.*.east"))]
    #[configurable(metadata(docs::examples = "time.>"))]
    #[configurable(metadata(docs::examples = ">"))]
    pub subject: String,

    /// The NATS queue group to join.
    pub queue: Option<String>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,

    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    pub auth: Option<NatsAuthConfig>,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,

    /// The `NATS` subject key.
    #[serde(default = "default_subject_key_field")]
    pub subject_key_field: OptionalValuePath,

    /// The buffer capacity of the underlying NATS subscriber.
    ///
    /// This value determines how many messages the NATS subscriber buffers
    /// before incoming messages are dropped.
    ///
    /// See the [async_nats documentation][async_nats_subscription_capacity] for more information.
    ///
    /// [async_nats_subscription_capacity]: https://docs.rs/async-nats/latest/async_nats/struct.ConnectOptions.html#method.subscription_capacity
    #[serde(default = "default_subscription_capacity")]
    #[derivative(Default(value = "default_subscription_capacity()"))]
    pub subscriber_capacity: usize,

    #[configurable(derived)]
    #[serde(default)]
    pub jetstream: Option<JetStreamConfig>,
}

pub fn default_subject_key_field() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("subject"))
}

pub const fn default_subscription_capacity() -> usize {
    65536
}

impl GenerateConfig for NatsSourceConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            connection_name = "vector"
            subject = "from.vector"
            url = "nats://127.0.0.1:4222""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "nats")]
impl SourceConfig for NatsSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        match self.mode() {
            NatsMode::JetStream(js_config) => {
                let connection = self.connect().await?;
                let js = async_nats::jetstream::new(connection.clone());
                let stream = js
                    .get_stream(&js_config.stream)
                    .await
                    .context(StreamSnafu)?;
                let consumer: PullConsumer = stream
                    .get_consumer(&js_config.consumer)
                    .await
                    .context(ConsumerSnafu)?;

                let batch_config = js_config.batch_config.clone();

                let messages = consumer
                    .stream()
                    .max_messages_per_batch(batch_config.batch)
                    .max_bytes_per_batch(batch_config.max_bytes)
                    .messages()
                    .await
                    .context(MessagesSnafu)?;

                Ok(Box::pin(run_nats_jetstream(
                    self.clone(),
                    connection,
                    messages,
                    decoder,
                    log_namespace,
                    cx.shutdown,
                    cx.out,
                )))
            }
            NatsMode::Core => {
                let (connection, subscription) = create_subscription(self).await?;

                Ok(Box::pin(run_nats_core(
                    self.clone(),
                    connection,
                    subscription,
                    decoder,
                    log_namespace,
                    cx.shutdown,
                    cx.out,
                )))
            }
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let legacy_subject_key_field = self
            .subject_key_field
            .clone()
            .path
            .map(LegacyKey::InsertIfEmpty);
        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                NatsSourceConfig::NAME,
                legacy_subject_key_field,
                &owned_value_path!("subject"),
                Kind::bytes(),
                None,
            );

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    // Acknowledgment is only possible with Jetstream.
    fn can_acknowledge(&self) -> bool {
        true
    }
}

enum NatsMode<'a> {
    JetStream(&'a JetStreamConfig),
    Core,
}

impl NatsSourceConfig {
    pub async fn connect(&self) -> Result<async_nats::Client, BuildError> {
        let options: async_nats::ConnectOptions = self.try_into().context(ConfigSnafu)?;

        let server_addrs = self.parse_server_addresses()?;
        options.connect(server_addrs).await.context(ConnectSnafu)
    }

    const fn mode(&self) -> NatsMode<'_> {
        match &self.jetstream {
            Some(config) => NatsMode::JetStream(config),
            None => NatsMode::Core,
        }
    }

    fn parse_server_addresses(&self) -> Result<Vec<async_nats::ServerAddr>, BuildError> {
        self.url
            .split(',')
            .map(|url| {
                url.parse::<async_nats::ServerAddr>()
                    .map_err(|_| BuildError::Connect {
                        source: async_nats::ConnectErrorKind::ServerParse.into(),
                    })
            })
            .collect()
    }
}

impl TryFrom<&NatsSourceConfig> for async_nats::ConnectOptions {
    type Error = NatsConfigError;

    fn try_from(config: &NatsSourceConfig) -> Result<Self, Self::Error> {
        from_tls_auth_config(&config.connection_name, &config.auth, &config.tls)
            .map(|options| options.subscription_capacity(config.subscriber_capacity))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::print_stdout)]

    use vector_lib::{
        lookup::{OwnedTargetPath, owned_value_path},
        schema::Definition,
    };
    use vrl::value::{Kind, kind::Collection};

    use super::*;
    use crate::sources::nats::config::default_subject_key_field;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<NatsSourceConfig>();
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = NatsSourceConfig {
            log_namespace: Some(true),
            subject_key_field: default_subject_key_field(),
            ..Default::default()
        };

        let definitions = config
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        let expected_definition =
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                .with_meaning(OwnedTargetPath::event_root(), "message")
                .with_metadata_field(
                    &owned_value_path!("vector", "source_type"),
                    Kind::bytes(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None,
                )
                .with_metadata_field(&owned_value_path!("nats", "subject"), Kind::bytes(), None);

        assert_eq!(definitions, Some(expected_definition));
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = NatsSourceConfig {
            subject_key_field: default_subject_key_field(),
            ..Default::default()
        };
        let definitions = config
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        let expected_definition = Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [LogNamespace::Legacy],
        )
        .with_event_field(
            &owned_value_path!("message"),
            Kind::bytes(),
            Some("message"),
        )
        .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None)
        .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
        .with_event_field(&owned_value_path!("subject"), Kind::bytes(), None);

        assert_eq!(definitions, Some(expected_definition));
    }
}
