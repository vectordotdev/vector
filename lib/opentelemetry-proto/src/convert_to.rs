use crate::proto::common::v1::any_value::Value as AnyValue_Value;
use crate::proto::common::v1::{AnyValue, ArrayValue, InstrumentationScope, KeyValue, KeyValueList};
use crate::proto::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use crate::proto::resource::v1::Resource;
use crate::types::{ATTRIBUTES_KEY, RESOURCE_KEY, SOURCE_NAME};
use vector_core::config::LogNamespace;
use vector_core::event::{Event, LogEvent};
use vrl::core::Value as VrlValue;
use vrl::{event_path, metadata_path};

impl ResourceLogs {
    pub fn from_event_iter<I>(events: I) -> Vec<Self>
    where
        I: IntoIterator<Item=Event>,
    {
        events.into_iter().filter_map(|event| Self::from_event(&event)).collect()
    }

    fn from_event(event: &Event) -> Option<Self> {
        let log_event = match event.maybe_as_log() {
            Some(log) => log,
            None => return None,
        };

        let schema_url = extract_schema_url(log_event);
        Some(Self {
            resource: extract_resource(log_event),
            scope_logs: vec![ScopeLogs {
                scope: extract_scope(log_event),
                log_records: extract_log_records(log_event),
                schema_url: schema_url.to_string(),
            }],
            schema_url: "".to_string(),
        })
    }
}

fn extract_schema_url(_: &LogEvent) -> String {
    "".to_string() // TODO
}

fn extract_resource(log_event: &LogEvent) -> Option<Resource> {
    match log_event.namespace() {
        LogNamespace::Vector => log_event
            .get(metadata_path!(SOURCE_NAME, RESOURCE_KEY))
            .and_then(|value| value_to_resource(value)),
        LogNamespace::Legacy => log_event
            .get(event_path!(RESOURCE_KEY))
            .and_then(|value| value_to_resource(value)),
    }
}

fn value_to_resource(value: &VrlValue) -> Option<Resource> {
    if let VrlValue::Object(map) = value {
        Some(Resource {
            attributes: map
                .iter()
                .map(|(key, value)| KeyValue {
                    key: key.to_string(),
                    value: vrl_value_to_any_value(value),
                })
                .collect(),
            dropped_attributes_count: 0,
        })
    } else {
        None
    }
}

fn extract_scope(_log_event: &LogEvent) -> Option<InstrumentationScope> {
    // If needed, implement scope extraction logic here
    None
}

fn extract_log_records(log_event: &LogEvent) -> Vec<LogRecord> {
    vec![LogRecord {
        time_unix_nano: extract_timestamp(log_event),
        observed_time_unix_nano: extract_observed_timestamp(log_event),
        severity_number: extract_severity_number(log_event),
        severity_text: extract_severity_text(log_event),
        body: extract_body(log_event),
        attributes: extract_attributes(log_event),
        dropped_attributes_count: 0, // Set as appropriate
        flags: 0, // Set as appropriate
        trace_id: vec![], // Set based on extraction logic
        span_id: vec![], // Set based on extraction logic
    }]
}

fn extract_timestamp(_log_event: &LogEvent) -> u64 {
    0 // Placeholder
}

fn extract_observed_timestamp(_log_event: &LogEvent) -> u64 {
    0 // Placeholder
}

fn extract_severity_number(_log_event: &LogEvent) -> i32 {
    0 // Placeholder
}

fn extract_severity_text(_log_event: &LogEvent) -> String {
    "".to_string() // Placeholder
}

fn extract_body(log_event: &LogEvent) -> Option<AnyValue> {
    log_event.get(event_path!("body")).and_then(vrl_value_to_any_value)
}

impl From<&VrlValue> for AnyValue {
    fn from(value: &VrlValue) -> Self {
        let converted_value = match value {
            VrlValue::Bytes(bytes) => Some(AnyValue_Value::BytesValue(bytes.clone().into())),
            VrlValue::Regex(v) => Some(AnyValue_Value::BytesValue(v.as_bytes().clone().into())),
            VrlValue::Integer(i) => Some(AnyValue_Value::IntValue(*i)),
            VrlValue::Float(f) => Some(AnyValue_Value::DoubleValue(f.into_inner())),
            VrlValue::Boolean(b) => Some(AnyValue_Value::BoolValue(*b)),
            VrlValue::Timestamp(ts) => Some(AnyValue_Value::StringValue(ts.to_rfc3339())),
            VrlValue::Object(map) => Some(AnyValue_Value::KvlistValue(KeyValueList {
                values: map
                    .iter()
                    .map(|(k, v)| KeyValue {
                        key: k.to_string(),
                        value: Some(v.into()), // Using the conversion here
                    })
                    .collect(),
            })),
            VrlValue::Array(arr) => Some(AnyValue_Value::ArrayValue(ArrayValue {
                values: arr.iter().filter_map(|v| Some(v.into())).collect(),
            })),
            VrlValue::Null => None,
        };

        AnyValue {
            value: converted_value,
        }
    }
}

fn vrl_value_to_any_value(value: &VrlValue) -> Option<AnyValue> {
    Some(AnyValue {
        value: match value {
            VrlValue::Bytes(bytes) => Some(AnyValue_Value::BytesValue(bytes.clone().into())),
            VrlValue::Regex(v) => Some(AnyValue_Value::BytesValue(v.as_bytes().clone().into())),
            VrlValue::Integer(i) => Some(AnyValue_Value::IntValue(*i)),
            VrlValue::Float(f) => Some(AnyValue_Value::DoubleValue(f.into_inner())),
            VrlValue::Boolean(b) => Some(AnyValue_Value::BoolValue(*b)),
            VrlValue::Timestamp(ts) => Some(AnyValue_Value::StringValue(ts.to_rfc3339())),
            VrlValue::Object(map) => Some(AnyValue_Value::KvlistValue(KeyValueList {
                values: map
                    .iter()
                    .map(|(k, v)| KeyValue {
                        key: k.to_string(),
                        value: vrl_value_to_any_value(v),
                    })
                    .collect(),
            })),
            VrlValue::Array(arr) => Some(AnyValue_Value::ArrayValue(ArrayValue {
                values: arr.iter().filter_map(vrl_value_to_any_value).collect(),
            })),
            VrlValue::Null => None,
        },
    })
}

fn extract_attributes(log_event: &LogEvent) -> Vec<KeyValue> {
    log_event
        .get(event_path!(ATTRIBUTES_KEY))
        .and_then(|value| {
            if let VrlValue::Object(map) = value {
                Some(
                    map.iter()
                        .map(|(key, val)| KeyValue {
                            key: key.to_string(),
                            value: vrl_value_to_any_value(val), // Reuse the existing conversion
                        })
                        .collect(),
                )
            } else {
                None
            }
        })
        .unwrap_or_default() // Return an empty Vec if no attributes found
}
