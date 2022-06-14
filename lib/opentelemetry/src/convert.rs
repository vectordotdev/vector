use super::{
    Common::{any_value::Value as PBValue, InstrumentationScope, KeyValue},
    Logs::{InstrumentationLibraryLogs, LogRecord, ResourceLogs, ScopeLogs},
    Resource as OtelResource,
};
use bytes::Bytes;
use ordered_float::NotNan;
use std::collections::BTreeMap;
use value::Value;
use vector_core::{
    config::log_schema,
    event::{Event, LogEvent},
};

const RESOURCE_KEY: &str = "resources";
const ATTRIBUTES_KEY: &str = "attributes";
const TRACE_ID_KEY: &str = "trace_id";
const SPAN_ID_KEY: &str = "span_id";
const SEVERITY_TEXT_KEY: &str = "severity_text";
const SEVERITY_NUMBER_KEY: &str = "severity_number";

impl From<InstrumentationLibraryLogs> for ScopeLogs {
    fn from(v: InstrumentationLibraryLogs) -> Self {
        Self {
            scope: v.instrumentation_library.map(|v| InstrumentationScope {
                name: v.name,
                version: v.version,
            }),
            log_records: v.log_records,
            schema_url: v.schema_url,
        }
    }
}

impl IntoIterator for ResourceLogs {
    type Item = Event;
    type IntoIter = std::vec::IntoIter<Self::Item>;
    #[allow(deprecated)]
    fn into_iter(self) -> Self::IntoIter {
        let resource = self.resource;
        // convert instrumentation_library_logs(deprecated) into scope_logs
        let scope_logs: Vec<ScopeLogs> = if !self.scope_logs.is_empty() {
            self.scope_logs
        } else {
            self.instrumentation_library_logs
                .into_iter()
                .map(ScopeLogs::from)
                .collect()
        };

        scope_logs
            .into_iter()
            .map(|scope_log| scope_log.log_records)
            .flatten()
            .map(|log_record| {
                ResourceLog {
                    resource: resource.clone(),
                    log_record,
                }
                .into()
            })
            .collect::<Vec<Event>>()
            .into_iter()
    }
}

impl From<PBValue> for Value {
    fn from(av: PBValue) -> Self {
        match av {
            PBValue::StringValue(v) => Value::Bytes(Bytes::from(v)),
            PBValue::BoolValue(v) => Value::Boolean(v),
            PBValue::IntValue(v) => Value::Integer(v),
            PBValue::DoubleValue(v) => Value::Float(NotNan::new(v).unwrap()),
            PBValue::BytesValue(v) => Value::Bytes(Bytes::from(v)),
            PBValue::ArrayValue(arr) => Value::Array(
                arr.values
                    .into_iter()
                    .map(|av| av.value.map(Into::into).unwrap_or(Value::Null))
                    .collect::<Vec<Value>>(),
            ),
            PBValue::KvlistValue(arr) => kv_list_into_value(arr.values),
        }
    }
}

struct ResourceLog {
    resource: Option<OtelResource>,
    log_record: LogRecord,
}

fn kv_list_into_value(arr: Vec<KeyValue>) -> Value {
    Value::Object(
        arr.into_iter()
            .filter_map(|kv| {
                kv.value
                    .map(|av| (kv.key, av.value.map(Into::into).unwrap_or(Value::Null)))
            })
            .collect::<BTreeMap<String, Value>>(),
    )
}

impl From<ResourceLog> for Event {
    fn from(rl: ResourceLog) -> Self {
        let mut le = LogEvent::default();
        if let Some(resource) = rl.resource {
            le.insert(RESOURCE_KEY, kv_list_into_value(resource.attributes));
        }
        if !rl.log_record.attributes.is_empty() {
            le.insert(ATTRIBUTES_KEY, kv_list_into_value(rl.log_record.attributes));
        }
        if let Some(v) = rl.log_record.body.and_then(|av| av.value) {
            le.insert(log_schema().message_key(), v);
        }
        le.insert(
            log_schema().timestamp_key(),
            rl.log_record.time_unix_nano as i64,
        );
        if !rl.log_record.trace_id.is_empty() {
            le.insert(
                TRACE_ID_KEY,
                Value::Bytes(Bytes::from(hex::encode(rl.log_record.trace_id))),
            );
        }
        if !rl.log_record.span_id.is_empty() {
            le.insert(
                SPAN_ID_KEY,
                Value::Bytes(Bytes::from(hex::encode(rl.log_record.span_id))),
            );
        }
        le.insert(SEVERITY_TEXT_KEY, rl.log_record.severity_text);
        le.insert(SEVERITY_NUMBER_KEY, rl.log_record.severity_number as i64);
        le.into()
    }
}
