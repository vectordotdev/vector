use crate::{
    amqp::AMQPConfig,
    codecs::EncodingConfig,
    config::{DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::{Healthcheck, VectorSink},
};
use codecs::TextSerializerConfig;
use futures::FutureExt;
use std::sync::Arc;
use vector_config::configurable_component;
use vector_core::config::AcknowledgementsConfig;

use super::sink::AMQPSink;

/// Configuration for the `amqp` sink.
///
/// Supports AMQP version 0.9.1
#[configurable_component(sink("amqp"))]
#[derive(Clone, Debug)]
pub struct AMQPSinkConfig {
    /// The exchange to publish messages to.
    pub(crate) exchange: String,

    /// Template use to generate a routing key which corresponds to a queue binding.
    // TODO: We will eventually be able to add metadata on a per-field basis, such that we can add metadata for marking
    // this field as being capable of using Vector's templating syntax.
    pub(crate) routing_key: Option<String>,

    /// Connection options for AMQP sink
    pub(crate) connection: AMQPConfig,

    #[configurable(derived)]
    pub(crate) encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub(crate) acknowledgements: AcknowledgementsConfig,
}

impl Default for AMQPSinkConfig {
    fn default() -> Self {
        Self {
            exchange: "vector".to_string(),
            routing_key: None,
            encoding: TextSerializerConfig::new().into(),
            connection: AMQPConfig::default(),
            acknowledgements: AcknowledgementsConfig::default(),
        }
    }
}

impl GenerateConfig for AMQPSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"connection.connection_string = "amqp://localhost:5672/%2f"
            routing_key = "user_id"
            exchange = "test"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for AMQPSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = AMQPSink::new(self.clone()).await?;
        let hc = healthcheck(Arc::clone(&sink.channel)).boxed();
        Ok((VectorSink::from_event_streamsink(sink), hc))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

pub(super) async fn healthcheck(channel: Arc<lapin::Channel>) -> crate::Result<()> {
    trace!("Healthcheck started.");

    if !channel.status().connected() {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "Not Connected",
        )));
    }

    trace!("Healthcheck completed.");
    Ok(())
}
