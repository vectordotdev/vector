#![allow(clippy::skip_while_next)]
#![cfg(all(
    feature = "sources-socket",
    feature = "transforms-sampler",
    feature = "sinks-socket"
))]

use approx::assert_relative_eq;
use futures01::{Future, Stream};
use stream_cancel::{StreamExt, Tripwire};
use tokio01::codec::{FramedRead, LinesCodec};
use tokio01::net::TcpListener;
use vector::test_util::{
    block_on, next_addr, random_lines, receive, runtime, send_lines, shutdown_on_idle, wait_for_tcp,
};
use vector::topology::{self, config};
use vector::{sinks, sources, transforms};

#[test]
fn pipe() {
    let num_lines: usize = 10000;

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

    let mut rt = runtime();

    let output_lines = receive(&out_addr);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    // Wait for server to accept traffic
    wait_for_tcp(in_addr);

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    // Shut down server
    block_on(topology.stop()).unwrap();
    shutdown_on_idle(rt);

    let output_lines = output_lines.wait();
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

#[test]
fn sample() {
    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_transform(
        "sampler",
        &["in"],
        transforms::sampler::SamplerConfig {
            rate: 10,
            key_field: None,
            pass_list: vec![],
        },
    );
    config.add_sink(
        "out",
        &["sampler"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );

    let mut rt = runtime();

    let output_lines = receive(&out_addr);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    // Wait for server to accept traffic
    wait_for_tcp(in_addr);

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    // Shut down server
    block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);
    let output_lines = output_lines.wait();
    let num_output_lines = output_lines.len();

    let output_lines_ratio = num_output_lines as f32 / num_lines as f32;
    assert_relative_eq!(output_lines_ratio, 0.1, epsilon = 0.01);

    let mut input_lines = input_lines.into_iter();
    // Assert that all of the output lines were present in the input and in the same order
    for output_line in output_lines {
        let next_line = input_lines
            .by_ref()
            .skip_while(|l| l != &output_line)
            .next();
        assert_eq!(Some(output_line), next_line);
    }
}

#[test]
fn merge() {
    let num_lines: usize = 10000;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in1",
        sources::socket::SocketConfig::make_tcp_config(in_addr1),
    );
    config.add_source(
        "in2",
        sources::socket::SocketConfig::make_tcp_config(in_addr2),
    );
    config.add_sink(
        "out",
        &["in1", "in2"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );

    let mut rt = runtime();

    let output_lines = receive(&out_addr);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    // Wait for server to accept traffic
    wait_for_tcp(in_addr1);
    wait_for_tcp(in_addr2);

    let input_lines1 = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send1 = send_lines(in_addr1, input_lines1.clone().into_iter());
    let send2 = send_lines(in_addr2, input_lines2.clone().into_iter());
    let send = send1.join(send2);
    rt.block_on(send).unwrap();

    // Shut down server
    block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);
    let output_lines = output_lines.wait();
    let num_output_lines = output_lines.len();

    assert_eq!(num_output_lines, num_lines * 2);

    let mut input_lines1 = input_lines1.into_iter().peekable();
    let mut input_lines2 = input_lines2.into_iter().peekable();
    // Assert that all of the output lines were present in the input and in the same order
    for output_line in &output_lines {
        if Some(output_line) == input_lines1.peek() {
            input_lines1.next();
        } else if Some(output_line) == input_lines2.peek() {
            input_lines2.next();
        } else {
            panic!("Got line in output that wasn't in input");
        }
    }
    assert_eq!(input_lines1.next(), None);
    assert_eq!(input_lines2.next(), None);
}

#[test]
fn fork() {
    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr1 = next_addr();
    let out_addr2 = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_sink(
        "out1",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr1.to_string()),
    );
    config.add_sink(
        "out2",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr2.to_string()),
    );

    let mut rt = runtime();

    let output_lines1 = receive(&out_addr1);
    let output_lines2 = receive(&out_addr2);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    // Wait for server to accept traffic
    wait_for_tcp(in_addr);

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    // Shut down server
    block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);
    let output_lines1 = output_lines1.wait();
    let output_lines2 = output_lines2.wait();
    assert_eq!(num_lines, output_lines1.len());
    assert_eq!(num_lines, output_lines2.len());
    assert_eq!(input_lines, output_lines1);
    assert_eq!(input_lines, output_lines2);
}

#[test]
fn merge_and_fork() {
    let num_lines: usize = 10000;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr1 = next_addr();
    let out_addr2 = next_addr();

    // out1 receives both in1 and in2
    // out2 receives in2 only
    let mut config = config::Config::empty();
    config.add_source(
        "in1",
        sources::socket::SocketConfig::make_tcp_config(in_addr1),
    );
    config.add_source(
        "in2",
        sources::socket::SocketConfig::make_tcp_config(in_addr2),
    );
    config.add_sink(
        "out1",
        &["in1", "in2"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr1.to_string()),
    );
    config.add_sink(
        "out2",
        &["in2"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr2.to_string()),
    );

    let mut rt = runtime();

    let output_lines1 = receive(&out_addr1);
    let output_lines2 = receive(&out_addr2);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    // Wait for server to accept traffic
    wait_for_tcp(in_addr1);
    wait_for_tcp(in_addr2);

    let input_lines1 = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send1 = send_lines(in_addr1, input_lines1.clone().into_iter());
    let send2 = send_lines(in_addr2, input_lines2.clone().into_iter());
    let send = send1.join(send2);
    rt.block_on(send).unwrap();

    // Shut down server
    block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);
    let output_lines1 = output_lines1.wait();
    let output_lines2 = output_lines2.wait();

    assert_eq!(num_lines, output_lines2.len());

    assert_eq!(input_lines2, output_lines2);

    assert_eq!(num_lines * 2, output_lines1.len());
    // Assert that all of the output lines were present in the input and in the same order
    let mut input_lines1 = input_lines1.into_iter().peekable();
    let mut input_lines2 = input_lines2.into_iter().peekable();
    for output_line in &output_lines1 {
        if Some(output_line) == input_lines1.peek() {
            input_lines1.next();
        } else if Some(output_line) == input_lines2.peek() {
            input_lines2.next();
        } else {
            panic!("Got line in output that wasn't in input");
        }
    }
    assert_eq!(input_lines1.next(), None);
    assert_eq!(input_lines2.next(), None);
}

#[test]
fn reconnect() {
    let num_lines: usize = 1000;

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

    let mut rt = runtime();
    let output_rt = runtime();

    let (output_trigger, output_tripwire) = Tripwire::new();
    let output_listener = TcpListener::bind(&out_addr).unwrap();
    let output_lines = output_listener
        .incoming()
        .take_until(output_tripwire)
        .map(|socket| FramedRead::new(socket, LinesCodec::new()).take(1))
        .flatten()
        .map_err(|e| panic!("{:?}", e))
        .collect();
    let output_lines = futures01::sync::oneshot::spawn(output_lines, &output_rt.executor());

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    // Wait for server to accept traffic
    wait_for_tcp(in_addr);

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    // Shut down server and wait for it to fully flush
    block_on(topology.stop()).unwrap();
    shutdown_on_idle(rt);

    drop(output_trigger);
    shutdown_on_idle(output_rt);

    let output_lines = output_lines.wait().unwrap();
    assert!(num_lines >= 2);
    assert!(output_lines.iter().all(|line| input_lines.contains(line)))
}

#[test]
fn healthcheck() {
    let addr = next_addr();
    let mut rt = runtime();
    let resolver = vector::dns::Resolver;

    let _listener = TcpListener::bind(&addr).unwrap();

    let healthcheck =
        vector::sinks::util::tcp::tcp_healthcheck(addr.ip().to_string(), addr.port(), resolver);

    assert!(rt.block_on(healthcheck).is_ok());

    let bad_addr = next_addr();
    let bad_healthcheck = vector::sinks::util::tcp::tcp_healthcheck(
        bad_addr.ip().to_string(),
        bad_addr.port(),
        resolver,
    );

    assert!(rt.block_on(bad_healthcheck).is_err());
}
