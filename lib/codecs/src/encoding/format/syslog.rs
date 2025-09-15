use bytes::{BufMut, BytesMut};
use chrono::{DateTime, SecondsFormat, SubsecRound, Utc};
use regex::Regex;
use serde::{Deserialize, Deserializer};
use std::{collections::HashMap, fmt::Write, str::FromStr, sync::LazyLock};
use strum::{EnumString, FromRepr, VariantNames};
use tokio_util::codec::Encoder;
use vector_config::configurable_component;
use vector_core::{
    config::DataType,
    event::{Event, LogEvent, Value},
    schema,
};
use vrl::{path::parse_target_path, value::ObjectMap};

/// Config used to build a `SyslogSerializer`.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(default)]
pub struct SyslogSerializerConfig {
    /// A list of fields to exclude from the encoded event.
    #[serde(default)]
    pub except_fields: Vec<String>,
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
    /// A dynamic value read from a field in the event using `$.` path syntax.
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
    /// `tag` is supported as an alias for `app_name` for RFC3164 compatibility.
    #[serde(alias = "tag")]
    app_name: Option<String>,
    /// Proc ID. Can be a static string or a dynamic field path like "$.pid".
    proc_id: Option<String>,
    /// Msg ID. Can be a static string or a dynamic field path like "$.msg_id".
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
            payload_key: None,
        }
    }
}

fn default_facility() -> DynamicOrStatic<Facility> {
    DynamicOrStatic::Static(Facility::User)
}
fn default_severity() -> DynamicOrStatic<Severity> {
    DynamicOrStatic::Static(Severity::Informational)
}

// Generic helper.
fn deserialize_syslog_code<'de, D, T>(
    deserializer: D,
    type_name: &'static str,
    max_value: usize,
    from_repr_fn: fn(usize) -> Option<T>,
) -> Result<DynamicOrStatic<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr + VariantNames,
{
    let s = String::deserialize(deserializer)?;
    if s.starts_with("$.") {
        Ok(DynamicOrStatic::Dynamic(s))
    } else {
        parse_syslog_code(&s, from_repr_fn)
            .map(DynamicOrStatic::Static)
            .ok_or_else(|| {
                serde::de::Error::custom(format!(
                    "Invalid {type_name}: '{s}'. Expected a name, integer 0-{max_value}, or path."
                ))
            })
    }
}

fn parse_syslog_code<T>(s: &str, from_repr_fn: fn(usize) -> Option<T>) -> Option<T>
where
    T: FromStr,
{
    if let Ok(value_from_name) = s.parse::<T>() {
        return Some(value_from_name);
    }
    if let Ok(value_from_number) = s.parse::<u64>() {
        return from_repr_fn(value_from_number as usize);
    }
    None
}

fn deserialize_facility<'de, D>(deserializer: D) -> Result<DynamicOrStatic<Facility>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_syslog_code(deserializer, "facility", 23, Facility::from_repr)
}

fn deserialize_severity<'de, D>(deserializer: D) -> Result<DynamicOrStatic<Severity>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_syslog_code(deserializer, "severity", 7, Severity::from_repr)
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
        if let Event::Log(mut log_event) = event {
            let syslog_message = ConfigDecanter::new(&mut log_event)
                .decant_config(&self.config.syslog, &self.config.except_fields);
            let vec = syslog_message
                .encode(&self.config.syslog.rfc)
                .as_bytes()
                .to_vec();
            buffer.put_slice(&vec);
        }

        Ok(())
    }
}

// Adapts a `LogEvent` into a `SyslogMessage` based on config from `SyslogSerializerOptions`:
// - Splits off the responsibility of encoding logic to `SyslogMessage` (which is not dependent upon Vector types).
// - Majority of methods are only needed to support the `decant_config()` operation.
struct ConfigDecanter<'a> {
    log: &'a mut LogEvent,
}

impl<'a> ConfigDecanter<'a> {
    fn new(log: &'a mut LogEvent) -> Self {
        Self { log }
    }

    fn decant_config(
        &mut self,
        config: &SyslogSerializerOptions,
        except_fields: &Vec<String>,
    ) -> SyslogMessage {
        SyslogMessage {
            pri: self.build_pri(config),
            timestamp: self.get_timestamp(),
            // TODO: use self.log.get_host() -> unit test failed
            hostname: self
                .get_value("hostname")
                .map(|v| v.to_string_lossy().to_string()),
            tag: self.build_tag(&config),
            structured_data: self.get_structured_data(),
            message: self.get_payload(config, except_fields),
        }
    }

    fn build_pri(&mut self, config: &SyslogSerializerOptions) -> Pri {
        Pri {
            facility: self.get_facility(config),
            severity: self.get_severity(config),
        }
    }

    fn build_tag(&mut self, config: &&SyslogSerializerOptions) -> Tag {
        Tag {
            app_name: self
                .get_field_or_static(&config.app_name)
                .unwrap_or_else(|| "vector".to_owned()),
            proc_id: self.get_field_or_static(&config.proc_id),
            msg_id: self.get_field_or_static(&config.msg_id),
        }
    }

    fn get_value(&self, key: &str) -> Option<Value> {
        if let Some(field_path) = key.strip_prefix("$.") {
            self.log.get(field_path).cloned()
        } else {
            self.log.get(key).cloned()
        }
    }

    fn get_syslog_code<T>(
        &self,
        config_value: &DynamicOrStatic<T>,
        from_repr_fn: fn(usize) -> Option<T>,
        default_value: T,
    ) -> T
    where
        T: Copy + FromStr,
    {
        match config_value {
            DynamicOrStatic::Static(val) => *val,
            DynamicOrStatic::Dynamic(path) => self
                .get_value(path)
                .and_then(|value| {
                    let s = value.to_string_lossy();
                    parse_syslog_code(&s, from_repr_fn)
                })
                .unwrap_or_else(|| {
                    tracing::warn!(
                        message = "Failed to resolve or parse dynamic value, using default.",
                        ?path
                    );
                    default_value
                }),
        }
    }

    fn get_field_or_static(&self, value: &Option<String>) -> Option<String> {
        value.as_ref().and_then(|path| {
            if path.starts_with("$.") {
                self.get_value(path)
                    .map(|v| v.to_string_lossy().to_string())
            } else {
                Some(path.clone())
            }
        })
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
        tracing::warn!("Timestamp not found in event, using current time.");
        Utc::now()
    }

    fn get_payload(
        &mut self,
        config: &SyslogSerializerOptions,
        except_fields: &Vec<String>,
    ) -> String {
        for field in except_fields {
            let parsed_path = parse_target_path(field).unwrap();
            self.log.remove_prune(&parsed_path, false);
        }
        let value_option = if let Some(key) = &config.payload_key {
            self.get_value(key)
        } else {
            Some(self.log.value().clone())
        };
        value_option
            .map(|v| v.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    fn get_facility(&self, config: &SyslogSerializerOptions) -> Facility {
        self.get_syslog_code(&config.facility, Facility::from_repr, Facility::default())
    }

    fn get_severity(&self, config: &SyslogSerializerOptions) -> Severity {
        self.get_syslog_code(&config.severity, Severity::from_repr, Severity::default())
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
                .round_subsecs(0) // Round to the nearest second
                .to_rfc3339_opts(SecondsFormat::Secs, true),
        };
        parts.push(timestamp_str);
        parts.push(self.hostname.as_deref().unwrap_or(NIL_VALUE).to_string());

        let tag_str = match rfc {
            SyslogRFC::Rfc3164 => self.tag.encode_rfc_3164(),
            SyslogRFC::Rfc5424 => self.tag.encode_rfc_5424(),
        };
        parts.push(tag_str);

        let mut message_part = self.message.clone();
        if let Some(sd) = &self.structured_data {
            let sd_string = sd.encode();
            if !sd.elements.is_empty() {
                if *rfc == SyslogRFC::Rfc3164 {
                    if !self.message.is_empty() {
                        message_part = format!("{} {}", sd_string, self.message);
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
}

#[derive(Default, Debug)]
struct Tag {
    app_name: String,
    proc_id: Option<String>,
    msg_id: Option<String>,
}

// This regex pattern checks for "something[digits]".
// It's created lazily to be compiled only once.
static RFC3164_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\S+\[\d+]$").unwrap());

impl Tag {
    fn encode_rfc_3164(&self) -> String {
        if RFC3164_TAG_REGEX.is_match(&self.app_name) {
            // If it's already formatted
            format!("{}:", self.app_name)
        } else if let Some(proc_id) = self.proc_id.as_deref() {
            format!("{}[{}]:", self.app_name, proc_id)
        } else {
            format!("{}:", self.app_name)
        }
    }

    fn encode_rfc_5424(&self) -> String {
        let proc_id_str = if let Some(proc_id) = self.proc_id.as_deref() {
            proc_id
        } else {
            NIL_VALUE
        };

        let msg_id_str = if let Some(msg_id) = self.msg_id.as_deref() {
            msg_id
        } else {
            NIL_VALUE
        };

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
                        let _ = write!(acc, " {key}=\"{value}\"");
                    }
                    let _ = write!(acc, "]");
                    acc
                })
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
            payload_key = ".message"
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
            payload_key = "message"
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
            payload_key = "message"
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
            payload_key = "message"
        "#,
        )
        .unwrap();
        let log = create_test_log();
        let output = run_encode(config, Event::Log(log));
        let expected = "<26>1 2025-08-28T18:30:00Z test-host.com vector - - [metrics retries=\"3\"] original message";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_dynamic_facility_and_severity_default_payload() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
        "#,
        )
        .unwrap();
        let log = create_simple_log();
        let log_as_json_value = serde_json::to_value(&log).unwrap();
        let message_content = serde_json::to_string(&log_as_json_value).unwrap();
        let output = run_encode(config, Event::Log(log));
        let expected =
            format!("<14>1 2025-08-28T18:30:00Z test-host.com vector - - - {message_content}");
        assert_eq!(output, expected);
    }

    #[test]
    fn test_config_tag_alias_for_app_name() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        # Use the "tag" alias in the config
        tag = "my-legacy-app"
    "#,
        )
        .unwrap();

        // Assert that the value of "tag" was correctly assigned to the "app_name" field.
        assert_eq!(config.syslog.app_name, Some("my-legacy-app".to_string()));
    }

    #[test]
    fn test_rfc3164_tag_field_dynamic() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc3164"
            facility = "auth"
            severity = "informational"
            tag = "$.app_name"
            proc_id = "$.proc_id"
            payload_key = "message"
        "#,
        )
        .unwrap();
        let log = create_test_log();
        let output = run_encode(config, Event::Log(log));
        let expected = "<38>Aug 28 18:30:00 test-host.com my-app[12345]: [metrics retries=\"3\"] original message";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_rfc3164_tag_field_formatted() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc3164"
            facility = "auth"
            severity = "informational"
            tag = "my-app[12345]"
            proc_id = "$.proc_id"
            payload_key = "message"
        "#,
        )
        .unwrap();
        let log = create_test_log();
        let output = run_encode(config, Event::Log(log));
        let expected = "<38>Aug 28 18:30:00 test-host.com my-app[12345]: [metrics retries=\"3\"] original message";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_deserialization_logic() {
        #[derive(Deserialize, Debug, PartialEq, Eq)]
        struct TestConfig {
            #[serde(deserialize_with = "deserialize_facility")]
            facility: DynamicOrStatic<Facility>,
            #[serde(deserialize_with = "deserialize_severity")]
            severity: DynamicOrStatic<Severity>,
        }

        // 1. Test dynamic
        let config_str = r#"
        facility = "$.sys.fac"
        severity = "$.sys.sev"
    "#;
        let config: TestConfig = toml::from_str(config_str).unwrap();
        assert_eq!(
            config.facility,
            DynamicOrStatic::Dynamic("$.sys.fac".to_string())
        );
        assert_eq!(
            config.severity,
            DynamicOrStatic::Dynamic("$.sys.sev".to_string())
        );

        // 2. Test static
        let config_str = r#"
        facility = "local1"
        severity = "critical"
    "#;
        let config: TestConfig = toml::from_str(config_str).unwrap();
        assert_eq!(config.facility, DynamicOrStatic::Static(Facility::Local1));
        assert_eq!(config.severity, DynamicOrStatic::Static(Severity::Critical));

        // 3. Test valid numbers
        let config_str = r#"
        facility = "10" # authpriv
        severity = "4"  # warning
    "#;
        let config: TestConfig = toml::from_str(config_str).unwrap();
        assert_eq!(config.facility, DynamicOrStatic::Static(Facility::Authpriv));
        assert_eq!(config.severity, DynamicOrStatic::Static(Severity::Warning));

        // 4. Test invalid name
        let config_str = r#"
        facility = "invalid-name"
        severity = "warning"
    "#;
        assert!(toml::from_str::<TestConfig>(config_str).is_err());

        // 5. Test invalid number
        let config_str = r#"
        facility = "99"
        severity = "warning"
    "#;
        assert!(toml::from_str::<TestConfig>(config_str).is_err());
    }

    #[test]
    fn test_except_fields_removes_from_payload() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        except_fields = ["_internal"]
    "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(event_path!("_internal"), value!({"secret": "do not show"}));

        log.insert(event_path!("include"), value!({"public": "show me"}));

        let output = run_encode(config, Event::Log(log));

        // Assert that the final output does not contain the excluded field.
        assert!(!output.contains("_internal"));
        assert!(!output.contains("do not show"));

        assert!(output.contains("original message"));
        assert!(output.contains("include"));
        assert!(output.contains("show me"));
    }
}
