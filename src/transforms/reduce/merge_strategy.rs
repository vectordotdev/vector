use crate::event::{LogEvent, Value};
use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    Discard,
    Sum,
    Max,
    Min,
    Array,
    Concat,
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct DiscardMerger {
    v: Value,
}

impl DiscardMerger {
    fn new(v: Value) -> Self {
        Self { v }
    }
}

impl ReduceValueMerger for DiscardMerger {
    fn add(&mut self, _v: Value) -> Result<(), String> {
        Ok(())
    }

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        v.insert(k, self.v);
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ConcatMerger {
    v: BytesMut,
}

impl ConcatMerger {
    fn new(v: Bytes) -> Self {
        Self { v: v.into() }
    }
}

impl ReduceValueMerger for ConcatMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        if let Value::Bytes(b) = v {
            self.v.extend(&[b' ']);
            self.v.extend_from_slice(&b);
            Ok(())
        } else {
            Err(format!(
                "expected string value, found: '{}'",
                v.to_string_lossy()
            ))
        }
    }

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        v.insert(k, Value::Bytes(self.v.into()));
        Ok(())
    }
}

//------------------------------------------------------------------------------

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

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        v.insert(k, Value::Array(self.v));
        Ok(())
    }
}

//------------------------------------------------------------------------------

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

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        v.insert(k, Value::Array(self.v));
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TimestampWindowMerger {
    started: DateTime<Utc>,
    latest: DateTime<Utc>,
}

impl TimestampWindowMerger {
    fn new(v: DateTime<Utc>) -> Self {
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

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        v.insert(format!("{}_end", k), Value::Timestamp(self.latest));
        v.insert(k, Value::Timestamp(self.started));
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum NumberMergerValue {
    Int(i64),
    Float(f64),
}

impl From<i64> for NumberMergerValue {
    fn from(v: i64) -> Self {
        NumberMergerValue::Int(v)
    }
}

impl From<f64> for NumberMergerValue {
    fn from(v: f64) -> Self {
        NumberMergerValue::Float(v)
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct AddNumbersMerger {
    v: NumberMergerValue,
}

impl AddNumbersMerger {
    fn new(v: NumberMergerValue) -> Self {
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
                NumberMergerValue::Float(j) => self.v = NumberMergerValue::Float(i as f64 + j),
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

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        match self.v {
            NumberMergerValue::Float(f) => v.insert(k, Value::Float(f)),
            NumberMergerValue::Int(i) => v.insert(k, Value::Integer(i)),
        };
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MaxNumberMerger {
    v: NumberMergerValue,
}

impl MaxNumberMerger {
    fn new(v: NumberMergerValue) -> Self {
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
                        let f = i as f64;
                        if f > f2 {
                            self.v = NumberMergerValue::Float(f);
                        }
                    }
                };
            }
            Value::Float(f) => {
                let f2 = match self.v {
                    NumberMergerValue::Int(i2) => i2 as f64,
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

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        match self.v {
            NumberMergerValue::Float(f) => v.insert(k, Value::Float(f)),
            NumberMergerValue::Int(i) => v.insert(k, Value::Integer(i)),
        };
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MinNumberMerger {
    v: NumberMergerValue,
}

impl MinNumberMerger {
    fn new(v: NumberMergerValue) -> Self {
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
                        let f = i as f64;
                        if f < f2 {
                            self.v = NumberMergerValue::Float(f);
                        }
                    }
                };
            }
            Value::Float(f) => {
                let f2 = match self.v {
                    NumberMergerValue::Int(i2) => i2 as f64,
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

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        match self.v {
            NumberMergerValue::Float(f) => v.insert(k, Value::Float(f)),
            NumberMergerValue::Int(i) => v.insert(k, Value::Integer(i)),
        };
        Ok(())
    }
}

//------------------------------------------------------------------------------

pub trait ReduceValueMerger: std::fmt::Debug + Send + Sync {
    fn add(&mut self, v: Value) -> Result<(), String>;
    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String>;
}

impl From<Value> for Box<dyn ReduceValueMerger> {
    fn from(v: Value) -> Self {
        match v {
            Value::Integer(i) => Box::new(AddNumbersMerger::new(i.into())),
            Value::Float(f) => Box::new(AddNumbersMerger::new(f.into())),
            Value::Timestamp(ts) => Box::new(TimestampWindowMerger::new(ts)),
            Value::Map(_) => Box::new(DiscardMerger::new(v)),
            Value::Null => Box::new(DiscardMerger::new(v)),
            Value::Boolean(_) => Box::new(DiscardMerger::new(v)),
            Value::Bytes(_) => Box::new(DiscardMerger::new(v)),
            Value::Array(_) => Box::new(DiscardMerger::new(v)),
        }
    }
}

pub fn get_value_merger(v: Value, m: &MergeStrategy) -> Result<Box<dyn ReduceValueMerger>, String> {
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
            Value::Bytes(b) => Ok(Box::new(ConcatMerger::new(b))),
            Value::Array(a) => Ok(Box::new(ConcatArrayMerger::new(a))),
            _ => Err(format!(
                "expected string or array value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::Array => Ok(Box::new(ArrayMerger::new(v))),
        MergeStrategy::Discard => Ok(Box::new(DiscardMerger::new(v))),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Event;
    use serde_json::json;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn initial_values() {
        assert!(get_value_merger("foo".into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger("foo".into(), &MergeStrategy::Sum).is_err());
        assert!(get_value_merger("foo".into(), &MergeStrategy::Max).is_err());
        assert!(get_value_merger("foo".into(), &MergeStrategy::Min).is_err());
        assert!(get_value_merger("foo".into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger("foo".into(), &MergeStrategy::Concat).is_ok());

        assert!(get_value_merger(42.into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(42.into(), &MergeStrategy::Sum).is_ok());
        assert!(get_value_merger(42.into(), &MergeStrategy::Min).is_ok());
        assert!(get_value_merger(42.into(), &MergeStrategy::Max).is_ok());
        assert!(get_value_merger(42.into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(42.into(), &MergeStrategy::Concat).is_err());

        assert!(get_value_merger(4.2.into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::Sum).is_ok());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::Min).is_ok());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::Max).is_ok());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(4.2.into(), &MergeStrategy::Concat).is_err());

        assert!(get_value_merger(true.into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(true.into(), &MergeStrategy::Sum).is_err());
        assert!(get_value_merger(true.into(), &MergeStrategy::Max).is_err());
        assert!(get_value_merger(true.into(), &MergeStrategy::Min).is_err());
        assert!(get_value_merger(true.into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(true.into(), &MergeStrategy::Concat).is_err());

        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Sum).is_err());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Max).is_err());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Min).is_err());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(Utc::now().into(), &MergeStrategy::Concat).is_err());

        assert!(get_value_merger(json!([]).into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::Sum).is_err());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::Max).is_err());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::Min).is_err());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(json!([]).into(), &MergeStrategy::Concat).is_ok());

        assert!(get_value_merger(json!({}).into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::Sum).is_err());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::Max).is_err());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::Min).is_err());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(json!({}).into(), &MergeStrategy::Concat).is_err());

        assert!(get_value_merger(json!(null).into(), &MergeStrategy::Discard).is_ok());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::Sum).is_err());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::Max).is_err());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::Min).is_err());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::Array).is_ok());
        assert!(get_value_merger(json!(null).into(), &MergeStrategy::Concat).is_err());
    }

    #[test]
    fn merging_values() {
        assert_eq!(
            merge("foo".into(), "bar".into(), &MergeStrategy::Discard),
            Ok("foo".into())
        );
        assert_eq!(
            merge("foo".into(), "bar".into(), &MergeStrategy::Array),
            Ok(json!(["foo", "bar"]).into())
        );
        assert_eq!(
            merge("foo".into(), "bar".into(), &MergeStrategy::Concat),
            Ok("foo bar".into())
        );
        assert!(merge("foo".into(), 42.into(), &MergeStrategy::Concat).is_err());
        assert!(merge("foo".into(), 4.2.into(), &MergeStrategy::Concat).is_err());
        assert!(merge("foo".into(), true.into(), &MergeStrategy::Concat).is_err());
        assert!(merge("foo".into(), Utc::now().into(), &MergeStrategy::Concat).is_err());
        assert!(merge("foo".into(), json!({}).into(), &MergeStrategy::Concat).is_err());
        assert!(merge("foo".into(), json!([]).into(), &MergeStrategy::Concat).is_err());
        assert!(merge("foo".into(), json!(null).into(), &MergeStrategy::Concat).is_err());

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
            merge(json!([4]).into(), json!([2]).into(), &MergeStrategy::Concat),
            Ok(json!([4, 2]).into())
        );
        assert_eq!(
            merge(json!([]).into(), 42.into(), &MergeStrategy::Concat),
            Ok(json!([42]).into())
        );
    }

    fn merge(initial: Value, additional: Value, strategy: &MergeStrategy) -> Result<Value, String> {
        let mut merger = get_value_merger(initial, strategy)?;
        merger.add(additional)?;
        let mut output = Event::new_empty_log();
        let mut output = output.as_mut_log();
        merger.insert_into("out".into(), &mut output)?;
        Ok(output.remove(&Atom::from("out")).unwrap())
    }
}
