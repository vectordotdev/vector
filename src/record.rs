use self::proto::{record::Event, Log};
use bytes::Bytes;
use chrono::{DateTime, SecondsFormat, Utc};
use lazy_static::lazy_static;
use serde::{Serialize, Serializer};
use std::borrow::Cow;
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/record.proto.rs"));
}

lazy_static! {
    pub static ref MESSAGE: Atom = Atom::from("message");
    pub static ref HOST: Atom = Atom::from("host");
    pub static ref TIMESTAMP: Atom = Atom::from("timestamp");
}

#[derive(Serialize, PartialEq, Debug, Clone)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Record {
    structured: HashMap<Atom, OuterValue>,
}

impl Record {
    pub fn get(&self, key: &Atom) -> Option<&Value> {
        self.structured.get(key).map(|v| &v.value)
    }

    pub fn into_value(mut self, key: &Atom) -> Option<Value> {
        self.structured.remove(key).map(|v| v.value)
    }

    pub fn insert_explicit(&mut self, key: Atom, value: Value) {
        self.structured.insert(
            key,
            OuterValue {
                value,
                explicit: true,
            },
        );
    }

    pub fn insert_implicit(&mut self, key: Atom, value: Value) {
        self.structured.insert(
            key,
            OuterValue {
                value,
                explicit: false,
            },
        );
    }

    pub fn remove(&mut self, key: &Atom) {
        self.structured.remove(key);
    }

    pub fn keys(&self) -> impl Iterator<Item = &Atom> {
        self.structured.keys()
    }
}

impl std::ops::Index<&Atom> for Record {
    type Output = Value;

    fn index(&self, key: &Atom) -> &Value {
        &self.structured[key].value
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct OuterValue {
    value: Value,
    explicit: bool,
}

impl Serialize for OuterValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.value.serialize(serializer)
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum Value {
    Bytes(Bytes),
    Timestamp(DateTime<Utc>),
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string_lossy())
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

impl Value {
    // TODO: return Cow
    pub fn to_string_lossy(&self) -> String {
        match self {
            Value::Bytes(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Value::Timestamp(timestamp) => timestamp_to_string(timestamp),
        }
    }

    pub fn as_bytes(&self) -> Cow<'_, [u8]> {
        match self {
            Value::Bytes(bytes) => Cow::from(bytes[..].as_ref()),
            Value::Timestamp(timestamp) => Cow::from(timestamp_to_string(timestamp).into_bytes()),
        }
    }

    pub fn into_bytes(self) -> Bytes {
        match self {
            Value::Bytes(bytes) => bytes,
            Value::Timestamp(timestamp) => timestamp_to_string(&timestamp).into_bytes().into(),
        }
    }
}

fn timestamp_to_string(timestamp: &DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
}

impl From<proto::Record> for Record {
    fn from(proto: proto::Record) -> Self {
        let event = proto.event.unwrap();

        match event {
            Event::Log(proto) => {
                let structured = proto
                    .structured
                    .into_iter()
                    .map(|(k, v)| {
                        let value = OuterValue {
                            value: v.data.into(),
                            explicit: v.explicit,
                        };
                        (Atom::from(k), value)
                    })
                    .collect::<HashMap<_, _>>();

                Record { structured }
            }
        }
    }
}

impl From<Record> for proto::Record {
    fn from(record: Record) -> Self {
        let structured = record
            .structured
            .into_iter()
            .map(|(k, v)| {
                let value = proto::Value {
                    data: v.value.as_bytes().into_owned(),
                    explicit: v.explicit,
                };
                (k.to_string(), value)
            })
            .collect::<HashMap<_, _>>();

        let event = Event::Log(Log { structured });

        proto::Record { event: Some(event) }
    }
}

impl From<Record> for Vec<u8> {
    fn from(mut record: Record) -> Vec<u8> {
        record
            .structured
            .remove(&MESSAGE)
            .unwrap()
            .value
            .as_bytes()
            .into_owned()
    }
}

impl From<Bytes> for Record {
    fn from(message: Bytes) -> Self {
        let mut record = Record {
            structured: HashMap::new(),
        };

        record.insert_implicit(MESSAGE.clone(), message.into());
        record.insert_implicit(TIMESTAMP.clone(), Utc::now().into());

        record
    }
}

impl From<&str> for Record {
    fn from(line: &str) -> Self {
        line.to_owned().into()
    }
}

impl From<String> for Record {
    fn from(line: String) -> Self {
        Bytes::from(line).into()
    }
}

#[cfg(test)]
mod test {
    use super::Record;
    use regex::Regex;

    #[test]
    fn serialization() {
        let mut record = Record::from("raw log line");
        record.insert_explicit("foo".into(), "bar".into());
        record.insert_explicit("bar".into(), "baz".into());

        let expected = serde_json::json!({
            "message": "raw log line",
            "foo": "bar",
            "bar": "baz",
            "timestamp": record.structured[&super::TIMESTAMP],
        });
        let actual = serde_json::to_value(record).unwrap();
        assert_eq!(expected, actual);

        let rfc3339_re = Regex::new(r"\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z\z").unwrap();
        assert!(rfc3339_re.is_match(actual.pointer("/timestamp").unwrap().as_str().unwrap()));
    }
}
