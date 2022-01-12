use std::collections::HashMap;

use futures::FutureExt;
use rdkafka::ClientConfig;
use serde::{Deserialize, Serialize};

use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext},
    kafka::{KafkaAuthConfig, KafkaCompression},
    serde::to_string,
    sinks::{
        kafka::sink::{healthcheck, KafkaSink},
        util::{
            encoding::{EncodingConfig, StandardEncodings},
            BatchConfig, NoDefaultsBatchSettings,
        },
        Healthcheck, VectorSink,
    },
};

pub(crate) const QUEUED_MIN_MESSAGES: u64 = 100000;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct KafkaSinkConfig {
    pub bootstrap_servers: String,
    pub topic: String,
    pub key_field: Option<String>,
    pub encoding: EncodingConfig<StandardEncodings>,
    /// These batching options will **not** override librdkafka_options values.
    #[serde(default)]
    pub batch: BatchConfig<NoDefaultsBatchSettings>,
    #[serde(default)]
    pub compression: KafkaCompression,
    #[serde(flatten)]
    pub auth: KafkaAuthConfig,
    #[serde(default = "default_socket_timeout_ms")]
    pub socket_timeout_ms: u64,
    #[serde(default = "default_message_timeout_ms")]
    pub message_timeout_ms: u64,
    #[serde(default)]
    pub librdkafka_options: HashMap<String, String>,
    #[serde(alias = "headers_field")] // accidentally released as `headers_field` in 0.18
    pub headers_key: Option<String>,
}

const fn default_socket_timeout_ms() -> u64 {
    60000 // default in librdkafka
}

const fn default_message_timeout_ms() -> u64 {
    300000 // default in librdkafka
}

/// Used to determine the options to set in configs, since both Kafka consumers and producers have
/// unique options, they use the same struct, and the error if given the wrong options.
#[derive(Debug, PartialOrd, PartialEq)]
pub enum KafkaRole {
    Consumer,
    Producer,
}

impl KafkaSinkConfig {
    pub(crate) fn to_rdkafka(&self, kafka_role: KafkaRole) -> crate::Result<ClientConfig> {
        let mut client_config = ClientConfig::new();
        client_config
            .set("bootstrap.servers", &self.bootstrap_servers)
            .set("socket.timeout.ms", &self.socket_timeout_ms.to_string())
            .set("statistics.interval.ms", "1000");

        self.auth.apply(&mut client_config)?;

        match kafka_role {
            // All batch options are producer only.
            KafkaRole::Producer => {
                client_config
                    .set("compression.codec", &to_string(self.compression))
                    .set("message.timeout.ms", &self.message_timeout_ms.to_string());

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
                    client_config.set(key, &(value * 1000).to_string());
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
                    client_config.set(key, &value.to_string());
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
                    client_config.set(key, &value.to_string());
                }
            }

            KafkaRole::Consumer => {
                client_config.set("queued.min.messages", QUEUED_MIN_MESSAGES.to_string());
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
            topic: "topic-1234".to_owned(),
            key_field: Some("user_id".to_owned()),
            encoding: StandardEncodings::Json.into(),
            batch: Default::default(),
            compression: KafkaCompression::None,
            auth: Default::default(),
            socket_timeout_ms: default_socket_timeout_ms(),
            message_timeout_ms: default_message_timeout_ms(),
            librdkafka_options: Default::default(),
            headers_key: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "kafka")]
impl SinkConfig for KafkaSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = KafkaSink::new(self.clone(), cx.acker())?;
        let hc = healthcheck(self.clone()).boxed();
        Ok((VectorSink::from_event_streamsink(sink), hc))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "kafka"
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
