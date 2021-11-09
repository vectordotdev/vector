use crate::{Expression, SharedValue, Value};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use std::collections::BTreeMap;
use std::iter::FromIterator;
use std::{borrow::Cow, cell::RefCell, rc::Rc};

impl SharedValue {
    /// Convert a given [`Value`] into a [`Expression`] trait object.
    pub fn into_expression(self) -> Box<dyn Expression> {
        Box::new(self.0.borrow().clone().into_expr())
    }
}

impl From<Value> for SharedValue {
    fn from(value: Value) -> Self {
        SharedValue(Rc::new(RefCell::new(value)))
    }
}

impl From<i8> for SharedValue {
    fn from(v: i8) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<i16> for SharedValue {
    fn from(v: i16) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<i32> for SharedValue {
    fn from(v: i32) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<i64> for SharedValue {
    fn from(v: i64) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<u16> for SharedValue {
    fn from(v: u16) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<u32> for SharedValue {
    fn from(v: u32) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<u64> for SharedValue {
    fn from(v: u64) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<usize> for SharedValue {
    fn from(v: usize) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<NotNan<f64>> for SharedValue {
    fn from(v: NotNan<f64>) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<f64> for SharedValue {
    fn from(v: f64) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<bool> for SharedValue {
    fn from(v: bool) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<Bytes> for SharedValue {
    fn from(v: Bytes) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<Cow<'_, str>> for SharedValue {
    fn from(v: Cow<'_, str>) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<Vec<u8>> for SharedValue {
    fn from(v: Vec<u8>) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<&[u8]> for SharedValue {
    fn from(v: &[u8]) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<String> for SharedValue {
    fn from(v: String) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<&str> for SharedValue {
    fn from(v: &str) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<regex::Regex> for SharedValue {
    fn from(regex: regex::Regex) -> Self {
        SharedValue::from(Value::from(regex))
    }
}

impl From<DateTime<Utc>> for SharedValue {
    fn from(v: DateTime<Utc>) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl<T: Into<Value>> From<Vec<T>> for SharedValue {
    fn from(v: Vec<T>) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<Vec<SharedValue>> for SharedValue {
    fn from(v: Vec<SharedValue>) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl From<BTreeMap<String, SharedValue>> for SharedValue {
    fn from(v: BTreeMap<String, SharedValue>) -> Self {
        SharedValue::from(Value::from(v))
    }
}

impl FromIterator<SharedValue> for SharedValue {
    fn from_iter<I: IntoIterator<Item = SharedValue>>(iter: I) -> Self {
        SharedValue::from(Value::Array(iter.into_iter().collect::<Vec<_>>()))
    }
}
