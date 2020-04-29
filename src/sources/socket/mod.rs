mod tcp;
mod udp;
#[cfg(unix)]
mod unix;

use super::util::TcpSource;
use crate::{
    event::{self, Event},
    shutdown::ShutdownSignal,
    tls::MaybeTlsSettings,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use futures01::sync::mpsc;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Deserialize, Serialize, Debug, Clone)]
// TODO: add back when serde-rs/serde#1358 is addressed
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
        out: mpsc::Sender<Event>,
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
                    .unwrap_or(event::log_schema().host_key().clone());
                Ok(udp::udp(config.address, host_key, shutdown, out))
            }
            #[cfg(unix)]
            Mode::Unix(config) => {
                let host_key = config
                    .host_key
                    .clone()
                    .unwrap_or(event::log_schema().host_key().to_string());
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
    use crate::event;
    use crate::runtime;
    use crate::shutdown::{ShutdownSignal, SourceShutdownCoordinator};
    use crate::test_util::{
        block_on, collect_n, next_addr, send_lines, send_lines_tls, wait_for_tcp, CollectN,
    };
    use crate::tls::{TlsConfig, TlsOptions};
    use crate::topology::config::{GlobalOptions, SourceConfig};
    #[cfg(unix)]
    use futures01::Sink;
    use futures01::{
        sync::{mpsc, oneshot},
        Future, Stream,
    };
    use std::net::UdpSocket;
    #[cfg(unix)]
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::{net::SocketAddr, thread, time::Duration, time::Instant};
    #[cfg(unix)]
    use tokio01::codec::{FramedWrite, LinesCodec};
    #[cfg(unix)]
    use tokio_uds::UnixStream;

    //////// TCP TESTS ////////
    #[test]
    fn tcp_it_includes_host() {
        let (tx, rx) = mpsc::channel(1);

        let addr = next_addr();

        let server = SocketConfig::from(TcpConfig::new(addr.into()))
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .unwrap();
        let mut rt = runtime::Runtime::new().unwrap();
        rt.spawn(server);
        wait_for_tcp(addr);

        rt.block_on(send_lines(addr, vec!["test".to_owned()].into_iter()))
            .unwrap();

        let event = rx.wait().next().unwrap().unwrap();
        assert_eq!(
            event.as_log()[&event::log_schema().host_key()],
            "127.0.0.1".into()
        );
    }

    #[test]
    fn tcp_it_includes_source_type() {
        let (tx, rx) = mpsc::channel(1);

        let addr = next_addr();

        let server = SocketConfig::from(TcpConfig::new(addr.into()))
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .unwrap();
        let mut rt = runtime::Runtime::new().unwrap();
        rt.spawn(server);
        wait_for_tcp(addr);

        rt.block_on(send_lines(addr, vec!["test".to_owned()].into_iter()))
            .unwrap();

        let event = rx.wait().next().unwrap().unwrap();
        assert_eq!(
            event.as_log()[event::log_schema().source_type_key()],
            "socket".into()
        );
    }

    #[test]
    fn tcp_continue_after_long_line() {
        let (tx, rx) = mpsc::channel(10);

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
        let mut rt = runtime::Runtime::new().unwrap();
        rt.spawn(server);
        wait_for_tcp(addr);

        let lines = vec![
            "short".to_owned(),
            "this is too long".to_owned(),
            "more short".to_owned(),
        ];

        rt.block_on(send_lines(addr, lines.into_iter())).unwrap();

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
        let (tx, rx) = mpsc::channel(10);

        let addr = next_addr();

        let mut config = TcpConfig::new(addr.into());
        config.max_length = 10;
        config.tls = Some(TlsConfig {
            enabled: Some(true),
            options: TlsOptions {
                crt_path: Some("tests/data/localhost.crt".into()),
                key_path: Some("tests/data/localhost.key".into()),
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
        let mut rt = runtime::Runtime::new().unwrap();
        rt.spawn(server);
        wait_for_tcp(addr);

        let lines = vec![
            "short".to_owned(),
            "this is too long".to_owned(),
            "more short".to_owned(),
        ];

        rt.block_on(send_lines_tls(addr, "localhost".into(), lines.into_iter()))
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
        let (tx, rx) = mpsc::channel(2);
        let addr = next_addr();

        let mut shutdown = SourceShutdownCoordinator::new();
        let (shutdown_signal, _) = shutdown.register_source(source_name);

        // Start TCP Source
        let server = SocketConfig::from(TcpConfig::new(addr.into()))
            .build(source_name, &GlobalOptions::default(), shutdown_signal, tx)
            .unwrap();
        let mut rt = runtime::Runtime::new().unwrap();
        let source_handle = oneshot::spawn(server, &rt.executor());
        wait_for_tcp(addr);

        // Send data to Source.
        rt.block_on(send_lines(addr, vec!["test".to_owned()].into_iter()))
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
        let (tx, rx) = mpsc::channel(1000);
        let source_name = "tcp_shutdown_infinite_stream";

        let addr = next_addr();

        let mut shutdown = SourceShutdownCoordinator::new();
        let (shutdown_signal, _) = shutdown.register_source(source_name);

        // Start TCP Source
        let server = SocketConfig::from(TcpConfig::new(addr.into()))
            .build(source_name, &GlobalOptions::default(), shutdown_signal, tx)
            .unwrap();
        let mut rt = runtime::Runtime::new().unwrap();
        let source_handle = oneshot::spawn(server, &rt.executor());
        wait_for_tcp(addr);

        // Spawn future that keeps sending lines to the TCP source forever.
        let run_pump_atomic_sender = Arc::new(AtomicBool::new(true));
        let run_pump_atomic_receiver = run_pump_atomic_sender.clone();
        let pump_future = send_lines(
            addr,
            std::iter::repeat("test".to_string())
                .take_while(move |_| run_pump_atomic_receiver.load(Ordering::Relaxed)),
        );
        let pump_handle = std::thread::spawn(move || {
            pump_future.wait().ok().unwrap();
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
        sender: mpsc::Sender<event::Event>,
        source_name: &str,
        shutdown: &mut SourceShutdownCoordinator,
    ) -> (SocketAddr, runtime::Runtime, oneshot::SpawnHandle<(), ()>) {
        let (shutdown_signal, _) = shutdown.register_source(source_name);
        init_udp_inner(sender, source_name, shutdown_signal)
    }

    fn init_udp(sender: mpsc::Sender<event::Event>) -> (SocketAddr, runtime::Runtime) {
        let (addr, rt, handle) = init_udp_inner(sender, "default", ShutdownSignal::noop());
        handle.forget();
        return (addr, rt);
    }

    fn init_udp_inner(
        sender: mpsc::Sender<event::Event>,
        source_name: &str,
        shutdown_signal: ShutdownSignal,
    ) -> (SocketAddr, runtime::Runtime, oneshot::SpawnHandle<(), ()>) {
        let addr = next_addr();

        let server = SocketConfig::from(UdpConfig::new(addr))
            .build(
                source_name,
                &GlobalOptions::default(),
                shutdown_signal,
                sender,
            )
            .unwrap();
        let rt = runtime::Runtime::new().unwrap();
        let source_handle = oneshot::spawn(server, &rt.executor());

        // Wait for udp to start listening
        thread::sleep(Duration::from_millis(100));

        (addr, rt, source_handle)
    }

    #[test]
    fn udp_message() {
        let (tx, rx) = mpsc::channel(2);

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
        let (tx, rx) = mpsc::channel(10);

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
        let (tx, rx) = mpsc::channel(10);

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
        let (tx, rx) = mpsc::channel(2);

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
        let (tx, rx) = mpsc::channel(2);

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
        let (tx, rx) = mpsc::channel(2);
        let source_name = "udp_shutdown_simple";

        let mut shutdown = SourceShutdownCoordinator::new();
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
        let (tx, rx) = mpsc::channel(10);
        let source_name = "udp_shutdown_infinite_stream";

        let mut shutdown = SourceShutdownCoordinator::new();
        let (address, mut rt, source_handle) =
            init_udp_with_shutdown(tx, source_name, &mut shutdown);

        // Stream that keeps sending lines to the UDP source forever.
        let run_pump_atomic_sender = Arc::new(AtomicBool::new(true));
        let run_pump_atomic_receiver = run_pump_atomic_sender.clone();
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
    fn init_unix(sender: mpsc::Sender<event::Event>) -> (PathBuf, runtime::Runtime) {
        let in_path = tempfile::tempdir().unwrap().into_path().join("unix_test");

        let server = SocketConfig::from(UnixConfig::new(in_path.clone()))
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                sender,
            )
            .unwrap();

        let mut rt = runtime::Runtime::new().unwrap();
        rt.spawn(server);

        // Wait for server to accept traffic
        while let Err(_) = std::os::unix::net::UnixStream::connect(&in_path) {}

        (in_path, rt)
    }

    #[cfg(unix)]
    fn send_lines_unix<'a>(path: PathBuf, lines: Vec<&'a str>) {
        let input_stream =
            futures01::stream::iter_ok::<_, ()>(lines.clone().into_iter().map(|s| s.to_string()));

        UnixStream::connect(&path)
            .map_err(|e| panic!("{:}", e))
            .and_then(|socket| {
                let out =
                    FramedWrite::new(socket, LinesCodec::new()).sink_map_err(|e| panic!("{:?}", e));

                input_stream
                    .forward(out)
                    .map(|(_source, sink)| sink)
                    .and_then(|sink| {
                        let socket = sink.into_inner().into_inner();
                        tokio01::io::shutdown(socket)
                            .map(|_| ())
                            .map_err(|e| panic!("{:}", e))
                    })
            })
            .wait()
            .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn unix_message() {
        let (tx, rx) = mpsc::channel(2);

        let (path, mut rt) = init_unix(tx);

        send_lines_unix(path, vec!["test"]);

        let events = rt.block_on(collect_n(rx, 1)).ok().unwrap();

        assert_eq!(1, events.len());
        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "test".into()
        );
        assert_eq!(
            events[0].as_log()[event::log_schema().source_type_key()],
            "socket".into()
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_multiple_messages() {
        let (tx, rx) = mpsc::channel(10);

        let (path, mut rt) = init_unix(tx);

        send_lines_unix(path, vec!["test\ntest2"]);
        let events = rt.block_on(collect_n(rx, 2)).ok().unwrap();

        assert_eq!(2, events.len());
        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "test".into()
        );
        assert_eq!(
            events[1].as_log()[&event::log_schema().message_key()],
            "test2".into()
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_multiple_packets() {
        let (tx, rx) = mpsc::channel(10);

        let (path, mut rt) = init_unix(tx);

        send_lines_unix(path, vec!["test", "test2"]);
        let events = rt.block_on(collect_n(rx, 2)).ok().unwrap();

        assert_eq!(2, events.len());
        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "test".into()
        );
        assert_eq!(
            events[1].as_log()[&event::log_schema().message_key()],
            "test2".into()
        );
    }
}
