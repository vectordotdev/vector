use crate::{
    codecs::{BoxedParser, Parser, ParserConfig},
    config::log_schema,
    event::{Event, Value},
    internal_events::SyslogConvertUtf8Error,
};
use bytes::Bytes;
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use syslog_loose::{IncompleteDate, Message, ProcId, Protocol};

/// Config used to build a `SyslogParser`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SyslogParserConfig;

#[typetag::serde(name = "syslog")]
impl ParserConfig for SyslogParserConfig {
    fn build(&self) -> crate::Result<BoxedParser> {
        Ok(Box::new(SyslogParser))
    }
}

/// Parser that builds an `Event` from a byte frame containing a syslog message.
#[derive(Debug, Clone)]
pub struct SyslogParser;

impl Parser for SyslogParser {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>> {
        let line = std::str::from_utf8(&bytes).map_err(|error| {
            emit!(SyslogConvertUtf8Error { error });
            error
        })?;
        let line = line.trim();
        let parsed = syslog_loose::parse_message_with_year(line, resolve_year);
        let mut event = Event::from(parsed.msg);

        insert_fields_from_syslog(&mut event, parsed);

        Ok(smallvec![event])
    }
}

/// Function used to resolve the year for syslog messages that don't include the
/// year.
///
/// If the current month is January, and the syslog message is for December, it
/// will take the previous year.
///
/// Otherwise, take the current year.
fn resolve_year((month, _date, _hour, _min, _sec): IncompleteDate) -> i32 {
    let now = Utc::now();
    if now.month() == 1 && month == 12 {
        now.year() - 1
    } else {
        now.year()
    }
}

fn insert_fields_from_syslog(event: &mut Event, parsed: Message<&str>) {
    let log = event.as_mut_log();

    if let Some(timestamp) = parsed.timestamp {
        log.insert(
            log_schema().timestamp_key(),
            DateTime::<Utc>::from(timestamp),
        );
    }
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
