use crate::record::Record;
use bytes::Bytes;
use chrono::TimeZone;
use derive_is_enum_variant::is_enum_variant;
use futures::{future, sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};
use tokio::{
    self,
    codec::{BytesCodec, FramedRead, LinesCodec},
    net::{TcpListener, UdpFramed, UdpSocket},
};
use tokio_uds::UnixListener;

#[derive(Deserialize, Serialize, Debug)]
// TODO: add back when serde-rs/serde#1358 is addressed
// #[serde(deny_unknown_fields)]
pub struct SyslogConfig {
    #[serde(flatten)]
    pub mode: Mode,
    #[serde(default = "default_max_length")]
    pub max_length: usize,
}

#[derive(Deserialize, Serialize, Debug, Clone, is_enum_variant)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Tcp { address: SocketAddr },
    Udp { address: SocketAddr },
    Unix { path: PathBuf },
}

fn default_max_length() -> usize {
    100 * 1024
}

impl SyslogConfig {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            max_length: default_max_length(),
        }
    }
}

#[typetag::serde(name = "syslog")]
impl crate::topology::config::SourceConfig for SyslogConfig {
    fn build(&self, out: mpsc::Sender<Record>) -> Result<super::Source, String> {
        match self.mode.clone() {
            Mode::Tcp { address } => Ok(tcp(address, self.max_length, out)),
            Mode::Udp { address } => Ok(udp(address, self.max_length, out)),
            Mode::Unix { path } => Ok(unix(path, self.max_length, out)),
        }
    }
}

pub fn tcp(addr: SocketAddr, max_length: usize, out: mpsc::Sender<Record>) -> super::Source {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    Box::new(future::lazy(move || {
        let listener = TcpListener::bind(&addr).expect("failed to bind to tcp listener socket");

        info!("listening on tcp {:?}", listener.local_addr());

        listener
            .incoming()
            .map_err(|e| error!("failed to accept socket; error = {:?}", e))
            .for_each(move |socket| {
                let out = out.clone();

                let lines_in = FramedRead::new(socket, LinesCodec::new_with_max_length(max_length))
                    .filter_map(record_from_str)
                    .map_err(|e| error!("error reading line: {:?}", e));

                let handler = lines_in.forward(out).map(|_| info!("finished sending"));

                tokio::spawn(handler)
            })
    }))
}

pub fn udp(addr: SocketAddr, _max_length: usize, out: mpsc::Sender<Record>) -> super::Source {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    Box::new(
        future::lazy(move || {
            let socket = UdpSocket::bind(&addr).expect("failed to bind to udp listener socket");

            info!("listening on {:?}", socket.local_addr());

            future::ok(socket)
        })
        .and_then(|socket| {
            let lines_in = UdpFramed::new(socket, BytesCodec::new())
                .filter_map(|(bytes, _sock)| record_from_bytes(&bytes))
                .map_err(|e| error!("error reading line: {:?}", e));

            lines_in.forward(out).map(|_| info!("finished sending"))
        }),
    )
}

pub fn unix(path: PathBuf, max_length: usize, out: mpsc::Sender<Record>) -> super::Source {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    Box::new(future::lazy(move || {
        let listener = UnixListener::bind(&path).expect("failed to bind to listener socket");

        info!("listening on {:?}", listener.local_addr());

        listener
            .incoming()
            .map_err(|e| error!("failed to accept socket; error = {:?}", e))
            .for_each(move |socket| {
                let out = out.clone();

                let lines_in = FramedRead::new(socket, LinesCodec::new_with_max_length(max_length))
                    .filter_map(record_from_str)
                    .map_err(|e| error!("error reading line: {:?}", e));

                let handler = lines_in.forward(out).map(|_| info!("finished sending"));

                tokio::spawn(handler)
            })
    }))
}

fn record_from_bytes(bytes: &[u8]) -> Option<Record> {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|s| record_from_str(s))
}

// TODO: many more cases to handle:
// handle parse errors instead of discarding
// non-strict rfc5424 parsing (see ignored tests)
// octet framing (i.e. num bytes as ascii string prefix) with and without delimiters
// null byte delimiter in place of newline

fn record_from_str(raw: impl AsRef<str>) -> Option<Record> {
    let line = raw.as_ref().trim();
    syslog_rfc5424::parse_message(line)
        .map(|parsed| Record {
            raw: Bytes::from(line.as_bytes()),
            timestamp: parsed
                .timestamp
                .map(|ts| chrono::Utc.timestamp(ts, parsed.timestamp_nanos.unwrap_or(0) as u32)),
            ..Default::default()
        })
        .ok()
}

#[cfg(test)]
mod test {
    use super::{record_from_str, SyslogConfig};
    use crate::record::Record;
    use chrono::TimeZone;

    #[test]
    fn config() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "tcp"
            address = "127.0.0.1:1235"
          "#,
        )
        .unwrap();
        assert!(config.mode.is_tcp());

        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "udp"
            address = "127.0.0.1:1235"
            max_length = 32187
          "#,
        )
        .unwrap();
        assert!(config.mode.is_udp());

        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "unix"
            path = "127.0.0.1:1235"
          "#,
        )
        .unwrap();
        assert!(config.mode.is_unix());
    }

    #[test]
    fn syslog_ng_network_syslog_protocol() {
        // this should also match rsyslog omfwd with template=RSYSLOG_SyslogProtocol23Format
        let raw = r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar"#;
        assert_eq!(
            Record {
                line: raw.to_owned(),
                timestamp: chrono::Utc.ymd(2019, 2, 13).and_hms(19, 48, 34),
                custom: Default::default(),
                host: Some(String::from("74794bfb6795")),
            },
            record_from_str(raw).unwrap()
        );
    }

    #[test]
    fn handles_weird_whitespace() {
        // this should also match rsyslog omfwd with template=RSYSLOG_SyslogProtocol23Format
        let raw = r#"
            <13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar
            "#;
        let cleaned = r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar"#;
        assert_eq!(
            Record {
                line: cleaned.to_owned(),
                timestamp: chrono::Utc.ymd(2019, 2, 13).and_hms(19, 48, 34),
                custom: Default::default(),
                host: Some(String::from("74794bfb6795")),
            },
            record_from_str(raw).unwrap()
        );
    }

    #[test]
    #[ignore]
    fn syslog_ng_default_network() {
        let raw = r#"<13>Feb 13 20:07:26 74794bfb6795 root[8539]: i am foobar"#;
        assert_eq!(
            Record {
                line: raw.to_owned(),
                timestamp: chrono::Utc.ymd(2019, 2, 13).and_hms(20, 7, 26),
                custom: Default::default(),
                host: Some(String::from("74794bfb6795")),
            },
            record_from_str(raw).unwrap()
        );
    }

    #[test]
    #[ignore]
    fn rsyslog_omfwd_tcp_default() {
        let raw = r#"<190>Feb 13 21:31:56 74794bfb6795 liblogging-stdlog:  [origin software="rsyslogd" swVersion="8.24.0" x-pid="8979" x-info="http://www.rsyslog.com"] start"#;
        assert_eq!(
            Record {
                line: raw.to_owned(),
                timestamp: chrono::Utc.ymd(2019, 2, 13).and_hms(21, 31, 56),
                custom: Default::default(),
                host: Some(String::from("74794bfb6795")),
            },
            record_from_str(raw).unwrap()
        );
    }

    #[test]
    #[ignore]
    fn rsyslog_omfwd_tcp_forward_format() {
        let raw = r#"<190>2019-02-13T21:53:30.605850+00:00 74794bfb6795 liblogging-stdlog:  [origin software="rsyslogd" swVersion="8.24.0" x-pid="9043" x-info="http://www.rsyslog.com"] start"#;
        assert_eq!(
            Record {
                line: raw.to_owned(),
                timestamp: chrono::Utc.ymd(2019, 2, 13).and_hms(21, 53, 30),
                custom: Default::default(),
                host: Some(String::from("74794bfb6795")),
            },
            record_from_str(raw).unwrap()
        );
    }
}
