use approx::assert_relative_eq;
use futures::{Future, Sink, Stream};
use std::{collections::HashMap, thread, time::Duration};
use tokio::codec::{FramedWrite, LinesCodec};
use tokio_uds::UnixStream;
use vector::test_util::{
    block_on, next_addr, random_lines, receive, send_lines, shutdown_on_idle, wait_for_tcp,
};
use vector::topology::{self, config};
use vector::{
    sinks,
    sources::syslog::{Mode, SyslogConfig},
};

#[test]
fn test_tcp_syslog() {
    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source("in", SyslogConfig::new(Mode::Tcp { address: in_addr }));
    config.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
    );

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive(&out_addr);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    // Wait for server to accept traffic
    wait_for_tcp(in_addr);

    let input_lines = random_lines(100)
        .enumerate()
        .map(|(id, line)| generate_rfc5424_log_line(id, line))
        .take(num_lines)
        .collect::<Vec<_>>();

    block_on(send_lines(in_addr, input_lines.clone().into_iter())).unwrap();

    // Shut down server
    block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);
    let output_lines = output_lines.wait();
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

#[test]
fn test_udp_syslog() {
    let num_lines: usize = 1000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source("in", SyslogConfig::new(Mode::Udp { address: in_addr }));
    config.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
    );

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive(&out_addr);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let input_lines = random_lines(100)
        .enumerate()
        .map(|(id, line)| generate_rfc5424_log_line(id, line))
        .take(num_lines)
        .collect::<Vec<_>>();

    let bind_addr = next_addr();
    let socket = std::net::UdpSocket::bind(&bind_addr).unwrap();
    for line in input_lines.iter() {
        socket.send_to(line.as_bytes(), &in_addr).unwrap();
        // Space things out slightly to try to avoid dropped packets
        thread::sleep(Duration::from_nanos(200_000));
    }

    // Give packets some time to flow through
    thread::sleep(Duration::from_millis(10));

    // Shut down server
    block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);
    let output_lines = output_lines.wait();

    // Account for some dropped packets :(
    let output_lines_ratio = output_lines.len() as f32 / num_lines as f32;
    assert_relative_eq!(output_lines_ratio, 1.0, epsilon = 0.01);
    for line in output_lines {
        assert!(input_lines.contains(&line));
    }
}

#[test]
fn test_unix_stream_syslog() {
    let num_lines: usize = 10000;

    let in_path = tempfile::tempdir().unwrap().into_path().join("stream_test");
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in",
        SyslogConfig::new(Mode::Unix {
            path: in_path.clone(),
        }),
    );
    config.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
    );

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive(&out_addr);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
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
    block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);
    let output_lines = output_lines.wait();
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
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
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
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
