use bytes::{BufMut, BytesMut};
use chrono::{DateTime, SecondsFormat, SubsecRound, Utc};
use lookup::lookup_v2::ConfigTargetPath;
use serde_json;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Write;
use strum::FromRepr;
use tokio_util::codec::Encoder;
use tracing::debug;
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
#[serde(default, deny_unknown_fields)]
pub struct SyslogSerializerOptions {
    /// RFC to use for formatting.
    rfc: SyslogRFC,
    /// Path to a field in the event to use for the facility. Defaults to "user".
    facility: Option<ConfigTargetPath>,
    /// Path to a field in the event to use for the severity. Defaults to "informational".
    severity: Option<ConfigTargetPath>,
    /// Path to a field in the event to use for the app name.
    ///
    /// If not provided, the encoder checks for a semantic "service" field.
    /// If that is also missing, it defaults to "vector".
    app_name: Option<ConfigTargetPath>,
    /// Path to a field in the event to use for the proc ID.
    proc_id: Option<ConfigTargetPath>,
    /// Path to a field in the event to use for the msg ID.
    msg_id: Option<ConfigTargetPath>,
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
        let Event::Log(log_event) = event else {
            return Err("Syslog serializer only supports log events.".into());
        };

        let syslog_message = ConfigDecanter::new(&log_event).decant_config(&self.config.syslog);
        let encoded = syslog_message.encode(&self.config.syslog.rfc);
        buffer.put_slice(encoded.as_bytes());

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
        let mut app_name = self.get_value(&config.app_name).unwrap_or_else(|| {
            self.log
                .get_by_meaning("service")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_else(|| "vector".to_owned())
        });
        let mut proc_id = self.get_value(&config.proc_id);
        let mut msg_id = self.get_value(&config.msg_id);
        let hostname = self
            .log
            .get_host()
            .map(|v| v.to_string_lossy().into_owned());

        match config.rfc {
            SyslogRFC::Rfc3164 => {
                // RFC 3164: the TAG (app_name and proc_id) must be printable
                // US-ASCII without spaces, since a space terminates the TAG on
                // most receivers. RFC 3164 has no NILVALUE, so an app_name
                // that is empty after sanitization falls back to the default.
                app_name = sanitize_printusascii_field(app_name, RFC3164_TAG_MAX_LENGTH)
                    .unwrap_or_else(|| "vector".to_owned());
                proc_id = proc_id
                    .and_then(|pid| sanitize_printusascii_field(pid, RFC3164_TAG_MAX_LENGTH));
            }
            SyslogRFC::Rfc5424 => {
                app_name = sanitize_printusascii_field(app_name, APP_NAME_MAX_LENGTH)
                    .unwrap_or_else(|| NIL_VALUE.to_owned());
                proc_id =
                    proc_id.and_then(|pid| sanitize_printusascii_field(pid, PROC_ID_MAX_LENGTH));
                msg_id = msg_id.and_then(|mid| sanitize_printusascii_field(mid, MSG_ID_MAX_LENGTH));
            }
        }
        let hostname =
            hostname.and_then(|host| sanitize_printusascii_field(host, HOSTNAME_MAX_LENGTH));

        SyslogMessage {
            pri: Pri {
                facility: self.get_facility(config),
                severity: self.get_severity(config),
            },
            timestamp: self.get_timestamp(),
            hostname,
            tag: Tag {
                app_name,
                proc_id,
                msg_id,
            },
            structured_data: self.get_structured_data(),
            message: self.get_payload(),
        }
    }

    fn get_value(&self, path: &Option<ConfigTargetPath>) -> Option<String> {
        path.as_ref()
            .and_then(|p| self.log.get(p))
            .map(|v| v.to_string_lossy().into_owned())
    }

    fn get_structured_data(&self) -> Option<StructuredData> {
        self.log
            .get("structured_data")
            .and_then(|v| v.as_object().cloned())
            .map(StructuredData::from)
    }

    fn get_timestamp(&self) -> DateTime<Utc> {
        if let Some(Value::Timestamp(timestamp)) = self.log.get_timestamp() {
            return *timestamp;
        }
        Utc::now()
    }

    fn get_payload(&self) -> String {
        self.log
            .get_message()
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn get_facility(&self, config: &SyslogSerializerOptions) -> Facility {
        config.facility.as_ref().map_or(Facility::User, |path| {
            self.log
                .get(path)
                .and_then(parse_facility)
                .unwrap_or(Facility::User)
        })
    }

    fn get_severity(&self, config: &SyslogSerializerOptions) -> Severity {
        config
            .severity
            .as_ref()
            .map_or(Severity::Informational, |path| {
                self.log
                    .get(path)
                    .and_then(parse_severity)
                    .unwrap_or(Severity::Informational)
            })
    }
}

const NIL_VALUE: &str = "-";
const SYSLOG_V1: &str = "1";
const RFC3164_TAG_MAX_LENGTH: usize = 32;
const HOSTNAME_MAX_LENGTH: usize = 255;
const APP_NAME_MAX_LENGTH: usize = 48;
const PROC_ID_MAX_LENGTH: usize = 128;
const MSG_ID_MAX_LENGTH: usize = 32;
const SD_NAME_MAX_LENGTH: usize = 32;

/// Replaces invalid characters with `replacement`
#[inline]
fn sanitize_with<F>(s: &str, is_valid: F, replacement: char) -> Cow<'_, str>
where
    F: Fn(char) -> bool,
{
    match s.char_indices().find(|(_, c)| !is_valid(*c)) {
        None => Cow::Borrowed(s), // All valid, zero allocation
        Some((first_invalid_idx, _)) => {
            let mut result = String::with_capacity(s.len());
            result.push_str(&s[..first_invalid_idx]); // Copy valid prefix
            for c in s[first_invalid_idx..].chars() {
                result.push(if is_valid(c) { c } else { replacement });
            }

            Cow::Owned(result)
        }
    }
}

/// Sanitize a PRINTUSASCII header field, replacing spaces and control
/// characters with underscores and returning `None` for empty fields.
#[inline]
fn sanitize_printusascii_field(s: String, max_chars: usize) -> Option<String> {
    let mut sanitized = match sanitize_with(&s, |c| c.is_ascii_graphic(), '_') {
        Cow::Borrowed(_) => s, // Already clean, reuse the caller's allocation
        Cow::Owned(owned) => owned,
    };
    truncate_chars(&mut sanitized, max_chars);

    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

/// Sanitize SD-ID or PARAM-NAME according to RFC 5424
/// Per RFC 5424, these NAMES must only contain printable ASCII (33-126)
/// excluding '=', ' ', ']', '"'
/// Invalid characters are replaced with '_'
#[inline]
fn sanitize_name(name: &str) -> Cow<'_, str> {
    sanitize_with(
        name,
        |c| c.is_ascii_graphic() && !matches!(c, '=' | ']' | '"'),
        '_',
    )
}

/// Escape PARAM-VALUE according to RFC 5424
fn escape_sd_value(s: &str) -> Cow<'_, str> {
    let needs_escaping = s.chars().any(|c| matches!(c, '\\' | '"' | ']'));

    if !needs_escaping {
        return Cow::Borrowed(s);
    }

    let mut result = String::with_capacity(s.len() + 10);
    for ch in s.chars() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            ']' => result.push_str("\\]"),
            _ => result.push(ch),
        }
    }

    Cow::Owned(result)
}

/// Safely truncate a string to a maximum number of characters (not bytes!)
/// This avoids panics when truncating at a multi-byte UTF-8 character boundary
/// Optimized to iterate only through necessary characters (not the entire string)
fn truncate_chars(s: &mut String, max_chars: usize) {
    if let Some((byte_idx, _)) = s.char_indices().nth(max_chars) {
        s.truncate(byte_idx);
    }
}

fn truncate_to_string(s: Cow<'_, str>, max_chars: usize) -> String {
    let mut s = s.into_owned();
    truncate_chars(&mut s, max_chars);
    s
}

fn sanitize_sd_name(name: &str) -> Option<String> {
    let sanitized = truncate_to_string(sanitize_name(name), SD_NAME_MAX_LENGTH);

    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

/// Joins an already-sanitized leaf name onto a (possibly empty) dotted prefix,
/// keeping the result within `SD_NAME_MAX_LENGTH` characters. The prefix is
/// truncated rather than the leaf so that sibling keys under a long path stay
/// distinguishable.
fn join_sd_name(prefix: &str, leaf: String) -> String {
    if prefix.is_empty() {
        return leaf;
    }

    let prefix_budget = SD_NAME_MAX_LENGTH.saturating_sub(leaf.chars().count() + 1);
    if prefix_budget == 0 {
        return leaf;
    }

    let mut joined = prefix.to_owned();
    truncate_chars(&mut joined, prefix_budget);
    joined.push('.');
    joined.push_str(&leaf);
    joined
}

fn deduped_key<V>(map: &BTreeMap<String, V>, base: String) -> String {
    if !map.contains_key(&base) {
        return base;
    }

    for index in 1usize.. {
        let suffix = format!("_{index}");
        let mut candidate = base.clone();
        truncate_chars(
            &mut candidate,
            SD_NAME_MAX_LENGTH.saturating_sub(suffix.len()),
        );
        candidate.push_str(&suffix);

        if !map.contains_key(&candidate) {
            return candidate;
        }
    }

    unreachable!("usize iteration should produce a unique structured-data name")
}

fn insert_structured_param(result: &mut BTreeMap<String, String>, key: String, value: String) {
    let key = deduped_key(result, key);
    result.insert(key, value);
}

fn parse_numeric_code<T>(value: &Value, from_repr: fn(usize) -> Option<T>) -> Option<T> {
    match value {
        Value::Integer(n) => usize::try_from(*n).ok().and_then(from_repr),
        other => other
            .to_string_lossy()
            .trim()
            .parse::<usize>()
            .ok()
            .and_then(from_repr),
    }
}

fn parse_facility(value: &Value) -> Option<Facility> {
    if let Some(facility) = parse_numeric_code(value, Facility::from_repr) {
        return Some(facility);
    }

    match value.to_string_lossy().trim().to_ascii_lowercase().as_str() {
        "kern" | "kernel" => Some(Facility::Kern),
        "user" => Some(Facility::User),
        "mail" => Some(Facility::Mail),
        "daemon" => Some(Facility::Daemon),
        "auth" => Some(Facility::Auth),
        "syslog" => Some(Facility::Syslog),
        "lpr" => Some(Facility::Lpr),
        "news" => Some(Facility::News),
        "uucp" => Some(Facility::Uucp),
        "cron" => Some(Facility::Cron),
        "authpriv" | "auth-priv" | "auth_priv" => Some(Facility::Authpriv),
        "ftp" => Some(Facility::Ftp),
        "ntp" => Some(Facility::Ntp),
        "security" | "audit" => Some(Facility::Security),
        "log-alert" | "log_alert" | "alert" | "console" => Some(Facility::LogAlert),
        "solaris-cron" | "solaris_cron" | "clockd" => Some(Facility::SolarisCron),
        "local0" => Some(Facility::Local0),
        "local1" => Some(Facility::Local1),
        "local2" => Some(Facility::Local2),
        "local3" => Some(Facility::Local3),
        "local4" => Some(Facility::Local4),
        "local5" => Some(Facility::Local5),
        "local6" => Some(Facility::Local6),
        "local7" => Some(Facility::Local7),
        _ => None,
    }
}

fn parse_severity(value: &Value) -> Option<Severity> {
    if let Some(severity) = parse_numeric_code(value, Severity::from_repr) {
        return Some(severity);
    }

    match value.to_string_lossy().trim().to_ascii_lowercase().as_str() {
        "emergency" | "emerg" => Some(Severity::Emergency),
        "alert" => Some(Severity::Alert),
        "critical" | "crit" => Some(Severity::Critical),
        "error" | "err" => Some(Severity::Error),
        "warning" | "warn" => Some(Severity::Warning),
        "notice" => Some(Severity::Notice),
        "informational" | "info" => Some(Severity::Informational),
        "debug" => Some(Severity::Debug),
        _ => None,
    }
}

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
        let mut result = String::with_capacity(256);

        let _ = write!(result, "<{}>", self.pri.prival());

        if *rfc == SyslogRFC::Rfc5424 {
            result.push_str(SYSLOG_V1);
            result.push(' ');
        }

        match rfc {
            SyslogRFC::Rfc3164 => {
                let _ = write!(result, "{} ", self.timestamp.format("%b %e %H:%M:%S"));
            }
            SyslogRFC::Rfc5424 => {
                result.push_str(
                    &self
                        .timestamp
                        .round_subsecs(6)
                        .to_rfc3339_opts(SecondsFormat::Micros, true),
                );
                result.push(' ');
            }
        }

        result.push_str(self.hostname.as_deref().unwrap_or(NIL_VALUE));
        result.push(' ');

        match rfc {
            SyslogRFC::Rfc3164 => {
                result.push_str(&self.tag.encode_rfc_3164());

                // RFC 3164 does not support structured data
                if self
                    .structured_data
                    .as_ref()
                    .is_some_and(|sd| !sd.elements.is_empty())
                {
                    debug!(
                        "Structured data present but ignored - RFC 3164 does not support structured data. Consider using RFC 5424 instead."
                    );
                }

                if !self.message.is_empty() {
                    result.push(' ');
                    result.push_str(&Self::sanitize_rfc3164_message(&self.message));
                }
            }
            SyslogRFC::Rfc5424 => {
                result.push_str(&self.tag.encode_rfc_5424());
                result.push(' ');

                match &self.structured_data {
                    Some(sd) => result.push_str(&sd.encode()),
                    None => result.push_str(NIL_VALUE),
                }

                if !self.message.is_empty() {
                    result.push(' ');
                    result.push_str(&self.message);
                }
            }
        }

        result
    }

    fn sanitize_rfc3164_message(message: &str) -> Cow<'_, str> {
        sanitize_with(message, |ch| (' '..='~').contains(&ch), ' ')
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
        // The TAG is limited to `RFC3164_TAG_MAX_LENGTH` characters, including
        // the terminating ':'. When the full tag does not fit, truncate the
        // app_name and keep "[proc_id]" intact; if even that is impossible,
        // drop the proc_id entirely rather than emit a corrupted, unbalanced
        // "app[123:" tag.
        if let Some(proc_id) = self.proc_id.as_deref() {
            let tag = format!("{}[{}]:", self.app_name, proc_id);
            if tag.chars().count() <= RFC3164_TAG_MAX_LENGTH {
                return tag;
            }

            let bracket_chars = proc_id.chars().count() + "[]:".len();
            if bracket_chars < RFC3164_TAG_MAX_LENGTH {
                let mut app_name = self.app_name.clone();
                truncate_chars(&mut app_name, RFC3164_TAG_MAX_LENGTH - bracket_chars);
                return format!("{app_name}[{proc_id}]:");
            }
        }

        let mut app_name = self.app_name.clone();
        truncate_chars(&mut app_name, RFC3164_TAG_MAX_LENGTH - 1);
        format!("{app_name}:")
    }

    fn encode_rfc_5424(&self) -> String {
        let proc_id_str = self.proc_id.as_deref().unwrap_or(NIL_VALUE);
        let msg_id_str = self.msg_id.as_deref().unwrap_or(NIL_VALUE);
        format!("{} {} {}", self.app_name, proc_id_str, msg_id_str)
    }
}

type StructuredDataMap = BTreeMap<String, BTreeMap<String, String>>;
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
                        let esc_val = escape_sd_value(value);
                        let _ = write!(acc, " {key}=\"{esc_val}\"");
                    }
                    let _ = write!(acc, "]");
                    acc
                })
        }
    }
}

impl From<ObjectMap> for StructuredData {
    fn from(fields: ObjectMap) -> Self {
        let mut elements = BTreeMap::new();

        for (sd_id, value) in fields {
            let sd_id_str: String = sd_id.into();
            let Some(final_id) = sanitize_sd_name(&sd_id_str) else {
                debug!("Dropping structured-data element whose SD-ID is empty after sanitization.");
                continue;
            };
            let final_id = deduped_key(&elements, final_id);

            let sd_params = match value {
                Value::Object(obj) => {
                    let mut map = BTreeMap::new();
                    flatten_object(obj, String::new(), &mut map);
                    map
                }
                scalar => {
                    let mut map = BTreeMap::new();
                    map.insert("value".to_string(), scalar.to_string_lossy().to_string());
                    map
                }
            };
            elements.insert(final_id, sd_params);
        }

        Self { elements }
    }
}

/// Helper function to flatten nested objects with dot notation
fn flatten_object(obj: ObjectMap, prefix: String, result: &mut BTreeMap<String, String>) {
    for (key, value) in obj {
        let key_str: String = key.into();
        let Some(sanitized_key) = sanitize_sd_name(&key_str) else {
            debug!("Dropping structured-data param whose name is empty after sanitization.");
            continue;
        };

        let full_key = join_sd_name(&prefix, sanitized_key);

        match value {
            Value::Object(nested) => {
                flatten_object(nested, full_key, result);
            }
            Value::Array(arr) => {
                if let Ok(json) = serde_json::to_string(&arr) {
                    insert_structured_param(result, full_key, json);
                } else {
                    insert_structured_param(result, full_key, format!("{:?}", arr));
                }
            }
            scalar => {
                insert_structured_param(result, full_key, scalar.to_string_lossy().to_string());
            }
        }
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
    fn prival(&self) -> u8 {
        (self.facility as u8 * 8) + self.severity as u8
    }
}

/// Syslog facility
///
/// Facility and severity names (including aliases) are resolved at runtime by
/// `parse_facility`/`parse_severity`, which are the single source of truth for
/// recognized spellings.
#[derive(Default, Debug, FromRepr, Copy, Clone, PartialEq, Eq)]
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
    /// Log alert
    LogAlert = 14,
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
#[derive(Default, Debug, FromRepr, Copy, Clone, PartialEq, Eq)]
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
    use std::sync::Arc;
    use vector_core::config::LogNamespace;
    use vector_core::event::Event::Metric;
    use vector_core::event::{Event, MetricKind, MetricValue, StatisticKind};
    use vrl::path::parse_target_path;
    use vrl::prelude::Kind;
    use vrl::{btreemap, event_path, value};

    fn run_encode(config: SyslogSerializerConfig, event: Event) -> String {
        let mut serializer = SyslogSerializer::new(&config);
        let mut buffer = BytesMut::new();
        serializer.encode(event, &mut buffer).unwrap();
        String::from_utf8(buffer.to_vec()).unwrap()
    }

    fn create_simple_log() -> LogEvent {
        create_simple_log_with_message("original message")
    }

    // Sets the message at construction time instead of overwriting the
    // literal "message" field afterwards: the decoder tests in this crate
    // mutate the process-global log schema's message key, so path-based
    // message overrides race with them under parallel test execution.
    fn create_simple_log_with_message(message: &str) -> LogEvent {
        let mut log = LogEvent::from(message);
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
        "#,
        )
        .unwrap();
        let log = create_test_log();
        let output = run_encode(config, Event::Log(log));
        // RFC 3164 does not support structured data, so it's ignored
        let expected = "<26>Aug 28 18:30:00 test-host.com my-app[12345]: original message";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_parsing_logic() {
        let mut log = LogEvent::from("test message");
        let config_fac =
            toml::from_str::<SyslogSerializerOptions>(r#"facility = ".syslog_facility""#).unwrap();
        let config_sev =
            toml::from_str::<SyslogSerializerOptions>(r#"severity = ".syslog_severity""#).unwrap();

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

        log.insert(event_path!("syslog_facility"), Value::from(3u8));
        log.insert(event_path!("syslog_severity"), Value::from(2u8));
        let decanter = ConfigDecanter::new(&log);
        let facility = decanter.get_facility(&config_fac);
        let severity = decanter.get_severity(&config_sev);
        assert_eq!(facility, Facility::Daemon);
        assert_eq!(severity, Severity::Critical);

        log.insert(event_path!("syslog_facility"), "3");
        log.insert(event_path!("syslog_severity"), "2");
        let decanter = ConfigDecanter::new(&log);
        let facility = decanter.get_facility(&config_fac);
        let severity = decanter.get_severity(&config_sev);
        assert_eq!(facility, Facility::Daemon);
        assert_eq!(severity, Severity::Critical);

        for (name, expected) in [
            ("kern", Facility::Kern),
            ("kernel", Facility::Kern),
            ("audit", Facility::Security),
            ("alert", Facility::LogAlert),
            ("log-alert", Facility::LogAlert),
            ("clockd", Facility::SolarisCron),
            ("local7", Facility::Local7),
        ] {
            log.insert(event_path!("syslog_facility"), name);
            let decanter = ConfigDecanter::new(&log);
            assert_eq!(decanter.get_facility(&config_fac), expected);
        }

        for (name, expected) in [
            ("emerg", Severity::Emergency),
            ("emergency", Severity::Emergency),
            ("crit", Severity::Critical),
            ("critical", Severity::Critical),
            ("err", Severity::Error),
            ("error", Severity::Error),
            ("warn", Severity::Warning),
            ("warning", Severity::Warning),
            ("info", Severity::Informational),
            ("informational", Severity::Informational),
        ] {
            log.insert(event_path!("syslog_severity"), name);
            let decanter = ConfigDecanter::new(&log);
            assert_eq!(decanter.get_severity(&config_sev), expected);
        }

        let empty_config =
            toml::from_str::<SyslogSerializerOptions>(r#"facility = ".missing_field""#).unwrap();
        let decanter = ConfigDecanter::new(&log);
        let default_facility = decanter.get_facility(&empty_config);
        let default_severity = decanter.get_severity(&empty_config);
        assert_eq!(default_facility, Facility::User);
        assert_eq!(default_severity, Severity::Informational);
    }

    #[test]
    fn test_pri_aliases_from_syslog_decoder_names() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc5424"
        facility = ".facility"
        severity = ".severity"
    "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(event_path!("facility"), "audit");
        log.insert(event_path!("severity"), "info");
        let output = run_encode(config.clone(), Event::Log(log));
        assert!(output.starts_with("<110>1"), "unexpected PRI: {output}");

        let mut log = create_simple_log();
        log.insert(event_path!("facility"), "clockd");
        log.insert(event_path!("severity"), "err");
        let output = run_encode(config.clone(), Event::Log(log));
        assert!(output.starts_with("<123>1"), "unexpected PRI: {output}");

        let mut log = create_simple_log();
        log.insert(event_path!("facility"), "3");
        log.insert(event_path!("severity"), "2");
        let output = run_encode(config, Event::Log(log));
        assert!(output.starts_with("<26>1"), "unexpected PRI: {output}");
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

        //newline, tab, unicode
        let log = create_simple_log_with_message("A\nB\tC, Привіт D, E\u{0007}F");

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
    fn test_rfc5424_header_field_sanitization() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc5424"
        app_name = ".app"
        proc_id = ".proc"
        msg_id = ".msg"
    "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(event_path!("host"), "bad host\n😀");
        log.insert(event_path!("app"), "my app\n");
        log.insert(event_path!("proc"), "proc\tid");
        log.insert(event_path!("msg"), "msg id\u{0007}");

        let output = run_encode(config, Event::Log(log));
        let expected = "<14>1 2025-08-28T18:30:00.123456Z bad_host__ my_app_ proc_id msg_id_ - original message";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_rfc5424_empty_header_fields_use_nilvalue() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc5424"
        app_name = ".app"
        proc_id = ".proc"
        msg_id = ".msg"
    "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(event_path!("host"), "");
        log.insert(event_path!("app"), "");
        log.insert(event_path!("proc"), "");
        log.insert(event_path!("msg"), "");

        let output = run_encode(config, Event::Log(log));
        let expected = "<14>1 2025-08-28T18:30:00.123456Z - - - - - original message";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_rfc3164_hostname_sanitization() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc3164"
    "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(event_path!("host"), "bad host\n😀");

        let output = run_encode(config, Event::Log(log));
        let expected = "<14>Aug 28 18:30:00 bad_host__ vector: original message";
        assert_eq!(output, expected);
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
            "this-is-a-very-very-long-application-name",
        );
        log.insert(event_path!("proc_id"), "1234567890");

        let output = run_encode(config.clone(), Event::Log(log));
        // The app_name is truncated so that "[proc_id]:" stays intact and the
        // whole tag fits in 32 characters.
        let expected_tag = " this-is-a-very-very[1234567890]: ";
        assert!(output.contains(expected_tag), "got: {output}");

        // Without a proc_id the app_name itself is truncated to fit.
        let mut log = create_simple_log();
        log.insert(
            event_path!("app_name"),
            "this-is-a-very-very-long-application-name",
        );
        let output = run_encode(config, Event::Log(log));
        let expected_tag = " this-is-a-very-very-long-applic: ";
        assert!(output.contains(expected_tag), "got: {output}");
    }

    #[test]
    fn test_rfc3164_tag_oversized_proc_id_dropped() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc3164"
        app_name = ".app_name"
        proc_id = ".proc_id"
    "#,
        )
        .unwrap();

        // "[proc_id]:" alone cannot fit in the 32-char tag, so the proc_id is
        // dropped entirely instead of being truncated into a wrong value.
        let mut log = create_simple_log();
        log.insert(event_path!("app_name"), "myapp");
        log.insert(event_path!("proc_id"), "a".repeat(30));

        let output = run_encode(config.clone(), Event::Log(log));
        assert!(output.contains(" myapp: "), "got: {output}");
        assert!(!output.contains('['), "got: {output}");

        // An empty proc_id is treated as absent rather than emitting "[]".
        let mut log = create_simple_log();
        log.insert(event_path!("app_name"), "myapp");
        log.insert(event_path!("proc_id"), "");

        let output = run_encode(config, Event::Log(log));
        assert!(output.contains(" myapp: "), "got: {output}");
        assert!(!output.contains('['), "got: {output}");
    }

    #[test]
    fn test_rfc3164_tag_space_sanitization() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc3164"
        app_name = ".app"
    "#,
        )
        .unwrap();

        // A space would terminate the TAG early on most receivers.
        let mut log = create_simple_log();
        log.insert(event_path!("app"), "my app");

        let output = run_encode(config, Event::Log(log));
        assert!(output.contains(" my_app: "), "got: {output}");
    }

    #[test]
    fn test_rfc3164_empty_app_name_falls_back_to_default() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc3164"
        app_name = ".app"
    "#,
        )
        .unwrap();

        // RFC 3164 has no NILVALUE; an empty app_name must not produce a bare
        // ":" tag.
        let mut log = create_simple_log();
        log.insert(event_path!("app"), "");

        let output = run_encode(config, Event::Log(log));
        assert!(output.contains(" vector: "), "got: {output}");
    }

    #[test]
    fn test_rfc3164_empty_message_no_trailing_space() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc3164"
    "#,
        )
        .unwrap();

        let log = create_simple_log_with_message("");

        let output = run_encode(config, Event::Log(log));
        let expected = "<14>Aug 28 18:30:00 test-host.com vector:";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_rfc5424_missing_fields() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc5424"
        app_name = ".app"  # configured path, but not in log
        proc_id = ".pid"   # configured path, but not in log
        msg_id = ".mid"    # configured path, but not in log
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
    fn test_invalid_parsing_fallback() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc5424"
        facility = ".fac"
        severity = ".sev"
    "#,
        )
        .unwrap();

        let mut log = create_simple_log();

        log.insert(event_path!("fac"), "");
        log.insert(event_path!("sev"), "invalid_severity_name");

        let output = run_encode(config, Event::Log(log));

        let expected_pri = "<14>";
        assert!(output.starts_with(expected_pri));

        let expected_suffix = "vector - - - original message";
        assert!(output.ends_with(expected_suffix));
    }

    #[test]
    fn test_rfc5424_empty_message_and_sd() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc5424"
        app_name = ".app"
        proc_id = ".pid"
        msg_id = ".mid"
    "#,
        )
        .unwrap();

        let mut log = create_simple_log_with_message("");
        log.insert(event_path!("structured_data"), value!({}));

        let output = run_encode(config, Event::Log(log));
        let expected = "<14>1 2025-08-28T18:30:00.123456Z test-host.com vector - - -";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_non_log_event_rejected() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc5424"
    "#,
        )
        .unwrap();

        let metric_event = Metric(vector_core::event::Metric::new(
            "metric1",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vector_core::samples![10.0 => 1],
                statistic: StatisticKind::Histogram,
            },
        ));

        let mut serializer = SyslogSerializer::new(&config);
        let mut buffer = BytesMut::new();

        let result = serializer.encode(metric_event, &mut buffer);

        assert!(result.is_err());
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_minimal_event() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
    "#,
        )
        .unwrap();
        let log = LogEvent::from("");

        let output = run_encode(config, Event::Log(log));
        let expected_suffix = "vector - - -";
        assert!(output.starts_with("<14>1"));
        assert!(output.ends_with(expected_suffix));
    }

    #[test]
    fn test_app_name_meaning_fallback() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
        [syslog]
        rfc = "rfc5424"
        severity = ".sev"
        app_name = ".nonexistent"
    "#,
        )
        .unwrap();

        let mut log = LogEvent::default();
        log.insert("syslog.service", "meaning-app");

        let schema = schema::Definition::new_with_default_metadata(
            Kind::object(btreemap! {
                "syslog" => Kind::object(btreemap! {
                    "service" => Kind::bytes(),
                })
            }),
            [LogNamespace::Vector],
        );
        let schema = schema.with_meaning(parse_target_path("syslog.service").unwrap(), "service");
        let mut event = Event::from(log);
        event
            .metadata_mut()
            .set_schema_definition(&Arc::new(schema));

        let output = run_encode(config, event);
        assert!(output.contains("meaning-app - -"));
    }

    #[test]
    fn test_structured_data_with_scalars() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc5424"
        "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(
            event_path!("structured_data"),
            value!({"simple_string": "hello", "simple_number": 42}),
        );

        let output = run_encode(config, Event::Log(log));
        assert!(output.contains(r#"[simple_number value="42"]"#));
        assert!(output.contains(r#"[simple_string value="hello"]"#));
    }

    #[test]
    fn test_structured_data_with_nested_objects() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc5424"
        "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(
            event_path!("structured_data"),
            value!({
                "meta": {
                    "request": {
                        "id": "abc-123",
                        "method": "GET"
                    },
                    "user": "bob"
                }
            }),
        );

        let output = run_encode(config, Event::Log(log));
        assert!(output.contains(r#"[meta request.id="abc-123" request.method="GET" user="bob"]"#));
    }

    #[test]
    fn test_structured_data_with_arrays() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc5424"
        "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(
            event_path!("structured_data"),
            value!({
                "data": {
                    "tags": ["tag1", "tag2", "tag3"]
                }
            }),
        );

        let output = run_encode(config, Event::Log(log));
        // Arrays should be JSON-encoded and escaped
        assert!(output.contains(r#"[data tags="[\"tag1\",\"tag2\",\"tag3\"\]"]"#));
    }

    #[test]
    fn test_structured_data_complex_nested() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc5424"
        "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(
            event_path!("structured_data"),
            value!({
                "tracking": {
                    "session": {
                        "user": {
                            "id": "123",
                            "name": "alice"
                        },
                        "duration_ms": 5000
                    }
                }
            }),
        );

        let output = run_encode(config, Event::Log(log));
        assert!(output.contains(r#"session.duration_ms="5000""#));
        assert!(output.contains(r#"session.user.id="123""#));
        assert!(output.contains(r#"session.user.name="alice""#));
    }

    #[test]
    fn test_structured_data_sanitization() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc5424"
        "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(
            event_path!("structured_data"),
            value!({
                "my id": {  // SD-ID with space - should be sanitized to my_id
                    "user=name": "alice",  // PARAM-NAME with = - should be sanitized to user_name
                    "foo]bar": "value1",   // PARAM-NAME with ] - should be sanitized to foo_bar
                    "has\"quote": "value2" // PARAM-NAME with " - should be sanitized to has_quote
                }
            }),
        );

        let output = run_encode(config, Event::Log(log));
        // All invalid characters should be replaced with _
        assert!(output.contains(r#"[my_id"#));
        assert!(output.contains(r#"foo_bar="value1""#));
        assert!(output.contains(r#"has_quote="value2""#));
        assert!(output.contains(r#"user_name="alice""#));
    }

    #[test]
    fn test_structured_data_sd_id_length_limit() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc5424"
        "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(
            event_path!("structured_data"),
            value!({
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": {
                    "key": "value"
                }
            }),
        );

        let output = run_encode(config, Event::Log(log));
        let expected_id = "a".repeat(32);
        assert!(output.contains(&format!("[{}", expected_id)));
        assert!(!output.contains(&format!("[{}", "a".repeat(50))));
    }

    #[test]
    fn test_structured_data_param_name_length_limit_and_collision() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc5424"
        "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(
            event_path!("structured_data"),
            value!({
                "metrics": {
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": "first",
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab": "second",
                }
            }),
        );

        let output = run_encode(config, Event::Log(log));
        let truncated_key = "a".repeat(32);
        let deduped_key = format!("{}_1", "a".repeat(30));
        assert!(output.contains(&format!(r#"{truncated_key}="first""#)));
        assert!(output.contains(&format!(r#"{deduped_key}="second""#)));
        assert!(!output.contains(&"a".repeat(50)));
    }

    #[test]
    fn test_structured_data_long_prefix_preserves_leaf_names() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc5424"
        "#,
        )
        .unwrap();

        // A nested prefix at the 32-char PARAM-NAME limit: the prefix is
        // truncated instead of the leaf, so sibling keys stay distinguishable.
        let long_prefix = "a".repeat(32);
        let leaf: ObjectMap = [("x".into(), Value::from(1)), ("y".into(), Value::from(2))]
            .into_iter()
            .collect();
        let nested: ObjectMap = [(long_prefix.into(), Value::Object(leaf))]
            .into_iter()
            .collect();
        let sd: ObjectMap = [("el".into(), Value::Object(nested))].into_iter().collect();

        let mut log = create_simple_log();
        log.insert(event_path!("structured_data"), Value::Object(sd));

        let output = run_encode(config, Event::Log(log));
        let truncated_prefix = "a".repeat(30);
        assert!(
            output.contains(&format!(r#"{truncated_prefix}.x="1""#)),
            "got: {output}"
        );
        assert!(
            output.contains(&format!(r#"{truncated_prefix}.y="2""#)),
            "got: {output}"
        );
    }

    #[test]
    fn test_structured_data_prefix_truncation_collision_deduped() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc5424"
        "#,
        )
        .unwrap();

        // Two distinct prefixes that only differ past the truncation point
        // collide after joining; the dedup suffix must keep both params.
        let prefix_a = "a".repeat(32);
        let prefix_b = format!("{}b", "a".repeat(31));
        let leaf_a: ObjectMap = [("v".into(), Value::from(1))].into_iter().collect();
        let leaf_b: ObjectMap = [("v".into(), Value::from(2))].into_iter().collect();
        let nested: ObjectMap = [
            (prefix_a.into(), Value::Object(leaf_a)),
            (prefix_b.into(), Value::Object(leaf_b)),
        ]
        .into_iter()
        .collect();
        let sd: ObjectMap = [("el".into(), Value::Object(nested))].into_iter().collect();

        let mut log = create_simple_log();
        log.insert(event_path!("structured_data"), Value::Object(sd));

        let output = run_encode(config, Event::Log(log));
        let truncated = "a".repeat(30);
        assert!(
            output.contains(&format!(r#"{truncated}.v="1""#)),
            "got: {output}"
        );
        assert!(
            output.contains(&format!(r#"{truncated}_1="2""#)),
            "got: {output}"
        );
    }

    #[test]
    fn test_structured_data_empty_names_are_skipped() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc5424"
        "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        log.insert(
            event_path!("structured_data"),
            value!({
                "": {
                    "ignored": "value"
                },
                "valid": {
                    "": "also ignored",
                    "kept": "yes"
                }
            }),
        );

        let output = run_encode(config, Event::Log(log));
        assert!(output.contains(r#"[valid kept="yes"]"#));
        assert!(!output.contains("ignored"));
        assert!(!output.contains("also ignored"));
    }

    #[test]
    fn test_utf8_safe_truncation() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc5424"
            app_name = ".app"
            proc_id = ".proc"
            msg_id = ".msg"
        "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        // Create fields with UTF-8 characters (emoji, Cyrillic, etc.) each emoji is 4 bytes
        log.insert(
            event_path!("app"),
            "app_😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀",
        );
        log.insert(
            event_path!("proc"),
            "процес_😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀",
        );
        log.insert(event_path!("msg"), "довге_повідомлення ");

        log.insert(
            event_path!("structured_data"),
            value!({
                "_😀_дуже_довге_значення_більше_тридцати_двух_символів": {
                    "_😀_": "value"
                }
            }),
        );
        let output = run_encode(config, Event::Log(log));
        assert!(output.starts_with("<14>1"));
        assert!(output.contains("app_"));

        let expected_sd_id: String = "_".repeat(32);
        assert!(output.contains(&format!("[{}", expected_sd_id)));
    }

    #[test]
    fn test_rfc3164_ascii_sanitization() {
        let config = toml::from_str::<SyslogSerializerConfig>(
            r#"
            [syslog]
            rfc = "rfc3164"
            app_name = ".app"
            proc_id = ".proc"
        "#,
        )
        .unwrap();

        let mut log = create_simple_log();
        // Use non-ASCII characters in app_name and proc_id
        log.insert(event_path!("app"), "my_app_😀_тест");
        log.insert(event_path!("proc"), "процес_123");

        let output = run_encode(config, Event::Log(log));

        assert!(output.starts_with("<14>"));
        assert!(output.contains("my_app_____"));
        assert!(output.contains("[_______123]:"));

        assert!(!output.contains("😀"));
        assert!(!output.contains("тест"));
        assert!(!output.contains("процес"));
    }
}
