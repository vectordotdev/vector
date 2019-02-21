#![cfg(feature = "flaky")]

use futures::{Future, Sink, Stream};
use router::test_util::{next_addr, random_lines, send_lines};
use router::topology::{self, config};
use router::{
    sinks,
    sources::syslog::{Mode, SyslogConfig},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::codec::{FramedRead, FramedWrite, LinesCodec};
use tokio::net::{TcpListener, UdpSocket};
use tokio_uds::UnixStream;

#[test]
fn test_tcp_syslog() {
    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut topology = config::Config::empty();
    topology.add_source("in", SyslogConfig::new(Mode::Tcp { address: in_addr }));
    topology.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig { address: out_addr },
    );
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

    let input_lines = random_lines(100)
        .enumerate()
        .map(|(id, line)| generate_rfc5424_log_line(id, line))
        .take(num_lines)
        .collect::<Vec<_>>();

    send_lines(in_addr, input_lines.clone().into_iter())
        .wait()
        .unwrap();

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines = output_lines.wait().unwrap();
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

#[test]
fn test_udp_syslog() {
    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut topology = config::Config::empty();
    topology.add_source("in", SyslogConfig::new(Mode::Udp { address: in_addr }));
    topology.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig { address: out_addr },
    );
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);

    let input_lines = random_lines(100)
        .enumerate()
        .map(|(id, line)| generate_rfc5424_log_line(id, line))
        .take(num_lines)
        .collect::<Vec<_>>();

    let bind_addr = next_addr();
    for line in input_lines.iter() {
        let socket = UdpSocket::bind(&bind_addr).unwrap();
        socket.send_dgram(line.as_bytes(), &in_addr).wait().unwrap();
    }

    // Give packets some time to flow through
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines = output_lines.wait().unwrap();
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

#[test]
fn test_unix_stream_syslog() {
    let num_lines: usize = 10000;

    let in_path = tempfile::tempdir().unwrap().into_path().join("stream_test");
    let out_addr = next_addr();

    let mut topology = config::Config::empty();
    topology.add_source(
        "in",
        SyslogConfig::new(Mode::Unix {
            path: in_path.clone(),
        }),
    );
    topology.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig { address: out_addr },
    );
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::os::unix::net::UnixStream::connect(&in_path) {}

    let input_lines = random_lines(100)
        .enumerate()
        .map(|(id, line)| generate_rfc5424_log_line(id, line))
        .take(num_lines)
        .collect::<Vec<_>>();

    let input_stream = futures::stream::iter_ok::<_, ()>(input_lines.clone().into_iter());

    UnixStream::connect(&in_path)
        .map_err(|e| panic!("{:}", e))
        .and_then(|socket| {
            let out =
                FramedWrite::new(socket, LinesCodec::new()).sink_map_err(|e| panic!("{:?}", e));

            input_stream
                .forward(out)
                .map(|(_source, sink)| sink)
                .and_then(|sink| {
                    let socket = sink.into_inner().into_inner();
                    tokio::io::shutdown(socket)
                        .map(|_| ())
                        .map_err(|e| panic!("{:}", e))
                })
        })
        .wait()
        .unwrap();

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines = output_lines.wait().unwrap();
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
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

fn generate_rfc5424_log_line(msg_id: usize, msg: String) -> String {
    let severity = Severity::LOG_INFO;
    let facility = Facility::LOG_USER;
    let hostname = "hogwarts";
    let process = "harry";
    let pid = 42;
    let data = StructuredData::new();

    format!(
        "<{}>{} {} {} {} {} {} {} {}",
        encode_priority(severity, facility),
        1, // version
        chrono::Utc::now().to_rfc3339(),
        hostname,
        process,
        pid,
        msg_id,
        format_5424_structured_data(data),
        msg
    )
}

#[allow(non_camel_case_types)]
#[derive(Copy, Clone)]
pub enum Severity {
    LOG_EMERG,
    LOG_ALERT,
    LOG_CRIT,
    LOG_ERR,
    LOG_WARNING,
    LOG_NOTICE,
    LOG_INFO,
    LOG_DEBUG,
}

#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug)]
pub enum Facility {
    LOG_KERN = 0 << 3,
    LOG_USER = 1 << 3,
    LOG_MAIL = 2 << 3,
    LOG_DAEMON = 3 << 3,
    LOG_AUTH = 4 << 3,
    LOG_SYSLOG = 5 << 3,
}

type StructuredData = HashMap<String, HashMap<String, String>>;

fn format_5424_structured_data(data: StructuredData) -> String {
    if data.is_empty() {
        "-".to_string()
    } else {
        let mut res = String::new();
        for (id, params) in &data {
            res = res + "[" + id;
            for (name, value) in params {
                res = res + " " + name + "=\"" + value + "\"";
            }
            res += "]";
        }

        res
    }
}

fn encode_priority(severity: Severity, facility: Facility) -> u8 {
    facility as u8 | severity as u8
}
