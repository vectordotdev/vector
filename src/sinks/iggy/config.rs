//! Configuration for the `iggy` sink.

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

const fn default_shards() -> u32 {
    8
}

fn default_stream() -> String {
    "obstack".to_string()
}

fn default_topic() -> String {
    "telemetry".to_string()
}

fn default_tenant() -> String {
    "default".to_string()
}

fn default_tenant_attribute() -> String {
    "obstack.tenant.id".to_string()
}

const fn default_batch_events() -> usize {
    1024
}

/// Publish OTLP telemetry to an Obstack storage cluster through Apache Iggy.
///
/// This sink is the producer half of Obstack's queue-only ingest path: it
/// converts OTLP events (from an `opentelemetry` source configured with
/// `use_otlp_decoding: true`) into Obstack's sharded, generation-stamped v3
/// queue envelopes and durably appends them to the configured Iggy topic. It
/// replaces the reference OpenTelemetry Collector plus the `obstack-otel-iggy`
/// adapter with a single Vector component.
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

    /// Iggy stream name. Must already exist (provisioned by `obstack-iggy-init`).
    #[serde(default = "default_stream")]
    #[configurable(metadata(docs::examples = "obstack"))]
    pub stream: String,

    /// Iggy topic name. Must exist with exactly `shards` partitions.
    #[serde(default = "default_topic")]
    #[configurable(metadata(docs::examples = "telemetry"))]
    pub topic: String,

    /// Storage shard count. Must equal the topic's partition count and the
    /// Obstack storage root's pinned `OBSTACK_SHARDS`.
    #[serde(default = "default_shards")]
    pub shards: u32,

    /// Tenant applied to telemetry that does not carry a tenant resource
    /// attribute.
    #[serde(default = "default_tenant")]
    #[configurable(metadata(docs::examples = "default", docs::examples = "team-a"))]
    pub tenant: String,

    /// Resource attribute read (per OTLP resource) to override `tenant`.
    #[serde(default = "default_tenant_attribute")]
    #[configurable(metadata(docs::examples = "obstack.tenant.id"))]
    pub tenant_attribute: String,

    /// Maximum encoded envelope size before a batch is split. Capped at
    /// Iggy's 64 MB limit.
    #[serde(default = "default_max_message_bytes")]
    pub max_message_bytes: usize,

    /// Number of Iggy connection lanes (defaults to `min(shards, 16)`), so
    /// independent shards append and fsync concurrently.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lanes: Option<usize>,

    /// Maximum number of source events coalesced into one publish pass.
    #[serde(default = "default_batch_events")]
    pub batch_events: usize,

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
            topic: default_topic(),
            shards: default_shards(),
            tenant: default_tenant(),
            tenant_attribute: default_tenant_attribute(),
            max_message_bytes: default_max_message_bytes(),
            lanes: None,
            batch_events: default_batch_events(),
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
