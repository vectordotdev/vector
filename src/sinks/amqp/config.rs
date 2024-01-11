//! Configuration functionality for the `AMQP` sink.
use crate::{amqp::AmqpConfig, sinks::prelude::*};
use lapin::{types::ShortString, BasicProperties};
use std::sync::Arc;
use vector_lib::codecs::TextSerializerConfig;

use super::sink::AmqpSink;

/// AMQP properties configuration.
#[configurable_component]
#[configurable(title = "Configure the AMQP message properties.")]
#[derive(Clone, Debug, Default)]
pub struct AmqpPropertiesConfig {
    /// Content-Type for the AMQP messages.
    #[configurable(derived)]
    pub(crate) content_type: Option<String>,

    /// Content-Encoding for the AMQP messages.
    #[configurable(derived)]
    pub(crate) content_encoding: Option<String>,
}

impl AmqpPropertiesConfig {
    pub(super) fn build(&self) -> BasicProperties {
        let mut prop = BasicProperties::default();
        if let Some(content_type) = &self.content_type {
            prop = prop.with_content_type(ShortString::from(content_type.clone()));
        }
        if let Some(content_encoding) = &self.content_encoding {
            prop = prop.with_content_encoding(ShortString::from(content_encoding.clone()));
        }
        prop
    }
}

/// Configuration for the `amqp` sink.
///
/// Supports AMQP version 0.9.1
#[configurable_component(sink(
    "amqp",
    "Send events to AMQP 0.9.1 compatible brokers like RabbitMQ."
))]
#[derive(Clone, Debug)]
pub struct AmqpSinkConfig {
    /// The exchange to publish messages to.
    pub(crate) exchange: Template,

    /// Template used to generate a routing key which corresponds to a queue binding.
    pub(crate) routing_key: Option<Template>,

    /// AMQP message properties.
    pub(crate) properties: Option<AmqpPropertiesConfig>,

    #[serde(flatten)]
    pub(crate) connection: AmqpConfig,

    #[configurable(derived)]
    pub(crate) encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub(crate) acknowledgements: AcknowledgementsConfig,
}

impl Default for AmqpSinkConfig {
    fn default() -> Self {
        Self {
            exchange: Template::try_from("vector").unwrap(),
            routing_key: None,
            properties: None,
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
#[typetag::serde(name = "amqp")]
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
