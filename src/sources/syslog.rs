use super::util::{SocketListenAddr, TcpSource};
#[cfg(unix)]
use crate::sources::util::build_unix_source;
use crate::{
    event::{self, Event, Value},
    internal_events::{SyslogEventReceived, SyslogUdpReadError},
    shutdown::ShutdownSignal,
    tls::{MaybeTlsSettings, TlsConfig},
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use bytes::Bytes;
use chrono::{Datelike, Utc};
use derive_is_enum_variant::is_enum_variant;
use futures01::{future, sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
#[cfg(unix)]
use std::path::PathBuf;
use syslog_loose::{self, IncompleteDate, Message, ProcId, Protocol};
use tokio01::{
    self,
    codec::{BytesCodec, LinesCodec},
    net::{UdpFramed, UdpSocket},
};
use tracing::field;

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
    Tcp {
        address: SocketListenAddr,
        tls: Option<TlsConfig>,
    },
    Udp {
        address: SocketAddr,
    },
    #[cfg(unix)]
    Unix {
        path: PathBuf,
    },
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

inventory::submit! {
    SourceDescription::new_without_default::<SyslogConfig>("syslog")
}

#[typetag::serde(name = "syslog")]
impl SourceConfig for SyslogConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        let host_key = self
            .host_key
            .clone()
            .unwrap_or(event::log_schema().host_key().to_string());

        match self.mode.clone() {
            Mode::Tcp { address, tls } => {
                let source = SyslogTcpSource {
                    max_length: self.max_length,
                    host_key,
                };
                let shutdown_secs = 30;
                let tls = MaybeTlsSettings::from_config(&tls, true)?;
                source.run(address, shutdown_secs, tls, shutdown, out)
            }
            Mode::Udp { address } => Ok(udp(address, self.max_length, host_key, out)),
            #[cfg(unix)]
            Mode::Unix { path } => Ok(build_unix_source(
                path,
                self.max_length,
                host_key,
                out,
                event_from_str,
            )),
        }
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "syslog"
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

    fn build_event(&self, frame: String, host: Bytes) -> Option<Event> {
        event_from_str(&self.host_key, Some(host), &frame).map(|event| {
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
                .filter_map(move |(bytes, received_from)| {
                    let host_key = host_key.clone();
                    let received_from = received_from.to_string().into();

                    std::str::from_utf8(&bytes)
                        .ok()
                        .and_then(|s| event_from_str(&host_key, Some(received_from), s))
                })
                .map_err(|error| emit!(SyslogUdpReadError { error }));

            lines_in.forward(out).map(|_| info!("finished sending"))
        }),
    )
}

/// Function used to resolve the year for syslog messages that don't include the year.
/// If the current month is January, and the syslog message is for December, it will take the previous year.
/// Otherwise, take the current year.
fn resolve_year((month, _date, _hour, _min, _sec): IncompleteDate) -> i32 {
    let now = Utc::now();
    if now.month() == 1 && month == 12 {
        now.year() - 1
    } else {
        now.year()
    }
}

/**
* Function to pass to build_unix_source, specific to the Unix mode of the syslog source.
* Handles the logic of parsing and decoding the syslog message format.
**/
// TODO: many more cases to handle:
// octet framing (i.e. num bytes as ascii string prefix) with and without delimiters
// null byte delimiter in place of newline
fn event_from_str(host_key: &str, default_host: Option<Bytes>, line: &str) -> Option<Event> {
    emit!(SyslogEventReceived {
        byte_size: line.len()
    });

    let line = line.trim();
    let parsed = syslog_loose::parse_message_with_year(line, resolve_year);
    let mut event = Event::from(&parsed.msg[..]);

    if let Some(host) = &parsed.hostname {
        event.as_mut_log().insert(host_key, host.clone());
    } else if let Some(default_host) = default_host {
        event.as_mut_log().insert(host_key, default_host);
    }

    let timestamp = parsed
        .timestamp
        .map(|ts| ts.into())
        .unwrap_or_else(Utc::now);
    event
        .as_mut_log()
        .insert(event::log_schema().timestamp_key().clone(), timestamp);

    insert_fields_from_syslog(&mut event, parsed);

    trace!(
        message = "processing one event.",
        event = &field::debug(&event)
    );

    Some(event)
}

fn insert_fields_from_syslog(event: &mut Event, parsed: Message<&str>) {
    let log = event.as_mut_log();

    if let Some(severity) = parsed.severity {
        log.insert("severity", severity.as_str());
    }
    if let Some(facility) = parsed.facility {
        log.insert("facility", facility.as_str());
    }
    if let Protocol::RFC5424(version) = parsed.protocol {
        log.insert("version", version as i64);
    }
    if let Some(app_name) = parsed.appname {
        log.insert("appname", app_name);
    }
    if let Some(msg_id) = parsed.msgid {
        log.insert("msgid", msg_id);
    }
    if let Some(procid) = parsed.procid {
        let value: Value = match procid {
            ProcId::PID(pid) => pid.into(),
            ProcId::Name(name) => name.into(),
        };
        log.insert("procid", value);
    }

    for element in parsed.structured_data.iter() {
        for (name, value) in element.params.iter() {
            let key = format!("{}.{}", element.id, name);
            log.insert(key, value.clone());
        }
    }
}

#[cfg(test)]
mod test {
    use super::{event_from_str, SyslogConfig};
    use crate::event::{self, Event};
    use chrono::TimeZone;

    #[test]
    fn config_tcp() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "tcp"
            address = "127.0.0.1:1235"
          "#,
        )
        .unwrap();
        assert!(config.mode.is_tcp());
    }

    #[test]
    fn config_udp() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "udp"
            address = "127.0.0.1:1235"
            max_length = 32187
          "#,
        )
        .unwrap();
        assert!(config.mode.is_udp());
    }

    #[cfg(unix)]
    #[test]
    fn config_unix() {
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
        let msg = "i am foobar";
        let raw = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {}{} {}"#,
            r#"[meta sequenceId="1" sysUpTime="37" language="EN"]"#,
            r#"[origin ip="192.168.0.1" software="test"]"#,
            msg
        );

        let mut expected = Event::from(msg);

        {
            let expected = expected.as_mut_log();
            expected.insert(
                event::log_schema().timestamp_key().clone(),
                chrono::Utc.ymd(2019, 2, 13).and_hms(19, 48, 34),
            );
            expected.insert("host", "74794bfb6795");

            expected.insert("meta.sequenceId", "1");
            expected.insert("meta.sysUpTime", "37");
            expected.insert("meta.language", "EN");
            expected.insert("origin.software", "test");
            expected.insert("origin.ip", "192.168.0.1");

            expected.insert("severity", "notice");
            expected.insert("facility", "user");
            expected.insert("version", 1);
            expected.insert("appname", "root");
            expected.insert("procid", 8449);
        }

        assert_eq!(
            event_from_str(&"host".to_string(), None, &raw).unwrap(),
            expected
        );
    }

    #[test]
    fn handles_incorrect_sd_element() {
        let msg = "qwerty";
        let raw = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} {}"#,
            r#"[incorrect x]"#, msg
        );

        let mut expected = Event::from(msg);
        {
            let expected = expected.as_mut_log();
            expected.insert(
                event::log_schema().timestamp_key().clone(),
                chrono::Utc.ymd(2019, 2, 13).and_hms(19, 48, 34),
            );
            expected.insert(event::log_schema().host_key().clone(), "74794bfb6795");
            expected.insert("severity", "notice");
            expected.insert("facility", "user");
            expected.insert("version", 1);
            expected.insert("appname", "root");
            expected.insert("procid", 8449);
        }

        let event = event_from_str(&"host".to_string(), None, &raw);
        assert_eq!(event, Some(expected.clone()));

        let raw = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} {}"#,
            r#"[incorrect x=]"#, msg
        );

        let event = event_from_str(&"host".to_string(), None, &raw);
        assert_eq!(event, Some(expected));
    }

    #[test]
    fn handles_empty_sd_element() {
        fn there_is_map_called_empty(event: Event) -> bool {
            event
                .as_log()
                .all_fields()
                .find(|(key, _)| (&key[..]).starts_with("empty"))
                == None
        }

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty]"#
        );

        let event = event_from_str(&"host".to_string(), None, &msg).unwrap();
        assert!(there_is_map_called_empty(event));

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[non_empty x="1"][empty]"#
        );

        let event = event_from_str(&"host".to_string(), None, &msg).unwrap();
        assert!(there_is_map_called_empty(event));

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty][non_empty x="1"]"#
        );

        let event = event_from_str(&"host".to_string(), None, &msg).unwrap();
        assert!(there_is_map_called_empty(event));

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty not_really="testing the test"]"#
        );

        let event = event_from_str(&"host".to_string(), None, &msg).unwrap();
        assert!(!there_is_map_called_empty(event));
    }

    #[test]
    fn handles_weird_whitespace() {
        // this should also match rsyslog omfwd with template=RSYSLOG_SyslogProtocol23Format
        let raw = r#"
            <13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar
            "#;
        let cleaned = r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar"#;

        assert_eq!(
            event_from_str(&"host".to_string(), None, raw).unwrap(),
            event_from_str(&"host".to_string(), None, cleaned).unwrap()
        );
    }

    #[test]
    fn syslog_ng_default_network() {
        let msg = "i am foobar";
        let raw = format!(r#"<13>Feb 13 20:07:26 74794bfb6795 root[8539]: {}"#, msg);

        let mut expected = Event::from(msg);
        {
            let expected = expected.as_mut_log();
            expected.insert(
                event::log_schema().timestamp_key().clone(),
                chrono::Utc.ymd(2020, 2, 13).and_hms(20, 7, 26),
            );
            expected.insert(event::log_schema().host_key().clone(), "74794bfb6795");
            expected.insert("severity", "notice");
            expected.insert("facility", "user");
            expected.insert("appname", "root");
            expected.insert("procid", 8539);
        }

        assert_eq!(
            event_from_str(&"host".to_string(), None, &raw).unwrap(),
            expected
        );
    }

    #[test]
    fn rsyslog_omfwd_tcp_default() {
        let msg = "start";
        let raw = format!(
            r#"<190>Feb 13 21:31:56 74794bfb6795 liblogging-stdlog:  [origin software="rsyslogd" swVersion="8.24.0" x-pid="8979" x-info="http://www.rsyslog.com"] {}"#,
            msg
        );

        let mut expected = Event::from(msg);
        {
            let expected = expected.as_mut_log();
            expected.insert(
                event::log_schema().timestamp_key().clone(),
                chrono::Utc.ymd(2020, 2, 13).and_hms(21, 31, 56),
            );
            expected.insert("host", "74794bfb6795");
            expected.insert("severity", "info");
            expected.insert("facility", "local7");
            expected.insert("appname", "liblogging-stdlog");
            expected.insert("origin.software", "rsyslogd");
            expected.insert("origin.swVersion", "8.24.0");
            expected.insert("origin.x-pid", "8979");
            expected.insert("origin.x-info", "http://www.rsyslog.com");
        }

        assert_eq!(
            event_from_str(&"host".to_string(), None, &raw).unwrap(),
            expected
        );
    }

    #[test]
    fn rsyslog_omfwd_tcp_forward_format() {
        let msg = "start";
        let raw = format!(
            r#"<190>2019-02-13T21:53:30.605850+00:00 74794bfb6795 liblogging-stdlog:  [origin software="rsyslogd" swVersion="8.24.0" x-pid="9043" x-info="http://www.rsyslog.com"] {}"#,
            msg
        );

        let mut expected = Event::from(msg);
        {
            let expected = expected.as_mut_log();
            expected.insert(
                event::log_schema().timestamp_key().clone(),
                chrono::Utc
                    .ymd(2019, 2, 13)
                    .and_hms_micro(21, 53, 30, 605_850),
            );
            expected.insert("host", "74794bfb6795");
            expected.insert("severity", "info");
            expected.insert("facility", "local7");
            expected.insert("appname", "liblogging-stdlog");
            expected.insert("origin.software", "rsyslogd");
            expected.insert("origin.swVersion", "8.24.0");
            expected.insert("origin.x-pid", "9043");
            expected.insert("origin.x-info", "http://www.rsyslog.com");
        }

        assert_eq!(
            event_from_str(&"host".to_string(), None, &raw).unwrap(),
            expected
        );
    }
}
