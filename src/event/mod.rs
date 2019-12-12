use self::proto::{event_wrapper::Event as EventProto, metric::Metric as MetricProto, Log};
use bytes::Bytes;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use lazy_static::lazy_static;
use metric::{MetricKind, MetricValue};
use serde::{Serialize, Serializer};
use std::collections::HashMap;
use std::iter::FromIterator;
use string_cache::DefaultAtom as Atom;

pub mod flatten;
pub mod metric;
mod unflatten;

pub use metric::Metric;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/event.proto.rs"));
}

lazy_static! {
    pub static ref MESSAGE: Atom = Atom::from("message");
    pub static ref HOST: Atom = Atom::from("host");
    pub static ref TIMESTAMP: Atom = Atom::from("timestamp");
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
    pub fn get(&self, key: &Atom) -> Option<&ValueKind> {
        self.fields.get(key).map(|v| &v.value)
    }

    pub fn get_mut(&mut self, key: &Atom) -> Option<&mut ValueKind> {
        self.fields.get_mut(key).map(|v| &mut v.value)
    }

    pub fn contains(&self, key: &Atom) -> bool {
        self.fields.contains_key(key)
    }

    pub fn into_value(mut self, key: &Atom) -> Option<ValueKind> {
        self.fields.remove(key).map(|v| v.value)
    }

    pub fn insert_explicit(&mut self, key: Atom, value: ValueKind) {
        self.fields.insert(
            key,
            Value {
                value,
                explicit: true,
            },
        );
    }

    pub fn insert_implicit(&mut self, key: Atom, value: ValueKind) {
        self.fields.insert(
            key,
            Value {
                value,
                explicit: false,
            },
        );
    }

    pub fn remove(&mut self, key: &Atom) -> Option<ValueKind> {
        self.fields.remove(key).map(|v| v.value)
    }

    pub fn keys(&self) -> impl Iterator<Item = &Atom> {
        self.fields.keys()
    }

    pub fn all_fields(&self) -> FieldsIter {
        FieldsIter {
            inner: self.fields.iter(),
            explicit_only: false,
        }
    }

    pub fn unflatten(self) -> unflatten::Unflatten {
        unflatten::Unflatten::from(self.fields)
    }

    pub fn explicit_fields(&self) -> FieldsIter {
        FieldsIter {
            inner: self.fields.iter(),
            explicit_only: true,
        }
    }
}

impl std::ops::Index<&Atom> for LogEvent {
    type Output = ValueKind;

    fn index(&self, key: &Atom) -> &ValueKind {
        &self.fields[key].value
    }
}

// Allow converting any kind of appropriate key/value iterator directly into a LogEvent.
impl<K: Into<Atom>, V: Into<ValueKind>> FromIterator<(K, V)> for LogEvent {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self {
            fields: iter
                .into_iter()
                .map(|(key, value)| {
                    (
                        key.into(),
                        Value {
                            value: value.into(),
                            explicit: true,
                        },
                    )
                })
                .collect(),
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct Value {
    value: ValueKind,
    explicit: bool,
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.value.serialize(serializer)
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum ValueKind {
    Bytes(Bytes),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Timestamp(DateTime<Utc>),
}

impl Serialize for ValueKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            ValueKind::Integer(i) => serializer.serialize_i64(*i),
            ValueKind::Float(f) => serializer.serialize_f64(*f),
            ValueKind::Boolean(b) => serializer.serialize_bool(*b),
            _ => serializer.serialize_str(&self.to_string_lossy()),
        }
    }
}

impl From<Bytes> for ValueKind {
    fn from(bytes: Bytes) -> Self {
        ValueKind::Bytes(bytes)
    }
}

impl From<Vec<u8>> for ValueKind {
    fn from(bytes: Vec<u8>) -> Self {
        ValueKind::Bytes(bytes.into())
    }
}

impl From<&[u8]> for ValueKind {
    fn from(bytes: &[u8]) -> Self {
        ValueKind::Bytes(bytes.into())
    }
}

impl From<String> for ValueKind {
    fn from(string: String) -> Self {
        ValueKind::Bytes(string.into())
    }
}

impl From<&str> for ValueKind {
    fn from(s: &str) -> Self {
        ValueKind::Bytes(s.into())
    }
}

impl From<DateTime<Utc>> for ValueKind {
    fn from(timestamp: DateTime<Utc>) -> Self {
        ValueKind::Timestamp(timestamp)
    }
}

impl From<f32> for ValueKind {
    fn from(value: f32) -> Self {
        ValueKind::Float(f64::from(value))
    }
}

impl From<f64> for ValueKind {
    fn from(value: f64) -> Self {
        ValueKind::Float(value)
    }
}

macro_rules! impl_valuekind_from_integer {
    ($t:ty) => {
        impl From<$t> for ValueKind {
            fn from(value: $t) -> Self {
                ValueKind::Integer(value as i64)
            }
        }
    };
}

impl_valuekind_from_integer!(i64);
impl_valuekind_from_integer!(i32);
impl_valuekind_from_integer!(i16);
impl_valuekind_from_integer!(i8);
impl_valuekind_from_integer!(isize);

impl From<bool> for ValueKind {
    fn from(value: bool) -> Self {
        ValueKind::Boolean(value)
    }
}

impl ValueKind {
    // TODO: return Cow
    pub fn to_string_lossy(&self) -> String {
        match self {
            ValueKind::Bytes(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            ValueKind::Timestamp(timestamp) => timestamp_to_string(timestamp),
            ValueKind::Integer(num) => format!("{}", num),
            ValueKind::Float(num) => format!("{}", num),
            ValueKind::Boolean(b) => format!("{}", b),
        }
    }

    pub fn as_bytes(&self) -> Bytes {
        match self {
            ValueKind::Bytes(bytes) => bytes.clone(), // cloning a Bytes is cheap
            ValueKind::Timestamp(timestamp) => Bytes::from(timestamp_to_string(timestamp)),
            ValueKind::Integer(num) => Bytes::from(format!("{}", num)),
            ValueKind::Float(num) => Bytes::from(format!("{}", num)),
            ValueKind::Boolean(b) => Bytes::from(format!("{}", b)),
        }
    }

    pub fn into_bytes(self) -> Bytes {
        self.as_bytes()
    }

    pub fn as_timestamp(&self) -> Option<&DateTime<Utc>> {
        match &self {
            ValueKind::Timestamp(ts) => Some(ts),
            _ => None,
        }
    }
}

fn timestamp_to_string(timestamp: &DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

fn decode_value(input: proto::Value) -> Option<Value> {
    let explicit = input.explicit;
    let value = match input.kind {
        Some(proto::value::Kind::RawBytes(data)) => Some(ValueKind::Bytes(data.into())),
        Some(proto::value::Kind::Timestamp(ts)) => Some(ValueKind::Timestamp(
            chrono::Utc.timestamp(ts.seconds, ts.nanos as u32),
        )),
        Some(proto::value::Kind::Integer(value)) => Some(ValueKind::Integer(value)),
        Some(proto::value::Kind::Float(value)) => Some(ValueKind::Float(value)),
        Some(proto::value::Kind::Boolean(value)) => Some(ValueKind::Boolean(value)),
        None => {
            error!("encoded event contains unknown value kind");
            None
        }
    };
    value.map(|decoded| Value {
        value: decoded,
        explicit,
    })
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

                let value = match proto.metric.unwrap() {
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
                            explicit: v.explicit,
                            kind: match v.value {
                                ValueKind::Bytes(b) => {
                                    Some(proto::value::Kind::RawBytes(b.to_vec()))
                                }
                                ValueKind::Timestamp(ts) => {
                                    Some(proto::value::Kind::Timestamp(prost_types::Timestamp {
                                        seconds: ts.timestamp(),
                                        nanos: ts.timestamp_subsec_nanos() as i32,
                                    }))
                                }
                                ValueKind::Integer(value) => {
                                    Some(proto::value::Kind::Integer(value))
                                }
                                ValueKind::Float(value) => Some(proto::value::Kind::Float(value)),
                                ValueKind::Boolean(value) => {
                                    Some(proto::value::Kind::Boolean(value))
                                }
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
                    metric: Some(metric),
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
            .into_value(&MESSAGE)
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

        event
            .as_mut_log()
            .insert_implicit(MESSAGE.clone(), message.into());
        event
            .as_mut_log()
            .insert_implicit(TIMESTAMP.clone(), Utc::now().into());

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
    explicit_only: bool,
}

impl<'a> Iterator for FieldsIter<'a> {
    type Item = (&'a Atom, &'a ValueKind);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (key, value) = match self.inner.next() {
                Some(next) => next,
                None => return None,
            };

            if self.explicit_only && !value.explicit {
                continue;
            }

            return Some((key, &value.value));
        }
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
        event
            .as_mut_log()
            .insert_explicit("foo".into(), "bar".into());
        event
            .as_mut_log()
            .insert_explicit("bar".into(), "baz".into());

        let expected_all = serde_json::json!({
            "message": "raw log line",
            "foo": "bar",
            "bar": "baz",
            "timestamp": event.as_log().get(&super::TIMESTAMP),
        });

        let expected_explicit = serde_json::json!({
            "foo": "bar",
            "bar": "baz",
        });

        let actual_all = serde_json::to_value(event.as_log().all_fields()).unwrap();
        assert_eq!(expected_all, actual_all);

        let actual_explicit = serde_json::to_value(event.as_log().explicit_fields()).unwrap();
        assert_eq!(expected_explicit, actual_explicit);

        let rfc3339_re = Regex::new(r"\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z\z").unwrap();
        assert!(rfc3339_re.is_match(actual_all.pointer("/timestamp").unwrap().as_str().unwrap()));
    }

    #[test]
    fn type_serialization() {
        use serde_json::json;

        let mut event = Event::from("hello world");
        event.as_mut_log().insert_explicit("int".into(), 4.into());
        event
            .as_mut_log()
            .insert_explicit("float".into(), 5.5.into());
        event
            .as_mut_log()
            .insert_explicit("bool".into(), true.into());
        event
            .as_mut_log()
            .insert_explicit("string".into(), "thisisastring".into());

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
            .insert_explicit("Ke$ha".into(), "It's going down, I'm yelling timber".into());
        event.as_mut_log().insert_implicit(
            "Pitbull".into(),
            "The bigger they are, the harder they fall".into(),
        );

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

        let explicit_only = event
            .as_log()
            .explicit_fields()
            .map(|(k, v)| (k, v.to_string_lossy()))
            .collect::<HashSet<_>>();
        assert_eq!(
            explicit_only,
            vec![(
                &"Ke$ha".into(),
                "It's going down, I'm yelling timber".to_string()
            ),]
            .into_iter()
            .collect::<HashSet<_>>()
        );
    }
}
