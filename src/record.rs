use self::proto::{record::Event, Log};
use bytes::{Buf, Bytes, IntoBuf};
use chrono::{SecondsFormat, Utc};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
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

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Record {
    #[serde(flatten, serialize_with = "crate::bytes::serialize_map")]
    pub structured: HashMap<Atom, Bytes>,
}

impl Record {
    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.structured[&MESSAGE]).into_owned()
    }
}

impl Default for Record {
    fn default() -> Self {
        Record {
            structured: HashMap::new(),
        }
    }
}

impl From<proto::Record> for Record {
    fn from(proto: proto::Record) -> Self {
        let event = proto.event.unwrap();

        match event {
            Event::Log(proto) => {
                let structured = proto
                    .structured
                    .into_iter()
                    .map(|(k, v)| (Atom::from(k), Bytes::from(v)))
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
            .map(|(k, v)| (k.to_string(), v.into_buf().collect()))
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
            .into_iter()
            .collect()
    }
}

impl From<Bytes> for Record {
    fn from(message: Bytes) -> Self {
        let mut structured = HashMap::new();
        structured.insert(MESSAGE.clone(), message);

        let timestamp = Utc::now();

        structured.insert(
            TIMESTAMP.clone(),
            timestamp
                .to_rfc3339_opts(SecondsFormat::Millis, true)
                .into(),
        );

        Record {
            structured,
            ..Default::default()
        }
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
        record.structured.insert("foo".into(), "bar".into());
        record.structured.insert("bar".into(), "baz".into());

        let expected = serde_json::json!({
            "message": "raw log line",
            "foo": "bar",
            "bar": "baz",
            "timestamp": std::str::from_utf8(&record.structured[&super::TIMESTAMP][..]).unwrap(),
        });
        let actual = serde_json::to_value(record).unwrap();
        assert_eq!(expected, actual);

        let rfc3339_re = Regex::new(r"\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z\z").unwrap();
        assert!(rfc3339_re.is_match(actual.pointer("/timestamp").unwrap().as_str().unwrap()));
    }
}
