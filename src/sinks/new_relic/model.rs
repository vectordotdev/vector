use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    fmt::Debug,
};

use chrono::Utc;
use ordered_float::NotNan;
use serde::Serialize;
use vector_lib::internal_event::{ComponentEventsDropped, INTENTIONAL, UNINTENTIONAL};
use vector_lib::{config::log_schema, event::ObjectMap};
use vrl::event_path;

use super::NewRelicSinkError;
use crate::event::{Event, MetricKind, MetricValue, Value};

#[derive(Debug)]
pub(super) enum NewRelicApiModel {
    Metrics(MetricsApiModel),
    Events(EventsApiModel),
    Logs(LogsApiModel),
}

/// The metrics API data model.
///
/// Reference: https://docs.newrelic.com/docs/data-apis/ingest-apis/metric-api/report-metrics-metric-api/
#[derive(Debug, Serialize)]
pub(super) struct MetricsApiModel(pub [MetricDataStore; 1]);

#[derive(Debug, Serialize)]
pub(super) struct MetricDataStore {
    pub metrics: Vec<MetricData>,
}

#[derive(Debug, Serialize)]
pub(super) struct MetricData {
    #[serde(rename = "interval.ms", skip_serializing_if = "Option::is_none")]
    pub interval_ms: Option<i64>,
    pub name: String,
    pub r#type: &'static str,
    pub value: f64,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<BTreeMap<String, String>>,
}

impl MetricsApiModel {
    pub(super) fn new(metrics: Vec<MetricData>) -> Self {
        Self([MetricDataStore { metrics }])
    }
}

impl TryFrom<Vec<Event>> for MetricsApiModel {
    type Error = NewRelicSinkError;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut num_non_metric_events = 0;
        let mut num_missing_interval = 0;
        let mut num_nan_value = 0;
        let mut num_unsupported_metric_type = 0;

        let metric_array: Vec<_> = buf_events
            .into_iter()
            .filter_map(|event| {
                let Some(metric) = event.try_into_metric() else {
                    num_non_metric_events += 1;
                    return None;
                };

                // Generate Value::Object() from BTreeMap<String, String>
                let (series, data, _) = metric.into_parts();

                // We only handle gauge and counter metrics
                // Extract value & type and set type-related attributes
                let (value, metric_type, interval_ms) = match (data.value, &data.kind) {
                    (MetricValue::Counter { value }, MetricKind::Incremental) => {
                        let Some(interval_ms) = data.time.interval_ms else {
                            // Incremental counter without an interval is worthless, skip this metric
                            num_missing_interval += 1;
                            return None;
                        };
                        (value, "count", Some(interval_ms.get() as i64))
                    }
                    (MetricValue::Counter { value }, MetricKind::Absolute)
                    | (MetricValue::Gauge { value }, _) => (value, "gauge", None),
                    _ => {
                        // Unsupported metric type
                        num_unsupported_metric_type += 1;
                        return None;
                    }
                };

                // Set name, type, value, timestamp, and attributes
                if value.is_nan() {
                    num_nan_value += 1;
                    return None;
                };

                let timestamp = data.time.timestamp.unwrap_or_else(Utc::now);
                Some(MetricData {
                    interval_ms,
                    name: series.name.name,
                    r#type: metric_type,
                    value,
                    timestamp: timestamp.timestamp_millis(),
                    attributes: series.tags.map(|tags| tags.into_iter_single().collect()),
                })
            })
            .collect();

        if num_non_metric_events > 0 {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: num_non_metric_events,
                reason: "non-metric event"
            });
        }
        if num_unsupported_metric_type > 0 {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: num_unsupported_metric_type,
                reason: "unsupported metric type"
            });
        }
        if num_nan_value > 0 {
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: num_nan_value,
                reason: "NaN value not supported"
            });
        }
        if num_missing_interval > 0 {
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: num_missing_interval,
                reason: "incremental counter missing interval"
            });
        }

        if !metric_array.is_empty() {
            Ok(Self::new(metric_array))
        } else {
            Err(NewRelicSinkError::new("No valid metrics to generate"))
        }
    }
}

/// The events API data mode.
///
/// Reference: https://docs.newrelic.com/docs/data-apis/ingest-apis/event-api/introduction-event-api/
#[derive(Debug, Serialize)]
pub(super) struct EventsApiModel(pub Vec<ObjectMap>);

impl EventsApiModel {
    pub(super) fn new(events_array: Vec<ObjectMap>) -> Self {
        Self(events_array)
    }
}

impl TryFrom<Vec<Event>> for EventsApiModel {
    type Error = NewRelicSinkError;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut num_non_log_events = 0;
        let mut num_nan_value = 0;

        let events_array: Vec<ObjectMap> = buf_events
            .into_iter()
            .filter_map(|event| {
                let Some(log) = event.try_into_log() else {
                    num_non_log_events += 1;
                    return None;
                };

                let mut event_model = ObjectMap::new();
                for (k, v) in log.convert_to_fields_unquoted() {
                    event_model.insert(k, v.clone());
                }

                if let Some(message) = log.get(event_path!("message")) {
                    let message = message.to_string_lossy().replace("\\\"", "\"");
                    // If message contains a JSON string, parse it and insert all fields into self
                    if let serde_json::Result::Ok(json_map) =
                        serde_json::from_str::<HashMap<String, serde_json::Value>>(&message)
                    {
                        for (k, v) in json_map {
                            match v {
                                serde_json::Value::String(s) => {
                                    event_model.insert(k.into(), Value::from(s));
                                }
                                serde_json::Value::Number(n) => {
                                    if let Some(f) = n.as_f64() {
                                        event_model.insert(
                                            k.into(),
                                            Value::from(NotNan::new(f).ok().or_else(|| {
                                                num_nan_value += 1;
                                                None
                                            })?),
                                        );
                                    } else {
                                        event_model.insert(k.into(), Value::from(n.as_i64()));
                                    }
                                }
                                serde_json::Value::Bool(b) => {
                                    event_model.insert(k.into(), Value::from(b));
                                }
                                _ => {
                                    // Note that arrays and nested objects are silently dropped.
                                }
                            }
                        }
                        event_model.remove("message");
                    }
                }

                if !event_model.contains_key("eventType") {
                    event_model.insert("eventType".into(), Value::from("VectorSink".to_owned()));
                }

                Some(event_model)
            })
            .collect();

        if num_non_log_events > 0 {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: num_non_log_events,
                reason: "non-log event"
            });
        }
        if num_nan_value > 0 {
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: num_nan_value,
                reason: "NaN value not supported"
            });
        }

        if !events_array.is_empty() {
            Ok(Self::new(events_array))
        } else {
            Err(NewRelicSinkError::new("No valid events to generate"))
        }
    }
}

/// The logs API data model.
///
/// Reference: https://docs.newrelic.com/docs/logs/log-api/introduction-log-api/
#[derive(Serialize, Debug)]
pub(super) struct LogsApiModel(pub [LogDataStore; 1]);

#[derive(Serialize, Debug)]
pub(super) struct LogDataStore {
    pub logs: Vec<LogMessage>,
}

#[derive(Debug, PartialEq, Serialize)]
pub(super) struct LogMessage {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<Timestamp>,
    pub attributes: ObjectMap,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub(super) enum Timestamp {
    Numeric(i64),
    String(String),
}

impl LogsApiModel {
    pub(super) fn new(logs: Vec<LogMessage>) -> Self {
        Self([LogDataStore { logs }])
    }
}

impl TryFrom<Vec<Event>> for LogsApiModel {
    type Error = NewRelicSinkError;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut num_non_log_events = 0;
        let mut num_non_object_events = 0;
        let message_key = log_schema().message_key_target_path().unwrap();
        let timestamp_key = log_schema().timestamp_key_target_path().unwrap();

        let logs_array: Vec<LogMessage> = buf_events
            .into_iter()
            .filter_map(|event| {
                let Some(mut log) = event.try_into_log() else {
                    num_non_log_events += 1;
                    return None;
                };

                let message = get_message_string(log.remove(message_key));
                let timestamp = log.remove(timestamp_key).and_then(map_timestamp_value);

                // We convert the log event into a logs API model simply by transmuting the type
                // wrapper and dropping all arrays, which are not supported by the API. We could
                // flatten out the keys, as this is what New Relic does internally, and we used to
                // do that, but the flattening iterator accessed through
                // `LogEvent::convert_to_fields` adds quotes to dotted fields names, which produces
                // broken attributes in New Relic, and nesting objects is actually a (slightly) more
                // efficient representation of the key names.
                let (value, _metadata) = log.into_parts();
                let Some(mut attributes) = value.into_object() else {
                    num_non_object_events += 1;
                    return None;
                };
                strip_arrays(&mut attributes);

                Some(LogMessage {
                    message,
                    timestamp,
                    attributes,
                })
            })
            .collect();

        if num_non_log_events > 0 {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: num_non_log_events,
                reason: "non-log event",
            });
        }
        if num_non_object_events > 0 {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: num_non_object_events,
                reason: "non-object event",
            });
        }

        if !logs_array.is_empty() {
            Ok(Self::new(logs_array))
        } else {
            Err(NewRelicSinkError::new("No valid logs to generate"))
        }
    }
}

const MILLISECONDS: f64 = 1000.0;

/// Convert a value into a timestamp value. New Relic accepts either milliseconds or seconds since
/// epoch as an integer, or ISO8601-formatted timestamp as a string.
///
/// Reference: https://docs.newrelic.com/docs/logs/log-api/introduction-log-api/#json-logs
fn map_timestamp_value(value: Value) -> Option<Timestamp> {
    match value {
        Value::Timestamp(t) => Some(Timestamp::Numeric(t.timestamp_millis())),
        Value::Integer(n) => Some(Timestamp::Numeric(n)),
        Value::Float(f) => Some(Timestamp::Numeric((f.into_inner() * MILLISECONDS) as i64)),
        Value::Bytes(b) => Some(Timestamp::String(
            String::from_utf8_lossy(b.as_ref()).into(),
        )),
        _ => None,
    }
}

fn get_message_string(value: Option<Value>) -> String {
    match value {
        Some(Value::Bytes(bytes)) => String::from_utf8_lossy(bytes.as_ref()).into(),
        Some(value) => value.to_string(),
        None => "log from vector".to_string(),
    }
}

fn strip_arrays(obj: &mut ObjectMap) {
    obj.retain(|_key, value| !value.is_array());
    obj.iter_mut().for_each(|(_key, value)| {
        if let Some(obj) = value.as_object_mut() {
            strip_arrays(obj);
        }
    });
}
