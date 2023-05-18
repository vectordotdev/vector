use bytes::Bytes;
use chrono::{DateTime, Datelike, Utc};
use lookup::lookup_v2::parse_value_path;
use lookup::{event_path, owned_value_path, OwnedTargetPath, OwnedValuePath, PathPrefix};
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use std::collections::BTreeMap;
use syslog_loose::{IncompleteDate, Message, ProcId, Protocol};
use vector_core::config::{LegacyKey, LogNamespace};
use vector_core::{
    config::{log_schema, DataType},
    event::{Event, LogEvent, Value},
    schema,
};
use vrl::value::{kind::Collection, Kind};

use super::Deserializer;

/// Config used to build a `SyslogDeserializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SyslogDeserializerConfig {
    source: Option<&'static str>,
}

impl SyslogDeserializerConfig {
    /// Create the `SyslogDeserializer` from the given source name.
    pub fn from_source(source: &'static str) -> Self {
        Self {
            source: Some(source),
        }
    }

    /// Build the `SyslogDeserializer` from this configuration.
    pub const fn build(&self) -> SyslogDeserializer {
        SyslogDeserializer { source: None }
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
                        &parse_value_path(log_schema().message_key()).expect("valid message key"),
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
                    .optional_field(&owned_value_path!("appname"), Kind::bytes(), None)
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
                .optional_field(&owned_value_path!("appname"), Kind::bytes(), None)
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
                        None,
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

/// Deserializer that builds an `Event` from a byte frame containing a syslog
/// message.
#[derive(Debug, Clone)]
pub struct SyslogDeserializer {
    /// The syslog source needs it's own syslog deserializer separate from the
    /// syslog codec since it needs to handle the structured of the decoded data
    /// differently when using the Vector lognamespace.
    pub source: Option<&'static str>,
}

impl Deserializer for SyslogDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        let line = std::str::from_utf8(&bytes)?;
        let line = line.trim();
        let parsed = syslog_loose::parse_message_with_year_exact(line, resolve_year)?;

        let log = match (self.source, log_namespace) {
            (Some(source), LogNamespace::Vector) => {
                let mut log = LogEvent::from(Value::Bytes(Bytes::from(parsed.msg.to_string())));
                insert_metadata_fields_from_syslog(&mut log, source, parsed, log_namespace);
                log
            }
            _ => {
                let mut log = LogEvent::from(Value::Object(BTreeMap::new()));
                insert_fields_from_syslog(&mut log, parsed, log_namespace);
                log
            }
        };

        Ok(smallvec![Event::from(log)])
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

    let mut sdata: BTreeMap<String, Value> = BTreeMap::new();
    for element in parsed.structured_data.into_iter() {
        let mut data: BTreeMap<String, Value> = BTreeMap::new();

        for (name, value) in element.params() {
            data.insert(name.to_string(), value.into());
        }

        sdata.insert(element.id.to_string(), data.into());
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
            log.insert(event_path!(log_schema().message_key()), parsed.msg);
        }
        LogNamespace::Vector => {
            log.insert(event_path!("message"), parsed.msg);
        }
    }

    if let Some(timestamp) = parsed.timestamp {
        let timestamp = DateTime::<Utc>::from(timestamp);
        match log_namespace {
            LogNamespace::Legacy => {
                if let Some(timestamp_key) = log_schema().timestamp_key() {
                    log.insert((PathPrefix::Event, timestamp_key), timestamp);
                }
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
        let mut sdata: BTreeMap<String, Value> = BTreeMap::new();
        for (name, value) in element.params() {
            sdata.insert(name.to_string(), value.into());
        }
        log.insert(event_path!(element.id), sdata);
    }
}

#[cfg(test)]
mod tests {
    use vector_core::config::{init_log_schema, log_schema, LogSchema};

    use super::*;

    #[test]
    fn deserialize_syslog_legacy_namespace() {
        init();

        let input =
            Bytes::from("<34>1 2003-10-11T22:14:15.003Z mymachine.example.com su - ID47 - MSG");
        let deserializer = SyslogDeserializer { source: None };

        let events = deserializer.parse(input, LogNamespace::Legacy).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].as_log()[log_schema().message_key()], "MSG".into());
        assert!(
            events[0].as_log()[log_schema().timestamp_key().unwrap().to_string()].is_timestamp()
        );
    }

    #[test]
    fn deserialize_syslog_vector_namespace() {
        init();

        let input =
            Bytes::from("<34>1 2003-10-11T22:14:15.003Z mymachine.example.com su - ID47 - MSG");
        let deserializer = SyslogDeserializer { source: None };

        let events = deserializer.parse(input, LogNamespace::Vector).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].as_log()["message"], "MSG".into());
        assert!(events[0].as_log()["timestamp"].is_timestamp());
    }

    fn init() {
        let mut schema = LogSchema::default();
        schema.set_message_key("legacy_message".to_string());
        schema.set_message_key("legacy_timestamp".to_string());
        init_log_schema(schema, false);
    }
}
