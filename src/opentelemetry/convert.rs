use super::{
    Common::{any_value::Value as PBValue, KeyValue},
    Logs::{LogRecord, ResourceLogs, SeverityNumber},
    Resource as OtelResource,
};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
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
const OBSERVED_TIMESTAMP_KEY: &str = "observed_timestamp";
const DROPPED_ATTRIBUTES_COUNT_KEY: &str = "dropped_attributes_count";
const FLAGS_KEY: &str = "flags";

impl IntoIterator for ResourceLogs {
    type Item = Event;
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        let resource = self.resource;
        self.scope_logs
            .into_iter()
            .flat_map(|scope_log| scope_log.log_records)
            .map(|log_record| {
                ResourceLog {
                    resource: resource.clone(),
                    log_record,
                }
                .into()
            })
            .collect::<Vec<Self::Item>>()
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

        // optional fields
        if let Some(resource) = rl.resource {
            if !resource.attributes.is_empty() {
                le.insert(RESOURCE_KEY, kv_list_into_value(resource.attributes));
            }
        }
        if !rl.log_record.attributes.is_empty() {
            le.insert(ATTRIBUTES_KEY, kv_list_into_value(rl.log_record.attributes));
        }
        if let Some(v) = rl.log_record.body.and_then(|av| av.value) {
            le.insert(log_schema().message_key(), v);
        }
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
        if !rl.log_record.severity_text.is_empty() {
            le.insert(SEVERITY_TEXT_KEY, rl.log_record.severity_text);
        }
        if rl.log_record.severity_number != SeverityNumber::Unspecified as i32 {
            le.insert(SEVERITY_NUMBER_KEY, rl.log_record.severity_number);
        }
        if rl.log_record.flags > 0 {
            le.insert(FLAGS_KEY, rl.log_record.flags);
        }

        // according to proto, if observed_time_unix_nano is missing, collector should set it
        let observed_timestamp = if rl.log_record.observed_time_unix_nano > 0 {
            Utc.timestamp_nanos(rl.log_record.observed_time_unix_nano as i64)
                .into()
        } else {
            Value::Timestamp(Utc::now())
        };
        le.insert(OBSERVED_TIMESTAMP_KEY, observed_timestamp.clone());

        // If time_unix_nano is not present (0 represents missing or unknown timestamp) use observed time
        let timestamp = if rl.log_record.time_unix_nano > 0 {
            Utc.timestamp_nanos(rl.log_record.time_unix_nano as i64)
                .into()
        } else {
            observed_timestamp
        };
        le.insert(log_schema().timestamp_key(), timestamp);

        le.insert(
            DROPPED_ATTRIBUTES_COUNT_KEY,
            rl.log_record.dropped_attributes_count,
        );

        le.insert(log_schema().source_type_key(), Bytes::from("opentelemetry"));

        le.into()
    }
}
