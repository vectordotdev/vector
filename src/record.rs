use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

pub mod proto {
    use prost_derive::Message;

    include!(concat!(env!("OUT_DIR"), "/record.proto.rs"));
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Record {
    pub line: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub custom: HashMap<Atom, String>,
    pub host: Option<String>,
}

impl From<proto::Record> for Record {
    fn from(proto: proto::Record) -> Self {
        use chrono::offset::TimeZone;
        let timestamp = proto.timestamp.unwrap();
        let timestamp = chrono::Utc.timestamp(timestamp.seconds, timestamp.nanos as _);

        let host = if proto.host.is_empty() {
            None
        } else {
            Some(proto.host)
        };

        let custom = proto
            .custom
            .into_iter()
            .map(|(k, v)| (Atom::from(k), v))
            .collect::<HashMap<_, _>>();

        Self {
            line: proto.line,
            timestamp,
            custom,
            host,
        }
    }
}

impl From<Record> for proto::Record {
    fn from(record: Record) -> Self {
        let timestamp = ::prost_types::Timestamp {
            seconds: record.timestamp.timestamp(),
            nanos: record.timestamp.timestamp_subsec_nanos() as _,
        };

        let custom = record
            .custom
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<HashMap<_, _>>();

        Self {
            line: record.line,
            timestamp: Some(timestamp),
            custom,
            host: record.host.unwrap_or_else(|| "".to_string()),
        }
    }
}

impl From<Record> for Vec<u8> {
    fn from(record: Record) -> Vec<u8> {
        record.line.into_bytes()
    }
}

impl From<&str> for Record {
    fn from(line: &str) -> Self {
        line.to_owned().into()
    }
}

impl From<String> for Record {
    fn from(line: String) -> Self {
        Record {
            line: line.into(),
            timestamp: chrono::Utc::now(),
            custom: HashMap::new(),
            host: None,
        }
    }
}
