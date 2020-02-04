use self::proto::{event_wrapper::Event as EventProto, metric::Value as MetricProto, Log};
use bytes::Bytes;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use lazy_static::lazy_static;
use metric::{MetricKind, MetricValue};
use serde::{Serialize, Serializer};
use std::collections::{hash_map::Drain, HashMap};
use std::iter::FromIterator;
use string_cache::DefaultAtom as Atom;

pub mod discriminant;
pub mod flatten;
pub mod merge;
pub mod merge_state;
pub mod metric;
mod unflatten;

pub use metric::Metric;
pub use unflatten::Unflatten;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/event.proto.rs"));
}

lazy_static! {
    pub static ref MESSAGE: Atom = Atom::from("message");
    pub static ref HOST: Atom = Atom::from("host");
    pub static ref TIMESTAMP: Atom = Atom::from("timestamp");
    pub static ref PARTIAL: Atom = Atom::from("_partial");
}

#[derive(PartialEq, Debug, Clone)]
pub enum Event {
    Log(LogEvent),
    Metric(Metric),
}

#[derive(PartialEq, Debug, Clone)]
pub struct LogEvent {
    fields: HashMap<Atom, Value>,
}

impl Event {
    pub fn new_empty_log() -> Self {
        Event::Log(LogEvent {
            fields: HashMap::new(),
        })
    }

    pub fn as_log(&self) -> &LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("failed type coercion, {:?} is not a log event", self),
        }
    }

    pub fn as_mut_log(&mut self) -> &mut LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("failed type coercion, {:?} is not a log event", self),
        }
    }

    pub fn into_log(self) -> LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("failed type coercion, {:?} is not a log event", self),
        }
    }

    pub fn as_metric(&self) -> &Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("failed type coercion, {:?} is not a metric", self),
        }
    }

    pub fn as_mut_metric(&mut self) -> &mut Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("failed type coercion, {:?} is not a metric", self),
        }
    }

    pub fn into_metric(self) -> Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("failed type coercion, {:?} is not a metric", self),
        }
    }
}

impl LogEvent {
    pub fn get(&self, key: &Atom) -> Option<&Value> {
        self.fields.get(key)
    }

    pub fn get_mut(&mut self, key: &Atom) -> Option<&mut Value> {
        self.fields.get_mut(key)
    }

    pub fn contains(&self, key: &Atom) -> bool {
        self.fields.contains_key(key)
    }

    pub fn insert<K, V>(&mut self, key: K, value: V)
    where
        K: Into<Atom>,
        V: Into<Value>,
    {
        self.fields.insert(key.into(), value.into());
    }

    pub fn remove(&mut self, key: &Atom) -> Option<Value> {
        self.fields.remove(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &Atom> {
        self.fields.keys()
    }

    pub fn all_fields(&self) -> FieldsIter {
        FieldsIter {
            inner: self.fields.iter(),
        }
    }

    pub fn unflatten(self) -> unflatten::Unflatten {
        unflatten::Unflatten::from(self.fields)
    }

    pub fn drain(&mut self) -> Drain<Atom, Value> {
        self.fields.drain()
    }
}

impl std::ops::Index<&Atom> for LogEvent {
    type Output = Value;

    fn index(&self, key: &Atom) -> &Value {
        &self.fields[key]
    }
}

impl<K: Into<Atom>, V: Into<Value>> Extend<(K, V)> for LogEvent {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.insert(k.into(), v.into());
        }
    }
}

// Allow converting any kind of appropriate key/value iterator directly into a LogEvent.
impl<K: Into<Atom>, V: Into<Value>> FromIterator<(K, V)> for LogEvent {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut log_event = Event::new_empty_log().into_log();
        log_event.extend(iter);
        log_event
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum Value {
    Bytes(Bytes),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Timestamp(DateTime<Utc>),
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            Value::Integer(i) => serializer.serialize_i64(*i),
            Value::Float(f) => serializer.serialize_f64(*f),
            Value::Boolean(b) => serializer.serialize_bool(*b),
            _ => serializer.serialize_str(&self.to_string_lossy()),
        }
    }
}

impl From<Bytes> for Value {
    fn from(bytes: Bytes) -> Self {
        Value::Bytes(bytes)
    }
}

impl From<Vec<u8>> for Value {
    fn from(bytes: Vec<u8>) -> Self {
        Value::Bytes(bytes.into())
    }
}

impl From<&[u8]> for Value {
    fn from(bytes: &[u8]) -> Self {
        Value::Bytes(bytes.into())
    }
}

impl From<String> for Value {
    fn from(string: String) -> Self {
        Value::Bytes(string.into())
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Bytes(s.into())
    }
}

impl From<DateTime<Utc>> for Value {
    fn from(timestamp: DateTime<Utc>) -> Self {
        Value::Timestamp(timestamp)
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Value::Float(f64::from(value))
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Float(value)
    }
}

macro_rules! impl_valuekind_from_integer {
    ($t:ty) => {
        impl From<$t> for Value {
            fn from(value: $t) -> Self {
                Value::Integer(value as i64)
            }
        }
    };
}

impl_valuekind_from_integer!(i64);
impl_valuekind_from_integer!(i32);
impl_valuekind_from_integer!(i16);
impl_valuekind_from_integer!(i8);
impl_valuekind_from_integer!(isize);

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Boolean(value)
    }
}

impl Value {
    // TODO: return Cow
    pub fn to_string_lossy(&self) -> String {
        match self {
            Value::Bytes(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Value::Timestamp(timestamp) => timestamp_to_string(timestamp),
            Value::Integer(num) => format!("{}", num),
            Value::Float(num) => format!("{}", num),
            Value::Boolean(b) => format!("{}", b),
        }
    }

    pub fn as_bytes(&self) -> Bytes {
        match self {
            Value::Bytes(bytes) => bytes.clone(), // cloning a Bytes is cheap
            Value::Timestamp(timestamp) => Bytes::from(timestamp_to_string(timestamp)),
            Value::Integer(num) => Bytes::from(format!("{}", num)),
            Value::Float(num) => Bytes::from(format!("{}", num)),
            Value::Boolean(b) => Bytes::from(format!("{}", b)),
        }
    }

    pub fn into_bytes(self) -> Bytes {
        self.as_bytes()
    }

    pub fn as_timestamp(&self) -> Option<&DateTime<Utc>> {
        match &self {
            Value::Timestamp(ts) => Some(ts),
            _ => None,
        }
    }
}

fn timestamp_to_string(timestamp: &DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true)
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
        None => {
            error!("encoded event contains unknown value kind");
            None
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
                    .filter_map(|(k, v)| decode_value(v).map(|value| (Atom::from(k), value)))
                    .collect::<HashMap<_, _>>();

                Event::Log(LogEvent { fields })
            }
            EventProto::Metric(proto) => {
                let kind = match proto.kind() {
                    proto::metric::Kind::Incremental => MetricKind::Incremental,
                    proto::metric::Kind::Absolute => MetricKind::Absolute,
                };

                let name = proto.name;

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
                        values: dist.values,
                        sample_rates: dist.sample_rates,
                    },
                    MetricProto::AggregatedHistogram(hist) => MetricValue::AggregatedHistogram {
                        buckets: hist.buckets,
                        counts: hist.counts,
                        count: hist.count,
                        sum: hist.sum,
                    },
                    MetricProto::AggregatedSummary(summary) => MetricValue::AggregatedSummary {
                        quantiles: summary.quantiles,
                        values: summary.values,
                        count: summary.count,
                        sum: summary.sum,
                    },
                };

                Event::Metric(Metric {
                    name,
                    timestamp,
                    tags,
                    kind,
                    value,
                })
            }
        }
    }
}

impl From<Event> for proto::EventWrapper {
    fn from(event: Event) -> Self {
        match event {
            Event::Log(LogEvent { fields }) => {
                let fields = fields
                    .into_iter()
                    .map(|(k, v)| {
                        let value = proto::Value {
                            kind: match v {
                                Value::Bytes(b) => Some(proto::value::Kind::RawBytes(b.to_vec())),
                                Value::Timestamp(ts) => {
                                    Some(proto::value::Kind::Timestamp(prost_types::Timestamp {
                                        seconds: ts.timestamp(),
                                        nanos: ts.timestamp_subsec_nanos() as i32,
                                    }))
                                }
                                Value::Integer(value) => Some(proto::value::Kind::Integer(value)),
                                Value::Float(value) => Some(proto::value::Kind::Float(value)),
                                Value::Boolean(value) => Some(proto::value::Kind::Boolean(value)),
                            },
                        };
                        (k.to_string(), value)
                    })
                    .collect::<HashMap<_, _>>();

                let event = EventProto::Log(Log { fields });

                proto::EventWrapper { event: Some(event) }
            }
            Event::Metric(Metric {
                name,
                timestamp,
                tags,
                kind,
                value,
            }) => {
                let timestamp = timestamp.map(|ts| prost_types::Timestamp {
                    seconds: ts.timestamp(),
                    nanos: ts.timestamp_subsec_nanos() as i32,
                });

                let tags = tags.unwrap_or_default();

                let kind = match kind {
                    MetricKind::Incremental => proto::metric::Kind::Incremental,
                    MetricKind::Absolute => proto::metric::Kind::Absolute,
                }
                .into();

                let metric = match value {
                    MetricValue::Counter { value } => {
                        MetricProto::Counter(proto::Counter { value })
                    }
                    MetricValue::Gauge { value } => MetricProto::Gauge(proto::Gauge { value }),
                    MetricValue::Set { values } => MetricProto::Set(proto::Set {
                        values: values.into_iter().collect(),
                    }),
                    MetricValue::Distribution {
                        values,
                        sample_rates,
                    } => MetricProto::Distribution(proto::Distribution {
                        values,
                        sample_rates,
                    }),
                    MetricValue::AggregatedHistogram {
                        buckets,
                        counts,
                        count,
                        sum,
                    } => MetricProto::AggregatedHistogram(proto::AggregatedHistogram {
                        buckets,
                        counts,
                        count,
                        sum,
                    }),
                    MetricValue::AggregatedSummary {
                        quantiles,
                        values,
                        count,
                        sum,
                    } => MetricProto::AggregatedSummary(proto::AggregatedSummary {
                        quantiles,
                        values,
                        count,
                        sum,
                    }),
                };

                let event = EventProto::Metric(proto::Metric {
                    name,
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

// TODO: should probably get rid of this
impl From<Event> for Vec<u8> {
    fn from(event: Event) -> Vec<u8> {
        event
            .into_log()
            .remove(&MESSAGE)
            .unwrap()
            .as_bytes()
            .to_vec()
    }
}

impl From<Bytes> for Event {
    fn from(message: Bytes) -> Self {
        let mut event = Event::Log(LogEvent {
            fields: HashMap::new(),
        });

        event.as_mut_log().insert(MESSAGE.clone(), message);
        event.as_mut_log().insert(TIMESTAMP.clone(), Utc::now());

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

#[derive(Clone)]
pub struct FieldsIter<'a> {
    inner: std::collections::hash_map::Iter<'a, Atom, Value>,
}

impl<'a> Iterator for FieldsIter<'a> {
    type Item = (&'a Atom, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a> Serialize for FieldsIter<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_map(self.clone())
    }
}

#[cfg(test)]
mod test {
    use super::Event;
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
            "timestamp": event.as_log().get(&super::TIMESTAMP),
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
                    &"Ke$ha".into(),
                    "It's going down, I'm yelling timber".to_string()
                ),
                (
                    &"Pitbull".into(),
                    "The bigger they are, the harder they fall".to_string()
                ),
            ]
            .into_iter()
            .collect::<HashSet<_>>()
        );
    }
}
