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
        return Self { v };
    }
}

impl TransactionValueMerger for DiscardMerger {
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

impl TransactionValueMerger for ConcatMerger {
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
struct ArrayMerger {
    v: Vec<Value>,
}

impl ArrayMerger {
    fn new(v: Vec<Value>) -> Self {
        Self { v }
    }
}

impl TransactionValueMerger for ArrayMerger {
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
        return Self {
            started: v.clone(),
            latest: v,
        };
    }
}

impl TransactionValueMerger for TimestampWindowMerger {
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
        return Self { v };
    }
}

impl TransactionValueMerger for AddNumbersMerger {
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
        return Self { v };
    }
}

impl TransactionValueMerger for MaxNumberMerger {
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
        return Self { v };
    }
}

impl TransactionValueMerger for MinNumberMerger {
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

pub trait TransactionValueMerger: std::fmt::Debug + Send + Sync {
    fn add(&mut self, v: Value) -> Result<(), String>;
    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String>;
}

impl From<Value> for Box<dyn TransactionValueMerger> {
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

pub fn get_value_merger(
    v: Value,
    m: &MergeStrategy,
) -> Result<Box<dyn TransactionValueMerger>, String> {
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
            _ => Err(format!(
                "expected string value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::Array => match v {
            Value::Array(a) => Ok(Box::new(ArrayMerger::new(a))),
            _ => Ok(Box::new(ArrayMerger::new(vec![v]))),
        },
        MergeStrategy::Discard => Ok(Box::new(DiscardMerger::new(v))),
    }
}
