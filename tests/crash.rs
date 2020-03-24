#![cfg(all(feature = "sources-socket", feature = "sinks-socket"))]

use futures01::{future, sync::mpsc, Async, AsyncSink, Sink, Stream};
use serde::{Deserialize, Serialize};
use vector::{
    shutdown::ShutdownSignal,
    test_util::{
        block_on, next_addr, random_lines, receive, runtime, send_lines, shutdown_on_idle,
        wait_for_tcp,
    },
    topology::{
        self,
        config::{self, GlobalOptions, SinkContext},
    },
    Event, {sinks, sources},
};

#[derive(Debug, Serialize, Deserialize)]
struct PanicSink;

#[typetag::serde(name = "panic")]
impl config::SinkConfig for PanicSink {
    fn build(
        &self,
        _cx: SinkContext,
    ) -> Result<(sinks::RouterSink, sinks::Healthcheck), vector::Error> {
        Ok((Box::new(PanicSink), Box::new(future::ok(()))))
    }

    fn input_type(&self) -> config::DataType {
        config::DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "panic"
    }
}

impl Sink for PanicSink {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(
        &mut self,
        _item: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        panic!();
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        panic!();
    }
}

#[test]
fn test_sink_panic() {
    let num_lines: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_sink(
        "out",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );
    config.add_sink("panic", &["in"], PanicSink);

    let mut rt = runtime();

    let output_lines = receive(&out_addr);
    std::panic::set_hook(Box::new(|_| {})); // Suppress panic print on background thread
    let (topology, crash) = topology::start(config, &mut rt, false).unwrap();
    // Wait for server to accept traffic
    wait_for_tcp(in_addr);
    std::thread::sleep(std::time::Duration::from_millis(100));

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    let mut rt2 = runtime();
    rt2.block_on(send).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));

    let _ = std::panic::take_hook();
    assert!(crash.wait().next().is_some());
    block_on(topology.stop()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    shutdown_on_idle(rt);

    let output_lines = output_lines.wait();
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

#[derive(Debug, Serialize, Deserialize)]
struct ErrorSink;

#[typetag::serde(name = "panic")]
impl config::SinkConfig for ErrorSink {
    fn build(
        &self,
        _cx: SinkContext,
    ) -> Result<(sinks::RouterSink, sinks::Healthcheck), vector::Error> {
        Ok((Box::new(ErrorSink), Box::new(future::ok(()))))
    }

    fn input_type(&self) -> config::DataType {
        config::DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "panic"
    }
}

impl Sink for ErrorSink {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(
        &mut self,
        _item: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        Err(())
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        Err(())
    }
}

#[test]
fn test_sink_error() {
    let num_lines: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_sink(
        "out",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );
    config.add_sink("error", &["in"], ErrorSink);

    let mut rt = runtime();

    let output_lines = receive(&out_addr);

    let (topology, crash) = topology::start(config, &mut rt, false).unwrap();
    // Wait for server to accept traffic
    wait_for_tcp(in_addr);
    std::thread::sleep(std::time::Duration::from_millis(100));

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    let mut rt2 = runtime();
    rt2.block_on(send).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));

    assert!(crash.wait().next().is_some());
    block_on(topology.stop()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    shutdown_on_idle(rt);

    let output_lines = output_lines.wait();
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

#[derive(Deserialize, Serialize, Debug)]
struct ErrorSourceConfig;

#[typetag::serde(name = "tcp")]
impl config::SourceConfig for ErrorSourceConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        _shutdown: ShutdownSignal,
        _out: mpsc::Sender<Event>,
    ) -> Result<sources::Source, vector::Error> {
        Ok(Box::new(future::err(())))
    }

    fn output_type(&self) -> config::DataType {
        config::DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "tcp"
    }
}

#[test]
fn test_source_error() {
    let num_lines: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_source("error", ErrorSourceConfig);
    config.add_sink(
        "out",
        &["in", "error"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );

    let mut rt = runtime();

    let output_lines = receive(&out_addr);

    let (topology, crash) = topology::start(config, &mut rt, false).unwrap();
    // Wait for server to accept traffic
    wait_for_tcp(in_addr);
    std::thread::sleep(std::time::Duration::from_millis(100));

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    let mut rt2 = runtime();
    rt2.block_on(send).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));

    assert!(crash.wait().next().is_some());
    block_on(topology.stop()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    shutdown_on_idle(rt);

    let output_lines = output_lines.wait();
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

#[derive(Deserialize, Serialize, Debug)]
struct PanicSourceConfig;

#[typetag::serde(name = "tcp")]
impl config::SourceConfig for PanicSourceConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        _shutdown: ShutdownSignal,
        _out: mpsc::Sender<Event>,
    ) -> Result<sources::Source, vector::Error> {
        Ok(Box::new(future::lazy::<_, future::FutureResult<(), ()>>(
            || panic!(),
        )))
    }

    fn output_type(&self) -> config::DataType {
        config::DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "tcp"
    }
}

#[test]
fn test_source_panic() {
    let num_lines: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_source("panic", PanicSourceConfig);
    config.add_sink(
        "out",
        &["in", "panic"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );

    let mut rt = runtime();

    let output_lines = receive(&out_addr);

    std::panic::set_hook(Box::new(|_| {})); // Suppress panic print on background thread
    let (topology, crash) = topology::start(config, &mut rt, false).unwrap();
    // Wait for server to accept traffic
    wait_for_tcp(in_addr);
    std::thread::sleep(std::time::Duration::from_millis(100));

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    let mut rt2 = runtime();
    rt2.block_on(send).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    let _ = std::panic::take_hook();

    assert!(crash.wait().next().is_some());
    block_on(topology.stop()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    shutdown_on_idle(rt);

    let output_lines = output_lines.wait();
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}
