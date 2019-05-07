use self::proto::{event_wrapper::Event as EventProto, Log};
use bytes::Bytes;
use chrono::{DateTime, SecondsFormat, Utc};
use lazy_static::lazy_static;
use serde::{Serialize, Serializer};
use std::borrow::Cow;
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

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
}

#[derive(PartialEq, Debug, Clone)]
pub struct LogEvent {
    structured: HashMap<Atom, Value>,
}

impl Event {
    pub fn new_empty() -> Self {
        Event::Log(LogEvent {
            structured: HashMap::new(),
        })
    }

    pub fn remove(&mut self, key: &Atom) {
        self.as_mut_log().remove(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &Atom> {
        self.as_log().keys()
    }

    pub fn all_fields<'a>(&'a self) -> FieldsIter<'a> {
        self.as_log().all_fields()
    }

    pub fn explicit_fields<'a>(&'a self) -> FieldsIter<'a> {
        self.as_log().explicit_fields()
    }

    pub fn as_log(&self) -> &LogEvent {
        match self {
            Event::Log(log) => log,
        }
    }

    pub fn as_mut_log(&mut self) -> &mut LogEvent {
        match self {
            Event::Log(log) => log,
        }
    }

    pub fn into_log(self) -> LogEvent {
        match self {
            Event::Log(log) => log,
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

    pub fn remove(&mut self, key: &Atom) {
        self.structured.remove(key);
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

    pub fn explicit_fields<'a>(&'a self) -> FieldsIter<'a> {
        FieldsIter {
            inner: self.structured.iter(),
            explicit_only: true,
        }
    }
}

impl std::ops::Index<&Atom> for Event {
    type Output = ValueKind;

    fn index(&self, key: &Atom) -> &ValueKind {
        match self {
            Event::Log(LogEvent { structured }) => &structured[key].value,
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
    Timestamp(DateTime<Utc>),
}

impl Serialize for ValueKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string_lossy())
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

impl ValueKind {
    // TODO: return Cow
    pub fn to_string_lossy(&self) -> String {
        match self {
            ValueKind::Bytes(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            ValueKind::Timestamp(timestamp) => timestamp_to_string(timestamp),
        }
    }

    pub fn as_bytes(&self) -> Cow<'_, [u8]> {
        match self {
            ValueKind::Bytes(bytes) => Cow::from(bytes[..].as_ref()),
            ValueKind::Timestamp(timestamp) => {
                Cow::from(timestamp_to_string(timestamp).into_bytes())
            }
        }
    }

    pub fn into_bytes(self) -> Bytes {
        match self {
            ValueKind::Bytes(bytes) => bytes,
            ValueKind::Timestamp(timestamp) => timestamp_to_string(&timestamp).into_bytes().into(),
        }
    }
}

fn timestamp_to_string(timestamp: &DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
}

impl From<proto::EventWrapper> for Event {
    fn from(proto: proto::EventWrapper) -> Self {
        let event = proto.event.unwrap();

        match event {
            EventProto::Log(proto) => {
                let structured = proto
                    .structured
                    .into_iter()
                    .map(|(k, v)| {
                        let value = Value {
                            value: v.data.into(),
                            explicit: v.explicit,
                        };
                        (Atom::from(k), value)
                    })
                    .collect::<HashMap<_, _>>();

                Event::Log(LogEvent { structured })
            }
        }
    }
}

impl From<Event> for proto::EventWrapper {
    fn from(record: Event) -> Self {
        match record {
            Event::Log(LogEvent { structured }) => {
                let structured = structured
                    .into_iter()
                    .map(|(k, v)| {
                        let value = proto::Value {
                            data: v.value.as_bytes().into_owned(),
                            explicit: v.explicit,
                        };
                        (k.to_string(), value)
                    })
                    .collect::<HashMap<_, _>>();

                let event = EventProto::Log(Log { structured });

                proto::EventWrapper { event: Some(event) }
            }
        }
    }
}

impl From<Event> for Vec<u8> {
    fn from(record: Event) -> Vec<u8> {
        match record {
            Event::Log(LogEvent { mut structured }) => structured
                .remove(&MESSAGE)
                .unwrap()
                .value
                .as_bytes()
                .into_owned(),
        }
    }
}

impl From<Bytes> for Event {
    fn from(message: Bytes) -> Self {
        let mut record = Event::Log(LogEvent {
            structured: HashMap::new(),
        });

        record
            .as_mut_log()
            .insert_implicit(MESSAGE.clone(), message.into());
        record
            .as_mut_log()
            .insert_implicit(TIMESTAMP.clone(), Utc::now().into());

        record
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
        let mut record = Event::from("raw log line");
        record
            .as_mut_log()
            .insert_explicit("foo".into(), "bar".into());
        record
            .as_mut_log()
            .insert_explicit("bar".into(), "baz".into());

        let expected_all = serde_json::json!({
            "message": "raw log line",
            "foo": "bar",
            "bar": "baz",
            "timestamp": record.as_log().get(&super::TIMESTAMP),
        });

        let expected_explicit = serde_json::json!({
            "foo": "bar",
            "bar": "baz",
        });

        let actual_all = serde_json::to_value(record.all_fields()).unwrap();
        assert_eq!(expected_all, actual_all);

        let actual_explicit = serde_json::to_value(record.explicit_fields()).unwrap();
        assert_eq!(expected_explicit, actual_explicit);

        let rfc3339_re = Regex::new(r"\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z\z").unwrap();
        assert!(rfc3339_re.is_match(actual_all.pointer("/timestamp").unwrap().as_str().unwrap()));
    }

    #[test]
    fn record_iteration() {
        let mut record = Event::new_empty();

        record
            .as_mut_log()
            .insert_explicit("Ke$ha".into(), "It's going down, I'm yelling timber".into());
        record.as_mut_log().insert_implicit(
            "Pitbull".into(),
            "The bigger they are, the harder they fall".into(),
        );

        let all = record
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

        let explicit_only = record
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
