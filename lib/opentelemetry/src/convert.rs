use std::collections::BTreeMap;
use bytes::Bytes;
use ordered_float::NotNan;
use super::{
    Logs::{
        InstrumentationLibraryLogs,
        ScopeLogs,
        ResourceLogs,
        LogRecord,
    },
    Common::{
        InstrumentationScope,
        any_value::Value as PBValue,
        KeyValue,
    },
    Resource as OtelResource,
};
use vector_core::{
    event::{
        Event,
        LogEvent,
    },
    config::log_schema,
};
use value::Value;

impl From<InstrumentationLibraryLogs> for ScopeLogs {
    fn from(v: InstrumentationLibraryLogs) -> Self {
        Self {
            scope: v.instrumentation_library.map(|v| InstrumentationScope{
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
    fn into_iter(self) -> Self::IntoIter {
        let resource = self.resource;
        // convert instrumentation_library_logs(deprecated) into scope_logs
        let scope_logs: Vec<ScopeLogs> = if !self.scope_logs.is_empty() {
            self.scope_logs
        } else {
            self.instrumentation_library_logs
                .into_iter().map(ScopeLogs::from)
                .collect()
        };

        scope_logs.into_iter()
            .map(|scope_log| scope_log.log_records).flatten()
            .map(|log_record| ResourceLog{
                resource: resource.clone(),
                log_record,
            }.into())
            .collect::<Vec<Event>>().into_iter()
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
            PBValue::ArrayValue(arr) => {
                Value::Array(arr.values.into_iter()
                    .filter_map(|av| av.value)
                    .map(|v| v.into())
                    .collect::<Vec<Value>>())
            },
            PBValue::KvlistValue(arr) => {
                kvlist_2_value(arr.values)
            }
        }
    }
}

struct ResourceLog {
    resource: Option<OtelResource>,
    log_record: LogRecord,
}

fn kvlist_2_value(arr: Vec<KeyValue>) -> Value {
    Value::Object(arr.into_iter()
        .filter_map(|kv| kv.value.map(|av| (kv.key, av)))
        .fold(BTreeMap::default(), |mut acc, (k, av)| {
            av.value.map(|v| {
                acc.insert(k, v.into());
            });
            acc
        }))
}

impl From<ResourceLog> for Event {
    fn from(rl: ResourceLog) -> Self {
        let mut le = LogEvent::default();
        // resource
        rl.resource.map(|resource| {
            le.insert("resources",kvlist_2_value(resource.attributes));
        });
        le.insert("attributes", kvlist_2_value(rl.log_record.attributes));
        rl.log_record.body.and_then(|av| av.value).map(|v| {
            le.insert(log_schema().message_key(), v);
        });
        le.insert(log_schema().timestamp_key(), rl.log_record.time_unix_nano as i64);
        le.insert("trace_id", Value::Bytes(Bytes::from(hex::encode(rl.log_record.trace_id))));
        le.insert("span_id", Value::Bytes(Bytes::from(hex::encode(rl.log_record.span_id))));
        le.insert("severity_text", rl.log_record.severity_text);
        le.insert("severity_number", rl.log_record.severity_number as i64);
        le.into()
    }
}