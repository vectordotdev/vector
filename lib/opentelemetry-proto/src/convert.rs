use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use lookup::path;
use ordered_float::NotNan;
use vector_core::{
    config::{log_schema, LegacyKey, LogNamespace},
    event::{Event, LogEvent},
};
use vrl::value::{ObjectMap, Value};

use super::proto::{
    common::v1::{any_value::Value as PBValue, KeyValue},
    logs::v1::{LogRecord, ResourceLogs, SeverityNumber},
    resource::v1::Resource,
};

const SOURCE_NAME: &str = "opentelemetry";

pub const RESOURCE_KEY: &str = "resources";
pub const ATTRIBUTES_KEY: &str = "attributes";
pub const TRACE_ID_KEY: &str = "trace_id";
pub const SPAN_ID_KEY: &str = "span_id";
pub const SEVERITY_TEXT_KEY: &str = "severity_text";
pub const SEVERITY_NUMBER_KEY: &str = "severity_number";
pub const OBSERVED_TIMESTAMP_KEY: &str = "observed_timestamp";
pub const DROPPED_ATTRIBUTES_COUNT_KEY: &str = "dropped_attributes_count";
pub const FLAGS_KEY: &str = "flags";

impl ResourceLogs {
    pub fn into_event_iter(self, log_namespace: LogNamespace) -> impl Iterator<Item = Event> {
        let resource = self.resource;
        let now = Utc::now();

        self.scope_logs
            .into_iter()
            .flat_map(|scope_log| scope_log.log_records)
            .map(move |log_record| {
                ResourceLog {
                    resource: resource.clone(),
                    log_record,
                }
                .into_event(log_namespace, now)
            })
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
                kv.value.map(|av| {
                    (
                        kv.key.into(),
                        av.value.map(Into::into).unwrap_or(Value::Null),
                    )
                })
            })
            .collect::<ObjectMap>(),
    )
}

// https://github.com/open-telemetry/opentelemetry-specification/blob/v1.15.0/specification/logs/data-model.md
impl ResourceLog {
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
                Bytes::from(hex::encode(self.log_record.trace_id)),
            );
        }
        if !self.log_record.span_id.is_empty() {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(SPAN_ID_KEY))),
                path!(SPAN_ID_KEY),
                Bytes::from(hex::encode(self.log_record.span_id)),
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
