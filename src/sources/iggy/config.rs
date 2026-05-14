use std::time::Duration;

use iggy::prelude::{
    AutoCommit, Client, IggyClient, IggyClientBuilder, IggyConsumer, IggyDuration,
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
    config::{
        GenerateConfig, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
    },
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    sources::{Source, iggy::source::run_iggy_source},
};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum BuildError {
    #[snafu(display("Iggy connection error: {}", source))]
    Connect { source: iggy::prelude::IggyError },
    #[snafu(display("Iggy consumer error: {}", source))]
    Consumer { source: iggy::prelude::IggyError },
}

/// Configuration for the `iggy` source.
#[configurable_component(source(
    "iggy",
    "Read observability data from a topic on the Iggy message streaming platform."
))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct IggySourceConfig {
    /// The Iggy [connection string][iggy_conn] of the server to consume from.
    ///
    /// The connection string takes the form
    /// `iggy+<protocol>://<credentials>@<host>:<port>` where `<protocol>` is one
    /// of `tcp`, `quic`, `http`, or `ws`, and `<credentials>` is either
    /// `username:password` or a personal access token.
    ///
    /// [iggy_conn]: https://iggy.apache.org/docs/connection-string
    #[configurable(metadata(docs::examples = "iggy+tcp://iggy:iggy@127.0.0.1:8090"))]
    pub url: String,

    /// The Iggy stream name to consume from.
    #[configurable(metadata(docs::examples = "vector"))]
    pub stream: String,

    /// The Iggy topic name within the stream to consume from.
    #[configurable(metadata(docs::examples = "logs"))]
    pub topic: String,

    /// The consumer name. Used as the durable consumer identifier (and as the
    /// consumer group name when `partition` is unset).
    #[configurable(metadata(docs::examples = "vector"))]
    pub consumer_name: String,

    /// Pin the consumer to a single partition. When unset, a consumer group
    /// named after `consumer_name` is used and the broker assigns partitions
    /// across members.
    #[serde(default)]
    pub partition: Option<u32>,

    /// The maximum number of messages pulled per poll. Defaults to 1000.
    #[serde(default = "default_batch_length")]
    pub batch_length: u32,

    /// The minimum interval, in milliseconds, between consecutive polls.
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,

    /// The interval, in seconds, at which consumer offsets are committed to the
    /// Iggy server. Only used when end-to-end acknowledgements are enabled.
    #[serde(default = "default_commit_interval_secs")]
    pub commit_interval_secs: u64,

    /// The maximum time, in seconds, to wait for in-flight events to be
    /// acknowledged downstream during shutdown before the final consumer
    /// offsets are committed. Only used when end-to-end acknowledgements are
    /// enabled.
    #[serde(default = "default_drain_timeout_secs")]
    pub drain_timeout_secs: u64,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,

    /// The Iggy stream key under which the source stream name is recorded on
    /// each event (Legacy log namespace only).
    #[serde(default = "default_stream_key_field")]
    pub stream_key_field: OptionalValuePath,

    /// The Iggy topic key under which the source topic name is recorded on
    /// each event (Legacy log namespace only).
    #[serde(default = "default_topic_key_field")]
    pub topic_key_field: OptionalValuePath,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    pub acknowledgements: SourceAcknowledgementsConfig,
}

const fn default_batch_length() -> u32 {
    1000
}

const fn default_poll_interval_ms() -> u64 {
    100
}

const fn default_commit_interval_secs() -> u64 {
    5
}

const fn default_drain_timeout_secs() -> u64 {
    5
}

pub fn default_stream_key_field() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("stream"))
}

pub fn default_topic_key_field() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("topic"))
}

impl GenerateConfig for IggySourceConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            url = "iggy+tcp://iggy:iggy@127.0.0.1:8090"
            stream = "vector"
            topic = "logs"
            consumer_name = "vector""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "iggy")]
impl SourceConfig for IggySourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        if self.batch_length == 0 {
            return Err("`batch_length` must be at least 1".into());
        }
        if self.commit_interval_secs == 0 {
            return Err("`commit_interval_secs` must be at least 1".into());
        }
        if self.drain_timeout_secs == 0 {
            return Err("`drain_timeout_secs` must be at least 1".into());
        }

        let log_namespace = cx.log_namespace(self.log_namespace);
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);
        let (client, consumer) = self.connect_and_init(acknowledgements).await?;

        Ok(Box::pin(run_iggy_source(
            self.clone(),
            client,
            consumer,
            decoder,
            log_namespace,
            acknowledgements,
            cx.shutdown,
            cx.out,
        )))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let legacy_stream_key = self
            .stream_key_field
            .path
            .clone()
            .map(LegacyKey::InsertIfEmpty);
        let legacy_topic_key = self
            .topic_key_field
            .path
            .clone()
            .map(LegacyKey::InsertIfEmpty);

        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                Self::NAME,
                legacy_stream_key,
                &owned_value_path!("stream"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                legacy_topic_key,
                &owned_value_path!("topic"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                None,
                &owned_value_path!("partition_id"),
                Kind::integer(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                None,
                &owned_value_path!("offset"),
                Kind::integer(),
                None,
            );

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

impl IggySourceConfig {
    async fn connect_and_init(
        &self,
        acknowledgements: bool,
    ) -> Result<(IggyClient, IggyConsumer), BuildError> {
        let client = IggyClientBuilder::from_connection_string(&self.url)
            .context(ConnectSnafu)?
            .build()
            .context(ConnectSnafu)?;

        client.connect().await.context(ConnectSnafu)?;

        let builder = match self.partition {
            Some(partition) => client
                .consumer(&self.consumer_name, &self.stream, &self.topic, partition)
                .context(ConsumerSnafu)?,
            None => client
                .consumer_group(&self.consumer_name, &self.stream, &self.topic)
                .context(ConsumerSnafu)?,
        }
        .batch_length(self.batch_length)
        .poll_interval(IggyDuration::from(Duration::from_millis(
            self.poll_interval_ms,
        )));

        // With end-to-end acknowledgements enabled, disable the SDK's automatic
        // offset committing so that consumer offsets are only stored on the
        // server once the events have been delivered downstream. See
        // `run_iggy_source` for the acknowledgement handling.
        let builder = if acknowledgements {
            builder.auto_commit(AutoCommit::Disabled)
        } else {
            builder
        };

        let mut consumer = builder.build();

        consumer.init().await.context(ConsumerSnafu)?;

        Ok((client, consumer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<IggySourceConfig>();
    }
}
