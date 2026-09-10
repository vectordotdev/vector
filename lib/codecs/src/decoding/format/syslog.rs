use std::borrow::Cow;

use bytes::Bytes;
use chrono::{DateTime, Datelike, Utc};
use derivative::Derivative;
use lookup::{OwnedTargetPath, OwnedValuePath, event_path, owned_value_path};
use smallvec::{SmallVec, smallvec};
use syslog_loose::{IncompleteDate, Message, ProcId, Protocol, Variant, decompose_pri};
use vector_config::configurable_component;
use vector_core::{
    config::{DataType, LegacyKey, LogNamespace, log_schema},
    event::{Event, LogEvent, ObjectMap, Value},
    schema,
};
use vrl::value::{Kind, kind::Collection};

use super::{Deserializer, default_lossy};

/// Config used to build a `SyslogDeserializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct SyslogDeserializerConfig {
    #[serde(skip)]
    source: Option<&'static str>,

    /// Syslog-specific decoding options.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub syslog: SyslogDeserializerOptions,
}

impl SyslogDeserializerConfig {
    /// Creates a new `SyslogDeserializerConfig`.
    pub fn new(options: SyslogDeserializerOptions) -> Self {
        Self {
            source: None,
            syslog: options,
        }
    }

    /// Create the `SyslogDeserializer` from the given source name.
    pub fn from_source(source: &'static str) -> Self {
        Self {
            source: Some(source),
            ..Default::default()
        }
    }

    /// Build the `SyslogDeserializer` from this configuration.
    pub const fn build(&self) -> SyslogDeserializer {
        SyslogDeserializer {
            source: self.source,
            lossy: self.syslog.lossy,
        }
    }

    /// Return the type of event build by this deserializer.
    pub fn output_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match (log_namespace, self.source) {
            (LogNamespace::Legacy, _) => {
                let mut definition = schema::Definition::empty_legacy_namespace()
                    // The `message` field is always defined. If parsing fails, the entire body becomes the
                    // message.
                    .with_event_field(
                        log_schema().message_key().expect("valid message key"),
                        Kind::bytes(),
                        Some("message"),
                    );

                if let Some(timestamp_key) = log_schema().timestamp_key() {
                    // All other fields are optional.
                    definition = definition.optional_field(
                        timestamp_key,
                        Kind::timestamp(),
                        Some("timestamp"),
                    )
                }

                definition = definition
                    .optional_field(&owned_value_path!("hostname"), Kind::bytes(), Some("host"))
                    .optional_field(
                        &owned_value_path!("severity"),
                        Kind::bytes(),
                        Some("severity"),
                    )
                    .optional_field(&owned_value_path!("facility"), Kind::bytes(), None)
                    .optional_field(&owned_value_path!("version"), Kind::integer(), None)
                    .optional_field(
                        &owned_value_path!("appname"),
                        Kind::bytes(),
                        Some("service"),
                    )
                    .optional_field(&owned_value_path!("msgid"), Kind::bytes(), None)
                    .optional_field(
                        &owned_value_path!("procid"),
                        Kind::integer().or_bytes(),
                        None,
                    )
                    // "structured data" is placed at the root. It will always be a map of strings
                    .unknown_fields(Kind::object(Collection::from_unknown(Kind::bytes())));

                if self.source.is_some() {
                    // This field is added by the syslog source. It will not be present if the data
                    // is coming from the codec.
                    definition.optional_field(&owned_value_path!("source_ip"), Kind::bytes(), None)
                } else {
                    definition
                }
            }
            (LogNamespace::Vector, None) => {
                schema::Definition::new_with_default_metadata(
                    Kind::object(Collection::empty()),
                    [log_namespace],
                )
                .with_event_field(
                    &owned_value_path!("message"),
                    Kind::bytes(),
                    Some("message"),
                )
                .optional_field(
                    &owned_value_path!("timestamp"),
                    Kind::timestamp(),
                    Some("timestamp"),
                )
                .optional_field(&owned_value_path!("hostname"), Kind::bytes(), Some("host"))
                .optional_field(
                    &owned_value_path!("severity"),
                    Kind::bytes(),
                    Some("severity"),
                )
                .optional_field(&owned_value_path!("facility"), Kind::bytes(), None)
                .optional_field(&owned_value_path!("version"), Kind::integer(), None)
                .optional_field(
                    &owned_value_path!("appname"),
                    Kind::bytes(),
                    Some("service"),
                )
                .optional_field(&owned_value_path!("msgid"), Kind::bytes(), None)
                .optional_field(
                    &owned_value_path!("procid"),
                    Kind::integer().or_bytes(),
                    None,
                )
                // "structured data" is placed at the root. It will always be a map strings
                .unknown_fields(Kind::object(Collection::from_unknown(Kind::bytes())))
            }
            (LogNamespace::Vector, Some(source)) => {
                schema::Definition::new_with_default_metadata(Kind::bytes(), [log_namespace])
                    .with_meaning(OwnedTargetPath::event_root(), "message")
                    .with_source_metadata(
                        source,
                        None,
                        &owned_value_path!("timestamp"),
                        Kind::timestamp(),
                        Some("timestamp"),
                    )
                    .with_source_metadata(
                        source,
                        None,
                        &owned_value_path!("hostname"),
                        Kind::bytes().or_undefined(),
                        Some("host"),
                    )
                    .with_source_metadata(
                        source,
                        None,
                        &owned_value_path!("source_ip"),
                        Kind::bytes().or_undefined(),
                        None,
                    )
                    .with_source_metadata(
                        source,
                        None,
                        &owned_value_path!("severity"),
                        Kind::bytes().or_undefined(),
                        Some("severity"),
                    )
                    .with_source_metadata(
                        source,
                        None,
                        &owned_value_path!("facility"),
                        Kind::bytes().or_undefined(),
                        None,
                    )
                    .with_source_metadata(
                        source,
                        None,
                        &owned_value_path!("version"),
                        Kind::integer().or_undefined(),
                        None,
                    )
                    .with_source_metadata(
                        source,
                        None,
                        &owned_value_path!("appname"),
                        Kind::bytes().or_undefined(),
                        Some("service"),
                    )
                    .with_source_metadata(
                        source,
                        None,
                        &owned_value_path!("msgid"),
                        Kind::bytes().or_undefined(),
                        None,
                    )
                    .with_source_metadata(
                        source,
                        None,
                        &owned_value_path!("procid"),
                        Kind::integer().or_bytes().or_undefined(),
                        None,
                    )
                    .with_source_metadata(
                        source,
                        None,
                        &owned_value_path!("structured_data"),
                        Kind::object(Collection::from_unknown(Kind::object(
                            Collection::from_unknown(Kind::bytes()),
                        ))),
                        None,
                    )
                    .with_source_metadata(
                        source,
                        None,
                        &owned_value_path!("tls_client_metadata"),
                        Kind::object(Collection::empty().with_unknown(Kind::bytes()))
                            .or_undefined(),
                        None,
                    )
            }
        }
    }
}

/// Syslog-specific decoding options.
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Derivative)]
#[derivative(Default)]
pub struct SyslogDeserializerOptions {
    /// Determines whether to replace invalid UTF-8 sequences instead of failing.
    ///
    /// When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].
    ///
    /// [U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
    #[serde(
        default = "default_lossy",
        skip_serializing_if = "vector_core::serde::is_default"
    )]
    #[derivative(Default(value = "default_lossy()"))]
    pub lossy: bool,
}

/// Deserializer that builds an `Event` from a byte frame containing a syslog
/// message.
#[derive(Debug, Clone, Derivative)]
#[derivative(Default)]
pub struct SyslogDeserializer {
    /// The syslog source needs it's own syslog deserializer separate from the
    /// syslog codec since it needs to handle the structured of the decoded data
    /// differently when using the Vector lognamespace.
    pub source: Option<&'static str>,
    #[derivative(Default(value = "default_lossy()"))]
    lossy: bool,
}

impl Deserializer for SyslogDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        let line: Cow<str> = match self.lossy {
            true => String::from_utf8_lossy(&bytes),
            false => Cow::from(std::str::from_utf8(&bytes)?),
        };
        let line = normalize_syslog_frame(&line);

        parse_syslog_line(line.as_ref(), self.source, log_namespace)
    }
}

fn parse_syslog_line(
    line: &str,
    source: Option<&'static str>,
    log_namespace: LogNamespace,
) -> vector_common::Result<SmallVec<[Event; 1]>> {
    match syslog_loose::parse_message_with_year_exact(line, resolve_year, Variant::Either) {
        Ok(parsed) => syslog_message_to_events(parsed, source, log_namespace),
        Err(error) => {
            if let Some(normalized) = normalize_year_first_timestamp(line) {
                let parsed = syslog_loose::parse_message_with_year_exact(
                    &normalized,
                    resolve_year,
                    Variant::Either,
                )?;
                syslog_message_to_events(parsed, source, log_namespace)
            } else if let Some(normalized) = normalize_dash_comma_timestamp(line) {
                let parsed = syslog_loose::parse_message_with_year_exact(
                    &normalized,
                    resolve_year,
                    Variant::Either,
                )?;
                syslog_message_to_events(parsed, source, log_namespace)
            } else if let Some(parsed) = parse_pri_only_message(line) {
                syslog_message_to_events(parsed, source, log_namespace)
            } else {
                Err(error.into())
            }
        }
    }
}

fn syslog_message_to_events(
    parsed: Message<&str>,
    source: Option<&'static str>,
    log_namespace: LogNamespace,
) -> vector_common::Result<SmallVec<[Event; 1]>> {
    let log = match (source, log_namespace) {
        (Some(source), LogNamespace::Vector) => {
            let mut log = LogEvent::from(Value::Bytes(Bytes::from(parsed.msg.to_string())));
            insert_metadata_fields_from_syslog(&mut log, source, parsed, log_namespace);
            log
        }
        _ => {
            let mut log = LogEvent::from(Value::Object(ObjectMap::new()));
            insert_fields_from_syslog(&mut log, parsed, log_namespace);
            log
        }
    };

    Ok(smallvec![Event::from(log)])
}

/// Function used to resolve the year for syslog messages that don't include the
/// year.
///
/// If the current month is January, and the syslog message is for December, it
/// will take the previous year.
///
/// Otherwise, take the current year. Leap-day messages are resolved to the most
/// recent leap year when the inferred year is not a leap year.
fn resolve_year((month, date, _hour, _min, _sec): IncompleteDate) -> i32 {
    let now = Utc::now();
    let year = if now.month() == 1 && month == 12 {
        now.year() - 1
    } else {
        now.year()
    };

    if month == 2 && date == 29 && !is_leap_year(year) {
        previous_leap_year(year)
    } else {
        year
    }
}

fn previous_leap_year(mut year: i32) -> i32 {
    while !is_leap_year(year) {
        year -= 1;
    }

    year
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn normalize_syslog_frame(input: &str) -> Cow<'_, str> {
    let input = input.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    if !input.bytes().any(|byte| matches!(byte, b'\r' | b'\n' | 0)) {
        return Cow::Borrowed(input);
    }

    let mut normalized = String::with_capacity(input.len());
    let mut in_frame_separator = false;

    for ch in input.chars() {
        if matches!(ch, '\r' | '\n' | '\0') {
            if !in_frame_separator {
                normalized.push(' ');
                in_frame_separator = true;
            }
        } else {
            normalized.push(ch);
            in_frame_separator = false;
        }
    }

    Cow::Owned(normalized)
}

/// Accept RFC3164-like timestamps that include the year before the month:
/// `YYYY Mmm DD HH:MM:SS`.
fn normalize_year_first_timestamp(input: &str) -> Option<String> {
    let input = input.trim();
    let (prefix, rest) = split_pri(input);
    let (year, rest) = take_token(rest)?;
    if !is_four_digit_year(year) {
        return None;
    }

    let (month, rest) = take_token(rest)?;
    if !is_month(month) {
        return None;
    }

    let (day, rest) = take_token(rest)?;
    if !is_valid_day(day) {
        return None;
    }

    let (time, rest) = take_token(rest)?;
    if !is_time(time) {
        return None;
    }

    Some(format!("{prefix}{month} {day} {year} {time}{rest}"))
}

/// Accept network-device timestamps shaped as `YYYY-MM-DD,HH:MM:SS`.
fn normalize_dash_comma_timestamp(input: &str) -> Option<String> {
    let input = input.trim();
    let (prefix, rest) = split_pri(input);
    let (timestamp, rest) = take_token(rest)?;
    let (date, time) = timestamp.split_once(',')?;
    if !is_time(time) {
        return None;
    }

    let mut date_parts = date.split('-');
    let (Some(year), Some(month), Some(day), None) = (
        date_parts.next(),
        date_parts.next(),
        date_parts.next(),
        date_parts.next(),
    ) else {
        return None;
    };

    if !is_four_digit_year(year) || !is_valid_day(day) {
        return None;
    }

    let month = month.parse::<usize>().ok()?;
    let month = month_name(month)?;
    let rest = rest.trim_start();

    if rest.is_empty() {
        Some(format!("{prefix}{month} {day} {year} {time}"))
    } else {
        Some(format!("{prefix}{month} {day} {year} {time} {rest}"))
    }
}

fn parse_pri_only_message(input: &str) -> Option<Message<&str>> {
    let input = input.trim();
    let (pri, rest) = parse_pri_prefix(input)?;
    let msg = rest.trim_start();
    if msg.is_empty() {
        return None;
    }

    let (facility, severity) = decompose_pri(pri);
    let (appname, procid, msg) = parse_message_tag(msg);

    Some(Message {
        protocol: Protocol::RFC3164,
        facility,
        severity,
        timestamp: None,
        hostname: None,
        appname,
        procid: procid.map(Into::into),
        msgid: None,
        structured_data: vec![],
        msg,
    })
}

fn parse_pri_prefix(input: &str) -> Option<(u8, &str)> {
    let after_open = input.strip_prefix('<')?;
    let close_index = after_open.find('>')?;
    let pri = &after_open[..close_index];
    if pri.is_empty() || !is_ascii_digits(pri) {
        return None;
    }

    let pri = pri.parse::<u16>().ok()?;
    if pri > 191 {
        return None;
    }

    Some((pri as u8, &after_open[close_index + 1..]))
}

fn parse_message_tag(input: &str) -> (Option<&str>, Option<&str>, &str) {
    let Some((tag, msg)) = input.split_once(':') else {
        return (None, None, input);
    };

    let tag = tag.trim();
    if tag.is_empty() || tag.chars().any(char::is_whitespace) {
        return (None, None, input);
    }

    let (appname, procid) = split_appname_procid(tag);
    (Some(appname), procid, msg.trim_start())
}

fn split_appname_procid(tag: &str) -> (&str, Option<&str>) {
    let Some(without_close) = tag.strip_suffix(']') else {
        return (tag, None);
    };

    let Some(open_index) = without_close.rfind('[') else {
        return (tag, None);
    };

    let appname = &without_close[..open_index];
    let procid = &without_close[open_index + 1..];
    if appname.is_empty() || procid.is_empty() {
        return (tag, None);
    }

    (appname, Some(procid))
}

fn split_pri(input: &str) -> (&str, &str) {
    let Some(after_open) = input.strip_prefix('<') else {
        return ("", input);
    };

    let Some(close_index) = after_open.find('>') else {
        return ("", input);
    };

    let pri = &after_open[..close_index];
    if pri.is_empty() || !is_ascii_digits(pri) {
        return ("", input);
    }

    input.split_at(close_index + 2)
}

fn take_token(input: &str) -> Option<(&str, &str)> {
    let input = input.trim_start();
    if input.is_empty() {
        return None;
    }

    match input.find(char::is_whitespace) {
        Some(index) => Some(input.split_at(index)),
        None => Some((input, "")),
    }
}

fn is_four_digit_year(value: &str) -> bool {
    value.len() == 4
        && is_ascii_digits(value)
        && value
            .parse::<i32>()
            .is_ok_and(|year| (1000..=9999).contains(&year))
}

fn is_month(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "jan"
            | "feb"
            | "mar"
            | "apr"
            | "may"
            | "jun"
            | "jul"
            | "aug"
            | "sep"
            | "oct"
            | "nov"
            | "dec"
    )
}

fn month_name(month: usize) -> Option<&'static str> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    MONTHS.get(month.checked_sub(1)?).copied()
}

fn is_valid_day(value: &str) -> bool {
    is_ascii_digits(value)
        && value
            .parse::<u32>()
            .is_ok_and(|day| (1..=31).contains(&day))
}

fn is_time(value: &str) -> bool {
    let value = value.strip_suffix(':').unwrap_or(value);
    let mut parts = value.split(':');
    let (Some(hour), Some(minute), Some(second), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };

    is_ascii_digits(hour)
        && is_ascii_digits(minute)
        && is_ascii_digits(second)
        && hour.parse::<u32>().is_ok_and(|hour| hour <= 23)
        && minute.parse::<u32>().is_ok_and(|minute| minute <= 59)
        && second.parse::<u32>().is_ok_and(|second| second <= 59)
}

fn is_ascii_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn insert_metadata_fields_from_syslog(
    log: &mut LogEvent,
    source: &'static str,
    parsed: Message<&str>,
    log_namespace: LogNamespace,
) {
    if let Some(timestamp) = parsed.timestamp {
        let timestamp = DateTime::<Utc>::from(timestamp);
        log_namespace.insert_source_metadata(
            source,
            log,
            None::<LegacyKey<&OwnedValuePath>>,
            &owned_value_path!("timestamp"),
            timestamp,
        );
    }
    if let Some(host) = parsed.hostname {
        log_namespace.insert_source_metadata(
            source,
            log,
            None::<LegacyKey<&OwnedValuePath>>,
            &owned_value_path!("hostname"),
            host.to_string(),
        );
    }
    if let Some(severity) = parsed.severity {
        log_namespace.insert_source_metadata(
            source,
            log,
            None::<LegacyKey<&OwnedValuePath>>,
            &owned_value_path!("severity"),
            severity.as_str().to_owned(),
        );
    }
    if let Some(facility) = parsed.facility {
        log_namespace.insert_source_metadata(
            source,
            log,
            None::<LegacyKey<&OwnedValuePath>>,
            &owned_value_path!("facility"),
            facility.as_str().to_owned(),
        );
    }
    if let Protocol::RFC5424(version) = parsed.protocol {
        log_namespace.insert_source_metadata(
            source,
            log,
            None::<LegacyKey<&OwnedValuePath>>,
            &owned_value_path!("version"),
            version as i64,
        );
    }
    if let Some(app_name) = parsed.appname {
        log_namespace.insert_source_metadata(
            source,
            log,
            None::<LegacyKey<&OwnedValuePath>>,
            &owned_value_path!("appname"),
            app_name.to_owned(),
        );
    }
    if let Some(msg_id) = parsed.msgid {
        log_namespace.insert_source_metadata(
            source,
            log,
            None::<LegacyKey<&OwnedValuePath>>,
            &owned_value_path!("msgid"),
            msg_id.to_owned(),
        );
    }
    if let Some(procid) = parsed.procid {
        let value: Value = match procid {
            ProcId::PID(pid) => pid.into(),
            ProcId::Name(name) => name.to_string().into(),
        };
        log_namespace.insert_source_metadata(
            source,
            log,
            None::<LegacyKey<&OwnedValuePath>>,
            &owned_value_path!("procid"),
            value,
        );
    }

    let mut sdata = ObjectMap::new();
    for element in parsed.structured_data.into_iter() {
        let mut data = ObjectMap::new();

        for (name, value) in element.params() {
            data.insert(name.to_string().into(), value.into());
        }

        sdata.insert(element.id.into(), data.into());
    }

    log_namespace.insert_source_metadata(
        source,
        log,
        None::<LegacyKey<&OwnedValuePath>>,
        &owned_value_path!("structured_data"),
        sdata,
    );
}

fn insert_fields_from_syslog(
    log: &mut LogEvent,
    parsed: Message<&str>,
    log_namespace: LogNamespace,
) {
    match log_namespace {
        LogNamespace::Legacy => {
            log.maybe_insert(log_schema().message_key_target_path(), parsed.msg);
        }
        LogNamespace::Vector => {
            log.insert(event_path!("message"), parsed.msg);
        }
    }

    if let Some(timestamp) = parsed.timestamp {
        let timestamp = DateTime::<Utc>::from(timestamp);
        match log_namespace {
            LogNamespace::Legacy => {
                log.maybe_insert(log_schema().timestamp_key_target_path(), timestamp);
            }
            LogNamespace::Vector => {
                log.insert(event_path!("timestamp"), timestamp);
            }
        };
    }
    if let Some(host) = parsed.hostname {
        log.insert(event_path!("hostname"), host.to_string());
    }
    if let Some(severity) = parsed.severity {
        log.insert(event_path!("severity"), severity.as_str().to_owned());
    }
    if let Some(facility) = parsed.facility {
        log.insert(event_path!("facility"), facility.as_str().to_owned());
    }
    if let Protocol::RFC5424(version) = parsed.protocol {
        log.insert(event_path!("version"), version as i64);
    }
    if let Some(app_name) = parsed.appname {
        log.insert(event_path!("appname"), app_name.to_owned());
    }
    if let Some(msg_id) = parsed.msgid {
        log.insert(event_path!("msgid"), msg_id.to_owned());
    }
    if let Some(procid) = parsed.procid {
        let value: Value = match procid {
            ProcId::PID(pid) => pid.into(),
            ProcId::Name(name) => name.to_string().into(),
        };
        log.insert(event_path!("procid"), value);
    }

    for element in parsed.structured_data.into_iter() {
        let mut sdata = ObjectMap::new();
        for (name, value) in element.params() {
            sdata.insert(name.to_string().into(), value.into());
        }
        log.insert(event_path!(element.id), sdata);
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Datelike as _, TimeZone as _, Timelike as _};
    use vector_core::config::log_schema;

    use super::*;

    fn parse(input: &str) -> LogEvent {
        let deserializer = SyslogDeserializer::default();

        deserializer
            .parse(Bytes::from(input.to_owned()), LogNamespace::Vector)
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
            .into_log()
    }

    #[test]
    fn deserialize_syslog_legacy_namespace() {
        let input =
            Bytes::from("<34>1 2003-10-11T22:14:15.003Z mymachine.example.com su - ID47 - MSG");
        let deserializer = SyslogDeserializer::default();

        let events = deserializer.parse(input, LogNamespace::Legacy).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].as_log()[log_schema().message_key().unwrap().to_string()],
            "MSG".into()
        );
        assert!(
            events[0].as_log()[log_schema().timestamp_key().unwrap().to_string()].is_timestamp()
        );
    }

    #[test]
    fn deserialize_syslog_vector_namespace() {
        let input =
            Bytes::from("<34>1 2003-10-11T22:14:15.003Z mymachine.example.com su - ID47 - MSG");
        let deserializer = SyslogDeserializer::default();

        let events = deserializer.parse(input, LogNamespace::Vector).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].as_log()["message"], "MSG".into());
        assert!(events[0].as_log()["timestamp"].is_timestamp());
    }

    #[test]
    fn deserialize_syslog_rfc3339_timestamp() {
        let log =
            parse("<190>2026-06-18T04:24:32.123456+00:00 device-rfc3339 app[123]: RFC3339 message");
        assert_eq!(log["message"], "RFC3339 message".into());
        assert_eq!(log["hostname"], "device-rfc3339".into());
        assert_eq!(log["appname"], "app".into());
        assert_eq!(log["procid"], 123.into());
        assert_eq!(log["facility"], "local7".into());
        assert_eq!(log["severity"], "info".into());
        assert_eq!(
            log["timestamp"],
            Utc.with_ymd_and_hms(2026, 6, 18, 4, 24, 32)
                .unwrap()
                .with_nanosecond(123_456_000)
                .unwrap()
                .into()
        );
    }

    #[test]
    fn deserialize_syslog_rfc3339_timestamp_with_offset() {
        let log = parse(
            r#"<190>2019-02-13T21:53:30.605850+02:00 host app: [origin software="rsyslogd"] start"#,
        );
        assert_eq!(log["message"], "start".into());
        assert_eq!(log["hostname"], "host".into());
        assert_eq!(log["appname"], "app".into());
        assert_eq!(
            log["timestamp"],
            Utc.with_ymd_and_hms(2019, 2, 13, 19, 53, 30)
                .unwrap()
                .with_nanosecond(605_850_000)
                .unwrap()
                .into()
        );
        assert_eq!(log["origin.software"], "rsyslogd".into());
    }

    #[test]
    fn deserialize_syslog_rfc3164() {
        let log = parse("<34>Jun 18 04:24:32 host app[123]: RFC3164 message");
        assert_eq!(log["message"], "RFC3164 message".into());
        assert_eq!(log["hostname"], "host".into());
        assert_eq!(log["appname"], "app".into());
        assert_eq!(log["procid"], 123.into());
        assert_eq!(log["facility"], "auth".into());
        assert_eq!(log["severity"], "crit".into());

        let expected = chrono::DateTime::<Utc>::from(
            chrono::Local
                .with_ymd_and_hms(Utc::now().year(), 6, 18, 4, 24, 32)
                .earliest()
                .unwrap(),
        );
        assert_eq!(log["timestamp"], expected.into());
    }

    #[test]
    fn deserialize_syslog_rfc3164_leap_day_with_nul_padding() {
        let log = parse(
            "<190>Feb 29 23:53:33 device-redacted %OLT: Interface EPON0/1:11's OAM Operational Status: Operational \0",
        );
        let expected_year = previous_leap_year(Utc::now().year());

        assert_eq!(
            log["message"],
            "Interface EPON0/1:11's OAM Operational Status: Operational".into()
        );
        assert_eq!(log["hostname"], "device-redacted".into());
        assert_eq!(log["appname"], "%OLT".into());
        assert_eq!(log["facility"], "local7".into());
        assert_eq!(log["severity"], "info".into());
        let expected = chrono::DateTime::<Utc>::from(
            chrono::Local
                .with_ymd_and_hms(expected_year, 2, 29, 23, 53, 33)
                .earliest()
                .unwrap(),
        );
        assert_eq!(log["timestamp"], expected.into());
    }

    #[test]
    fn deserialize_syslog_rfc3164_missing_pri() {
        let log = parse("Jun 18 04:24:32 host app: RFC3164 missing PRI");
        assert_eq!(log["message"], "RFC3164 missing PRI".into());
        assert_eq!(log["hostname"], "host".into());
        assert_eq!(log["appname"], "app".into());
        let fields = log.value().as_object().unwrap();
        assert!(fields.get("severity").is_none());
        assert!(fields.get("facility").is_none());
    }

    #[test]
    fn deserialize_syslog_rfc3164_no_hostname_with_proc_id() {
        let log = parse("<133>Jun 18 04:24:32 haproxy[73411]: Proxy started");
        assert_eq!(log["message"], "Proxy started".into());
        assert_eq!(log["appname"], "haproxy".into());
        assert_eq!(log["procid"], 73411.into());
        assert_eq!(log["facility"], "local0".into());
        assert_eq!(log["severity"], "notice".into());
        assert!(log.value().as_object().unwrap().get("hostname").is_none());
    }

    #[test]
    fn deserialize_syslog_rfc3164_structured_data() {
        let log = parse(
            r#"<46>Jun 18 04:24:32 host rsyslogd: [origin software="rsyslogd" swVersion="8.32.0"] start"#,
        );
        assert_eq!(log["message"], "start".into());
        assert_eq!(log["hostname"], "host".into());
        assert_eq!(log["appname"], "rsyslogd".into());
        assert_eq!(log["origin.software"], "rsyslogd".into());
        assert_eq!(log["origin.swVersion"], "8.32.0".into());
    }

    #[test]
    fn deserialize_syslog_rfc3164_with_year() {
        let log = parse("<34>Jun 18 2026 04:24:32 host app: RFC3164 message");
        assert_eq!(log["message"], "RFC3164 message".into());
        assert_eq!(log["hostname"], "host".into());
        assert_eq!(log["appname"], "app".into());

        let expected = chrono::DateTime::<Utc>::from(
            chrono::Local
                .with_ymd_and_hms(2026, 6, 18, 4, 24, 32)
                .earliest()
                .unwrap(),
        );
        assert_eq!(log["timestamp"], expected.into());
    }

    #[test]
    fn deserialize_syslog_rfc5424() {
        let log = parse(
            r#"<13>1 2020-03-13T20:45:38.119Z device.example.net non 2426 ID931 [exampleSDID@32473 iut="3"] RFC5424 message"#,
        );
        assert_eq!(log["message"], "RFC5424 message".into());
        assert_eq!(log["hostname"], "device.example.net".into());
        assert_eq!(log["appname"], "non".into());
        assert_eq!(log["procid"], 2426.into());
        assert_eq!(log["msgid"], "ID931".into());
        assert_eq!(log["version"], 1.into());
        assert_eq!(
            log["timestamp"],
            Utc.with_ymd_and_hms(2020, 3, 13, 20, 45, 38)
                .unwrap()
                .with_nanosecond(119_000_000)
                .unwrap()
                .into()
        );
    }

    #[test]
    fn deserialize_syslog_rfc5424_nil_header_fields() {
        let log = parse("<13>1 - - - - - - RFC5424 nil header fields");
        assert_eq!(log["message"], "RFC5424 nil header fields".into());
        assert_eq!(log["facility"], "user".into());
        assert_eq!(log["severity"], "notice".into());
        assert_eq!(log["version"], 1.into());
        let fields = log.value().as_object().unwrap();
        assert!(fields.get("timestamp").is_none());
        assert!(fields.get("hostname").is_none());
        assert!(fields.get("appname").is_none());
        assert!(fields.get("procid").is_none());
        assert!(fields.get("msgid").is_none());
    }

    #[test]
    fn deserialize_syslog_missing_pri_rfc5424() {
        let log = parse("1 2020-05-22T14:59:09.250-03:00 router app 6589 - - Missing PRI RFC5424");
        assert_eq!(log["message"], "Missing PRI RFC5424".into());
        assert_eq!(log["hostname"], "router".into());
        assert_eq!(log["appname"], "app".into());
        assert_eq!(log["procid"], 6589.into());
        assert_eq!(log["version"], 1.into());
        assert_eq!(
            log["timestamp"],
            Utc.with_ymd_and_hms(2020, 5, 22, 17, 59, 9)
                .unwrap()
                .with_nanosecond(250_000_000)
                .unwrap()
                .into()
        );
        let fields = log.value().as_object().unwrap();
        assert!(fields.get("severity").is_none());
        assert!(fields.get("facility").is_none());
    }

    #[test]
    fn deserialize_syslog_rfc5424_multiple_structured_data_blocks() {
        let log = parse(
            r#"<165>1 2003-10-11T22:14:15.003Z host app - ID47 [exampleSDID@32473 iut="3"][priority@32473 class="high"] message"#,
        );
        assert_eq!(log["message"], "message".into());
        assert_eq!(log["version"], 1.into());
        assert_eq!(log["exampleSDID@32473.iut"], "3".into());
        assert_eq!(log["priority@32473.class"], "high".into());
    }

    #[test]
    fn deserialize_syslog_year_first_timestamp() {
        let input = Bytes::from(
            "<130>2026 Jun 18 04:24:32 zte-device command-log:An alarm 35125 level minor occurred",
        );
        let deserializer = SyslogDeserializer::default();

        let events = deserializer.parse(input, LogNamespace::Vector).unwrap();
        let log = events[0].as_log();
        assert_eq!(log["message"], "An alarm 35125 level minor occurred".into());
        assert_eq!(log["hostname"], "zte-device".into());
        assert_eq!(log["appname"], "command-log".into());
        assert_eq!(log["facility"], "local0".into());
        assert_eq!(log["severity"], "crit".into());

        let expected = chrono::DateTime::<Utc>::from(
            chrono::Local
                .with_ymd_and_hms(2026, 6, 18, 4, 24, 32)
                .earliest()
                .unwrap(),
        );
        assert_eq!(log["timestamp"], expected.into());
    }

    #[test]
    fn deserialize_syslog_multiline_message_without_vrl_workaround() {
        let log = parse(
            "<130>2026 Jun 18 04:24:32 device-redacted command-log:An alarm 35125 level minor occurred at 04:24:32 06/18/2026 UTC sent by MCP GPON alarm link: shelf 1 slot 8 olt 11 onu 83 level 2 \n on  \n",
        );

        assert_eq!(
            log["message"],
            "An alarm 35125 level minor occurred at 04:24:32 06/18/2026 UTC sent by MCP GPON alarm link: shelf 1 slot 8 olt 11 onu 83 level 2   on".into()
        );
        assert_eq!(log["hostname"], "device-redacted".into());
        assert_eq!(log["appname"], "command-log".into());

        let expected = chrono::DateTime::<Utc>::from(
            chrono::Local
                .with_ymd_and_hms(2026, 6, 18, 4, 24, 32)
                .earliest()
                .unwrap(),
        );
        assert_eq!(log["timestamp"], expected.into());
    }

    #[test]
    fn deserialize_syslog_dash_comma_timestamp() {
        let log = parse(
            "<190>2025-12-21,23:06:36  device-redacted: SSH-SERVER-6-CLOSE_SESSION:Scrn pty want close Session id = 0",
        );
        assert_eq!(
            log["message"],
            "SSH-SERVER-6-CLOSE_SESSION:Scrn pty want close Session id = 0".into()
        );
        assert_eq!(log["hostname"], "device-redacted".into());
        assert_eq!(log["facility"], "local7".into());
        assert_eq!(log["severity"], "info".into());
        assert!(log.value().as_object().unwrap().get("appname").is_none());

        let expected = chrono::DateTime::<Utc>::from(
            chrono::Local
                .with_ymd_and_hms(2025, 12, 21, 23, 6, 36)
                .earliest()
                .unwrap(),
        );
        assert_eq!(log["timestamp"], expected.into());
    }

    #[test]
    fn deserialize_syslog_pri_only_network_message() {
        let log = parse("<174>%LINK-I-Up:  1/e12");
        assert_eq!(log["message"], "1/e12".into());
        assert_eq!(log["appname"], "%LINK-I-Up".into());
        assert_eq!(log["facility"], "local5".into());
        assert_eq!(log["severity"], "info".into());
        let fields = log.value().as_object().unwrap();
        assert!(fields.get("timestamp").is_none());
        assert!(fields.get("hostname").is_none());
    }

    #[test]
    fn deserialize_syslog_network_vendor_samples() {
        #[derive(Clone, Copy)]
        struct VendorCase {
            name: &'static str,
            input: &'static str,
            hostname: Option<&'static str>,
            appname: Option<&'static str>,
            procid: Option<i64>,
            message: &'static str,
        }

        let cases = [
            VendorCase {
                name: "cisco-classic-rfc3164",
                input: "<189>Jun 18 04:24:32 cisco-device %LINK-3-UPDOWN: Interface GigabitEthernet0/1, changed state to up",
                hostname: Some("cisco-device"),
                appname: Some("%LINK-3-UPDOWN"),
                procid: None,
                message: "Interface GigabitEthernet0/1, changed state to up",
            },
            VendorCase {
                name: "cisco-asa-rfc3164",
                input: "<166>Jun 18 04:24:32 asa-device %ASA-6-302013: Built outbound TCP connection 12345 for outside:192.0.2.10/443",
                hostname: Some("asa-device"),
                appname: Some("%ASA-6-302013"),
                procid: None,
                message: "Built outbound TCP connection 12345 for outside:192.0.2.10/443",
            },
            VendorCase {
                name: "cisco-nxos-rfc3339-forward",
                input: "<190>2026-06-18T04:24:32Z nxos-device %ETHPORT-5-IF_UP: Interface Ethernet1/1 is up",
                hostname: Some("nxos-device"),
                appname: Some("%ETHPORT-5-IF_UP"),
                procid: None,
                message: "Interface Ethernet1/1 is up",
            },
            VendorCase {
                name: "juniper-rfc5424",
                input: "<28>1 2020-05-22T14:59:09.250-03:00 juniper-device OX-XXX-CONTEUDO:rpd 6589 - - bgp_listen_accept: Connection from 192.0.2.10",
                hostname: Some("juniper-device"),
                appname: Some("OX-XXX-CONTEUDO:rpd"),
                procid: Some(6589),
                message: "bgp_listen_accept: Connection from 192.0.2.10",
            },
            VendorCase {
                name: "huawei-vrp-rfc3164",
                input: "<189>Jun 18 04:24:32 huawei-device %%01IFNET/4/LINK_STATE(l)[12345]: The line protocol IP on the interface GigabitEthernet0/0/1 has entered the UP state.",
                hostname: Some("huawei-device"),
                appname: Some("%%01IFNET/4/LINK_STATE(l)"),
                procid: Some(12345),
                message: "The line protocol IP on the interface GigabitEthernet0/0/1 has entered the UP state.",
            },
            VendorCase {
                name: "zte-year-first-rfc3164",
                input: "<130>2026 Jun 18 04:24:32 zte-device command-log:An alarm 35125 level minor occurred",
                hostname: Some("zte-device"),
                appname: Some("command-log"),
                procid: None,
                message: "An alarm 35125 level minor occurred",
            },
            VendorCase {
                name: "arista-eos-rfc3164",
                input: "<190>Jun 18 04:24:32 arista-device ConfigAgent: %SYS-5-CONFIG_I: Configured from console by redacted-user",
                hostname: Some("arista-device"),
                appname: Some("ConfigAgent"),
                procid: None,
                message: "%SYS-5-CONFIG_I: Configured from console by redacted-user",
            },
            VendorCase {
                name: "dell-switch-rfc3164",
                input: "<189>Jun 18 04:24:32 dell-device dn_alm: %STKUNIT0-M:CP %IFMGR-5-ASTATE_UP: Interface ethernet1/1/1 is up",
                hostname: Some("dell-device"),
                appname: Some("dn_alm"),
                procid: None,
                message: "%STKUNIT0-M:CP %IFMGR-5-ASTATE_UP: Interface ethernet1/1/1 is up",
            },
            VendorCase {
                name: "dell-powerconnect-pri-only",
                input: "<174>%LINK-I-Up:  1/e12",
                hostname: None,
                appname: Some("%LINK-I-Up"),
                procid: None,
                message: "1/e12",
            },
            VendorCase {
                name: "mikrotik-routeros-rfc3164",
                input: "<134>Jun 18 04:24:32 mikrotik-device system,info,account user redacted-user logged in from 192.0.2.10 via ssh",
                hostname: Some("mikrotik-device"),
                appname: Some("system,info,account"),
                procid: None,
                message: "user redacted-user logged in from 192.0.2.10 via ssh",
            },
            VendorCase {
                name: "raisecom-dash-comma-timestamp",
                input: "<189>2025-12-28,05:51:46  device-redacted: SSH-SERVER-5-PASSWORD_OK:password auth succeeded for 'redacted-user' from 203.0.113.7",
                hostname: Some("device-redacted"),
                appname: None,
                procid: None,
                message: "SSH-SERVER-5-PASSWORD_OK:password auth succeeded for 'redacted-user' from 203.0.113.7",
            },
            VendorCase {
                name: "olt-rfc3164-leap-day-nul-padding",
                input: "<190>Feb 29 23:31:55 device-redacted %EPON-ONUREG: ONU 0200.0000.0001 is registered on EPON0/5:29. \0",
                hostname: Some("device-redacted"),
                appname: Some("%EPON-ONUREG"),
                procid: None,
                message: "ONU 0200.0000.0001 is registered on EPON0/5:29.",
            },
        ];

        for case in cases {
            let log = parse(case.input);
            let fields = log.value().as_object().unwrap();

            match case.hostname {
                Some(hostname) => {
                    assert_eq!(log["hostname"], hostname.into(), "{} hostname", case.name);
                }
                None => assert!(
                    fields.get("hostname").is_none(),
                    "{} unexpected hostname",
                    case.name
                ),
            }

            match case.appname {
                Some(appname) => {
                    assert_eq!(log["appname"], appname.into(), "{} appname", case.name);
                }
                None => assert!(
                    fields.get("appname").is_none(),
                    "{} unexpected appname",
                    case.name
                ),
            }

            match case.procid {
                Some(procid) => {
                    assert_eq!(log["procid"], procid.into(), "{} procid", case.name);
                }
                None => assert!(
                    fields.get("procid").is_none(),
                    "{} unexpected procid",
                    case.name
                ),
            }

            assert_eq!(log["message"], case.message.into(), "{} message", case.name);
        }
    }

    #[test]
    fn deserialize_syslog_trims_json_escaped_nul_character() {
        let log = parse(
            "\u{0000}<190>Feb 29 23:53:33 device-redacted %OLT: Interface EPON0/1:11's CTC OAM extension negotiated successfully! \u{0000}",
        );

        assert_eq!(
            log["message"],
            "Interface EPON0/1:11's CTC OAM extension negotiated successfully!".into()
        );
        assert_eq!(log["hostname"], "device-redacted".into());
        assert_eq!(log["appname"], "%OLT".into());
    }
}
