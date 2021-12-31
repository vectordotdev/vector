use bytes::Bytes;
use chrono::{DateTime, Utc};

use crate::conversion::parse_bool;

#[cfg(unix)] // see https://github.com/timberio/vector/issues/1201
mod unix;

#[derive(PartialEq, Debug, Clone)]
enum StubValue {
    Bytes(Bytes),
    Timestamp(DateTime<Utc>),
    Float(f64),
    Integer(i64),
    Boolean(bool),
}

impl From<Bytes> for StubValue {
    fn from(v: Bytes) -> Self {
        StubValue::Bytes(v)
    }
}

impl From<DateTime<Utc>> for StubValue {
    fn from(v: DateTime<Utc>) -> Self {
        StubValue::Timestamp(v)
    }
}

impl From<f64> for StubValue {
    fn from(v: f64) -> Self {
        StubValue::Float(v)
    }
}

impl From<i64> for StubValue {
    fn from(v: i64) -> Self {
        StubValue::Integer(v)
    }
}

impl From<bool> for StubValue {
    fn from(v: bool) -> Self {
        StubValue::Boolean(v)
    }
}

// These should perhaps each go into an individual test function to be
// able to determine what part failed, but that would end up really
// spamming the test logs.

#[test]
fn parse_bool_true() {
    assert_eq!(parse_bool("true"), Ok(true));
    assert_eq!(parse_bool("True"), Ok(true));
    assert_eq!(parse_bool("t"), Ok(true));
    assert_eq!(parse_bool("T"), Ok(true));
    assert_eq!(parse_bool("yes"), Ok(true));
    assert_eq!(parse_bool("YES"), Ok(true));
    assert_eq!(parse_bool("y"), Ok(true));
    assert_eq!(parse_bool("Y"), Ok(true));
    assert_eq!(parse_bool("1"), Ok(true));
    assert_eq!(parse_bool("23456"), Ok(true));
    assert_eq!(parse_bool("-8"), Ok(true));
}

#[test]
fn parse_bool_false() {
    assert_eq!(parse_bool("false"), Ok(false));
    assert_eq!(parse_bool("fAlSE"), Ok(false));
    assert_eq!(parse_bool("f"), Ok(false));
    assert_eq!(parse_bool("F"), Ok(false));
    assert_eq!(parse_bool("no"), Ok(false));
    assert_eq!(parse_bool("NO"), Ok(false));
    assert_eq!(parse_bool("n"), Ok(false));
    assert_eq!(parse_bool("N"), Ok(false));
    assert_eq!(parse_bool("0"), Ok(false));
    assert_eq!(parse_bool("000"), Ok(false));
}

#[test]
fn parse_bool_errors() {
    assert!(parse_bool("X").is_err());
    assert!(parse_bool("yes or no").is_err());
    assert!(parse_bool("123.4").is_err());
}
