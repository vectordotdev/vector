#[cfg(unix)]
use crate::sinks::util::unix::UnixSinkConfig;
use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::LogEvent,
    event::Value,
    sinks::util::{tcp::TcpSinkConfig, udp::UdpSinkConfig},
    Event,
};
use bytes::Bytes;
use chrono::{FixedOffset, TimeZone};
use serde::{Deserialize, Serialize};
use syslog_loose::{Message, ProcId, Protocol, StructuredElement, SyslogFacility, SyslogSeverity};

#[derive(Deserialize, Serialize, Debug)]
// TODO: add back when serde-rs/serde#1358 is addressed
// #[serde(deny_unknown_fields)]
pub struct SyslogSinkConfig {
    #[serde(flatten)]
    mode: Mode,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Tcp(TcpSinkConfig),
    Udp(UdpSinkConfig),
    #[cfg(unix)]
    Unix(UnixSinkConfig),
}

inventory::submit! {
    SinkDescription::new::<SyslogSinkConfig>("syslog")
}

impl GenerateConfig for SyslogSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"address = "2001:db8::1:514"
            mode = "tcp"
            "#,
        )
        .unwrap()
    }
}

impl SyslogSinkConfig {
    pub fn new(mode: Mode) -> Self {
        SyslogSinkConfig { mode }
    }

    pub fn make_basic_tcp_config(address: String) -> Self {
        Self::new(Mode::Tcp(TcpSinkConfig::from_address(address)))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "syslog")]
impl SinkConfig for SyslogSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let syslog_encode = move |event| build_syslog_message(event);

        match &self.mode {
            Mode::Tcp(config) => config.build(cx, syslog_encode),
            Mode::Udp(config) => config.build(cx, syslog_encode),
            #[cfg(unix)]
            Mode::Unix(config) => config.build(cx, syslog_encode),
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "syslog"
    }
}

/*

{
  "appname": "root",
  "facility": "user",
  "host": "74794bfb6795",
  "hostname": "74794bfb6795",
  "message": "i am foobar",
  "meta": {
    "sequenceId": "1"
  },
  "procid": 8449,
  "service": "vector",
  "severity": "notice",
  "source_ip": "10.121.132.66",
  "source_type": "syslog",
  "timestamp": "2019-02-13T19:48:34Z",
  "version": 1
}*/

fn build_syslog_message(event: Event) -> Option<Bytes> {
    let mut log = event.into_log();
    let ts = log.remove(log_schema().timestamp_key()).and_then(|v| {
        v.as_timestamp()
            .map(|v| FixedOffset::west(0).timestamp_nanos(v.timestamp_nanos()))
    });

    let procid = log.remove("procid").map(|procid| match procid {
        Value::Integer(pid) => ProcId::PID(pid as i32),
        Value::Bytes(_) => ProcId::Name(procid.to_string_lossy()),
        _ => ProcId::Name("vector".to_string()),
    });

    let msg = Message {
        protocol: Protocol::RFC5424(1),
        facility: log.remove("facility").and_then(get_facility),
        severity: log.remove("severity").and_then(get_severity),
        timestamp: ts,
        // Note: the syslog source uses host and hostname key
        hostname: log
            .remove(log_schema().host_key())
            .map(|v| v.to_string_lossy()),
        appname: log.remove("appname").map(|v| v.to_string_lossy()),
        procid: procid,
        msgid: log.remove("appname").map(|v| v.to_string_lossy()),
        msg: log
            .remove(log_schema().message_key())
            .map(|v| v.to_string_lossy())
            .unwrap_or("-".to_owned()),
        structured_data: build_structured_data(log),
    };
    Some(Bytes::from(format!("{}", msg)))
}

fn build_structured_data(log: LogEvent) -> Vec<StructuredElement<String>> {
    vec![]
}

fn get_facility(value: Value) -> Option<SyslogFacility> {
    match value {
        Value::Integer(i) => match i {
            0 => Some(SyslogFacility::LOG_KERN),
            1 => Some(SyslogFacility::LOG_USER),
            2 => Some(SyslogFacility::LOG_MAIL),
            3 => Some(SyslogFacility::LOG_DAEMON),
            4 => Some(SyslogFacility::LOG_AUTH),
            5 => Some(SyslogFacility::LOG_SYSLOG),
            6 => Some(SyslogFacility::LOG_LPR),
            7 => Some(SyslogFacility::LOG_NEWS),
            8 => Some(SyslogFacility::LOG_UUCP),
            9 => Some(SyslogFacility::LOG_CRON),
            10 => Some(SyslogFacility::LOG_AUTHPRIV),
            11 => Some(SyslogFacility::LOG_FTP),
            12 => Some(SyslogFacility::LOG_NTP),
            13 => Some(SyslogFacility::LOG_AUDIT),
            14 => Some(SyslogFacility::LOG_ALERT),
            15 => Some(SyslogFacility::LOG_CLOCKD),
            16 => Some(SyslogFacility::LOG_LOCAL0),
            17 => Some(SyslogFacility::LOG_LOCAL1),
            18 => Some(SyslogFacility::LOG_LOCAL2),
            19 => Some(SyslogFacility::LOG_LOCAL3),
            20 => Some(SyslogFacility::LOG_LOCAL4),
            21 => Some(SyslogFacility::LOG_LOCAL5),
            22 => Some(SyslogFacility::LOG_LOCAL6),
            23 => Some(SyslogFacility::LOG_LOCAL7),
            _ => None,
        },
        Value::Bytes(_) => match value.to_string_lossy().to_lowercase().as_str() {
            "kern" => Some(SyslogFacility::LOG_KERN),
            "user" => Some(SyslogFacility::LOG_USER),
            "mail" => Some(SyslogFacility::LOG_MAIL),
            "daemon" => Some(SyslogFacility::LOG_DAEMON),
            "auth" => Some(SyslogFacility::LOG_AUTH),
            "syslog" => Some(SyslogFacility::LOG_SYSLOG),
            "lpr" => Some(SyslogFacility::LOG_LPR),
            "news" => Some(SyslogFacility::LOG_NEWS),
            "uucp" => Some(SyslogFacility::LOG_UUCP),
            "cron" => Some(SyslogFacility::LOG_CRON),
            "authpriv" => Some(SyslogFacility::LOG_AUTHPRIV),
            "ftp" => Some(SyslogFacility::LOG_FTP),
            "ntp" => Some(SyslogFacility::LOG_NTP),
            "audit" => Some(SyslogFacility::LOG_AUDIT),
            "alert" => Some(SyslogFacility::LOG_ALERT),
            "clockd" => Some(SyslogFacility::LOG_CLOCKD),
            "local0" => Some(SyslogFacility::LOG_LOCAL0),
            "local1" => Some(SyslogFacility::LOG_LOCAL1),
            "local2" => Some(SyslogFacility::LOG_LOCAL2),
            "local3" => Some(SyslogFacility::LOG_LOCAL3),
            "local4" => Some(SyslogFacility::LOG_LOCAL4),
            "local5" => Some(SyslogFacility::LOG_LOCAL5),
            "local6" => Some(SyslogFacility::LOG_LOCAL6),
            "local7" => Some(SyslogFacility::LOG_LOCAL7),
            _ => None,
        },
        _ => None,
    }
}

fn get_severity(value: Value) -> Option<SyslogSeverity> {
    match value {
        Value::Integer(i) => match i {
            0 => Some(SyslogSeverity::SEV_EMERG),
            1 => Some(SyslogSeverity::SEV_ALERT),
            2 => Some(SyslogSeverity::SEV_CRIT),
            3 => Some(SyslogSeverity::SEV_ERR),
            4 => Some(SyslogSeverity::SEV_WARNING),
            5 => Some(SyslogSeverity::SEV_NOTICE),
            6 => Some(SyslogSeverity::SEV_INFO),
            7 => Some(SyslogSeverity::SEV_DEBUG),
            _ => None,
        },
        Value::Bytes(_) => match value.to_string_lossy().to_lowercase().as_str() {
            "emerg" => Some(SyslogSeverity::SEV_EMERG),
            "alert" => Some(SyslogSeverity::SEV_ALERT),
            "crit" => Some(SyslogSeverity::SEV_CRIT),
            "err" => Some(SyslogSeverity::SEV_ERR),
            "warning" => Some(SyslogSeverity::SEV_WARNING),
            "notice" => Some(SyslogSeverity::SEV_NOTICE),
            "info" => Some(SyslogSeverity::SEV_INFO),
            "debug" => Some(SyslogSeverity::SEV_DEBUG),
            _ => None,
        },
        _ => None,
    }
}
