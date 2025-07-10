use bytes::{BufMut, BytesMut};
use syslog::{Facility, Formatter3164, LogFormat, Severity};
use vector_lib::configurable::configurable_component;
use vrl::value::Kind;

use crate::{
    codecs::{Encoder, EncodingConfig, Transformer},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    event::Event,
    internal_events::TemplateRenderingError,
    schema,
    sinks::util::{tcp::TcpSinkConfig, UriSerde},
    tcp::TcpKeepaliveConfig,
    template::Template,
    tls::TlsEnableableConfig,
};

/// Configuration for the `papertrail` sink.
#[configurable_component(sink("papertrail", "Deliver log events to Papertrail from SolarWinds."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PapertrailConfig {
    /// The TCP endpoint to send logs to.
    #[configurable(metadata(docs::examples = "logs.papertrailapp.com:12345"))]
    endpoint: UriSerde,

    #[configurable(derived)]
    encoding: EncodingConfig,

    #[configurable(derived)]
    keepalive: Option<TcpKeepaliveConfig>,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    /// Configures the send buffer size using the `SO_SNDBUF` option on the socket.
    send_buffer_bytes: Option<usize>,

    /// The value to use as the `process` in Papertrail.
    #[configurable(metadata(docs::examples = "{{ process }}", docs::examples = "my-process",))]
    #[serde(default = "default_process")]
    process: Template,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

fn default_process() -> Template {
    Template::try_from("vector").unwrap()
}

impl GenerateConfig for PapertrailConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"endpoint = "logs.papertrailapp.com:12345"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "papertrail")]
impl SinkConfig for PapertrailConfig {
    async fn build(
        &self,
        _cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let host = self
            .endpoint
            .uri
            .host()
            .map(str::to_string)
            .ok_or_else(|| "A host is required for endpoint".to_string())?;
        let port = self
            .endpoint
            .uri
            .port_u16()
            .ok_or_else(|| "A port is required for endpoint".to_string())?;

        let address = format!("{}:{}", host, port);
        let tls = Some(
            self.tls
                .clone()
                .unwrap_or_else(TlsEnableableConfig::enabled),
        );

        let pid = std::process::id();
        let process = self.process.clone();

        let sink_config = TcpSinkConfig::new(address, self.keepalive, tls, self.send_buffer_bytes);

        let transformer = self.encoding.transformer();
        let serializer = self.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);

        sink_config.build(
            Transformer::default(),
            PapertrailEncoder {
                pid,
                process,
                transformer,
                encoder,
            },
        )
    }

    fn input(&self) -> Input {
        let requirement = schema::Requirement::empty().optional_meaning("host", Kind::bytes());

        Input::new(self.encoding.config().input_type() & DataType::Log)
            .with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Debug, Clone)]
struct PapertrailEncoder {
    pid: u32,
    process: Template,
    transformer: Transformer,
    encoder: Encoder<()>,
}

impl tokio_util::codec::Encoder<Event> for PapertrailEncoder {
    type Error = vector_lib::codecs::encoding::Error;

    fn encode(
        &mut self,
        mut event: Event,
        buffer: &mut bytes::BytesMut,
    ) -> Result<(), Self::Error> {
        let host = event
            .as_mut_log()
            .get_host()
            .map(|host| host.to_string_lossy().into_owned());

        let process = self
            .process
            .render_string(&event)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("process"),
                    drop_event: false,
                })
            })
            .ok()
            .unwrap_or_else(|| String::from("vector"));

        let formatter = Formatter3164 {
            facility: Facility::LOG_USER,
            hostname: host,
            process,
            pid: self.pid,
        };

        self.transformer.transform(&mut event);

        let mut bytes = BytesMut::new();
        self.encoder.encode(event, &mut bytes)?;

        let message = String::from_utf8_lossy(&bytes);

        formatter
            .format(&mut buffer.writer(), Severity::LOG_INFO, message)
            .map_err(|error| Self::Error::SerializingError(format!("{}", error).into()))?;

        buffer.put_u8(b'\n');

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use std::convert::TryFrom;

    use bytes::BytesMut;
    use futures::{future::ready, stream};
    use tokio_util::codec::Encoder as _;
    use vector_lib::codecs::JsonSerializerConfig;
    use vector_lib::event::{Event, LogEvent};

    use crate::test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        http::{always_200_response, spawn_blackhole_http_server},
    };

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PapertrailConfig>();
    }

    #[tokio::test]
    async fn component_spec_compliance() {
        let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

        let config = PapertrailConfig::generate_config().to_string();
        let mut config = PapertrailConfig::deserialize(toml::de::ValueDeserializer::new(&config))
            .expect("config should be valid");
        config.endpoint = mock_endpoint.into();
        config.tls = Some(TlsEnableableConfig::default());

        let context = SinkContext::default();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let event = Event::Log(LogEvent::from("simple message"));
        run_and_assert_sink_compliance(sink, stream::once(ready(event)), &SINK_TAGS).await;
    }

    #[test]
    fn encode_event_apply_rules() {
        let mut evt = Event::Log(LogEvent::from("vector"));
        evt.as_mut_log().insert("magic", "key");
        evt.as_mut_log().insert("process", "foo");

        let mut encoder = PapertrailEncoder {
            pid: 0,
            process: Template::try_from("{{ process }}").unwrap(),
            transformer: Transformer::new(None, Some(vec!["magic".into()]), None).unwrap(),
            encoder: Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
        };

        let mut bytes = BytesMut::new();
        encoder.encode(evt, &mut bytes).unwrap();
        let bytes = bytes.freeze();

        let msg = bytes.slice(String::from_utf8_lossy(&bytes).find(": ").unwrap() + 2..bytes.len());
        let value: serde_json::Value = serde_json::from_slice(&msg).unwrap();
        let value = value.as_object().unwrap();

        assert!(!value.contains_key("magic"));
        assert_eq!(value.get("process").unwrap().as_str(), Some("foo"));
    }
}
