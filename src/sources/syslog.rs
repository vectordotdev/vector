use super::util::{SocketListenAddr, TcpSource};
#[cfg(unix)]
use crate::sources::util::build_unix_stream_source;
use crate::{
    codecs::{BoxedFramingError, OctetCountingCodec},
    config::{
        log_schema, DataType, GenerateConfig, Resource, SourceConfig, SourceContext,
        SourceDescription,
    },
    event::{Event, Value},
    internal_events::{SyslogEventReceived, SyslogUdpReadError, SyslogUdpUtf8Error},
    shutdown::ShutdownSignal,
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsConfig},
    udp, Pipeline,
};
use bytes::Bytes;
use chrono::{Datelike, Utc};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
#[cfg(unix)]
use std::path::PathBuf;
use syslog_loose::{IncompleteDate, Message, ProcId, Protocol};
use tokio::net::UdpSocket;
use tokio_util::{codec::BytesCodec, udp::UdpFramed};

#[derive(Deserialize, Serialize, Debug)]
// TODO: add back when serde-rs/serde#1358 is addressed
// #[serde(deny_unknown_fields)]
pub struct SyslogConfig {
    #[serde(flatten)]
    mode: Mode,
    #[serde(default = "default_max_length")]
    max_length: usize,
    /// The host key of the log. (This differs from `hostname`)
    host_key: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Tcp {
        address: SocketListenAddr,
        keepalive: Option<TcpKeepaliveConfig>,
        tls: Option<TlsConfig>,
        receive_buffer_bytes: Option<usize>,
    },
    Udp {
        address: SocketAddr,
        receive_buffer_bytes: Option<usize>,
    },
    #[cfg(unix)]
    Unix { path: PathBuf },
}

pub fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
}

impl SyslogConfig {
    pub fn from_mode(mode: Mode) -> Self {
        Self {
            mode,
            host_key: None,
            max_length: default_max_length(),
        }
    }
}

inventory::submit! {
    SourceDescription::new::<SyslogConfig>("syslog")
}

impl GenerateConfig for SyslogConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            mode: Mode::Tcp {
                address: SocketListenAddr::SocketAddr("0.0.0.0:514".parse().unwrap()),
                keepalive: None,
                tls: None,
                receive_buffer_bytes: None,
            },
            host_key: None,
            max_length: default_max_length(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "syslog")]
impl SourceConfig for SyslogConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let host_key = self
            .host_key
            .clone()
            .unwrap_or_else(|| log_schema().host_key().to_string());

        match self.mode.clone() {
            Mode::Tcp {
                address,
                keepalive,
                tls,
                receive_buffer_bytes,
            } => {
                let source = SyslogTcpSource {
                    max_length: self.max_length,
                    host_key,
                };
                let shutdown_secs = 30;
                let tls = MaybeTlsSettings::from_config(&tls, true)?;
                source.run(
                    address,
                    keepalive,
                    shutdown_secs,
                    tls,
                    receive_buffer_bytes,
                    cx.shutdown,
                    cx.out,
                )
            }
            Mode::Udp {
                address,
                receive_buffer_bytes,
            } => Ok(udp(
                address,
                self.max_length,
                host_key,
                receive_buffer_bytes,
                cx.shutdown,
                cx.out,
            )),
            #[cfg(unix)]
            Mode::Unix { path } => Ok(build_unix_stream_source(
                path,
                OctetCountingCodec::new_with_max_length(self.max_length),
                host_key,
                cx.shutdown,
                cx.out,
                |host_key, default_host, bytes| {
                    let line = std::str::from_utf8(&bytes)
                        .expect("valid UTF-8 guaranteed by OctetCountingCodec");
                    Some(event_from_str(host_key, default_host, line))
                },
            )),
        }
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "syslog"
    }

    fn resources(&self) -> Vec<Resource> {
        match self.mode.clone() {
            Mode::Tcp { address, .. } => vec![address.into()],
            Mode::Udp { address, .. } => vec![Resource::udp(address)],
            #[cfg(unix)]
            Mode::Unix { .. } => vec![],
        }
    }
}

#[derive(Debug, Clone)]
struct SyslogTcpSource {
    max_length: usize,
    host_key: String,
}

impl TcpSource for SyslogTcpSource {
    type Error = BoxedFramingError;
    type Decoder = OctetCountingCodec;

    fn decoder(&self) -> Self::Decoder {
        OctetCountingCodec::new_with_max_length(self.max_length)
    }

    fn build_event(&self, frame: Bytes, host: Bytes) -> Option<Event> {
        let line =
            std::str::from_utf8(&frame).expect("valid UTF-8 guaranteed by OctetCountingCodec");
        Some(event_from_str(&self.host_key, Some(host), line))
    }
}

pub fn udp(
    addr: SocketAddr,
    _max_length: usize,
    host_key: String,
    receive_buffer_bytes: Option<usize>,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> super::Source {
    let out = out.sink_map_err(|error| error!(message = "Error sending line.", %error));

    Box::pin(async move {
        let socket = UdpSocket::bind(&addr)
            .await
            .expect("Failed to bind to UDP listener socket");

        if let Some(receive_buffer_bytes) = receive_buffer_bytes {
            if let Err(error) = udp::set_receive_buffer_size(&socket, receive_buffer_bytes) {
                warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
            }
        }

        info!(
            message = "Listening.",
            addr = %addr,
            r#type = "udp"
        );

        let _ = UdpFramed::new(socket, BytesCodec::new())
            .take_until(shutdown)
            .filter_map(|frame| {
                let host_key = host_key.clone();
                async move {
                    match frame {
                        Ok((bytes, received_from)) => {
                            let received_from = received_from.ip().to_string().into();

                            std::str::from_utf8(&bytes)
                                .map_err(|error| emit!(SyslogUdpUtf8Error { error }))
                                .ok()
                                .map(|s| Ok(event_from_str(&host_key, Some(received_from), s)))
                        }
                        Err(error) => {
                            emit!(SyslogUdpReadError { error });
                            None
                        }
                    }
                }
            })
            .forward(out)
            .await;

        info!("Finished sending.");
        Ok(())
    })
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
* Function to pass to build_unix_stream_source, specific to the Unix mode of the syslog source.
* Handles the logic of parsing and decoding the syslog message format.
**/
// TODO: many more cases to handle:
// octet framing (i.e. num bytes as ascii string prefix) with and without delimiters
// null byte delimiter in place of newline
fn event_from_str(host_key: &str, default_host: Option<Bytes>, line: &str) -> Event {
    let line = line.trim();
    let parsed = syslog_loose::parse_message_with_year(line, resolve_year);
    let mut event = Event::from(parsed.msg);

    // Add source type
    event
        .as_mut_log()
        .insert(log_schema().source_type_key(), Bytes::from("syslog"));

    if let Some(default_host) = default_host.clone() {
        event.as_mut_log().insert("source_ip", default_host);
    }

    let parsed_hostname = parsed.hostname.map(|x| Bytes::from(x.to_owned()));
    if let Some(parsed_host) = parsed_hostname.or(default_host) {
        event.as_mut_log().insert(host_key, parsed_host);
    }

    let timestamp = parsed
        .timestamp
        .map(|ts| ts.into())
        .unwrap_or_else(Utc::now);
    event
        .as_mut_log()
        .insert(log_schema().timestamp_key(), timestamp);

    insert_fields_from_syslog(&mut event, parsed);

    emit!(SyslogEventReceived {
        byte_size: line.len()
    });

    trace!(
        message = "Processing one event.",
        event = ?event
    );

    event
}

fn insert_fields_from_syslog(event: &mut Event, parsed: Message<&str>) {
    let log = event.as_mut_log();

    if let Some(host) = parsed.hostname {
        log.insert("hostname", host.to_string());
    }
    if let Some(severity) = parsed.severity {
        log.insert("severity", severity.as_str().to_owned());
    }
    if let Some(facility) = parsed.facility {
        log.insert("facility", facility.as_str().to_owned());
    }
    if let Protocol::RFC5424(version) = parsed.protocol {
        log.insert("version", version as i64);
    }
    if let Some(app_name) = parsed.appname {
        log.insert("appname", app_name.to_owned());
    }
    if let Some(msg_id) = parsed.msgid {
        log.insert("msgid", msg_id.to_owned());
    }
    if let Some(procid) = parsed.procid {
        let value: Value = match procid {
            ProcId::PID(pid) => pid.into(),
            ProcId::Name(name) => name.to_string().into(),
        };
        log.insert("procid", value);
    }

    for element in parsed.structured_data.into_iter() {
        for (name, value) in element.params.into_iter() {
            let key = format!("{}.{}", element.id, name);
            log.insert(key, value.to_string());
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{config::log_schema, event::Event};
    use chrono::prelude::*;
    use shared::assert_event_data_eq;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SyslogConfig>();
    }

    #[test]
    fn config_tcp() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "tcp"
            address = "127.0.0.1:1235"
          "#,
        )
        .unwrap();
        assert!(matches!(config.mode, Mode::Tcp { .. }));
    }

    #[test]
    fn config_tcp_with_receive_buffer_size() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "tcp"
            address = "127.0.0.1:1235"
            receive_buffer_bytes = 256
          "#,
        )
        .unwrap();

        let receive_buffer_bytes = match config.mode {
            Mode::Tcp {
                receive_buffer_bytes,
                ..
            } => receive_buffer_bytes,
            _ => panic!("expected Mode::Tcp"),
        };

        assert_eq!(receive_buffer_bytes, Some(256));
    }

    #[test]
    fn config_tcp_keepalive_empty() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "tcp"
            address = "127.0.0.1:1235"
          "#,
        )
        .unwrap();

        let keepalive = match config.mode {
            Mode::Tcp { keepalive, .. } => keepalive,
            _ => panic!("expected Mode::Tcp"),
        };

        assert_eq!(keepalive, None);
    }

    #[test]
    fn config_tcp_keepalive_full() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "tcp"
            address = "127.0.0.1:1235"
            keepalive.time_secs = 7200
          "#,
        )
        .unwrap();

        let keepalive = match config.mode {
            Mode::Tcp { keepalive, .. } => keepalive,
            _ => panic!("expected Mode::Tcp"),
        };

        let keepalive = keepalive.expect("keepalive config not set");

        assert_eq!(keepalive.time_secs, Some(7200));
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
        assert!(matches!(config.mode, Mode::Udp { .. }));
    }

    #[test]
    fn config_udp_with_receive_buffer_size() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "udp"
            address = "127.0.0.1:1235"
            max_length = 32187
            receive_buffer_bytes = 256
          "#,
        )
        .unwrap();

        let receive_buffer_bytes = match config.mode {
            Mode::Udp {
                receive_buffer_bytes,
                ..
            } => receive_buffer_bytes,
            _ => panic!("expected Mode::Udp"),
        };

        assert_eq!(receive_buffer_bytes, Some(256));
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
        assert!(matches!(config.mode, Mode::Unix { .. }));
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
                log_schema().timestamp_key(),
                chrono::Utc.ymd(2019, 2, 13).and_hms(19, 48, 34),
            );
            expected.insert(log_schema().source_type_key(), "syslog");
            expected.insert("host", "74794bfb6795");
            expected.insert("hostname", "74794bfb6795");

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

        assert_event_data_eq!(event_from_str(&"host".to_string(), None, &raw), expected);
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
                log_schema().timestamp_key(),
                chrono::Utc.ymd(2019, 2, 13).and_hms(19, 48, 34),
            );
            expected.insert(log_schema().host_key(), "74794bfb6795");
            expected.insert("hostname", "74794bfb6795");
            expected.insert(log_schema().source_type_key(), "syslog");
            expected.insert("severity", "notice");
            expected.insert("facility", "user");
            expected.insert("version", 1);
            expected.insert("appname", "root");
            expected.insert("procid", 8449);
        }

        let event = event_from_str(&"host".to_string(), None, &raw);
        assert_event_data_eq!(event, expected);

        let raw = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} {}"#,
            r#"[incorrect x=]"#, msg
        );

        let event = event_from_str(&"host".to_string(), None, &raw);
        assert_event_data_eq!(event, expected);
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

        let event = event_from_str(&"host".to_string(), None, &msg);
        assert!(there_is_map_called_empty(event));

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[non_empty x="1"][empty]"#
        );

        let event = event_from_str(&"host".to_string(), None, &msg);
        assert!(there_is_map_called_empty(event));

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty][non_empty x="1"]"#
        );

        let event = event_from_str(&"host".to_string(), None, &msg);
        assert!(there_is_map_called_empty(event));

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty not_really="testing the test"]"#
        );

        let event = event_from_str(&"host".to_string(), None, &msg);
        assert!(!there_is_map_called_empty(event));
    }

    #[test]
    fn handles_weird_whitespace() {
        // this should also match rsyslog omfwd with template=RSYSLOG_SyslogProtocol23Format
        let raw = r#"
            <13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar
            "#;
        let cleaned = r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar"#;

        assert_event_data_eq!(
            event_from_str(&"host".to_string(), None, raw),
            event_from_str(&"host".to_string(), None, cleaned)
        );
    }

    #[test]
    fn syslog_ng_default_network() {
        let msg = "i am foobar";
        let raw = format!(r#"<13>Feb 13 20:07:26 74794bfb6795 root[8539]: {}"#, msg);
        let event = event_from_str(&"host".to_string(), None, &raw);

        let mut expected = Event::from(msg);
        {
            let value = event.as_log().get("timestamp").unwrap();
            let year = value.as_timestamp().unwrap().naive_local().year();

            let expected = expected.as_mut_log();
            let expected_date: DateTime<Utc> =
                chrono::Local.ymd(year, 2, 13).and_hms(20, 7, 26).into();
            expected.insert(log_schema().timestamp_key(), expected_date);
            expected.insert(log_schema().host_key(), "74794bfb6795");
            expected.insert(log_schema().source_type_key(), "syslog");
            expected.insert("hostname", "74794bfb6795");
            expected.insert("severity", "notice");
            expected.insert("facility", "user");
            expected.insert("appname", "root");
            expected.insert("procid", 8539);
        }

        assert_event_data_eq!(event, expected);
    }

    #[test]
    fn rsyslog_omfwd_tcp_default() {
        let msg = "start";
        let raw = format!(
            r#"<190>Feb 13 21:31:56 74794bfb6795 liblogging-stdlog:  [origin software="rsyslogd" swVersion="8.24.0" x-pid="8979" x-info="http://www.rsyslog.com"] {}"#,
            msg
        );
        let event = event_from_str(&"host".to_string(), None, &raw);

        let mut expected = Event::from(msg);
        {
            let value = event.as_log().get("timestamp").unwrap();
            let year = value.as_timestamp().unwrap().naive_local().year();

            let expected = expected.as_mut_log();
            let expected_date: DateTime<Utc> =
                chrono::Local.ymd(year, 2, 13).and_hms(21, 31, 56).into();
            expected.insert(log_schema().timestamp_key(), expected_date);
            expected.insert(log_schema().source_type_key(), "syslog");
            expected.insert("host", "74794bfb6795");
            expected.insert("hostname", "74794bfb6795");
            expected.insert("severity", "info");
            expected.insert("facility", "local7");
            expected.insert("appname", "liblogging-stdlog");
            expected.insert("origin.software", "rsyslogd");
            expected.insert("origin.swVersion", "8.24.0");
            expected.insert("origin.x-pid", "8979");
            expected.insert("origin.x-info", "http://www.rsyslog.com");
        }

        assert_event_data_eq!(event, expected);
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
                log_schema().timestamp_key(),
                chrono::Utc
                    .ymd(2019, 2, 13)
                    .and_hms_micro(21, 53, 30, 605_850),
            );
            expected.insert(log_schema().source_type_key(), "syslog");
            expected.insert("host", "74794bfb6795");
            expected.insert("hostname", "74794bfb6795");
            expected.insert("severity", "info");
            expected.insert("facility", "local7");
            expected.insert("appname", "liblogging-stdlog");
            expected.insert("origin.software", "rsyslogd");
            expected.insert("origin.swVersion", "8.24.0");
            expected.insert("origin.x-pid", "9043");
            expected.insert("origin.x-info", "http://www.rsyslog.com");
        }

        assert_event_data_eq!(event_from_str(&"host".to_string(), None, &raw), expected);
    }
}
