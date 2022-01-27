use bytes::Bytes;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

// --- TODO LIST ----
//TODO: VRL uses standard `PartialEq`, but Vector has odd f64 eq requirements

pub enum Value {
    Bytes(Bytes),
    Integer(i64),
    // TODO: Maybe use NotNan<f64>
    Float(f64),
    Boolean(bool),
    Timestamp(DateTime<Utc>),
    Map(BTreeMap<String, Value>),
    Array(Vec<Value>),

    // TODO: figure out how to make Regex work
    // Regex(Regex),
    Null,
}

/*
// VECTOR
#[derive(PartialOrd, Debug, Clone, Deserialize)]
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

// VRL
#[derive(Debug, Clone, Hash, PartialEq)]
pub enum Value {
    Bytes(Bytes),
    Integer(i64),
    Float(NotNan<f64>),
    Boolean(bool),
    Timestamp(DateTime<Utc>),
    Object(BTreeMap<String, Value>),
    Array(Vec<Value>),

    Regex(Regex),
    Null,
}
*/
