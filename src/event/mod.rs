use self::proto::{event_wrapper::Event as EventProto, metric::Value as MetricProto, Log};
use crate::config::log_schema;
use bytes::Bytes;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use std::collections::{BTreeMap, HashMap};

pub mod discriminant;
pub mod merge;
pub mod merge_state;
pub mod metric;
pub mod util;

mod log_event;
mod lookup;
mod value;

pub use log_event::LogEvent;
pub use lookup::Lookup;
pub use metric::{Metric, MetricKind, MetricValue, StatisticKind};
use std::convert::{TryFrom, TryInto};
pub(crate) use util::log::PathComponent;
pub(crate) use util::log::PathIter;
pub use value::Value;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/event.proto.rs"));
}

pub const PARTIAL: &str = "_partial";

#[derive(PartialEq, Debug, Clone)]
pub enum Event {
    Log(LogEvent),
    Metric(Metric),
}

impl Event {
    pub fn new_empty_log() -> Self {
        Event::Log(LogEvent::default())
    }

    pub fn as_log(&self) -> &LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    pub fn as_mut_log(&mut self) -> &mut LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    pub fn into_log(self) -> LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    pub fn as_metric(&self) -> &Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }

    pub fn as_mut_metric(&mut self) -> &mut Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }

    pub fn into_metric(self) -> Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }
}

fn timestamp_to_string(timestamp: &DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

fn decode_map(fields: BTreeMap<String, proto::Value>) -> Option<Value> {
    let mut accum: BTreeMap<String, Value> = BTreeMap::new();
    for (key, value) in fields {
        match decode_value(value) {
            Some(value) => {
                accum.insert(key, value);
            }
            None => return None,
        }
    }
    Some(Value::Map(accum))
}

fn decode_array(items: Vec<proto::Value>) -> Option<Value> {
    let mut accum = Vec::with_capacity(items.len());
    for value in items {
        match decode_value(value) {
            Some(value) => accum.push(value),
            None => return None,
        }
    }
    Some(Value::Array(accum))
}

fn decode_value(input: proto::Value) -> Option<Value> {
    match input.kind {
        Some(proto::value::Kind::RawBytes(data)) => Some(Value::Bytes(data.into())),
        Some(proto::value::Kind::Timestamp(ts)) => Some(Value::Timestamp(
            chrono::Utc.timestamp(ts.seconds, ts.nanos as u32),
        )),
        Some(proto::value::Kind::Integer(value)) => Some(Value::Integer(value)),
        Some(proto::value::Kind::Float(value)) => Some(Value::Float(value)),
        Some(proto::value::Kind::Boolean(value)) => Some(Value::Boolean(value)),
        Some(proto::value::Kind::Map(map)) => decode_map(map.fields),
        Some(proto::value::Kind::Array(array)) => decode_array(array.items),
        Some(proto::value::Kind::Null(_)) => Some(Value::Null),
        None => {
            error!("Encoded event contains unknown value kind.");
            None
        }
    }
}

impl From<BTreeMap<String, Value>> for Event {
    fn from(map: BTreeMap<String, Value>) -> Self {
        Self::Log(LogEvent::from(map))
    }
}

impl From<HashMap<String, Value>> for Event {
    fn from(map: HashMap<String, Value>) -> Self {
        Self::Log(LogEvent::from(map))
    }
}

impl TryFrom<serde_json::Value> for Event {
    type Error = crate::Error;

    fn try_from(map: serde_json::Value) -> Result<Self, Self::Error> {
        match map {
            serde_json::Value::Object(fields) => Ok(Event::from(
                fields
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<BTreeMap<_, _>>(),
            )),
            _ => Err(crate::Error::from(
                "Attempted to convert non-Object JSON into an Event.",
            )),
        }
    }
}

impl TryInto<serde_json::Value> for Event {
    type Error = serde_json::Error;

    fn try_into(self) -> Result<serde_json::Value, Self::Error> {
        match self {
            Event::Log(fields) => serde_json::to_value(fields),
            Event::Metric(metric) => serde_json::to_value(metric),
        }
    }
}

impl From<proto::EventWrapper> for Event {
    fn from(proto: proto::EventWrapper) -> Self {
        let event = proto.event.unwrap();

        match event {
            EventProto::Log(proto) => {
                let fields = proto
                    .fields
                    .into_iter()
                    .filter_map(|(k, v)| decode_value(v).map(|value| (k, value)))
                    .collect::<BTreeMap<_, _>>();

                Event::Log(LogEvent::from(fields))
            }
            EventProto::Metric(proto) => {
                let kind = match proto.kind() {
                    proto::metric::Kind::Incremental => MetricKind::Incremental,
                    proto::metric::Kind::Absolute => MetricKind::Absolute,
                };

                let name = proto.name;

                let namespace = if proto.namespace.is_empty() {
                    None
                } else {
                    Some(proto.namespace)
                };

                let timestamp = proto
                    .timestamp
                    .map(|ts| chrono::Utc.timestamp(ts.seconds, ts.nanos as u32));

                let tags = if !proto.tags.is_empty() {
                    Some(proto.tags)
                } else {
                    None
                };

                let value = match proto.value.unwrap() {
                    MetricProto::Counter(counter) => MetricValue::Counter {
                        value: counter.value,
                    },
                    MetricProto::Gauge(gauge) => MetricValue::Gauge { value: gauge.value },
                    MetricProto::Set(set) => MetricValue::Set {
                        values: set.values.into_iter().collect(),
                    },
                    MetricProto::Distribution(dist) => MetricValue::Distribution {
                        statistic: match dist.statistic() {
                            proto::distribution::StatisticKind::Histogram => {
                                StatisticKind::Histogram
                            }
                            proto::distribution::StatisticKind::Summary => StatisticKind::Summary,
                        },
                        samples: metric::zip_samples(dist.values, dist.sample_rates),
                    },
                    MetricProto::AggregatedHistogram(hist) => MetricValue::AggregatedHistogram {
                        buckets: metric::zip_buckets(hist.buckets, hist.counts),
                        count: hist.count,
                        sum: hist.sum,
                    },
                    MetricProto::AggregatedSummary(summary) => MetricValue::AggregatedSummary {
                        quantiles: metric::zip_quantiles(summary.quantiles, summary.values),
                        count: summary.count,
                        sum: summary.sum,
                    },
                };

                Event::Metric(Metric::new(name, namespace, timestamp, tags, kind, value))
            }
        }
    }
}

fn encode_value(value: Value) -> proto::Value {
    proto::Value {
        kind: match value {
            Value::Bytes(b) => Some(proto::value::Kind::RawBytes(b.to_vec())),
            Value::Timestamp(ts) => Some(proto::value::Kind::Timestamp(prost_types::Timestamp {
                seconds: ts.timestamp(),
                nanos: ts.timestamp_subsec_nanos() as i32,
            })),
            Value::Integer(value) => Some(proto::value::Kind::Integer(value)),
            Value::Float(value) => Some(proto::value::Kind::Float(value)),
            Value::Boolean(value) => Some(proto::value::Kind::Boolean(value)),
            Value::Map(fields) => Some(proto::value::Kind::Map(encode_map(fields))),
            Value::Array(items) => Some(proto::value::Kind::Array(encode_array(items))),
            Value::Null => Some(proto::value::Kind::Null(proto::ValueNull::NullValue as i32)),
        },
    }
}

fn encode_map(fields: BTreeMap<String, Value>) -> proto::ValueMap {
    proto::ValueMap {
        fields: fields
            .into_iter()
            .map(|(key, value)| (key, encode_value(value)))
            .collect(),
    }
}

fn encode_array(items: Vec<Value>) -> proto::ValueArray {
    proto::ValueArray {
        items: items.into_iter().map(encode_value).collect(),
    }
}

impl From<Event> for proto::EventWrapper {
    fn from(event: Event) -> Self {
        match event {
            Event::Log(log_event) => {
                let fields = log_event
                    .into_iter()
                    .map(|(k, v)| (k, encode_value(v)))
                    .collect::<BTreeMap<_, _>>();

                let event = EventProto::Log(Log { fields });

                proto::EventWrapper { event: Some(event) }
            }
            Event::Metric(Metric { series, data }) => {
                let name = series.name.name;
                let namespace = series.name.namespace.unwrap_or_default();

                let timestamp = data.timestamp.map(|ts| prost_types::Timestamp {
                    seconds: ts.timestamp(),
                    nanos: ts.timestamp_subsec_nanos() as i32,
                });

                let tags = series.tags.unwrap_or_default();

                let kind = match data.kind {
                    MetricKind::Incremental => proto::metric::Kind::Incremental,
                    MetricKind::Absolute => proto::metric::Kind::Absolute,
                }
                .into();

                let metric = match data.value {
                    MetricValue::Counter { value } => {
                        MetricProto::Counter(proto::Counter { value })
                    }
                    MetricValue::Gauge { value } => MetricProto::Gauge(proto::Gauge { value }),
                    MetricValue::Set { values } => MetricProto::Set(proto::Set {
                        values: values.into_iter().collect(),
                    }),
                    MetricValue::Distribution { samples, statistic } => {
                        MetricProto::Distribution(proto::Distribution {
                            values: samples.iter().map(|s| s.value).collect(),
                            sample_rates: samples.iter().map(|s| s.rate).collect(),
                            statistic: match statistic {
                                StatisticKind::Histogram => {
                                    proto::distribution::StatisticKind::Histogram
                                }
                                StatisticKind::Summary => {
                                    proto::distribution::StatisticKind::Summary
                                }
                            }
                            .into(),
                        })
                    }
                    MetricValue::AggregatedHistogram {
                        buckets,
                        count,
                        sum,
                    } => MetricProto::AggregatedHistogram(proto::AggregatedHistogram {
                        buckets: buckets.iter().map(|b| b.upper_limit).collect(),
                        counts: buckets.iter().map(|b| b.count).collect(),
                        count,
                        sum,
                    }),
                    MetricValue::AggregatedSummary {
                        quantiles,
                        count,
                        sum,
                    } => MetricProto::AggregatedSummary(proto::AggregatedSummary {
                        quantiles: quantiles.iter().map(|q| q.upper_limit).collect(),
                        values: quantiles.iter().map(|q| q.value).collect(),
                        count,
                        sum,
                    }),
                };

                let event = EventProto::Metric(proto::Metric {
                    name,
                    namespace,
                    timestamp,
                    tags,
                    kind,
                    value: Some(metric),
                });

                proto::EventWrapper { event: Some(event) }
            }
        }
    }
}

impl From<Bytes> for Event {
    fn from(message: Bytes) -> Self {
        let mut event = Event::Log(LogEvent::from(BTreeMap::new()));

        event
            .as_mut_log()
            .insert(log_schema().message_key(), message);
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key(), Utc::now());

        event
    }
}

impl From<&str> for Event {
    fn from(line: &str) -> Self {
        line.to_owned().into()
    }
}

impl From<String> for Event {
    fn from(line: String) -> Self {
        Bytes::from(line).into()
    }
}

impl From<LogEvent> for Event {
    fn from(log: LogEvent) -> Self {
        Event::Log(log)
    }
}

impl From<Metric> for Event {
    fn from(metric: Metric) -> Self {
        Event::Metric(metric)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use regex::Regex;
    use std::collections::HashSet;

    #[test]
    fn serialization() {
        let mut event = Event::from("raw log line");
        event.as_mut_log().insert("foo", "bar");
        event.as_mut_log().insert("bar", "baz");

        let expected_all = serde_json::json!({
            "message": "raw log line",
            "foo": "bar",
            "bar": "baz",
            "timestamp": event.as_log().get(log_schema().timestamp_key()),
        });

        let actual_all = serde_json::to_value(event.as_log().all_fields()).unwrap();
        assert_eq!(expected_all, actual_all);

        let rfc3339_re = Regex::new(r"\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z\z").unwrap();
        assert!(rfc3339_re.is_match(actual_all.pointer("/timestamp").unwrap().as_str().unwrap()));
    }

    #[test]
    fn type_serialization() {
        use serde_json::json;

        let mut event = Event::from("hello world");
        event.as_mut_log().insert("int", 4);
        event.as_mut_log().insert("float", 5.5);
        event.as_mut_log().insert("bool", true);
        event.as_mut_log().insert("string", "thisisastring");

        let map = serde_json::to_value(event.as_log().all_fields()).unwrap();
        assert_eq!(map["float"], json!(5.5));
        assert_eq!(map["int"], json!(4));
        assert_eq!(map["bool"], json!(true));
        assert_eq!(map["string"], json!("thisisastring"));
    }

    #[test]
    fn event_iteration() {
        let mut event = Event::new_empty_log();

        event
            .as_mut_log()
            .insert("Ke$ha", "It's going down, I'm yelling timber");
        event
            .as_mut_log()
            .insert("Pitbull", "The bigger they are, the harder they fall");

        let all = event
            .as_log()
            .all_fields()
            .map(|(k, v)| (k, v.to_string_lossy()))
            .collect::<HashSet<_>>();
        assert_eq!(
            all,
            vec![
                (
                    String::from("Ke$ha"),
                    "It's going down, I'm yelling timber".to_string()
                ),
                (
                    String::from("Pitbull"),
                    "The bigger they are, the harder they fall".to_string()
                ),
            ]
            .into_iter()
            .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn event_iteration_order() {
        let mut event = Event::new_empty_log();
        let log = event.as_mut_log();
        log.insert("lZDfzKIL", Value::from("tOVrjveM"));
        log.insert("o9amkaRY", Value::from("pGsfG7Nr"));
        log.insert("YRjhxXcg", Value::from("nw8iM5Jr"));

        let collected: Vec<_> = log.all_fields().collect();
        assert_eq!(
            collected,
            vec![
                (String::from("YRjhxXcg"), &Value::from("nw8iM5Jr")),
                (String::from("lZDfzKIL"), &Value::from("tOVrjveM")),
                (String::from("o9amkaRY"), &Value::from("pGsfG7Nr")),
            ]
        );
    }
}
