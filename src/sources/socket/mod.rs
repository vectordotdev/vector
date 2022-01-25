pub mod tcp;
mod udp;
#[cfg(unix)]
mod unix;

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[cfg(unix)]
use crate::serde::default_framing_message_based;
use crate::{
    codecs::{decoding::DecodingConfig, NewlineDelimitedDecoderConfig},
    config::{
        log_schema, DataType, GenerateConfig, Output, Resource, SourceConfig, SourceContext,
        SourceDescription,
    },
    sources::util::TcpSource,
    tls::MaybeTlsSettings,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
// TODO: add back when https://github.com/serde-rs/serde/issues/1358 is addressed
// #[serde(deny_unknown_fields)]
pub struct SocketConfig {
    #[serde(flatten)]
    pub mode: Mode,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Tcp(tcp::TcpConfig),
    Udp(udp::UdpConfig),
    #[cfg(unix)]
    UnixDatagram(unix::UnixConfig),
    #[cfg(unix)]
    #[serde(alias = "unix")]
    UnixStream(unix::UnixConfig),
}

impl SocketConfig {
    pub fn new_tcp(tcp_config: tcp::TcpConfig) -> Self {
        tcp_config.into()
    }

    pub fn make_basic_tcp_config(addr: SocketAddr) -> Self {
        tcp::TcpConfig::from_address(addr.into()).into()
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

inventory::submit! {
    SourceDescription::new::<SocketConfig>("socket")
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
                if config.framing().is_some() && config.max_length().is_some() {
                    return Err("Using `max_length` is deprecated and does not have any effect when framing is provided. Configure `max_length` on the framing config instead.".into());
                }

                let max_length = config
                    .max_length()
                    .unwrap_or_else(crate::serde::default_max_length);

                let framing = match config.framing().as_ref() {
                    Some(framing) => framing.clone(),
                    None => Box::new(NewlineDelimitedDecoderConfig::new_with_max_length(
                        max_length,
                    )),
                };

                let decoder = DecodingConfig::new(framing, config.decoding().clone()).build()?;

                let tcp = tcp::RawTcpSource::new(config.clone(), decoder);
                let tls = MaybeTlsSettings::from_config(config.tls(), true)?;
                tcp.run(
                    config.address(),
                    config.keepalive(),
                    config.shutdown_timeout_secs(),
                    tls,
                    config.receive_buffer_bytes(),
                    cx,
                    false.into(),
                    config.connection_limit,
                )
            }
            Mode::Udp(config) => {
                let host_key = config
                    .host_key()
                    .clone()
                    .unwrap_or_else(|| log_schema().host_key().to_string());
                let decoder =
                    DecodingConfig::new(config.framing().clone(), config.decoding().clone())
                        .build()?;
                Ok(udp::udp(
                    config.address(),
                    config.max_length(),
                    host_key,
                    config.receive_buffer_bytes(),
                    decoder,
                    cx.shutdown,
                    cx.out,
                ))
            }
            #[cfg(unix)]
            Mode::UnixDatagram(config) => {
                let host_key = config
                    .host_key
                    .unwrap_or_else(|| log_schema().host_key().to_string());
                let decoder = DecodingConfig::new(
                    config.framing.unwrap_or_else(default_framing_message_based),
                    config.decoding.clone(),
                )
                .build()?;
                Ok(unix::unix_datagram(
                    config.path,
                    config
                        .max_length
                        .unwrap_or_else(crate::serde::default_max_length),
                    host_key,
                    decoder,
                    cx.shutdown,
                    cx.out,
                ))
            }
            #[cfg(unix)]
            Mode::UnixStream(config) => {
                if config.framing.is_some() && config.max_length.is_some() {
                    return Err("Using `max_length` is deprecated and does not have any effect when framing is provided. Configure `max_length` on the framing config instead.".into());
                }

                let max_length = config
                    .max_length
                    .unwrap_or_else(crate::serde::default_max_length);

                let framing = match config.framing.as_ref() {
                    Some(framing) => framing.clone(),
                    None => Box::new(NewlineDelimitedDecoderConfig::new_with_max_length(
                        max_length,
                    )),
                };

                let decoder = DecodingConfig::new(framing, config.decoding.clone()).build()?;

                let host_key = config
                    .host_key
                    .unwrap_or_else(|| log_schema().host_key().to_string());
                Ok(unix::unix_stream(
                    config.path,
                    host_key,
                    decoder,
                    cx.shutdown,
                    cx.out,
                ))
            }
        }
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "socket"
    }

    fn resources(&self) -> Vec<Resource> {
        match self.mode.clone() {
            Mode::Tcp(tcp) => vec![tcp.address().into()],
            Mode::Udp(udp) => vec![Resource::udp(udp.address())],
            #[cfg(unix)]
            Mode::UnixDatagram(_) => vec![],
            #[cfg(unix)]
            Mode::UnixStream(_) => vec![],
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        net::{SocketAddr, UdpSocket},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
    };

    use bytes::Bytes;
    use futures::{stream, StreamExt};
    use tokio::{
        task::JoinHandle,
        time::{Duration, Instant},
    };
    #[cfg(unix)]
    use {
        super::{unix::UnixConfig, Mode},
        futures::SinkExt,
        std::path::PathBuf,
        tokio::{
            io::AsyncWriteExt,
            net::{UnixDatagram, UnixStream},
            task::yield_now,
        },
        tokio_util::codec::{FramedWrite, LinesCodec},
    };

    use super::{tcp::TcpConfig, udp::UdpConfig, SocketConfig};
    #[cfg(unix)]
    use crate::source_sender::ReceiverStream;
    use crate::{
        codecs::NewlineDelimitedDecoderConfig,
        config::{
            log_schema, ComponentKey, GlobalOptions, SinkContext, SourceConfig, SourceContext,
        },
        event::Event,
        shutdown::{ShutdownSignal, SourceShutdownCoordinator},
        sinks::util::tcp::TcpSinkConfig,
        test_util::{
            collect_n,
            components::{self, SOURCE_TESTS, TCP_SOURCE_TAGS},
            next_addr, random_string, send_lines, send_lines_tls, wait_for_tcp,
        },
        tls::{self, TlsConfig, TlsOptions},
        SourceSender,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SocketConfig>();
    }

    //////// TCP TESTS ////////
    #[tokio::test]
    async fn tcp_it_includes_host() {
        components::init_test();
        let (tx, mut rx) = SourceSender::new_test();
        let addr = next_addr();

        let server = SocketConfig::from(TcpConfig::from_address(addr.into()))
            .build(SourceContext::new_test(tx))
            .await
            .unwrap();
        tokio::spawn(server);

        wait_for_tcp(addr).await;
        send_lines(addr, vec!["test".to_owned()].into_iter())
            .await
            .unwrap();

        let event = rx.next().await.unwrap();
        assert_eq!(event.as_log()[log_schema().host_key()], "127.0.0.1".into());

        SOURCE_TESTS.assert(&TCP_SOURCE_TAGS);
    }

    #[tokio::test]
    async fn tcp_splits_on_newline() {
        let (tx, rx) = SourceSender::new_test();
        let addr = next_addr();

        let server = SocketConfig::from(TcpConfig::from_address(addr.into()))
            .build(SourceContext::new_test(tx))
            .await
            .unwrap();
        tokio::spawn(server);

        wait_for_tcp(addr).await;
        send_lines(addr, vec!["foo\nbar".to_owned()].into_iter())
            .await
            .unwrap();

        let events = collect_n(rx, 2).await;

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].as_log()[log_schema().message_key()], "foo".into());
        assert_eq!(events[1].as_log()[log_schema().message_key()], "bar".into());
    }

    #[tokio::test]
    async fn tcp_it_includes_source_type() {
        components::init_test();
        let (tx, mut rx) = SourceSender::new_test();
        let addr = next_addr();

        let server = SocketConfig::from(TcpConfig::from_address(addr.into()))
            .build(SourceContext::new_test(tx))
            .await
            .unwrap();
        tokio::spawn(server);

        wait_for_tcp(addr).await;
        send_lines(addr, vec!["test".to_owned()].into_iter())
            .await
            .unwrap();

        let event = rx.next().await.unwrap();
        assert_eq!(
            event.as_log()[log_schema().source_type_key()],
            "socket".into()
        );

        SOURCE_TESTS.assert(&TCP_SOURCE_TAGS);
    }

    #[tokio::test]
    async fn tcp_continue_after_long_line() {
        components::init_test();
        let (tx, mut rx) = SourceSender::new_test();
        let addr = next_addr();

        let mut config = TcpConfig::from_address(addr.into());
        config.set_max_length(None);
        config.set_framing(Some(Box::new(
            NewlineDelimitedDecoderConfig::new_with_max_length(10),
        )));

        let server = SocketConfig::from(config)
            .build(SourceContext::new_test(tx))
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
        assert_eq!(event.as_log()[log_schema().message_key()], "short".into());

        let event = rx.next().await.unwrap();
        assert_eq!(
            event.as_log()[log_schema().message_key()],
            "more short".into()
        );

        SOURCE_TESTS.assert(&TCP_SOURCE_TAGS);
    }

    #[tokio::test]
    async fn tcp_with_tls() {
        components::init_test();
        let (tx, mut rx) = SourceSender::new_test();
        let addr = next_addr();

        let mut config = TcpConfig::from_address(addr.into());
        config.set_tls(Some(TlsConfig::test_config()));

        let server = SocketConfig::from(config)
            .build(SourceContext::new_test(tx))
            .await
            .unwrap();
        tokio::spawn(server);

        let lines = vec!["one line".to_owned(), "another line".to_owned()];

        wait_for_tcp(addr).await;
        send_lines_tls(addr, "localhost".into(), lines.into_iter(), None)
            .await
            .unwrap();

        let event = rx.next().await.unwrap();
        assert_eq!(
            event.as_log()[log_schema().message_key()],
            "one line".into()
        );

        let event = rx.next().await.unwrap();
        assert_eq!(
            event.as_log()[log_schema().message_key()],
            "another line".into()
        );

        SOURCE_TESTS.assert(&TCP_SOURCE_TAGS);
    }

    #[tokio::test]
    async fn tcp_with_tls_intermediate_ca() {
        components::init_test();
        let (tx, mut rx) = SourceSender::new_test();
        let addr = next_addr();

        let mut config = TcpConfig::from_address(addr.into());
        config.set_tls(Some(TlsConfig {
            enabled: Some(true),
            options: TlsOptions {
                crt_file: Some("tests/data/Chain_with_intermediate.crt".into()),
                key_file: Some("tests/data/Crt_from_intermediate.key".into()),
                ..Default::default()
            },
        }));

        let server = SocketConfig::from(config)
            .build(SourceContext::new_test(tx))
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
        )
        .await
        .unwrap();

        let event = rx.next().await.unwrap();
        assert_eq!(
            event.as_log()[crate::config::log_schema().message_key()],
            "one line".into()
        );

        let event = rx.next().await.unwrap();
        assert_eq!(
            event.as_log()[crate::config::log_schema().message_key()],
            "another line".into()
        );

        SOURCE_TESTS.assert(&TCP_SOURCE_TAGS);
    }

    #[tokio::test]
    async fn tcp_shutdown_simple() {
        components::init_test();
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
        assert_eq!(event.as_log()[log_schema().message_key()], "test".into());

        SOURCE_TESTS.assert(&TCP_SOURCE_TAGS);

        // Now signal to the Source to shut down.
        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(&source_id, deadline);
        let shutdown_success = shutdown_complete.await;
        assert!(shutdown_success);

        // Ensure source actually shut down successfully.
        let _ = source_handle.await.unwrap();
    }

    #[tokio::test]
    async fn tcp_shutdown_infinite_stream() {
        components::init_test();
        // It's important that the buffer be large enough that the TCP source doesn't have
        // to block trying to forward its input into the Sender because the channel is full,
        // otherwise even sending the signal to shut down won't wake it up.
        let (tx, rx) = SourceSender::new_with_buffer(10_000);
        let source_id = ComponentKey::from("tcp_shutdown_infinite_stream");

        let addr = next_addr();
        let (cx, mut shutdown) = SourceContext::new_shutdown(&source_id, tx);

        // Start TCP Source
        let server = SocketConfig::from({
            let mut config = TcpConfig::from_address(addr.into());
            config.set_shutdown_timeout_secs(1);
            config
        })
        .build(cx)
        .await
        .unwrap();
        let source_handle = tokio::spawn(server);

        wait_for_tcp(addr).await;

        let message = random_string(512);
        let message_bytes = Bytes::from(message.clone() + "\n");

        let cx = SinkContext::new_test();
        let encode_event = move |_event| Some(message_bytes.clone());
        let sink_config = TcpSinkConfig::from_address(format!("localhost:{}", addr.port()));
        let (sink, _healthcheck) = sink_config.build(cx, encode_event).unwrap();

        // Spawn future that keeps sending lines to the TCP source forever.
        tokio::spawn(async move {
            let input = stream::repeat(())
                .map(move |_| Event::new_empty_log().into())
                .boxed();
            sink.run(input).await.unwrap();
        });

        // Important that 'rx' doesn't get dropped until the pump has finished sending items to it.
        let events = collect_n(rx, 100).await;
        assert_eq!(100, events.len());
        for event in events {
            assert_eq!(
                event.as_log()[log_schema().message_key()],
                message.clone().into()
            );
        }

        SOURCE_TESTS.assert(&TCP_SOURCE_TAGS);

        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(&source_id, deadline);
        let shutdown_success = shutdown_complete.await;
        assert!(shutdown_success);

        // Ensure that the source has actually shut down.
        let _ = source_handle.await.unwrap();
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
        let (shutdown_signal, _) = shutdown.register_source(source_id);
        init_udp_inner(sender, source_id, shutdown_signal).await
    }

    async fn init_udp(sender: SourceSender) -> SocketAddr {
        let (addr, _handle) = init_udp_inner(
            sender,
            &ComponentKey::from("default"),
            ShutdownSignal::noop(),
        )
        .await;
        addr
    }

    async fn init_udp_inner(
        sender: SourceSender,
        source_key: &ComponentKey,
        shutdown_signal: ShutdownSignal,
    ) -> (SocketAddr, JoinHandle<Result<(), ()>>) {
        let address = next_addr();

        let server = SocketConfig::from(UdpConfig::from_address(address))
            .build(SourceContext {
                key: source_key.clone(),
                globals: GlobalOptions::default(),
                shutdown: shutdown_signal,
                out: sender,
                proxy: Default::default(),
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
        let (tx, rx) = SourceSender::new_test();
        let address = init_udp(tx).await;

        send_lines_udp(address, vec!["test".to_string()]);
        let events = collect_n(rx, 1).await;

        assert_eq!(
            events[0].as_log()[log_schema().message_key()],
            "test".into()
        );
    }

    #[tokio::test]
    async fn udp_message_preserves_newline() {
        let (tx, rx) = SourceSender::new_test();
        let address = init_udp(tx).await;

        send_lines_udp(address, vec!["foo\nbar".to_string()]);
        let events = collect_n(rx, 1).await;

        assert_eq!(
            events[0].as_log()[log_schema().message_key()],
            "foo\nbar".into()
        );
    }

    #[tokio::test]
    async fn udp_multiple_packets() {
        let (tx, rx) = SourceSender::new_test();
        let address = init_udp(tx).await;

        send_lines_udp(address, vec!["test".to_string(), "test2".to_string()]);
        let events = collect_n(rx, 2).await;

        assert_eq!(
            events[0].as_log()[log_schema().message_key()],
            "test".into()
        );
        assert_eq!(
            events[1].as_log()[log_schema().message_key()],
            "test2".into()
        );
    }

    #[tokio::test]
    async fn udp_it_includes_host() {
        let (tx, rx) = SourceSender::new_test();
        let address = init_udp(tx).await;

        let from = send_lines_udp(address, vec!["test".to_string()]);
        let events = collect_n(rx, 1).await;

        assert_eq!(
            events[0].as_log()[log_schema().host_key()],
            format!("{}", from).into()
        );
    }

    #[tokio::test]
    async fn udp_it_includes_source_type() {
        let (tx, rx) = SourceSender::new_test();
        let address = init_udp(tx).await;

        let _ = send_lines_udp(address, vec!["test".to_string()]);
        let events = collect_n(rx, 1).await;

        assert_eq!(
            events[0].as_log()[log_schema().source_type_key()],
            "socket".into()
        );
    }

    #[tokio::test]
    async fn udp_shutdown_simple() {
        let (tx, rx) = SourceSender::new_test();
        let source_id = ComponentKey::from("udp_shutdown_simple");

        let mut shutdown = SourceShutdownCoordinator::default();
        let (address, source_handle) = init_udp_with_shutdown(tx, &source_id, &mut shutdown).await;

        send_lines_udp(address, vec!["test".to_string()]);
        let events = collect_n(rx, 1).await;

        assert_eq!(
            events[0].as_log()[log_schema().message_key()],
            "test".into()
        );

        // Now signal to the Source to shut down.
        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(&source_id, deadline);
        let shutdown_success = shutdown_complete.await;
        assert!(shutdown_success);

        // Ensure source actually shut down successfully.
        let _ = source_handle.await.unwrap();
    }

    #[tokio::test]
    async fn udp_shutdown_infinite_stream() {
        let (tx, rx) = SourceSender::new_test();
        let source_id = ComponentKey::from("udp_shutdown_infinite_stream");

        let mut shutdown = SourceShutdownCoordinator::default();
        let (address, source_handle) = init_udp_with_shutdown(tx, &source_id, &mut shutdown).await;

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
            assert_eq!(event.as_log()[log_schema().message_key()], "test".into());
        }

        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(&source_id, deadline);
        let shutdown_success = shutdown_complete.await;
        assert!(shutdown_success);

        // Ensure that the source has actually shut down.
        let _ = source_handle.await.unwrap();

        // Stop the pump from sending lines forever.
        run_pump_atomic_sender.store(false, Ordering::Relaxed);
        assert!(pump_handle.join().is_ok());
    }

    ////////////// UNIX TEST LIBS //////////////
    #[cfg(unix)]
    async fn init_unix(sender: SourceSender, stream: bool) -> PathBuf {
        let in_path = tempfile::tempdir().unwrap().into_path().join("unix_test");

        let config = UnixConfig::new(in_path.clone());
        let mode = if stream {
            Mode::UnixStream(config)
        } else {
            Mode::UnixDatagram(config)
        };
        let server = SocketConfig { mode }
            .build(SourceContext::new_test(sender))
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
    async fn unix_message(message: &str, stream: bool) -> ReceiverStream<Event> {
        let (tx, rx) = SourceSender::new_test();
        let path = init_unix(tx, stream).await;

        unix_send_lines(stream, path, &[message]).await;

        rx
    }

    #[cfg(unix)]
    async fn unix_multiple_packets(stream: bool) {
        let (tx, rx) = SourceSender::new_test();
        let path = init_unix(tx, stream).await;

        unix_send_lines(stream, path, &["test", "test2"]).await;
        let events = collect_n(rx, 2).await;

        assert_eq!(2, events.len());
        assert_eq!(
            events[0].as_log()[log_schema().message_key()],
            "test".into()
        );
        assert_eq!(
            events[1].as_log()[log_schema().message_key()],
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
        let rx = unix_message("test", false).await;
        let events = collect_n(rx, 1).await;

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].as_log()[log_schema().message_key()],
            "test".into()
        );
        assert_eq!(
            events[0].as_log()[log_schema().source_type_key()],
            "socket".into()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_datagram_message_preserves_newline() {
        let rx = unix_message("foo\nbar", false).await;
        let events = collect_n(rx, 1).await;

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].as_log()[log_schema().message_key()],
            "foo\nbar".into()
        );
        assert_eq!(
            events[0].as_log()[log_schema().source_type_key()],
            "socket".into()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_datagram_multiple_packets() {
        unix_multiple_packets(false).await
    }

    #[cfg(unix)]
    #[test]
    fn parses_unix_datagram_config() {
        let config = parses_unix_config("unix_datagram");
        assert!(matches!(config.mode, Mode::UnixDatagram { .. }));
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
        let rx = unix_message("test", true).await;
        let events = collect_n(rx, 1).await;

        assert_eq!(1, events.len());
        assert_eq!(
            events[0].as_log()[log_schema().message_key()],
            "test".into()
        );
        assert_eq!(
            events[0].as_log()[log_schema().source_type_key()],
            "socket".into()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_stream_message_splits_on_newline() {
        let rx = unix_message("foo\nbar", true).await;
        let events = collect_n(rx, 2).await;

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].as_log()[log_schema().message_key()], "foo".into());
        assert_eq!(
            events[0].as_log()[log_schema().source_type_key()],
            "socket".into()
        );
        assert_eq!(events[1].as_log()[log_schema().message_key()], "bar".into());
        assert_eq!(
            events[1].as_log()[log_schema().source_type_key()],
            "socket".into()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_stream_multiple_packets() {
        unix_multiple_packets(true).await
    }

    #[cfg(unix)]
    #[test]
    fn parses_new_unix_stream_config() {
        let config = parses_unix_config("unix_stream");
        assert!(matches!(config.mode, Mode::UnixStream { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn parses_old_unix_stream_config() {
        let config = parses_unix_config("unix");
        assert!(matches!(config.mode, Mode::UnixStream { .. }));
    }
}
