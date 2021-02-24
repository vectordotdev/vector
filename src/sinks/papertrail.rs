use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        tcp::TcpSinkConfig,
        Encoding, UriSerde,
    },
    tcp::TcpKeepaliveConfig,
    tls::TlsConfig,
    Event,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use syslog::{Facility, Formatter3164, LogFormat, Severity};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct PapertrailConfig {
    endpoint: UriSerde,
    encoding: EncodingConfig<Encoding>,
    keepalive: Option<TcpKeepaliveConfig>,
    tls: Option<TlsConfig>,
    send_buffer_bytes: Option<usize>,
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

        let sink_config = TcpSinkConfig::new(address, self.keepalive, tls, self.send_buffer_bytes);

        sink_config.build(cx, move |event| Some(encode_event(event, pid, &encoding)))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "papertrail"
    }
}

fn encode_event(mut event: Event, pid: u32, encoding: &EncodingConfig<Encoding>) -> Bytes {
    let host = if let Some(host) = event.as_mut_log().remove(log_schema().host_key()) {
        Some(host.to_string_lossy())
    } else {
        None
    };

    let formatter = Formatter3164 {
        facility: Facility::LOG_USER,
        hostname: host,
        process: "vector".into(),
        pid: pid as i32,
    };

    let mut s: Vec<u8> = Vec::new();

    encoding.apply_rules(&mut event);
    let log = event.into_log();

    let message = match encoding.codec() {
        Encoding::Json => serde_json::to_string(&log).unwrap(),
        Encoding::Text => log
            .get(log_schema().message_key())
            .map(|v| v.to_string_lossy())
            .unwrap_or_default(),
    };

    formatter
        .format(&mut s, Severity::LOG_INFO, message)
        .unwrap();

    s.push(b'\n');

    Bytes::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PapertrailConfig>();
    }

    #[test]
    fn encode_event_apply_rules() {
        let mut evt = Event::from("vector");
        evt.as_mut_log().insert("magic", "key");

        let bytes = encode_event(
            evt,
            0,
            &EncodingConfig {
                codec: Encoding::Json,
                schema: None,
                only_fields: None,
                except_fields: Some(vec!["magic".into()]),
                timestamp_format: None,
            },
        );

        let msg =
            bytes.slice(String::from_utf8_lossy(&bytes).find(": ").unwrap() + 2..bytes.len() - 1);
        let value: serde_json::Value = serde_json::from_slice(&msg).unwrap();
        assert!(!value.as_object().unwrap().contains_key("magic"));
    }
}
