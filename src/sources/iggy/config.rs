use std::time::Duration;

use iggy::prelude::{
    Client, CommitPolicy, IggyClient, IggyClientBuilder, IggyConsumer, IggyDuration,
    NonZeroIggyDuration, PollOptions,
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
    common::iggy::{login_if_http, secure_connection_string, uses_http},
    config::{
        GenerateConfig, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
    },
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    sources::{Source, iggy::source::run_iggy_source},
};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub(crate) enum BuildError {
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
    /// across members. The HTTP transport keeps no consumer group state, so
    /// `partition` is required with `iggy+http://` URLs.
    #[serde(default)]
    pub partition: Option<u32>,

    /// The maximum number of messages pulled per poll. Defaults to 1000.
    #[serde(default = "default_batch_length")]
    #[derivative(Default(value = "default_batch_length()"))]
    pub batch_length: u32,

    /// The maximum time, in milliseconds, the broker may hold a poll while
    /// no messages are available.
    #[serde(default = "default_poll_interval_ms")]
    #[derivative(Default(value = "default_poll_interval_ms()"))]
    pub poll_interval_ms: u64,

    /// The interval, in seconds, at which consumer offsets are committed to the
    /// Iggy server. Only used when end-to-end acknowledgements are enabled.
    #[serde(default = "default_commit_interval_secs")]
    #[derivative(Default(value = "default_commit_interval_secs()"))]
    pub commit_interval_secs: u64,

    /// The maximum time, in seconds, to wait for in-flight events to be
    /// acknowledged downstream during shutdown before the final consumer
    /// offsets are committed. Only used when end-to-end acknowledgements are
    /// enabled.
    #[serde(default = "default_drain_timeout_secs")]
    #[derivative(Default(value = "default_drain_timeout_secs()"))]
    pub drain_timeout_secs: u64,

    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub framing: FramingConfig,

    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,

    /// The Iggy stream key under which the source stream name is recorded on
    /// each event (Legacy log namespace only).
    #[serde(default = "default_stream_key_field")]
    #[derivative(Default(value = "default_stream_key_field()"))]
    pub stream_key_field: OptionalValuePath,

    /// The Iggy topic key under which the source topic name is recorded on
    /// each event (Legacy log namespace only).
    #[serde(default = "default_topic_key_field")]
    #[derivative(Default(value = "default_topic_key_field()"))]
    pub topic_key_field: OptionalValuePath,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,

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

pub(crate) fn default_stream_key_field() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("stream"))
}

pub(crate) fn default_topic_key_field() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("topic"))
}

impl GenerateConfig for IggySourceConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::json!({
            "url": "iggy+tcp://iggy:iggy@127.0.0.1:8090",
            "stream": "vector",
            "topic": "logs",
            "consumer_name": "vector"
        })
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
        // The SDK's HTTP client has no consumer group state and refuses the
        // join, so reject the combination here instead of failing the source
        // after it connects.
        if self.partition.is_none() && uses_http(&self.url) {
            return Err(
                "`partition` must be set: the Iggy HTTP transport cannot join a consumer group"
                    .into(),
            );
        }

        let log_namespace = cx.log_namespace(self.log_namespace);
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);

        // Connecting and initializing the consumer happens in the source
        // future, not here: sources are built before any sink runs, so a
        // broker that is still starting, or a topic a sink in the same
        // topology creates on its first send, would fail construction with no
        // retry that could resolve it.
        Ok(crate::sources::opaque_source(run_iggy_source(
            self.clone(),
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
    /// With acknowledgements the source commits its own checkpoints. Without
    /// them the server stores a read batch's offset in the same round trip, so
    /// a restart replays at most the batch in flight instead of the SDK's
    /// default 10,000-message auto-commit.
    fn poll_options(&self, acknowledgements: bool) -> PollOptions {
        PollOptions {
            count: self.batch_length,
            commit: if acknowledgements {
                CommitPolicy::Manual
            } else {
                CommitPolicy::OnRead
            },
            ..PollOptions::wait(IggyDuration::from(Duration::from_millis(
                self.poll_interval_ms,
            )))
        }
    }

    pub(super) async fn connect_and_init(
        &self,
        acknowledgements: bool,
    ) -> Result<(IggyClient, IggyConsumer), BuildError> {
        let url = secure_connection_string(&self.url);
        let client = IggyClientBuilder::from_connection_string(&url)
            .context(ConnectSnafu)?
            .build()
            .context(ConnectSnafu)?;

        client.connect().await.context(ConnectSnafu)?;
        login_if_http(&client, &self.url)
            .await
            .context(ConnectSnafu)?;

        let poll_options = self.poll_options(acknowledgements);
        let builder = match self.partition {
            Some(partition) => client
                .consumer(&self.consumer_name, &self.stream, &self.topic)
                .context(ConsumerSnafu)?
                .partitions([partition]),
            None => client
                .consumer_group(&self.consumer_name, &self.stream, &self.topic)
                .context(ConsumerSnafu)?,
        }
        .poll_options(poll_options)
        // Two polls of read-ahead keeps the consumer busy while staying far
        // below the SDK default of 16,000 buffered messages.
        .prefetch_messages(self.batch_length.saturating_mul(2))
        // A source in the same topology as its sink can start before the sink
        // creates the topic. The SDK retries the lookup a few times instead of
        // failing the build, which the topology would only retry with backoff.
        .init_retries(3, NonZeroIggyDuration::ONE_SECOND);

        // A partition reassigned to this consumer can hold offsets the previous
        // owner yielded without committing. The SDK filters them unless replay
        // is allowed, and the acknowledgement path cannot redeliver offsets it
        // never sees.
        let builder = if acknowledgements {
            builder.allow_replay()
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

    #[test]
    fn acknowledgements_choose_who_stores_offsets() {
        let config = IggySourceConfig::default();
        assert_eq!(
            config.poll_options(true).commit,
            CommitPolicy::Manual,
            "the source commits its own checkpoints, so the SDK must not store"
        );
        assert_eq!(
            config.poll_options(false).commit,
            CommitPolicy::OnRead,
            "without acknowledgements the server stores each read batch"
        );
    }
}
