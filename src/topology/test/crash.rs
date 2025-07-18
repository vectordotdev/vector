use crate::{
    config::Config,
    sinks::socket::SocketSinkConfig,
    sources::socket::SocketConfig,
    test_util::{
        mock::{error_sink, error_source, panic_sink, panic_source},
        next_addr, random_lines, send_lines, start_topology, trace_init, wait_for_tcp,
        CountReceiver,
    },
};
use futures_util::StreamExt;
use tokio::time::{sleep, Duration};
use tokio_stream::wrappers::UnboundedReceiverStream;

/// Ensures that an unrelated source completing immediately with an error does not prematurely terminate the topology.
#[tokio::test]
async fn test_source_error() {
    trace_init();

    let num_lines: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

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

    let in_addr = next_addr();
    let out_addr = next_addr();

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

    let in1_addr = next_addr();
    let in2_addr = next_addr();
    let out_addr = next_addr();

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

    let in1_addr = next_addr();
    let in2_addr = next_addr();
    let out_addr = next_addr();

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
