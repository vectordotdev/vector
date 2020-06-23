use self::proto::{event_wrapper::Event as EventProto, metric::Value as MetricProto, Log};
use bytes::Bytes;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use getset::{Getters, Setters};
use lazy_static::lazy_static;
use metric::{MetricKind, MetricValue};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value as JsonValue;
use std::{collections::BTreeMap, iter::FromIterator};
use string_cache::DefaultAtom as Atom;

pub mod discriminant;
pub mod merge;
pub mod merge_state;
pub mod metric;
mod util;

pub use metric::Metric;
pub(crate) use util::log::PathComponent;
pub(crate) use util::log::PathIter;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/event.proto.rs"));
}

pub static LOG_SCHEMA: OnceCell<LogSchema> = OnceCell::new();

lazy_static! {
    pub static ref PARTIAL: Atom = Atom::from("_partial");
    static ref LOG_SCHEMA_DEFAULT: LogSchema = LogSchema {
        message_key: Atom::from("message"),
        timestamp_key: Atom::from("timestamp"),
        host_key: Atom::from("host"),
        source_type_key: Atom::from("source_type"),
    };
}

#[derive(PartialEq, Debug, Clone)]
pub enum Event {
    Log(LogEvent),
    Metric(Metric),
}

#[derive(PartialEq, Debug, Clone)]
pub struct LogEvent {
    fields: BTreeMap<String, Value>,
}

impl Event {
    pub fn new_empty_log() -> Self {
        Event::Log(LogEvent::new())
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
    pub fn new() -> Self {
        Self {
            fields: BTreeMap::new(),
        }
    }

    pub fn get(&self, key: &Atom) -> Option<&Value> {
        util::log::get(&self.fields, key)
    }

    pub fn get_flat(&self, key: impl AsRef<str>) -> Option<&Value> {
        self.fields.get(key.as_ref())
    }

    pub fn get_mut(&mut self, key: &Atom) -> Option<&mut Value> {
        util::log::get_mut(&mut self.fields, key)
    }

    pub fn contains(&self, key: &Atom) -> bool {
        util::log::contains(&self.fields, key)
    }

    pub fn insert<K, V>(&mut self, key: K, value: V) -> Option<Value>
    where
        K: AsRef<str>,
        V: Into<Value>,
    {
        util::log::insert(&mut self.fields, key.as_ref(), value.into())
    }

    pub fn insert_path<V>(&mut self, key: Vec<PathComponent>, value: V) -> Option<Value>
    where
        V: Into<Value>,
    {
        util::log::insert_path(&mut self.fields, key, value.into())
    }

    pub fn insert_flat<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Into<Value>,
    {
        self.fields.insert(key.into(), value.into());
    }

    pub fn try_insert<V>(&mut self, key: &Atom, value: V)
    where
        V: Into<Value>,
    {
        if !self.contains(key) {
            self.insert(key.clone(), value);
        }
    }

    pub fn remove(&mut self, key: &Atom) -> Option<Value> {
        util::log::remove(&mut self.fields, &key, false)
    }

    pub fn remove_prune(&mut self, key: &Atom, prune: bool) -> Option<Value> {
        util::log::remove(&mut self.fields, &key, prune)
    }

    pub fn keys<'a>(&'a self) -> impl Iterator<Item = String> + 'a {
        util::log::keys(&self.fields)
    }

    pub fn all_fields<'a>(&'a self) -> impl Iterator<Item = (String, &'a Value)> + Serialize {
        util::log::all_fields(&self.fields)
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

impl std::ops::Index<&Atom> for LogEvent {
    type Output = Value;

    fn index(&self, key: &Atom) -> &Value {
        self.get(key).expect("Key is not found")
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
        let mut log_event = LogEvent::new();
        log_event.extend(iter);
        log_event
    }
}

/// Converts event into an iterator over top-level key/value pairs.
impl IntoIterator for LogEvent {
    type Item = (String, Value);
    type IntoIter = std::collections::btree_map::IntoIter<String, Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.fields.into_iter()
    }
}

impl Serialize for LogEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_map(self.fields.iter())
    }
}

pub fn log_schema() -> &'static LogSchema {
    LOG_SCHEMA.get().unwrap_or(&LOG_SCHEMA_DEFAULT)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Getters, Setters)]
#[serde(default)]
pub struct LogSchema {
    #[serde(default = "LogSchema::default_message_key")]
    #[getset(get = "pub", set = "pub(crate)")]
    message_key: Atom,
    #[serde(default = "LogSchema::default_timestamp_key")]
    #[getset(get = "pub", set = "pub(crate)")]
    timestamp_key: Atom,
    #[serde(default = "LogSchema::default_host_key")]
    #[getset(get = "pub", set = "pub(crate)")]
    host_key: Atom,
    #[serde(default = "LogSchema::default_source_type_key")]
    #[getset(get = "pub", set = "pub(crate)")]
    source_type_key: Atom,
}

impl Default for LogSchema {
    fn default() -> Self {
        LogSchema {
            message_key: Atom::from("message"),
            timestamp_key: Atom::from("timestamp"),
            host_key: Atom::from("host"),
            source_type_key: Atom::from("source_type"),
        }
    }
}

impl LogSchema {
    fn default_message_key() -> Atom {
        Atom::from("message")
    }
    fn default_timestamp_key() -> Atom {
        Atom::from("timestamp")
    }
    fn default_host_key() -> Atom {
        Atom::from("host")
    }
    fn default_source_type_key() -> Atom {
        Atom::from("source_type")
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum Value {
    Bytes(Bytes),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Timestamp(DateTime<Utc>),
    Map(BTreeMap<String, Value>),
    Array(Vec<Value>),
    Null,
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
            Value::Bytes(_) | Value::Timestamp(_) => {
                serializer.serialize_str(&self.to_string_lossy())
            }
            Value::Map(m) => serializer.collect_map(m),
            Value::Array(a) => serializer.collect_seq(a),
            Value::Null => serializer.serialize_none(),
        }
    }
}

impl From<Bytes> for Value {
    fn from(bytes: Bytes) -> Self {
        Value::Bytes(bytes)
    }
}

impl From<bytes05::Bytes> for Value {
    fn from(bytes: bytes05::Bytes) -> Self {
        Value::Bytes(bytes.as_ref().into())
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

impl From<&String> for Value {
    fn from(string: &String) -> Self {
        string.as_str().into()
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

impl From<BTreeMap<String, Value>> for Value {
    fn from(value: BTreeMap<String, Value>) -> Self {
        Value::Map(value)
    }
}

impl From<Vec<Value>> for Value {
    fn from(value: Vec<Value>) -> Self {
        Value::Array(value)
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

impl From<JsonValue> for Value {
    fn from(json_value: JsonValue) -> Self {
        match json_value {
            JsonValue::Bool(b) => Value::Boolean(b),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    Value::Float(f)
                } else {
                    Value::Bytes(n.to_string().into())
                }
            }
            JsonValue::String(s) => Value::Bytes(Bytes::from(s)),
            JsonValue::Object(obj) => Value::Map(
                obj.into_iter()
                    .map(|(key, value)| (key.into(), Value::from(value)))
                    .collect(),
            ),
            JsonValue::Array(arr) => {
                Value::Array(arr.into_iter().map(|value| Value::from(value)).collect())
            }
            JsonValue::Null => Value::Null,
        }
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
            Value::Map(map) => serde_json::to_string(map).expect("Cannot serialize map"),
            Value::Array(arr) => serde_json::to_string(arr).expect("Cannot serialize array"),
            Value::Null => "<null>".to_string(),
        }
    }

    pub fn as_bytes(&self) -> Bytes {
        match self {
            Value::Bytes(bytes) => bytes.clone(), // cloning a Bytes is cheap
            Value::Timestamp(timestamp) => Bytes::from(timestamp_to_string(timestamp)),
            Value::Integer(num) => Bytes::from(format!("{}", num)),
            Value::Float(num) => Bytes::from(format!("{}", num)),
            Value::Boolean(b) => Bytes::from(format!("{}", b)),
            Value::Map(map) => Bytes::from(serde_json::to_vec(map).expect("Cannot serialize map")),
            Value::Array(arr) => {
                Bytes::from(serde_json::to_vec(arr).expect("Cannot serialize array"))
            }
            Value::Null => Bytes::from("<null>"),
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
                    .filter_map(|(k, v)| decode_value(v).map(|value| (k, value)))
                    .collect::<BTreeMap<_, _>>();

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
        items: items.into_iter().map(|value| encode_value(value)).collect(),
    }
}

impl From<Event> for proto::EventWrapper {
    fn from(event: Event) -> Self {
        match event {
            Event::Log(LogEvent { fields }) => {
                let fields = fields
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), encode_value(v)))
                    .collect::<BTreeMap<_, _>>();

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
            .remove(&log_schema().message_key())
            .unwrap()
            .as_bytes()
            .to_vec()
    }
}

impl From<Bytes> for Event {
    fn from(message: Bytes) -> Self {
        let mut event = Event::Log(LogEvent {
            fields: BTreeMap::new(),
        });

        event
            .as_mut_log()
            .insert(log_schema().message_key().clone(), message);
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key().clone(), Utc::now());

        event
    }
}

impl From<bytes05::Bytes> for Event {
    fn from(message: bytes05::Bytes) -> Self {
        let mut event = Event::Log(LogEvent {
            fields: BTreeMap::new(),
        });

        event
            .as_mut_log()
            .insert(log_schema().message_key().clone(), message);
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key().clone(), Utc::now());

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
    use super::{Atom, Event, LogSchema, Value};
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
            "timestamp": event.as_log().get(&super::log_schema().timestamp_key()),
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
        log.insert(&Atom::from("lZDfzKIL"), Value::from("tOVrjveM"));
        log.insert(&Atom::from("o9amkaRY"), Value::from("pGsfG7Nr"));
        log.insert(&Atom::from("YRjhxXcg"), Value::from("nw8iM5Jr"));

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

    #[test]
    fn partial_log_schema() {
        let toml = r#"
message_key = "message"
timestamp_key = "timestamp"
"#;
        let _ = toml::from_str::<LogSchema>(toml).unwrap();
    }
}
