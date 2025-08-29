use bytes::{BufMut, BytesMut};
use chrono::{DateTime, NaiveDate, SecondsFormat, SubsecRound, Utc};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
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

/// A configuration value that can be either a static value or a dynamic path.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynamicOrStatic<T: 'static> {
    /// A static, fixed value.
    Static(T),
    /// A dynamic value read from a field in the event.
    Dynamic(String),
}

/// Syslog serializer options.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct SyslogSerializerOptions {
    /// RFC to use for formatting.
    #[serde(default)]
    rfc: SyslogRFC,
    /// Syslog facility.
    #[serde(
        default = "default_facility",
        deserialize_with = "deserialize_facility"
    )]
    facility: DynamicOrStatic<Facility>,
    /// Syslog severity.
    #[serde(
        default = "default_severity",
        deserialize_with = "deserialize_severity"
    )]
    severity: DynamicOrStatic<Severity>,
    /// App Name. Can be a static string or a dynamic field path like "$.app".
    app_name: Option<String>,
    /// Proc ID. Can be a static string or a dynamic field path like "$.pid".
    proc_id: Option<String>,
    /// Msg ID. Can be a static string or a dynamic field path like "$.request_id".
    msg_id: Option<String>,
    /// The key to use for the main message payload.
    payload_key: Option<String>,
}

impl Default for SyslogSerializerOptions {
    fn default() -> Self {
        Self {
            rfc: SyslogRFC::default(),
            facility: default_facility(),
            severity: default_severity(),
            app_name: None,
            proc_id: None,
            msg_id: None,
            payload_key: Some("message".to_string()),
        }
    }
}

fn default_facility() -> DynamicOrStatic<Facility> {
    DynamicOrStatic::Static(Facility::User)
}
fn default_severity() -> DynamicOrStatic<Severity> {
    DynamicOrStatic::Static(Severity::Informational)
}

fn deserialize_facility<'de, D>(deserializer: D) -> Result<DynamicOrStatic<Facility>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.starts_with("$.") {
        Ok(DynamicOrStatic::Dynamic(s))
    } else if let Ok(val) = s.parse::<Facility>() {
        Ok(DynamicOrStatic::Static(val))
    } else if let Ok(num) = s.parse::<usize>() {
        Facility::from_repr(num)
            .map(DynamicOrStatic::Static)
            .ok_or_else(|| {
                serde::de::Error::custom(format!("Invalid facility number: {}. Must be 0-23.", s))
            })
    } else {
        Err(serde::de::Error::custom(format!(
            "Invalid facility '{}'. Expected a name, integer, or path.",
            s
        )))
    }
}

fn deserialize_severity<'de, D>(deserializer: D) -> Result<DynamicOrStatic<Severity>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.starts_with("$.") {
        Ok(DynamicOrStatic::Dynamic(s))
    } else if let Ok(val) = s.parse::<Severity>() {
        Ok(DynamicOrStatic::Static(val))
    } else if let Ok(num) = s.parse::<usize>() {
        Severity::from_repr(num)
            .map(DynamicOrStatic::Static)
            .ok_or_else(|| {
                serde::de::Error::custom(format!("Invalid severity number: {}. Must be 0-7.", s))
            })
    } else {
        Err(serde::de::Error::custom(format!(
            "Invalid severity '{}'. Expected a name, integer, or path.",
            s
        )))
    }
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
            let syslog_message = ConfigDecanter::new(log_event).decant_config(&self.config.syslog);

            let vec = syslog_message
                .encode(&self.config.syslog.rfc)
                .as_bytes()
                .to_vec();
            buffer.put_slice(&vec);
        }

        Ok(())
    }
}

struct ConfigDecanter {
    log: LogEvent,
}

impl ConfigDecanter {
    fn new(log: LogEvent) -> Self {
        Self { log }
    }

    fn decant_config(&self, config: &SyslogSerializerOptions) -> SyslogMessage {
        SyslogMessage {
            pri: Pri {
                facility: self.get_facility(config),
                severity: self.get_severity(config),
            },
            timestamp: self.get_timestamp(),
            hostname: self.value_by_key("hostname"),
            tag: Tag {
                app_name: self
                    .replace_if_proxied_opt(&config.app_name)
                    .unwrap_or_else(|| "vector".to_owned()),
                proc_id: self.replace_if_proxied_opt(&config.proc_id),
                msg_id: self.replace_if_proxied_opt(&config.msg_id),
            },
            structured_data: self.get_structured_data(),
            message: self.get_message(config),
        }
    }

    fn replace_if_proxied_opt(&self, value: &Option<String>) -> Option<String> {
        value.as_ref().and_then(|v| self.replace_if_proxied(v))
    }

    // When the value has the expected prefix, perform a lookup for a field key without that prefix part.
    // A failed lookup returns `None`, while a value without the prefix uses the config value as-is.
    //
    // Q: Why `$.message.` as the prefix? (Appears to be JSONPath syntax?)
    // NOTE: Originally named in PR as: `get_field_or_config()`
    fn replace_if_proxied(&self, value: &str) -> Option<String> {
        if let Some(field_key) = value.strip_prefix("$.") {
            self.value_by_key(field_key)
        } else {
            Some(value.to_owned())
        }
    }

    fn value_by_key(&self, field_key: &str) -> Option<String> {
        self.log
            .get(field_key)
            .map(|v| v.to_string_lossy().to_string())
    }

    fn get_structured_data(&self) -> Option<StructuredData> {
        self.log
            .get("structured_data")
            .and_then(|v| v.clone().into_object())
            .map(StructuredData::from)
    }

    fn get_timestamp(&self) -> DateTime<Utc> {
        if let Some(&Value::Timestamp(timestamp)) = self.log.get("@timestamp") {
            timestamp
        } else if let Some(Value::Timestamp(timestamp)) = self.log.get_timestamp() {
            *timestamp
        } else {
            tracing::warn!("Timestamp not found in event, using current time.");
            Utc::now()
        }
    }

    fn get_message(&self, config: &SyslogSerializerOptions) -> String {
        if let Some(key) = &config.payload_key {
            // If a key is configured, try to use it.
            self.value_by_key(key).unwrap_or_default()
        } else {
            // Otherwise, fall back to the default log message.
            self.log
                .get_message()
                .map(|v| v.to_string_lossy().to_string())
                .unwrap_or_default()
        }
    }

    fn get_facility(&self, config: &SyslogSerializerOptions) -> Facility {
        match &config.facility {
            DynamicOrStatic::Static(f) => *f,
            DynamicOrStatic::Dynamic(path) => self
                .value_by_key(path.trim_start_matches("$."))
                .and_then(|s| {
                    s.parse::<Facility>()
                        .ok()
                        .or_else(|| s.parse::<usize>().ok().and_then(Facility::from_repr))
                })
                .unwrap_or_else(|| {
                    tracing::warn!(
                        message = "Failed to resolve or parse dynamic facility, using default.",
                        ?path
                    );
                    Facility::default()
                }),
        }
    }

    fn get_severity(&self, config: &SyslogSerializerOptions) -> Severity {
        match &config.severity {
            DynamicOrStatic::Static(s) => *s,
            DynamicOrStatic::Dynamic(path) => self
                .value_by_key(path.trim_start_matches("$."))
                .and_then(|s| {
                    s.parse::<Severity>()
                        .ok()
                        .or_else(|| s.parse::<usize>().ok().and_then(Severity::from_repr))
                })
                .unwrap_or_else(|| {
                    tracing::warn!(
                        message = "Failed to resolve or parse dynamic severity, using default.",
                        ?path
                    );
                    Severity::default()
                }),
        }
    }
}

//
// SyslogMessage support
//

const NIL_VALUE: &str = "-";
const SYSLOG_V1: &str = "1";

/// Syslog RFC
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyslogRFC {
    /// The legacy RFC3164 syslog format.
    Rfc3164,
    /// The modern RFC5424 syslog format.
    #[default]
    Rfc5424,
}

// ABNF definition:
// https://datatracker.ietf.org/doc/html/rfc5424#section-6
// https://datatracker.ietf.org/doc/html/rfc5424#section-6.2
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
        // Q: NIL_VALUE is unlikely? Technically invalid for RFC 3164:
        // https://datatracker.ietf.org/doc/html/rfc5424#section-6.2.4
        // https://datatracker.ietf.org/doc/html/rfc3164#section-4.1.2
        let hostname = self.hostname.as_deref().unwrap_or(NIL_VALUE);
        let structured_data = self.structured_data.as_ref().map(|sd| sd.encode());

        let fields_encoded = match rfc {
            SyslogRFC::Rfc3164 => {
                // TIMESTAMP field format:
                // https://datatracker.ietf.org/doc/html/rfc3164#section-4.1.2
                // https://docs.rs/chrono/latest/chrono/format/strftime/index.html
                //
                // TODO: Should this remain as UTC or adjust to the local TZ of the environment (or Vector config)?
                // RFC 5424 suggests (when adapting for RFC 3164) to present a timestamp with the local TZ of the log source:
                // https://www.rfc-editor.org/rfc/rfc5424#appendix-A.1
                let timestamp = self.timestamp.format("%b %e %H:%M:%S").to_string();
                // MSG part begins with TAG field + optional context:
                // https://datatracker.ietf.org/doc/html/rfc3164#section-4.1.3
                let mut msg_start = self.tag.encode_rfc_3164();
                // When RFC 5424 "Structured Data" is available, it can be compatible with RFC 3164
                // by including it in the RFC 3164 `CONTENT` field (part of MSG):
                // https://datatracker.ietf.org/doc/html/rfc5424#appendix-A.1
                if let Some(sd) = structured_data.as_deref() {
                    msg_start.push(' ');
                    msg_start.push_str(sd);
                }
                [timestamp.as_str(), hostname, &msg_start].join(" ")
            }
            SyslogRFC::Rfc5424 => {
                // HEADER part fields:
                // https://datatracker.ietf.org/doc/html/rfc5424#section-6.2
                // TIME-FRAC max length is 6 digits (microseconds):
                // https://datatracker.ietf.org/doc/html/rfc5424#section-6
                // TODO: Likewise for RFC 5424, as UTC the offset will always render as `Z` if not configurable.
                let timestamp = self
                    .timestamp
                    .round_subsecs(6)
                    .to_rfc3339_opts(SecondsFormat::AutoSi, true);
                let tag = self.tag.encode_rfc_5424();
                let sd = structured_data.as_deref().unwrap_or(NIL_VALUE);
                [SYSLOG_V1, timestamp.as_str(), hostname, &tag, sd].join(" ")
            }
        };

        [&self.pri.encode(), &fields_encoded, " ", &self.message].concat()
    }
}

#[derive(Default, Debug)]
struct Tag {
    app_name: String,
    proc_id: Option<String>,
    msg_id: Option<String>,
}

impl Tag {
    // Roughly equivalent - RFC 5424 fields can compose the start of
    // an RFC 3164 MSG part (TAG + CONTENT fields):
    // https://datatracker.ietf.org/doc/html/rfc5424#appendix-A.1
    fn encode_rfc_3164(&self) -> String {
        if let Some(context) = self.proc_id.as_deref().or(self.msg_id.as_deref()) {
            format!("{}[{}]:", self.app_name, context)
        } else {
            format!("{}:", self.app_name)
        }
    }

    // TAG was split into separate fields: APP-NAME, PROCID, MSGID
    // https://datatracker.ietf.org/doc/html/rfc5424#section-6.2.5
    fn encode_rfc_5424(&self) -> String {
        [
            &self.app_name,
            self.proc_id.as_deref().unwrap_or(NIL_VALUE),
            self.msg_id.as_deref().unwrap_or(NIL_VALUE),
        ]
        .join(" ")
    }
}

// Structured Data:
// https://datatracker.ietf.org/doc/html/rfc5424#section-6.3
// An SD-ELEMENT consists of a name (SD-ID) + parameter key-value pairs (SD-PARAM)
type StructuredDataMap = HashMap<String, HashMap<String, String>>;
#[derive(Debug, Default)]
struct StructuredData {
    elements: StructuredDataMap,
}

// Used by `SyslogMessage::encode()`
impl StructuredData {
    fn encode(&self) -> String {
        if self.elements.is_empty() {
            NIL_VALUE.to_string()
        } else {
            self.elements
                .iter()
                .map(|(sd_id, sd_params)| {
                    let params_encoded = sd_params
                        .iter()
                        .map(|(key, value)| format!(" {}=\"{}\"", key, value))
                        .collect::<String>();
                    format!("[{}{}]", sd_id, params_encoded)
                })
                .collect()
        }
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
        let prival = (self.facility as u8 * 8) + self.severity as u8;
        format!("<{}>", prival)
    }
}

// Facility + Severity mapping from Name => Ordinal number:
// NOTE:
// - Vector component enforces variant doc-comments, even though it's pointless for these enums?
// - `configurable_component(no_deser)` is used to match the existing functionality to support deserializing config with ordinal mapping.
// - `EnumString` with `strum(serialize_all = "kebab-case")` provides the `FromStr` support, while `FromRepr` handles ordinal support.
// - `VariantNames` assists with generating the equivalent `de::Error::unknown_variant` serde error message.

/// Syslog facility
#[derive(Default, Debug, EnumString, FromRepr, VariantNames, Copy, Clone, PartialEq, Eq)]
#[strum(serialize_all = "kebab-case")]
#[configurable_component]
enum Facility {
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
    /// LPR
    LPR = 6,
    /// News
    News = 7,
    /// UUCP
    UUCP = 8,
    /// Cron
    Cron = 9,
    /// AuthPriv
    AuthPriv = 10,
    /// FTP
    FTP = 11,
    /// NTP
    NTP = 12,
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
enum Severity {
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
            event_path!("@timestamp"),
            NaiveDate::from_ymd_opt(2025, 8, 28)
                .unwrap()
                .and_hms_opt(18, 30, 00)
                .unwrap()
                .and_local_timezone(Utc)
                .unwrap(),
        );
        log.insert(event_path!("hostname"), "test-host.com");
        log
    }

    fn create_test_log() -> LogEvent {
        let mut log = create_simple_log();
        log.insert(event_path!("app_name"), "my-app");
        log.insert(event_path!("proc_id"), "12345");
        log.insert(event_path!("msg_id"), "req-abc-789");
        log.insert(event_path!("syslog_facility"), "daemon");
        log.insert(event_path!("syslog_severity"), Value::from(2u64)); // Critical
        log.insert(
            event_path!("structured_data"),
            value!({"metrics": {"retries": 3}}),
        );
        log
    }

    #[test]
    fn test_rfc5424_basic_static_config() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc5424"
            facility = "user"
            severity = "notice"
            app_name = "static-app"
            proc_id = "987"
            msg_id = "static-msg-id"
            payload_key = "message"
        "#,
        )
        .unwrap();
        let log = create_simple_log();
        let output = run_encode(config, Event::Log(log));
        let expected = "<13>1 2025-08-28T18:30:00Z test-host.com static-app 987 static-msg-id - original message";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_rfc5424_all_fields_dynamic() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            app_name = "$.app_name"
            proc_id = "$.proc_id"
            msg_id = "$.msg_id"
        "#,
        )
        .unwrap();
        let log = create_test_log();
        let output = run_encode(config, Event::Log(log));
        let expected = "<14>1 2025-08-28T18:30:00Z test-host.com my-app 12345 req-abc-789 [metrics retries=\"3\"] original message";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_rfc3164_all_fields_dynamic() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc3164"
            facility = "auth"
            severity = "informational"
            app_name = "$.app_name"
            proc_id = "$.proc_id"
        "#,
        )
        .unwrap();
        let log = create_test_log();
        let output = run_encode(config, Event::Log(log));
        let expected = "<38>Aug 28 18:30:00 test-host.com my-app[12345]: [metrics retries=\"3\"] original message";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_dynamic_facility_and_severity() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            facility = "$.syslog_facility"
            severity = "$.syslog_severity"
        "#,
        )
        .unwrap();
        let log = create_test_log();
        let output = run_encode(config, Event::Log(log));
        // Facility "daemon" (3) and Severity "critical" (2) -> 3 * 8 + 2 = 26
        let expected = "<26>1 2025-08-28T18:30:00Z test-host.com vector - - [metrics retries=\"3\"] original message";
        assert_eq!(output, expected);
    }
}
