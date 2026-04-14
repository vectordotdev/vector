//! Configuration functionality for the `AMQP` sink.
use lapin::{BasicProperties, types::ShortString};
use vector_lib::{
    codecs::TextSerializerConfig,
    internal_event::{error_stage, error_type},
};

use super::{channel::AmqpSinkChannels, sink::AmqpSink};
use crate::{amqp::AmqpConfig, sinks::prelude::*};

/// AMQP properties configuration.
#[configurable_component]
#[configurable(title = "Configure the AMQP message properties.")]
#[derive(Clone, Debug, Default)]
pub struct AmqpPropertiesConfig {
    /// Content-Type for the AMQP messages.
    pub(crate) content_type: Option<String>,

    /// Content-Encoding for the AMQP messages.
    pub(crate) content_encoding: Option<String>,

    /// Expiration for AMQP messages (in milliseconds).
    pub(crate) expiration_ms: Option<u64>,

    /// Priority for AMQP messages. It can be templated to an integer between 0 and 255 inclusive.
    pub(crate) priority: Option<UnsignedIntTemplate>,
}

impl AmqpPropertiesConfig {
    pub(super) fn build(&self, event: &Event) -> Option<BasicProperties> {
        let mut prop = BasicProperties::default();
        if let Some(content_type) = &self.content_type {
            prop = prop.with_content_type(ShortString::from(content_type.clone()));
        }
        if let Some(content_encoding) = &self.content_encoding {
            prop = prop.with_content_encoding(ShortString::from(content_encoding.clone()));
        }
        if let Some(expiration_ms) = &self.expiration_ms {
            prop = prop.with_expiration(ShortString::from(expiration_ms.to_string()));
        }
        if let Some(priority_template) = &self.priority {
            let priority = priority_template.render(event).unwrap_or_else(|error| {
                warn!(
                    message = "Failed to render numeric template for \"properties.priority\".",
                    error = %error,
                    error_type = error_type::TEMPLATE_FAILED,
                    stage = error_stage::PROCESSING,
                    internal_log_rate_limit = false,
                );
                Default::default()
            });

            // Clamp the value to the range of 0-255, as AMQP priority is a u8.
            let priority = priority.clamp(0, u8::MAX.into()) as u8;
            prop = prop.with_priority(priority);
        }
        Some(prop)
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

    /// Maximum number of AMQP channels to keep active (channels are created as needed).
    #[serde(default = "default_max_channels")]
    pub(crate) max_channels: u32,
}

const fn default_max_channels() -> u32 {
    4
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
            max_channels: default_max_channels(),
        }
    }
}

impl GenerateConfig for AmqpSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"connection_string = "amqp://localhost:5672/%2f"
            routing_key = "user_id"
            exchange = "test"
            encoding.codec = "json"
            max_channels = 4"#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "amqp")]
impl SinkConfig for AmqpSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = AmqpSink::new(self.clone()).await?;
        let hc = healthcheck(sink.channels.clone()).boxed();
        Ok((VectorSink::from_event_streamsink(sink), hc))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

pub(super) async fn healthcheck(channels: AmqpSinkChannels) -> crate::Result<()> {
    trace!("Healthcheck started.");

    let channel = channels.get().await?;

    if !channel.status().connected() {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "Not Connected",
        )));
    }

    trace!("Healthcheck completed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::format::{Format, deserialize};

    #[test]
    pub fn generate_config() {
        crate::test_util::test_generate_config::<AmqpSinkConfig>();
    }

    fn assert_config_priority_eq(config: AmqpSinkConfig, event: &LogEvent, priority: u8) {
        assert_eq!(
            config
                .properties
                .unwrap()
                .priority
                .unwrap()
                .render(event)
                .unwrap(),
            priority as u64
        );
    }

    #[test]
    pub fn parse_config_priority_static() {
        for (format, config) in [
            (
                Format::Yaml,
                r#"
            exchange: "test"
            routing_key: "user_id"
            encoding:
                codec: "json"
            connection_string: "amqp://user:password@127.0.0.1:5672/"
            properties:
                priority: 1
            "#,
            ),
            (
                Format::Toml,
                r#"
            exchange = "test"
            routing_key = "user_id"
            encoding.codec = "json"
            connection_string = "amqp://user:password@127.0.0.1:5672/"
            properties = { priority = 1 }
            "#,
            ),
            (
                Format::Json,
                r#"
            {
                "exchange": "test",
                "routing_key": "user_id",
                "encoding": {
                    "codec": "json"
                },
                "connection_string": "amqp://user:password@127.0.0.1:5672/",
                "properties": {
                    "priority": 1
                }
            }
            "#,
            ),
        ] {
            let config: AmqpSinkConfig = deserialize(config, format).unwrap();
            let event = LogEvent::from_str_legacy("message");
            assert_config_priority_eq(config, &event, 1);
        }
    }

    #[test]
    pub fn parse_config_priority_templated() {
        for (format, config) in [
            (
                Format::Yaml,
                r#"
            exchange: "test"
            routing_key: "user_id"
            encoding:
                codec: "json"
            connection_string: "amqp://user:password@127.0.0.1:5672/"
            properties:
                priority: "{{ .priority }}"
            "#,
            ),
            (
                Format::Toml,
                r#"
            exchange = "test"
            routing_key = "user_id"
            encoding.codec = "json"
            connection_string = "amqp://user:password@127.0.0.1:5672/"
            properties = { priority = "{{ .priority }}" }
            "#,
            ),
            (
                Format::Json,
                r#"
            {
                "exchange": "test",
                "routing_key": "user_id",
                "encoding": {
                    "codec": "json"
                },
                "connection_string": "amqp://user:password@127.0.0.1:5672/",
                "properties": {
                    "priority": "{{ .priority }}"
                }
            }
            "#,
            ),
        ] {
            let config: AmqpSinkConfig = deserialize(config, format).unwrap();
            let event = {
                let mut event = LogEvent::from_str_legacy("message");
                event.insert("priority", 2);
                event
            };
            assert_config_priority_eq(config, &event, 2);
        }
    }
}
