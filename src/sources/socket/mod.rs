mod tcp;
mod udp;
#[cfg(unix)]
mod unix;

use super::util::TcpSource;
use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    event,
    shutdown::ShutdownSignal,
    tls::MaybeTlsSettings,
    Pipeline,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

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
    SourceDescription::new_without_default::<SocketConfig>("socket")
}

#[typetag::serde(name = "socket")]
impl SourceConfig for SocketConfig {
    fn build(
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
                    .clone()
                    .unwrap_or_else(|| event::log_schema().host_key().clone());
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
                    .clone()
                    .unwrap_or_else(|| event::log_schema().host_key().to_string());
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
    use super::tcp::TcpConfig;
    use super::udp::UdpConfig;
    #[cfg(unix)]
    use super::unix::UnixConfig;
    use super::SocketConfig;
    use crate::config::{GlobalOptions, SourceConfig};
    use crate::dns::Resolver;
    use crate::event;
    use crate::runtime::Runtime;
    use crate::shutdown::{ShutdownSignal, SourceShutdownCoordinator};
    use crate::sinks::util::tcp::TcpSink;
    use crate::test_util::{
        block_on, collect_n, next_addr, runtime, send_lines, send_lines_tls, wait_for_tcp, CollectN,
    };
    use crate::tls::{MaybeTlsSettings, TlsConfig, TlsOptions};
    use crate::Pipeline;
    use bytes::Bytes;
    #[cfg(unix)]
    use futures::{compat::Future01CompatExt, stream, SinkExt};
    use futures01::{sync::oneshot, Future, Stream};
    #[cfg(unix)]
    use std::path::PathBuf;
    use std::{
        net::{SocketAddr, UdpSocket},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
    };
    #[cfg(unix)]
    use tokio::net::UnixStream;
    use tokio::time::{Duration, Instant};
    #[cfg(unix)]
    use tokio_util::codec::{FramedWrite, LinesCodec};

    //////// TCP TESTS ////////
    #[test]
    fn tcp_it_includes_host() {
        let (tx, rx) = Pipeline::new_test();

        let addr = next_addr();

        let server = SocketConfig::from(TcpConfig::new(addr.into()))
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .unwrap();
        let mut rt = runtime();
        rt.spawn(server);
        rt.block_on_std(async move { wait_for_tcp(addr).await });

        rt.block_on_std(send_lines(addr, vec!["test".to_owned()].into_iter()))
            .unwrap();

        let event = rx.wait().next().unwrap().unwrap();
        assert_eq!(
            event.as_log()[&event::log_schema().host_key()],
            "127.0.0.1".into()
        );
    }

    #[test]
    fn tcp_it_includes_source_type() {
        let (tx, rx) = Pipeline::new_test();

        let addr = next_addr();

        let server = SocketConfig::from(TcpConfig::new(addr.into()))
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .unwrap();
        let mut rt = runtime();
        rt.spawn(server);
        rt.block_on_std(async move { wait_for_tcp(addr).await });

        rt.block_on_std(send_lines(addr, vec!["test".to_owned()].into_iter()))
            .unwrap();

        let event = rx.wait().next().unwrap().unwrap();
        assert_eq!(
            event.as_log()[event::log_schema().source_type_key()],
            "socket".into()
        );
    }

    #[test]
    fn tcp_continue_after_long_line() {
        let (tx, rx) = Pipeline::new_test();

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
            .unwrap();
        let mut rt = runtime();
        rt.spawn(server);
        rt.block_on_std(async move { wait_for_tcp(addr).await });

        let lines = vec![
            "short".to_owned(),
            "this is too long".to_owned(),
            "more short".to_owned(),
        ];

        rt.block_on_std(send_lines(addr, lines.into_iter()))
            .unwrap();

        let (event, rx) = block_on(rx.into_future()).unwrap();
        assert_eq!(
            event.unwrap().as_log()[&event::log_schema().message_key()],
            "short".into()
        );

        let (event, _rx) = block_on(rx.into_future()).unwrap();
        assert_eq!(
            event.unwrap().as_log()[&event::log_schema().message_key()],
            "more short".into()
        );
    }

    #[test]
    fn tcp_with_tls() {
        let (tx, rx) = Pipeline::new_test();

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
            .unwrap();
        let mut rt = runtime();
        rt.spawn(server);
        rt.block_on_std(async move { wait_for_tcp(addr).await });

        let lines = vec![
            "short".to_owned(),
            "this is too long".to_owned(),
            "more short".to_owned(),
        ];

        rt.block_on_std(send_lines_tls(addr, "localhost".into(), lines.into_iter()))
            .unwrap();

        let (event, rx) = block_on(rx.into_future()).unwrap();
        assert_eq!(
            event.unwrap().as_log()[&event::log_schema().message_key()],
            "short".into()
        );

        let (event, _rx) = block_on(rx.into_future()).unwrap();
        assert_eq!(
            event.unwrap().as_log()[&event::log_schema().message_key()],
            "more short".into()
        );
    }

    #[test]
    fn tcp_shutdown_simple() {
        let source_name = "tcp_shutdown_simple";
        let (tx, rx) = Pipeline::new_test();
        let addr = next_addr();

        let mut shutdown = SourceShutdownCoordinator::default();
        let (shutdown_signal, _) = shutdown.register_source(source_name);

        // Start TCP Source
        let server = SocketConfig::from(TcpConfig::new(addr.into()))
            .build(source_name, &GlobalOptions::default(), shutdown_signal, tx)
            .unwrap();
        let mut rt = runtime();
        let source_handle = oneshot::spawn(server, &rt.executor());
        rt.block_on_std(async move { wait_for_tcp(addr).await });

        // Send data to Source.
        rt.block_on_std(send_lines(addr, vec!["test".to_owned()].into_iter()))
            .unwrap();

        let event = rx.wait().next().unwrap().unwrap();
        assert_eq!(
            event.as_log()[&event::log_schema().message_key()],
            "test".into()
        );

        // Now signal to the Source to shut down.
        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(source_name, deadline);
        let shutdown_success = rt.block_on(shutdown_complete).unwrap();
        assert_eq!(true, shutdown_success);

        // Ensure source actually shut down successfully.
        rt.block_on(source_handle).unwrap();
    }

    #[test]
    fn tcp_shutdown_infinite_stream() {
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
        .unwrap();
        let mut rt = Runtime::with_thread_count(2).unwrap();
        let source_handle = oneshot::spawn(server, &rt.executor());
        rt.block_on_std(async move { wait_for_tcp(addr).await });

        // Spawn future that keeps sending lines to the TCP source forever.
        let sink = TcpSink::new(
            "localhost".to_owned(),
            addr.port(),
            Resolver,
            MaybeTlsSettings::Raw(()),
        );
        rt.spawn(
            futures01::stream::iter_ok::<_, ()>(std::iter::repeat(()))
                .map(|_| Bytes::from("test\n"))
                .map_err(|_| ())
                .forward(sink)
                .map(|_| ()),
        );

        // Important that 'rx' doesn't get dropped until the pump has finished sending items to it.
        let (_rx, events) = rt.block_on(CollectN::new(rx, 100)).ok().unwrap();
        assert_eq!(100, events.len());
        for event in events {
            assert_eq!(
                event.as_log()[&event::log_schema().message_key()],
                "test".into()
            );
        }

        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(source_name, deadline);
        let shutdown_success = rt.block_on(shutdown_complete).unwrap();
        assert_eq!(true, shutdown_success);

        // Ensure that the source has actually shut down.
        rt.block_on(source_handle).unwrap();
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

    fn init_udp_with_shutdown(
        sender: Pipeline,
        source_name: &str,
        shutdown: &mut SourceShutdownCoordinator,
    ) -> (SocketAddr, Runtime, oneshot::SpawnHandle<(), ()>) {
        let (shutdown_signal, _) = shutdown.register_source(source_name);
        init_udp_inner(sender, source_name, shutdown_signal)
    }

    fn init_udp(sender: Pipeline) -> (SocketAddr, Runtime) {
        let (addr, rt, handle) = init_udp_inner(sender, "default", ShutdownSignal::noop());
        handle.forget();
        (addr, rt)
    }

    fn init_udp_inner(
        sender: Pipeline,
        source_name: &str,
        shutdown_signal: ShutdownSignal,
    ) -> (SocketAddr, Runtime, oneshot::SpawnHandle<(), ()>) {
        let addr = next_addr();

        let server = SocketConfig::from(UdpConfig::new(addr))
            .build(
                source_name,
                &GlobalOptions::default(),
                shutdown_signal,
                sender,
            )
            .unwrap();
        let rt = runtime();
        let source_handle = oneshot::spawn(server, &rt.executor());

        // Wait for UDP to start listening
        thread::sleep(Duration::from_millis(100));

        (addr, rt, source_handle)
    }

    #[test]
    fn udp_message() {
        let (tx, rx) = Pipeline::new_test();

        let (address, mut rt) = init_udp(tx);

        send_lines_udp(address, vec!["test".to_string()]);
        let events = rt.block_on(collect_n(rx, 1)).ok().unwrap();

        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "test".into()
        );
    }

    #[test]
    fn udp_multiple_messages() {
        let (tx, rx) = Pipeline::new_test();

        let (address, mut rt) = init_udp(tx);

        send_lines_udp(address, vec!["test\ntest2".to_string()]);
        let events = rt.block_on(collect_n(rx, 2)).ok().unwrap();

        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "test".into()
        );
        assert_eq!(
            events[1].as_log()[&event::log_schema().message_key()],
            "test2".into()
        );
    }

    #[test]
    fn udp_multiple_packets() {
        let (tx, rx) = Pipeline::new_test();

        let (address, mut rt) = init_udp(tx);

        send_lines_udp(address, vec!["test".to_string(), "test2".to_string()]);
        let events = rt.block_on(collect_n(rx, 2)).ok().unwrap();

        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "test".into()
        );
        assert_eq!(
            events[1].as_log()[&event::log_schema().message_key()],
            "test2".into()
        );
    }

    #[test]
    fn udp_it_includes_host() {
        let (tx, rx) = Pipeline::new_test();

        let (address, mut rt) = init_udp(tx);

        let from = send_lines_udp(address, vec!["test".to_string()]);
        let events = rt.block_on(collect_n(rx, 1)).ok().unwrap();

        assert_eq!(
            events[0].as_log()[&event::log_schema().host_key()],
            format!("{}", from).into()
        );
    }

    #[test]
    fn udp_it_includes_source_type() {
        let (tx, rx) = Pipeline::new_test();

        let (address, mut rt) = init_udp(tx);

        let _ = send_lines_udp(address, vec!["test".to_string()]);
        let events = rt.block_on(collect_n(rx, 1)).ok().unwrap();

        assert_eq!(
            events[0].as_log()[event::log_schema().source_type_key()],
            "socket".into()
        );
    }

    #[test]
    fn udp_shutdown_simple() {
        let (tx, rx) = Pipeline::new_test();
        let source_name = "udp_shutdown_simple";

        let mut shutdown = SourceShutdownCoordinator::default();
        let (address, mut rt, source_handle) =
            init_udp_with_shutdown(tx, source_name, &mut shutdown);

        send_lines_udp(address, vec!["test".to_string()]);
        let events = rt.block_on(collect_n(rx, 1)).ok().unwrap();

        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "test".into()
        );

        // Now signal to the Source to shut down.
        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(source_name, deadline);
        let shutdown_success = rt.block_on(shutdown_complete).unwrap();
        assert_eq!(true, shutdown_success);

        // Ensure source actually shut down successfully.
        rt.block_on(source_handle).unwrap();
    }

    #[test]
    fn udp_shutdown_infinite_stream() {
        let (tx, rx) = Pipeline::new_test();
        let source_name = "udp_shutdown_infinite_stream";

        let mut shutdown = SourceShutdownCoordinator::default();
        let (address, mut rt, source_handle) =
            init_udp_with_shutdown(tx, source_name, &mut shutdown);

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
        let (_rx, events) = rt.block_on(CollectN::new(rx, 100)).ok().unwrap();
        assert_eq!(100, events.len());
        for event in events {
            assert_eq!(
                event.as_log()[&event::log_schema().message_key()],
                "test".into()
            );
        }

        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(source_name, deadline);
        let shutdown_success = rt.block_on(shutdown_complete).unwrap();
        assert_eq!(true, shutdown_success);

        // Ensure that the source has actually shut down.
        rt.block_on(source_handle).unwrap();

        // Stop the pump from sending lines forever.
        run_pump_atomic_sender.store(false, Ordering::Relaxed);
        assert!(pump_handle.join().is_ok());
    }

    ////////////// UNIX TESTS //////////////
    #[cfg(unix)]
    fn init_unix(sender: Pipeline) -> (PathBuf, Runtime) {
        let in_path = tempfile::tempdir().unwrap().into_path().join("unix_test");

        let server = SocketConfig::from(UnixConfig::new(in_path.clone()))
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                sender,
            )
            .unwrap();

        let mut rt = runtime();
        rt.spawn(server);

        // Wait for server to accept traffic
        while std::os::unix::net::UnixStream::connect(&in_path).is_err() {}

        (in_path, rt)
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
    #[test]
    fn unix_message() {
        let (tx, rx) = Pipeline::new_test();
        let (path, mut rt) = init_unix(tx);
        rt.block_on_std(async move {
            send_lines_unix(path, vec!["test"]).await;

            let events = collect_n(rx, 1).compat().await.ok().unwrap();

            assert_eq!(1, events.len());
            assert_eq!(
                events[0].as_log()[&event::log_schema().message_key()],
                "test".into()
            );
            assert_eq!(
                events[0].as_log()[event::log_schema().source_type_key()],
                "socket".into()
            );
        });
    }

    #[cfg(unix)]
    #[test]
    fn unix_multiple_messages() {
        let (tx, rx) = Pipeline::new_test();
        let (path, mut rt) = init_unix(tx);
        rt.block_on_std(async move {
            send_lines_unix(path, vec!["test\ntest2"]).await;
            let events = collect_n(rx, 2).compat().await.ok().unwrap();

            assert_eq!(2, events.len());
            assert_eq!(
                events[0].as_log()[&event::log_schema().message_key()],
                "test".into()
            );
            assert_eq!(
                events[1].as_log()[&event::log_schema().message_key()],
                "test2".into()
            );
        });
    }

    #[cfg(unix)]
    #[test]
    fn unix_multiple_packets() {
        let (tx, rx) = Pipeline::new_test();
        let (path, mut rt) = init_unix(tx);
        rt.block_on_std(async move {
            send_lines_unix(path, vec!["test", "test2"]).await;
            let events = collect_n(rx, 2).compat().await.ok().unwrap();

            assert_eq!(2, events.len());
            assert_eq!(
                events[0].as_log()[&event::log_schema().message_key()],
                "test".into()
            );
            assert_eq!(
                events[1].as_log()[&event::log_schema().message_key()],
                "test2".into()
            );
        });
    }
}
