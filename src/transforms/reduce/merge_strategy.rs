use std::collections::HashSet;

use crate::event::{LogEvent, Value};
use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use vector_lib::configurable::configurable_component;
use vrl::path::OwnedTargetPath;

/// Strategies for merging events.
#[configurable_component]
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// Discard all but the first value found.
    Discard,

    /// Discard all but the last value found.
    ///
    /// Works as a way to coalesce by not retaining `null`.
    Retain,

    /// Sum all numeric values.
    Sum,

    /// Keep the maximum numeric value seen.
    Max,

    /// Keep the minimum numeric value seen.
    Min,

    /// Append each value to an array.
    Array,

    /// Concatenate each string value, delimited with a space.
    Concat,

    /// Concatenate each string value, delimited with a newline.
    ConcatNewline,

    /// Concatenate each string, without a delimiter.
    ConcatRaw,

    /// Keep the shortest array seen.
    ShortestArray,

    /// Keep the longest array seen.
    LongestArray,

    /// Create a flattened array of all unique values.
    FlatUnique,
}

#[derive(Debug, Clone)]
struct DiscardMerger {
    v: Value,
}

impl DiscardMerger {
    const fn new(v: Value) -> Self {
        Self { v }
    }
}

impl ReduceValueMerger for DiscardMerger {
    fn add(&mut self, _v: Value) -> Result<(), String> {
        Ok(())
    }

    fn insert_into(
        self: Box<Self>,
        path: &OwnedTargetPath,
        v: &mut LogEvent,
    ) -> Result<(), String> {
        v.insert(path, self.v);
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct RetainMerger {
    v: Value,
}

impl RetainMerger {
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    fn new(v: Value) -> Self {
        Self { v }
    }
}

impl ReduceValueMerger for RetainMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        if Value::Null != v {
            self.v = v;
        }
        Ok(())
    }

    fn insert_into(
        self: Box<Self>,
        path: &OwnedTargetPath,
        v: &mut LogEvent,
    ) -> Result<(), String> {
        v.insert(path, self.v);
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ConcatMerger {
    v: BytesMut,
    join_by: Option<Vec<u8>>,
}

impl ConcatMerger {
    fn new(v: Bytes, join_by: Option<char>) -> Self {
        // We need to get the resulting bytes for this character in case it's actually a multi-byte character.
        let join_by = join_by.map(|c| c.to_string().into_bytes());

        Self {
            v: BytesMut::from(&v[..]),
            join_by,
        }
    }
}

impl ReduceValueMerger for ConcatMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        if let Value::Bytes(b) = v {
            if let Some(buf) = self.join_by.as_ref() {
                self.v.extend(&buf[..]);
            }
            self.v.extend_from_slice(&b);
            Ok(())
        } else {
            Err(format!(
                "expected string value, found: '{}'",
                v.to_string_lossy()
            ))
        }
    }

    fn insert_into(
        self: Box<Self>,
        path: &OwnedTargetPath,
        v: &mut LogEvent,
    ) -> Result<(), String> {
        v.insert(path, Value::Bytes(self.v.into()));
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ConcatArrayMerger {
    v: Vec<Value>,
}

impl ConcatArrayMerger {
    fn new(v: Vec<Value>) -> Self {
        Self { v }
    }
}

impl ReduceValueMerger for ConcatArrayMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        if let Value::Array(a) = v {
            self.v.extend_from_slice(&a);
        } else {
            self.v.push(v);
        }
        Ok(())
    }

    fn insert_into(
        self: Box<Self>,
        path: &OwnedTargetPath,
        v: &mut LogEvent,
    ) -> Result<(), String> {
        v.insert(path, Value::Array(self.v));
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ArrayMerger {
    v: Vec<Value>,
}

impl ArrayMerger {
    fn new(v: Value) -> Self {
        Self { v: vec![v] }
    }
}

impl ReduceValueMerger for ArrayMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        self.v.push(v);
        Ok(())
    }

    fn insert_into(
        self: Box<Self>,
        path: &OwnedTargetPath,
        v: &mut LogEvent,
    ) -> Result<(), String> {
        v.insert(path, Value::Array(self.v));
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct LongestArrayMerger {
    v: Vec<Value>,
}

impl LongestArrayMerger {
    fn new(v: Vec<Value>) -> Self {
        Self { v }
    }
}

impl ReduceValueMerger for LongestArrayMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        if let Value::Array(a) = v {
            if a.len() > self.v.len() {
                self.v = a;
            }
            Ok(())
        } else {
            Err(format!(
                "expected array value, found: '{}'",
                v.to_string_lossy()
            ))
        }
    }

    fn insert_into(
        self: Box<Self>,
        path: &OwnedTargetPath,
        v: &mut LogEvent,
    ) -> Result<(), String> {
        v.insert(path, Value::Array(self.v));
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ShortestArrayMerger {
    v: Vec<Value>,
}

impl ShortestArrayMerger {
    fn new(v: Vec<Value>) -> Self {
        Self { v }
    }
}

impl ReduceValueMerger for ShortestArrayMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        if let Value::Array(a) = v {
            if a.len() < self.v.len() {
                self.v = a;
            }
            Ok(())
        } else {
            Err(format!(
                "expected array value, found: '{}'",
                v.to_string_lossy()
            ))
        }
    }

    fn insert_into(
        self: Box<Self>,
        path: &OwnedTargetPath,
        v: &mut LogEvent,
    ) -> Result<(), String> {
        v.insert(path, Value::Array(self.v));
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct FlatUniqueMerger {
    v: HashSet<Value>,
}

#[allow(clippy::mutable_key_type)] // false positive due to bytes::Bytes
fn insert_value(h: &mut HashSet<Value>, v: Value) {
    match v {
        Value::Object(m) => {
            for (_, v) in m {
                h.insert(v);
            }
        }
        Value::Array(vec) => {
            for v in vec {
                h.insert(v);
            }
        }
        _ => {
            h.insert(v);
        }
    }
}

impl FlatUniqueMerger {
    #[allow(clippy::mutable_key_type)] // false positive due to bytes::Bytes
    fn new(v: Value) -> Self {
        let mut h = HashSet::default();
        insert_value(&mut h, v);
        Self { v: h }
    }
}

impl ReduceValueMerger for FlatUniqueMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        insert_value(&mut self.v, v);
        Ok(())
    }

    fn insert_into(
        self: Box<Self>,
        path: &OwnedTargetPath,
        v: &mut LogEvent,
    ) -> Result<(), String> {
        v.insert(path, Value::Array(self.v.into_iter().collect()));
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct TimestampWindowMerger {
    started: DateTime<Utc>,
    latest: DateTime<Utc>,
}

impl TimestampWindowMerger {
    const fn new(v: DateTime<Utc>) -> Self {
        Self {
            started: v,
            latest: v,
        }
    }
}

impl ReduceValueMerger for TimestampWindowMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        if let Value::Timestamp(ts) = v {
            self.latest = ts
        } else {
            return Err(format!(
                "expected timestamp value, found: {}",
                v.to_string_lossy()
            ));
        }
        Ok(())
    }

    fn insert_into(
        self: Box<Self>,
        path: &OwnedTargetPath,
        v: &mut LogEvent,
    ) -> Result<(), String> {
        v.insert(
            format!("{}_end", path).as_str(),
            Value::Timestamp(self.latest),
        );
        v.insert(path, Value::Timestamp(self.started));
        Ok(())
    }
}

#[derive(Debug, Clone)]
enum NumberMergerValue {
    Int(i64),
    Float(NotNan<f64>),
}

impl From<i64> for NumberMergerValue {
    fn from(v: i64) -> Self {
        NumberMergerValue::Int(v)
    }
}

impl From<NotNan<f64>> for NumberMergerValue {
    fn from(v: NotNan<f64>) -> Self {
        NumberMergerValue::Float(v)
    }
}

#[derive(Debug, Clone)]
struct AddNumbersMerger {
    v: NumberMergerValue,
}

impl AddNumbersMerger {
    const fn new(v: NumberMergerValue) -> Self {
        Self { v }
    }
}

impl ReduceValueMerger for AddNumbersMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        // Try and keep max precision with integer values, but once we've
        // received a float downgrade to float precision.
        match v {
            Value::Integer(i) => match self.v {
                NumberMergerValue::Int(j) => self.v = NumberMergerValue::Int(i + j),
                NumberMergerValue::Float(j) => {
                    self.v = NumberMergerValue::Float(NotNan::new(i as f64).unwrap() + j)
                }
            },
            Value::Float(f) => match self.v {
                NumberMergerValue::Int(j) => self.v = NumberMergerValue::Float(f + j as f64),
                NumberMergerValue::Float(j) => self.v = NumberMergerValue::Float(f + j),
            },
            _ => {
                return Err(format!(
                    "expected numeric value, found: '{}'",
                    v.to_string_lossy()
                ));
            }
        }
        Ok(())
    }

    fn insert_into(
        self: Box<Self>,
        path: &OwnedTargetPath,
        v: &mut LogEvent,
    ) -> Result<(), String> {
        match self.v {
            NumberMergerValue::Float(f) => v.insert(path, Value::Float(f)),
            NumberMergerValue::Int(i) => v.insert(path, Value::Integer(i)),
        };
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct MaxNumberMerger {
    v: NumberMergerValue,
}

impl MaxNumberMerger {
    const fn new(v: NumberMergerValue) -> Self {
        Self { v }
    }
}

impl ReduceValueMerger for MaxNumberMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        // Try and keep max precision with integer values, but once we've
        // received a float downgrade to float precision.
        match v {
            Value::Integer(i) => {
                match self.v {
                    NumberMergerValue::Int(i2) => {
                        if i > i2 {
                            self.v = NumberMergerValue::Int(i);
                        }
                    }
                    NumberMergerValue::Float(f2) => {
                        let f = NotNan::new(i as f64).unwrap();
                        if f > f2 {
                            self.v = NumberMergerValue::Float(f);
                        }
                    }
                };
            }
            Value::Float(f) => {
                let f2 = match self.v {
                    NumberMergerValue::Int(i2) => NotNan::new(i2 as f64).unwrap(),
                    NumberMergerValue::Float(f2) => f2,
                };
                if f > f2 {
                    self.v = NumberMergerValue::Float(f);
                }
            }
            _ => {
                return Err(format!(
                    "expected numeric value, found: '{}'",
                    v.to_string_lossy()
                ));
            }
        }
        Ok(())
    }

    fn insert_into(
        self: Box<Self>,
        path: &OwnedTargetPath,
        v: &mut LogEvent,
    ) -> Result<(), String> {
        match self.v {
            NumberMergerValue::Float(f) => v.insert(path, Value::Float(f)),
            NumberMergerValue::Int(i) => v.insert(path, Value::Integer(i)),
        };
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct MinNumberMerger {
    v: NumberMergerValue,
}

impl MinNumberMerger {
    const fn new(v: NumberMergerValue) -> Self {
        Self { v }
    }
}

impl ReduceValueMerger for MinNumberMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        // Try and keep max precision with integer values, but once we've
        // received a float downgrade to float precision.
        match v {
            Value::Integer(i) => {
                match self.v {
                    NumberMergerValue::Int(i2) => {
                        if i < i2 {
                            self.v = NumberMergerValue::Int(i);
                        }
                    }
                    NumberMergerValue::Float(f2) => {
                        let f = NotNan::new(i as f64).unwrap();
                        if f < f2 {
                            self.v = NumberMergerValue::Float(f);
                        }
                    }
                };
            }
            Value::Float(f) => {
                let f2 = match self.v {
                    NumberMergerValue::Int(i2) => NotNan::new(i2 as f64).unwrap(),
                    NumberMergerValue::Float(f2) => f2,
                };
                if f < f2 {
                    self.v = NumberMergerValue::Float(f);
                }
            }
            _ => {
                return Err(format!(
                    "expected numeric value, found: '{}'",
                    v.to_string_lossy()
                ));
            }
        }
        Ok(())
    }

    fn insert_into(
        self: Box<Self>,
        path: &OwnedTargetPath,
        v: &mut LogEvent,
    ) -> Result<(), String> {
        match self.v {
            NumberMergerValue::Float(f) => v.insert(path, Value::Float(f)),
            NumberMergerValue::Int(i) => v.insert(path, Value::Integer(i)),
        };
        Ok(())
    }
}

pub trait ReduceValueMerger: std::fmt::Debug + Send + Sync {
    fn add(&mut self, v: Value) -> Result<(), String>;
    fn insert_into(self: Box<Self>, path: &OwnedTargetPath, v: &mut LogEvent)
        -> Result<(), String>;
}

impl From<Value> for Box<dyn ReduceValueMerger> {
    fn from(v: Value) -> Self {
        match v {
            Value::Integer(i) => Box::new(AddNumbersMerger::new(i.into())),
            Value::Float(f) => Box::new(AddNumbersMerger::new(f.into())),
            Value::Timestamp(ts) => Box::new(TimestampWindowMerger::new(ts)),
            Value::Object(_) => Box::new(DiscardMerger::new(v)),
            Value::Null => Box::new(DiscardMerger::new(v)),
            Value::Boolean(_) => Box::new(DiscardMerger::new(v)),
            Value::Bytes(_) => Box::new(DiscardMerger::new(v)),
            Value::Regex(_) => Box::new(DiscardMerger::new(v)),
            Value::Array(_) => Box::new(DiscardMerger::new(v)),
        }
    }
}

pub(crate) fn get_value_merger(
    v: Value,
    m: &MergeStrategy,
) -> Result<Box<dyn ReduceValueMerger>, String> {
    match m {
        MergeStrategy::Sum => match v {
            Value::Integer(i) => Ok(Box::new(AddNumbersMerger::new(i.into()))),
            Value::Float(f) => Ok(Box::new(AddNumbersMerger::new(f.into()))),
            _ => Err(format!(
                "expected number value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::Max => match v {
            Value::Integer(i) => Ok(Box::new(MaxNumberMerger::new(i.into()))),
            Value::Float(f) => Ok(Box::new(MaxNumberMerger::new(f.into()))),
            _ => Err(format!(
                "expected number value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::Min => match v {
            Value::Integer(i) => Ok(Box::new(MinNumberMerger::new(i.into()))),
            Value::Float(f) => Ok(Box::new(MinNumberMerger::new(f.into()))),
            _ => Err(format!(
                "expected number value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::Concat => match v {
            Value::Bytes(b) => Ok(Box::new(ConcatMerger::new(b, Some(' ')))),
            Value::Array(a) => Ok(Box::new(ConcatArrayMerger::new(a))),
            _ => Err(format!(
                "expected string or array value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::ConcatNewline => match v {
            Value::Bytes(b) => Ok(Box::new(ConcatMerger::new(b, Some('\n')))),
            _ => Err(format!(
                "expected string value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::ConcatRaw => match v {
            Value::Bytes(b) => Ok(Box::new(ConcatMerger::new(b, None))),
            _ => Err(format!(
                "expected string value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::Array => Ok(Box::new(ArrayMerger::new(v))),
        MergeStrategy::ShortestArray => match v {
            Value::Array(a) => Ok(Box::new(ShortestArrayMerger::new(a))),
            _ => Err(format!(
                "expected array value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::LongestArray => match v {
            Value::Array(a) => Ok(Box::new(LongestArrayMerger::new(a))),
            _ => Err(format!(
                "expected array value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::Discard => Ok(Box::new(DiscardMerger::new(v))),
        MergeStrategy::Retain => Ok(Box::new(RetainMerger::new(v))),
        MergeStrategy::FlatUnique => Ok(Box::new(FlatUniqueMerger::new(v))),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::event::LogEvent;
    use serde_json::json;
    use vrl::owned_event_path;

    #[test]
    fn initial_values() {
        assert!(get_value_merger("foo".into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger("foo".into(), &MergeStrategy::Retain).is_ok());
        assert!(get_value_merger("foo".into(), &MergeStrategy::Sum).is_err());
        assert!(get_value_merger("foo".into(), &MergeStrategy::Max).is_err());
        assert!(get_value_merger("foo".into(), &MergeStrategy::Min).is_err());
        assert!(get_value_merger("foo".into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger("foo".into(), &MergeStrategy::LongestArray).is_err());
        assert!(get_value_merger("foo".into(), &MergeStrategy::ShortestArray).is_err());
        assert!(get_value_merger("foo".into(), &MergeStrategy::Concat).is_ok());
        assert!(get_value_merger("foo".into(), &MergeStrategy::ConcatNewline).is_ok());
        assert!(get_value_merger("foo".into(), &MergeStrategy::ConcatRaw).is_ok());
        assert!(get_value_merger("foo".into(), &MergeStrategy::FlatUnique).is_ok());

        assert!(get_value_merger(42.into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(42.into(), &MergeStrategy::Retain).is_ok());
        assert!(get_value_merger(42.into(), &MergeStrategy::Sum).is_ok());
        assert!(get_value_merger(42.into(), &MergeStrategy::Min).is_ok());
        assert!(get_value_merger(42.into(), &MergeStrategy::Max).is_ok());
        assert!(get_value_merger(42.into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(42.into(), &MergeStrategy::LongestArray).is_err());
        assert!(get_value_merger(42.into(), &MergeStrategy::ShortestArray).is_err());
        assert!(get_value_merger(42.into(), &MergeStrategy::Concat).is_err());
        assert!(get_value_merger(42.into(), &MergeStrategy::ConcatNewline).is_err());
        assert!(get_value_merger(42.into(), &MergeStrategy::ConcatRaw).is_err());
        assert!(get_value_merger(42.into(), &MergeStrategy::FlatUnique).is_ok());

        assert!(get_value_merger(42.into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(42.into(), &MergeStrategy::Retain).is_ok());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::Sum).is_ok());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::Min).is_ok());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::Max).is_ok());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::LongestArray).is_err());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::ShortestArray).is_err());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::Concat).is_err());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::ConcatNewline).is_err());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::ConcatRaw).is_err());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::FlatUnique).is_ok());

        assert!(get_value_merger(true.into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(true.into(), &MergeStrategy::Retain).is_ok());
        assert!(get_value_merger(true.into(), &MergeStrategy::Sum).is_err());
        assert!(get_value_merger(true.into(), &MergeStrategy::Max).is_err());
        assert!(get_value_merger(true.into(), &MergeStrategy::Min).is_err());
        assert!(get_value_merger(true.into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(true.into(), &MergeStrategy::LongestArray).is_err());
        assert!(get_value_merger(true.into(), &MergeStrategy::ShortestArray).is_err());
        assert!(get_value_merger(true.into(), &MergeStrategy::Concat).is_err());
        assert!(get_value_merger(true.into(), &MergeStrategy::ConcatNewline).is_err());
        assert!(get_value_merger(true.into(), &MergeStrategy::ConcatRaw).is_err());
        assert!(get_value_merger(true.into(), &MergeStrategy::FlatUnique).is_ok());

        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Retain).is_ok());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Sum).is_err());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Max).is_err());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Min).is_err());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::LongestArray).is_err());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::ShortestArray).is_err());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Concat).is_err());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::ConcatNewline).is_err());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::ConcatRaw).is_err());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::FlatUnique).is_ok());

        assert!(get_value_merger(json!([]).into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::Retain).is_ok());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::Sum).is_err());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::Max).is_err());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::Min).is_err());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::LongestArray).is_ok());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::ShortestArray).is_ok());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::Concat).is_ok());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::ConcatNewline).is_err());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::ConcatRaw).is_err());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::FlatUnique).is_ok());

        assert!(get_value_merger(json!({}).into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::Retain).is_ok());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::Sum).is_err());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::Max).is_err());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::Min).is_err());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::LongestArray).is_err());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::ShortestArray).is_err());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::Concat).is_err());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::ConcatNewline).is_err());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::ConcatRaw).is_err());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::FlatUnique).is_ok());

        assert!(get_value_merger(json!(null).into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::Retain).is_ok());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::Sum).is_err());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::Max).is_err());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::Min).is_err());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::LongestArray).is_err());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::ShortestArray).is_err());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::Concat).is_err());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::ConcatNewline).is_err());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::ConcatRaw).is_err());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::FlatUnique).is_ok());
    }

    #[test]
    fn merging_values() {
        assert_eq!(
            merge("foo".into(), "bar".into(), &MergeStrategy::Discard),
            Ok("foo".into())
        );
        assert_eq!(
            merge("foo".into(), "bar".into(), &MergeStrategy::Retain),
            Ok("bar".into())
        );
        assert_eq!(
            merge("foo".into(), "bar".into(), &MergeStrategy::Array),
            Ok(json!(["foo", "bar"]).into())
        );
        assert_eq!(
            merge("foo".into(), "bar".into(), &MergeStrategy::Concat),
            Ok("foo bar".into())
        );
        assert_eq!(
            merge("foo".into(), "bar".into(), &MergeStrategy::ConcatNewline),
            Ok("foo\nbar".into())
        );
        assert_eq!(
            merge("foo".into(), "bar".into(), &MergeStrategy::ConcatRaw),
            Ok("foobar".into())
        );
        assert!(merge("foo".into(), 42.into(), &MergeStrategy::Concat).is_err());
        assert!(merge("foo".into(), 4.2.into(), &MergeStrategy::Concat).is_err());
        assert!(merge("foo".into(), true.into(), &MergeStrategy::Concat).is_err());
        assert!(merge("foo".into(), Utc::now().into(), &MergeStrategy::Concat).is_err());
        assert!(merge("foo".into(), json!({}).into(), &MergeStrategy::Concat).is_err());
        assert!(merge("foo".into(), json!([]).into(), &MergeStrategy::Concat).is_err());
        assert!(merge("foo".into(), json!(null).into(), &MergeStrategy::Concat).is_err());

        assert_eq!(
            merge("foo".into(), "bar".into(), &MergeStrategy::ConcatNewline),
            Ok("foo\nbar".into())
        );

        assert_eq!(
            merge(21.into(), 21.into(), &MergeStrategy::Sum),
            Ok(42.into())
        );
        assert_eq!(
            merge(41.into(), 42.into(), &MergeStrategy::Max),
            Ok(42.into())
        );
        assert_eq!(
            merge(42.into(), 41.into(), &MergeStrategy::Max),
            Ok(42.into())
        );
        assert_eq!(
            merge(42.into(), 43.into(), &MergeStrategy::Min),
            Ok(42.into())
        );
        assert_eq!(
            merge(43.into(), 42.into(), &MergeStrategy::Min),
            Ok(42.into())
        );

        assert_eq!(
            merge(2.1.into(), 2.1.into(), &MergeStrategy::Sum),
            Ok(4.2.into())
        );
        assert_eq!(
            merge(4.1.into(), 4.2.into(), &MergeStrategy::Max),
            Ok(4.2.into())
        );
        assert_eq!(
            merge(4.2.into(), 4.1.into(), &MergeStrategy::Max),
            Ok(4.2.into())
        );
        assert_eq!(
            merge(4.2.into(), 4.3.into(), &MergeStrategy::Min),
            Ok(4.2.into())
        );
        assert_eq!(
            merge(4.3.into(), 4.2.into(), &MergeStrategy::Min),
            Ok(4.2.into())
        );

        assert_eq!(
            merge(
                json!([4_i64]).into(),
                json!([2_i64]).into(),
                &MergeStrategy::Concat
            ),
            Ok(json!([4_i64, 2_i64]).into())
        );
        assert_eq!(
            merge(json!([]).into(), 42_i64.into(), &MergeStrategy::Concat),
            Ok(json!([42_i64]).into())
        );

        assert_eq!(
            merge(
                json!([34_i64]).into(),
                json!([42_i64, 43_i64]).into(),
                &MergeStrategy::ShortestArray
            ),
            Ok(json!([34_i64]).into())
        );
        assert_eq!(
            merge(
                json!([34_i64]).into(),
                json!([42_i64, 43_i64]).into(),
                &MergeStrategy::LongestArray
            ),
            Ok(json!([42_i64, 43_i64]).into())
        );

        let v = merge(34_i64.into(), 43_i64.into(), &MergeStrategy::FlatUnique).unwrap();
        if let Value::Array(v) = v.clone() {
            let v: Vec<_> = v
                .into_iter()
                .map(|i| {
                    if let Value::Integer(i) = i {
                        i
                    } else {
                        panic!("Bad value");
                    }
                })
                .collect();
            assert_eq!(v.iter().filter(|i| **i == 34i64).count(), 1);
            assert_eq!(v.iter().filter(|i| **i == 43i64).count(), 1);
        } else {
            panic!("Not array");
        }
        let v = merge(v, 34_i32.into(), &MergeStrategy::FlatUnique).unwrap();
        if let Value::Array(v) = v {
            let v: Vec<_> = v
                .into_iter()
                .map(|i| {
                    if let Value::Integer(i) = i {
                        i
                    } else {
                        panic!("Bad value");
                    }
                })
                .collect();
            assert_eq!(v.iter().filter(|i| **i == 34i64).count(), 1);
            assert_eq!(v.iter().filter(|i| **i == 43i64).count(), 1);
        } else {
            panic!("Not array");
        }
    }

    fn merge(initial: Value, additional: Value, strategy: &MergeStrategy) -> Result<Value, String> {
        let mut merger = get_value_merger(initial, strategy)?;
        merger.add(additional)?;
        let mut output = LogEvent::default();
        let out_path = owned_event_path!("out");
        merger.insert_into(&out_path, &mut output)?;
        Ok(output.remove(&out_path).unwrap())
    }
}
