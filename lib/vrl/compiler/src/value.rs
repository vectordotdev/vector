mod arithmetic;
mod convert;
mod error;
pub mod kind;
mod r#macro;
mod path;
mod regex;
mod serde;
mod target;

use bytes::Bytes;
use chrono::{DateTime, SecondsFormat, Utc};
use ordered_float::NotNan;
use std::collections::BTreeMap;
use std::fmt;

pub use self::regex::Regex;
pub use error::Error;
pub use kind::Kind;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bytes(Bytes),
    Integer(i64),
    Float(NotNan<f64>),
    Boolean(bool),
    Object(BTreeMap<String, Value>),
    Array(Vec<Value>),
    Timestamp(DateTime<Utc>),
    Regex(Regex),
    Null,
}

// -----------------------------------------------------------------------------

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bytes(val) => write!(f, r#""{}""#, String::from_utf8_lossy(val)),
            Value::Integer(val) => write!(f, "{}", val),
            Value::Float(val) => write!(f, "{}", val),
            Value::Boolean(val) => write!(f, "{}", val),
            Value::Object(map) => {
                let joined = map
                    .iter()
                    .map(|(key, val)| format!(r#""{}": {}"#, key, val))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{{ {} }}", joined)
            }
            Value::Array(array) => {
                let joined = array
                    .iter()
                    .map(|val| format!("{}", val))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "[{}]", joined)
            }
            Value::Timestamp(val) => {
                write!(f, "{}", val.to_rfc3339_opts(SecondsFormat::AutoSi, true))
            }
            Value::Regex(regex) => write!(f, "/{}/", regex.to_string()),
            Value::Null => write!(f, "null"),
        }
    }
}
