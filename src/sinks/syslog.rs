use bytes::{BufMut, BytesMut};
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
    codecs::{Encoder, Transformer, encoding::BoxedFramingError},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    schema,
    sinks::util::{tcp::TcpSinkConfig, udp::UdpSinkConfig},
};

/// Configuration for the `syslog` sink.
///
/// Sends log events as syslog messages over TCP, UDP, or Unix stream sockets.
/// For syslog over TLS (RFC 5425), configure TCP with TLS, RFC 5424, and
/// octet-counting framing.
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

    #[configurable(derived)]
    #[serde(default)]
    pub framing: SyslogFramingConfig,
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

    #[configurable(derived)]
    #[serde(default)]
    pub framing: SyslogFramingConfig,
}

/// Stream framing configuration.
///
/// Applies only to stream-oriented transports: TCP and Unix stream sockets. UDP
/// sends exactly one syslog message per datagram and does not use framing.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(default, deny_unknown_fields)]
pub struct SyslogFramingConfig {
    /// The framing method used to separate syslog messages in stream transports.
    pub method: SyslogFramingMethod,
}

impl SyslogFramingConfig {
    fn build(&self) -> Framer {
        match self.method {
            SyslogFramingMethod::NewlineDelimited => NewlineDelimitedEncoder::default().into(),
            SyslogFramingMethod::OctetCounting => Framer::Boxed(Box::new(OctetCountingEncoder)),
        }
    }
}

/// Stream framing method.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyslogFramingMethod {
    /// Terminates each syslog message with a newline (LF) character.
    ///
    /// This is RFC 6587 non-transparent framing. Use octet-counting if
    /// messages can contain embedded newlines.
    #[default]
    NewlineDelimited,

    /// Prefixes each syslog message with its byte length and a space.
    ///
    /// This is RFC 6587 octet-counting framing. When used with TCP, TLS, and
    /// RFC 5424 messages, this is the framing required by RFC 5425.
    OctetCounting,
}

/// RFC 6587/RFC 5425 octet-counting framing.
#[derive(Clone, Debug, Default)]
struct OctetCountingEncoder;

impl tokio_util::codec::Encoder<()> for OctetCountingEncoder {
    type Error = BoxedFramingError;

    fn encode(&mut self, _: (), buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let payload_len = buffer.len();
        let payload = buffer.split();
        let payload_len_str = payload_len.to_string();

        buffer.reserve(payload_len_str.len() + 1 + payload.len());
        buffer.put_slice(payload_len_str.as_bytes());
        buffer.put_u8(b' ');
        buffer.unsplit(payload);

        Ok(())
    }
}

#[cfg(all(test, feature = "syslog-integration-tests"))]
mod integration_tests;

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
            Mode::Tcp(TcpMode { config, framing }) => {
                let encoder = Encoder::<Framer>::new(framing.build(), serializer.into());
                config.build(transformer, encoder)
            }
            Mode::Udp(UdpMode { config }) => {
                let encoder = Encoder::<()>::new(serializer.into());
                config.build(transformer, encoder, None)
            }
            #[cfg(unix)]
            Mode::UnixStream(UnixMode { config, framing }) => {
                let encoder = Encoder::<Framer>::new(framing.build(), serializer.into());
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
    use std::{future::ready, net::SocketAddr, time::Duration};

    use futures::{StreamExt, stream};
    use serde::Deserialize;
    use tokio::{
        io::AsyncReadExt,
        net::TcpListener,
        task::JoinHandle,
        time::{sleep, timeout},
    };
    use tokio_stream::wrappers::TcpListenerStream;
    use tokio_util::codec::{FramedRead, LinesCodec};
    use vector_lib::event::{BatchNotifier, BatchStatus, Event, LogEvent};

    use super::*;
    use crate::{
        config::SinkContext,
        test_util::{
            CountReceiver,
            addr::next_addr,
            components::{SINK_TAGS, assert_sink_compliance, run_and_assert_sink_compliance},
            random_lines_with_stream, trace_init,
        },
        tls::{self, MaybeTlsSettings, TlsConfig, TlsEnableableConfig},
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
            framing: Default::default(),
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
                framing: Default::default(),
            }),
            syslog: Default::default(),
            acknowledgements: Default::default(),
        };

        let mut event = Event::Log(LogEvent::from("test syslog message"));
        event.as_mut_log().insert("host", "test-host");

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
            line.contains("test-host"),
            "Host not found in output: {line}"
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
        event.as_mut_log().insert("host", "myhost");

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
        assert!(line.contains("myhost"), "Host not found in output: {line}");
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
            syslog.facility = ".syslog_facility"
            syslog.severity = ".syslog_severity"
            "#,
        ))
        .expect("config should parse");

        let mut event = Event::Log(LogEvent::from("custom fields test"));
        event.as_mut_log().insert("my_app", "myservice");
        event.as_mut_log().insert("syslog_facility", "local0");
        event.as_mut_log().insert("syslog_severity", "error");

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
            line.starts_with("<131>"),
            "Custom facility and severity did not produce expected PRI: {line}"
        );
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
                framing: Default::default(),
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
    async fn tcp_syslog_octet_counting() {
        trace_init();

        let (_guard, addr) = next_addr();
        let receiver = spawn_tcp_receiver(addr).await;

        let config: SyslogSinkConfig = toml::from_str(&format!(
            r#"
            mode = "tcp"
            address = "{addr}"
            framing.method = "octet_counting"
            syslog.rfc = "rfc5424"
            "#,
        ))
        .expect("config should parse");

        let event = Event::Log(LogEvent::from("line one\nline two"));

        assert_sink_compliance(&SINK_TAGS, async move {
            let context = SinkContext::default();
            let (sink, _healthcheck) = config.build(context).await.unwrap();
            sink.run(stream::once(ready(event.into()))).await
        })
        .await
        .expect("Running sink failed");

        let output = receiver.await.expect("receiver task should complete");
        let frames = decode_octet_counted_frames(&output);
        assert_eq!(frames.len(), 1, "Expected one octet-counted frame");
        assert!(
            frames[0].contains("line one\nline two"),
            "Multiline message was not preserved in one frame: {:?}",
            frames[0]
        );
        assert!(
            !output.ends_with(b"\n"),
            "Octet-counting should not add newline framing: {:?}",
            String::from_utf8_lossy(&output)
        );
    }

    #[tokio::test]
    async fn tcp_tls_syslog_octet_counting_rfc5425() {
        trace_init();

        let (_guard, addr) = next_addr();
        let receiver = spawn_tls_tcp_receiver(addr).await;

        let config = SyslogSinkConfig {
            mode: Mode::Tcp(TcpMode {
                config: TcpSinkConfig::new(
                    addr.to_string(),
                    None,
                    Some(TlsEnableableConfig {
                        enabled: Some(true),
                        options: TlsConfig {
                            verify_certificate: Some(false),
                            verify_hostname: Some(false),
                            ca_file: Some(tls::TEST_PEM_CRT_PATH.into()),
                            ..Default::default()
                        },
                    }),
                    None,
                ),
                framing: SyslogFramingConfig {
                    method: SyslogFramingMethod::OctetCounting,
                },
            }),
            syslog: Default::default(),
            acknowledgements: Default::default(),
        };

        let event = Event::Log(LogEvent::from("tls syslog test"));

        assert_sink_compliance(&SINK_TAGS, async move {
            let context = SinkContext::default();
            let (sink, _healthcheck) = config.build(context).await.unwrap();
            sink.run(stream::once(ready(event.into()))).await
        })
        .await
        .expect("Running sink failed");

        let output = receiver.await.expect("receiver task should complete");
        let frames = decode_octet_counted_frames(&output);
        assert_eq!(frames.len(), 1, "Expected one RFC5425 frame");
        assert!(
            frames[0].contains(">1 "),
            "Expected RFC5424 message inside RFC5425 frame: {:?}",
            frames[0]
        );
        assert!(
            frames[0].contains("tls syslog test"),
            "Message not found in RFC5425 frame: {:?}",
            frames[0]
        );
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
        assert!(
            !packet.ends_with('\n'),
            "UDP syslog packets should contain exactly one syslog message without stream framing: {packet:?}"
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
                framing: Default::default(),
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

    /// Verifies that batch finalizers are marked `Delivered` after a successful
    /// TCP send so downstream acknowledgement chains complete correctly.
    #[tokio::test]
    async fn tcp_finalizers_delivered_on_success() {
        trace_init();

        let (_guard, addr) = next_addr();
        let _receiver = CountReceiver::receive_lines(addr);

        let config = SyslogSinkConfig {
            mode: Mode::Tcp(TcpMode {
                config: TcpSinkConfig::from_address(addr.to_string()),
                framing: Default::default(),
            }),
            syslog: Default::default(),
            acknowledgements: Default::default(),
        };

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let event = Event::Log(
            LogEvent::from("ack-tcp-test").with_batch_notifier(&batch),
        );
        drop(batch);

        assert_sink_compliance(&SINK_TAGS, async move {
            let context = SinkContext::default();
            let (sink, _healthcheck) = config.build(context).await.unwrap();
            sink.run(stream::once(ready(event.into()))).await
        })
        .await
        .expect("Running sink failed");

        assert_eq!(
            receiver.try_recv(),
            Ok(BatchStatus::Delivered),
            "TCP sink should mark batch finalizer Delivered"
        );
    }

    /// Verifies that batch finalizers are marked `Delivered` after a successful
    /// UDP send. This guards the connectionless datagram path in
    /// `src/sinks/util/datagram.rs::send_and_emit`.
    #[tokio::test]
    async fn udp_finalizers_delivered_on_success() {
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

        let (batch, mut ack_receiver) = BatchNotifier::new_with_receiver();
        let event = Event::Log(
            LogEvent::from("ack-udp-test").with_batch_notifier(&batch),
        );
        drop(batch);

        let context = SinkContext::default();
        assert_sink_compliance(&SINK_TAGS, async move {
            let (sink, _healthcheck) = config.build(context).await.unwrap();
            sink.run(stream::once(ready(event.into()))).await
        })
        .await
        .expect("Running sink failed");

        // Drain the datagram so the receiver socket is properly cleaned up.
        let mut buf = [0; 1024];
        receiver
            .recv_from(&mut buf)
            .expect("Did not receive message");

        assert_eq!(
            ack_receiver.try_recv(),
            Ok(BatchStatus::Delivered),
            "UDP sink should mark batch finalizer Delivered"
        );
    }

    /// Forces a hard TCP disconnect mid-stream and verifies the sink reconnects
    /// to a freshly-bound listener and continues delivering events. Mirrors
    /// `src/sinks/socket.rs::tests::reconnect`.
    #[tokio::test]
    async fn tcp_reconnect_after_server_close() {
        trace_init();

        let (_guard, addr) = next_addr();
        let config = SyslogSinkConfig {
            mode: Mode::Tcp(TcpMode {
                config: TcpSinkConfig::from_address(addr.to_string()),
                framing: Default::default(),
            }),
            syslog: Default::default(),
            acknowledgements: Default::default(),
        };

        let context = SinkContext::default();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let (_, events) = random_lines_with_stream(1000, 10000, None);
        let sink_handle = tokio::spawn(run_and_assert_sink_compliance(
            sink,
            events,
            &SINK_TAGS,
        ));

        // First listener: drain a handful of events then drop the connection.
        let mut count = 20usize;
        TcpListenerStream::new(TcpListener::bind(addr).await.unwrap())
            .next()
            .await
            .unwrap()
            .map(|socket| FramedRead::new(socket, LinesCodec::new()))
            .unwrap()
            .map(|x| x.unwrap())
            .take_while(|_| {
                ready(if count > 0 {
                    count -= 1;
                    true
                } else {
                    false
                })
            })
            .collect::<Vec<_>>()
            .await;

        if cfg!(windows) {
            sleep(Duration::from_secs(1)).await;
        }

        // Second listener: if the sink failed to reconnect this never connects
        // and the test hangs out to its tokio timeout.
        assert!(
            timeout(
                Duration::from_secs(5),
                CountReceiver::receive_lines(addr).connected()
            )
            .await
            .is_ok(),
            "syslog sink did not reconnect after the TCP peer closed"
        );

        sink_handle.await.unwrap();
    }

    #[cfg(unix)]
    fn temp_uds_path(name: &str) -> std::path::PathBuf {
        tempfile::tempdir().unwrap().keep().join(name)
    }

    async fn spawn_tcp_receiver(addr: SocketAddr) -> JoinHandle<Vec<u8>> {
        let listener = TcpListener::bind(addr)
            .await
            .expect("TCP receiver should bind");

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("TCP should accept");
            let mut output = Vec::new();
            timeout(Duration::from_secs(5), socket.read_to_end(&mut output))
                .await
                .expect("TCP read should complete")
                .expect("TCP read should succeed");
            output
        })
    }

    async fn spawn_tls_tcp_receiver(addr: SocketAddr) -> JoinHandle<Vec<u8>> {
        let config = Some(TlsEnableableConfig::test_config());
        let tls = MaybeTlsSettings::from_config(config.as_ref(), true)
            .expect("TLS settings should build");
        let mut listener = tls.bind(&addr).await.expect("TLS receiver should bind");

        tokio::spawn(async move {
            let mut socket = listener.accept().await.expect("TLS should accept");
            let mut output = Vec::new();
            timeout(Duration::from_secs(5), socket.read_to_end(&mut output))
                .await
                .expect("TLS read should complete")
                .expect("TLS read should succeed");
            output
        })
    }

    fn decode_octet_counted_frames(output: &[u8]) -> Vec<String> {
        let mut frames = Vec::new();
        let mut cursor = 0;

        while cursor < output.len() {
            let length_start = cursor;
            while cursor < output.len() && output[cursor].is_ascii_digit() {
                cursor += 1;
            }

            assert!(
                cursor > length_start,
                "Octet-counted frame must start with a non-empty length: {:?}",
                String::from_utf8_lossy(output)
            );
            assert!(
                cursor < output.len() && output[cursor] == b' ',
                "Octet-counted frame length must be followed by a space: {:?}",
                String::from_utf8_lossy(output)
            );

            let length = std::str::from_utf8(&output[length_start..cursor])
                .expect("Length should be UTF-8")
                .parse::<usize>()
                .expect("Length should be numeric");
            cursor += 1;

            let frame_end = cursor + length;
            assert!(
                frame_end <= output.len(),
                "Octet-counted frame length exceeds available bytes"
            );
            frames.push(
                String::from_utf8(output[cursor..frame_end].to_vec())
                    .expect("Syslog frame should be UTF-8"),
            );
            cursor = frame_end;
        }

        frames
    }
}
