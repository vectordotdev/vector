use super::util::TcpSource;
use crate::{
    event::{self, Event},
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use derive_is_enum_variant::is_enum_variant;
use futures::{future, sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};
use tokio::{
    self,
    codec::{BytesCodec, FramedRead, LinesCodec},
    net::{UdpFramed, UdpSocket},
};
use tokio_uds::UnixListener;
use tracing::field;
use tracing_futures::Instrument;

#[derive(Deserialize, Serialize, Debug)]
// TODO: add back when serde-rs/serde#1358 is addressed
// #[serde(deny_unknown_fields)]
pub struct SyslogConfig {
    #[serde(flatten)]
    pub mode: Mode,
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    pub host_key: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, is_enum_variant)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Tcp { address: SocketAddr },
    Udp { address: SocketAddr },
    Unix { path: PathBuf },
}

fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
}

impl SyslogConfig {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            host_key: None,
            max_length: default_max_length(),
        }
    }
}

#[typetag::serde(name = "syslog")]
impl SourceConfig for SyslogConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> Result<super::Source, String> {
        let host_key = self.host_key.clone().unwrap_or(event::HOST.to_string());

        match self.mode.clone() {
            Mode::Tcp { address } => {
                let source = SyslogTcpSource {
                    max_length: self.max_length,
                    host_key,
                };
                let shutdown_secs = 30;
                source.run(address, shutdown_secs, out)
            }
            Mode::Udp { address } => Ok(udp(address, self.max_length, host_key, out)),
            Mode::Unix { path } => Ok(unix(path, self.max_length, host_key, out)),
        }
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

#[derive(Debug, Clone)]
struct SyslogTcpSource {
    max_length: usize,
    host_key: String,
}

impl TcpSource for SyslogTcpSource {
    type Decoder = LinesCodec;

    fn decoder(&self) -> Self::Decoder {
        LinesCodec::new_with_max_length(self.max_length)
    }

    fn build_event(&self, frame: String, host: Option<Bytes>) -> Option<Event> {
        event_from_str(&self.host_key, host, frame).map(|event| {
            trace!(
                message = "Received one event.",
                event = field::debug(&event)
            );

            event
        })
    }
}

pub fn udp(
    addr: SocketAddr,
    _max_length: usize,
    host_key: String,
    out: mpsc::Sender<Event>,
) -> super::Source {
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
        .and_then(move |socket| {
            let host_key = host_key.clone();

            let lines_in = UdpFramed::new(socket, BytesCodec::new())
                .filter_map(move |(bytes, addr)| {
                    let host_key = host_key.clone();
                    event_from_bytes(&host_key, &bytes).map(|mut e| {
                        e.as_mut_log()
                            .insert_implicit(host_key.into(), addr.to_string().into());
                        e
                    })
                })
                .map_err(|e| error!("error reading line: {:?}", e));

            lines_in.forward(out).map(|_| info!("finished sending"))
        }),
    )
}

pub fn unix(
    path: PathBuf,
    max_length: usize,
    host_key: String,
    out: mpsc::Sender<Event>,
) -> super::Source {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    Box::new(future::lazy(move || {
        let listener = UnixListener::bind(&path).expect("failed to bind to listener socket");

        info!(message = "listening.", ?path, r#type = "unix");

        listener
            .incoming()
            .map_err(|e| error!("failed to accept socket; error = {:?}", e))
            .for_each(move |socket| {
                let out = out.clone();
                let peer_addr = socket.peer_addr().ok();
                let host_key = host_key.clone();

                let span = info_span!("connection");
                let path = if let Some(addr) = peer_addr.clone() {
                    if let Some(path) = addr.as_pathname().map(|e| e.to_owned()) {
                        span.record("peer_path", &field::debug(&path));
                        Some(path.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };

                let host_key2 = host_key.clone();
                let lines_in = FramedRead::new(socket, LinesCodec::new_with_max_length(max_length))
                    .filter_map(move |event| event_from_str(&host_key.clone(), None, event))
                    .map(move |mut e| {
                        if let Some(path) = &path {
                            e.as_mut_log().insert_implicit(
                                host_key2.clone().into(),
                                path.to_string_lossy().into_owned().into(),
                            );
                        }
                        e
                    })
                    .map_err(|e| error!("error reading line: {:?}", e));

                let handler = lines_in.forward(out).map(|_| info!("finished sending"));

                tokio::spawn(handler.instrument(span))
            })
    }))
}

fn event_from_bytes(host_key: &String, bytes: &[u8]) -> Option<Event> {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|s| event_from_str(host_key, None, s))
}

// TODO: many more cases to handle:
// handle parse errors instead of discarding
// non-strict rfc5424 parsing (see ignored tests)
// octet framing (i.e. num bytes as ascii string prefix) with and without delimiters
// null byte delimiter in place of newline

fn event_from_str(host_key: &String, host: Option<Bytes>, raw: impl AsRef<str>) -> Option<Event> {
    let line = raw.as_ref();
    trace!(
        message = "Received line.",
        bytes = &field::display(line.len())
    );

    let line = line.trim();
    syslog_rfc5424::parse_message(line)
        .map(|parsed| {
            let mut event = Event::from(line);

            if let Some(host) = &parsed.hostname {
                event
                    .as_mut_log()
                    .insert_implicit(host_key.clone().into(), host.clone().into());
            } else if let Some(host) = host {
                event
                    .as_mut_log()
                    .insert_implicit(host_key.clone().into(), host.into());
            }

            let timestamp = parsed
                .timestamp
                .map(|ts| Utc.timestamp(ts, parsed.timestamp_nanos.unwrap_or(0) as u32))
                .unwrap_or(Utc::now());
            event
                .as_mut_log()
                .insert_implicit(event::TIMESTAMP.clone(), timestamp.into());

            trace!(
                message = "processing one event.",
                event = &field::debug(&event)
            );

            event
        })
        .ok()
}

#[cfg(test)]
mod test {
    use super::{event_from_str, SyslogConfig};
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
        expected.as_mut_log().insert_implicit(
            event::TIMESTAMP.clone(),
            chrono::Utc.ymd(2019, 2, 13).and_hms(19, 48, 34).into(),
        );
        expected
            .as_mut_log()
            .insert_implicit("host".into(), "74794bfb6795".into());

        assert_eq!(
            expected,
            event_from_str(&"host".to_string(), None, raw).unwrap()
        );
    }

    #[test]
    fn handles_weird_whitespace() {
        // this should also match rsyslog omfwd with template=RSYSLOG_SyslogProtocol23Format
        let raw = r#"
            <13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar
            "#;
        let cleaned = r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar"#;

        let mut expected = Event::from(cleaned);
        expected.as_mut_log().insert_implicit(
            event::TIMESTAMP.clone(),
            chrono::Utc.ymd(2019, 2, 13).and_hms(19, 48, 34).into(),
        );
        expected
            .as_mut_log()
            .insert_implicit("host".into(), "74794bfb6795".into());

        assert_eq!(
            expected,
            event_from_str(&"host".to_string(), None, raw).unwrap()
        );
    }

    #[test]
    #[ignore]
    fn syslog_ng_default_network() {
        let raw = r#"<13>Feb 13 20:07:26 74794bfb6795 root[8539]: i am foobar"#;

        let mut expected = Event::from(raw);
        expected.as_mut_log().insert_implicit(
            event::TIMESTAMP.clone(),
            chrono::Utc.ymd(2019, 2, 13).and_hms(20, 7, 26).into(),
        );
        expected
            .as_mut_log()
            .insert_implicit("host".into(), "74794bfb6795".into());

        assert_eq!(
            expected,
            event_from_str(&"host".to_string(), None, raw).unwrap()
        );
    }

    #[test]
    #[ignore]
    fn rsyslog_omfwd_tcp_default() {
        let raw = r#"<190>Feb 13 21:31:56 74794bfb6795 liblogging-stdlog:  [origin software="rsyslogd" swVersion="8.24.0" x-pid="8979" x-info="http://www.rsyslog.com"] start"#;

        let mut expected = Event::from(raw);
        expected.as_mut_log().insert_implicit(
            event::TIMESTAMP.clone(),
            chrono::Utc.ymd(2019, 2, 13).and_hms(21, 31, 56).into(),
        );
        expected
            .as_mut_log()
            .insert_implicit("host".into(), "74794bfb6795".into());

        assert_eq!(
            expected,
            event_from_str(&"host".to_string(), None, raw).unwrap()
        );
    }

    #[test]
    #[ignore]
    fn rsyslog_omfwd_tcp_forward_format() {
        let raw = r#"<190>2019-02-13T21:53:30.605850+00:00 74794bfb6795 liblogging-stdlog:  [origin software="rsyslogd" swVersion="8.24.0" x-pid="9043" x-info="http://www.rsyslog.com"] start"#;

        let mut expected = Event::from(raw);
        expected.as_mut_log().insert_implicit(
            event::TIMESTAMP.clone(),
            chrono::Utc.ymd(2019, 2, 13).and_hms(21, 53, 30).into(),
        );
        expected
            .as_mut_log()
            .insert_implicit("host".into(), "74794bfb6795".into());

        assert_eq!(
            expected,
            event_from_str(&"host".to_string(), None, raw).unwrap()
        );
    }
}
