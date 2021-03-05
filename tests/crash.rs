#![allow(clippy::type_complexity)]
#![cfg(all(feature = "sources-socket", feature = "sinks-socket"))]

use async_trait::async_trait;
use futures::{future, FutureExt, Sink, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::time::{delay_for, Duration};
use vector::{
    config::{self, GlobalOptions, SinkConfig, SinkContext, SourceConfig},
    shutdown::ShutdownSignal,
    test_util::{next_addr, random_lines, send_lines, start_topology, wait_for_tcp, CountReceiver},
    Event, Pipeline,
    {
        sinks::{self, Healthcheck, VectorSink},
        sources,
    },
};

#[derive(Debug, Serialize, Deserialize)]
struct PanicSink;

#[async_trait]
#[typetag::serde(name = "panic")]
impl SinkConfig for PanicSink {
    async fn build(&self, _cx: SinkContext) -> Result<(VectorSink, Healthcheck), vector::Error> {
        Ok((
            VectorSink::Sink(Box::new(PanicSink)),
            future::ok(()).boxed(),
        ))
    }

    fn input_type(&self) -> config::DataType {
        config::DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "panic"
    }
}

impl Sink<Event> for PanicSink {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        panic!()
    }

    fn start_send(self: Pin<&mut Self>, _item: Event) -> Result<(), Self::Error> {
        panic!()
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        panic!()
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        panic!()
    }
}

#[tokio::test]
async fn test_sink_panic() {
    let num_lines: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::builder();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
    );
    config.add_sink(
        "out",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );
    config.add_sink("panic", &["in"], PanicSink);

    let mut output_lines = CountReceiver::receive_lines(out_addr);

    std::panic::set_hook(Box::new(|_| {})); // Suppress panic print on background thread
    let (topology, mut crash) = start_topology(config.build().unwrap(), false).await;
    // Wait for server to accept traffic
    wait_for_tcp(in_addr).await;
    delay_for(Duration::from_millis(100)).await;

    // Wait for output to connect
    output_lines.connected().await;

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    send_lines(in_addr, input_lines.clone()).await.unwrap();
    delay_for(Duration::from_millis(100)).await;

    let _ = std::panic::take_hook();
    assert!(crash.next().await.is_some());
    topology.stop().await;
    delay_for(Duration::from_millis(100)).await;

    let output_lines = output_lines.await;
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

#[derive(Debug, Serialize, Deserialize)]
struct ErrorSink;

#[async_trait]
#[typetag::serde(name = "panic")]
impl SinkConfig for ErrorSink {
    async fn build(&self, _cx: SinkContext) -> Result<(VectorSink, Healthcheck), vector::Error> {
        Ok((
            VectorSink::Sink(Box::new(ErrorSink)),
            future::ok(()).boxed(),
        ))
    }

    fn input_type(&self) -> config::DataType {
        config::DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "panic"
    }
}

impl Sink<Event> for ErrorSink {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Err(()))
    }

    fn start_send(self: Pin<&mut Self>, _item: Event) -> Result<(), Self::Error> {
        Err(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Err(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Err(()))
    }
}

#[tokio::test]
async fn test_sink_error() {
    let num_lines: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::builder();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
    );
    config.add_sink(
        "out",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );
    config.add_sink("error", &["in"], ErrorSink);

    let mut output_lines = CountReceiver::receive_lines(out_addr);

    let (topology, mut crash) = start_topology(config.build().unwrap(), false).await;
    // Wait for server to accept traffic
    wait_for_tcp(in_addr).await;
    delay_for(Duration::from_millis(100)).await;

    // Wait for output to connect
    output_lines.connected().await;

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    send_lines(in_addr, input_lines.clone()).await.unwrap();
    delay_for(Duration::from_millis(100)).await;

    assert!(crash.next().await.is_some());
    topology.stop().await;
    delay_for(Duration::from_millis(100)).await;

    let output_lines = output_lines.await;
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

#[derive(Deserialize, Serialize, Debug)]
struct ErrorSourceConfig;

#[async_trait]
#[typetag::serde(name = "tcp")]
impl SourceConfig for ErrorSourceConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        _shutdown: ShutdownSignal,
        _out: Pipeline,
    ) -> Result<sources::Source, vector::Error> {
        Ok(Box::pin(future::err(())))
    }

    fn output_type(&self) -> config::DataType {
        config::DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "tcp"
    }
}

#[tokio::test]
async fn test_source_error() {
    let num_lines: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::builder();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
    );
    config.add_source("error", ErrorSourceConfig);
    config.add_sink(
        "out",
        &["in", "error"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );

    let mut output_lines = CountReceiver::receive_lines(out_addr);

    let (topology, mut crash) = start_topology(config.build().unwrap(), false).await;
    // Wait for server to accept traffic
    wait_for_tcp(in_addr).await;
    delay_for(Duration::from_millis(100)).await;

    // Wait for output to connect
    output_lines.connected().await;

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    send_lines(in_addr, input_lines.clone()).await.unwrap();
    delay_for(Duration::from_millis(100)).await;

    assert!(crash.next().await.is_some());
    topology.stop().await;
    delay_for(Duration::from_millis(100)).await;

    let output_lines = output_lines.await;
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

#[derive(Deserialize, Serialize, Debug)]
struct PanicSourceConfig;

#[async_trait]
#[typetag::serde(name = "tcp")]
impl SourceConfig for PanicSourceConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        _shutdown: ShutdownSignal,
        _out: Pipeline,
    ) -> Result<sources::Source, vector::Error> {
        Ok(Box::pin(async { panic!() }))
    }

    fn output_type(&self) -> config::DataType {
        config::DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "tcp"
    }
}

#[tokio::test]
async fn test_source_panic() {
    let num_lines: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::builder();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
    );
    config.add_source("panic", PanicSourceConfig);
    config.add_sink(
        "out",
        &["in", "panic"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );

    let mut output_lines = CountReceiver::receive_lines(out_addr);

    std::panic::set_hook(Box::new(|_| {})); // Suppress panic print on background thread
    let (topology, mut crash) = start_topology(config.build().unwrap(), false).await;
    // Wait for server to accept traffic
    wait_for_tcp(in_addr).await;
    delay_for(Duration::from_millis(100)).await;

    // Wait for output to connect
    output_lines.connected().await;

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    send_lines(in_addr, input_lines.clone()).await.unwrap();
    delay_for(Duration::from_millis(100)).await;
    let _ = std::panic::take_hook();

    assert!(crash.next().await.is_some());
    topology.stop().await;
    delay_for(Duration::from_millis(100)).await;

    let output_lines = output_lines.await;
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}
