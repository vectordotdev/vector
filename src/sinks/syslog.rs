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
    #[serde(default = "crate::serde::default_true")]
    rfc5424: bool,
    #[serde(default = "crate::serde::default_false")]
    include_extra_fields: bool,
    #[serde(default = "appname_key")]
    appname_key: String,
    #[serde(default = "facility_key")]
    facility_key: String,
    #[serde(default = "host_key")]
    host_key: String,
    #[serde(default = "msgid_key")]
    msgid_key: String,
    #[serde(default = "procid_key")]
    procid_key: String,
    #[serde(default = "severity_key")]
    severity_key: String,
    #[serde(with = "SyslogFacilityDef", default = "facility")]
    default_facility: SyslogFacility,
    #[serde(with = "SyslogSeverityDef", default = "severity")]
    default_severity: SyslogSeverity,
}

fn appname_key() -> String {
    "appname".to_string()
}

fn facility_key() -> String {
    "facility".to_string()
}

fn host_key() -> String {
    crate::config::log_schema().host_key().to_string()
}

fn msgid_key() -> String {
    "msgid".to_string()
}

fn procid_key() -> String {
    "procid".to_string()
}

fn severity_key() -> String {
    "severity".to_string()
}

fn facility() -> SyslogFacility {
    SyslogFacility::LOG_SYSLOG
}

fn severity() -> SyslogSeverity {
    SyslogSeverity::SEV_DEBUG
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

#[async_trait::async_trait]
#[typetag::serde(name = "syslog")]
impl SinkConfig for SyslogSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let rfc5424 = self.rfc5424.clone();
        let include_extra_fields = self.include_extra_fields.clone();
        let appname_key = self.appname_key.to_owned();
        let facility_key = self.facility_key.to_owned();
        let host_key = self.host_key.to_owned();
        let msgid_key = self.procid_key.to_owned();
        let procid_key = self.msgid_key.to_owned();
        let severity_key = self.severity_key.to_owned();
        let default_facility = self.default_facility.clone();
        let default_severity = self.default_severity.clone();

        let syslog_encode = move |event, include_len| {
            build_syslog_message(
                event,
                include_len,
                rfc5424,
                include_extra_fields,
                appname_key.as_str(),
                facility_key.as_str(),
                host_key.as_str(),
                msgid_key.as_str(),
                procid_key.as_str(),
                severity_key.as_str(),
                default_facility,
                default_severity,
            )
        };

        match &self.mode {
            Mode::Tcp(config) => config.build(cx, move |e| syslog_encode(e, true)),
            Mode::Udp(config) => config.build(cx, move |e| syslog_encode(e, false)),
            #[cfg(unix)]
            Mode::Unix(config) => config.build(cx, move |e| syslog_encode(e, true)),
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "syslog"
    }
}

fn build_syslog_message(
    event: Event,
    include_len: bool,
    rfc5424: bool,
    include_extra_fields: bool,
    appname_key: &str,
    facility_key: &str,
    host_key: &str,
    msgid_key: &str,
    procid_key: &str,
    severity_key: &str,
    default_facility: SyslogFacility,
    default_severity: SyslogSeverity,
) -> Option<Bytes> {
    let mut log = event.into_log();

    let ts = log.remove(log_schema().timestamp_key()).and_then(|v| {
        v.as_timestamp()
            .map(|v| FixedOffset::west(0).timestamp_nanos(v.timestamp_nanos()))
    });

    let procid = log
        .remove(procid_key)
        .map(|procid| match procid {
            Value::Integer(pid) => ProcId::PID(pid as i32),
            Value::Bytes(_) => ProcId::Name(procid.to_string_lossy()),
            _ => ProcId::Name("vector".to_string()),
        })
        .or(Some(ProcId::Name("vector".to_string())));

    let msg = Message {
        protocol: if rfc5424 {
            Protocol::RFC5424(1)
        } else {
            Protocol::RFC3164
        },
        timestamp: ts,
        procid: procid,
        facility: log
            .remove(facility_key)
            .and_then(get_facility)
            .or(Some(default_facility)),
        severity: log
            .remove(severity_key)
            .and_then(get_severity)
            .or(Some(default_severity)),
        appname: log.remove(appname_key).map(|v| v.to_string_lossy()),
        msgid: log.remove(msgid_key).map(|v| v.to_string_lossy()),
        hostname: log.remove(host_key).map(|v| v.to_string_lossy()),
        msg: log
            .remove(log_schema().message_key())
            .map(|v| v.to_string_lossy())
            .unwrap_or("-".to_owned()),
        structured_data: if include_extra_fields {
            build_structured_data(log)
        } else {
            vec![]
        },
    };

    if include_len {
        let msg = format!("{}", msg);
        Some(Bytes::from(format!("{} {}\n", msg.len(), msg)))
    } else {
        Some(Bytes::from(format!("{}\n", msg)))
    }
}

fn build_structured_data(log: LogEvent) -> Vec<StructuredElement<String>> {
    let mut d = vec![];
    for (k, v) in log.as_map().iter() {
        let mut e = StructuredElement {
            id: k.clone(),
            params: vec![],
        };
        if let Value::Map(m) = v {
            for (k, v) in m.iter() {
                e.params.push((k.clone(), v.to_string_lossy()));
            }
        } else {
            e.params.push(("value".to_string(), v.to_string_lossy()));
        }
        d.push(e);
    }
    d
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

/// Syslog Severities from RFC 5424.
#[derive(Serialize, Deserialize)]
#[serde(remote = "SyslogSeverity")]
#[allow(non_camel_case_types)]
pub enum SyslogSeverityDef {
    SEV_EMERG = 0,
    SEV_ALERT = 1,
    SEV_CRIT = 2,
    SEV_ERR = 3,
    SEV_WARNING = 4,
    SEV_NOTICE = 5,
    SEV_INFO = 6,
    SEV_DEBUG = 7,
}

/// Names are from Linux.
#[derive(Serialize, Deserialize)]
#[serde(remote = "SyslogFacility")]
#[allow(non_camel_case_types)]
pub enum SyslogFacilityDef {
    LOG_KERN = 0,
    LOG_USER = 1,
    LOG_MAIL = 2,
    LOG_DAEMON = 3,
    LOG_AUTH = 4,
    LOG_SYSLOG = 5,
    LOG_LPR = 6,
    LOG_NEWS = 7,
    LOG_UUCP = 8,
    LOG_CRON = 9,
    LOG_AUTHPRIV = 10,
    LOG_FTP = 11,
    LOG_NTP = 12,
    LOG_AUDIT = 13,
    LOG_ALERT = 14,
    LOG_CLOCKD = 15,
    LOG_LOCAL0 = 16,
    LOG_LOCAL1 = 17,
    LOG_LOCAL2 = 18,
    LOG_LOCAL3 = 19,
    LOG_LOCAL4 = 20,
    LOG_LOCAL5 = 21,
    LOG_LOCAL6 = 22,
    LOG_LOCAL7 = 23,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Event;
    use chrono::Utc;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SyslogSinkConfig>();
    }

    fn build_message(event: Event, rfc5434: bool) -> Option<Bytes> {
        build_syslog_message(
            event,
            false,
            rfc5434,
            true,
            "appname",
            "facility",
            "hostname",
            "msgid",
            "procid",
            "severity",
            SyslogFacility::LOG_USER,
            SyslogSeverity::SEV_INFO,
        )
    }

    #[test]
    fn basic_syslog5424_message() {
        let mut event = Event::from("A message");

        let timestamp = Utc.ymd(2021, 04, 12).and_hms(21, 0, 1);

        event.as_mut_log().insert("timestamp", timestamp);
        event.as_mut_log().insert("hostname", "foohost");
        event.as_mut_log().insert("appname", "myapp");

        let bytes = build_message(event, true).unwrap();

        let msg =
            Bytes::from("<14>1 2021-04-12T21:00:01+00:00 foohost myapp vector - - A message\n");

        assert_eq!(bytes, msg);
    }

    #[test]
    fn basic_syslog3164_message() {
        let mut event = Event::from("A message");

        let timestamp = Utc.ymd(2021, 04, 12).and_hms(21, 0, 1);

        event.as_mut_log().insert("timestamp", timestamp);
        event.as_mut_log().insert("hostname", "foo");
        event.as_mut_log().insert("appname", "bar");
        event.as_mut_log().insert("severity", "warning");
        event.as_mut_log().insert("facility", "kern");

        let bytes = build_message(event, false).unwrap();

        let msg =
            Bytes::from("<4> 2021-04-12T21:00:01+00:00 foo bar[vector]: A message\n");

        assert_eq!(bytes, msg);
    }
}
