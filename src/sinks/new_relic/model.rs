use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    fmt::Debug,
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use serde::{Deserialize, Serialize};

use super::NewRelicSinkError;
use crate::event::{Event, MetricKind, MetricValue, Value};

#[derive(Debug)]
pub enum NewRelicApiModel {
    Metrics(MetricsApiModel),
    Events(EventsApiModel),
    Logs(LogsApiModel),
}

type KeyValData = HashMap<String, Value>;
type DataStore = HashMap<String, Vec<KeyValData>>;

#[derive(Serialize, Deserialize, Debug)]
pub struct MetricsApiModel(pub Vec<DataStore>);

impl MetricsApiModel {
    pub fn new(metric_array: Vec<KeyValData>) -> Self {
        let mut metric_store = DataStore::new();
        metric_store.insert("metrics".to_owned(), metric_array);
        Self(vec![metric_store])
    }
}

impl TryFrom<Vec<Event>> for MetricsApiModel {
    type Error = NewRelicSinkError;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let metric_array: Vec<_> = buf_events
            .into_iter()
            .filter_map(|e| e.try_into_metric())
            .filter_map(|metric| {
                // Generate Value::Object() from BTreeMap<String, String>
                let (series, data, _) = metric.into_parts();

                let mut metric_data = KeyValData::new();

                // We only handle gauge and counter metrics
                // Extract value & type and set type-related attributes
                let (value, metric_type) = match (data.value, &data.kind, &data.time.interval_ms) {
                    (
                        MetricValue::Counter { value },
                        MetricKind::Incremental,
                        Some(interval_ms),
                    ) => {
                        metric_data.insert(
                            "interval.ms".to_owned(),
                            Value::from(interval_ms.get() as i64),
                        );
                        (value, "count")
                    }
                    (MetricValue::Counter { value }, MetricKind::Absolute, _) => (value, "gauge"),
                    (MetricValue::Gauge { value }, _, _) => (value, "gauge"),
                    _ => {
                        // Note that this includes incremental counters without an interval
                        return None;
                    }
                };

                // Set name, type, value, timestamp, and attributes
                metric_data.insert("name".to_owned(), Value::from(series.name.name));
                metric_data.insert("type".to_owned(), Value::from(metric_type));
                let value = match NotNan::new(value) {
                    Ok(value) => value,
                    Err(_) => {
                        return None;
                    }
                };
                metric_data.insert("value".to_owned(), Value::from(value));
                metric_data.insert(
                    "timestamp".to_owned(),
                    Value::from(
                        data.time
                            .timestamp
                            .unwrap_or_else(|| DateTime::<Utc>::from(SystemTime::now()))
                            .timestamp(),
                    ),
                );
                if let Some(tags) = series.tags {
                    metric_data.insert(
                        "attributes".to_owned(),
                        Value::from(
                            tags.iter_single()
                                .map(|(key, value)| (key.to_string(), Value::from(value)))
                                .collect::<BTreeMap<_, _>>(),
                        ),
                    );
                }

                Some(metric_data)
            })
            .collect();

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
        let mut events_array = vec![];
        for buf_event in buf_events {
            if let Event::Log(log) = buf_event {
                let mut event_model = KeyValData::new();
                for (k, v) in log.convert_to_fields() {
                    event_model.insert(k, v.clone());
                }

                if let Some(message) = log.get("message") {
                    let message = message.to_string_lossy().replace("\\\"", "\"");
                    // If message contains a JSON string, parse it and insert all fields into self
                    if let serde_json::Result::Ok(json_map) =
                        serde_json::from_str::<HashMap<String, serde_json::Value>>(&message)
                    {
                        for (k, v) in json_map {
                            match v {
                                serde_json::Value::String(s) => {
                                    event_model.insert(k, Value::from(s));
                                }
                                serde_json::Value::Number(n) => {
                                    if let Some(f) = n.as_f64() {
                                        event_model.insert(
                                            k,
                                            Value::from(NotNan::new(f).map_err(|_| {
                                                NewRelicSinkError::new("NaN value not supported")
                                            })?),
                                        );
                                    } else {
                                        event_model.insert(k, Value::from(n.as_i64()));
                                    }
                                }
                                serde_json::Value::Bool(b) => {
                                    event_model.insert(k, Value::from(b));
                                }
                                _ => {}
                            }
                        }
                        event_model.remove("message");
                    }
                }

                if event_model.get("eventType").is_none() {
                    event_model
                        .insert("eventType".to_owned(), Value::from("VectorSink".to_owned()));
                }

                events_array.push(event_model);
            }
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
        logs_store.insert("logs".to_owned(), logs_array);
        Self(vec![logs_store])
    }
}

impl TryFrom<Vec<Event>> for LogsApiModel {
    type Error = NewRelicSinkError;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut logs_array = vec![];
        for buf_event in buf_events {
            if let Event::Log(log) = buf_event {
                let mut log_model = KeyValData::new();
                for (k, v) in log.convert_to_fields() {
                    log_model.insert(k, v.clone());
                }
                if log.get("message").is_none() {
                    log_model.insert(
                        "message".to_owned(),
                        Value::from("log from vector".to_owned()),
                    );
                }
                logs_array.push(log_model);
            }
        }

        if !logs_array.is_empty() {
            Ok(Self::new(logs_array))
        } else {
            Err(NewRelicSinkError::new("No valid logs to generate"))
        }
    }
}
