use crate::event::{self, Event};
use chrono::{TimeZone, Utc};
use derive_is_enum_variant::is_enum_variant;
use futures::{future, sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};
use tokio::{
    self,
    codec::{BytesCodec, FramedRead, LinesCodec},
    net::{TcpListener, UdpFramed, UdpSocket},
};
use tokio_trace::field;
use tokio_trace_futures::Instrument;
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
    fn build(&self, out: mpsc::Sender<Event>) -> Result<super::Source, String> {
        match self.mode.clone() {
            Mode::Tcp { address } => Ok(tcp(address, self.max_length, out)),
            Mode::Udp { address } => Ok(udp(address, self.max_length, out)),
            Mode::Unix { path } => Ok(unix(path, self.max_length, out)),
        }
    }
}

pub fn tcp(addr: SocketAddr, max_length: usize, out: mpsc::Sender<Event>) -> super::Source {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    Box::new(future::lazy(move || {
        let listener = TcpListener::bind(&addr).expect("failed to bind to tcp listener socket");

        info!(
            message = "listening.",
            addr = &field::display(addr),
            r#type = "tcp"
        );

        listener
            .incoming()
            .map_err(|e| error!("failed to accept socket; error = {}", e))
            .for_each(move |socket| {
                let out = out.clone();
                let peer_addr = socket.peer_addr().ok().map(|s| s.ip());

                let span = info_span!("connection");

                if let Some(addr) = peer_addr {
                    span.record("peer_addr", &field::display(&addr));
                }

                let lines_in = FramedRead::new(socket, LinesCodec::new_with_max_length(max_length))
                    .filter_map(record_from_str)
                    .map_err(|e| error!("error reading line: {:?}", e));

                let handler = lines_in.forward(out).map(|_| info!("finished sending"));

                tokio::spawn(handler.instrument(span))
            })
    }))
}

pub fn udp(addr: SocketAddr, _max_length: usize, out: mpsc::Sender<Event>) -> super::Source {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    Box::new(
        future::lazy(move || {
            let socket = UdpSocket::bind(&addr).expect("failed to bind to udp listener socket");

            info!(
                message = "listening.",
                addr = &field::display(addr),
                r#type = "udp"
            );

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

pub fn unix(path: PathBuf, max_length: usize, out: mpsc::Sender<Event>) -> super::Source {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    Box::new(future::lazy(move || {
        let listener = UnixListener::bind(&path).expect("failed to bind to listener socket");

        info!(
            message = "listening.",
            path = &field::debug(path),
            r#type = "unix"
        );

        listener
            .incoming()
            .map_err(|e| error!("failed to accept socket; error = {:?}", e))
            .for_each(move |socket| {
                let out = out.clone();
                let peer_addr = socket.peer_addr().ok();

                let span = info_span!("connection");
                if let Some(addr) = &peer_addr {
                    if let Some(path) = addr.as_pathname() {
                        span.record("peer_path", &field::debug(&path));
                    }
                }

                let lines_in = FramedRead::new(socket, LinesCodec::new_with_max_length(max_length))
                    .filter_map(record_from_str)
                    .map_err(|e| error!("error reading line: {:?}", e));

                let handler = lines_in.forward(out).map(|_| info!("finished sending"));

                tokio::spawn(handler.instrument(span))
            })
    }))
}

fn record_from_bytes(bytes: &[u8]) -> Option<Event> {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|s| record_from_str(s))
}

// TODO: many more cases to handle:
// handle parse errors instead of discarding
// non-strict rfc5424 parsing (see ignored tests)
// octet framing (i.e. num bytes as ascii string prefix) with and without delimiters
// null byte delimiter in place of newline

fn record_from_str(raw: impl AsRef<str>) -> Option<Event> {
    let line = raw.as_ref();
    trace!(
        message = "Received line.",
        bytes = &field::display(line.len())
    );

    let line = line.trim();
    syslog_rfc5424::parse_message(line)
        .map(|parsed| {
            let mut record = Event::from(line);

            if let Some(host) = &parsed.hostname {
                record.insert_implicit("host".into(), host.clone().into());
            }

            let timestamp = parsed
                .timestamp
                .map(|ts| Utc.timestamp(ts, parsed.timestamp_nanos.unwrap_or(0) as u32))
                .unwrap_or(Utc::now());
            record.insert_implicit(event::TIMESTAMP.clone(), timestamp.into());

            trace!(
                message = "processing one record.",
                record = &field::debug(&record)
            );

            record
        })
        .ok()
}

#[cfg(test)]
mod test {
    use super::{record_from_str, SyslogConfig};
    use crate::event::{self, Event};
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

        let mut expected = Event::from(raw);
        expected.insert_implicit(
            event::TIMESTAMP.clone(),
            chrono::Utc.ymd(2019, 2, 13).and_hms(19, 48, 34).into(),
        );
        expected.insert_implicit("host".into(), "74794bfb6795".into());

        assert_eq!(expected, record_from_str(raw).unwrap());
    }

    #[test]
    fn handles_weird_whitespace() {
        // this should also match rsyslog omfwd with template=RSYSLOG_SyslogProtocol23Format
        let raw = r#"
            <13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar
            "#;
        let cleaned = r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar"#;

        let mut expected = Event::from(cleaned);
        expected.insert_implicit(
            event::TIMESTAMP.clone(),
            chrono::Utc.ymd(2019, 2, 13).and_hms(19, 48, 34).into(),
        );
        expected.insert_implicit("host".into(), "74794bfb6795".into());

        assert_eq!(expected, record_from_str(raw).unwrap());
    }

    #[test]
    #[ignore]
    fn syslog_ng_default_network() {
        let raw = r#"<13>Feb 13 20:07:26 74794bfb6795 root[8539]: i am foobar"#;

        let mut expected = Event::from(raw);
        expected.insert_implicit(
            event::TIMESTAMP.clone(),
            chrono::Utc.ymd(2019, 2, 13).and_hms(20, 7, 26).into(),
        );
        expected.insert_implicit("host".into(), "74794bfb6795".into());

        assert_eq!(expected, record_from_str(raw).unwrap());
    }

    #[test]
    #[ignore]
    fn rsyslog_omfwd_tcp_default() {
        let raw = r#"<190>Feb 13 21:31:56 74794bfb6795 liblogging-stdlog:  [origin software="rsyslogd" swVersion="8.24.0" x-pid="8979" x-info="http://www.rsyslog.com"] start"#;

        let mut expected = Event::from(raw);
        expected.insert_implicit(
            event::TIMESTAMP.clone(),
            chrono::Utc.ymd(2019, 2, 13).and_hms(21, 31, 56).into(),
        );
        expected.insert_implicit("host".into(), "74794bfb6795".into());

        assert_eq!(expected, record_from_str(raw).unwrap());
    }

    #[test]
    #[ignore]
    fn rsyslog_omfwd_tcp_forward_format() {
        let raw = r#"<190>2019-02-13T21:53:30.605850+00:00 74794bfb6795 liblogging-stdlog:  [origin software="rsyslogd" swVersion="8.24.0" x-pid="9043" x-info="http://www.rsyslog.com"] start"#;

        let mut expected = Event::from(raw);
        expected.insert_implicit(
            event::TIMESTAMP.clone(),
            chrono::Utc.ymd(2019, 2, 13).and_hms(21, 53, 30).into(),
        );
        expected.insert_implicit("host".into(), "74794bfb6795".into());

        assert_eq!(expected, record_from_str(raw).unwrap());
    }
}
