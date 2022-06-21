use crate::{
    config::Config,
    sinks::socket::SocketSinkConfig,
    sources::socket::SocketConfig,
    test_util::{
        next_addr, random_lines, send_lines, start_topology, wait_for_tcp, CountReceiver,
        ErrorSinkConfig, ErrorSourceConfig, PanicSinkConfig, PanicSourceConfig,
    },
};
use futures_util::StreamExt;
use tokio::time::{sleep, Duration};
use tokio_stream::wrappers::UnboundedReceiverStream;

/// Ensures that an unrelated source completing immediately with an error does not prematurely terminate the topology.
#[tokio::test]
async fn test_source_error() {
    let num_lines: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = Config::builder();
    config.add_source("in", SocketConfig::make_basic_tcp_config(in_addr));
    config.add_source("error", ErrorSourceConfig::default());
    config.add_sink(
        "out",
        &["in", "error"],
        SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );

    let mut output_lines = CountReceiver::receive_lines(out_addr);

    let (topology, crash) = start_topology(config.build().unwrap(), false).await;
    // Wait for server to accept traffic
    wait_for_tcp(in_addr).await;
    sleep(Duration::from_millis(100)).await;

    // Wait for output to connect
    output_lines.connected().await;

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    send_lines(in_addr, input_lines.clone()).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    assert!(UnboundedReceiverStream::new(crash).next().await.is_some());
    topology.stop().await;
    sleep(Duration::from_millis(100)).await;

    let output_lines = output_lines.await;
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

/// Ensures that an unrelated source panicking does not prematurely terminate the topology.
#[tokio::test]
async fn test_source_panic() {
    let num_lines: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = Config::builder();
    config.add_source("in", SocketConfig::make_basic_tcp_config(in_addr));
    config.add_source("panic", PanicSourceConfig::default());
    config.add_sink(
        "out",
        &["in", "panic"],
        SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );

    let mut output_lines = CountReceiver::receive_lines(out_addr);

    std::panic::set_hook(Box::new(|_| {})); // Suppress panic print on background thread
    let (topology, crash) = start_topology(config.build().unwrap(), false).await;
    // Wait for server to accept traffic
    wait_for_tcp(in_addr).await;
    sleep(Duration::from_millis(100)).await;

    // Wait for output to connect
    output_lines.connected().await;

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    send_lines(in_addr, input_lines.clone()).await.unwrap();
    sleep(Duration::from_millis(100)).await;
    let _ = std::panic::take_hook();

    assert!(UnboundedReceiverStream::new(crash).next().await.is_some());
    topology.stop().await;
    sleep(Duration::from_millis(100)).await;

    let output_lines = output_lines.await;
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}
