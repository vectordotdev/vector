use std::{borrow::Cow, cmp::Ordering};

use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use vector_core::{
    config::{LegacyKey, LogNamespace, LogSchema, log_schema},
    event::{Event, LogEvent},
};
use vrl::{
    core::Value,
    path,
    path::{OwnedSegment, OwnedValuePath, PathPrefix, ValuePath},
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
pub const EVENT_NAME_KEY: &str = "event_name";

impl ResourceLogs {
    pub fn into_event_iter(self, log_namespace: LogNamespace) -> impl Iterator<Item = Event> {
        let now = Utc::now();
        let schema = log_schema();

        self.scope_logs.into_iter().flat_map(move |scope_log| {
            let scope = scope_log.scope;
            let resource = self.resource.clone();
            scope_log.log_records.into_iter().map(move |log_record| {
                ResourceLog {
                    resource: resource.clone(),
                    scope: scope.clone(),
                    log_record,
                }
                .into_event(log_namespace, schema, now)
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
    fn into_event(
        self,
        log_namespace: LogNamespace,
        schema: &LogSchema,
        now: DateTime<Utc>,
    ) -> Event {
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
                    log.maybe_insert(schema.message_key_target_path(), v);
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
        if !self.log_record.event_name.is_empty() {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(EVENT_NAME_KEY))),
                path!(EVENT_NAME_KEY),
                self.log_record.event_name,
            );
        }
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
                Value::from(to_hex(&self.log_record.trace_id)),
            );
        }
        if !self.log_record.span_id.is_empty() {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(SPAN_ID_KEY))),
                path!(SPAN_ID_KEY),
                Value::from(to_hex(&self.log_record.span_id)),
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
            schema.timestamp_key().map(LegacyKey::Overwrite),
            path!(TIMESTAMP_KEY),
            timestamp,
        );

        log_namespace.insert_vector_metadata(
            &mut log,
            schema.source_type_key(),
            path!("source_type"),
            Value::from_static_str(SOURCE_NAME),
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
/// - A resource without attributes is not set, because the source does not decode it. An
///   unset resource means that the resource is unknown.
///
/// - Legacy namespace: the message field becomes the body and the timestamp field becomes
///   `time_unix_nano`. The fields written by the source (`attributes`, `resources`, `scope`,
///   `trace_id`, `span_id`, `severity_text`, `severity_number`, `flags`, `event_name`,
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
/// write it there. A timestamp at a metadata timestamp key that OTLP cannot represent is
/// sent as an attribute at the same path, unless that replaces event data.
#[must_use]
pub fn log_event_to_export_request(log: LogEvent) -> ExportLogsServiceRequest {
    encode_log_event(log, log_schema())
}

fn encode_log_event(log: LogEvent, schema: &LogSchema) -> ExportLogsServiceRequest {
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
        LogNamespace::Legacy => take_legacy_fields(log, schema, &mut record),
    };

    let (resource, scope) = take_record_fields(&mut fields, &mut record);
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: (!resource.attributes.is_empty()).then_some(resource),
            scope_logs: vec![ScopeLogs {
                scope: Some(scope),
                log_records: vec![record],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

/// Move the body and timestamps of a Legacy event into `record` and return the other fields.
fn take_legacy_fields(mut log: LogEvent, schema: &LogSchema, record: &mut LogRecord) -> Value {
    // Read timestamps before removing a message that may contain them. The
    // opentelemetry source ignores the timestamp key's metadata prefix, so
    // prefer metadata but retain the event-root fallback.
    let metadata_timestamp_path = schema
        .timestamp_key_target_path()
        .filter(|path| path.prefix == PathPrefix::Metadata);
    let metadata_time = metadata_timestamp_path
        .and_then(|path| log.get(path))
        .and_then(Value::as_timestamp)
        .and_then(timestamp_nanos);
    let event_time = schema
        .timestamp_key()
        .filter(|_| metadata_time.is_none())
        .and_then(|path| log.get((PathPrefix::Event, path)))
        .and_then(Value::as_timestamp)
        .and_then(timestamp_nanos);
    // The message, the source type marker, and the timestamp can share an array, for
    // example as `.payload[0]` and `.payload[1]`. Remove them in an order that does not
    // move the values that are still to be removed.
    let message_path = schema
        .message_key_target_path()
        .map(|path| resolve_indexes(&log, path.prefix, &path.path));
    // A marker at the configured metadata path leaves the event field as user data.
    // Otherwise, remove the root marker for Legacy sources that ignore the prefix.
    let source_type_path = schema.source_type_key_target_path().map(|path| {
        let prefix = if path.prefix == PathPrefix::Event || log.contains(path) {
            path.prefix
        } else {
            PathPrefix::Event
        };
        resolve_indexes(&log, prefix, &path.path)
    });
    // A timestamp inside the message stays in the body.
    let timestamp_path = schema
        .timestamp_key()
        .filter(|_| event_time.is_some())
        .map(|path| resolve_indexes(&log, PathPrefix::Event, path))
        .filter(|path| {
            message_path
                .as_ref()
                .is_none_or(|message| locate(path, message) != Location::Inside)
        });
    // A metadata timestamp that OTLP cannot represent is sent as an attribute, because
    // the metadata is not sent. A timestamp inside the message stays in the body.
    let invalid_metadata_timestamp_path = metadata_timestamp_path
        .filter(|_| metadata_time.is_none())
        .map(|path| resolve_indexes(&log, path.prefix, &path.path))
        .filter(|path| {
            message_path
                .as_ref()
                .is_none_or(|message| locate(path, message) != Location::Inside)
        });
    // The message is the first entry. A path without a value is not removed: after an earlier
    // removal prunes an array element, it can point to a value that moved there.
    let mut pending = [
        message_path,
        source_type_path,
        timestamp_path,
        invalid_metadata_timestamp_path,
    ]
    .map(|path| path.filter(|(prefix, path)| log.contains((*prefix, &**path))));
    let mut invalid_metadata_timestamp = None;
    while let Some(index) = next_removal(&pending) {
        let Some((prefix, path)) = pending[index].take() else {
            break;
        };
        let removed = log.remove_prune((prefix, &*path), true);
        match index {
            0 => record.body = removed.and_then(into_body),
            3 => invalid_metadata_timestamp = removed,
            _ => {}
        }
    }
    let (mut fields, _) = log.into_parts();
    // Existing event data at the path or at an ancestor of it is not replaced.
    if let Some(value) = invalid_metadata_timestamp
        && let Some(path) = schema.timestamp_key()
        && is_vacant(&fields, path)
    {
        fields.insert(path, value);
    }
    record.time_unix_nano = metadata_time.or(event_time).unwrap_or_default();
    record.observed_time_unix_nano = take(
        &mut fields,
        path!(OBSERVED_TIMESTAMP_KEY),
        into_timestamp_nanos,
    )
    .unwrap_or_default();
    fields
}

/// Move the namespace-independent OTLP fields from `fields` into `record` and return the
/// resource and scope. All fields that remain become log record attributes.
fn take_record_fields(
    fields: &mut Value,
    record: &mut LogRecord,
) -> (Resource, InstrumentationScope) {
    record.severity_text = take(fields, path!(SEVERITY_TEXT_KEY), into_string).unwrap_or_default();
    record.severity_number = take(fields, path!(SEVERITY_NUMBER_KEY), into_i32).unwrap_or_default();
    record.flags = take(fields, path!(FLAGS_KEY), into_u32).unwrap_or_default();
    record.event_name = take(fields, path!(EVENT_NAME_KEY), into_string).unwrap_or_default();
    record.dropped_attributes_count =
        take(fields, path!(DROPPED_ATTRIBUTES_COUNT_KEY), into_u32).unwrap_or_default();
    record.trace_id = take(fields, path!(TRACE_ID_KEY), into_id::<16>).unwrap_or_default();
    record.span_id = take(fields, path!(SPAN_ID_KEY), into_id::<8>).unwrap_or_default();
    let attributes = take(fields, path!(ATTRIBUTES_KEY), into_object).unwrap_or_default();

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

    let mut rest = match std::mem::replace(fields, Value::Null) {
        Value::Object(rest) => rest,
        // A Legacy event whose root is not an object has no message field; the root is the body.
        other => {
            if record.body.is_none() {
                record.body = into_body(other);
            }
            ObjectMap::new()
        }
    };
    // Keys from `attributes` come first and win over the remaining fields with the same key.
    for key in attributes.keys() {
        rest.remove(key);
    }
    record.attributes = attributes
        .into_iter()
        .chain(rest)
        .map(|(key, value)| KeyValue {
            key: key.into(),
            value: Some(value.into()),
        })
        .collect();

    (resource, scope)
}

/// Convert the value at `path` and remove it. If the conversion fails, the value stays in
/// place, so that it is sent as an attribute instead of being lost. The value is not removed
/// first, because removing an array element moves the elements after it.
fn take<'a, T>(
    fields: &mut Value,
    path: impl ValuePath<'a> + Copy,
    convert: fn(Value) -> Result<T, Value>,
) -> Option<T> {
    let slot = fields.get_mut(path)?;
    match convert(std::mem::replace(slot, Value::Null)) {
        Ok(converted) => {
            fields.remove(path, true);
            Some(converted)
        }
        Err(value) => {
            *slot = value;
            None
        }
    }
}

/// Whether inserting at `path` in `value` adds a value without replacing or moving an
/// existing value. Missing objects and arrays are created, and an index after the end of an
/// array fills the gap with nulls.
fn is_vacant(mut value: &Value, path: &OwnedValuePath) -> bool {
    for segment in &path.segments {
        let next = match (value, segment) {
            (Value::Object(map), OwnedSegment::Field(field)) => map.get(field),
            (Value::Array(array), &OwnedSegment::Index(index)) => match usize::try_from(index) {
                Ok(index) => array.get(index),
                // A negative index before the start would move the existing elements.
                Err(_) => match index
                    .checked_add_unsigned(array.len())
                    .and_then(|index| usize::try_from(index).ok())
                {
                    Some(index) => array.get(index),
                    None => return false,
                },
            },
            _ => return false,
        };
        match next {
            Some(next) => value = next,
            None => return true,
        }
    }
    false
}

type FieldPath<'a> = (PathPrefix, Cow<'a, OwnedValuePath>);

/// Replace the negative array indexes in `path` with the indexes that they point to in `log`,
/// so that the path points to the same value after other array elements are removed.
fn resolve_indexes<'a>(
    log: &LogEvent,
    prefix: PathPrefix,
    path: &'a OwnedValuePath,
) -> FieldPath<'a> {
    let mut resolved = Cow::Borrowed(path);
    for (depth, segment) in path.segments.iter().enumerate() {
        if let OwnedSegment::Index(index) = *segment
            && index < 0
            && let Some(Value::Array(array)) = log.get((prefix, &resolved.segments[..depth]))
            && let Some(index) = index
                .checked_add_unsigned(array.len())
                .filter(|index| *index >= 0)
        {
            resolved.to_mut().segments[depth] = OwnedSegment::Index(index);
        }
    }
    (prefix, resolved)
}

/// Where the value at one path is, compared with the value at another path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Location {
    /// Removing one of the values does not move or remove the other.
    Separate,
    /// Both values are in one array, and this value comes first.
    Before,
    /// Both values are in one array, and this value comes after the other.
    After,
    /// This value contains the other value, or both paths are the same.
    Contains,
    /// The other value contains this value.
    Inside,
}

/// Find where the value at `path` is, compared with the value at `other`. The paths must not
/// have negative array indexes that point to a value (see [`resolve_indexes`]).
fn locate((prefix, path): &FieldPath<'_>, (other_prefix, other): &FieldPath<'_>) -> Location {
    if prefix != other_prefix {
        return Location::Separate;
    }
    for segments in path.segments.iter().zip(&other.segments) {
        match segments {
            (OwnedSegment::Field(field), OwnedSegment::Field(other_field))
                if field == other_field => {}
            (OwnedSegment::Index(index), OwnedSegment::Index(other_index)) => {
                match index.cmp(other_index) {
                    Ordering::Less => return Location::Before,
                    Ordering::Greater => return Location::After,
                    Ordering::Equal => {}
                }
            }
            _ => return Location::Separate,
        }
    }
    if path.segments.len() <= other.segments.len() {
        Location::Contains
    } else {
        Location::Inside
    }
}

/// The index of a pending path whose removal does not move or remove the value at any other
/// pending path. If there is no such path (for example, when two paths are the same), the
/// first pending path is used.
fn next_removal(pending: &[Option<FieldPath<'_>>]) -> Option<usize> {
    let mut first = None;
    for (index, path) in pending.iter().enumerate() {
        let Some(path) = path else {
            continue;
        };
        first.get_or_insert(index);
        let disturbs_other = pending.iter().enumerate().any(|(other_index, other)| {
            other_index != index
                && other.as_ref().is_some_and(|other| {
                    matches!(locate(path, other), Location::Before | Location::Contains)
                })
        });
        if !disturbs_other {
            return Some(index);
        }
    }
    first
}

fn into_body(value: Value) -> Option<AnyValue> {
    (!value.is_null()).then(|| value.into())
}

/// Not `timestamp_nanos_opt`, whose `i64` ends in 2262: the OTLP `uint64` ends in 2554.
fn timestamp_nanos(timestamp: &DateTime<Utc>) -> Option<u64> {
    u64::try_from(timestamp.timestamp())
        .ok()?
        .checked_mul(1_000_000_000)?
        .checked_add(u64::from(timestamp.timestamp_subsec_nanos()))
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

/// Any `i32` is accepted, because the source keeps severity numbers that are newer than the
/// vendored OTLP schema.
fn into_i32(value: Value) -> Result<i32, Value> {
    match value {
        Value::Integer(int) => i32::try_from(int).map_err(|_| Value::Integer(int)),
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

fn into_object(value: Value) -> Result<ObjectMap, Value> {
    match value {
        Value::Object(object) => Ok(object),
        other => Err(other),
    }
}

fn into_kv_list(value: Value) -> Result<Vec<KeyValue>, Value> {
    into_object(value).map(object_into_kv_list)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use prost::Message;
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
        resource: Option<Resource>,
        scope: InstrumentationScope,
        record: LogRecord,
    ) -> ExportLogsServiceRequest {
        ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource,
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
            Some(Resource {
                attributes: vec![kv("service.name", string("api"))],
                ..Default::default()
            }),
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
                event_name: "build.finished".to_owned(),
            },
        )
    }

    fn round_trip(namespace: LogNamespace, schema: &LogSchema) {
        assert_round_trip(&full_request(), namespace, schema);
    }

    fn assert_round_trip(
        expected: &ExportLogsServiceRequest,
        namespace: LogNamespace,
        schema: &LogSchema,
    ) {
        let resource_logs = expected.resource_logs[0].clone();
        let scope_logs = resource_logs.scope_logs[0].clone();
        let log = ResourceLog {
            resource: resource_logs.resource,
            scope: scope_logs.scope,
            log_record: scope_logs.log_records[0].clone(),
        }
        .into_event(namespace, schema, Utc::now())
        .into_log();
        assert_eq!(log.namespace(), namespace);
        assert_eq!(&encode_log_event(log, schema), expected);
    }

    #[test]
    fn decoded_legacy_log_round_trips() {
        round_trip(LogNamespace::Legacy, &LogSchema::default());
    }

    #[test]
    fn decoded_vector_namespace_log_round_trips() {
        round_trip(LogNamespace::Vector, &LogSchema::default());
    }

    #[test]
    fn decoded_log_without_resource_round_trips() {
        let expected = request(
            None,
            InstrumentationScope::default(),
            LogRecord {
                time_unix_nano: 1,
                observed_time_unix_nano: 2,
                ..Default::default()
            },
        );
        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            assert_round_trip(&expected, namespace, &LogSchema::default());
        }
    }

    #[test]
    fn native_legacy_log_encodes_timestamp_after_2262() {
        // Later than the `i64` nanosecond range, in the `u64` range of OTLP.
        let timestamp = DateTime::from_timestamp(10_000_000_000, 5).unwrap();
        let mut log = LogEvent::from("message");
        log.insert(event_path!("timestamp"), timestamp);
        log.insert(event_path!("observed_timestamp"), timestamp);

        let request = log_event_to_export_request(log);
        let record = &request.resource_logs[0].scope_logs[0].log_records[0];
        assert_eq!(record.time_unix_nano, 10_000_000_000_000_000_005);
        assert_eq!(record.observed_time_unix_nano, 10_000_000_000_000_000_005);
        assert!(record.attributes.is_empty());
    }

    #[test]
    fn decoded_log_with_unknown_severity_number_round_trips() {
        // A severity number that is newer than the vendored OTLP schema.
        let severity_number = 25;
        assert!(SeverityNumber::try_from(severity_number).is_err());
        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let log = ResourceLog {
                resource: None,
                scope: None,
                log_record: LogRecord {
                    severity_number,
                    ..Default::default()
                },
            }
            .into_event(namespace, &LogSchema::default(), Utc::now())
            .into_log();
            let request = encode_log_event(log, &LogSchema::default());
            let record = &request.resource_logs[0].scope_logs[0].log_records[0];
            assert_eq!(record.severity_number, severity_number);
            assert!(record.attributes.is_empty());
        }
    }

    #[test]
    fn decoded_legacy_log_with_metadata_message_key_round_trips() {
        let mut schema = LogSchema::default();
        schema.set_message_key(Some(OwnedTargetPath::metadata(owned_value_path!(
            "message"
        ))));

        round_trip(LogNamespace::Legacy, &schema);
    }

    #[test]
    fn native_legacy_log_reads_timestamp_inside_event_message() {
        let mut schema = LogSchema::default();
        schema.set_message_key(Some(OwnedTargetPath::event(owned_value_path!("payload"))));
        schema.set_timestamp_key(Some(OwnedTargetPath::event(owned_value_path!(
            "payload",
            "timestamp"
        ))));
        let mut log = LogEvent::default();
        log.insert(event_path!("payload", "message"), "hello");
        log.insert(event_path!("payload", "timestamp"), Utc.timestamp_nanos(5));

        let request = encode_log_event(log, &schema);
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
        let mut log = LogEvent::default();
        log.insert((PathPrefix::Metadata, path!("payload", "message")), "hello");
        log.insert(
            schema.timestamp_key_target_path().unwrap(),
            Utc.timestamp_nanos(5),
        );
        log.insert(event_path!("payload", "timestamp"), Utc.timestamp_nanos(9));

        let request = encode_log_event(log, &schema);
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

    fn metadata_timestamp_key_schema() -> LogSchema {
        let mut schema = LogSchema::default();
        schema.set_timestamp_key(Some(OwnedTargetPath::metadata(owned_value_path!(
            "timestamp"
        ))));
        schema
    }

    #[test]
    fn native_legacy_log_reads_metadata_timestamp_key() {
        let schema = metadata_timestamp_key_schema();
        let mut log = LogEvent::from("disk full");
        log.insert(
            schema.timestamp_key_target_path().unwrap(),
            Utc.timestamp_nanos(1_700_000_000_000_000_000),
        );
        log.insert(event_path!("timestamp"), "root field");

        let request = encode_log_event(log, &schema);
        let record = &request.resource_logs[0].scope_logs[0].log_records[0];
        assert_eq!(record.time_unix_nano, 1_700_000_000_000_000_000);
        // With a metadata timestamp key, a `timestamp` event field is an ordinary field.
        assert_eq!(
            record.attributes,
            vec![kv("timestamp", string("root field"))]
        );
    }

    #[test]
    fn native_legacy_log_keeps_invalid_metadata_timestamp_as_attribute() {
        let schema = metadata_timestamp_key_schema();
        let mut log = LogEvent::from("disk full");
        // Before the epoch, so not in the OTLP `uint64` range.
        log.insert(
            schema.timestamp_key_target_path().unwrap(),
            Utc.timestamp_nanos(-1),
        );
        log.insert(event_path!("timestamp"), Utc.timestamp_nanos(5));

        let request = encode_log_event(log, &schema);
        let record = &request.resource_logs[0].scope_logs[0].log_records[0];
        // The event-root fallback supplies `time_unix_nano`.
        assert_eq!(record.time_unix_nano, 5);
        assert_eq!(
            record.attributes,
            vec![kv("timestamp", string("1969-12-31T23:59:59.999999999Z"))]
        );
    }

    #[test]
    fn native_legacy_log_keeps_event_ancestor_of_invalid_metadata_timestamp() {
        let mut schema = LogSchema::default();
        schema.set_timestamp_key(Some(OwnedTargetPath::metadata(owned_value_path!(
            "payload",
            "timestamp"
        ))));
        let mut log = LogEvent::default();
        log.insert(
            schema.timestamp_key_target_path().unwrap(),
            Utc.timestamp_nanos(-1),
        );
        log.insert(event_path!("payload"), "keep");

        let request = encode_log_event(log, &schema);
        let record = &request.resource_logs[0].scope_logs[0].log_records[0];
        assert_eq!(record.attributes, vec![kv("payload", string("keep"))]);
    }

    #[test]
    fn decoded_legacy_log_with_metadata_timestamp_key_round_trips() {
        // The source writes the timestamp to the event root even with a metadata key.
        round_trip(LogNamespace::Legacy, &metadata_timestamp_key_schema());
    }

    fn metadata_source_type_key_schema() -> LogSchema {
        let mut schema = LogSchema::default();
        schema.set_source_type_key(Some(OwnedTargetPath::metadata(owned_value_path!(
            "source_type"
        ))));
        schema
    }

    #[test]
    fn native_legacy_log_preserves_payload_with_metadata_source_type() {
        let schema = metadata_source_type_key_schema();
        let mut log = LogEvent::from("disk full");
        log.insert(schema.source_type_key_target_path().unwrap(), "kafka");
        // Even when the values match, the event field is not the metadata marker.
        log.insert(event_path!("source_type"), "kafka");

        let request = encode_log_event(log, &schema);
        let record = &request.resource_logs[0].scope_logs[0].log_records[0];
        assert_eq!(record.attributes, vec![kv("source_type", string("kafka"))]);
    }

    #[test]
    fn decoded_legacy_log_with_metadata_source_type_key_round_trips() {
        // The source writes the marker to the event root even with a metadata key.
        round_trip(LogNamespace::Legacy, &metadata_source_type_key_schema());
    }

    #[test]
    fn native_legacy_log_omits_source_type_inside_message() {
        for (message_key, source_type_key) in [
            (
                OwnedTargetPath::event(owned_value_path!("payload")),
                OwnedTargetPath::event(owned_value_path!("payload", "source_type")),
            ),
            (
                OwnedTargetPath::metadata(owned_value_path!("payload")),
                OwnedTargetPath::metadata(owned_value_path!("payload", "source_type")),
            ),
        ] {
            let mut schema = LogSchema::default();
            schema.set_message_key(Some(message_key.clone()));
            schema.set_source_type_key(Some(source_type_key.clone()));
            let mut log = LogEvent::default();
            log.insert(&message_key.with_field_appended("message"), "hello");
            log.insert(&source_type_key, "file");

            let request = encode_log_event(log, &schema);
            let record = &request.resource_logs[0].scope_logs[0].log_records[0];
            assert_eq!(
                record.body,
                Some(AnyValue {
                    value: Some(PBValue::KvlistValue(KeyValueList {
                        values: vec![kv("message", string("hello"))],
                    })),
                })
            );
            assert!(record.attributes.is_empty());
        }
    }

    #[test]
    fn native_legacy_log_maps_message_timestamp_and_extra_fields() {
        let timestamp = Utc.timestamp_nanos(1_700_000_000_000_000_000);
        let mut log = LogEvent::from("disk full");
        log.insert(event_path!("timestamp"), timestamp);
        log.insert(event_path!("host"), "web-1");
        log.insert(event_path!("source_type"), "file");

        let expected = request(
            None,
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
        // Larger than an OTLP `int32`.
        log.insert(event_path!("severity_number"), 1_i64 << 31);
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
                kv("severity_number", PBValue::IntValue(1 << 31)),
                kv("span_id", string("0102")),
                kv("trace_id", string("not-hex")),
            ]
        );
    }

    fn payload_index(index: isize) -> OwnedTargetPath {
        OwnedTargetPath::event(OwnedValuePath {
            segments: vec![OwnedSegment::field("payload"), OwnedSegment::Index(index)],
        })
    }

    #[test]
    fn native_legacy_log_keeps_array_siblings_of_invalid_timestamp() {
        let mut schema = LogSchema::default();
        schema.set_timestamp_key(Some(payload_index(0)));
        let mut log = LogEvent::default();
        log.insert(
            event_path!("payload"),
            Value::Array(vec!["bad".into(), "keep".into()]),
        );

        let request = encode_log_event(log, &schema);
        let record = &request.resource_logs[0].scope_logs[0].log_records[0];
        assert_eq!(record.time_unix_nano, 0);
        assert_eq!(
            record.attributes,
            vec![kv(
                "payload",
                PBValue::ArrayValue(ArrayValue {
                    values: vec![
                        AnyValue {
                            value: Some(string("bad")),
                        },
                        AnyValue {
                            value: Some(string("keep")),
                        },
                    ],
                }),
            )]
        );
    }

    #[test]
    fn native_legacy_log_keeps_array_element_after_pruned_element() {
        let field_in_first = |field: &str| {
            OwnedTargetPath::event(OwnedValuePath {
                segments: vec![
                    OwnedSegment::field("payload"),
                    OwnedSegment::Index(0),
                    OwnedSegment::field(field),
                ],
            })
        };
        let mut schema = LogSchema::default();
        schema.set_message_key(Some(field_in_first("message")));
        schema.set_source_type_key(Some(field_in_first("source_type")));
        let mut log = LogEvent::default();
        log.insert(
            event_path!("payload"),
            Value::Array(vec![
                Value::from(ObjectMap::from([("message".into(), "hello".into())])),
                Value::from(ObjectMap::from([(
                    "source_type".into(),
                    "user-data".into(),
                )])),
            ]),
        );

        let request = encode_log_event(log, &schema);
        let record = &request.resource_logs[0].scope_logs[0].log_records[0];
        assert_eq!(
            record.body,
            Some(AnyValue {
                value: Some(string("hello")),
            })
        );
        // Removing the message prunes the first element, so the second element moves to
        // index 0. The source type path had no value, so the moved element is kept.
        assert_eq!(
            record.attributes,
            vec![kv(
                "payload",
                PBValue::ArrayValue(ArrayValue {
                    values: vec![AnyValue {
                        value: Some(PBValue::KvlistValue(KeyValueList {
                            values: vec![kv("source_type", string("user-data"))],
                        })),
                    }],
                }),
            )]
        );
    }

    #[test]
    fn native_legacy_log_removes_schema_fields_that_share_an_array() {
        // The array indexes of the message, the source type marker, and the timestamp.
        for indexes @ [message, source_type, timestamp] in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
            [-1, 0, 1],
            [0, -2, 2],
        ] {
            let mut schema = LogSchema::default();
            schema.set_message_key(Some(payload_index(message)));
            schema.set_source_type_key(Some(payload_index(source_type)));
            schema.set_timestamp_key(Some(payload_index(timestamp)));
            let mut payload = vec![Value::Null; 3];
            let position = |index: isize| usize::try_from(index.rem_euclid(3)).unwrap();
            payload[position(message)] = "disk full".into();
            payload[position(source_type)] = "file".into();
            payload[position(timestamp)] = Utc.timestamp_nanos(5).into();
            let mut log = LogEvent::default();
            log.insert(event_path!("payload"), Value::Array(payload));

            let request = encode_log_event(log, &schema);
            let record = &request.resource_logs[0].scope_logs[0].log_records[0];
            assert_eq!(
                record.body,
                Some(AnyValue {
                    value: Some(string("disk full")),
                }),
                "{indexes:?}"
            );
            assert_eq!(record.time_unix_nano, 5, "{indexes:?}");
            assert!(record.attributes.is_empty(), "{indexes:?}");
        }
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
            None,
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

    #[test]
    fn decodes_native_event_name_in_both_namespaces() {
        // OTLP LogRecord field 12, independently encoded so the test checks the wire tag.
        let encoded = b"\x62\x0ebuild.finished";
        for log_namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let event = ResourceLog {
                resource: None,
                scope: None,
                log_record: LogRecord::decode(encoded.as_slice()).unwrap(),
            }
            .into_event(log_namespace, &LogSchema::default(), Utc::now());
            let log = event.as_log();
            let event_name = match log_namespace {
                LogNamespace::Legacy => log.get(event_path!(EVENT_NAME_KEY)),
                LogNamespace::Vector => log
                    .metadata()
                    .value()
                    .get(path!(SOURCE_NAME, EVENT_NAME_KEY)),
            };
            assert_eq!(event_name, Some(&Value::from("build.finished")));
        }
    }

    #[test]
    fn omits_absent_or_empty_event_name() {
        for encoded in [b"".as_slice(), b"\x62\x00".as_slice()] {
            for log_namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
                let event = ResourceLog {
                    resource: None,
                    scope: None,
                    log_record: LogRecord::decode(encoded).unwrap(),
                }
                .into_event(log_namespace, &LogSchema::default(), Utc::now());
                let log = event.as_log();
                assert!(log.get(event_path!(EVENT_NAME_KEY)).is_none());
                assert!(
                    log.metadata()
                        .value()
                        .get(path!(SOURCE_NAME, EVENT_NAME_KEY))
                        .is_none()
                );
            }
        }
    }
}
