use std::net::{TcpListener, UdpSocket};

use futures_util::StreamExt;
use tokio::time::{Duration, sleep, timeout};
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::{
    config::Config,
    signal::ShutdownError,
    sinks::socket::SocketSinkConfig,
    sources::socket::{SocketConfig, udp::UdpConfig},
    test_util::{
        CountReceiver,
        addr::next_addr,
        mock::{basic_sink, error_sink, error_source, panic_sink, panic_source},
        random_lines, send_lines, start_topology, trace_init, wait_for_tcp,
    },
};

#[cfg(feature = "sources-syslog")]
use crate::sources::syslog::{Mode as SyslogMode, SyslogConfig};

#[tokio::test]
async fn test_tcp_socket_bind_error_is_reported() {
    trace_init();

    let (_guard, address) = next_addr();
    let _listener = TcpListener::bind(address).unwrap();
    let expected_error = TcpListener::bind(address).unwrap_err().to_string();

    let mut config = Config::builder();
    config.add_source("in", SocketConfig::make_basic_tcp_config(address));
    config.add_sink("out", &["in"], basic_sink(1).1);

    let (topology, mut errors) = start_topology(config.build().unwrap(), false).await;
    let error = timeout(Duration::from_secs(5), errors.recv())
        .await
        .expect("source error timed out")
        .expect("source error missing");
    assert!(matches!(
        error,
        ShutdownError::SourceAborted { error, .. }
            if error.contains("TCP bind failed") && error.contains(&expected_error)
    ));
    topology.stop().await;
}

#[tokio::test]
async fn test_udp_socket_bind_error_is_reported() {
    trace_init();

    let (_guard, address) = next_addr();
    let _socket = UdpSocket::bind(address).unwrap();
    let expected_error = UdpSocket::bind(address).unwrap_err().to_string();

    let mut config = Config::builder();
    config.add_source(
        "in",
        SocketConfig::from(UdpConfig::from_address(address.into())),
    );
    config.add_sink("out", &["in"], basic_sink(1).1);

    let (topology, mut errors) = start_topology(config.build().unwrap(), false).await;
    let error = timeout(Duration::from_secs(5), errors.recv())
        .await
        .expect("source error timed out")
        .expect("source error missing");
    assert!(matches!(
        error,
        ShutdownError::SourceAborted { error, .. } if error == expected_error
    ));
    topology.stop().await;
}

#[cfg(feature = "sources-syslog")]
#[tokio::test]
async fn test_udp_syslog_bind_error_is_reported() {
    trace_init();

    let (_guard, address) = next_addr();
    let _socket = UdpSocket::bind(address).unwrap();
    let expected_error = UdpSocket::bind(address).unwrap_err().to_string();

    let mut config = Config::builder();
    config.add_source(
        "in",
        SyslogConfig::from_mode(SyslogMode::Udp {
            address: address.into(),
            receive_buffer_bytes: None,
        }),
    );
    config.add_sink("out", &["in"], basic_sink(1).1);

    let (topology, mut errors) = start_topology(config.build().unwrap(), false).await;
    let error = timeout(Duration::from_secs(5), errors.recv())
        .await
        .expect("source error timed out")
        .expect("source error missing");
    assert!(matches!(
        error,
        ShutdownError::SourceAborted { error, .. } if error == expected_error
    ));
    topology.stop().await;
}

/// Ensures that an unrelated source completing immediately with an error does not prematurely terminate the topology.
#[tokio::test]
async fn test_source_error() {
    trace_init();

    let num_lines: usize = 10;

    let (_guard_0, in_addr) = next_addr();
    let (_guard_1, out_addr) = next_addr();

    let mut config = Config::builder();
    config.add_source("in", SocketConfig::make_basic_tcp_config(in_addr));
    config.add_source("error", error_source());
    config.add_sink(
        "out",
        &["in", "error"],
        SocketSinkConfig::make_basic_tcp_config(out_addr.to_string(), Default::default()),
    );

    let mut output_lines = CountReceiver::receive_lines(out_addr);

    let (topology, crash) = start_topology(config.build().unwrap(), false).await;

    // Wait for our source to become ready to accept connections, and likewise, wait for our sink's target server to
    // receive its connection from the output sink.
    wait_for_tcp(in_addr).await;
    output_lines.connected().await;

    // Generate 100 random lines, and send them to our source. Wait for a second after that to give time for the
    // topology to process them.
    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    send_lines(in_addr, input_lines.clone()).await.unwrap();
    sleep(Duration::from_secs(1)).await;

    // Our error source should have errored, but since the sink was also pulling from the other source, it should have
    // still been able to get all the events it sent.
    assert!(UnboundedReceiverStream::new(crash).next().await.is_some());
    topology.stop().await;

    let output_lines = output_lines.await;
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

/// Ensures that an unrelated source panicking does not prematurely terminate the topology.
#[tokio::test]
async fn test_source_panic() {
    trace_init();

    let num_lines: usize = 10;

    let (_guard_0, in_addr) = next_addr();
    let (_guard_1, out_addr) = next_addr();

    let mut config = Config::builder();
    config.add_source("in", SocketConfig::make_basic_tcp_config(in_addr));
    config.add_source("panic", panic_source());
    config.add_sink(
        "out",
        &["in", "panic"],
        SocketSinkConfig::make_basic_tcp_config(out_addr.to_string(), Default::default()),
    );

    let mut output_lines = CountReceiver::receive_lines(out_addr);

    std::panic::set_hook(Box::new(|_| {})); // Suppress panic print on background thread
    let (topology, crash) = start_topology(config.build().unwrap(), false).await;

    // Wait for our source to become ready to accept connections, and likewise, wait for our sink's target server to
    // receive its connection from the output sink.
    wait_for_tcp(in_addr).await;
    output_lines.connected().await;

    // Generate 100 random lines, and send them to our source. Wait for a second after that to give time for the
    // topology to process them.
    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    send_lines(in_addr, input_lines.clone()).await.unwrap();
    sleep(Duration::from_secs(1)).await;
    _ = std::panic::take_hook();

    // Our panic source should have panicked, but since the sink was also pulling from the other source, it should have
    // still been able to get all the events it sent.
    assert!(UnboundedReceiverStream::new(crash).next().await.is_some());
    topology.stop().await;

    let output_lines = output_lines.await;
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

/// Ensures that an unrelated sink completing immediately with an error does not prematurely terminate the topology.
#[tokio::test]
async fn test_sink_error() {
    trace_init();

    let num_lines: usize = 10;

    let (_guard_in1, in1_addr) = next_addr();
    let (_guard_in2, in2_addr) = next_addr();
    let (_guard_out, out_addr) = next_addr();

    let mut config = Config::builder();
    config.add_source("in1", SocketConfig::make_basic_tcp_config(in1_addr));
    config.add_source("in2", SocketConfig::make_basic_tcp_config(in2_addr));
    config.add_sink(
        "out",
        &["in1"],
        SocketSinkConfig::make_basic_tcp_config(out_addr.to_string(), Default::default()),
    );
    config.add_sink("error", &["in2"], error_sink());

    let mut output_lines = CountReceiver::receive_lines(out_addr);

    let (topology, crash) = start_topology(config.build().unwrap(), false).await;

    // Wait for our sources to become ready to accept connections, and likewise, wait for our sink's target server to
    // receive its connection from the output sink.
    wait_for_tcp(in1_addr).await;
    wait_for_tcp(in2_addr).await;
    output_lines.connected().await;

    // Generate 100 random lines, and send them to our source. Wait for a second after that to give time for the
    // topology to process them.
    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    send_lines(in1_addr, input_lines.clone()).await.unwrap();
    send_lines(in2_addr, input_lines.clone()).await.unwrap();
    sleep(Duration::from_secs(1)).await;

    // Our error sink should have errored, but the other sink should have still been able to finish processing as it was not
    // directly attached.
    assert!(UnboundedReceiverStream::new(crash).next().await.is_some());
    topology.stop().await;

    let output_lines = output_lines.await;
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

/// Ensures that an unrelated sink panicking does not prematurely terminate the topology.
#[tokio::test]
async fn test_sink_panic() {
    trace_init();

    let num_lines: usize = 10;

    let (_guard_in1, in1_addr) = next_addr();
    let (_guard_in2, in2_addr) = next_addr();
    let (_guard_out, out_addr) = next_addr();

    let mut config = Config::builder();
    config.add_source("in1", SocketConfig::make_basic_tcp_config(in1_addr));
    config.add_source("in2", SocketConfig::make_basic_tcp_config(in2_addr));
    config.add_sink(
        "out",
        &["in1"],
        SocketSinkConfig::make_basic_tcp_config(out_addr.to_string(), Default::default()),
    );
    config.add_sink("panic", &["in2"], panic_sink());

    let mut output_lines = CountReceiver::receive_lines(out_addr);

    std::panic::set_hook(Box::new(|_| {})); // Suppress panic print on background thread
    let (topology, crash) = start_topology(config.build().unwrap(), false).await;

    // Wait for our sources to become ready to accept connections, and likewise, wait for our sink's target server to
    // receive its connection from the output sink.
    wait_for_tcp(in1_addr).await;
    wait_for_tcp(in2_addr).await;
    output_lines.connected().await;

    // Generate 100 random lines, and send them to both of our sources. Wait for a second after that to give time for the
    // topology to process them.
    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    send_lines(in1_addr, input_lines.clone()).await.unwrap();
    send_lines(in2_addr, input_lines.clone()).await.unwrap();
    sleep(Duration::from_secs(1)).await;

    // Our panic sink should have panicked, but the other sink should have still been able to finish processing as it was not
    // directly attached.
    _ = std::panic::take_hook();
    assert!(UnboundedReceiverStream::new(crash).next().await.is_some());
    topology.stop().await;

    let output_lines = output_lines.await;
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}
