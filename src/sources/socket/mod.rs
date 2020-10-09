mod tcp;
mod udp;
#[cfg(unix)]
mod unix;

use super::util::TcpSource;
use crate::{
    config::{
        log_schema, DataType, GenerateConfig, GlobalOptions, SourceConfig, SourceDescription,
    },
    shutdown::ShutdownSignal,
    tls::MaybeTlsSettings,
    Pipeline,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use string_cache::DefaultAtom as Atom;

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
    Unix(unix::UnixConfig),
}

impl SocketConfig {
    pub fn make_tcp_config(addr: SocketAddr) -> Self {
        tcp::TcpConfig::new(addr.into()).into()
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

#[cfg(unix)]
impl From<unix::UnixConfig> for SocketConfig {
    fn from(config: unix::UnixConfig) -> Self {
        SocketConfig {
            mode: Mode::Unix(config),
        }
    }
}

inventory::submit! {
    SourceDescription::new::<SocketConfig>("socket")
}

impl GenerateConfig for SocketConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "socket")]
impl SourceConfig for SocketConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        match self.mode.clone() {
            Mode::Tcp(config) => {
                let tcp = tcp::RawTcpSource {
                    config: config.clone(),
                };
                let tls = MaybeTlsSettings::from_config(&config.tls, true)?;
                tcp.run(
                    config.address,
                    config.shutdown_timeout_secs,
                    tls,
                    shutdown,
                    out,
                )
            }
            Mode::Udp(config) => {
                let host_key = config
                    .host_key
                    .unwrap_or_else(|| Atom::from(log_schema().host_key()));
                Ok(udp::udp(
                    config.address,
                    config.max_length,
                    host_key,
                    shutdown,
                    out,
                ))
            }
            #[cfg(unix)]
            Mode::Unix(config) => {
                let host_key = config
                    .host_key
                    .unwrap_or_else(|| log_schema().host_key().to_string());
                Ok(unix::unix(
                    config.path,
                    config.max_length,
                    host_key,
                    shutdown,
                    out,
                ))
            }
        }
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "socket"
    }
}

#[cfg(test)]
mod test {
    use super::{tcp::TcpConfig, udp::UdpConfig, SocketConfig};
    use crate::{
        config::{log_schema, GlobalOptions, SourceConfig},
        dns::Resolver,
        shutdown::{ShutdownSignal, SourceShutdownCoordinator},
        sinks::util::tcp::TcpSink,
        test_util::{
            collect_n, next_addr, random_string, send_lines, send_lines_tls, wait_for_tcp,
        },
        tls::{MaybeTlsSettings, TlsConfig, TlsOptions},
        Pipeline,
    };
    use bytes::Bytes;
    use futures::{
        compat::{Future01CompatExt, Sink01CompatExt, Stream01CompatExt},
        stream, StreamExt,
    };
    use std::{
        net::{SocketAddr, UdpSocket},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
    };
    use string_cache::DefaultAtom as Atom;
    use tokio::{
        task::JoinHandle,
        time::{Duration, Instant},
    };
    #[cfg(unix)]
    use {
        super::unix::UnixConfig,
        futures::SinkExt,
        std::path::PathBuf,
        tokio::{net::UnixStream, task::yield_now},
        tokio_util::codec::{FramedWrite, LinesCodec},
    };

    //////// TCP TESTS ////////
    #[tokio::test]
    async fn tcp_it_includes_host() {
        let (tx, rx) = Pipeline::new_test();
        let addr = next_addr();

        let server = SocketConfig::from(TcpConfig::new(addr.into()))
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .await
            .unwrap()
            .compat();
        tokio::spawn(server);

        wait_for_tcp(addr).await;
        send_lines(addr, vec!["test".to_owned()].into_iter())
            .await
            .unwrap();

        let event = rx.compat().next().await.unwrap().unwrap();
        assert_eq!(
            event.as_log()[&Atom::from(log_schema().host_key())],
            "127.0.0.1".into()
        );
    }

    #[tokio::test]
    async fn tcp_it_includes_source_type() {
        let (tx, rx) = Pipeline::new_test();
        let addr = next_addr();

        let server = SocketConfig::from(TcpConfig::new(addr.into()))
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .await
            .unwrap()
            .compat();
        tokio::spawn(server);

        wait_for_tcp(addr).await;
        send_lines(addr, vec!["test".to_owned()].into_iter())
            .await
            .unwrap();

        let event = rx.compat().next().await.unwrap().unwrap();
        assert_eq!(
            event.as_log()[&Atom::from(log_schema().source_type_key())],
            "socket".into()
        );
    }

    #[tokio::test]
    async fn tcp_continue_after_long_line() {
        let (tx, rx) = Pipeline::new_test();
        let mut rx = rx.compat();
        let addr = next_addr();

        let mut config = TcpConfig::new(addr.into());
        config.max_length = 10;

        let server = SocketConfig::from(config)
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .await
            .unwrap()
            .compat();
        tokio::spawn(server);

        let lines = vec![
            "short".to_owned(),
            "this is too long".to_owned(),
            "more short".to_owned(),
        ];

        wait_for_tcp(addr).await;
        send_lines(addr, lines.into_iter()).await.unwrap();

        let event = rx.next().await.unwrap().unwrap();
        assert_eq!(
            event.as_log()[&Atom::from(log_schema().message_key())],
            "short".into()
        );

        let event = rx.next().await.unwrap().unwrap();
        assert_eq!(
            event.as_log()[&Atom::from(log_schema().message_key())],
            "more short".into()
        );
    }

    #[tokio::test]
    async fn tcp_with_tls() {
        let (tx, rx) = Pipeline::new_test();
        let mut rx = rx.compat();
        let addr = next_addr();

        let mut config = TcpConfig::new(addr.into());
        config.max_length = 10;
        config.tls = Some(TlsConfig {
            enabled: Some(true),
            options: TlsOptions {
                crt_file: Some("tests/data/localhost.crt".into()),
                key_file: Some("tests/data/localhost.key".into()),
                ..Default::default()
            },
        });

        let server = SocketConfig::from(config)
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .await
            .unwrap()
            .compat();
        tokio::spawn(server);

        let lines = vec![
            "short".to_owned(),
            "this is too long".to_owned(),
            "more short".to_owned(),
        ];

        wait_for_tcp(addr).await;
        send_lines_tls(addr, "localhost".into(), lines.into_iter(), None)
            .await
            .unwrap();

        let event = rx.next().await.unwrap().unwrap();
        assert_eq!(
            event.as_log()[&Atom::from(log_schema().message_key())],
            "short".into()
        );

        let event = rx.next().await.unwrap().unwrap();
        assert_eq!(
            event.as_log()[&Atom::from(log_schema().message_key())],
            "more short".into()
        );
    }

    #[tokio::test]
    async fn tcp_with_tls_intermediate_ca() {
        let (tx, rx) = Pipeline::new_test();
        let mut rx = rx.compat();
        let addr = next_addr();

        let mut config = TcpConfig::new(addr.into());
        config.max_length = 10;
        config.tls = Some(TlsConfig {
            enabled: Some(true),
            options: TlsOptions {
                crt_file: Some("tests/data/Chain_with_intermediate.crt".into()),
                key_file: Some("tests/data/Crt_from_intermediate.key".into()),
                ..Default::default()
            },
        });

        let server = SocketConfig::from(config)
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .await
            .unwrap()
            .compat();
        tokio::spawn(server);

        let lines = vec![
            "short".to_owned(),
            "this is too long".to_owned(),
            "more short".to_owned(),
        ];

        wait_for_tcp(addr).await;
        send_lines_tls(
            addr,
            "localhost".into(),
            lines.into_iter(),
            std::path::Path::new("tests/data/Vector_CA.crt"),
        )
        .await
        .unwrap();

        let event = rx.next().await.unwrap().unwrap();
        assert_eq!(
            event.as_log()[&Atom::from(crate::config::log_schema().message_key())],
            "short".into()
        );

        let event = rx.next().await.unwrap().unwrap();
        assert_eq!(
            event.as_log()[&Atom::from(crate::config::log_schema().message_key())],
            "more short".into()
        );
    }

    #[tokio::test]
    async fn tcp_shutdown_simple() {
        let source_name = "tcp_shutdown_simple";
        let (tx, rx) = Pipeline::new_test();
        let addr = next_addr();

        let mut shutdown = SourceShutdownCoordinator::default();
        let (shutdown_signal, _) = shutdown.register_source(source_name);

        // Start TCP Source
        let server = SocketConfig::from(TcpConfig::new(addr.into()))
            .build(source_name, &GlobalOptions::default(), shutdown_signal, tx)
            .await
            .unwrap()
            .compat();
        let source_handle = tokio::spawn(server);

        // Send data to Source.
        wait_for_tcp(addr).await;
        send_lines(addr, vec!["test".to_owned()].into_iter())
            .await
            .unwrap();

        let event = rx.compat().next().await.unwrap().unwrap();
        assert_eq!(
            event.as_log()[&Atom::from(log_schema().message_key())],
            "test".into()
        );

        // Now signal to the Source to shut down.
        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(source_name, deadline);
        let shutdown_success = shutdown_complete.compat().await.unwrap();
        assert_eq!(true, shutdown_success);

        // Ensure source actually shut down successfully.
        let _ = source_handle.await.unwrap();
    }

    #[tokio::test]
    async fn tcp_shutdown_infinite_stream() {
        // It's important that the buffer be large enough that the TCP source doesn't have
        // to block trying to forward its input into the Sender because the channel is full,
        // otherwise even sending the signal to shut down won't wake it up.
        let (tx, rx) = Pipeline::new_with_buffer(10_000);
        let source_name = "tcp_shutdown_infinite_stream";

        let addr = next_addr();
        let mut shutdown = SourceShutdownCoordinator::default();
        let (shutdown_signal, _) = shutdown.register_source(source_name);

        // Start TCP Source
        let server = SocketConfig::from(TcpConfig {
            shutdown_timeout_secs: 1,
            ..TcpConfig::new(addr.into())
        })
        .build(source_name, &GlobalOptions::default(), shutdown_signal, tx)
        .await
        .unwrap()
        .compat();
        let source_handle = tokio::spawn(server);

        wait_for_tcp(addr).await;

        // Spawn future that keeps sending lines to the TCP source forever.
        let sink = TcpSink::new(
            "localhost".to_owned(),
            addr.port(),
            Resolver,
            MaybeTlsSettings::Raw(()),
        );
        let message = random_string(512);
        let message_event = Bytes::from(message.clone() + "\n");
        tokio::spawn(async move {
            let _ = stream::repeat(())
                .map(move |_| Ok(message_event.clone()))
                .forward(sink.sink_compat())
                .await
                .unwrap();
        });

        // Important that 'rx' doesn't get dropped until the pump has finished sending items to it.
        let events = collect_n(rx, 100).await.unwrap();
        assert_eq!(100, events.len());
        for event in events {
            assert_eq!(
                event.as_log()[&Atom::from(log_schema().message_key())],
                message.clone().into()
            );
        }

        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(source_name, deadline);
        let shutdown_success = shutdown_complete.compat().await.unwrap();
        assert_eq!(true, shutdown_success);

        // Ensure that the source has actually shut down.
        let _ = source_handle.await.unwrap();
    }

    //////// UDP TESTS ////////
    fn send_lines_udp(addr: SocketAddr, lines: impl IntoIterator<Item = String>) -> SocketAddr {
        let bind = next_addr();
        let socket = UdpSocket::bind(bind)
            .map_err(|e| panic!("{:}", e))
            .ok()
            .unwrap();

        for line in lines {
            assert_eq!(
                socket
                    .send_to(line.as_bytes(), addr)
                    .map_err(|e| panic!("{:}", e))
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
        sender: Pipeline,
        source_name: &str,
        shutdown: &mut SourceShutdownCoordinator,
    ) -> (SocketAddr, JoinHandle<Result<(), ()>>) {
        let (shutdown_signal, _) = shutdown.register_source(source_name);
        init_udp_inner(sender, source_name, shutdown_signal).await
    }

    async fn init_udp(sender: Pipeline) -> SocketAddr {
        let (addr, _handle) = init_udp_inner(sender, "default", ShutdownSignal::noop()).await;
        addr
    }

    async fn init_udp_inner(
        sender: Pipeline,
        source_name: &str,
        shutdown_signal: ShutdownSignal,
    ) -> (SocketAddr, JoinHandle<Result<(), ()>>) {
        let addr = next_addr();

        let server = SocketConfig::from(UdpConfig::new(addr))
            .build(
                source_name,
                &GlobalOptions::default(),
                shutdown_signal,
                sender,
            )
            .await
            .unwrap()
            .compat();
        let source_handle = tokio::spawn(server);

        // Wait for UDP to start listening
        tokio::time::delay_for(tokio::time::Duration::from_millis(100)).await;

        (addr, source_handle)
    }

    #[tokio::test]
    async fn udp_message() {
        let (tx, rx) = Pipeline::new_test();
        let address = init_udp(tx).await;

        send_lines_udp(address, vec!["test".to_string()]);
        let events = collect_n(rx, 1).await.unwrap();

        assert_eq!(
            events[0].as_log()[&Atom::from(log_schema().message_key())],
            "test".into()
        );
    }

    #[tokio::test]
    async fn udp_multiple_messages() {
        let (tx, rx) = Pipeline::new_test();
        let address = init_udp(tx).await;

        send_lines_udp(address, vec!["test\ntest2".to_string()]);
        let events = collect_n(rx, 2).await.unwrap();

        assert_eq!(
            events[0].as_log()[&Atom::from(log_schema().message_key())],
            "test".into()
        );
        assert_eq!(
            events[1].as_log()[&Atom::from(log_schema().message_key())],
            "test2".into()
        );
    }

    #[tokio::test]
    async fn udp_multiple_packets() {
        let (tx, rx) = Pipeline::new_test();
        let address = init_udp(tx).await;

        send_lines_udp(address, vec!["test".to_string(), "test2".to_string()]);
        let events = collect_n(rx, 2).await.unwrap();

        assert_eq!(
            events[0].as_log()[&Atom::from(log_schema().message_key())],
            "test".into()
        );
        assert_eq!(
            events[1].as_log()[&Atom::from(log_schema().message_key())],
            "test2".into()
        );
    }

    #[tokio::test]
    async fn udp_it_includes_host() {
        let (tx, rx) = Pipeline::new_test();
        let address = init_udp(tx).await;

        let from = send_lines_udp(address, vec!["test".to_string()]);
        let events = collect_n(rx, 1).await.unwrap();

        assert_eq!(
            events[0].as_log()[&Atom::from(log_schema().host_key())],
            format!("{}", from).into()
        );
    }

    #[tokio::test]
    async fn udp_it_includes_source_type() {
        let (tx, rx) = Pipeline::new_test();
        let address = init_udp(tx).await;

        let _ = send_lines_udp(address, vec!["test".to_string()]);
        let events = collect_n(rx, 1).await.unwrap();

        assert_eq!(
            events[0].as_log()[&Atom::from(log_schema().source_type_key())],
            "socket".into()
        );
    }

    #[tokio::test]
    async fn udp_shutdown_simple() {
        let (tx, rx) = Pipeline::new_test();
        let source_name = "udp_shutdown_simple";

        let mut shutdown = SourceShutdownCoordinator::default();
        let (address, source_handle) = init_udp_with_shutdown(tx, source_name, &mut shutdown).await;

        send_lines_udp(address, vec!["test".to_string()]);
        let events = collect_n(rx, 1).await.unwrap();

        assert_eq!(
            events[0].as_log()[&Atom::from(log_schema().message_key())],
            "test".into()
        );

        // Now signal to the Source to shut down.
        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(source_name, deadline);
        let shutdown_success = shutdown_complete.compat().await.unwrap();
        assert_eq!(true, shutdown_success);

        // Ensure source actually shut down successfully.
        let _ = source_handle.await.unwrap();
    }

    #[tokio::test]
    async fn udp_shutdown_infinite_stream() {
        let (tx, rx) = Pipeline::new_test();
        let source_name = "udp_shutdown_infinite_stream";

        let mut shutdown = SourceShutdownCoordinator::default();
        let (address, source_handle) = init_udp_with_shutdown(tx, source_name, &mut shutdown).await;

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
        let events = collect_n(rx, 100).await.unwrap();
        assert_eq!(100, events.len());
        for event in events {
            assert_eq!(
                event.as_log()[&Atom::from(log_schema().message_key())],
                "test".into()
            );
        }

        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(source_name, deadline);
        let shutdown_success = shutdown_complete.compat().await.unwrap();
        assert_eq!(true, shutdown_success);

        // Ensure that the source has actually shut down.
        let _ = source_handle.await.unwrap();

        // Stop the pump from sending lines forever.
        run_pump_atomic_sender.store(false, Ordering::Relaxed);
        assert!(pump_handle.join().is_ok());
    }

    ////////////// UNIX TESTS //////////////
    #[cfg(unix)]
    async fn init_unix(sender: Pipeline) -> PathBuf {
        let in_path = tempfile::tempdir().unwrap().into_path().join("unix_test");

        let server = SocketConfig::from(UnixConfig::new(in_path.clone()))
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                sender,
            )
            .await
            .unwrap()
            .compat();
        tokio::spawn(server);

        // Wait for server to accept traffic
        while std::os::unix::net::UnixStream::connect(&in_path).is_err() {
            yield_now().await;
        }

        in_path
    }

    #[cfg(unix)]
    async fn send_lines_unix(path: PathBuf, lines: Vec<&str>) {
        let socket = UnixStream::connect(path).await.unwrap();
        let mut sink = FramedWrite::new(socket, LinesCodec::new());

        let lines = lines.into_iter().map(|s| Ok(s.to_string()));
        let lines = lines.collect::<Vec<_>>();
        sink.send_all(&mut stream::iter(lines)).await.unwrap();

        let socket = sink.into_inner();
        socket.shutdown(std::net::Shutdown::Both).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_message() {
        let (tx, rx) = Pipeline::new_test();
        let path = init_unix(tx).await;

        send_lines_unix(path, vec!["test"]).await;

        let events = collect_n(rx, 1).await.unwrap();

        assert_eq!(1, events.len());
        assert_eq!(
            events[0].as_log()[&Atom::from(log_schema().message_key())],
            "test".into()
        );
        assert_eq!(
            events[0].as_log()[&Atom::from(log_schema().source_type_key())],
            "socket".into()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_multiple_messages() {
        let (tx, rx) = Pipeline::new_test();
        let path = init_unix(tx).await;

        send_lines_unix(path, vec!["test\ntest2"]).await;
        let events = collect_n(rx, 2).await.unwrap();

        assert_eq!(2, events.len());
        assert_eq!(
            events[0].as_log()[&Atom::from(log_schema().message_key())],
            "test".into()
        );
        assert_eq!(
            events[1].as_log()[&Atom::from(log_schema().message_key())],
            "test2".into()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_multiple_packets() {
        let (tx, rx) = Pipeline::new_test();
        let path = init_unix(tx).await;

        send_lines_unix(path, vec!["test", "test2"]).await;
        let events = collect_n(rx, 2).await.unwrap();

        assert_eq!(2, events.len());
        assert_eq!(
            events[0].as_log()[&Atom::from(log_schema().message_key())],
            "test".into()
        );
        assert_eq!(
            events[1].as_log()[&Atom::from(log_schema().message_key())],
            "test2".into()
        );
    }
}
