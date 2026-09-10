use std::{borrow::Cow, collections::BTreeMap};

use chrono::{DateTime, Datelike, Utc};
use syslog_loose::{IncompleteDate, Message, ProcId, Protocol, Variant, decompose_pri};
use vrl::prelude::*;

pub(crate) fn parse_syslog(value: &Value, ctx: &Context) -> Resolved {
    let message = value.try_bytes_utf8_lossy()?;
    let timezone = match ctx.timezone() {
        TimeZone::Local => None,
        TimeZone::Named(tz) => Some(*tz),
    };

    parse_syslog_to_value(&message, timezone)
}

fn parse_syslog_to_value<Tz>(message: &str, timezone: Option<Tz>) -> Resolved
where
    Tz: chrono::TimeZone + Copy,
{
    let message = normalize_syslog_frame(message);
    let message = message.as_ref();

    match syslog_loose::parse_message_with_year_exact_tz(
        message,
        resolve_year,
        timezone,
        Variant::Either,
    ) {
        Ok(parsed) => Ok(message_to_value(parsed)),
        Err(error) => {
            if let Some(normalized) = normalize_year_first_timestamp(message) {
                let parsed = syslog_loose::parse_message_with_year_exact_tz(
                    &normalized,
                    resolve_year,
                    timezone,
                    Variant::Either,
                )?;
                Ok(message_to_value(parsed))
            } else if let Some(normalized) = normalize_dash_comma_timestamp(message) {
                let parsed = syslog_loose::parse_message_with_year_exact_tz(
                    &normalized,
                    resolve_year,
                    timezone,
                    Variant::Either,
                )?;
                Ok(message_to_value(parsed))
            } else if let Some(parsed) = parse_pri_only_message(message) {
                Ok(message_to_value(parsed))
            } else {
                Err(error.into())
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParseSyslog;

impl Function for ParseSyslog {
    fn identifier(&self) -> &'static str {
        "parse_syslog"
    }

    fn usage(&self) -> &'static str {
        "Parses the `value` in [Syslog](https://en.wikipedia.org/wiki/Syslog) format."
    }

    fn category(&self) -> &'static str {
        Category::Parse.as_ref()
    }

    fn internal_failure_reasons(&self) -> &'static [&'static str] {
        &["`value` is not a properly formatted Syslog message."]
    }

    fn return_kind(&self) -> u16 {
        kind::OBJECT
    }

    fn notices(&self) -> &'static [&'static str] {
        &[
            indoc! {"
                The function makes a best effort to parse the various Syslog formats that exists out
                in the wild. This includes [RFC 6587](https://tools.ietf.org/html/rfc6587),
                [RFC 5424](https://tools.ietf.org/html/rfc5424),
                [RFC 3164](https://tools.ietf.org/html/rfc3164), and other common variations (such
                as RFC 3339 timestamps, year-first RFC 3164-like timestamps, comma-separated
                `YYYY-MM-DD,HH:MM:SS` timestamps, PRI-only network-device messages, multi-line
                messages, NUL-padded frames, and the Nginx Syslog style).
            "},
            "All values are returned as strings. We recommend manually coercing values to desired types as you see fit.",
        ]
    }

    fn parameters(&self) -> &'static [Parameter] {
        const PARAMETERS: &[Parameter] = &[Parameter::required(
            "value",
            kind::BYTES,
            "The text containing the Syslog message to parse.",
        )];
        PARAMETERS
    }

    fn examples(&self) -> &'static [Example] {
        &[example! {
            title: "Parse Syslog log (5424)",
            source: r#"parse_syslog!(s'<13>1 2020-03-13T20:45:38.119Z device.example.net non 2426 ID931 [exampleSDID@32473 iut="3" eventSource= "Application" eventID="1011"] Try to override the THX port, maybe it will reboot the neural interface!')"#,
            result: Ok(indoc! {r#"{
                "appname": "non",
                "exampleSDID@32473": {
                    "eventID": "1011",
                    "eventSource": "Application",
                    "iut": "3"
                },
                "facility": "user",
                "hostname": "device.example.net",
                "message": "Try to override the THX port, maybe it will reboot the neural interface!",
                "msgid": "ID931",
                "procid": 2426,
                "severity": "notice",
                "timestamp": "2020-03-13T20:45:38.119Z",
                "version": 1
            }"#}),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(ParseSyslogFn { value }.as_expr())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ParseSyslogFn {
    pub(crate) value: Box<dyn Expression>,
}

impl FunctionExpression for ParseSyslogFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        parse_syslog(&value, ctx)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::object(inner_kind()).fallible()
    }
}

/// Function used to resolve the year for syslog messages that don't include the
/// year. If the current month is January, and the syslog message is for
/// December, it will take the previous year. Otherwise, take the current year.
/// Leap-day messages are resolved to the most recent leap year when the
/// inferred year is not a leap year.
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

/// Create a `Value::Map` from the fields of the given syslog message.
fn message_to_value(message: Message<&str>) -> Value {
    let mut result = BTreeMap::new();

    result.insert("message".to_string().into(), message.msg.to_string().into());

    if let Some(host) = message.hostname {
        result.insert("hostname".to_string().into(), host.to_string().into());
    }

    if let Some(severity) = message.severity {
        result.insert(
            "severity".to_string().into(),
            severity.as_str().to_owned().into(),
        );
    }

    if let Some(facility) = message.facility {
        result.insert(
            "facility".to_string().into(),
            facility.as_str().to_owned().into(),
        );
    }

    if let Protocol::RFC5424(version) = message.protocol {
        result.insert("version".to_string().into(), version.into());
    }

    if let Some(app_name) = message.appname {
        result.insert("appname".to_string().into(), app_name.to_owned().into());
    }

    if let Some(msg_id) = message.msgid {
        result.insert("msgid".to_string().into(), msg_id.to_owned().into());
    }

    if let Some(timestamp) = message.timestamp {
        let timestamp: DateTime<Utc> = timestamp.into();
        result.insert("timestamp".to_string().into(), timestamp.into());
    }

    if let Some(procid) = message.procid {
        let value: Value = match procid {
            ProcId::PID(pid) => pid.into(),
            ProcId::Name(name) => name.to_string().into(),
        };
        result.insert("procid".to_string().into(), value);
    }

    for element in message.structured_data {
        let mut sdata = BTreeMap::new();
        for (name, value) in element.params() {
            sdata.insert((*name).into(), value.into());
        }
        result.insert(element.id.to_string().into(), sdata.into());
    }

    result.into()
}

fn inner_kind() -> BTreeMap<Field, Kind> {
    BTreeMap::from([
        ("message".into(), Kind::bytes()),
        ("hostname".into(), Kind::bytes().or_null()),
        ("severity".into(), Kind::bytes().or_null()),
        ("facility".into(), Kind::bytes().or_null()),
        ("appname".into(), Kind::bytes().or_null()),
        ("msgid".into(), Kind::bytes().or_null()),
        ("timestamp".into(), Kind::timestamp().or_null()),
        ("procid".into(), Kind::bytes().or_integer().or_null()),
        ("version".into(), Kind::integer().or_null()),
    ])
}

#[cfg(test)]
mod tests {
    use chrono::{Datelike as _, TimeZone as _, Timelike as _};

    use super::*;

    fn parse(input: &str) -> Value {
        parse_syslog_to_value(input, Some(Utc)).unwrap()
    }

    #[test]
    fn parses_rfc5424() {
        let value = parse(
            r#"<13>1 2020-03-13T20:45:38.119Z device.example.net non 2426 ID931 [exampleSDID@32473 iut="3" eventSource= "Application" eventID="1011"] Try to override the THX port, maybe it will reboot the neural interface!"#,
        );
        let map = value.as_object().unwrap();

        assert_eq!(map.get("severity"), Some(&Value::from("notice")));
        assert_eq!(map.get("facility"), Some(&Value::from("user")));
        assert_eq!(
            map.get("timestamp"),
            Some(&Value::from(
                Utc.with_ymd_and_hms(2020, 3, 13, 20, 45, 38)
                    .unwrap()
                    .with_nanosecond(119_000_000)
                    .unwrap()
            ))
        );
        assert_eq!(
            map.get("hostname"),
            Some(&Value::from("device.example.net"))
        );
        assert_eq!(map.get("appname"), Some(&Value::from("non")));
        assert_eq!(map.get("procid"), Some(&Value::from(2426)));
        assert_eq!(map.get("msgid"), Some(&Value::from("ID931")));
        assert_eq!(
            map.get("message"),
            Some(&Value::from(
                "Try to override the THX port, maybe it will reboot the neural interface!"
            ))
        );
        assert_eq!(map.get("version"), Some(&Value::from(1)));

        let structured_data = map.get("exampleSDID@32473").unwrap().as_object().unwrap();
        assert_eq!(structured_data.get("iut"), Some(&Value::from("3")));
        assert_eq!(
            structured_data.get("eventSource"),
            Some(&Value::from("Application"))
        );
        assert_eq!(structured_data.get("eventID"), Some(&Value::from("1011")));
    }

    #[test]
    fn parses_rfc5424_nil_header_fields() {
        let value = parse("<13>1 - - - - - - RFC5424 nil header fields");
        let map = value.as_object().unwrap();

        assert_eq!(map.get("severity"), Some(&Value::from("notice")));
        assert_eq!(map.get("facility"), Some(&Value::from("user")));
        assert_eq!(map.get("version"), Some(&Value::from(1)));
        assert_eq!(
            map.get("message"),
            Some(&Value::from("RFC5424 nil header fields"))
        );
        assert!(!map.contains_key("timestamp"));
        assert!(!map.contains_key("hostname"));
        assert!(!map.contains_key("appname"));
        assert!(!map.contains_key("procid"));
        assert!(!map.contains_key("msgid"));
    }

    #[test]
    fn parses_rfc5424_multiple_structured_data_blocks() {
        let value = parse(
            r#"<165>1 2003-10-11T22:14:15.003Z host app - ID47 [exampleSDID@32473 iut="3"][priority@32473 class="high"] message"#,
        );
        let map = value.as_object().unwrap();

        assert_eq!(map.get("message"), Some(&Value::from("message")));
        assert_eq!(map.get("version"), Some(&Value::from(1)));

        let first = map.get("exampleSDID@32473").unwrap().as_object().unwrap();
        assert_eq!(first.get("iut"), Some(&Value::from("3")));

        let second = map.get("priority@32473").unwrap().as_object().unwrap();
        assert_eq!(second.get("class"), Some(&Value::from("high")));
    }

    #[test]
    fn parses_rfc3339_timestamp() {
        let value =
            parse("<190>2026-06-18T04:24:32.123456+00:00 device-rfc3339 app[123]: RFC3339 message");
        let map = value.as_object().unwrap();

        assert_eq!(map.get("severity"), Some(&Value::from("info")));
        assert_eq!(map.get("facility"), Some(&Value::from("local7")));
        assert_eq!(
            map.get("timestamp"),
            Some(&Value::from(
                Utc.with_ymd_and_hms(2026, 6, 18, 4, 24, 32)
                    .unwrap()
                    .with_nanosecond(123_456_000)
                    .unwrap()
            ))
        );
        assert_eq!(map.get("hostname"), Some(&Value::from("device-rfc3339")));
        assert_eq!(map.get("appname"), Some(&Value::from("app")));
        assert_eq!(map.get("procid"), Some(&Value::from(123)));
        assert_eq!(map.get("message"), Some(&Value::from("RFC3339 message")));
    }

    #[test]
    fn parses_rfc3339_timestamp_with_offset() {
        let value = parse(
            r#"<190>2019-02-13T21:53:30.605850+02:00 host app: [origin software="rsyslogd"] start"#,
        );
        let map = value.as_object().unwrap();

        assert_eq!(
            map.get("timestamp"),
            Some(&Value::from(
                Utc.with_ymd_and_hms(2019, 2, 13, 19, 53, 30)
                    .unwrap()
                    .with_nanosecond(605_850_000)
                    .unwrap()
            ))
        );
        assert_eq!(map.get("message"), Some(&Value::from("start")));

        let structured_data = map.get("origin").unwrap().as_object().unwrap();
        assert_eq!(
            structured_data.get("software"),
            Some(&Value::from("rsyslogd"))
        );
    }

    #[test]
    fn parses_rfc3164() {
        let value = parse("<34>Jun 18 04:24:32 host app[123]: RFC3164 message");
        let map = value.as_object().unwrap();

        assert_eq!(map.get("severity"), Some(&Value::from("crit")));
        assert_eq!(map.get("facility"), Some(&Value::from("auth")));
        assert_eq!(
            map.get("timestamp"),
            Some(&Value::from(
                Utc.with_ymd_and_hms(Utc::now().year(), 6, 18, 4, 24, 32)
                    .unwrap()
            ))
        );
        assert_eq!(map.get("hostname"), Some(&Value::from("host")));
        assert_eq!(map.get("appname"), Some(&Value::from("app")));
        assert_eq!(map.get("procid"), Some(&Value::from(123)));
        assert_eq!(map.get("message"), Some(&Value::from("RFC3164 message")));
    }

    #[test]
    fn parses_rfc3164_leap_day_with_nul_padding() {
        let value = parse(
            "<190>Feb 29 23:53:33 device-redacted %OLT: Interface EPON0/1:11's OAM Operational Status: Operational \0",
        );
        let map = value.as_object().unwrap();
        let expected_year = previous_leap_year(Utc::now().year());

        assert_eq!(map.get("severity"), Some(&Value::from("info")));
        assert_eq!(map.get("facility"), Some(&Value::from("local7")));
        assert_eq!(
            map.get("timestamp"),
            Some(&Value::from(
                Utc.with_ymd_and_hms(expected_year, 2, 29, 23, 53, 33)
                    .unwrap()
            ))
        );
        assert_eq!(map.get("hostname"), Some(&Value::from("device-redacted")));
        assert_eq!(map.get("appname"), Some(&Value::from("%OLT")));
        assert_eq!(
            map.get("message"),
            Some(&Value::from(
                "Interface EPON0/1:11's OAM Operational Status: Operational"
            ))
        );
    }

    #[test]
    fn parses_multiline_rfc3164_message_without_vrl_workaround() {
        let value = parse(
            "<130>2026 Jun 18 04:24:32 device-redacted command-log:An alarm 35125 level minor occurred at 04:24:32 06/18/2026 UTC sent by MCP GPON alarm link: shelf 1 slot 8 olt 11 onu 83 level 2 \n on  \n",
        );
        let map = value.as_object().unwrap();

        assert_eq!(
            map.get("timestamp"),
            Some(&Value::from(
                Utc.with_ymd_and_hms(2026, 6, 18, 4, 24, 32).unwrap()
            ))
        );
        assert_eq!(map.get("hostname"), Some(&Value::from("device-redacted")));
        assert_eq!(map.get("appname"), Some(&Value::from("command-log")));
        assert_eq!(
            map.get("message"),
            Some(&Value::from(
                "An alarm 35125 level minor occurred at 04:24:32 06/18/2026 UTC sent by MCP GPON alarm link: shelf 1 slot 8 olt 11 onu 83 level 2   on"
            ))
        );
    }

    #[test]
    fn parses_rfc3164_missing_pri() {
        let value = parse("Jun 18 04:24:32 host app: RFC3164 missing PRI");
        let map = value.as_object().unwrap();

        assert_eq!(map.get("hostname"), Some(&Value::from("host")));
        assert_eq!(map.get("appname"), Some(&Value::from("app")));
        assert_eq!(
            map.get("message"),
            Some(&Value::from("RFC3164 missing PRI"))
        );
        assert!(!map.contains_key("severity"));
        assert!(!map.contains_key("facility"));
    }

    #[test]
    fn parses_rfc3164_no_hostname_with_proc_id() {
        let value = parse("<133>Jun 18 04:24:32 haproxy[73411]: Proxy started");
        let map = value.as_object().unwrap();

        assert_eq!(map.get("severity"), Some(&Value::from("notice")));
        assert_eq!(map.get("facility"), Some(&Value::from("local0")));
        assert_eq!(map.get("appname"), Some(&Value::from("haproxy")));
        assert_eq!(map.get("procid"), Some(&Value::from(73411)));
        assert_eq!(map.get("message"), Some(&Value::from("Proxy started")));
        assert!(!map.contains_key("hostname"));
    }

    #[test]
    fn parses_rfc3164_structured_data() {
        let value = parse(
            r#"<46>Jun 18 04:24:32 host rsyslogd: [origin software="rsyslogd" swVersion="8.32.0"] start"#,
        );
        let map = value.as_object().unwrap();

        assert_eq!(map.get("message"), Some(&Value::from("start")));
        let structured_data = map.get("origin").unwrap().as_object().unwrap();
        assert_eq!(
            structured_data.get("software"),
            Some(&Value::from("rsyslogd"))
        );
        assert_eq!(
            structured_data.get("swVersion"),
            Some(&Value::from("8.32.0"))
        );
    }

    #[test]
    fn parses_rfc3164_with_year() {
        let value = parse("<34>Jun 18 2026 04:24:32 host app: RFC3164 message");
        let map = value.as_object().unwrap();

        assert_eq!(
            map.get("timestamp"),
            Some(&Value::from(
                Utc.with_ymd_and_hms(2026, 6, 18, 4, 24, 32).unwrap()
            ))
        );
        assert_eq!(map.get("hostname"), Some(&Value::from("host")));
        assert_eq!(map.get("appname"), Some(&Value::from("app")));
        assert_eq!(map.get("message"), Some(&Value::from("RFC3164 message")));
    }

    #[test]
    fn parses_missing_pri_rfc5424() {
        let value =
            parse("1 2020-05-22T14:59:09.250-03:00 router app 6589 - - Missing PRI RFC5424");
        let map = value.as_object().unwrap();

        assert_eq!(
            map.get("timestamp"),
            Some(&Value::from(
                Utc.with_ymd_and_hms(2020, 5, 22, 17, 59, 9)
                    .unwrap()
                    .with_nanosecond(250_000_000)
                    .unwrap()
            ))
        );
        assert_eq!(map.get("hostname"), Some(&Value::from("router")));
        assert_eq!(map.get("appname"), Some(&Value::from("app")));
        assert_eq!(map.get("procid"), Some(&Value::from(6589)));
        assert_eq!(
            map.get("message"),
            Some(&Value::from("Missing PRI RFC5424"))
        );
        assert_eq!(map.get("version"), Some(&Value::from(1)));
        assert!(!map.contains_key("severity"));
        assert!(!map.contains_key("facility"));
    }

    #[test]
    fn parses_year_first_rfc3164() {
        let value = parse(
            "<130>2026 Jun 18 04:24:32 zte-device command-log:An alarm 35125 level minor occurred",
        );
        let map = value.as_object().unwrap();

        assert_eq!(map.get("severity"), Some(&Value::from("crit")));
        assert_eq!(map.get("facility"), Some(&Value::from("local0")));
        assert_eq!(
            map.get("timestamp"),
            Some(&Value::from(
                Utc.with_ymd_and_hms(2026, 6, 18, 4, 24, 32).unwrap()
            ))
        );
        assert_eq!(map.get("hostname"), Some(&Value::from("zte-device")));
        assert_eq!(map.get("appname"), Some(&Value::from("command-log")));
        assert_eq!(
            map.get("message"),
            Some(&Value::from("An alarm 35125 level minor occurred"))
        );
    }

    #[test]
    fn parses_dash_comma_timestamp() {
        let value = parse(
            "<190>2025-12-21,23:06:36  device-redacted: SSH-SERVER-6-CLOSE_SESSION:Scrn pty want close Session id = 0",
        );
        let map = value.as_object().unwrap();

        assert_eq!(map.get("severity"), Some(&Value::from("info")));
        assert_eq!(map.get("facility"), Some(&Value::from("local7")));
        assert_eq!(
            map.get("timestamp"),
            Some(&Value::from(
                Utc.with_ymd_and_hms(2025, 12, 21, 23, 6, 36).unwrap()
            ))
        );
        assert_eq!(map.get("hostname"), Some(&Value::from("device-redacted")));
        assert!(!map.contains_key("appname"));
        assert_eq!(
            map.get("message"),
            Some(&Value::from(
                "SSH-SERVER-6-CLOSE_SESSION:Scrn pty want close Session id = 0"
            ))
        );
    }

    #[test]
    fn parses_pri_only_network_message() {
        let value = parse("<174>%LINK-I-Up:  1/e12");
        let map = value.as_object().unwrap();

        assert_eq!(map.get("severity"), Some(&Value::from("info")));
        assert_eq!(map.get("facility"), Some(&Value::from("local5")));
        assert_eq!(map.get("appname"), Some(&Value::from("%LINK-I-Up")));
        assert_eq!(map.get("message"), Some(&Value::from("1/e12")));
        assert!(!map.contains_key("timestamp"));
        assert!(!map.contains_key("hostname"));
    }

    #[test]
    fn parses_network_vendor_samples() {
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
            let value = parse(case.input);
            let map = value.as_object().unwrap();

            match case.hostname {
                Some(hostname) => {
                    assert_eq!(
                        map.get("hostname"),
                        Some(&Value::from(hostname)),
                        "{} hostname",
                        case.name
                    );
                }
                None => assert!(
                    !map.contains_key("hostname"),
                    "{} unexpected hostname",
                    case.name
                ),
            }

            match case.appname {
                Some(appname) => {
                    assert_eq!(
                        map.get("appname"),
                        Some(&Value::from(appname)),
                        "{} appname",
                        case.name
                    );
                }
                None => assert!(
                    !map.contains_key("appname"),
                    "{} unexpected appname",
                    case.name
                ),
            }

            match case.procid {
                Some(procid) => {
                    assert_eq!(
                        map.get("procid"),
                        Some(&Value::from(procid)),
                        "{} procid",
                        case.name
                    );
                }
                None => assert!(
                    !map.contains_key("procid"),
                    "{} unexpected procid",
                    case.name
                ),
            }

            assert_eq!(
                map.get("message"),
                Some(&Value::from(case.message)),
                "{} message",
                case.name
            );
        }
    }

    #[test]
    fn rejects_invalid_syslog() {
        let error = parse_syslog_to_value("not much of a syslog message", Some(Utc))
            .unwrap_err()
            .to_string();

        assert_eq!(error, "unable to parse input as valid syslog message");
    }

    #[test]
    fn normalize_year_first_timestamp_keeps_non_matching_input_unchanged() {
        assert_eq!(
            normalize_year_first_timestamp("<130>2026 Jun 18 04:24:32 host app: msg").as_deref(),
            Some("<130>Jun 18 2026 04:24:32 host app: msg")
        );
        assert_eq!(
            normalize_dash_comma_timestamp("<190>2025-12-21,23:06:36  host: msg").as_deref(),
            Some("<190>Dec 21 2025 23:06:36 host: msg")
        );
        assert!(normalize_year_first_timestamp("<130>Jun 18 04:24:32 host app: msg").is_none());
        assert!(
            normalize_year_first_timestamp("<130>1 2026-06-18T04:24:32Z host app - - msg")
                .is_none()
        );
        assert_eq!(
            normalize_syslog_frame(" \0<190>Feb 29 23:53:33 host app: msg \0 ").as_ref(),
            "<190>Feb 29 23:53:33 host app: msg"
        );
        assert_eq!(
            normalize_syslog_frame(" \u{0000}<190>Feb 29 23:53:33 host app: msg \u{0000} ")
                .as_ref(),
            "<190>Feb 29 23:53:33 host app: msg"
        );
        assert_eq!(
            normalize_syslog_frame("<190>Jun 18 04:24:32 host app: before\r\nmiddle\n after")
                .as_ref(),
            "<190>Jun 18 04:24:32 host app: before middle  after"
        );
    }
}
