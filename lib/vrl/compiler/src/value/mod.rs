mod arithmetic;
mod convert;
mod error;
pub mod kind;
mod r#macro;
// mod path;
mod regex;
// mod serde;
mod target;

use std::{collections::BTreeMap, fmt};

use bytes::Bytes;
use chrono::{DateTime, SecondsFormat, Utc};
pub use error::Error;
pub use kind::Kind;
use ordered_float::NotNan;

pub use self::regex::Regex;

pub use value::Value;

// #[derive(Debug, Clone, Hash, PartialEq)]
// pub enum Value {
//     Bytes(Bytes),
//     Integer(i64),
//     Float(NotNan<f64>),
//     Boolean(bool),
//     Object(BTreeMap<String, Value>),
//     Array(Vec<Value>),
//     Timestamp(DateTime<Utc>),
//     Regex(Regex),
//     Null,
// }

// impl Eq for Value {}
//

//
// impl From<serde_json::Value> for Value {
//     fn from(json_value: serde_json::Value) -> Self {
//         match json_value {
//             serde_json::Value::Bool(b) => Value::Boolean(b),
//             serde_json::Value::Number(n) if n.is_i64() => n.as_i64().unwrap().into(),
//             serde_json::Value::Number(n) if n.is_f64() => n.as_f64().unwrap().into(),
//             serde_json::Value::Number(n) => n.to_string().into(),
//             serde_json::Value::String(s) => Value::Bytes(Bytes::from(s)),
//             serde_json::Value::Object(obj) => Value::Object(
//                 obj.into_iter()
//                     .map(|(key, value)| (key, Value::from(value)))
//                     .collect(),
//             ),
//             serde_json::Value::Array(arr) => {
//                 Value::Array(arr.into_iter().map(Value::from).collect())
//             }
//             serde_json::Value::Null => Value::Null,
//         }
//     }
// }
