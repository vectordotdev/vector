use self::proto::{event_wrapper::Event as EventProto, metric::Metric as MetricProto, Log};
use bytes::Bytes;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use lazy_static::lazy_static;
use serde::{Serialize, Serializer};
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

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
    structured: HashMap<Atom, Value>,
}

impl Event {
    pub fn new_empty_log() -> Self {
        Event::Log(LogEvent {
            structured: HashMap::new(),
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
        self.structured.get(key).map(|v| &v.value)
    }

    pub fn into_value(mut self, key: &Atom) -> Option<ValueKind> {
        self.structured.remove(key).map(|v| v.value)
    }

    pub fn is_structured(&self) -> bool {
        self.structured.iter().any(|(_, v)| v.explicit)
    }

    pub fn insert_explicit(&mut self, key: Atom, value: ValueKind) {
        self.structured.insert(
            key,
            Value {
                value,
                explicit: true,
            },
        );
    }

    pub fn insert_implicit(&mut self, key: Atom, value: ValueKind) {
        self.structured.insert(
            key,
            Value {
                value,
                explicit: false,
            },
        );
    }

    pub fn remove(&mut self, key: &Atom) -> Option<ValueKind> {
        self.structured.remove(key).map(|v| v.value)
    }

    pub fn keys(&self) -> impl Iterator<Item = &Atom> {
        self.structured.keys()
    }

    pub fn all_fields<'a>(&'a self) -> FieldsIter<'a> {
        FieldsIter {
            inner: self.structured.iter(),
            explicit_only: false,
        }
    }

    pub fn unflatten(self) -> unflatten::Unflatten {
        unflatten::Unflatten::from(self.structured)
    }

    pub fn explicit_fields<'a>(&'a self) -> FieldsIter<'a> {
        FieldsIter {
            inner: self.structured.iter(),
            explicit_only: true,
        }
    }
}

impl std::ops::Index<&Atom> for LogEvent {
    type Output = ValueKind;

    fn index(&self, key: &Atom) -> &ValueKind {
        &self.structured[key].value
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
        ValueKind::Float(value as f64)
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
                let structured = proto
                    .structured
                    .into_iter()
                    .filter_map(|(k, v)| decode_value(v).map(|value| (Atom::from(k), value)))
                    .collect::<HashMap<_, _>>();

                Event::Log(LogEvent { structured })
            }
            EventProto::Metric(proto) => {
                let metric = proto.metric.unwrap();
                match metric {
                    MetricProto::Counter(counter) => {
                        let timestamp = counter
                            .timestamp
                            .map(|ts| chrono::Utc.timestamp(ts.seconds, ts.nanos as u32));

                        Event::Metric(Metric::Counter {
                            name: counter.name,
                            val: counter.val,
                            timestamp,
                        })
                    }
                    MetricProto::Histogram(hist) => {
                        let timestamp = hist
                            .timestamp
                            .map(|ts| chrono::Utc.timestamp(ts.seconds, ts.nanos as u32));

                        Event::Metric(Metric::Histogram {
                            name: hist.name,
                            val: hist.val,
                            sample_rate: hist.sample_rate,
                            timestamp,
                        })
                    }
                    MetricProto::Gauge(gauge) => {
                        let direction = match gauge.direction() {
                            proto::gauge::Direction::None => None,
                            proto::gauge::Direction::Plus => Some(metric::Direction::Plus),
                            proto::gauge::Direction::Minus => Some(metric::Direction::Minus),
                        };

                        let timestamp = gauge
                            .timestamp
                            .map(|ts| chrono::Utc.timestamp(ts.seconds, ts.nanos as u32));

                        Event::Metric(Metric::Gauge {
                            name: gauge.name,
                            val: gauge.val,
                            direction,
                            timestamp,
                        })
                    }
                    MetricProto::Set(set) => {
                        let timestamp = set
                            .timestamp
                            .map(|ts| chrono::Utc.timestamp(ts.seconds, ts.nanos as u32));

                        Event::Metric(Metric::Set {
                            name: set.name,
                            val: set.val,
                            timestamp,
                        })
                    }
                }
            }
        }
    }
}

impl From<Event> for proto::EventWrapper {
    fn from(event: Event) -> Self {
        match event {
            Event::Log(LogEvent { structured }) => {
                let structured = structured
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

                let event = EventProto::Log(Log { structured });

                proto::EventWrapper { event: Some(event) }
            }
            Event::Metric(Metric::Counter {
                name,
                val,
                timestamp,
            }) => {
                let timestamp = timestamp.map(|ts| prost_types::Timestamp {
                    seconds: ts.timestamp(),
                    nanos: ts.timestamp_subsec_nanos() as i32,
                });
                let counter = proto::Counter {
                    name,
                    val,
                    timestamp,
                };
                let event = EventProto::Metric(proto::Metric {
                    metric: Some(MetricProto::Counter(counter)),
                });
                proto::EventWrapper { event: Some(event) }
            }
            Event::Metric(Metric::Histogram {
                name,
                val,
                sample_rate,
                timestamp,
            }) => {
                let timestamp = timestamp.map(|ts| prost_types::Timestamp {
                    seconds: ts.timestamp(),
                    nanos: ts.timestamp_subsec_nanos() as i32,
                });
                let hist = proto::Histogram {
                    name,
                    val,
                    sample_rate,
                    timestamp,
                };
                let event = EventProto::Metric(proto::Metric {
                    metric: Some(MetricProto::Histogram(hist)),
                });
                proto::EventWrapper { event: Some(event) }
            }
            Event::Metric(Metric::Gauge {
                name,
                val,
                direction,
                timestamp,
            }) => {
                let timestamp = timestamp.map(|ts| prost_types::Timestamp {
                    seconds: ts.timestamp(),
                    nanos: ts.timestamp_subsec_nanos() as i32,
                });
                let direction = match direction {
                    None => proto::gauge::Direction::None,
                    Some(metric::Direction::Plus) => proto::gauge::Direction::Plus,
                    Some(metric::Direction::Minus) => proto::gauge::Direction::Minus,
                }
                .into();
                let gauge = proto::Gauge {
                    name,
                    val,
                    direction,
                    timestamp,
                };
                let event = EventProto::Metric(proto::Metric {
                    metric: Some(MetricProto::Gauge(gauge)),
                });
                proto::EventWrapper { event: Some(event) }
            }
            Event::Metric(Metric::Set {
                name,
                val,
                timestamp,
            }) => {
                let timestamp = timestamp.map(|ts| prost_types::Timestamp {
                    seconds: ts.timestamp(),
                    nanos: ts.timestamp_subsec_nanos() as i32,
                });
                let set = proto::Set {
                    name,
                    val,
                    timestamp,
                };
                let event = EventProto::Metric(proto::Metric {
                    metric: Some(MetricProto::Set(set)),
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
            structured: HashMap::new(),
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
