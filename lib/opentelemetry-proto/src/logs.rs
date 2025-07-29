use super::common::{kv_list_into_value, to_hex};
use crate::proto::common::v1::{AnyValue as OtelAnyValueStruct, ArrayValue, KeyValue, KeyValueList};
use crate::proto::{
    common::v1::{any_value::Value as OtelAnyValueEnum, InstrumentationScope},
    logs::v1::{LogRecord, ResourceLogs, ScopeLogs, SeverityNumber},
    resource::v1::Resource,
};
use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use std::collections::HashMap;
use vector_core::{
    config::{log_schema, LegacyKey, LogNamespace},
    event::{Event, LogEvent},
};
use vrl::{event_path, path, value::{ObjectMap, Value}};

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
    fn into_event(self, log_namespace: LogNamespace, now: DateTime<Utc>) -> Event {
        let mut log = match log_namespace {
            LogNamespace::Vector => {
                if let Some(v) = self.log_record.body.and_then(|av| av.value) {
                    LogEvent::from(<OtelAnyValueEnum as Into<Value>>::into(v))
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
        if let Some(resource) = self.resource {
            if !resource.attributes.is_empty() {
                log_namespace.insert_source_metadata(
                    SOURCE_NAME,
                    &mut log,
                    Some(LegacyKey::Overwrite(path!(RESOURCE_KEY))),
                    path!(RESOURCE_KEY),
                    kv_list_into_value(resource.attributes),
                );
            }
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
            path!("timestamp"),
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


/// Converts a list of `LogEvent`s into a single `ResourceLogs` message.
/// All logs will share the same `Resource`. They are grouped into `ScopeLogs`
/// by `(scope.name, scope.version)`.
pub fn log_events_to_resource_logs(events: Vec<LogEvent>) -> ResourceLogs {
    let mut scope_map: HashMap<(String, String), Vec<LogRecord>> = HashMap::new();
    let mut resource: Option<Resource> = None;

    for event in events {
        let record = build_log_record(&event);

        let scope_name = event
            .get(event_path!("scope", "name"))
            .and_then(Value::as_str)
            .unwrap_or("".into()).to_string();
        let scope_version = event
            .get(event_path!("scope", "version"))
            .and_then(Value::as_str)
            .unwrap_or("".into())
            .to_string();

        scope_map
            .entry((scope_name, scope_version))
            .or_default()
            .push(record);

        if resource.is_none() {
            resource = extract_resource(&event);
        }
    }

    let scope_logs = scope_map
        .into_iter()
        .map(|((name, version), log_records)| ScopeLogs {
            scope: Some(InstrumentationScope {
                name,
                version,
                ..Default::default()
            }),
            log_records,
            schema_url: String::new(),
        })
        .collect();

    ResourceLogs {
        resource,
        scope_logs,
        schema_url: String::new(),
    }
}

/// Converts a `LogEvent` into an OTLP `LogRecord` according to the OpenTelemetry log data model.
fn build_log_record(event: &LogEvent) -> LogRecord {
    let mut record = LogRecord::default();

    // Timestamps
    if let Some(ts) = event.get_timestamp() {
        if let Some(ts) = ts.as_timestamp() {
            if let Some(nanos) = ts.timestamp_nanos_opt() {
                record.time_unix_nano = nanos as u64;
            }
        }
    }
    if let Some(ts) = event
        .get(event_path!("observed_timestamp"))
        .and_then(Value::as_timestamp)
    {
        if let Some(nanos) = ts.timestamp_nanos_opt() {
            record.observed_time_unix_nano = nanos as u64;
        }
    }

    // Severity fields
    if let Some(text) = event.get(event_path!("severity_text")).and_then(Value::as_str) {
        record.severity_text = text.to_string();
    }
    if let Some(num) = event.get(event_path!("severity_number")).and_then(Value::as_integer) {
        record.severity_number = num as i32;
    }

    // Body
    if let Some(body) = event.get(event_path!("message")).or_else(|| event.get(event_path!("body"))) {
        record.body = Some(to_any_value(body));
    }

    // Attributes
    if let Some(attrs) = event.get(event_path!("attributes")).and_then(Value::as_object) {
        record.attributes = vrl_map_to_key_value_vec(attrs);
    }

    // Trace ID (must be 16 bytes)
    if let Some(bytes) = event.get(event_path!("trace_id")).and_then(Value::as_bytes) {
        if bytes.len() == 16 && !bytes.iter().all(|&b| b == 0) {
            record.trace_id = bytes.to_vec();
        }
    }

    // Span ID (must be 8 bytes)
    if let Some(bytes) = event.get(event_path!("span_id")).and_then(Value::as_bytes) {
        if bytes.len() == 8 && !bytes.iter().all(|&b| b == 0) {
            record.span_id = bytes.to_vec();
        }
    }

    // Flags
    if let Some(flags) = event.get(event_path!("flags")).and_then(Value::as_integer) {
        record.flags = flags as u32;
    }

    // Dropped attributes count (optional)
    if let Some(dropped) = event
        .get(event_path!("dropped_attributes_count"))
        .and_then(Value::as_integer)
    {
        record.dropped_attributes_count = dropped as u32;
    }

    record
}

fn vrl_map_to_key_value_vec(object_map: &ObjectMap) -> Vec<KeyValue> {
    object_map.iter()
        .map(|(k, v)| {
            KeyValue {
                key: k.to_string(),
                value: Some(to_any_value(v)),
            }
        }).collect()
}

/// Extracts a `Resource` from a `LogEvent` if a `resources` object is present.
fn extract_resource(event: &LogEvent) -> Option<Resource> {
    event
        .get(event_path!("resources"))
        .and_then(Value::as_object)
        .map(|object_map| Resource {
            attributes: vrl_map_to_key_value_vec(object_map),
            dropped_attributes_count: 0,
        })
}

fn convert_vrl_object_map_to_kvlistvalue(object_map: &ObjectMap) -> OtelAnyValueEnum {
    OtelAnyValueEnum::KvlistValue(KeyValueList {
            values:
            object_map
                .iter()
                .map(|(k,
                          v)| KeyValue {
                    key: k.to_string(),
                    value: Some(to_any_value(v)),
                })
                .collect(),
        })
}

/// Converts a VRL `Value` into an OTLP `AnyValue`.
fn to_any_value(value: &Value) -> OtelAnyValueStruct {
    let value = match value {
        Value::Null => None,
        Value::Boolean(b) => Some(OtelAnyValueEnum::BoolValue(*b)),
        Value::Integer(i) => Some(OtelAnyValueEnum::IntValue(*i)),
        Value::Float(f) => Some(OtelAnyValueEnum::DoubleValue(f.into_inner())),
        Value::Bytes(_) => value
            .as_str()
            .map(|s| OtelAnyValueEnum::StringValue(s.to_string())),
        Value::Regex(r) => Some(OtelAnyValueEnum::StringValue(r.to_string())),
        Value::Timestamp(ts) => Some(OtelAnyValueEnum::StringValue(ts.to_rfc3339())),
        Value::Array(arr) => Some(OtelAnyValueEnum::ArrayValue(ArrayValue { values: arr.iter().map(to_any_value).collect() })),
        Value::Object(map) => Some(convert_vrl_object_map_to_kvlistvalue(map)),
    };

    OtelAnyValueStruct { value }
}

#[cfg(test)]
mod tests {
    use crate::logs::log_events_to_resource_logs;
    use crate::logs::DateTime;
    use chrono::Utc;
    use std::collections::BTreeMap;
    use vector_core::event::LogEvent;
    use vrl::btreemap;

    fn group_key(name: &str, version: &str) -> (String, String) {
        (name.to_string(), version.to_string())
    }

    pub fn make_event(scope_name: &str, scope_version: &str) -> LogEvent {
        let attributes = btreemap! {"id" => 1 };
        let resources = btreemap! {"service.name" => "opentelemetry-logs"};
        let scope = btreemap! { "name" => scope_name, "version" => scope_version };

        LogEvent::from(btreemap! {
            "message" => "[X] WARN Some log",
            "severity_text" => "WARN",
            "timestamp" => "2025-06-06T06:06:06Z".parse::<DateTime<Utc>>().unwrap(),
            "observed_timestamp" => "2025-06-06T07:06:06Z".parse::<DateTime<Utc>>().unwrap(),
            "attributes" => attributes,
            "resources" => resources,
            "scope" => scope,
            "source_type" => "opentelemetry",
        })
    }

    #[test]
    fn test_logevent_to_resource_logs() {
        let resource_logs = log_events_to_resource_logs(vec![
            make_event("s1", "1"),
            make_event("s1", "1"),
            make_event("s2", "1"),
            make_event("s2", "2"),
            make_event("s3", "1"),
        ]);

        println!("{:?}", resource_logs);

        let scope_logs = &resource_logs.scope_logs;
        assert_eq!(scope_logs.len(), 4, "should be 4 unique scope groups");

        let groups = scope_logs
            .iter()
            .map(|s| {
                let scope = s.scope.as_ref().unwrap();
                ((scope.name.clone(), scope.version.clone()), s.log_records.len())
            })
            .collect::<BTreeMap<_, _>>();

        assert_eq!(groups.get(&group_key("s1", "1")).unwrap(), &2);
        assert_eq!(groups.get(&group_key("s2", "1")), Some(&1));
        assert_eq!(groups.get(&group_key("s2", "2")), Some(&1));
        assert_eq!(groups.get(&group_key("s3", "1")), Some(&1));
    }
}
