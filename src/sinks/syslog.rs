use vector_lib::{
    codecs::{
        NewlineDelimitedEncoder, SyslogSerializerConfig, SyslogSerializerOptions, encoding::Framer,
    },
    configurable::configurable_component,
};
use vrl::value::Kind;

#[cfg(not(windows))]
use crate::sinks::util::unix::UnixSinkConfig;
use crate::{
    codecs::{Encoder, Transformer},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    schema,
    sinks::util::{tcp::TcpSinkConfig, udp::UdpSinkConfig},
};

/// Configuration for the `syslog` sink.
///
/// Sends log events as syslog messages over TCP, UDP, or Unix sockets.
/// For TCP with TLS (RFC 5425), configure the `tls` options under the TCP mode.
#[configurable_component(sink(
    "syslog",
    "Deliver log events to a remote syslog endpoint in RFC 5424 or RFC 3164 format."
))]
#[derive(Clone, Debug)]
pub struct SyslogSinkConfig {
    #[serde(flatten)]
    pub mode: Mode,

    /// Syslog encoding options.
    ///
    /// Controls the RFC format, facility, severity, and field mappings for the syslog output.
    #[configurable(derived)]
    #[serde(default)]
    pub syslog: SyslogSerializerOptions,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

/// Syslog sink mode.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The type of socket to use."))]
pub enum Mode {
    /// Send over TCP.
    Tcp(TcpMode),

    /// Send over UDP.
    Udp(UdpMode),

    /// Send over a Unix domain socket (UDS), in stream mode.
    #[serde(alias = "unix")]
    UnixStream(UnixMode),
}

/// TCP configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct TcpMode {
    #[serde(flatten)]
    pub config: TcpSinkConfig,
}

/// UDP configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct UdpMode {
    #[serde(flatten)]
    pub config: UdpSinkConfig,
}

/// Unix Domain Socket configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct UnixMode {
    #[serde(flatten)]
    pub config: UnixSinkConfig,
}

// Workaround for https://github.com/vectordotdev/vector/issues/22198.
#[cfg(windows)]
/// A Unix Domain Socket sink.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct UnixSinkConfig {
    /// The Unix socket path.
    ///
    /// This should be an absolute path.
    #[configurable(metadata(docs::examples = "/path/to/socket"))]
    pub path: std::path::PathBuf,
}

impl GenerateConfig for SyslogSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"address = "127.0.0.1:514"
            mode = "tcp"
            syslog.rfc = "rfc5424""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "syslog")]
impl SinkConfig for SyslogSinkConfig {
    async fn build(
        &self,
        _cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let syslog_config = SyslogSerializerConfig {
            syslog: self.syslog.clone(),
        };
        let serializer = syslog_config.build();
        // No user-configurable transformer: the syslog serializer handles all
        // field extraction internally. Users who need field filtering should
        // use the `socket` sink with `encoding.codec = "syslog"` instead.
        let transformer = Transformer::default();

        match &self.mode {
            Mode::Tcp(TcpMode { config }) => {
                // Newline-delimited framing (RFC 6587 "non-transparent" method).
                // Octet-counting framing (RFC 6587 "transparent" method) is not
                // yet supported — it would require an OctetCountingEncoder in the
                // codecs crate.
                let framer = NewlineDelimitedEncoder::default();
                let encoder = Encoder::<Framer>::new(framer.into(), serializer.into());
                config.build(transformer, encoder)
            }
            Mode::Udp(UdpMode { config }) => {
                let encoder = Encoder::<()>::new(serializer.into());
                config.build(transformer, encoder, None)
            }
            #[cfg(unix)]
            Mode::UnixStream(UnixMode { config }) => {
                let framer = NewlineDelimitedEncoder::default();
                let encoder = Encoder::<Framer>::new(framer.into(), serializer.into());
                config.build(
                    transformer,
                    encoder,
                    super::util::service::net::UnixMode::Stream,
                )
            }
            #[cfg(not(unix))]
            Mode::UnixStream(_) => {
                Err("Unix stream mode is supported only on Unix platforms.".into())
            }
        }
    }

    fn input(&self) -> Input {
        let requirement = schema::Requirement::empty()
            .optional_meaning("host", Kind::bytes())
            .optional_meaning("service", Kind::bytes());

        Input::new(DataType::Log).with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[cfg(test)]
mod tests {
    use std::future::ready;

    use futures::stream;
    use serde::Deserialize;
    use vector_lib::event::{Event, LogEvent};

    use super::*;
    use crate::{
        config::SinkContext,
        test_util::{
            CountReceiver,
            addr::next_addr,
            components::{SINK_TAGS, assert_sink_compliance, run_and_assert_sink_compliance},
            random_lines_with_stream, trace_init,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SyslogSinkConfig>();
    }

    #[tokio::test]
    async fn component_spec_compliance() {
        trace_init();

        let (_guard, addr) = next_addr();
        let _receiver = CountReceiver::receive_lines(addr);

        let config = SyslogSinkConfig::generate_config().to_string();
        let mut config = SyslogSinkConfig::deserialize(
            toml::de::ValueDeserializer::parse(&config).expect("toml should deserialize"),
        )
        .expect("config should be valid");
        // Point to our local test listener instead of the default address.
        config.mode = Mode::Tcp(TcpMode {
            config: TcpSinkConfig::from_address(addr.to_string()),
        });

        let context = SinkContext::default();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let event = Event::Log(LogEvent::from("spec compliance"));
        run_and_assert_sink_compliance(sink, stream::once(ready(event)), &SINK_TAGS).await;
    }

    #[tokio::test]
    async fn tcp_syslog_rfc5424() {
        trace_init();

        let (_guard, addr) = next_addr();
        let mut receiver = CountReceiver::receive_lines(addr);

        let config = SyslogSinkConfig {
            mode: Mode::Tcp(TcpMode {
                config: TcpSinkConfig::from_address(addr.to_string()),
            }),
            syslog: Default::default(),
            acknowledgements: Default::default(),
        };

        let mut event = Event::Log(LogEvent::from("test syslog message"));
        event.as_mut_log().insert("hostname", "test-host");

        assert_sink_compliance(&SINK_TAGS, async move {
            let context = SinkContext::default();
            let (sink, _healthcheck) = config.build(context).await.unwrap();
            sink.run(stream::once(ready(event.into()))).await
        })
        .await
        .expect("Running sink failed");

        receiver.connected().await;
        let output = receiver.await;
        assert!(!output.is_empty(), "No messages received");
        let line = &output[0];
        // RFC 5424: <PRI>VERSION TIMESTAMP HOSTNAME APP-NAME ...
        // The version is always "1", so after the closing ">" we expect "1 ".
        assert!(line.starts_with('<'), "Expected syslog format, got: {line}");
        assert!(
            line.contains(">1 "),
            "Expected RFC 5424 version marker, got: {line}"
        );
        assert!(
            line.contains("test syslog message"),
            "Message not found in output: {line}"
        );
    }

    #[tokio::test]
    async fn tcp_syslog_rfc3164() {
        trace_init();

        let (_guard, addr) = next_addr();
        let mut receiver = CountReceiver::receive_lines(addr);

        let config: SyslogSinkConfig = toml::from_str(&format!(
            r#"
            mode = "tcp"
            address = "{addr}"
            syslog.rfc = "rfc3164"
            "#,
        ))
        .expect("config should parse");

        let mut event = Event::Log(LogEvent::from("rfc3164 test message"));
        event.as_mut_log().insert("hostname", "myhost");

        assert_sink_compliance(&SINK_TAGS, async move {
            let context = SinkContext::default();
            let (sink, _healthcheck) = config.build(context).await.unwrap();
            sink.run(stream::once(ready(event.into()))).await
        })
        .await
        .expect("Running sink failed");

        receiver.connected().await;
        let output = receiver.await;
        assert!(!output.is_empty(), "No messages received");
        let line = &output[0];
        // RFC 3164: <PRI>TIMESTAMP HOSTNAME TAG: MSG
        // Should NOT contain ">1 " (that's RFC 5424's version field).
        assert!(line.starts_with('<'), "Expected syslog format, got: {line}");
        assert!(
            !line.contains(">1 "),
            "RFC 3164 should not have version field, got: {line}"
        );
        assert!(
            line.contains("rfc3164 test message"),
            "Message not found in output: {line}"
        );
    }

    /// Verifies that custom syslog field paths (app_name, facility) are
    /// correctly wired through to the serializer output.
    #[tokio::test]
    async fn tcp_syslog_custom_fields() {
        trace_init();

        let (_guard, addr) = next_addr();
        let mut receiver = CountReceiver::receive_lines(addr);

        let config: SyslogSinkConfig = toml::from_str(&format!(
            r#"
            mode = "tcp"
            address = "{addr}"
            syslog.rfc = "rfc5424"
            syslog.app_name = ".my_app"
            "#,
        ))
        .expect("config should parse");

        let mut event = Event::Log(LogEvent::from("custom fields test"));
        event.as_mut_log().insert("my_app", "myservice");

        assert_sink_compliance(&SINK_TAGS, async move {
            let context = SinkContext::default();
            let (sink, _healthcheck) = config.build(context).await.unwrap();
            sink.run(stream::once(ready(event.into()))).await
        })
        .await
        .expect("Running sink failed");

        receiver.connected().await;
        let output = receiver.await;
        assert!(!output.is_empty(), "No messages received");
        let line = &output[0];
        assert!(
            line.contains("myservice"),
            "Custom app_name not found in output: {line}"
        );
        assert!(
            line.contains("custom fields test"),
            "Message not found in output: {line}"
        );
    }

    /// Sends multiple events over TCP and verifies that newline-delimited
    /// framing produces the correct number of distinct messages.
    #[tokio::test]
    async fn tcp_multiple_events() {
        trace_init();

        let (_guard, addr) = next_addr();
        let mut receiver = CountReceiver::receive_lines(addr);

        let config = SyslogSinkConfig {
            mode: Mode::Tcp(TcpMode {
                config: TcpSinkConfig::from_address(addr.to_string()),
            }),
            syslog: Default::default(),
            acknowledgements: Default::default(),
        };

        let (lines, events) = random_lines_with_stream(10, 100, None);

        assert_sink_compliance(&SINK_TAGS, async move {
            let context = SinkContext::default();
            let (sink, _healthcheck) = config.build(context).await.unwrap();
            sink.run(events).await
        })
        .await
        .expect("Running sink failed");

        receiver.connected().await;
        let output = receiver.await;
        assert_eq!(
            lines.len(),
            output.len(),
            "Expected {} messages but got {}",
            lines.len(),
            output.len()
        );
        for (source, received) in lines.iter().zip(output.iter()) {
            assert!(
                received.starts_with('<'),
                "Expected syslog format, got: {received}"
            );
            assert!(
                received.contains(source),
                "Source line not found in output.\nExpected to contain: {source}\nGot: {received}"
            );
        }
    }

    #[tokio::test]
    async fn udp_syslog() {
        trace_init();

        let (_guard, addr) = next_addr();
        let receiver = std::net::UdpSocket::bind(addr).expect("Failed to bind UDP socket");

        let config = SyslogSinkConfig {
            mode: Mode::Udp(UdpMode {
                config: UdpSinkConfig::from_address(addr.to_string()),
            }),
            syslog: Default::default(),
            acknowledgements: Default::default(),
        };

        let context = SinkContext::default();
        assert_sink_compliance(&SINK_TAGS, async move {
            let (sink, _healthcheck) = config.build(context).await.unwrap();
            let event = Event::Log(LogEvent::from("udp syslog test"));
            sink.run(stream::once(ready(event.into()))).await
        })
        .await
        .expect("Running sink failed");

        let mut buf = [0; 1024];
        let size = receiver
            .recv_from(&mut buf)
            .expect("Did not receive message")
            .0;

        let packet = String::from_utf8(buf[..size].to_vec()).expect("Invalid data received");
        assert!(
            packet.starts_with('<'),
            "Expected syslog format, got: {packet}"
        );
        assert!(
            packet.contains(">1 "),
            "Expected RFC 5424 version marker, got: {packet}"
        );
        assert!(
            packet.contains("udp syslog test"),
            "Message not found in output: {packet}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_stream_syslog() {
        trace_init();

        let out_path = temp_uds_path("syslog_unix_stream_test");
        let mut receiver = CountReceiver::receive_lines_unix(out_path.clone());

        let config = SyslogSinkConfig {
            mode: Mode::UnixStream(UnixMode {
                config: UnixSinkConfig::new(out_path),
            }),
            syslog: Default::default(),
            acknowledgements: Default::default(),
        };

        let event = Event::Log(LogEvent::from("unix syslog test"));

        assert_sink_compliance(&SINK_TAGS, async move {
            let context = SinkContext::default();
            let (sink, _healthcheck) = config.build(context).await.unwrap();
            sink.run(stream::once(ready(event.into()))).await
        })
        .await
        .expect("Running sink failed");

        receiver.connected().await;
        let output = receiver.await;
        assert!(!output.is_empty(), "No messages received");
        let line = &output[0];
        assert!(line.starts_with('<'), "Expected syslog format, got: {line}");
        assert!(
            line.contains(">1 "),
            "Expected RFC 5424 version marker, got: {line}"
        );
        assert!(
            line.contains("unix syslog test"),
            "Message not found in output: {line}"
        );
    }

    #[cfg(unix)]
    fn temp_uds_path(name: &str) -> std::path::PathBuf {
        tempfile::tempdir().unwrap().keep().join(name)
    }
}
