// Value::Array ----------------------------------------------------------------

use crate::Value;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use regex::Regex;
use std::borrow::Cow;
use std::collections::BTreeMap;

impl Value {
    /// Returns the value as Vec<Value> only if the type is `Value::Array`, otherwise returns None
    pub fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(v) => Some(v),
            _ => None,
        }
    }

    /// Returns the value as Vec<Value> only if the type is `Value::Array`, otherwise returns None
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Self>> {
        match self {
            Self::Array(v) => Some(v),
            _ => None,
        }
    }

    /// Returns the value as `DateTime`<Utc> only if the type is `Value::Timestamp`, otherwise returns None
    pub fn as_timestamp(&self) -> Option<&DateTime<Utc>> {
        match &self {
            Self::Timestamp(ts) => Some(ts),
            _ => None,
        }
    }

    /// Returns the value as Bytes only if the type is `Value::Bytes`, otherwise returns None
    pub fn as_bytes(&self) -> Option<&Bytes> {
        match self {
            Self::Bytes(bytes) => Some(bytes), // cloning a Bytes is cheap
            _ => None,
        }
    }

    /// Returns the value as BTreeMap<String, Value> only if the type is `Value::Map`, otherwise returns None
    pub fn as_map(&self) -> Option<&BTreeMap<String, Self>> {
        match &self {
            Self::Map(map) => Some(map),
            _ => None,
        }
    }

    /// Returns the value as `NotNan`<f64> only if the type is `Value::Float`, otherwise returns None
    pub fn as_float(&self) -> Option<NotNan<f64>> {
        match self {
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }
    /// Checks if the Value is a `Value::Integer`
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the value as `NotNan`<f64> only if the type is `Value::Float`, otherwise returns None
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Boolean(f) => Some(*f),
            _ => None,
        }
    }

    /// Returns the value as BTreeMap<String, Value> only if the type is `Value::Map`, otherwise returns None
    pub fn into_map(self) -> Option<BTreeMap<String, Self>> {
        match self {
            Self::Map(map) => Some(map),
            _ => None,
        }
    }

    /// Returns self as a mutable `BTreeMap<String, Value>`
    pub fn as_map_mut(&mut self) -> Option<&mut BTreeMap<String, Self>> {
        match self {
            Self::Map(ref mut m) => Some(m),
            _ => None,
        }
    }

    /// Returns self as a `&mut BTreeMap<String, Value>`
    ///
    /// # Panics
    ///
    /// This function will panic if self is anything other than `Value::Map`.
    pub fn unwrap_map_mut(&mut self) -> &mut BTreeMap<String, Self> {
        self.as_map_mut()
            .expect("Tried to call `Value::unwrap_map_mut` on a non-map value.")
    }

    /// Returns self as a `&BTreeMap<String, Value>`
    ///
    /// # Panics
    ///
    /// This function will panic if self is anything other than `Value::Map`.
    pub fn unwrap_map(&self) -> &BTreeMap<String, Self> {
        self.as_map()
            .expect("Tried to call `Value::unwrap_map` on a non-map value.")
    }

    /// Returns self as a `&[Value]`
    ///
    /// # Panics
    ///
    /// This function will panic if self is anything other than `Value::Array`.
    pub fn unwrap_array(&self) -> &[Self] {
        self.as_array()
            .expect("Tried to call `Value::unwrap_array` on a non-array value.")
    }

    /// Converts the Value into a byte representation regardless of its original type.
    ///
    /// # Errors
    /// Object and Array are currently not supported, although technically there's no reason why it
    /// couldn't in future should the need arise.
    pub fn encode_as_bytes(&self) -> Result<Bytes, String> {
        match self {
            Self::Bytes(bytes) => Ok(bytes.clone()),
            Self::Integer(i) => Ok(Bytes::copy_from_slice(&i.to_le_bytes())),
            Self::Float(f) => Ok(Bytes::copy_from_slice(&f.into_inner().to_le_bytes())),
            Self::Boolean(b) => Ok(if *b {
                Bytes::copy_from_slice(&[1_u8])
            } else {
                Bytes::copy_from_slice(&[0_u8])
            }),
            Self::Map(_o) => Err("cannot convert object to bytes.".to_string()),
            Self::Array(_a) => Err("cannot convert array to bytes.".to_string()),
            Self::Timestamp(t) => Ok(Bytes::copy_from_slice(&t.timestamp().to_le_bytes())),
            Self::Regex(r) => Ok(r.to_string().into()),
            Self::Null => Ok(Bytes::copy_from_slice(&[0_u8])),
        }
    }
}

impl<T: Into<Self>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Self::Array(v.into_iter().map(Into::into).collect::<Vec<_>>())
    }
}

impl FromIterator<Self> for Value {
    fn from_iter<I: IntoIterator<Item = Self>>(iter: I) -> Self {
        Self::Array(iter.into_iter().collect::<Vec<_>>())
    }
}

impl From<NotNan<f32>> for Value {
    fn from(value: NotNan<f32>) -> Self {
        Self::Float(NotNan::<f64>::from(value))
    }
}

impl From<NotNan<f64>> for Value {
    fn from(value: NotNan<f64>) -> Self {
        Self::Float(value)
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

impl From<Bytes> for Value {
    fn from(bytes: Bytes) -> Self {
        Self::Bytes(bytes)
    }
}

impl From<String> for Value {
    fn from(string: String) -> Self {
        Self::Bytes(string.into())
    }
}

impl From<BTreeMap<String, Self>> for Value {
    fn from(value: BTreeMap<String, Self>) -> Self {
        Self::Map(value)
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Self::Null
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

// TODO: this was copied from the VRL "Value".
// TODO: this exists to satisfy the `vector_common::Convert` utility.
//
// We'll have to fix that so that we can remove this impl.
#[allow(clippy::fallible_impl_from)]
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        let v = if v.is_nan() { 0.0 } else { v };

        Self::Float(NotNan::new(v).unwrap())
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
impl_valuekind_from_integer!(usize);

impl From<Regex> for Value {
    fn from(regex: Regex) -> Self {
        Self::Regex(regex.into())
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl FromIterator<(String, Self)> for Value {
    fn from_iter<I: IntoIterator<Item = (String, Self)>>(iter: I) -> Self {
        Self::Map(iter.into_iter().collect::<BTreeMap<String, Self>>())
    }
}

impl From<&[u8]> for Value {
    fn from(v: &[u8]) -> Self {
        Self::Bytes(Bytes::copy_from_slice(v))
    }
}

impl From<Cow<'_, str>> for Value {
    fn from(v: Cow<'_, str>) -> Self {
        v.as_ref().into()
    }
}

// impl TryFrom<f64> for Value {
//     type Error = ();
//
//     fn try_from(value: f64) -> Result<Self, Self::Error> {
//         Ok(Value::Float(NotNan::new(value).map_err(|_| ())?))
//     }
// }
