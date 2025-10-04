use bytes::{BufMut, BytesMut};
use chrono::{DateTime, SecondsFormat, SubsecRound, Utc};
use lookup::lookup_v2::ConfigTargetPath;
use std::collections::HashMap;
use std::fmt::Write;
use std::str::FromStr;
use strum::{EnumString, FromRepr, VariantNames};
use tokio_util::codec::Encoder;
use vector_config::configurable_component;
use vector_core::{
    config::DataType,
    event::{Event, LogEvent, Value},
    schema,
};
use vrl::value::ObjectMap;

/// Config used to build a `SyslogSerializer`.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(default)]
pub struct SyslogSerializerConfig {
    /// Options for the Syslog serializer.
    pub syslog: SyslogSerializerOptions,
}

impl SyslogSerializerConfig {
    /// Build the `SyslogSerializer` from this configuration.
    pub fn build(&self) -> SyslogSerializer {
        SyslogSerializer::new(self)
    }

    /// The data type of events that are accepted by `SyslogSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty()
    }
}

/// Syslog serializer options.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(default)]
pub struct SyslogSerializerOptions {
    /// RFC to use for formatting.
    rfc: SyslogRFC,
    /// Path to a field in the event to use for the facility. Defaults to "user".
    facility: Option<ConfigTargetPath>,
    /// Path to a field in the event to use for the severity. Defaults to "informational".
    severity: Option<ConfigTargetPath>,
    /// Path to a field in the event to use for the app name. Defaults to "vector".
    app_name: Option<ConfigTargetPath>,
    /// Path to a field in the event to use for the proc ID.
    proc_id: Option<ConfigTargetPath>,
    /// Path to a field in the event to use for the msg ID.
    msg_id: Option<ConfigTargetPath>,
    /// Path to a field in the event to use for the main message payload.
    payload_key: Option<ConfigTargetPath>,
}

/// Serializer that converts an `Event` to bytes using the Syslog format.
#[derive(Debug, Clone)]
pub struct SyslogSerializer {
    config: SyslogSerializerConfig,
}

impl SyslogSerializer {
    /// Creates a new `SyslogSerializer`.
    pub fn new(conf: &SyslogSerializerConfig) -> Self {
        Self {
            config: conf.clone(),
        }
    }
}

impl Encoder<Event> for SyslogSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        if let Event::Log(log_event) = event {
            let syslog_message = ConfigDecanter::new(&log_event).decant_config(&self.config.syslog);
            let vec = syslog_message
                .encode(&self.config.syslog.rfc)
                .as_bytes()
                .to_vec();
            buffer.put_slice(&vec);
        }

        Ok(())
    }
}

struct ConfigDecanter<'a> {
    log: &'a LogEvent,
}

impl<'a> ConfigDecanter<'a> {
    fn new(log: &'a LogEvent) -> Self {
        Self { log }
    }

    fn decant_config(&self, config: &SyslogSerializerOptions) -> SyslogMessage {
        let mut app_name = self
            .get_value(&config.app_name)
            .unwrap_or_else(|| "vector".to_owned());
        let mut proc_id = self.get_value(&config.proc_id);
        let mut msg_id = self.get_value(&config.msg_id);
        if config.rfc == SyslogRFC::Rfc5424 {
            if app_name.len() > 48 {
                app_name.truncate(48);
            }
            if let Some(pid) = &mut proc_id {
                if pid.len() > 128 {
                    pid.truncate(128);
                }
            }
            if let Some(mid) = &mut msg_id {
                if mid.len() > 32 {
                    mid.truncate(32);
                }
            }
        }

        SyslogMessage {
            pri: Pri {
                facility: self.get_facility(config),
                severity: self.get_severity(config),
            },
            timestamp: self.get_timestamp(),
            hostname: self.log.get_host().map(|v| v.to_string_lossy().to_string()),
            tag: Tag {
                app_name,
                proc_id,
                msg_id,
            },
            structured_data: self.get_structured_data(),
            message: self.get_payload(config),
        }
    }

    fn get_value(&self, path: &Option<ConfigTargetPath>) -> Option<String> {
        path.as_ref()
            .and_then(|p| self.log.get(p).cloned())
            .map(|v| v.to_string_lossy().to_string())
    }

    fn get_structured_data(&self) -> Option<StructuredData> {
        self.log
            .get("structured_data")
            .and_then(|v| v.clone().into_object())
            .map(StructuredData::from)
    }

    fn get_timestamp(&self) -> DateTime<Utc> {
        if let Some(Value::Timestamp(timestamp)) = self.log.get_timestamp() {
            return *timestamp;
        }
        Utc::now()
    }

    fn get_payload(&self, config: &SyslogSerializerOptions) -> String {
        self.get_value(&config.payload_key).unwrap_or_else(|| {
            self.log
                .get_message()
                .map(|v| v.to_string_lossy().to_string())
                .unwrap_or_default()
        })
    }

    fn get_facility(&self, config: &SyslogSerializerOptions) -> Facility {
        config.facility.as_ref().map_or(Facility::User, |path| {
            self.get_syslog_code(path, Facility::from_repr, Facility::User)
        })
    }

    fn get_severity(&self, config: &SyslogSerializerOptions) -> Severity {
        config
            .severity
            .as_ref()
            .map_or(Severity::Informational, |path| {
                self.get_syslog_code(path, Severity::from_repr, Severity::Informational)
            })
    }

    fn get_syslog_code<T>(
        &self,
        path: &ConfigTargetPath,
        from_repr_fn: fn(usize) -> Option<T>,
        default_value: T,
    ) -> T
    where
        T: Copy + FromStr,
    {
        if let Some(value) = self.log.get(path).cloned() {
            let s = value.to_string_lossy();
            if let Ok(val_from_name) = s.to_ascii_lowercase().parse::<T>() {
                return val_from_name;
            }
            if let Value::Integer(n) = value {
                if let Some(val_from_num) = from_repr_fn(n as usize) {
                    return val_from_num;
                }
            }
        }
        default_value
    }
}

const NIL_VALUE: &str = "-";
const SYSLOG_V1: &str = "1";

/// The syslog RFC standard to use for formatting.
#[configurable_component]
#[derive(PartialEq, Clone, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyslogRFC {
    /// The legacy RFC3164 syslog format.
    Rfc3164,
    /// The modern RFC5424 syslog format.
    #[default]
    Rfc5424,
}

#[derive(Default, Debug)]
struct SyslogMessage {
    pri: Pri,
    timestamp: DateTime<Utc>,
    hostname: Option<String>,
    tag: Tag,
    structured_data: Option<StructuredData>,
    message: String,
}

impl SyslogMessage {
    fn encode(&self, rfc: &SyslogRFC) -> String {
        let pri_header = self.pri.encode();

        let mut parts = Vec::new();

        let timestamp_str = match rfc {
            SyslogRFC::Rfc3164 => self.timestamp.format("%b %e %H:%M:%S").to_string(),
            SyslogRFC::Rfc5424 => self
                .timestamp
                .round_subsecs(6)
                .to_rfc3339_opts(SecondsFormat::Micros, true),
        };
        parts.push(timestamp_str);
        parts.push(self.hostname.as_deref().unwrap_or(NIL_VALUE).to_string());

        let tag_str = match rfc {
            SyslogRFC::Rfc3164 => self.tag.encode_rfc_3164(),
            SyslogRFC::Rfc5424 => self.tag.encode_rfc_5424(),
        };
        parts.push(tag_str);

        let mut message_part = self.message.clone();
        if *rfc == SyslogRFC::Rfc3164 {
            message_part = Self::sanitize_rfc3164_message(&message_part);
        }

        if let Some(sd) = &self.structured_data {
            let sd_string = sd.encode();
            if !sd.elements.is_empty() {
                if *rfc == SyslogRFC::Rfc3164 {
                    if !self.message.is_empty() {
                        message_part = format!("{sd_string} {message_part}");
                    } else {
                        message_part = sd_string;
                    }
                } else {
                    parts.push(sd_string);
                }
            }
        } else if *rfc == SyslogRFC::Rfc5424 {
            parts.push(NIL_VALUE.to_string());
        }

        if !message_part.is_empty() {
            parts.push(message_part);
        }

        let main_message = parts.join(" ");

        if *rfc == SyslogRFC::Rfc5424 {
            format!("{pri_header}{SYSLOG_V1} {main_message}")
        } else {
            format!("{pri_header}{main_message}")
        }
    }

    fn sanitize_rfc3164_message(message: &str) -> String {
        message
            .chars()
            .map(|ch| if (' '..='~').contains(&ch) { ch } else { ' ' })
            .collect()
    }
}

#[derive(Default, Debug)]
struct Tag {
    app_name: String,
    proc_id: Option<String>,
    msg_id: Option<String>,
}

impl Tag {
    fn encode_rfc_3164(&self) -> String {
        let mut tag = if let Some(proc_id) = self.proc_id.as_deref() {
            format!("{}[{}]:", self.app_name, proc_id)
        } else {
            format!("{}:", self.app_name)
        };
        if tag.len() > 32 {
            tag.truncate(31);
            if !tag.ends_with(':') {
                tag.pop();
                tag.push(':');
            }
        }
        tag
    }

    fn encode_rfc_5424(&self) -> String {
        let proc_id_str = self.proc_id.as_deref().unwrap_or(NIL_VALUE);
        let msg_id_str = self.msg_id.as_deref().unwrap_or(NIL_VALUE);
        format!("{} {} {}", self.app_name, proc_id_str, msg_id_str)
    }
}

type StructuredDataMap = HashMap<String, HashMap<String, String>>;
#[derive(Debug, Default)]
struct StructuredData {
    elements: StructuredDataMap,
}

impl StructuredData {
    fn encode(&self) -> String {
        if self.elements.is_empty() {
            NIL_VALUE.to_string()
        } else {
            self.elements
                .iter()
                .fold(String::new(), |mut acc, (sd_id, sd_params)| {
                    let _ = write!(acc, "[{sd_id}");
                    for (key, value) in sd_params {
                        let esc_val = Self::escape_sd(value);
                        let _ = write!(acc, " {key}=\"{esc_val}\"");
                    }
                    let _ = write!(acc, "]");
                    acc
                })
        }
    }

    fn escape_sd(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace(']', "\\]")
    }
}

impl From<ObjectMap> for StructuredData {
    fn from(fields: ObjectMap) -> Self {
        let elements = fields
            .into_iter()
            .flat_map(|(sd_id, value)| {
                let sd_params = value
                    .into_object()?
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.to_string_lossy().to_string()))
                    .collect();
                Some((sd_id.into(), sd_params))
            })
            .collect();
        Self { elements }
    }
}

#[derive(Default, Debug)]
struct Pri {
    facility: Facility,
    severity: Severity,
}

impl Pri {
    // The last paragraph describes how to compose the enums into `PRIVAL`:
    // https://datatracker.ietf.org/doc/html/rfc5424#section-6.2.1
    fn encode(&self) -> String {
        let pri_val = (self.facility as u8 * 8) + self.severity as u8;
        format!("<{pri_val}>")
    }
}

/// Syslog facility
#[derive(Default, Debug, EnumString, FromRepr, VariantNames, Copy, Clone, PartialEq, Eq)]
#[strum(serialize_all = "kebab-case")]
#[configurable_component]
pub enum Facility {
    /// Kern
    Kern = 0,
    /// User
    #[default]
    User = 1,
    /// Mail
    Mail = 2,
    /// Daemon
    Daemon = 3,
    /// Auth
    Auth = 4,
    /// Syslog
    Syslog = 5,
    /// Lpr
    Lpr = 6,
    /// News
    News = 7,
    /// Uucp
    Uucp = 8,
    /// Cron
    Cron = 9,
    /// Authpriv
    Authpriv = 10,
    /// Ftp
    Ftp = 11,
    /// Ntp
    Ntp = 12,
    /// Security
    Security = 13,
    /// Console
    Console = 14,
    /// SolarisCron
    SolarisCron = 15,
    /// Local0
    Local0 = 16,
    /// Local1
    Local1 = 17,
    /// Local2
    Local2 = 18,
    /// Local3
    Local3 = 19,
    /// Local4
    Local4 = 20,
    /// Local5
    Local5 = 21,
    /// Local6
    Local6 = 22,
    /// Local7
    Local7 = 23,
}

/// Syslog severity
#[derive(Default, Debug, EnumString, FromRepr, VariantNames, Copy, Clone, PartialEq, Eq)]
#[strum(serialize_all = "kebab-case")]
#[configurable_component]
pub enum Severity {
    /// Emergency
    Emergency = 0,
    /// Alert
    Alert = 1,
    /// Critical
    Critical = 2,
    /// Error
    Error = 3,
    /// Warning
    Warning = 4,
    /// Notice
    Notice = 5,
    /// Informational
    #[default]
    Informational = 6,
    /// Debug
    Debug = 7,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use chrono::NaiveDate;
    use vector_core::event::Event;
    use vrl::{event_path, value};

    fn run_encode(config: SyslogSerializerConfig, event: Event) -> String {
        let mut serializer = SyslogSerializer::new(&config);
        let mut buffer = BytesMut::new();
        serializer.encode(event, &mut buffer).unwrap();
        String::from_utf8(buffer.to_vec()).unwrap()
    }

    fn create_simple_log() -> LogEvent {
        let mut log = LogEvent::from("original message");
        log.insert(
            event_path!("timestamp"),
            NaiveDate::from_ymd_opt(2025, 8, 28)
                .unwrap()
                .and_hms_micro_opt(18, 30, 00, 123456)
                .unwrap()
                .and_local_timezone(Utc)
                .unwrap(),
        );
        log.insert(event_path!("host"), "test-host.com");
        log
    }


    fn create_test_log() -> LogEvent {
        let mut log = create_simple_log();
        log.insert(event_path!("app"), "my-app");
        log.insert(event_path!("pid"), "12345");
        log.insert(event_path!("mid"), "req-abc-789");
        log.insert(event_path!("fac"), "daemon"); //3
        log.insert(event_path!("sev"), Value::from(2u8)); // Critical
        log.insert(
            event_path!("structured_data"),
            value!({"metrics": {"retries": 3}}),
        );
        log
    }

    #[test]
    fn test_rfc5424_defaults() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc5424"
        "#,
        )
        .unwrap();
        let log = create_simple_log();
        let output = run_encode(config, Event::Log(log));
        let expected =
            "<14>1 2025-08-28T18:30:00.123456Z test-host.com vector - - - original message";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_rfc5424_all_fields() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            app_name = ".app"
            proc_id = ".pid"
            msg_id = ".mid"
            facility = ".fac"
            severity = ".sev"
            payload_key = ".message"
        "#,
        )
        .unwrap();
        let log = create_test_log();
        let output = run_encode(config, Event::Log(log));
        let expected = "<26>1 2025-08-28T18:30:00.123456Z test-host.com my-app 12345 req-abc-789 [metrics retries=\"3\"] original message";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_rfc3164_all_fields() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc3164"
            facility = ".fac"
            severity = ".sev"
            app_name = ".app"
            proc_id = ".pid"
            payload_key = ".message"
        "#,
        )
        .unwrap();
        let log = create_test_log();
        let output = run_encode(config, Event::Log(log));
        let expected = "<26>Aug 28 18:30:00 test-host.com my-app[12345]: [metrics retries=\"3\"] original message";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_parsing_logic() {
        let mut log = LogEvent::from("test message");
        let config_fac =
            toml::from_str::<SyslogSerializerOptions>(r#"facility = ".syslog_facility""#).unwrap();
        let config_sev =
            toml::from_str::<SyslogSerializerOptions>(r#"severity = ".syslog_severity""#).unwrap();
        //check lowercase and digit
        log.insert(event_path!("syslog_facility"), "daemon");
        log.insert(event_path!("syslog_severity"), "critical");
        let decanter = ConfigDecanter::new(&log);
        let facility = decanter.get_facility(&config_fac);
        let severity = decanter.get_severity(&config_sev);
        assert_eq!(facility, Facility::Daemon);
        assert_eq!(severity, Severity::Critical);

        //check uppercase
        log.insert(event_path!("syslog_facility"), "DAEMON");
        log.insert(event_path!("syslog_severity"), "CRITICAL");
        let decanter = ConfigDecanter::new(&log);
        let facility = decanter.get_facility(&config_fac);
        let severity = decanter.get_severity(&config_sev);
        assert_eq!(facility, Facility::Daemon);
        assert_eq!(severity, Severity::Critical);

        //check digit
        log.insert(event_path!("syslog_facility"), Value::from(3u8));
        log.insert(event_path!("syslog_severity"), Value::from(2u8));
        let decanter = ConfigDecanter::new(&log);
        let facility = decanter.get_facility(&config_fac);
        let severity = decanter.get_severity(&config_sev);
        assert_eq!(facility, Facility::Daemon);
        assert_eq!(severity, Severity::Critical);

        //check defaults with empty config
        let empty_config =
            toml::from_str::<SyslogSerializerOptions>(r#"facility = ".missing_field""#).unwrap();
        let default_facility = decanter.get_facility(&empty_config);
        let default_severity = decanter.get_severity(&empty_config);
        assert_eq!(default_facility, Facility::User);
        assert_eq!(default_severity, Severity::Informational);
    }

    #[test]
    fn test_rfc3164_sanitization() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc3164"
    "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(
            event_path!("message"),
            "A\nB\tC, Привіт D, E\u{0007}F", //newline, tab, unicode
        );

        let output = run_encode(config, Event::Log(log));
        let expected_message = "A B C,        D, E F";
        assert!(output.ends_with(expected_message));
    }

    #[test]
    fn test_rfc5424_field_truncation() {
        let long_string = "vector".repeat(50);

        let mut log = create_simple_log();
        log.insert(event_path!("long_app_name"), long_string.clone());
        log.insert(event_path!("long_proc_id"), long_string.clone());
        log.insert(event_path!("long_msg_id"), long_string.clone());

        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc5424"
        app_name = ".long_app_name"
        proc_id = ".long_proc_id"
        msg_id = ".long_msg_id"
    "#,
        )
        .unwrap();

        let decanter = ConfigDecanter::new(&log);
        let message = decanter.decant_config(&config.syslog);

        assert_eq!(message.tag.app_name.len(), 48);
        assert_eq!(message.tag.proc_id.unwrap().len(), 128);
        assert_eq!(message.tag.msg_id.unwrap().len(), 32);
    }

    #[test]
    fn test_rfc3164_tag_truncation() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc3164"
        facility = "user"
        severity = "notice"
        app_name = ".app_name"
        proc_id = ".proc_id"
    "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(
            event_path!("app_name"),
            "this-is-a-very-long-application-name",
        );
        log.insert(event_path!("proc_id"), "1234567890");

        let output = run_encode(config, Event::Log(log));
        let expected_tag = "this-is-a-very-long-applicatio:";
        assert!(output.contains(expected_tag));
    }
}
