pub mod tcp;
pub mod udp;
#[cfg(unix)]
mod unix;

use vector_lib::codecs::decoding::DeserializerConfig;
use vector_lib::config::{log_schema, LegacyKey, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path};
use vrl::value::{kind::Collection, Kind};

#[cfg(unix)]
use crate::serde::default_framing_message_based;
use crate::{
    codecs::DecodingConfig,
    config::{GenerateConfig, Resource, SourceConfig, SourceContext, SourceOutput},
    sources::util::net::TcpSource,
    tls::MaybeTlsSettings,
};

/// Configuration for the `socket` source.
#[configurable_component(source("socket", "Collect logs over a socket."))]
#[derive(Clone, Debug)]
pub struct SocketConfig {
    #[serde(flatten)]
    pub mode: Mode,
}

/// Listening mode for the `socket` source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The type of socket to use."))]
#[allow(clippy::large_enum_variant)] // just used for configuration
pub enum Mode {
    /// Listen on TCP.
    Tcp(tcp::TcpConfig),

    /// Listen on UDP.
    Udp(udp::UdpConfig),

    /// Listen on a Unix domain socket (UDS), in datagram mode.
    #[cfg(unix)]
    UnixDatagram(unix::UnixConfig),

    /// Listen on a Unix domain socket (UDS), in stream mode.
    #[cfg(unix)]
    #[serde(alias = "unix")]
    UnixStream(unix::UnixConfig),
}

impl SocketConfig {
    pub fn new_tcp(tcp_config: tcp::TcpConfig) -> Self {
        tcp_config.into()
    }

    pub fn make_basic_tcp_config(addr: std::net::SocketAddr) -> Self {
        tcp::TcpConfig::from_address(addr.into()).into()
    }

    fn decoding(&self) -> DeserializerConfig {
        match &self.mode {
            Mode::Tcp(config) => config.decoding().clone(),
            Mode::Udp(config) => config.decoding().clone(),
            #[cfg(unix)]
            Mode::UnixDatagram(config) => config.decoding().clone(),
            #[cfg(unix)]
            Mode::UnixStream(config) => config.decoding().clone(),
        }
    }

    fn log_namespace(&self, global_log_namespace: LogNamespace) -> LogNamespace {
        match &self.mode {
            Mode::Tcp(config) => global_log_namespace.merge(config.log_namespace),
            Mode::Udp(config) => global_log_namespace.merge(config.log_namespace),
            #[cfg(unix)]
            Mode::UnixDatagram(config) => global_log_namespace.merge(config.log_namespace),
            #[cfg(unix)]
            Mode::UnixStream(config) => global_log_namespace.merge(config.log_namespace),
        }
    }
}

impl From<tcp::TcpConfig> for SocketConfig {
    fn from(config: tcp::TcpConfig) -> Self {
        SocketConfig {
            mode: Mode::Tcp(config),
        }
    }
}

impl From<udp::UdpConfig> for SocketConfig {
    fn from(config: udp::UdpConfig) -> Self {
        SocketConfig {
            mode: Mode::Udp(config),
        }
    }
}

impl GenerateConfig for SocketConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"mode = "tcp"
            address = "0.0.0.0:9000""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "socket")]
impl SourceConfig for SocketConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        match self.mode.clone() {
            Mode::Tcp(config) => {
                let log_namespace = cx.log_namespace(config.log_namespace);

                let decoding = config.decoding().clone();
                let decoder = DecodingConfig::new(
                    config
                        .framing
                        .clone()
                        .unwrap_or_else(|| decoding.default_stream_framing()),
                    decoding,
                    log_namespace,
                )
                .build()?;

                let tcp = tcp::RawTcpSource::new(config.clone(), decoder, log_namespace);
                let tls_config = config.tls().as_ref().map(|tls| tls.tls_config.clone());
                let tls_client_metadata_key = config
                    .tls()
                    .as_ref()
                    .and_then(|tls| tls.client_metadata_key.clone())
                    .and_then(|k| k.path);
                let tls = MaybeTlsSettings::from_config(&tls_config, true)?;
                tcp.run(
                    config.address(),
                    config.keepalive(),
                    config.shutdown_timeout_secs(),
                    tls,
                    tls_client_metadata_key,
                    config.receive_buffer_bytes(),
                    config.max_connection_duration_secs(),
                    cx,
                    false.into(),
                    config.connection_limit,
                    config.permit_origin.map(Into::into),
                    SocketConfig::NAME,
                    log_namespace,
                )
            }
            Mode::Udp(config) => {
                let log_namespace = cx.log_namespace(config.log_namespace);
                let decoder = DecodingConfig::new(
                    config.framing().clone(),
                    config.decoding().clone(),
                    log_namespace,
                )
                .build()?;
                Ok(udp::udp(
                    config,
                    decoder,
                    cx.shutdown,
                    cx.out,
                    log_namespace,
                ))
            }
            #[cfg(unix)]
            Mode::UnixDatagram(config) => {
                let log_namespace = cx.log_namespace(config.log_namespace);
                let decoder = DecodingConfig::new(
                    config
                        .framing
                        .clone()
                        .unwrap_or_else(default_framing_message_based),
                    config.decoding.clone(),
                    log_namespace,
                )
                .build()?;

                unix::unix_datagram(config, decoder, cx.shutdown, cx.out, log_namespace)
            }
            #[cfg(unix)]
            Mode::UnixStream(config) => {
                let log_namespace = cx.log_namespace(config.log_namespace);

                let decoding = config.decoding().clone();
                let decoder = DecodingConfig::new(
                    config
                        .framing
                        .clone()
                        .unwrap_or_else(|| decoding.default_stream_framing()),
                    decoding,
                    log_namespace,
                )
                .build()?;

                unix::unix_stream(config, decoder, cx.shutdown, cx.out, log_namespace)
            }
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = self.log_namespace(global_log_namespace);

        let schema_definition = self
            .decoding()
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata();

        let schema_definition = match &self.mode {
            Mode::Tcp(config) => {
                let legacy_host_key = config.host_key().path.map(LegacyKey::InsertIfEmpty);

                let legacy_port_key = config.port_key().clone().path.map(LegacyKey::InsertIfEmpty);

                let tls_client_metadata_path = config
                    .tls()
                    .as_ref()
                    .and_then(|tls| tls.client_metadata_key.as_ref())
                    .and_then(|k| k.path.clone())
                    .map(LegacyKey::Overwrite);

                schema_definition
                    .with_source_metadata(
                        Self::NAME,
                        legacy_host_key,
                        &owned_value_path!("host"),
                        Kind::bytes(),
                        Some("host"),
                    )
                    .with_source_metadata(
                        Self::NAME,
                        legacy_port_key,
                        &owned_value_path!("port"),
                        Kind::integer(),
                        None,
                    )
                    .with_source_metadata(
                        Self::NAME,
                        tls_client_metadata_path,
                        &owned_value_path!("tls_client_metadata"),
                        Kind::object(Collection::empty().with_unknown(Kind::bytes()))
                            .or_undefined(),
                        None,
                    )
            }
            Mode::Udp(config) => {
                let legacy_host_key = config.host_key().path.map(LegacyKey::InsertIfEmpty);

                let legacy_port_key = config.port_key().clone().path.map(LegacyKey::InsertIfEmpty);

                schema_definition
                    .with_source_metadata(
                        Self::NAME,
                        legacy_host_key,
                        &owned_value_path!("host"),
                        Kind::bytes(),
                        None,
                    )
                    .with_source_metadata(
                        Self::NAME,
                        legacy_port_key,
                        &owned_value_path!("port"),
                        Kind::integer(),
                        None,
                    )
            }
            #[cfg(unix)]
            Mode::UnixDatagram(config) => {
                let legacy_host_key = config.host_key().clone().path.map(LegacyKey::InsertIfEmpty);

                schema_definition.with_source_metadata(
                    Self::NAME,
                    legacy_host_key,
                    &owned_value_path!("host"),
                    Kind::bytes(),
                    None,
                )
            }
            #[cfg(unix)]
            Mode::UnixStream(config) => {
                let legacy_host_key = config.host_key().clone().path.map(LegacyKey::InsertIfEmpty);

                schema_definition.with_source_metadata(
                    Self::NAME,
                    legacy_host_key,
                    &owned_value_path!("host"),
                    Kind::bytes(),
                    None,
                )
            }
        };

        vec![SourceOutput::new_maybe_logs(
            self.decoding().output_type(),
            schema_definition,
        )]
    }

    fn resources(&self) -> Vec<Resource> {
        match self.mode.clone() {
            Mode::Tcp(tcp) => vec![tcp.address().as_tcp_resource()],
            Mode::Udp(udp) => vec![udp.address().as_udp_resource()],
            #[cfg(unix)]
            Mode::UnixDatagram(_) => vec![],
            #[cfg(unix)]
            Mode::UnixStream(_) => vec![],
        }
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

pub(crate) fn default_host_key() -> OptionalValuePath {
    log_schema().host_key().cloned().into()
}

#[cfg(test)]
mod test {
    use approx::assert_relative_eq;
    use std::{
        collections::HashMap,
        net::{SocketAddr, UdpSocket},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
    };

    use bytes::{BufMut, Bytes, BytesMut};
    use futures::{stream, StreamExt};
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpStream;
    use tokio::{
        task::JoinHandle,
        time::{timeout, Duration, Instant},
    };
    use vector_lib::codecs::NewlineDelimitedDecoderConfig;
    #[cfg(unix)]
    use vector_lib::codecs::{
        decoding::CharacterDelimitedDecoderOptions, CharacterDelimitedDecoderConfig,
    };
    use vector_lib::event::EventContainer;
    use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path, path};
    use vrl::value::ObjectMap;
    use vrl::{btreemap, value};

    #[cfg(unix)]
    use {
        super::{unix::UnixConfig, Mode},
        crate::sources::util::unix::UNNAMED_SOCKET_HOST,
        crate::test_util::wait_for,
        futures::{SinkExt, Stream},
        std::future::ready,
        std::os::unix::fs::PermissionsExt,
        std::path::PathBuf,
        tokio::{
            io::AsyncWriteExt,
            net::{UnixDatagram, UnixStream},
            task::yield_now,
        },
        tokio_util::codec::{FramedWrite, LinesCodec},
    };

    use super::{tcp::TcpConfig, udp::UdpConfig, SocketConfig};
    use crate::{
        config::{log_schema, ComponentKey, GlobalOptions, SourceConfig, SourceContext},
        event::{Event, LogEvent},
        shutdown::{ShutdownSignal, SourceShutdownCoordinator},
        sinks::util::tcp::TcpSinkConfig,
        sources::util::net::SocketListenAddr,
        test_util::{
            collect_n, collect_n_limited,
            components::{assert_source_compliance, SOCKET_PUSH_SOURCE_TAGS},
            next_addr, random_string, send_lines, send_lines_tls, wait_for_tcp,
        },
        tls::{self, TlsConfig, TlsEnableableConfig, TlsSourceConfig},
        SourceSender,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SocketConfig>();
    }

    //////// TCP TESTS ////////
    #[tokio::test]
    async fn tcp_it_includes_host() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, mut rx) = SourceSender::new_test();
            let addr = next_addr();

            let server = SocketConfig::from(TcpConfig::from_address(addr.into()))
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap();
            tokio::spawn(server);

            wait_for_tcp(addr).await;
            let addr = send_lines(addr, vec!["test".to_owned()].into_iter())
                .await
                .unwrap();

            let event = rx.next().await.unwrap();

            assert_eq!(event.as_log()["host"], addr.ip().to_string().into());
            assert_eq!(event.as_log()["port"], addr.port().into());
        })
        .await;
    }

    #[tokio::test]
    async fn tcp_it_includes_vector_namespaced_fields() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, mut rx) = SourceSender::new_test();
            let addr = next_addr();
            let mut conf = TcpConfig::from_address(addr.into());
            conf.set_log_namespace(Some(true));

            let server = SocketConfig::from(conf)
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap();
            tokio::spawn(server);

            wait_for_tcp(addr).await;
            let addr = send_lines(addr, vec!["test".to_owned()].into_iter())
                .await
                .unwrap();

            let event = rx.next().await.unwrap();
            let log = event.as_log();
            let event_meta = log.metadata().value();

            assert_eq!(log.value(), &"test".into());
            assert_eq!(
                event_meta.get(path!("vector", "source_type")).unwrap(),
                &value!(SocketConfig::NAME)
            );
            assert_eq!(
                event_meta.get(path!(SocketConfig::NAME, "host")).unwrap(),
                &value!(addr.ip().to_string())
            );
            assert_eq!(
                event_meta.get(path!(SocketConfig::NAME, "port")).unwrap(),
                &value!(addr.port())
            );
        })
        .await;
    }

    #[tokio::test]
    async fn tcp_splits_on_newline() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, rx) = SourceSender::new_test();
            let addr = next_addr();

            let server = SocketConfig::from(TcpConfig::from_address(addr.into()))
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap();
            tokio::spawn(server);

            wait_for_tcp(addr).await;
            send_lines(addr, vec!["foo\nbar".to_owned()].into_iter())
                .await
                .unwrap();

            let events = collect_n(rx, 2).await;

            assert_eq!(events.len(), 2);
            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                "foo".into()
            );
            assert_eq!(
                events[1].as_log()[log_schema().message_key().unwrap().to_string()],
                "bar".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn tcp_it_includes_source_type() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, mut rx) = SourceSender::new_test();
            let addr = next_addr();

            let server = SocketConfig::from(TcpConfig::from_address(addr.into()))
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap();
            tokio::spawn(server);

            wait_for_tcp(addr).await;
            send_lines(addr, vec!["test".to_owned()].into_iter())
                .await
                .unwrap();

            let event = rx.next().await.unwrap();
            assert_eq!(
                event.as_log()[log_schema().source_type_key().unwrap().to_string()],
                "socket".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn tcp_continue_after_long_line() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, mut rx) = SourceSender::new_test();
            let addr = next_addr();

            let mut config = TcpConfig::from_address(addr.into());
            config.set_framing(Some(
                NewlineDelimitedDecoderConfig::new_with_max_length(10).into(),
            ));

            let server = SocketConfig::from(config)
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap();
            tokio::spawn(server);

            let lines = vec![
                "short".to_owned(),
                "this is too long".to_owned(),
                "more short".to_owned(),
            ];

            wait_for_tcp(addr).await;
            send_lines(addr, lines.into_iter()).await.unwrap();

            let event = rx.next().await.unwrap();
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                "short".into()
            );

            let event = rx.next().await.unwrap();
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                "more short".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn tcp_with_tls() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, mut rx) = SourceSender::new_test();
            let addr = next_addr();

            let mut config = TcpConfig::from_address(addr.into());
            config.set_tls(Some(TlsSourceConfig {
                tls_config: TlsEnableableConfig {
                    enabled: Some(true),
                    options: TlsConfig {
                        verify_certificate: Some(true),
                        crt_file: Some(tls::TEST_PEM_CRT_PATH.into()),
                        key_file: Some(tls::TEST_PEM_KEY_PATH.into()),
                        ca_file: Some(tls::TEST_PEM_CA_PATH.into()),
                        ..Default::default()
                    },
                },
                client_metadata_key: Some(OptionalValuePath::from(owned_value_path!("tls_peer"))),
            }));

            let server = SocketConfig::from(config)
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap();
            tokio::spawn(server);

            let lines = vec!["one line".to_owned(), "another line".to_owned()];

            wait_for_tcp(addr).await;
            send_lines_tls(
                addr,
                "localhost".into(),
                lines.into_iter(),
                std::path::Path::new(tls::TEST_PEM_CA_PATH),
                std::path::Path::new(tls::TEST_PEM_CLIENT_CRT_PATH),
                std::path::Path::new(tls::TEST_PEM_CLIENT_KEY_PATH),
            )
            .await
            .unwrap();

            let event = rx.next().await.unwrap();
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                "one line".into()
            );

            let tls_meta: ObjectMap = btreemap!(
                "subject" => "CN=localhost,OU=Vector,O=Datadog,L=New York,ST=New York,C=US"
            );

            assert_eq!(event.as_log()["tls_peer"], tls_meta.clone().into(),);

            let event = rx.next().await.unwrap();
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                "another line".into()
            );

            assert_eq!(event.as_log()["tls_peer"], tls_meta.clone().into(),);
        })
        .await;
    }

    #[tokio::test]
    async fn tcp_with_tls_vector_namespace() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, mut rx) = SourceSender::new_test();
            let addr = next_addr();

            let mut config = TcpConfig::from_address(addr.into());
            config.set_tls(Some(TlsSourceConfig {
                tls_config: TlsEnableableConfig {
                    enabled: Some(true),
                    options: TlsConfig {
                        verify_certificate: Some(true),
                        crt_file: Some(tls::TEST_PEM_CRT_PATH.into()),
                        key_file: Some(tls::TEST_PEM_KEY_PATH.into()),
                        ca_file: Some(tls::TEST_PEM_CA_PATH.into()),
                        ..Default::default()
                    },
                },
                client_metadata_key: None,
            }));
            config.log_namespace = Some(true);

            let server = SocketConfig::from(config)
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap();
            tokio::spawn(server);

            let lines = vec!["one line".to_owned(), "another line".to_owned()];

            wait_for_tcp(addr).await;
            send_lines_tls(
                addr,
                "localhost".into(),
                lines.into_iter(),
                std::path::Path::new(tls::TEST_PEM_CA_PATH),
                std::path::Path::new(tls::TEST_PEM_CLIENT_CRT_PATH),
                std::path::Path::new(tls::TEST_PEM_CLIENT_KEY_PATH),
            )
            .await
            .unwrap();

            let event = rx.next().await.unwrap();
            let log = event.as_log();
            let event_meta = log.metadata().value();

            assert_eq!(log.value(), &"one line".into());

            let tls_meta: ObjectMap = btreemap!(
                "subject" => "CN=localhost,OU=Vector,O=Datadog,L=New York,ST=New York,C=US"
            );

            assert_eq!(
                event_meta
                    .get(path!(SocketConfig::NAME, "tls_client_metadata"))
                    .unwrap(),
                &value!(tls_meta.clone())
            );

            let event = rx.next().await.unwrap();
            let log = event.as_log();
            let event_meta = log.metadata().value();

            assert_eq!(log.value(), &"another line".into());

            assert_eq!(
                event_meta
                    .get(path!(SocketConfig::NAME, "tls_client_metadata"))
                    .unwrap(),
                &value!(tls_meta.clone())
            );
        })
        .await;
    }

    #[tokio::test]
    async fn tcp_shutdown_simple() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let source_id = ComponentKey::from("tcp_shutdown_simple");
            let (tx, mut rx) = SourceSender::new_test();
            let addr = next_addr();
            let (cx, mut shutdown) = SourceContext::new_shutdown(&source_id, tx);

            // Start TCP Source
            let server = SocketConfig::from(TcpConfig::from_address(addr.into()))
                .build(cx)
                .await
                .unwrap();
            let source_handle = tokio::spawn(server);

            // Send data to Source.
            wait_for_tcp(addr).await;
            send_lines(addr, vec!["test".to_owned()].into_iter())
                .await
                .unwrap();

            let event = rx.next().await.unwrap();
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                "test".into()
            );

            // Now signal to the Source to shut down.
            let deadline = Instant::now() + Duration::from_secs(10);
            let shutdown_complete = shutdown.shutdown_source(&source_id, deadline);
            let shutdown_success = shutdown_complete.await;
            assert!(shutdown_success);

            // Ensure source actually shut down successfully.
            _ = source_handle.await.unwrap();
        })
        .await;
    }

    // Intentially not using assert_source_compliance here because this is a round-trip test which
    // means source and sink will both emit `EventsSent` , triggering multi-emission check.
    #[tokio::test]
    async fn tcp_shutdown_infinite_stream() {
        // We create our TCP source with a larger-than-normal send buffer, which helps ensure that
        // the source doesn't block on sending the events downstream, otherwise if it was blocked on
        // doing so, it wouldn't be able to wake up and loop to see that it had been signalled to
        // shutdown.
        let addr = next_addr();

        let (source_tx, source_rx) = SourceSender::new_test_sender_with_buffer(10_000);
        let source_key = ComponentKey::from("tcp_shutdown_infinite_stream");
        let (source_cx, mut shutdown) = SourceContext::new_shutdown(&source_key, source_tx);

        let mut source_config = TcpConfig::from_address(addr.into());
        source_config.set_shutdown_timeout_secs(1);
        let source_task = SocketConfig::from(source_config)
            .build(source_cx)
            .await
            .unwrap();

        // Spawn the source task and wait until we're sure it's listening:
        let source_handle = tokio::spawn(source_task);
        wait_for_tcp(addr).await;

        // Now we create a TCP _sink_ which we'll feed with an infinite stream of events to ship to
        // our TCP source.  This will ensure that our TCP source is fully-loaded as we try to shut
        // it down, exercising the logic we have to ensure timely shutdown even under load:
        let message = random_string(512);
        let message_bytes = Bytes::from(message.clone());

        #[derive(Clone, Debug)]
        struct Serializer {
            bytes: Bytes,
        }
        impl tokio_util::codec::Encoder<Event> for Serializer {
            type Error = vector_lib::codecs::encoding::Error;

            fn encode(&mut self, _: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
                buffer.put(self.bytes.as_ref());
                buffer.put_u8(b'\n');
                Ok(())
            }
        }
        let sink_config = TcpSinkConfig::from_address(format!("localhost:{}", addr.port()));
        let encoder = Serializer {
            bytes: message_bytes,
        };
        let (sink, _healthcheck) = sink_config.build(Default::default(), encoder).unwrap();

        tokio::spawn(async move {
            let input = stream::repeat_with(|| LogEvent::default().into()).boxed();
            sink.run(input).await.unwrap();
        });

        // Now with our sink running, feeding events to the source, collect 100 event arrays from
        // the source and make sure each event within them matches the single message we repeatedly
        // sent via the sink:
        let events = collect_n_limited(source_rx, 100)
            .await
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(100, events.len());

        let message_key = log_schema().message_key().unwrap().to_string();
        let expected_message = message.clone().into();
        for event in events.into_iter().flat_map(EventContainer::into_events) {
            assert_eq!(event.as_log()[message_key.as_str()], expected_message);
        }

        // Now trigger shutdown on the source and ensure that it shuts down before or at the
        // deadline, and make sure the source task actually finished as well:
        let shutdown_timeout_limit = Duration::from_secs(10);
        let deadline = Instant::now() + shutdown_timeout_limit;
        let shutdown_complete = shutdown.shutdown_source(&source_key, deadline);

        let shutdown_result = timeout(shutdown_timeout_limit, shutdown_complete).await;
        assert_eq!(shutdown_result, Ok(true));

        let source_result = source_handle.await.expect("source task should not panic");
        assert_eq!(source_result, Ok(()));
    }

    #[tokio::test]
    async fn tcp_connection_close_after_max_duration() {
        let (tx, _) = SourceSender::new_test();
        let addr = next_addr();

        let mut source_config = TcpConfig::from_address(addr.into());
        source_config.set_max_connection_duration_secs(Some(1));
        let source_task = SocketConfig::from(source_config)
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();

        // Spawn the source task and wait until we're sure it's listening:
        drop(tokio::spawn(source_task));
        wait_for_tcp(addr).await;

        let mut stream: TcpStream = TcpStream::connect(addr)
            .await
            .expect("stream should be able to connect");
        let start = Instant::now();

        let timeout = tokio::time::sleep(Duration::from_millis(1200));
        let mut buffer = [0u8; 10];

        tokio::select! {
             _ = timeout => {
                 panic!("timed out waiting for stream to close")
             },
             read_result = stream.read(&mut buffer) => {
                 match read_result {
                    // read resulting with 0 bytes -> the connection was closed
                    Ok(0) => assert_relative_eq!(start.elapsed().as_secs_f64(), 1.0, epsilon = 0.3),
                    Ok(_) => panic!("unexpectedly read data from stream"),
                    Err(e) => panic!("{:}", e)
                 }
             }
        }
    }

    //////// UDP TESTS ////////
    fn send_lines_udp(addr: SocketAddr, lines: impl IntoIterator<Item = String>) -> SocketAddr {
        let bind = next_addr();
        let socket = UdpSocket::bind(bind)
            .map_err(|error| panic!("{:}", error))
            .ok()
            .unwrap();

        for line in lines {
            assert_eq!(
                socket
                    .send_to(line.as_bytes(), addr)
                    .map_err(|error| panic!("{:}", error))
                    .ok()
                    .unwrap(),
                line.as_bytes().len()
            );
            // Space things out slightly to try to avoid dropped packets
            thread::sleep(Duration::from_millis(1));
        }

        // Give packets some time to flow through
        thread::sleep(Duration::from_millis(10));

        // Done
        bind
    }

    async fn init_udp_with_shutdown(
        sender: SourceSender,
        source_id: &ComponentKey,
        shutdown: &mut SourceShutdownCoordinator,
    ) -> (SocketAddr, JoinHandle<Result<(), ()>>) {
        let (shutdown_signal, _) = shutdown.register_source(source_id, false);
        init_udp_inner(sender, source_id, shutdown_signal, None, false).await
    }

    async fn init_udp(sender: SourceSender, use_log_namespace: bool) -> SocketAddr {
        let (addr, _handle) = init_udp_inner(
            sender,
            &ComponentKey::from("default"),
            ShutdownSignal::noop(),
            None,
            use_log_namespace,
        )
        .await;
        addr
    }

    async fn init_udp_with_config(sender: SourceSender, config: UdpConfig) -> SocketAddr {
        let (addr, _handle) = init_udp_inner(
            sender,
            &ComponentKey::from("default"),
            ShutdownSignal::noop(),
            Some(config),
            false,
        )
        .await;
        addr
    }

    async fn init_udp_inner(
        sender: SourceSender,
        source_key: &ComponentKey,
        shutdown_signal: ShutdownSignal,
        config: Option<UdpConfig>,
        use_vector_namespace: bool,
    ) -> (SocketAddr, JoinHandle<Result<(), ()>>) {
        let (address, mut config) = match config {
            Some(config) => match config.address() {
                SocketListenAddr::SocketAddr(addr) => (addr, config),
                _ => panic!("listen address should not be systemd FD offset in tests"),
            },
            None => {
                let address = next_addr();
                (address, UdpConfig::from_address(address.into()))
            }
        };

        let config = if use_vector_namespace {
            config.set_log_namespace(Some(true));
            config
        } else {
            config
        };

        let server = SocketConfig::from(config)
            .build(SourceContext {
                key: source_key.clone(),
                globals: GlobalOptions::default(),
                shutdown: shutdown_signal,
                out: sender,
                proxy: Default::default(),
                acknowledgements: false,
                schema: Default::default(),
                schema_definitions: HashMap::default(),
                extra_context: Default::default(),
            })
            .await
            .unwrap();
        let source_handle = tokio::spawn(server);

        // Wait for UDP to start listening
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        (address, source_handle)
    }

    #[tokio::test]
    async fn udp_message() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, rx) = SourceSender::new_test();
            let address = init_udp(tx, false).await;

            send_lines_udp(address, vec!["test".to_string()]);
            let events = collect_n(rx, 1).await;

            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                "test".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn udp_message_preserves_newline() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, rx) = SourceSender::new_test();
            let address = init_udp(tx, false).await;

            send_lines_udp(address, vec!["foo\nbar".to_string()]);
            let events = collect_n(rx, 1).await;

            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                "foo\nbar".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn udp_multiple_packets() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, rx) = SourceSender::new_test();
            let address = init_udp(tx, false).await;

            send_lines_udp(address, vec!["test".to_string(), "test2".to_string()]);
            let events = collect_n(rx, 2).await;

            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                "test".into()
            );
            assert_eq!(
                events[1].as_log()[log_schema().message_key().unwrap().to_string()],
                "test2".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn udp_max_length() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, rx) = SourceSender::new_test();
            let address = next_addr();
            let mut config = UdpConfig::from_address(address.into());
            config.max_length = 11;
            let address = init_udp_with_config(tx, config).await;

            send_lines_udp(
                address,
                vec![
                    "short line".to_string(),
                    "test with a long line".to_string(),
                    "a short un".to_string(),
                ],
            );

            let events = collect_n(rx, 2).await;
            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                "short line".into()
            );
            assert_eq!(
                events[1].as_log()[log_schema().message_key().unwrap().to_string()],
                "a short un".into()
            );
        })
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    /// This test only works on Unix.
    /// Unix truncates at max_length giving us the bytes to get the first n delimited messages.
    /// Windows will drop the entire packet if we exceed the max_length so we are unable to
    /// extract anything.
    async fn udp_max_length_delimited() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, rx) = SourceSender::new_test();
            let address = next_addr();
            let mut config = UdpConfig::from_address(address.into());
            config.max_length = 10;
            config.framing = CharacterDelimitedDecoderConfig {
                character_delimited: CharacterDelimitedDecoderOptions::new(b',', None),
            }
            .into();
            let address = init_udp_with_config(tx, config).await;

            send_lines_udp(
                address,
                vec!["test with, long line".to_string(), "short one".to_string()],
            );

            let events = collect_n(rx, 2).await;
            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                "test with".into()
            );
            assert_eq!(
                events[1].as_log()[log_schema().message_key().unwrap().to_string()],
                "short one".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn udp_it_includes_host() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, rx) = SourceSender::new_test();
            let address = init_udp(tx, false).await;

            let from = send_lines_udp(address, vec!["test".to_string()]);
            let events = collect_n(rx, 1).await;

            assert_eq!(events[0].as_log()["host"], from.ip().to_string().into());
            assert_eq!(events[0].as_log()["port"], from.port().into());
        })
        .await;
    }

    #[tokio::test]
    async fn udp_it_includes_vector_namespaced_fields() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, rx) = SourceSender::new_test();
            let address = init_udp(tx, true).await;

            let from = send_lines_udp(address, vec!["test".to_string()]);
            let events = collect_n(rx, 1).await;
            let log = events[0].as_log();
            let event_meta = log.metadata().value();

            assert_eq!(log.value(), &"test".into());
            assert_eq!(
                event_meta.get(path!("vector", "source_type")).unwrap(),
                &value!(SocketConfig::NAME)
            );
            assert_eq!(
                event_meta.get(path!(SocketConfig::NAME, "host")).unwrap(),
                &value!(from.ip().to_string())
            );
            assert_eq!(
                event_meta.get(path!(SocketConfig::NAME, "port")).unwrap(),
                &value!(from.port())
            );
        })
        .await;
    }

    #[tokio::test]
    async fn udp_it_includes_source_type() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, rx) = SourceSender::new_test();
            let address = init_udp(tx, false).await;

            _ = send_lines_udp(address, vec!["test".to_string()]);
            let events = collect_n(rx, 1).await;

            assert_eq!(
                events[0].as_log()[log_schema().source_type_key().unwrap().to_string()],
                "socket".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn udp_shutdown_simple() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, rx) = SourceSender::new_test();
            let source_id = ComponentKey::from("udp_shutdown_simple");

            let mut shutdown = SourceShutdownCoordinator::default();
            let (address, source_handle) =
                init_udp_with_shutdown(tx, &source_id, &mut shutdown).await;

            send_lines_udp(address, vec!["test".to_string()]);
            let events = collect_n(rx, 1).await;

            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                "test".into()
            );

            // Now signal to the Source to shut down.
            let deadline = Instant::now() + Duration::from_secs(10);
            let shutdown_complete = shutdown.shutdown_source(&source_id, deadline);
            let shutdown_success = shutdown_complete.await;
            assert!(shutdown_success);

            // Ensure source actually shut down successfully.
            _ = source_handle.await.unwrap();
        })
        .await;
    }

    #[tokio::test]
    async fn udp_shutdown_infinite_stream() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (tx, rx) = SourceSender::new_test();
            let source_id = ComponentKey::from("udp_shutdown_infinite_stream");

            let mut shutdown = SourceShutdownCoordinator::default();
            let (address, source_handle) =
                init_udp_with_shutdown(tx, &source_id, &mut shutdown).await;

            // Stream that keeps sending lines to the UDP source forever.
            let run_pump_atomic_sender = Arc::new(AtomicBool::new(true));
            let run_pump_atomic_receiver = Arc::clone(&run_pump_atomic_sender);
            let pump_handle = std::thread::spawn(move || {
                send_lines_udp(
                    address,
                    std::iter::repeat("test".to_string())
                        .take_while(move |_| run_pump_atomic_receiver.load(Ordering::Relaxed)),
                );
            });

            // Important that 'rx' doesn't get dropped until the pump has finished sending items to it.
            let events = collect_n(rx, 100).await;
            assert_eq!(100, events.len());
            for event in events {
                assert_eq!(
                    event.as_log()[log_schema().message_key().unwrap().to_string()],
                    "test".into()
                );
            }

            let deadline = Instant::now() + Duration::from_secs(10);
            let shutdown_complete = shutdown.shutdown_source(&source_id, deadline);
            let shutdown_success = shutdown_complete.await;
            assert!(shutdown_success);

            // Ensure that the source has actually shut down.
            _ = source_handle.await.unwrap();

            // Stop the pump from sending lines forever.
            run_pump_atomic_sender.store(false, Ordering::Relaxed);
            assert!(pump_handle.join().is_ok());
        })
        .await;
    }

    ////////////// UNIX TEST LIBS //////////////
    #[cfg(unix)]
    async fn init_unix(sender: SourceSender, stream: bool, use_vector_namespace: bool) -> PathBuf {
        let in_path = tempfile::tempdir().unwrap().into_path().join("unix_test");

        let mut config = UnixConfig::new(in_path.clone());
        if use_vector_namespace {
            config.log_namespace = Some(true);
        }

        let mode = if stream {
            Mode::UnixStream(config)
        } else {
            Mode::UnixDatagram(config)
        };
        let server = SocketConfig { mode }
            .build(SourceContext::new_test(sender, None))
            .await
            .unwrap();
        tokio::spawn(server);

        // Wait for server to accept traffic
        while if stream {
            std::os::unix::net::UnixStream::connect(&in_path).is_err()
        } else {
            let socket = std::os::unix::net::UnixDatagram::unbound().unwrap();
            socket.connect(&in_path).is_err()
        } {
            yield_now().await;
        }

        in_path
    }

    #[cfg(unix)]
    async fn unix_send_lines(stream: bool, path: PathBuf, lines: &[&str]) {
        match stream {
            false => send_lines_unix_datagram(path, lines).await,
            true => send_lines_unix_stream(path, lines).await,
        }
    }

    #[cfg(unix)]
    async fn unix_message(
        message: &str,
        stream: bool,
        use_vector_namespace: bool,
    ) -> (PathBuf, impl Stream<Item = Event>) {
        let (tx, rx) = SourceSender::new_test();
        let path = init_unix(tx, stream, use_vector_namespace).await;
        let path_clone = path.clone();

        unix_send_lines(stream, path, &[message]).await;

        (path_clone, rx)
    }

    #[cfg(unix)]
    async fn unix_multiple_packets(stream: bool) {
        let (tx, rx) = SourceSender::new_test();
        let path = init_unix(tx, stream, false).await;

        unix_send_lines(stream, path, &["test", "test2"]).await;
        let events = collect_n(rx, 2).await;

        assert_eq!(2, events.len());
        assert_eq!(
            events[0].as_log()[log_schema().message_key().unwrap().to_string()],
            "test".into()
        );
        assert_eq!(
            events[1].as_log()[log_schema().message_key().unwrap().to_string()],
            "test2".into()
        );
    }

    #[cfg(unix)]
    fn parses_unix_config(mode: &str) -> SocketConfig {
        toml::from_str::<SocketConfig>(&format!(
            r#"
               mode = "{}"
               path = "/does/not/exist"
            "#,
            mode
        ))
        .unwrap()
    }

    #[cfg(unix)]
    fn parses_unix_config_file_mode(mode: &str) -> SocketConfig {
        toml::from_str::<SocketConfig>(&format!(
            r#"
               mode = "{}"
               path = "/does/not/exist"
               socket_file_mode = 0o777
            "#,
            mode
        ))
        .unwrap()
    }

    ////////////// UNIX DATAGRAM TESTS //////////////
    #[cfg(unix)]
    async fn send_lines_unix_datagram(path: PathBuf, lines: &[&str]) {
        let socket = UnixDatagram::unbound().unwrap();
        socket.connect(path).unwrap();

        for line in lines {
            socket.send(line.as_bytes()).await.unwrap();
        }
        socket.shutdown(std::net::Shutdown::Both).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_datagram_message() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (_, rx) = unix_message("test", false, false).await;
            let events = collect_n(rx, 1).await;

            assert_eq!(events.len(), 1);
            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                "test".into()
            );
            assert_eq!(
                events[0].as_log()[log_schema().source_type_key().unwrap().to_string()],
                "socket".into()
            );
            assert_eq!(events[0].as_log()["host"], UNNAMED_SOCKET_HOST.into());
        })
        .await;
    }

    #[ignore]
    #[cfg(unix)]
    #[tokio::test]
    async fn unix_datagram_socket_test() {
        // This test is useful for testing the behavior of datagram
        // sockets.

        use tempfile::tempdir;
        use tokio::net::UnixDatagram;

        let tmp = tempdir().unwrap();

        let tx_path = tmp.path().join("tx");

        // Switch this var between "bound" and "unbound" to test
        // different types of socket behavior.
        let tx_type = "bound";

        let tx = if tx_type == "bound" {
            UnixDatagram::bind(&tx_path).unwrap()
        } else {
            UnixDatagram::unbound().unwrap()
        };

        // The following debug statements showcase some useful info:
        // dbg!(tx.local_addr().unwrap());
        // dbg!(std::os::unix::prelude::AsRawFd::as_raw_fd(&tx));

        // Create another, bound socket
        let rx_path = tmp.path().join("rx");
        let rx = UnixDatagram::bind(&rx_path).unwrap();

        // Connect to the bound socket
        tx.connect(&rx_path).unwrap();

        // Send to the bound socket
        let bytes = b"hello world";
        tx.send(bytes).await.unwrap();

        let mut buf = vec![0u8; 24];
        let (size, _) = rx.recv_from(&mut buf).await.unwrap();

        let dgram = &buf[..size];
        assert_eq!(dgram, bytes);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_datagram_message_with_vector_namespace() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (_, rx) = unix_message("test", false, true).await;
            let events = collect_n(rx, 1).await;
            let log = events[0].as_log();
            let event_meta = log.metadata().value();

            assert_eq!(log.value(), &"test".into());
            assert_eq!(events.len(), 1);

            assert_eq!(
                event_meta.get(path!("vector", "source_type")).unwrap(),
                &value!(SocketConfig::NAME)
            );

            assert_eq!(
                event_meta.get(path!(SocketConfig::NAME, "host")).unwrap(),
                &value!(UNNAMED_SOCKET_HOST)
            );
        })
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_datagram_message_preserves_newline() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (_, rx) = unix_message("foo\nbar", false, false).await;
            let events = collect_n(rx, 1).await;

            assert_eq!(events.len(), 1);
            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                "foo\nbar".into()
            );
            assert_eq!(
                events[0].as_log()[log_schema().source_type_key().unwrap().to_string()],
                "socket".into()
            );
        })
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_datagram_multiple_packets() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            unix_multiple_packets(false).await
        })
        .await;
    }

    #[cfg(unix)]
    #[test]
    fn parses_unix_datagram_config() {
        let config = parses_unix_config("unix_datagram");
        assert!(matches!(config.mode, Mode::UnixDatagram { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn parses_unix_datagram_perms() {
        let config = parses_unix_config_file_mode("unix_datagram");
        assert!(matches!(config.mode, Mode::UnixDatagram { .. }));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_datagram_permissions() {
        let in_path = tempfile::tempdir().unwrap().into_path().join("unix_test");
        let (tx, _) = SourceSender::new_test();

        let mut config = UnixConfig::new(in_path.clone());
        config.socket_file_mode = Some(0o555);
        let mode = Mode::UnixDatagram(config);
        let server = SocketConfig { mode }
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();
        tokio::spawn(server);

        wait_for(|| {
            match std::fs::metadata(&in_path) {
                Ok(meta) => {
                    match meta.permissions().mode() {
                        // S_IFSOCK   0140000   socket
                        0o140555 => ready(true),
                        _ => ready(false),
                    }
                }
                Err(_) => ready(false),
            }
        })
        .await;
    }

    ////////////// UNIX STREAM TESTS //////////////
    #[cfg(unix)]
    async fn send_lines_unix_stream(path: PathBuf, lines: &[&str]) {
        let socket = UnixStream::connect(path).await.unwrap();
        let mut sink = FramedWrite::new(socket, LinesCodec::new());

        let lines = lines.iter().map(|s| Ok(s.to_string()));
        let lines = lines.collect::<Vec<_>>();
        sink.send_all(&mut stream::iter(lines)).await.unwrap();

        let mut socket = sink.into_inner();
        socket.shutdown().await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_stream_message() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (_, rx) = unix_message("test", true, false).await;
            let events = collect_n(rx, 1).await;

            assert_eq!(1, events.len());
            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                "test".into()
            );
            assert_eq!(
                events[0].as_log()[log_schema().source_type_key().unwrap().to_string()],
                "socket".into()
            );
        })
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_stream_message_with_vector_namespace() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (_, rx) = unix_message("test", true, true).await;
            let events = collect_n(rx, 1).await;
            let log = events[0].as_log();
            let event_meta = log.metadata().value();

            assert_eq!(log.value(), &"test".into());
            assert_eq!(1, events.len());
            assert_eq!(
                event_meta.get(path!("vector", "source_type")).unwrap(),
                &value!(SocketConfig::NAME)
            );
            assert_eq!(
                event_meta.get(path!(SocketConfig::NAME, "host")).unwrap(),
                &value!(UNNAMED_SOCKET_HOST)
            );
        })
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_stream_message_splits_on_newline() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (_, rx) = unix_message("foo\nbar", true, false).await;
            let events = collect_n(rx, 2).await;

            assert_eq!(events.len(), 2);
            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                "foo".into()
            );
            assert_eq!(
                events[0].as_log()[log_schema().source_type_key().unwrap().to_string()],
                "socket".into()
            );
            assert_eq!(
                events[1].as_log()[log_schema().message_key().unwrap().to_string()],
                "bar".into()
            );
            assert_eq!(
                events[1].as_log()[log_schema().source_type_key().unwrap().to_string()],
                "socket".into()
            );
        })
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_stream_multiple_packets() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            unix_multiple_packets(true).await
        })
        .await;
    }

    #[cfg(unix)]
    #[test]
    fn parses_new_unix_stream_config() {
        let config = parses_unix_config("unix_stream");
        assert!(matches!(config.mode, Mode::UnixStream { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn parses_new_unix_datagram_perms() {
        let config = parses_unix_config_file_mode("unix_stream");
        assert!(matches!(config.mode, Mode::UnixStream { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn parses_old_unix_stream_config() {
        let config = parses_unix_config("unix");
        assert!(matches!(config.mode, Mode::UnixStream { .. }));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_stream_permissions() {
        let in_path = tempfile::tempdir().unwrap().into_path().join("unix_test");
        let (tx, _) = SourceSender::new_test();

        let mut config = UnixConfig::new(in_path.clone());
        config.socket_file_mode = Some(0o421);
        let mode = Mode::UnixStream(config);
        let server = SocketConfig { mode }
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();
        tokio::spawn(server);

        wait_for(|| {
            match std::fs::metadata(&in_path) {
                Ok(meta) => {
                    match meta.permissions().mode() {
                        // S_IFSOCK   0140000   socket
                        0o140421 => ready(true),
                        _ => ready(false),
                    }
                }
                Err(_) => ready(false),
            }
        })
        .await;
    }
}
