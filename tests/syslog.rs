#![allow(clippy::inherent_to_string)]
#![allow(clippy::redundant_clone)]
#![cfg(all(feature = "sources-syslog", feature = "sinks-socket"))]

use approx::assert_relative_eq;
#[cfg(unix)]
use futures01::{Future, Sink, Stream};
use rand::{thread_rng, Rng};
use serde::Deserialize;
use serde_json::Value;
use sinks::socket::SocketSinkConfig;
use sinks::util::{encoding::EncodingConfig, Encoding};
use std::{collections::HashMap, str::FromStr, thread, time::Duration};
#[cfg(unix)]
use tokio01::codec::{FramedWrite, LinesCodec};
#[cfg(unix)]
use tokio_uds::UnixStream;
use vector::test_util::{
    block_on, next_addr, random_maps, random_string, receive, runtime, send_lines,
    shutdown_on_idle, wait_for_tcp,
};
use vector::topology::{self, config};
use vector::{
    sinks,
    sources::syslog::{Mode, SyslogConfig},
};

#[test]
fn test_tcp_syslog() {
    let num_messages: usize = 10000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in",
        SyslogConfig::new(Mode::Tcp {
            address: in_addr.into(),
            tls: None,
        }),
    );
    config.add_sink("out", &["in"], tcp_json_sink(out_addr.to_string()));

    let mut rt = runtime();

    let output_lines = receive(&out_addr);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    // Wait for server to accept traffic
    wait_for_tcp(in_addr);

    let input_messages: Vec<SyslogMessageRFC5424> = (0..num_messages)
        .map(|i| SyslogMessageRFC5424::random(i, 30, 4, 3, 3))
        .collect();

    let input_lines: Vec<String> = input_messages.iter().map(|msg| msg.to_string()).collect();

    block_on(send_lines(in_addr, input_lines.clone().into_iter())).unwrap();

    // Shut down server
    block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);
    let output_lines = output_lines.wait();
    assert_eq!(output_lines.len(), num_messages);

    let output_messages: Vec<SyslogMessageRFC5424> = output_lines
        .iter()
        .map(|s| {
            let mut value = Value::from_str(s).unwrap();
            value.as_object_mut().unwrap().remove("hostname"); // Vector adds this field which will cause a parse error.
            value.as_object_mut().unwrap().remove("source_ip"); // Vector adds this field which will cause a parse error.
            serde_json::from_value(value).unwrap()
        })
        .collect();
    assert_eq!(output_messages, input_messages);
}

#[test]
fn test_udp_syslog() {
    let num_messages: usize = 1000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source("in", SyslogConfig::new(Mode::Udp { address: in_addr }));
    config.add_sink("out", &["in"], tcp_json_sink(out_addr.to_string()));

    let mut rt = runtime();

    let output_lines = receive(&out_addr);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let input_messages: Vec<SyslogMessageRFC5424> = (0..num_messages)
        .map(|i| SyslogMessageRFC5424::random(i, 30, 4, 3, 3))
        .collect();

    let input_lines: Vec<String> = input_messages.iter().map(|msg| msg.to_string()).collect();

    let bind_addr = next_addr();
    let socket = std::net::UdpSocket::bind(&bind_addr).unwrap();
    for line in input_lines.iter() {
        socket.send_to(line.as_bytes(), &in_addr).unwrap();
        // Space things out slightly to try to avoid dropped packets
        thread::sleep(Duration::from_millis(2));
    }

    // Give packets some time to flow through
    thread::sleep(Duration::from_millis(300));

    // Shut down server
    block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);
    let output_lines = output_lines.wait();

    // Account for some dropped packets :(
    let output_lines_ratio = output_lines.len() as f32 / num_messages as f32;
    assert_relative_eq!(output_lines_ratio, 1.0, epsilon = 0.01);

    let mut output_messages: Vec<SyslogMessageRFC5424> = output_lines
        .iter()
        .map(|s| {
            let mut value = Value::from_str(s).unwrap();
            value.as_object_mut().unwrap().remove("hostname"); // Vector adds this field which will cause a parse error.
            value.as_object_mut().unwrap().remove("source_ip"); // Vector adds this field which will cause a parse error.
            serde_json::from_value(value).unwrap()
        })
        .collect();

    output_messages.sort_by_key(|m| m.timestamp.clone());

    for i in 0..num_messages {
        let x = input_messages[i].clone();
        let y = output_messages[i].clone();
        assert_eq!(y, x);
    }
}

#[cfg(unix)]
#[test]
fn test_unix_stream_syslog() {
    let num_messages: usize = 10000;

    let in_path = tempfile::tempdir().unwrap().into_path().join("stream_test");
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in",
        SyslogConfig::new(Mode::Unix {
            path: in_path.clone(),
        }),
    );
    config.add_sink("out", &["in"], tcp_json_sink(out_addr.to_string()));

    let mut rt = runtime();

    let output_lines = receive(&out_addr);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    // Wait for server to accept traffic
    while std::os::unix::net::UnixStream::connect(&in_path).is_err() {}

    let input_messages: Vec<SyslogMessageRFC5424> = (0..num_messages)
        .map(|i| SyslogMessageRFC5424::random(i, 30, 4, 3, 3))
        .collect();

    let input_lines: Vec<String> = input_messages.iter().map(|msg| msg.to_string()).collect();
    let input_stream = futures01::stream::iter_ok::<_, ()>(input_lines.clone().into_iter());

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
                    // In tokio 0.1 `AsyncWrite::shutdown` for `TcpStream` is a noop.
                    // See https://docs.rs/tokio-tcp/0.1.4/src/tokio_tcp/stream.rs.html#917
                    // Use `TcpStream::shutdown` instead - it actually does something.
                    socket
                        .shutdown(std::net::Shutdown::Both)
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
    assert_eq!(output_lines.len(), num_messages);

    let output_messages: Vec<SyslogMessageRFC5424> = output_lines
        .iter()
        .map(|s| {
            let mut value = Value::from_str(s).unwrap();
            value.as_object_mut().unwrap().remove("hostname"); // Vector adds this field which will cause a parse error.
            value.as_object_mut().unwrap().remove("source_ip"); // Vector adds this field which will cause a parse error.
            serde_json::from_value(value).unwrap()
        })
        .collect();
    assert_eq!(output_messages, input_messages);
}

#[derive(Deserialize, PartialEq, Clone, Debug)]
struct SyslogMessageRFC5424 {
    msgid: String,
    severity: Severity,
    facility: Facility,
    version: u8,
    timestamp: String,
    host: String,
    source_type: String,
    appname: String,
    procid: usize,
    message: String,
    #[serde(flatten)]
    structured_data: StructuredData,
}

impl SyslogMessageRFC5424 {
    fn random(
        id: usize,
        msg_len: usize,
        field_len: usize,
        max_map_size: usize,
        max_children: usize,
    ) -> Self {
        let msg = random_string(msg_len);
        let structured_data = random_structured_data(max_map_size, max_children, field_len);

        let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        //"secfrac" can contain up to 6 digits, but TCP sinks uses `AutoSi`

        Self {
            msgid: format!("test{}", id),
            severity: Severity::LOG_INFO,
            facility: Facility::LOG_USER,
            version: 1,
            timestamp,
            host: "hogwarts".to_owned(),
            source_type: "syslog".to_owned(),
            appname: "harry".to_owned(),
            procid: thread_rng().gen_range(0, 32768),
            structured_data,
            message: msg,
        }
    }

    fn to_string(&self) -> String {
        format!(
            "<{}>{} {} {} {} {} {} {} {}",
            encode_priority(self.severity, self.facility),
            self.version,
            self.timestamp,
            self.host,
            self.appname,
            self.procid,
            self.msgid,
            format_structured_data_rfc5424(&self.structured_data),
            self.message
        )
    }
}

#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Deserialize, PartialEq, Debug)]
pub enum Severity {
    #[serde(rename(deserialize = "emergency"))]
    LOG_EMERG,
    #[serde(rename(deserialize = "alert"))]
    LOG_ALERT,
    #[serde(rename(deserialize = "critical"))]
    LOG_CRIT,
    #[serde(rename(deserialize = "error"))]
    LOG_ERR,
    #[serde(rename(deserialize = "warn"))]
    LOG_WARNING,
    #[serde(rename(deserialize = "notice"))]
    LOG_NOTICE,
    #[serde(rename(deserialize = "info"))]
    LOG_INFO,
    #[serde(rename(deserialize = "debug"))]
    LOG_DEBUG,
}

#[allow(non_camel_case_types)]
#[derive(Copy, Clone, PartialEq, Deserialize, Debug)]
pub enum Facility {
    #[serde(rename(deserialize = "kernel"))]
    LOG_KERN = 0 << 3,
    #[serde(rename(deserialize = "user"))]
    LOG_USER = 1 << 3,
    #[serde(rename(deserialize = "mail"))]
    LOG_MAIL = 2 << 3,
    #[serde(rename(deserialize = "daemon"))]
    LOG_DAEMON = 3 << 3,
    #[serde(rename(deserialize = "auth"))]
    LOG_AUTH = 4 << 3,
    #[serde(rename(deserialize = "syslog"))]
    LOG_SYSLOG = 5 << 3,
}

type StructuredData = HashMap<String, HashMap<String, String>>;

fn random_structured_data(
    max_map_size: usize,
    max_children: usize,
    field_len: usize,
) -> StructuredData {
    let amount = thread_rng().gen_range(0, max_children);

    random_maps(max_map_size, field_len)
        .filter(|m| !m.is_empty()) //syslog_rfc5424 ignores empty maps, tested separately
        .take(amount)
        .enumerate()
        .map(|(i, map)| (format!("id{}", i), map))
        .collect()
}

fn format_structured_data_rfc5424(data: &StructuredData) -> String {
    if data.is_empty() {
        "-".to_string()
    } else {
        let mut res = String::new();
        for (id, params) in data {
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

fn tcp_json_sink(address: String) -> SocketSinkConfig {
    SocketSinkConfig::make_tcp_config(address, EncodingConfig::from(Encoding::Json), None)
}
