use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    fmt::Debug,
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use serde::{Deserialize, Serialize};
use vector_lib::internal_event::{ComponentEventsDropped, INTENTIONAL, UNINTENTIONAL};
use vrl::event_path;

use super::NewRelicSinkError;
use crate::event::{Event, KeyString, MetricKind, MetricValue, Value};

#[derive(Debug)]
pub enum NewRelicApiModel {
    Metrics(MetricsApiModel),
    Events(EventsApiModel),
    Logs(LogsApiModel),
}

type KeyValData = HashMap<KeyString, Value>;
type DataStore = HashMap<KeyString, Vec<KeyValData>>;

#[derive(Serialize, Deserialize, Debug)]
pub struct MetricsApiModel(pub Vec<DataStore>);

impl MetricsApiModel {
    pub fn new(metric_array: Vec<KeyValData>) -> Self {
        let mut metric_store = DataStore::new();
        metric_store.insert("metrics".into(), metric_array);
        Self(vec![metric_store])
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

                let mut metric_data = KeyValData::new();

                // We only handle gauge and counter metrics
                // Extract value & type and set type-related attributes
                let (value, metric_type) = match (data.value, &data.kind) {
                    (MetricValue::Counter { value }, MetricKind::Incremental) => {
                        let Some(interval_ms) = data.time.interval_ms else {
                            // Incremental counter without an interval is worthless, skip this metric
                            num_missing_interval += 1;
                            return None;
                        };
                        metric_data
                            .insert("interval.ms".into(), Value::from(interval_ms.get() as i64));
                        (value, "count")
                    }
                    (MetricValue::Counter { value }, MetricKind::Absolute) => (value, "gauge"),
                    (MetricValue::Gauge { value }, _) => (value, "gauge"),
                    _ => {
                        // Unsupported metric type
                        num_unsupported_metric_type += 1;
                        return None;
                    }
                };

                // Set name, type, value, timestamp, and attributes
                metric_data.insert("name".into(), Value::from(series.name.name));
                metric_data.insert("type".into(), Value::from(metric_type));
                let Some(value) = NotNan::new(value).ok() else {
                    num_nan_value += 1;
                    return None;
                };
                metric_data.insert("value".into(), Value::from(value));
                metric_data.insert(
                    "timestamp".into(),
                    Value::from(
                        data.time
                            .timestamp
                            .unwrap_or_else(|| DateTime::<Utc>::from(SystemTime::now()))
                            .timestamp(),
                    ),
                );
                if let Some(tags) = series.tags {
                    metric_data.insert(
                        "attributes".into(),
                        Value::from(
                            tags.iter_single()
                                .map(|(key, value)| (key.into(), Value::from(value)))
                                .collect::<BTreeMap<_, _>>(),
                        ),
                    );
                }

                Some(metric_data)
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

#[derive(Serialize, Deserialize, Debug)]
pub struct EventsApiModel(pub Vec<KeyValData>);

impl EventsApiModel {
    pub fn new(events_array: Vec<KeyValData>) -> Self {
        Self(events_array)
    }
}

impl TryFrom<Vec<Event>> for EventsApiModel {
    type Error = NewRelicSinkError;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut num_non_log_events = 0;
        let mut num_nan_value = 0;

        let events_array: Vec<HashMap<KeyString, Value>> = buf_events
            .into_iter()
            .filter_map(|event| {
                let Some(log) = event.try_into_log() else {
                    num_non_log_events += 1;
                    return None;
                };

                let mut event_model = KeyValData::new();
                for (k, v) in log.convert_to_fields() {
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

                if event_model.get("eventType").is_none() {
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

#[derive(Serialize, Deserialize, Debug)]
pub struct LogsApiModel(pub Vec<DataStore>);

impl LogsApiModel {
    pub fn new(logs_array: Vec<KeyValData>) -> Self {
        let mut logs_store = DataStore::new();
        logs_store.insert("logs".into(), logs_array);
        Self(vec![logs_store])
    }
}

impl TryFrom<Vec<Event>> for LogsApiModel {
    type Error = NewRelicSinkError;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut num_non_log_events = 0;

        let logs_array: Vec<HashMap<KeyString, Value>> = buf_events
            .into_iter()
            .filter_map(|event| {
                let Some(log) = event.try_into_log() else {
                    num_non_log_events += 1;
                    return None;
                };

                let mut log_model = KeyValData::new();
                for (k, v) in log.convert_to_fields() {
                    log_model.insert(k, v.clone());
                }
                if log.get(event_path!("message")).is_none() {
                    log_model.insert("message".into(), Value::from("log from vector".to_owned()));
                }

                Some(log_model)
            })
            .collect();

        if num_non_log_events > 0 {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: num_non_log_events,
                reason: "non-log event"
            });
        }

        if !logs_array.is_empty() {
            Ok(Self::new(logs_array))
        } else {
            Err(NewRelicSinkError::new("No valid logs to generate"))
        }
    }
}
