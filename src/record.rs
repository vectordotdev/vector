use self::proto::{record::Event, Log};
use bytes::{Buf, Bytes, IntoBuf};
use chrono::{offset::TimeZone, DateTime, Utc};
use prost_types::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/record.proto.rs"));
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Record {
    pub raw: Bytes,
    pub timestamp: DateTime<Utc>,
    pub structured: HashMap<Atom, Bytes>,
}

impl Record {
    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.raw[..]).into_owned()
    }
}

impl Default for Record {
    fn default() -> Self {
        Record {
            raw: Bytes::new(),
            timestamp: Utc::now(),
            structured: HashMap::new(),
        }
    }
}

impl From<proto::Record> for Record {
    fn from(proto: proto::Record) -> Self {
        let event = proto.event.unwrap();

        match event {
            Event::Log(proto) => {
                let raw = Bytes::from(proto.raw);

                let timestamp = proto
                    .timestamp
                    .map(|timestamp| Utc.timestamp(timestamp.seconds, timestamp.nanos as _))
                    .unwrap_or_else(|| Utc::now());

                let structured = proto
                    .structured
                    .into_iter()
                    .map(|(k, v)| (Atom::from(k), Bytes::from(v)))
                    .collect::<HashMap<_, _>>();

                Record {
                    raw,
                    timestamp,
                    structured,
                }
            }
        }
    }
}

impl From<Record> for proto::Record {
    fn from(record: Record) -> Self {
        let raw = record.raw.into_iter().collect::<Vec<u8>>();

        let timestamp = Some(Timestamp {
            seconds: record.timestamp.timestamp(),
            nanos: record.timestamp.timestamp_subsec_nanos() as _,
        });

        let structured = record
            .structured
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.into_buf().collect()))
            .collect::<HashMap<_, _>>();

        let event = Event::Log(Log {
            raw,
            timestamp,
            structured,
        });

        proto::Record { event: Some(event) }
    }
}

impl From<Record> for Vec<u8> {
    fn from(record: Record) -> Vec<u8> {
        record.raw.into_iter().collect()
    }
}

impl From<Bytes> for Record {
    fn from(raw: Bytes) -> Self {
        Record {
            raw,
            timestamp: Utc::now(),
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
        let raw = Bytes::from(line);

        Record {
            raw,
            timestamp: Utc::now(),
            structured: HashMap::new(),
        }
    }
}
