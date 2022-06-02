use bytes::Bytes;
use chrono::{DateTime, Datelike, Utc};
use lookup::lookup_v2::Path;
use lookup::path;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use syslog_loose::{IncompleteDate, Message, ProcId, Protocol};
use value::kind::Collection;
use value::Kind;
use vector_core::config::LogNamespace;
use vector_core::{
    config::{log_schema, DataType},
    event::{Event, Value},
    schema,
};

use super::Deserializer;

/// Config used to build a `SyslogDeserializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SyslogDeserializerConfig;

impl SyslogDeserializerConfig {
    /// Build the `SyslogDeserializer` from this configuration.
    pub const fn build(&self) -> SyslogDeserializer {
        SyslogDeserializer
    }

    /// Return the type of event build by this deserializer.
    pub fn output_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match log_namespace {
            LogNamespace::Legacy => {
                schema::Definition::empty()
                    // The `message` field is always defined. If parsing fails, the entire body becomes the
                    // message.
                    .required_field(log_schema().message_key(), Kind::bytes(), Some("message"))
                    // All other fields are optional.
                    .optional_field(
                        log_schema().timestamp_key(),
                        Kind::timestamp(),
                        Some("timestamp"),
                    )
                    .optional_field("hostname", Kind::bytes(), None)
                    .optional_field("severity", Kind::bytes(), Some("severity"))
                    .optional_field("facility", Kind::bytes(), None)
                    .optional_field("version", Kind::integer(), None)
                    .optional_field("appname", Kind::bytes(), None)
                    .optional_field("msgid", Kind::bytes(), None)
                    .optional_field("procid", Kind::integer().or_bytes(), None)
                    // "structured data" in a syslog message can be stored in any field, but will always be
                    // a string.
                    .unknown_fields(Kind::bytes())
            }
            LogNamespace::Vector => schema::Definition::empty()
                .required_field("message", Kind::bytes(), Some("message"))
                .optional_field("timestamp", Kind::timestamp(), Some("timestamp"))
                .optional_field("hostname", Kind::bytes(), None)
                .optional_field("severity", Kind::bytes(), Some("severity"))
                .optional_field("facility", Kind::bytes(), None)
                .optional_field("version", Kind::integer(), None)
                .optional_field("appname", Kind::bytes(), None)
                .optional_field("msgid", Kind::bytes(), None)
                .optional_field("procid", Kind::integer().or_bytes(), None)
                .optional_field(
                    "structured_data",
                    Kind::object(Collection::from_unknown(Kind::bytes())),
                    None,
                ),
        }
    }
}

/// Deserializer that builds an `Event` from a byte frame containing a syslog
/// message.
#[derive(Debug, Clone)]
pub struct SyslogDeserializer;

impl Deserializer for SyslogDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_core::Result<SmallVec<[Event; 1]>> {
        let line = std::str::from_utf8(&bytes)?;
        let line = line.trim();
        let parsed = syslog_loose::parse_message_with_year_exact(line, resolve_year)?;
        let mut event = Event::from(parsed.msg);

        insert_fields_from_syslog(&mut event, parsed, log_namespace);

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

fn insert_fields_from_syslog(
    event: &mut Event,
    parsed: Message<&str>,
    log_namespace: LogNamespace,
) {
    let log = event.as_mut_log();

    if let Some(timestamp) = parsed.timestamp {
        let timestamp = DateTime::<Utc>::from(timestamp);
        match log_namespace {
            LogNamespace::Legacy => {
                log.insert(path!(log_schema().timestamp_key()), timestamp);
            }
            LogNamespace::Vector => {
                log.insert(path!("timestamp"), timestamp);
            }
        };
    }
    if let Some(host) = parsed.hostname {
        log.insert(path!("hostname"), host.to_string());
    }
    if let Some(severity) = parsed.severity {
        log.insert(path!("severity"), severity.as_str().to_owned());
    }
    if let Some(facility) = parsed.facility {
        log.insert(path!("facility"), facility.as_str().to_owned());
    }
    if let Protocol::RFC5424(version) = parsed.protocol {
        log.insert(path!("version"), version as i64);
    }
    if let Some(app_name) = parsed.appname {
        log.insert(path!("appname"), app_name.to_owned());
    }
    if let Some(msg_id) = parsed.msgid {
        log.insert(path!("msgid"), msg_id.to_owned());
    }
    if let Some(procid) = parsed.procid {
        let value: Value = match procid {
            ProcId::PID(pid) => pid.into(),
            ProcId::Name(name) => name.to_string().into(),
        };
        log.insert(path!("procid"), value);
    }

    for element in parsed.structured_data.into_iter() {
        for (name, value) in element.params() {
            let element_id_path = path!(element.id);
            let name_path = path!(*name);
            let path = element_id_path.concat(name_path);
            log.insert(path, value);
        }
    }
}
