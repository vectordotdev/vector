use std::{collections::HashMap, time::Duration};

use futures::FutureExt;
use rdkafka::ClientConfig;
use serde_with::serde_as;
use vector_lib::codecs::JsonSerializerConfig;
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::lookup_v2::ConfigTargetPath;
use vrl::value::Kind;

use crate::{
    kafka::{KafkaAuthConfig, KafkaCompression},
    serde::json::to_string,
    sinks::{
        kafka::sink::{healthcheck, KafkaSink},
        prelude::*,
    },
};

/// Configuration for the `kafka` sink.
#[serde_as]
#[configurable_component(sink(
    "kafka",
    "Publish observability event data to Apache Kafka topics."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct KafkaSinkConfig {
    /// A comma-separated list of Kafka bootstrap servers.
    ///
    /// These are the servers in a Kafka cluster that a client should use to bootstrap its
    /// connection to the cluster, allowing discovery of all the other hosts in the cluster.
    ///
    /// Must be in the form of `host:port`, and comma-separated.
    #[configurable(metadata(docs::examples = "10.14.22.123:9092,10.14.23.332:9092"))]
    pub bootstrap_servers: String,

    /// The Kafka topic name to write events to.
    #[configurable(metadata(docs::templateable))]
    #[configurable(metadata(
        docs::examples = "topic-1234",
        docs::examples = "logs-{{unit}}-%Y-%m-%d"
    ))]
    pub topic: Template,

    /// The topic name to use for healthcheck. If omitted, `topic` is used.
    /// This option helps prevent healthcheck warnings when `topic` is templated.
    ///
    /// It is ignored when healthcheck is disabled.
    pub healthcheck_topic: Option<String>,

    /// The log field name or tag key to use for the topic key.
    ///
    /// If the field does not exist in the log or in the tags, a blank value is used. If
    /// unspecified, the key is not sent.
    ///
    /// Kafka uses a hash of the key to choose the partition or uses round-robin if the record has
    /// no key.
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::examples = "user_id"))]
    #[configurable(metadata(docs::examples = ".my_topic"))]
    #[configurable(metadata(docs::examples = "%my_topic"))]
    pub key_field: Option<ConfigTargetPath>,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    // These batching options will **not** override librdkafka_options values.
    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    #[serde(default)]
    pub batch: BatchConfig<NoDefaultsBatchSettings>,

    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    #[serde(default)]
    pub compression: KafkaCompression,

    #[configurable(derived)]
    #[serde(flatten)]
    pub auth: KafkaAuthConfig,

    /// Default timeout, in milliseconds, for network requests.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[serde(default = "default_socket_timeout_ms")]
    #[configurable(metadata(docs::examples = 30000, docs::examples = 60000))]
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::human_name = "Socket Timeout"))]
    pub socket_timeout_ms: Duration,

    /// Local message timeout, in milliseconds.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::examples = 150000, docs::examples = 450000))]
    #[serde(default = "default_message_timeout_ms")]
    #[configurable(metadata(docs::human_name = "Message Timeout"))]
    #[configurable(metadata(docs::advanced))]
    pub message_timeout_ms: Duration,

    /// A map of advanced options to pass directly to the underlying `librdkafka` client.
    ///
    /// For more information on configuration options, see [Configuration properties][config_props_docs].
    ///
    /// [config_props_docs]: https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md
    #[serde(default)]
    #[configurable(metadata(docs::examples = "example_librdkafka_options()"))]
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(
        docs::additional_props_description = "A librdkafka configuration option."
    ))]
    pub librdkafka_options: HashMap<String, String>,

    /// The log field name to use for the Kafka headers.
    ///
    /// If omitted, no headers are written.
    #[configurable(metadata(docs::advanced))]
    #[serde(alias = "headers_field")] // accidentally released as `headers_field` in 0.18
    #[configurable(metadata(docs::examples = "headers"))]
    pub headers_key: Option<ConfigTargetPath>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

const fn default_socket_timeout_ms() -> Duration {
    Duration::from_millis(60000) // default in librdkafka
}

const fn default_message_timeout_ms() -> Duration {
    Duration::from_millis(300000) // default in librdkafka
}

fn example_librdkafka_options() -> HashMap<String, String> {
    HashMap::<_, _>::from_iter([
        ("client.id".to_string(), "${ENV_VAR}".to_string()),
        ("fetch.error.backoff.ms".to_string(), "1000".to_string()),
        ("socket.send.buffer.bytes".to_string(), "100".to_string()),
    ])
}

/// Used to determine the options to set in configs, since both Kafka consumers and producers have
/// unique options, they use the same struct, and the error if given the wrong options.
#[derive(Debug, PartialOrd, PartialEq, Eq)]
pub enum KafkaRole {
    Consumer,
    Producer,
}

impl KafkaSinkConfig {
    pub(crate) fn to_rdkafka(&self, kafka_role: KafkaRole) -> crate::Result<ClientConfig> {
        let mut client_config = ClientConfig::new();
        client_config
            .set("bootstrap.servers", &self.bootstrap_servers)
            .set(
                "socket.timeout.ms",
                self.socket_timeout_ms.as_millis().to_string(),
            )
            .set("statistics.interval.ms", "1000");

        self.auth.apply(&mut client_config)?;

        // All batch options are producer only.
        if kafka_role == KafkaRole::Producer {
            client_config
                .set("compression.codec", to_string(self.compression))
                .set(
                    "message.timeout.ms",
                    self.message_timeout_ms.as_millis().to_string(),
                );

            if let Some(value) = self.batch.timeout_secs {
                // Delay in milliseconds to wait for messages in the producer queue to accumulate before
                // constructing message batches (MessageSets) to transmit to brokers. A higher value
                // allows larger and more effective (less overhead, improved compression) batches of
                // messages to accumulate at the expense of increased message delivery latency.
                // Type: float
                let key = "queue.buffering.max.ms";
                if let Some(val) = self.librdkafka_options.get(key) {
                    return Err(format!("Batching setting `batch.timeout_secs` sets `librdkafka_options.{}={}`.\
                                        The config already sets this as `librdkafka_options.queue.buffering.max.ms={}`.\
                                        Please delete one.", key, value, val).into());
                }
                debug!(
                    librdkafka_option = key,
                    batch_option = "timeout_secs",
                    value,
                    "Applying batch option as librdkafka option."
                );
                client_config.set(key, (value * 1000.0).round().to_string());
            }
            if let Some(value) = self.batch.max_events {
                // Maximum number of messages batched in one MessageSet. The total MessageSet size is
                // also limited by batch.size and message.max.bytes.
                // Type: integer
                let key = "batch.num.messages";
                if let Some(val) = self.librdkafka_options.get(key) {
                    return Err(format!("Batching setting `batch.max_events` sets `librdkafka_options.{}={}`.\
                                        The config already sets this as `librdkafka_options.batch.num.messages={}`.\
                                        Please delete one.", key, value, val).into());
                }
                debug!(
                    librdkafka_option = key,
                    batch_option = "max_events",
                    value,
                    "Applying batch option as librdkafka option."
                );
                client_config.set(key, value.to_string());
            }
            if let Some(value) = self.batch.max_bytes {
                // Maximum size (in bytes) of all messages batched in one MessageSet, including protocol
                // framing overhead. This limit is applied after the first message has been added to the
                // batch, regardless of the first message's size, this is to ensure that messages that
                // exceed batch.size are produced. The total MessageSet size is also limited by
                // batch.num.messages and message.max.bytes.
                // Type: integer
                let key = "batch.size";
                if let Some(val) = self.librdkafka_options.get(key) {
                    return Err(format!("Batching setting `batch.max_bytes` sets `librdkafka_options.{}={}`.\
                                        The config already sets this as `librdkafka_options.batch.size={}`.\
                                        Please delete one.", key, value, val).into());
                }
                debug!(
                    librdkafka_option = key,
                    batch_option = "max_bytes",
                    value,
                    "Applying batch option as librdkafka option."
                );
                client_config.set(key, value.to_string());
            }
        }

        for (key, value) in self.librdkafka_options.iter() {
            debug!(option = %key, value = %value, "Setting librdkafka option.");
            client_config.set(key.as_str(), value.as_str());
        }

        Ok(client_config)
    }
}

impl GenerateConfig for KafkaSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            bootstrap_servers: "10.14.22.123:9092,10.14.23.332:9092".to_owned(),
            topic: Template::try_from("topic-1234".to_owned()).unwrap(),
            healthcheck_topic: None,
            key_field: Some(ConfigTargetPath::try_from("user_id".to_owned()).unwrap()),
            encoding: JsonSerializerConfig::default().into(),
            batch: Default::default(),
            compression: KafkaCompression::None,
            auth: Default::default(),
            socket_timeout_ms: default_socket_timeout_ms(),
            message_timeout_ms: default_message_timeout_ms(),
            librdkafka_options: Default::default(),
            headers_key: None,
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "kafka")]
impl SinkConfig for KafkaSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = KafkaSink::new(self.clone())?;
        let hc = healthcheck(self.clone()).boxed();
        Ok((VectorSink::from_event_streamsink(sink), hc))
    }

    fn input(&self) -> Input {
        let requirements = Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::new(self.encoding.config().input_type() & (DataType::Log | DataType::Metric))
            .with_schema_requirement(requirements)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        KafkaSinkConfig::generate_config();
    }
}
