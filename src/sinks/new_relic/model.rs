use crate::event::{
    Event, Value, MetricValue
};
use serde::{
    Deserialize, Serialize
};
use chrono::{
    DateTime, Utc
};
use std::{
    fmt::Debug,
    collections::HashMap,
    convert::TryFrom,
    time::SystemTime,
};
use super::NewRelicSinkError;

#[derive(Debug)]
pub enum NewRelicApiModel {
    Metrics(MetricsApiModel),
    Events(EventsApiModel),
    Logs(LogsApiModel)
}

type KeyValData = HashMap<String, Value>;
type DataStore = HashMap<String, Vec<KeyValData>>;

#[derive(Serialize, Deserialize, Debug)]
pub struct MetricsApiModel(pub Vec<DataStore>);

impl MetricsApiModel {
    pub fn new(metric_array: Vec<(Value, Value, Value)>) -> Self {
        let mut metric_data_array = vec!();
        for (m_name, m_value, m_timestamp) in metric_array {
            let mut metric_data = KeyValData::new();
            metric_data.insert("name".to_owned(), m_name);
            metric_data.insert("value".to_owned(), m_value);
            match m_timestamp {
                Value::Timestamp(ts) => { metric_data.insert("timestamp".to_owned(), Value::from(ts.timestamp())); },
                Value::Integer(i) => { metric_data.insert("timestamp".to_owned(), Value::from(i)); },
                _ => { metric_data.insert("timestamp".to_owned(), Value::from(DateTime::<Utc>::from(SystemTime::now()).timestamp())); }
            }
            metric_data_array.push(metric_data);
        }
        let mut metric_store = DataStore::new();
        metric_store.insert("metrics".to_owned(), metric_data_array);
        Self(vec!(metric_store))
    }
}

impl TryFrom<Vec<Event>> for MetricsApiModel {
    type Error = NewRelicSinkError;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut metric_array = vec!();

        for buf_event in buf_events {
            match buf_event {
                Event::Metric(metric) => {
                    // Future improvement: put metric type. If type = count, NR metric model requiere an interval.ms field, that is not provided by the Vector Metric model.
                    match metric.value() {
                        MetricValue::Gauge { value } => {
                            metric_array.push((
                                Value::from(metric.name().to_owned()),
                                Value::from(*value),
                                Value::from(metric.timestamp())
                            ));
                        },
                        MetricValue::Counter { value } => {
                            metric_array.push((
                                Value::from(metric.name().to_owned()),
                                Value::from(*value),
                                Value::from(metric.timestamp())
                            ));
                        },
                        _ => {
                            // Unrecognized metric type
                        }
                    }
                },
                _ => {
                    // Unrecognized event type
                }
            }
        }

        if metric_array.len() > 0 {
            Ok(MetricsApiModel::new(metric_array))
        }
        else {
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
        let mut events_array = vec!();
        for buf_event in buf_events {
            match buf_event {
                Event::Log(log) => {
                    let mut event_model = KeyValData::new();
                    for (k, v) in log.all_fields() {
                        event_model.insert(k, v.clone());
                    }

                    if let Some(message) = log.get("message") {
                        let message = message.to_string_lossy().replace("\\\"", "\"");
                        // If message contains a JSON string, parse it and insert all fields into self
                        if let serde_json::Result::Ok(json_map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&message) {
                            for (k, v) in json_map {
                                match v {
                                    serde_json::Value::String(s) => {
                                        event_model.insert(k, Value::from(s));
                                    },
                                    serde_json::Value::Number(n) => {
                                        if n.is_f64() {
                                            event_model.insert(k, Value::from(n.as_f64()));
                                        }
                                        else {
                                            event_model.insert(k, Value::from(n.as_i64()));
                                        }
                                    },
                                    serde_json::Value::Bool(b) => {
                                        event_model.insert(k, Value::from(b));
                                    },
                                    _ => {}
                                }
                            }
                            event_model.remove("message");
                        }
                    }

                    if let None = event_model.get("eventType") {
                        event_model.insert("eventType".to_owned(), Value::from("VectorSink".to_owned()));
                    }

                    events_array.push(event_model);
                },
                _ => {
                    // Unrecognized event type
                }
            }
        }

        if events_array.len() > 0 {
            Ok(Self::new(events_array))
        }
        else {
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
        Self(vec!(logs_store))
    }
}

impl TryFrom<Vec<Event>> for LogsApiModel {
    type Error = NewRelicSinkError;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut logs_array = vec!();
        for buf_event in buf_events {
            match buf_event {
                Event::Log(log) => {
                    let mut log_model = KeyValData::new();
                    for (k, v) in log.all_fields() {
                        log_model.insert(k, v.clone());
                    }
                    if let None = log.get("message") {
                        log_model.insert("message".to_owned(), Value::from("log from vector".to_owned()));
                    }
                    logs_array.push(log_model);
                },
                _ => {
                    // Unrecognized event type
                }
            }
        }

        if logs_array.len() > 0 {
            Ok(Self::new(logs_array))
        }
        else {
            Err(NewRelicSinkError::new("No valid logs to generate"))
        }
    }
}
