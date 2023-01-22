//! Configuration functionality for the `AMQP` sink.
use crate::{
    amqp::AmqpConfig,
    codecs::EncodingConfig,
    config::{DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::{Healthcheck, VectorSink},
    template::Template,
};
use codecs::TextSerializerConfig;
use futures::FutureExt;
use std::sync::Arc;
use vector_config::configurable_component;
use vector_core::config::AcknowledgementsConfig;

use super::sink::AmqpSink;

/// Configuration for the `amqp` sink.
///
/// Supports AMQP version 0.9.1
#[configurable_component(sink("amqp"))]
#[derive(Clone, Debug)]
pub struct AmqpSinkConfig {
    /// The exchange to publish messages to.
    pub(crate) exchange: Template,

    /// Template used to generate a routing key which corresponds to a queue binding.
    pub(crate) routing_key: Option<Template>,

    #[serde(flatten)]
    pub(crate) connection: AmqpConfig,

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

impl Default for AmqpSinkConfig {
    fn default() -> Self {
        Self {
            exchange: Template::try_from("vector").unwrap(),
            routing_key: None,
            encoding: TextSerializerConfig::default().into(),
            connection: AmqpConfig::default(),
            acknowledgements: AcknowledgementsConfig::default(),
        }
    }
}

impl GenerateConfig for AmqpSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"connection_string = "amqp://localhost:5672/%2f"
            routing_key = "user_id"
            exchange = "test"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for AmqpSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = AmqpSink::new(self.clone()).await?;
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

#[test]
pub fn generate_config() {
    crate::test_util::test_generate_config::<AmqpSinkConfig>();
}
