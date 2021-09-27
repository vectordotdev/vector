use crate::config::{DataType, GenerateConfig, SinkConfig, SinkContext};
use crate::kafka::{KafkaAuthConfig, KafkaCompression, KafkaStatisticsContext};
use crate::serde::to_string;
use crate::sinks::kafka::sink::KafkaSink;
use crate::sinks::util::encoding::{
    Encoder, EncodingConfig, DEFAULT_JSON_ENCODER, DEFAULT_TEXT_ENCODER,
};
use crate::sinks::util::BatchConfig;
use crate::sinks::{Healthcheck, VectorSink};
use crate::template::Template;
use crate::template::TemplateParseError;
use futures::FutureExt;
use rdkafka::consumer::{BaseConsumer, Consumer};
use rdkafka::error::KafkaError;
use rdkafka::producer::FutureProducer;
use rdkafka::ClientConfig;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::Write;
use tokio::time::Duration;
use vector_core::event::Event;

pub(crate) const QUEUED_MIN_MESSAGES: u64 = 100000;

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("creating kafka producer failed: {}", source))]
    KafkaCreateFailed { source: KafkaError },
    #[snafu(display("invalid topic template: {}", source))]
    TopicTemplate { source: TemplateParseError },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct KafkaSinkConfig {
    pub bootstrap_servers: String,
    pub topic: String,
    pub key_field: Option<String>,
    pub encoding: EncodingConfig<Encoding>,
    /// These batching options will **not** override librdkafka_options values.
    #[serde(default)]
    pub batch: BatchConfig,
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
    pub headers_field: Option<String>,
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
            .set("compression.codec", &to_string(self.compression))
            .set("socket.timeout.ms", &self.socket_timeout_ms.to_string())
            .set("message.timeout.ms", &self.message_timeout_ms.to_string())
            .set("statistics.interval.ms", "1000")
            .set("queued.min.messages", QUEUED_MIN_MESSAGES.to_string());

        self.auth.apply(&mut client_config)?;

        // All batch options are producer only.
        if kafka_role == KafkaRole::Producer {
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

        for (key, value) in self.librdkafka_options.iter() {
            debug!(option = %key, value = %value, "Setting librdkafka option.");
            client_config.set(key.as_str(), value.as_str());
        }

        Ok(client_config)
    }
}

pub fn create_producer(
    client_config: ClientConfig,
) -> crate::Result<FutureProducer<KafkaStatisticsContext>> {
    let producer = client_config
        .create_with_context(KafkaStatisticsContext)
        .context(KafkaCreateFailed)?;
    Ok(producer)
}

impl GenerateConfig for KafkaSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            bootstrap_servers: "10.14.22.123:9092,10.14.23.332:9092".to_owned(),
            topic: "topic-1234".to_owned(),
            key_field: Some("user_id".to_owned()),
            encoding: Encoding::Json.into(),
            batch: Default::default(),
            compression: KafkaCompression::None,
            auth: Default::default(),
            socket_timeout_ms: default_socket_timeout_ms(),
            message_timeout_ms: default_message_timeout_ms(),
            librdkafka_options: Default::default(),
            headers_field: None,
        })
        .unwrap()
    }
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

impl Encoder for Encoding {
    fn encode_event(&self, event: Event, writer: &mut dyn Write) -> std::io::Result<()> {
        match self {
            Encoding::Json => DEFAULT_JSON_ENCODER.encode_event(event, writer),
            Encoding::Text => match event {
                Event::Log(log) => DEFAULT_TEXT_ENCODER.encode_event(Event::Log(log), writer),
                Event::Metric(metric) => {
                    metric.to_string().into_bytes();
                    Ok(())
                }
            },
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "kafka")]
impl SinkConfig for KafkaSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = KafkaSink::new(self.clone(), cx.acker())?;
        let hc = healthcheck(self.clone()).boxed();
        Ok((VectorSink::Stream(Box::new(sink)), hc))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "kafka"
    }
}

async fn healthcheck(config: KafkaSinkConfig) -> crate::Result<()> {
    trace!("Healthcheck started.");
    let client = config.to_rdkafka(KafkaRole::Consumer).unwrap();
    let topic = match Template::try_from(config.topic)
        .context(TopicTemplate)?
        .render_string(&Event::from(""))
    {
        Ok(topic) => Some(topic),
        Err(error) => {
            warn!(
                message = "Could not generate topic for healthcheck.",
                %error,
            );
            None
        }
    };

    tokio::task::spawn_blocking(move || {
        let consumer: BaseConsumer = client.create().unwrap();
        let topic = topic.as_ref().map(|topic| &topic[..]);

        consumer
            .fetch_metadata(topic, Duration::from_secs(3))
            .map(|_| ())
    })
    .await??;
    trace!("Healthcheck completed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        KafkaSinkConfig::generate_config();
    }
}
