use approx::{__assert_approx, assert_relative_eq, relative_eq};
use futures::{Future, Stream};
use router::sources;
use router::test_util::{next_addr, random_lines, send_lines};
use router::topology::{self, config};
use serde_json::json;
use std::net::SocketAddr;
use stream_cancel::{StreamExt, Tripwire};
use tokio::codec::{FramedRead, LinesCodec};
use tokio::net::TcpListener;

#[test]
fn test_pipe() {
    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut topology = config::Config::empty();
    topology.add_source("in", sources::splunk::TcpConfig { address: in_addr });
    topology.add_sink(
        "out",
        &["in"],
        config::Sink::SplunkTcp { address: out_addr },
    );
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines = output_lines.wait().unwrap();
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

#[test]
fn test_sample() {
    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut topology = config::Config::empty();
    topology.add_source("in", sources::splunk::TcpConfig { address: in_addr });
    topology.add_transform(
        "sampler",
        &["in"],
        config::Transform::Sampler {
            rate: 10,
            pass_list: vec![],
        },
    );
    topology.add_sink(
        "out",
        &["sampler"],
        config::Sink::SplunkTcp { address: out_addr },
    );
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines = output_lines.wait().unwrap();
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
fn test_parse() {
    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut topology = config::Config::empty();
    topology.add_source("in", sources::splunk::TcpConfig { address: in_addr });
    topology.add_transform(
        "parser",
        &["in"],
        config::Transform::RegexParser {
            regex: r"status=(?P<status>\d+)".to_string(),
        },
    );
    topology.add_transform(
        "filter",
        &["parser"],
        config::Transform::FieldFilter {
            field: "status".to_string(),
            value: "404".to_string(),
        },
    );
    topology.add_sink(
        "out",
        &["filter"],
        config::Sink::SplunkTcp { address: out_addr },
    );
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

    let input_lines = vec![
        "good status=200",
        "missing status=404",
        "none foo=bar",
        "blank status=",
    ]
    .into_iter()
    .map(str::to_owned);
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines = output_lines.wait().unwrap();
    assert_eq!(output_lines, vec!["missing status=404".to_owned()]);
}

#[test]
fn test_merge() {
    let num_lines: usize = 10000;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr = next_addr();

    let mut topology = config::Config::empty();
    topology.add_source("in1", sources::splunk::TcpConfig { address: in_addr1 });
    topology.add_source("in2", sources::splunk::TcpConfig { address: in_addr2 });
    topology.add_sink(
        "out",
        &["in1", "in2"],
        config::Sink::SplunkTcp { address: out_addr },
    );
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::net::TcpStream::connect(in_addr1) {}
    while let Err(_) = std::net::TcpStream::connect(in_addr2) {}

    let input_lines1 = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send1 = send_lines(in_addr1, input_lines1.clone().into_iter());
    let send2 = send_lines(in_addr2, input_lines2.clone().into_iter());
    let send = send1.join(send2);
    rt.block_on(send).unwrap();

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines = output_lines.wait().unwrap();
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
fn test_fork() {
    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr1 = next_addr();
    let out_addr2 = next_addr();

    let mut topology = config::Config::empty();
    topology.add_source("in", sources::splunk::TcpConfig { address: in_addr });
    topology.add_sink(
        "out1",
        &["in"],
        config::Sink::SplunkTcp { address: out_addr1 },
    );
    topology.add_sink(
        "out2",
        &["in"],
        config::Sink::SplunkTcp { address: out_addr2 },
    );
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines1 = receive_lines(&out_addr1, &rt.executor());
    let output_lines2 = receive_lines(&out_addr2, &rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines1 = output_lines1.wait().unwrap();
    let output_lines2 = output_lines2.wait().unwrap();
    assert_eq!(num_lines, output_lines1.len());
    assert_eq!(num_lines, output_lines2.len());
    assert_eq!(input_lines, output_lines1);
    assert_eq!(input_lines, output_lines2);
}

#[test]
fn test_merge_and_fork() {
    let num_lines: usize = 10000;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr1 = next_addr();
    let out_addr2 = next_addr();

    // out1 receives both in1 and in2
    // out2 receives in2 only
    let mut topology = config::Config::empty();
    topology.add_source("in1", sources::splunk::TcpConfig { address: in_addr1 });
    topology.add_source("in2", sources::splunk::TcpConfig { address: in_addr2 });
    topology.add_sink(
        "out1",
        &["in1", "in2"],
        config::Sink::SplunkTcp { address: out_addr1 },
    );
    topology.add_sink(
        "out2",
        &["in2"],
        config::Sink::SplunkTcp { address: out_addr2 },
    );
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines1 = receive_lines(&out_addr1, &rt.executor());
    let output_lines2 = receive_lines(&out_addr2, &rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::net::TcpStream::connect(in_addr1) {}
    while let Err(_) = std::net::TcpStream::connect(in_addr2) {}

    let input_lines1 = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send1 = send_lines(in_addr1, input_lines1.clone().into_iter());
    let send2 = send_lines(in_addr2, input_lines2.clone().into_iter());
    let send = send1.join(send2);
    rt.block_on(send).unwrap();

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines1 = output_lines1.wait().unwrap();
    let output_lines2 = output_lines2.wait().unwrap();

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
fn test_merge_and_fork_json() {
    let num_lines: usize = 10000;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr1 = next_addr();
    let out_addr2 = next_addr();

    // out1 receives both in1 and in2
    // out2 receives in2 only
    let config = json!({
        "sources": {
            "in1": {
                "type": "splunk",
                "address": in_addr1,
            },
            "in2": {
                "type": "splunk",
                "address": in_addr2,
            },
        },
        "sinks": {
            "out1": {
                "type": "splunk_tcp",
                "address": out_addr1,
                "inputs": ["in1", "in2"],
            },
            "out2": {
                "type": "splunk_tcp",
                "address": out_addr2,
                "inputs": ["in2"],
            },
        },
    });

    let config = serde_json::to_string_pretty(&config).unwrap();
    println!("{}", config);
    let config: topology::Config = serde_json::from_str(&config).unwrap();

    let (server, trigger, _healthcheck, _warnings) = topology::build(config).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines1 = receive_lines(&out_addr1, &rt.executor());
    let output_lines2 = receive_lines(&out_addr2, &rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::net::TcpStream::connect(in_addr1) {}
    while let Err(_) = std::net::TcpStream::connect(in_addr2) {}

    let input_lines1 = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send1 = send_lines(in_addr1, input_lines1.clone().into_iter());
    let send2 = send_lines(in_addr2, input_lines2.clone().into_iter());
    let send = send1.join(send2);
    rt.block_on(send).unwrap();

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines1 = output_lines1.wait().unwrap();
    let output_lines2 = output_lines2.wait().unwrap();

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
fn test_reconnect() {
    let num_lines: usize = 1000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut topology = config::Config::empty();
    topology.add_source("in", sources::splunk::TcpConfig { address: in_addr });
    topology.add_sink(
        "out",
        &["in"],
        config::Sink::SplunkTcp { address: out_addr },
    );
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();
    let output_rt = tokio::runtime::Runtime::new().unwrap();

    let (output_trigger, output_tripwire) = Tripwire::new();
    let output_listener = TcpListener::bind(&out_addr).unwrap();
    let output_lines = output_listener
        .incoming()
        .take_until(output_tripwire)
        .map(|socket| FramedRead::new(socket, LinesCodec::new()).take(1))
        .flatten()
        .map_err(|e| panic!("{:?}", e))
        .collect();
    let output_lines = futures::sync::oneshot::spawn(output_lines, &output_rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    // Shut down server and wait for it to fully flush
    drop(trigger);
    rt.shutdown_on_idle().wait().unwrap();

    drop(output_trigger);
    output_rt.shutdown_on_idle().wait().unwrap();

    let output_lines = output_lines.wait().unwrap();
    assert!(num_lines >= 2);
    assert!(output_lines.iter().all(|line| input_lines.contains(line)))
}

#[test]
fn test_healthcheck() {
    let addr = next_addr();

    let _listener = TcpListener::bind(&addr).unwrap();

    let healthcheck = router::sinks::splunk::tcp_healthcheck(addr);

    assert!(healthcheck.wait().is_ok());

    let bad_addr = next_addr();
    let bad_healthcheck = router::sinks::splunk::tcp_healthcheck(bad_addr);

    assert!(bad_healthcheck.wait().is_err());
}

fn receive_lines(
    addr: &SocketAddr,
    executor: &tokio::runtime::TaskExecutor,
) -> impl Future<Item = Vec<String>, Error = ()> {
    let listener = TcpListener::bind(addr).unwrap();

    let lines = listener
        .incoming()
        .take(1)
        .map(|socket| FramedRead::new(socket, LinesCodec::new()))
        .flatten()
        .map_err(|e| panic!("{:?}", e))
        .collect();

    futures::sync::oneshot::spawn(lines, executor)
}
