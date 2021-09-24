use serde::{Serialize, Deserialize};
use crate::sinks::util::encoding::EncodingConfig;
use crate::sinks::util::BatchConfig;
use crate::kafka::{KafkaCompression, KafkaAuthConfig};
use std::collections::HashMap;
use crate::config::{GenerateConfig, SinkConfig, SinkContext, DataType};
use crate::sinks::{VectorSink, Healthcheck};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KafkaSinkConfig {
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
    pub headers_key: Option<String>,
}

const fn default_socket_timeout_ms() -> u64 {
    60000 // default in librdkafka
}

const fn default_message_timeout_ms() -> u64 {
    300000 // default in librdkafka
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
            headers_key: None
        }).unwrap()
    }
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}


#[async_trait::async_trait]
#[typetag::serde(name = "kafka")]
impl SinkConfig for KafkaSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        todo!()
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
