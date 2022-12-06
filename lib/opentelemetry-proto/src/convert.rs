use super::proto::{
    common::v1::{any_value::Value as PBValue, KeyValue},
    logs::v1::{LogRecord, ResourceLogs, SeverityNumber},
    resource::v1::Resource,
};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use ordered_float::NotNan;
use std::collections::BTreeMap;
use value::Value;
use vector_core::config::LogNamespace;
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

impl ResourceLogs {
    pub fn into_iter(self, log_namespace: LogNamespace) -> impl Iterator<Item = Event> {
        let resource = self.resource;
        self.scope_logs
            .into_iter()
            .flat_map(|scope_log| scope_log.log_records)
            .map(move |log_record| {
                ResourceLog {
                    resource: resource.clone(),
                    log_record,
                }
                .into(log_namespace)
            })
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
    resource: Option<Resource>,
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

impl ResourceLog {
    fn into(self: Self, log_namespace: LogNamespace) -> Event {
        let mut log = LogEvent::default();

        match log_namespace {
            LogNamespace::Vector => log_namespace.insert_standard_vector_source_metadata(
                &mut log,
                "opentelemetry",
                Utc::now(),
            ),
            LogNamespace::Legacy => {
                // optional fields
                if let Some(resource) = self.resource {
                    if !resource.attributes.is_empty() {
                        log.insert(RESOURCE_KEY, kv_list_into_value(resource.attributes));
                    }
                }
                if !self.log_record.attributes.is_empty() {
                    log.insert(
                        ATTRIBUTES_KEY,
                        kv_list_into_value(self.log_record.attributes),
                    );
                }
                if let Some(v) = self.log_record.body.and_then(|av| av.value) {
                    log.insert(log_schema().message_key(), v);
                }
                if !self.log_record.trace_id.is_empty() {
                    log.insert(
                        TRACE_ID_KEY,
                        Value::Bytes(Bytes::from(hex::encode(self.log_record.trace_id))),
                    );
                }
                if !self.log_record.span_id.is_empty() {
                    log.insert(
                        SPAN_ID_KEY,
                        Value::Bytes(Bytes::from(hex::encode(self.log_record.span_id))),
                    );
                }
                if !self.log_record.severity_text.is_empty() {
                    log.insert(SEVERITY_TEXT_KEY, self.log_record.severity_text);
                }
                if self.log_record.severity_number != SeverityNumber::Unspecified as i32 {
                    log.insert(SEVERITY_NUMBER_KEY, self.log_record.severity_number);
                }
                if self.log_record.flags > 0 {
                    log.insert(FLAGS_KEY, self.log_record.flags);
                }

                // according to proto, if observed_time_unix_nano is missing, collector should set it
                let observed_timestamp = if self.log_record.observed_time_unix_nano > 0 {
                    Utc.timestamp_nanos(self.log_record.observed_time_unix_nano as i64)
                        .into()
                } else {
                    Value::Timestamp(Utc::now())
                };
                log.insert(OBSERVED_TIMESTAMP_KEY, observed_timestamp.clone());

                // If time_unix_nano is not present (0 represents missing or unknown timestamp) use observed time
                let timestamp = if self.log_record.time_unix_nano > 0 {
                    Utc.timestamp_nanos(self.log_record.time_unix_nano as i64)
                        .into()
                } else {
                    observed_timestamp
                };
                log.insert(log_schema().timestamp_key(), timestamp);

                log.insert(
                    DROPPED_ATTRIBUTES_COUNT_KEY,
                    self.log_record.dropped_attributes_count,
                );

                log.insert(log_schema().source_type_key(), Bytes::from("opentelemetry"));
            }
        }
        log.into()
    }
}
