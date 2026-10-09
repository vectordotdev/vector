use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use vector_core::{
    config::{LegacyKey, LogNamespace, log_schema},
    event::{Event, LogEvent},
};
use vrl::{
    core::Value,
    path,
    path::{PathPrefix, ValuePath},
    value::ObjectMap,
};

use super::common::{kv_list_into_value, object_into_kv_list, to_hex};
use crate::proto::{
    collector::logs::v1::ExportLogsServiceRequest,
    common::v1::{AnyValue, InstrumentationScope, KeyValue, any_value::Value as PBValue},
    logs::v1::{LogRecord, ResourceLogs, ScopeLogs, SeverityNumber},
    resource::v1::Resource,
};

const SOURCE_NAME: &str = "opentelemetry";
pub const RESOURCE_KEY: &str = "resources";
pub const ATTRIBUTES_KEY: &str = "attributes";
pub const SCOPE_KEY: &str = "scope";
pub const NAME_KEY: &str = "name";
pub const VERSION_KEY: &str = "version";
pub const TRACE_ID_KEY: &str = "trace_id";
pub const SPAN_ID_KEY: &str = "span_id";
pub const SEVERITY_TEXT_KEY: &str = "severity_text";
pub const SEVERITY_NUMBER_KEY: &str = "severity_number";
pub const OBSERVED_TIMESTAMP_KEY: &str = "observed_timestamp";
pub const DROPPED_ATTRIBUTES_COUNT_KEY: &str = "dropped_attributes_count";
pub const FLAGS_KEY: &str = "flags";
pub const TIMESTAMP_KEY: &str = "timestamp";

impl ResourceLogs {
    pub fn into_event_iter(self, log_namespace: LogNamespace) -> impl Iterator<Item = Event> {
        let now = Utc::now();

        self.scope_logs.into_iter().flat_map(move |scope_log| {
            let scope = scope_log.scope;
            let resource = self.resource.clone();
            scope_log.log_records.into_iter().map(move |log_record| {
                ResourceLog {
                    resource: resource.clone(),
                    scope: scope.clone(),
                    log_record,
                }
                .into_event(log_namespace, now)
            })
        })
    }
}

struct ResourceLog {
    resource: Option<Resource>,
    scope: Option<InstrumentationScope>,
    log_record: LogRecord,
}

// https://github.com/open-telemetry/opentelemetry-specification/blob/v1.15.0/specification/logs/data-model.md
impl ResourceLog {
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::too_many_lines,
        reason = "keep the existing protocol field mapping together; splitting is deferred"
    )]
    fn into_event(self, log_namespace: LogNamespace, now: DateTime<Utc>) -> Event {
        let mut log = match log_namespace {
            LogNamespace::Vector => {
                if let Some(v) = self.log_record.body.and_then(|av| av.value) {
                    LogEvent::from(<PBValue as Into<Value>>::into(v))
                } else {
                    LogEvent::from(Value::Null)
                }
            }
            LogNamespace::Legacy => {
                let mut log = LogEvent::default();
                if let Some(v) = self.log_record.body.and_then(|av| av.value) {
                    log.maybe_insert(log_schema().message_key_target_path(), v);
                }
                log
            }
        };

        // Insert instrumentation scope (scope name, version, and attributes)
        if let Some(scope) = self.scope {
            if !scope.name.is_empty() {
                log_namespace.insert_source_metadata(
                    SOURCE_NAME,
                    &mut log,
                    Some(LegacyKey::Overwrite(path!(SCOPE_KEY, NAME_KEY))),
                    path!(SCOPE_KEY, NAME_KEY),
                    scope.name,
                );
            }
            if !scope.version.is_empty() {
                log_namespace.insert_source_metadata(
                    SOURCE_NAME,
                    &mut log,
                    Some(LegacyKey::Overwrite(path!(SCOPE_KEY, VERSION_KEY))),
                    path!(SCOPE_KEY, VERSION_KEY),
                    scope.version,
                );
            }
            if !scope.attributes.is_empty() {
                log_namespace.insert_source_metadata(
                    SOURCE_NAME,
                    &mut log,
                    Some(LegacyKey::Overwrite(path!(SCOPE_KEY, ATTRIBUTES_KEY))),
                    path!(SCOPE_KEY, ATTRIBUTES_KEY),
                    kv_list_into_value(scope.attributes),
                );
            }
            if scope.dropped_attributes_count > 0 {
                log_namespace.insert_source_metadata(
                    SOURCE_NAME,
                    &mut log,
                    Some(LegacyKey::Overwrite(path!(
                        SCOPE_KEY,
                        DROPPED_ATTRIBUTES_COUNT_KEY
                    ))),
                    path!(SCOPE_KEY, DROPPED_ATTRIBUTES_COUNT_KEY),
                    scope.dropped_attributes_count,
                );
            }
        }

        // Optional fields
        if let Some(resource) = self.resource
            && !resource.attributes.is_empty()
        {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(RESOURCE_KEY))),
                path!(RESOURCE_KEY),
                kv_list_into_value(resource.attributes),
            );
        }
        if !self.log_record.attributes.is_empty() {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(ATTRIBUTES_KEY))),
                path!(ATTRIBUTES_KEY),
                kv_list_into_value(self.log_record.attributes),
            );
        }
        if !self.log_record.trace_id.is_empty() {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(TRACE_ID_KEY))),
                path!(TRACE_ID_KEY),
                Bytes::from(to_hex(&self.log_record.trace_id)),
            );
        }
        if !self.log_record.span_id.is_empty() {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(SPAN_ID_KEY))),
                path!(SPAN_ID_KEY),
                Bytes::from(to_hex(&self.log_record.span_id)),
            );
        }
        if !self.log_record.severity_text.is_empty() {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(SEVERITY_TEXT_KEY))),
                path!(SEVERITY_TEXT_KEY),
                self.log_record.severity_text,
            );
        }
        if self.log_record.severity_number != SeverityNumber::Unspecified as i32 {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(SEVERITY_NUMBER_KEY))),
                path!(SEVERITY_NUMBER_KEY),
                self.log_record.severity_number,
            );
        }
        if self.log_record.flags > 0 {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(FLAGS_KEY))),
                path!(FLAGS_KEY),
                self.log_record.flags,
            );
        }

        log_namespace.insert_source_metadata(
            SOURCE_NAME,
            &mut log,
            Some(LegacyKey::Overwrite(path!(DROPPED_ATTRIBUTES_COUNT_KEY))),
            path!(DROPPED_ATTRIBUTES_COUNT_KEY),
            self.log_record.dropped_attributes_count,
        );

        // According to log data model spec, if observed_time_unix_nano is missing, the collector
        // should set it to the current time.
        // https://github.com/vectordotdev/vector/issues/23659
        #[allow(
            clippy::cast_possible_wrap,
            reason = "preserve existing unsigned OTLP timestamp conversion; out-of-range handling is deferred"
        )]
        let observed_timestamp = if self.log_record.observed_time_unix_nano > 0 {
            Utc.timestamp_nanos(self.log_record.observed_time_unix_nano as i64)
                .into()
        } else {
            Value::Timestamp(now)
        };
        log_namespace.insert_source_metadata(
            SOURCE_NAME,
            &mut log,
            Some(LegacyKey::Overwrite(path!(OBSERVED_TIMESTAMP_KEY))),
            path!(OBSERVED_TIMESTAMP_KEY),
            observed_timestamp.clone(),
        );

        // If time_unix_nano is not present (0 represents missing or unknown timestamp) use observed time
        // https://github.com/vectordotdev/vector/issues/23659
        #[allow(
            clippy::cast_possible_wrap,
            reason = "preserve existing unsigned OTLP timestamp conversion; out-of-range handling is deferred"
        )]
        let timestamp = if self.log_record.time_unix_nano > 0 {
            Utc.timestamp_nanos(self.log_record.time_unix_nano as i64)
                .into()
        } else {
            observed_timestamp
        };
        log_namespace.insert_source_metadata(
            SOURCE_NAME,
            &mut log,
            log_schema().timestamp_key().map(LegacyKey::Overwrite),
            path!(TIMESTAMP_KEY),
            timestamp,
        );

        log_namespace.insert_vector_metadata(
            &mut log,
            log_schema().source_type_key(),
            path!("source_type"),
            Bytes::from_static(SOURCE_NAME.as_bytes()),
        );
        if log_namespace == LogNamespace::Vector {
            log.metadata_mut()
                .value_mut()
                .insert(path!("vector", "ingest_timestamp"), now);
        }

        log.into()
    }
}

/// Convert a native Vector log event into an OTLP export request.
///
/// This is the inverse of the `opentelemetry` source decoding, so a log record decoded by
/// that source encodes back to an equivalent OTLP log record, with these exceptions:
///
/// - The source does not keep the resource and scope `schema_url` or the resource
///   `dropped_attributes_count`, so these fields are empty.
/// - A `bytes_value` becomes a `string_value`, because the source decodes both to the same
///   Vector value. Bytes that are not valid UTF-8 are replaced with U+FFFD.
/// - A record without `time_unix_nano` gets the observed time, because the source uses the
///   observed time as the event timestamp.
/// - The request has one `ResourceLogs` and one `ScopeLogs` for the one record, so records
///   that shared a resource and scope in the original request are not grouped again.
///
/// - Legacy namespace: the message field becomes the body and the timestamp field becomes
///   `time_unix_nano`. The fields written by the source (`attributes`, `resources`, `scope`,
///   `trace_id`, `span_id`, `severity_text`, `severity_number`, `flags`,
///   `dropped_attributes_count`, and `observed_timestamp`) fill the matching OTLP fields.
/// - Vector namespace: the event value becomes the body. The same fields are read from the
///   `opentelemetry` source metadata. When they are missing, the timestamp falls back to the
///   field with the `timestamp` meaning and the observed timestamp to the Vector ingest
///   timestamp.
///
/// A field that does not have the type OTLP requires (for example a `trace_id` that is not
/// 32 hex characters) is not dropped: it is sent as a log record attribute. All other event
/// fields are also sent as log record attributes; when a key is in both, the value from
/// `attributes` is used. Vector's source type marker is not sent. In the Legacy namespace,
/// a marker at the configured metadata path takes precedence over the matching event field.
/// If that metadata is absent, the event field is treated as the marker for sources that
/// write it there.
#[must_use]
pub fn log_event_to_export_request(mut log: LogEvent) -> ExportLogsServiceRequest {
    let mut record = LogRecord::default();

    let mut fields = match log.namespace() {
        LogNamespace::Vector => {
            let meaning_time = log
                .get_timestamp()
                .and_then(Value::as_timestamp)
                .and_then(timestamp_nanos);
            let (body, mut metadata) = log.into_parts();
            record.body = into_body(body);
            let ingest_time = metadata
                .value()
                .get(path!("vector", "ingest_timestamp"))
                .and_then(Value::as_timestamp)
                .and_then(timestamp_nanos);
            let mut fields = match metadata.value_mut().remove(path!(SOURCE_NAME), false) {
                Some(fields @ Value::Object(_)) => fields,
                _ => Value::Object(ObjectMap::new()),
            };
            record.time_unix_nano = take(&mut fields, path!(TIMESTAMP_KEY), into_timestamp_nanos)
                .or(meaning_time)
                .unwrap_or_default();
            record.observed_time_unix_nano = take(
                &mut fields,
                path!(OBSERVED_TIMESTAMP_KEY),
                into_timestamp_nanos,
            )
            .or(ingest_time)
            .unwrap_or_default();
            fields
        }
        LogNamespace::Legacy => {
            let schema = log_schema();
            // A marker at the configured metadata path leaves the event field as user data.
            // Otherwise, remove the root marker for Legacy sources that ignore the prefix.
            let source_type_key = schema
                .source_type_key_target_path()
                .filter(|path| path.prefix == PathPrefix::Event || !log.contains(*path));
            // Read timestamps before removing a message that may contain them. The
            // opentelemetry source ignores the timestamp key's metadata prefix, so
            // prefer metadata but retain the event-root fallback.
            let metadata_time = schema
                .timestamp_key_target_path()
                .filter(|path| path.prefix == PathPrefix::Metadata)
                .and_then(|path| log.get(path))
                .and_then(Value::as_timestamp)
                .and_then(timestamp_nanos);
            let event_time = schema
                .timestamp_key()
                .filter(|_| metadata_time.is_none())
                .and_then(|path| log.get((PathPrefix::Event, path)))
                .and_then(Value::as_timestamp)
                .and_then(timestamp_nanos);
            // The message key can point into metadata (for example `%message`), so remove it
            // with its full target path before the metadata is discarded.
            if let Some(path) = schema.message_key_target_path() {
                record.body = log.remove_prune(path, true).and_then(into_body);
            }
            let (mut fields, _) = log.into_parts();
            if let Some(path) = source_type_key {
                fields.remove(&path.path, true);
            }
            record.time_unix_nano = metadata_time
                .or_else(|| {
                    let extracted_time = schema
                        .timestamp_key()
                        .and_then(|path| take(&mut fields, path, into_timestamp_nanos));
                    event_time.or(extracted_time)
                })
                .unwrap_or_default();
            record.observed_time_unix_nano = take(
                &mut fields,
                path!(OBSERVED_TIMESTAMP_KEY),
                into_timestamp_nanos,
            )
            .unwrap_or_default();
            fields
        }
    };

    let (resource, scope) = take_record_fields(&mut fields, &mut record);
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(resource),
            scope_logs: vec![ScopeLogs {
                scope: Some(scope),
                log_records: vec![record],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

/// Move the namespace-independent OTLP fields from `fields` into `record` and return the
/// resource and scope. All fields that remain become log record attributes.
fn take_record_fields(
    fields: &mut Value,
    record: &mut LogRecord,
) -> (Resource, InstrumentationScope) {
    record.severity_text = take(fields, path!(SEVERITY_TEXT_KEY), into_string).unwrap_or_default();
    record.severity_number =
        take(fields, path!(SEVERITY_NUMBER_KEY), into_severity_number).unwrap_or_default();
    record.flags = take(fields, path!(FLAGS_KEY), into_u32).unwrap_or_default();
    record.dropped_attributes_count =
        take(fields, path!(DROPPED_ATTRIBUTES_COUNT_KEY), into_u32).unwrap_or_default();
    record.trace_id = take(fields, path!(TRACE_ID_KEY), into_id::<16>).unwrap_or_default();
    record.span_id = take(fields, path!(SPAN_ID_KEY), into_id::<8>).unwrap_or_default();
    record.attributes = take(fields, path!(ATTRIBUTES_KEY), into_kv_list).unwrap_or_default();

    let resource = Resource {
        attributes: take(fields, path!(RESOURCE_KEY), into_kv_list).unwrap_or_default(),
        ..Default::default()
    };
    let scope = InstrumentationScope {
        name: take(fields, path!(SCOPE_KEY, NAME_KEY), into_string).unwrap_or_default(),
        version: take(fields, path!(SCOPE_KEY, VERSION_KEY), into_string).unwrap_or_default(),
        attributes: take(fields, path!(SCOPE_KEY, ATTRIBUTES_KEY), into_kv_list)
            .unwrap_or_default(),
        dropped_attributes_count: take(
            fields,
            path!(SCOPE_KEY, DROPPED_ATTRIBUTES_COUNT_KEY),
            into_u32,
        )
        .unwrap_or_default(),
    };

    match std::mem::replace(fields, Value::Null) {
        Value::Object(rest) => {
            for (key, value) in rest {
                if !record.attributes.iter().any(|kv| kv.key == key.as_str()) {
                    record.attributes.push(KeyValue {
                        key: key.into(),
                        value: Some(value.into()),
                    });
                }
            }
        }
        // A Legacy event whose root is not an object has no message field; the root is the body.
        other if record.body.is_none() => record.body = into_body(other),
        _ => {}
    }

    (resource, scope)
}

/// Remove the value at `path` and convert it. If the conversion fails, the value is put back
/// so that it is sent as an attribute instead of being lost.
fn take<'a, T>(
    fields: &mut Value,
    path: impl ValuePath<'a> + Copy,
    convert: fn(Value) -> Result<T, Value>,
) -> Option<T> {
    let value = fields.remove(path, true)?;
    match convert(value) {
        Ok(converted) => Some(converted),
        Err(value) => {
            fields.insert(path, value);
            None
        }
    }
}

fn into_body(value: Value) -> Option<AnyValue> {
    (!value.is_null()).then(|| value.into())
}

fn timestamp_nanos(timestamp: &DateTime<Utc>) -> Option<u64> {
    timestamp
        .timestamp_nanos_opt()
        .and_then(|nanos| u64::try_from(nanos).ok())
}

fn into_timestamp_nanos(value: Value) -> Result<u64, Value> {
    match value {
        Value::Timestamp(timestamp) => {
            timestamp_nanos(&timestamp).ok_or(Value::Timestamp(timestamp))
        }
        other => Err(other),
    }
}

/// The raw bytes of a string value, or the value itself if it is not a string.
fn into_string_bytes(value: Value) -> Result<Bytes, Value> {
    match value {
        Value::Bytes(bytes) => Ok(bytes),
        Value::String(string) => Ok(string.into_bytes()),
        other => Err(other),
    }
}

fn into_string(value: Value) -> Result<String, Value> {
    String::from_utf8(Vec::from(into_string_bytes(value)?))
        .map_err(|error| Value::Bytes(error.into_bytes().into()))
}

fn into_u32(value: Value) -> Result<u32, Value> {
    match value {
        Value::Integer(int) => u32::try_from(int).map_err(|_| Value::Integer(int)),
        other => Err(other),
    }
}

fn into_severity_number(value: Value) -> Result<i32, Value> {
    match value {
        Value::Integer(int) => i32::try_from(int)
            .ok()
            .filter(|number| SeverityNumber::try_from(*number).is_ok())
            .ok_or(Value::Integer(int)),
        other => Err(other),
    }
}

/// Decode a hex ID of `LEN` bytes, as written by the source. An empty string means "no ID".
fn into_id<const LEN: usize>(value: Value) -> Result<Vec<u8>, Value> {
    let hex = into_string_bytes(value)?;
    match hex::decode(&hex) {
        Ok(id) if id.is_empty() || id.len() == LEN => Ok(id),
        _ => Err(Value::Bytes(hex)),
    }
}

fn into_kv_list(value: Value) -> Result<Vec<KeyValue>, Value> {
    match value {
        Value::Object(object) => Ok(object_into_kv_list(object)),
        other => Err(other),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use vector_core::config::{LogSchema, init_log_schema};
    use vrl::{event_path, owned_value_path, path::OwnedTargetPath};

    use super::*;
    use crate::proto::common::v1::{ArrayValue, KeyValueList};

    fn kv(key: &str, value: PBValue) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue { value: Some(value) }),
        }
    }

    fn string(value: &str) -> PBValue {
        PBValue::StringValue(value.to_owned())
    }

    fn request(
        resource: Resource,
        scope: InstrumentationScope,
        record: LogRecord,
    ) -> ExportLogsServiceRequest {
        ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: Some(resource),
                scope_logs: vec![ScopeLogs {
                    scope: Some(scope),
                    log_records: vec![record],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        }
    }

    /// Every field the source decodes, with attribute keys in sorted order because the
    /// decoded event stores attributes in a sorted map.
    fn full_request() -> ExportLogsServiceRequest {
        request(
            Resource {
                attributes: vec![kv("service.name", string("api"))],
                ..Default::default()
            },
            InstrumentationScope {
                name: "lib".to_owned(),
                version: "1.2.3".to_owned(),
                attributes: vec![kv("scope.attr", PBValue::BoolValue(true))],
                dropped_attributes_count: 1,
            },
            LogRecord {
                time_unix_nano: 1_700_000_000_000_000_001,
                observed_time_unix_nano: 1_700_000_000_000_000_002,
                severity_number: SeverityNumber::Warn as i32,
                severity_text: "WARN".to_owned(),
                body: Some(AnyValue {
                    value: Some(string("hello")),
                }),
                attributes: vec![
                    kv("count", PBValue::IntValue(3)),
                    kv(
                        "list",
                        PBValue::ArrayValue(ArrayValue {
                            values: vec![AnyValue {
                                value: Some(string("a")),
                            }],
                        }),
                    ),
                    kv(
                        "nested",
                        PBValue::KvlistValue(KeyValueList {
                            values: vec![kv("inner", PBValue::DoubleValue(0.5))],
                        }),
                    ),
                ],
                dropped_attributes_count: 2,
                flags: 1,
                trace_id: (1..=16).collect(),
                span_id: (1..=8).collect(),
            },
        )
    }

    fn round_trip(namespace: LogNamespace) {
        let expected = full_request();
        let mut events: Vec<Event> = expected.resource_logs[0]
            .clone()
            .into_event_iter(namespace)
            .collect();
        assert_eq!(events.len(), 1);
        let log = events.pop().unwrap().into_log();
        assert_eq!(log.namespace(), namespace);
        assert_eq!(log_event_to_export_request(log), expected);
    }

    #[test]
    fn decoded_legacy_log_round_trips() {
        round_trip(LogNamespace::Legacy);
    }

    #[test]
    fn decoded_vector_namespace_log_round_trips() {
        round_trip(LogNamespace::Vector);
    }

    #[test]
    fn decoded_legacy_log_with_metadata_message_key_round_trips() {
        let mut schema = LogSchema::default();
        schema.set_message_key(Some(OwnedTargetPath::metadata(owned_value_path!(
            "message"
        ))));
        init_log_schema(schema, true);

        round_trip(LogNamespace::Legacy);
    }

    #[test]
    fn native_legacy_log_reads_timestamp_inside_event_message() {
        let mut schema = LogSchema::default();
        schema.set_message_key(Some(OwnedTargetPath::event(owned_value_path!("payload"))));
        schema.set_timestamp_key(Some(OwnedTargetPath::event(owned_value_path!(
            "payload",
            "timestamp"
        ))));
        init_log_schema(schema, true);
        let mut log = LogEvent::default();
        log.insert(event_path!("payload", "message"), "hello");
        log.insert(event_path!("payload", "timestamp"), Utc.timestamp_nanos(5));

        let request = log_event_to_export_request(log);
        let record = &request.resource_logs[0].scope_logs[0].log_records[0];
        assert_eq!(record.time_unix_nano, 5);
        assert_eq!(
            record.body,
            Some(AnyValue {
                value: Some(PBValue::KvlistValue(KeyValueList {
                    values: vec![
                        kv("message", string("hello")),
                        kv("timestamp", string("1970-01-01T00:00:00.000000005Z")),
                    ],
                })),
            })
        );
    }

    #[test]
    fn native_legacy_log_reads_timestamp_inside_metadata_message() {
        let mut schema = LogSchema::default();
        schema.set_message_key(Some(OwnedTargetPath::metadata(owned_value_path!(
            "payload"
        ))));
        schema.set_timestamp_key(Some(OwnedTargetPath::metadata(owned_value_path!(
            "payload",
            "timestamp"
        ))));
        init_log_schema(schema, true);
        let mut log = LogEvent::default();
        log.insert((PathPrefix::Metadata, path!("payload", "message")), "hello");
        log.insert(
            log_schema().timestamp_key_target_path().unwrap(),
            Utc.timestamp_nanos(5),
        );
        log.insert(event_path!("payload", "timestamp"), Utc.timestamp_nanos(9));

        let request = log_event_to_export_request(log);
        let record = &request.resource_logs[0].scope_logs[0].log_records[0];
        assert_eq!(record.time_unix_nano, 5);
        assert_eq!(
            record.body,
            Some(AnyValue {
                value: Some(PBValue::KvlistValue(KeyValueList {
                    values: vec![
                        kv("message", string("hello")),
                        kv("timestamp", string("1970-01-01T00:00:00.000000005Z")),
                    ],
                })),
            })
        );
        assert_eq!(
            record.attributes,
            vec![kv(
                "payload",
                PBValue::KvlistValue(KeyValueList {
                    values: vec![kv("timestamp", string("1970-01-01T00:00:00.000000009Z"))],
                }),
            )]
        );
    }

    fn init_metadata_timestamp_key() {
        let mut schema = LogSchema::default();
        schema.set_timestamp_key(Some(OwnedTargetPath::metadata(owned_value_path!(
            "timestamp"
        ))));
        init_log_schema(schema, true);
    }

    #[test]
    fn native_legacy_log_reads_metadata_timestamp_key() {
        init_metadata_timestamp_key();
        let mut log = LogEvent::from("disk full");
        log.insert(
            log_schema().timestamp_key_target_path().unwrap(),
            Utc.timestamp_nanos(1_700_000_000_000_000_000),
        );
        log.insert(event_path!("timestamp"), "root field");

        let request = log_event_to_export_request(log);
        let record = &request.resource_logs[0].scope_logs[0].log_records[0];
        assert_eq!(record.time_unix_nano, 1_700_000_000_000_000_000);
        // With a metadata timestamp key, a `timestamp` event field is an ordinary field.
        assert_eq!(
            record.attributes,
            vec![kv("timestamp", string("root field"))]
        );
    }

    #[test]
    fn decoded_legacy_log_with_metadata_timestamp_key_round_trips() {
        // The source writes the timestamp to the event root even with a metadata key.
        init_metadata_timestamp_key();
        round_trip(LogNamespace::Legacy);
    }

    fn init_metadata_source_type_key() {
        let mut schema = LogSchema::default();
        schema.set_source_type_key(Some(OwnedTargetPath::metadata(owned_value_path!(
            "source_type"
        ))));
        init_log_schema(schema, true);
    }

    #[test]
    fn native_legacy_log_preserves_payload_with_metadata_source_type() {
        init_metadata_source_type_key();
        let mut log = LogEvent::from("disk full");
        log.insert(log_schema().source_type_key_target_path().unwrap(), "kafka");
        // Even when the values match, the event field is not the metadata marker.
        log.insert(event_path!("source_type"), "kafka");

        let request = log_event_to_export_request(log);
        let record = &request.resource_logs[0].scope_logs[0].log_records[0];
        assert_eq!(record.attributes, vec![kv("source_type", string("kafka"))]);
    }

    #[test]
    fn decoded_legacy_log_with_metadata_source_type_key_round_trips() {
        // The source writes the marker to the event root even with a metadata key.
        init_metadata_source_type_key();
        round_trip(LogNamespace::Legacy);
    }

    #[test]
    fn native_legacy_log_maps_message_timestamp_and_extra_fields() {
        let timestamp = Utc.timestamp_nanos(1_700_000_000_000_000_000);
        let mut log = LogEvent::from("disk full");
        log.insert(event_path!("timestamp"), timestamp);
        log.insert(event_path!("host"), "web-1");
        log.insert(event_path!("source_type"), "file");

        let expected = request(
            Resource::default(),
            InstrumentationScope::default(),
            LogRecord {
                time_unix_nano: 1_700_000_000_000_000_000,
                body: Some(AnyValue {
                    value: Some(string("disk full")),
                }),
                attributes: vec![kv("host", string("web-1"))],
                ..Default::default()
            },
        );
        assert_eq!(log_event_to_export_request(log), expected);
    }

    #[test]
    fn fields_with_invalid_otlp_values_are_kept_as_attributes() {
        let mut log = LogEvent::from("message");
        log.insert(event_path!("trace_id"), "not-hex");
        log.insert(event_path!("span_id"), "0102");
        log.insert(event_path!("severity_number"), 99);
        log.insert(event_path!("attributes", "host"), "from-attributes");
        log.insert(event_path!("host"), "from-root");

        let request = log_event_to_export_request(log);
        let record = &request.resource_logs[0].scope_logs[0].log_records[0];
        assert!(record.trace_id.is_empty());
        assert!(record.span_id.is_empty());
        assert_eq!(record.severity_number, 0);
        // Keys from `attributes` come first and win over top-level fields with the same key.
        assert_eq!(
            record.attributes,
            vec![
                kv("host", string("from-attributes")),
                kv("severity_number", PBValue::IntValue(99)),
                kv("span_id", string("0102")),
                kv("trace_id", string("not-hex")),
            ]
        );
    }

    #[test]
    fn native_vector_namespace_log_uses_event_as_body_and_ingest_time_as_observed() {
        let mut log = LogEvent::from(Value::from(ObjectMap::from([(
            "msg".into(),
            Value::from("hi"),
        )])));
        let metadata = log.metadata_mut().value_mut();
        metadata.insert(path!("vector", "ingest_timestamp"), Utc.timestamp_nanos(5));
        metadata.insert(path!("vector", "source_type"), "demo_logs");
        metadata.insert(path!("demo_logs", "service"), "vector");
        assert_eq!(log.namespace(), LogNamespace::Vector);

        let expected = request(
            Resource::default(),
            InstrumentationScope::default(),
            LogRecord {
                observed_time_unix_nano: 5,
                body: Some(AnyValue {
                    value: Some(PBValue::KvlistValue(KeyValueList {
                        values: vec![kv("msg", string("hi"))],
                    })),
                }),
                ..Default::default()
            },
        );
        assert_eq!(log_event_to_export_request(log), expected);
    }
}
