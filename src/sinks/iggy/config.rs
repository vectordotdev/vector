//! Configuration for the `iggy` sink.

use std::time::Duration;

use serde_with::serde_as;
use vector_lib::configurable::configurable_component;

use crate::{
    config::{AcknowledgementsConfig, DataType, Input, SinkConfig, SinkContext},
    sinks::{Healthcheck, VectorSink},
};

use super::sink::IggySink;

/// Default maximum encoded message size (Iggy's own hard limit is 64 MB).
const fn default_max_message_bytes() -> usize {
    16 * 1024 * 1024
}

const fn default_partitions() -> u32 {
    8
}

fn default_stream() -> String {
    "obstack".to_string()
}

fn default_topic_prefix() -> String {
    super::proto::DEFAULT_TOPIC_PREFIX.to_string()
}

fn default_tenant_attribute() -> String {
    "obstack.tenant.id".to_string()
}

const fn default_batch_events() -> usize {
    8_192
}

const fn default_batch_bytes() -> usize {
    8 * 1024 * 1024
}

const fn default_batch_timeout() -> Duration {
    Duration::from_millis(200)
}

const fn default_replication_factor() -> u8 {
    1
}

const fn default_max_active_topics() -> usize {
    1_000
}

const fn default_bootstrap_timeout() -> Duration {
    Duration::from_secs(10)
}

/// Publish OTLP telemetry to an Obstack storage cluster through Apache Iggy.
///
/// This sink is the producer half of Obstack's queue-only ingest path: it
/// converts OTLP events (from an `opentelemetry` source configured with
/// `use_otlp_decoding: true`) into Obstack's partitioned, generation-stamped
/// queue envelopes and durably appends them to a topic per `(tenant,
/// producer)`. It replaces the former Collector adapter with one strict,
/// retry-aware producer boundary.
#[serde_as]
#[configurable_component(sink(
    "iggy",
    "Publish OTLP telemetry to an Obstack cluster through Apache Iggy."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct IggySinkConfig {
    /// Iggy connection string, e.g. `iggy://user:pass@host:8090`.
    #[configurable(metadata(docs::examples = "iggy://iggy:iggy@127.0.0.1:8090"))]
    pub connection_string: String,

    /// Iggy stream name. The sink creates it when absent.
    #[serde(default = "default_stream")]
    #[configurable(metadata(docs::examples = "obstack"))]
    pub stream: String,

    /// Prefix for deterministic tenant/producer topic names.
    #[serde(default = "default_topic_prefix")]
    #[configurable(metadata(docs::examples = "obstack-p-"))]
    pub topic_prefix: String,

    /// Partitions per producer topic. Obstack requires exactly eight.
    #[serde(default = "default_partitions")]
    pub partitions: u32,

    /// Required string resource attribute containing the tenant.
    #[serde(default = "default_tenant_attribute")]
    #[configurable(metadata(docs::examples = "obstack.tenant.id"))]
    pub tenant_attribute: String,

    /// Iggy replication factor used when creating producer topics.
    #[serde(default = "default_replication_factor")]
    pub replication_factor: u8,

    /// Maximum producer topics cached by this sink instance.
    #[serde(default = "default_max_active_topics")]
    pub max_active_topics: usize,

    /// How many seconds a provisioning loser waits for the topic creator's bootstrap.
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(default = "default_bootstrap_timeout")]
    pub bootstrap_timeout: Duration,

    /// Maximum encoded envelope size before a batch is split. Capped at
    /// Iggy's 64 MB limit.
    #[serde(default = "default_max_message_bytes")]
    pub max_message_bytes: usize,

    /// Number of shared Iggy connection lanes (defaults to 16).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lanes: Option<usize>,

    /// Maximum number of source events coalesced into one publish pass.
    #[serde(default = "default_batch_events")]
    pub batch_events: usize,

    /// Approximate maximum source bytes coalesced into one publish pass.
    #[serde(default = "default_batch_bytes")]
    pub batch_bytes: usize,

    /// Maximum milliseconds to wait for a partially full publish pass.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[serde(default = "default_batch_timeout")]
    pub batch_timeout: Duration,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl_generate_config_from_default!(IggySinkConfig);

impl Default for IggySinkConfig {
    fn default() -> Self {
        Self {
            connection_string: "iggy://iggy:iggy@127.0.0.1:8090".to_string(),
            stream: default_stream(),
            topic_prefix: default_topic_prefix(),
            partitions: default_partitions(),
            tenant_attribute: default_tenant_attribute(),
            replication_factor: default_replication_factor(),
            max_active_topics: default_max_active_topics(),
            bootstrap_timeout: default_bootstrap_timeout(),
            max_message_bytes: default_max_message_bytes(),
            lanes: None,
            batch_events: default_batch_events(),
            batch_bytes: default_batch_bytes(),
            batch_timeout: default_batch_timeout(),
            acknowledgements: AcknowledgementsConfig::default(),
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "iggy")]
impl SinkConfig for IggySinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = IggySink::connect(self.clone()).await?;
        let healthcheck = futures::future::ok::<(), crate::Error>(()).boxed();
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        // OTLP-decoding logs/metrics arrive as Log events, traces as Trace.
        Input::new(DataType::Log | DataType::Trace)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

use futures::FutureExt;
