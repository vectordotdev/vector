use bytes::{BufMut, BytesMut};
use tokio_util::codec::Encoder;
use vector_core::{config::DataType, event::{Event, LogEvent}, schema};
use chrono::{DateTime, SecondsFormat, Local};
use vrl::value::Value;
use serde::{de, Deserialize};
use vector_config::configurable_component;

const NIL_VALUE: &'static str = "-";

/// Syslog RFC
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyslogRFC {
    /// RFC 3164
    Rfc3164,

    /// RFC 5424
    Rfc5424
}

impl Default for SyslogRFC {
    fn default() -> Self {
        SyslogRFC::Rfc5424
    }
}

/// Syslog facility
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
enum Facility {
    /// Syslog facility ordinal number
    Fixed(u8),

    /// Syslog facility name
    Field(String)
}

impl Default for Facility {
    fn default() -> Self {
        Facility::Fixed(1)
    }
}

/// Syslog severity
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
enum Severity {
    /// Syslog severity ordinal number
    Fixed(u8),

    /// Syslog severity name
    Field(String)
}

impl Default for Severity {
    fn default() -> Self {
        Severity::Fixed(6)
    }
}

/// Config used to build a `SyslogSerializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct SyslogSerializerConfig {
    /// RFC
    #[serde(default)]
    rfc: SyslogRFC,

    /// Facility
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_facility")]
    facility: Facility,

    /// Severity
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_severity")]
    severity: Severity,

    /// Tag
    #[serde(default)]
    tag: String,

    /// Trim prefix
    trim_prefix: Option<String>,

    /// Payload key
    #[serde(default)]
    payload_key: String,

    /// Add log source
    #[serde(default)]
    add_log_source: bool,

    /// App Name, RFC 5424 only
    #[serde(default = "default_app_name")]
    app_name: String,

    /// Proc ID, RFC 5424 only
    #[serde(default = "default_nil_value")]
    proc_id: String,

    /// Msg ID, RFC 5424 only
    #[serde(default = "default_nil_value")]
    msg_id: String
}

impl SyslogSerializerConfig {
    /// Build the `SyslogSerializer` from this configuration.
    pub fn build(&self) -> SyslogSerializer {
        SyslogSerializer::new(&self)
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

/// Serializer that converts an `Event` to bytes using the Syslog format.
#[derive(Debug, Clone)]
pub struct SyslogSerializer {
    config: SyslogSerializerConfig
}

impl SyslogSerializer {
    /// Creates a new `SyslogSerializer`.
    pub fn new(conf: &SyslogSerializerConfig) -> Self {
        Self { config: conf.clone() }
    }
}

impl Encoder<Event> for SyslogSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        match event {
            Event::Log(log) => {
                let mut buf = String::from("<");
                let pri = get_num_facility(&self.config.facility, &log) * 8 + get_num_severity(&self.config.severity, &log);
                buf.push_str(&pri.to_string());
                buf.push_str(">");
                match self.config.rfc {
                    SyslogRFC::Rfc3164 => {
                        let timestamp = get_timestamp(&log);
                        let formatted_timestamp = format!(" {} ", timestamp.format("%b %e %H:%M:%S"));
                        buf.push_str(&formatted_timestamp);
                        buf.push_str(&get_field("hostname", &log));
                        buf.push(' ');
                        buf.push_str(&get_field_or_config(&self.config.tag, &log));
                        buf.push_str(": ");
                        if self.config.add_log_source {
                            add_log_source(&log, &mut buf);
                        }
                    },
                    SyslogRFC::Rfc5424 => {
                        buf.push_str("1 ");
                        let timestamp = get_timestamp(&log);
                        buf.push_str(&timestamp.to_rfc3339_opts(SecondsFormat::Millis, true));
                        buf.push(' ');
                        buf.push_str(&get_field("hostname", &log));
                        buf.push(' ');
                        buf.push_str(&get_field_or_config(&&self.config.app_name, &log));
                        buf.push(' ');
                        buf.push_str(&get_field_or_config(&&self.config.proc_id, &log));
                        buf.push(' ');
                        buf.push_str(&get_field_or_config(&&self.config.msg_id, &log));
                        buf.push_str(" - "); // no structured data
                        if self.config.add_log_source {
                            add_log_source(&log, &mut buf);
                        }
                    }
                }
                let mut payload = if self.config.payload_key.is_empty() {
                    serde_json::to_vec(&log).unwrap_or_default()
                } else {
                    get_field(&&self.config.payload_key, &log).as_bytes().to_vec()
                };
                let mut vec = buf.as_bytes().to_vec();
                vec.append(&mut payload);
                buffer.put_slice(&vec);
            },
            _ => {}
        }
        Ok(())
    }
}

fn deserialize_facility<'de, D>(d: D) -> Result<Facility, D::Error>
    where D: de::Deserializer<'de>
{
    let value: String = String::deserialize(d)?;
    let num_value = value.parse::<u8>();
    match num_value {
        Ok(num) => {
            if num > 23 {
                return Err(de::Error::invalid_value(de::Unexpected::Unsigned(num as u64), &"facility number too large"));
            } else {
                return Ok(Facility::Fixed(num));
            }
        }
        Err(_) => {
            if let Some(field_name) = value.strip_prefix("$.message.") {
                return Ok(Facility::Field(field_name.to_string()));
            } else {
                let num = match value.to_uppercase().as_str() {
                    "KERN" => 0,
                    "USER" => 1,
                    "MAIL" => 2,
                    "DAEMON" => 3,
                    "AUTH" => 4,
                    "SYSLOG" => 5,
                    "LPR" => 6,
                    "NEWS" => 7,
                    "UUCP" => 8,
                    "CRON" => 9,
                    "AUTHPRIV" => 10,
                    "FTP" => 11,
                    "NTP" => 12,
                    "SECURITY" => 13,
                    "CONSOLE" => 14,
                    "SOLARIS-CRON" => 15,
                    "LOCAL0" => 16,
                    "LOCAL1" => 17,
                    "LOCAL2" => 18,
                    "LOCAL3" => 19,
                    "LOCAL4" => 20,
                    "LOCAL5" => 21,
                    "LOCAL6" => 22,
                    "LOCAL7" => 23,
                    _ => 24,
                };
                if num > 23 {
                    return Err(de::Error::invalid_value(de::Unexpected::Unsigned(num as u64), &"unknown facility"));
                } else {
                    return Ok(Facility::Fixed(num))
                }
            }
        }
    }
}

fn deserialize_severity<'de, D>(d: D) -> Result<Severity, D::Error>
    where D: de::Deserializer<'de>
{
    let value: String = String::deserialize(d)?;
    let num_value = value.parse::<u8>();
    match num_value {
        Ok(num) => {
            if num > 7 {
                return Err(de::Error::invalid_value(de::Unexpected::Unsigned(num as u64), &"severity number too large"))
            } else {
                return Ok(Severity::Fixed(num))
            }
        }
        Err(_) => {
            if let Some(field_name) = value.strip_prefix("$.message.") {
                return Ok(Severity::Field(field_name.to_string()));
            } else {
                let num = match value.to_uppercase().as_str() {
                    "EMERGENCY" => 0,
                    "ALERT" => 1,
                    "CRITICAL" => 2,
                    "ERROR" => 3,
                    "WARNING" => 4,
                    "NOTICE" => 5,
                    "INFORMATIONAL" => 6,
                    "DEBUG" => 7,
                    _ => 8,
                };
                if num > 7 {
                    return Err(de::Error::invalid_value(de::Unexpected::Unsigned(num as u64), &"unknown severity"))
                } else {
                    return Ok(Severity::Fixed(num))
                }
            }
        }
    }
}

fn default_app_name() -> String {
    String::from("vector")
}

fn default_nil_value() -> String {
    String::from(NIL_VALUE)
}

fn add_log_source(log: &LogEvent, buf: &mut String) {
    buf.push_str("namespace_name=");
    buf.push_str(&String::from_utf8(
        log
        .get("kubernetes.namespace_name")
        .map(|h| h.coerce_to_bytes())
        .unwrap_or_default().to_vec()
    ).unwrap());
    buf.push_str(", container_name=");
    buf.push_str(&String::from_utf8(
        log
        .get("kubernetes.container_name")
        .map(|h| h.coerce_to_bytes())
        .unwrap_or_default().to_vec()
    ).unwrap());
    buf.push_str(", pod_name=");
    buf.push_str(&String::from_utf8(
        log
        .get("kubernetes.pod_name")
        .map(|h| h.coerce_to_bytes())
        .unwrap_or_default().to_vec()
    ).unwrap());
    buf.push_str(", message=");
}

fn get_num_facility(config_facility: &Facility, log: &LogEvent) -> u8 {
    match config_facility {
        Facility::Fixed(num) => return *num,
        Facility::Field(field_name) => {
            if let Some(field_value) = log.get(field_name.as_str()) {
                let field_value_string = String::from_utf8(field_value.coerce_to_bytes().to_vec()).unwrap_or_default();
                let num_value = field_value_string.parse::<u8>();
                match num_value {
                    Ok(num) => {
                        if num > 23 {
                            return 1 // USER
                        } else {
                            return num
                        }
                    }
                    Err(_) => {
                            let num = match field_value_string.to_uppercase().as_str() {
                                "KERN" => 0,
                                "USER" => 1,
                                "MAIL" => 2,
                                "DAEMON" => 3,
                                "AUTH" => 4,
                                "SYSLOG" => 5,
                                "LPR" => 6,
                                "NEWS" => 7,
                                "UUCP" => 8,
                                "CRON" => 9,
                                "AUTHPRIV" => 10,
                                "FTP" => 11,
                                "NTP" => 12,
                                "SECURITY" => 13,
                                "CONSOLE" => 14,
                                "SOLARIS-CRON" => 15,
                                "LOCAL0" => 16,
                                "LOCAL1" => 17,
                                "LOCAL2" => 18,
                                "LOCAL3" => 19,
                                "LOCAL4" => 20,
                                "LOCAL5" => 21,
                                "LOCAL6" => 22,
                                "LOCAL7" => 23,
                                _ => 24,
                            };
                            if num > 23 {
                                return 1 // USER
                            } else {
                                return num
                            }
                        }
                    }
            } else {
                return 1 // USER
            }
        }
    }
}

fn get_num_severity(config_severity: &Severity, log: &LogEvent) -> u8 {
    match config_severity {
        Severity::Fixed(num) => return *num,
        Severity::Field(field_name) => {
            if let Some(field_value) = log.get(field_name.as_str()) {
                let field_value_string = String::from_utf8(field_value.coerce_to_bytes().to_vec()).unwrap_or_default();
                let num_value = field_value_string.parse::<u8>();
                match num_value {
                    Ok(num) => {
                        if num > 7 {
                            return 6 // INFORMATIONAL
                        } else {
                            return num
                        }
                    }
                    Err(_) => {
                            let num = match field_value_string.to_uppercase().as_str() {
                                "EMERGENCY" => 0,
                                "ALERT" => 1,
                                "CRITICAL" => 2,
                                "ERROR" => 3,
                                "WARNING" => 4,
                                "NOTICE" => 5,
                                "INFORMATIONAL" => 6,
                                "DEBUG" => 7,
                                _ => 8,
                            };
                            if num > 7 {
                                return 6 // INFORMATIONAL
                            } else {
                                return num
                            }
                        }
                    }
            } else {
                return 6 // INFORMATIONAL
            }
        }
    }
}

fn get_field_or_config(config_name: &String, log: &LogEvent) -> String {
    if let Some(field_name) = config_name.strip_prefix("$.message.") {
        return get_field(field_name, log)
    } else {
        return config_name.clone()
    }
}

fn get_field(field_name: &str, log: &LogEvent) -> String {
    if let Some(field_value) = log.get(field_name) {
        return String::from_utf8(field_value.coerce_to_bytes().to_vec()).unwrap_or_default();
    } else {
        return NIL_VALUE.to_string()
    }
}

fn get_timestamp(log: &LogEvent) -> DateTime::<Local> {
    match log.get("@timestamp") {
        Some(value) => {
            if let Value::Timestamp(timestamp) = value {
                DateTime::<Local>::from(*timestamp)
            } else {
                Local::now()
            }
        },
        _ => Local::now()
    }
}

