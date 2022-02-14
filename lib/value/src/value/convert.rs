use crate::Value;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use std::collections::BTreeMap;

impl Value {
    /// Returns self as `&BTreeMap<String, Value>`, only if self is `Value::Map`.
    pub fn as_map(&self) -> Option<&BTreeMap<String, Self>> {
        match &self {
            Value::Map(map) => Some(map),
            _ => None,
        }
    }

    /// Returns self as `NotNan<f64>`, only if self is `Value::Float`.
    pub fn as_float(&self) -> Option<NotNan<f64>> {
        match self {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Returns self as `BTreeMap<String, Value>`, only if self is `Value::Map`.
    pub fn into_map(self) -> Option<BTreeMap<String, Self>> {
        match self {
            Value::Map(map) => Some(map),
            _ => None,
        }
    }

    /// Returns self as `&DateTime<Utc>`, only if self is `Value::Timestamp`.
    pub fn as_timestamp(&self) -> Option<&DateTime<Utc>> {
        match &self {
            Value::Timestamp(ts) => Some(ts),
            _ => None,
        }
    }

    /// Returns self as a mutable `BTreeMap<String, Value>`.
    ///
    /// # Panics
    ///
    /// This function will panic if self is anything other than `Value::Map`.
    pub fn as_map_mut(&mut self) -> &mut BTreeMap<String, Self> {
        match self {
            Value::Map(ref mut m) => m,
            _ => panic!("Tried to call `Value::as_map` on a non-map value."),
        }
    }

    /// Returns self as a `Vec<Value>`.
    ///
    /// # Panics
    ///
    /// This function will panic if self is anything other than `Value::Array`.
    pub fn as_array(&self) -> &Vec<Self> {
        match self {
            Value::Array(ref a) => a,
            _ => panic!("Tried to call `Value::as_array` on a non-array value."),
        }
    }

    /// Returns self as a mutable `Vec<Value>`.
    ///
    /// # Panics
    ///
    /// This function will panic if self is anything other than `Value::Array`.
    pub fn as_array_mut(&mut self) -> &mut Vec<Self> {
        match self {
            Value::Array(ref mut a) => a,
            _ => panic!("Tried to call `Value::as_array` on a non-array value."),
        }
    }
}

impl From<Bytes> for Value {
    fn from(bytes: Bytes) -> Self {
        Self::Bytes(bytes)
    }
}

impl<T: Into<Self>> From<Vec<T>> for Value {
    fn from(set: Vec<T>) -> Self {
        set.into_iter()
            .map(::std::convert::Into::into)
            .collect::<Self>()
    }
}

impl From<String> for Value {
    fn from(string: String) -> Self {
        Self::Bytes(string.into())
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::Bytes(Vec::from(s.as_bytes()).into())
    }
}

impl From<DateTime<Utc>> for Value {
    fn from(timestamp: DateTime<Utc>) -> Self {
        Self::Timestamp(timestamp)
    }
}

impl<T: Into<Self>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        match value {
            None => Self::Null,
            Some(v) => v.into(),
        }
    }
}

impl From<NotNan<f32>> for Value {
    fn from(value: NotNan<f32>) -> Self {
        Self::Float(value.into())
    }
}

impl From<NotNan<f64>> for Value {
    fn from(value: NotNan<f64>) -> Self {
        Self::Float(value)
    }
}

#[cfg(any(test, feature = "test"))]
#[allow(clippy::fallible_impl_from)] // fallibility is intentional here, it's only for tests
impl From<f64> for Value {
    fn from(f: f64) -> Self {
        NotNan::new(f).unwrap().into()
    }
}

impl From<BTreeMap<String, Self>> for Value {
    fn from(value: BTreeMap<String, Self>) -> Self {
        Self::Map(value)
    }
}

impl FromIterator<Self> for Value {
    fn from_iter<I: IntoIterator<Item = Self>>(iter: I) -> Self {
        Self::Array(iter.into_iter().collect::<Vec<Self>>())
    }
}

impl FromIterator<(String, Self)> for Value {
    fn from_iter<I: IntoIterator<Item = (String, Self)>>(iter: I) -> Self {
        Self::Map(iter.into_iter().collect::<BTreeMap<String, Self>>())
    }
}

macro_rules! impl_valuekind_from_integer {
    ($t:ty) => {
        impl From<$t> for Value {
            fn from(value: $t) -> Self {
                Value::Integer(value as i64)
            }
        }
    };
}

impl_valuekind_from_integer!(i64);
impl_valuekind_from_integer!(i32);
impl_valuekind_from_integer!(i16);
impl_valuekind_from_integer!(i8);
impl_valuekind_from_integer!(u32);
impl_valuekind_from_integer!(u16);
impl_valuekind_from_integer!(u8);
impl_valuekind_from_integer!(isize);

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}
