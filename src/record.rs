use bytes::Bytes;
use chrono::{offset::TimeZone, DateTime, Utc};
use prost_types::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

pub mod proto {
    use prost_derive::Message;

    include!(concat!(env!("OUT_DIR"), "/record.proto.rs"));
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Default)]
pub struct Record {
    pub raw: Bytes,
    pub timestamp: Option<DateTime<chrono::Utc>>,
    pub structured: HashMap<Atom, String>,
}

impl From<proto::Record> for Record {
    fn from(proto: proto::Record) -> Self {
        let raw = Bytes::from(proto.raw);

        let timestamp = proto
            .timestamp
            .map(|timestamp| Utc.timestamp(timestamp.seconds, timestamp.nanos as _));

        let structured = proto
            .structured
            .into_iter()
            .map(|(k, v)| (Atom::from(k), v))
            .collect::<HashMap<_, _>>();

        Self {
            raw,
            timestamp,
            structured,
        }
    }
}

impl From<Record> for proto::Record {
    fn from(record: Record) -> Self {
        let raw = record.raw.into_iter().collect::<Vec<u8>>();

        let timestamp = record.timestamp.map(|timestamp| Timestamp {
            seconds: timestamp.timestamp(),
            nanos: timestamp.timestamp_subsec_nanos() as _,
        });

        let structured = record
            .structured
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<HashMap<_, _>>();

        Self {
            raw,
            timestamp,
            structured,
        }
    }
}

impl From<Record> for Vec<u8> {
    fn from(record: Record) -> Vec<u8> {
        record.raw.into_iter().collect()
    }
}

impl From<&str> for Record {
    fn from(line: &str) -> Self {
        line.to_owned().into()
    }
}

impl From<String> for Record {
    fn from(line: String) -> Self {
        let raw = Bytes::from(line);

        Record {
            raw,
            // TODO(lucio): should this be `None`?
            timestamp: Some(chrono::Utc::now()),
            structured: HashMap::new(),
        }
    }
}
