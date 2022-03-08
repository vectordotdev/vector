use bytes::BufMut;
use codecs::{
    encoding::{BoxedSerializer, Framer},
    NewlineDelimitedEncoder,
};
use serde::{Deserialize, Serialize};
use syslog::{Facility, Formatter3164, LogFormat, Severity};

use crate::{
    codecs::Encoder,
    config::{
        log_schema, AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext,
        SinkDescription,
    },
    event::Event,
    internal_events::TemplateRenderingError,
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration, Transformer},
        tcp::TcpSinkConfig,
        Encoding, UriSerde,
    },
    tcp::TcpKeepaliveConfig,
    template::Template,
    tls::TlsConfig,
};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub(self) struct PapertrailConfig {
    endpoint: UriSerde,
    encoding: EncodingConfig<Encoding>,
    keepalive: Option<TcpKeepaliveConfig>,
    tls: Option<TlsConfig>,
    send_buffer_bytes: Option<usize>,
    process: Option<Template>,
}

inventory::submit! {
    SinkDescription::new::<PapertrailConfig>("papertrail")
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
        cx: SinkContext,
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
        let tls = Some(self.tls.clone().unwrap_or_else(TlsConfig::enabled));

        let pid = std::process::id();
        let encoding = self.encoding.clone();
        let process = self.process.clone();

        let sink_config = TcpSinkConfig::new(address, self.keepalive, tls, self.send_buffer_bytes);

        let framer = NewlineDelimitedEncoder::new().into();
        let serializer = (Box::new(PapertrailSerializer {
            pid,
            process,
            encoding,
        }) as BoxedSerializer)
            .into();
        let encoder = Encoder::<Framer>::new(framer, serializer);
        sink_config.build(cx, Transformer::default(), encoder)
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "papertrail"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        None
    }
}

#[derive(Debug, Clone)]
struct PapertrailSerializer {
    pid: u32,
    process: Option<Template>,
    encoding: EncodingConfig<Encoding>,
}

impl tokio_util::codec::Encoder<Event> for PapertrailSerializer {
    type Error = crate::Error;

    fn encode(
        &mut self,
        mut event: Event,
        buffer: &mut bytes::BytesMut,
    ) -> Result<(), Self::Error> {
        let host = event
            .as_mut_log()
            .remove(log_schema().host_key())
            .map(|host| host.to_string_lossy());

        let process = self
            .process
            .as_ref()
            .and_then(|t| {
                t.render_string(&event)
                    .map_err(|error| {
                        emit!(TemplateRenderingError {
                            error,
                            field: Some("process"),
                            drop_event: false,
                        })
                    })
                    .ok()
            })
            .unwrap_or_else(|| String::from("vector"));

        let formatter = Formatter3164 {
            facility: Facility::LOG_USER,
            hostname: host,
            process,
            pid: self.pid,
        };

        self.encoding.apply_rules(&mut event);
        let log = event.into_log();

        let message = match self.encoding.codec() {
            Encoding::Json => serde_json::to_string(&log).unwrap(),
            Encoding::Text => log
                .get(log_schema().message_key())
                .map(|v| v.to_string_lossy())
                .unwrap_or_default(),
        };

        formatter
            .format(&mut buffer.writer(), Severity::LOG_INFO, message)
            .map_err(|error| format!("{}", error))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use std::convert::TryFrom;
    use tokio_util::codec::Encoder;

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PapertrailConfig>();
    }

    #[test]
    fn encode_event_apply_rules() {
        let mut evt = Event::from("vector");
        evt.as_mut_log().insert("magic", "key");
        evt.as_mut_log().insert("process", "foo");

        let mut bytes = BytesMut::new();
        let mut serializer = PapertrailSerializer {
            pid: 0,
            process: Some(Template::try_from("{{ process }}").unwrap()),
            encoding: EncodingConfig {
                codec: Encoding::Json,
                schema: None,
                only_fields: None,
                except_fields: Some(vec!["magic".into()]),
                timestamp_format: None,
            },
        };
        serializer.encode(evt, &mut bytes).unwrap();
        let bytes = bytes.freeze();

        let msg = bytes.slice(String::from_utf8_lossy(&bytes).find(": ").unwrap() + 2..bytes.len());
        let value: serde_json::Value = serde_json::from_slice(&msg).unwrap();
        let value = value.as_object().unwrap();

        assert!(!value.contains_key("magic"));
        assert_eq!(value.get("process").unwrap().as_str(), Some("foo"));
    }
}
