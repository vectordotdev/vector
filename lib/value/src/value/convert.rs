use std::borrow::Cow;
use std::collections::BTreeMap;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use regex::Regex;
use std::sync::Arc;

use crate::value::regex::ValueRegex;
use crate::{Kind, Value};

impl Value {
    /// Returns self as `NotNan<f64>`, only if self is `Value::Float`.
    pub fn as_float(&self) -> Option<NotNan<f64>> {
        match self {
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Returns self as `BTreeMap<String, Value>`, only if self is `Value::Object`.
    pub fn into_object(self) -> Option<BTreeMap<String, Self>> {
        match self {
            Self::Object(map) => Some(map),
            _ => None,
        }
    }

    /// Returns self as `&DateTime<Utc>`, only if self is `Value::Timestamp`.
    pub fn as_timestamp(&self) -> Option<&DateTime<Utc>> {
        match &self {
            Self::Timestamp(ts) => Some(ts),
            _ => None,
        }
    }

    /// Returns self as a `DateTime<Utc>`.
    ///
    /// # Panics
    ///
    /// This function will panic if self is anything other than `Value::Timestamp`.
    pub fn as_timestamp_unwrap(&self) -> &DateTime<Utc> {
        match self {
            Self::Timestamp(ref timestamp) => timestamp,
            _ => panic!("Tried to call `Value::as_timestamp_unwrap` on a non-timestamp value."),
        }
    }

    /// Returns self as a mutable `BTreeMap<String, Value>`.
    ///
    /// # Panics
    ///
    /// This function will panic if self is anything other than `Value::Object`.
    pub fn as_object_mut_unwrap(&mut self) -> &mut BTreeMap<String, Self> {
        match self {
            Self::Object(ref mut m) => m,
            _ => panic!("Tried to call `Value::as_map` on a non-map value."),
        }
    }

    /// Returns self as a `Vec<Value>`.
    ///
    /// # Panics
    ///
    /// This function will panic if self is anything other than `Value::Array`.
    pub fn as_array_unwrap(&self) -> &[Self] {
        match self {
            Self::Array(ref a) => a,
            _ => panic!("Tried to call `Value::as_array` on a non-array value."),
        }
    }

    /// Returns self as a mutable `Vec<Value>`.
    ///
    /// # Panics
    ///
    /// This function will panic if self is anything other than `Value::Array`.
    pub fn as_array_mut_unwrap(&mut self) -> &mut Vec<Self> {
        match self {
            Self::Array(ref mut a) => a,
            _ => panic!("Tried to call `Value::as_array` on a non-array value."),
        }
    }

    /// Returns true if self is `Value::Integer`.
    pub fn is_integer(&self) -> bool {
        matches!(self, Self::Integer(_))
    }

    /// Returns self as `f64`, only if self is `Value::Integer`.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(v) => Some(*v),
            _ => None,
        }
    }

    /// Returns true if self is `Value::Float`.
    pub fn is_float(&self) -> bool {
        matches!(self, Self::Float(_))
    }

    // This replaces the more implicit "From<f64>", but keeps the same behavior.
    // Ideally https://github.com/vectordotdev/vector/issues/11177 will remove this entirely
    /// Creates a Value from an f64. If the value is Nan, it is converted to 0.0
    #[must_use]
    pub fn from_f64_or_zero(value: f64) -> Self {
        NotNan::new(value).map_or_else(
            |_| Self::Float(NotNan::new(0.0).expect("0.0 is not NaN")),
            Value::Float,
        )
    }

    /// Returns true if self is `Value::Bytes`.
    pub fn is_bytes(&self) -> bool {
        matches!(self, Self::Bytes(_))
    }

    /// Returns self as `&Bytes`, only if self is `Value::Bytes`.
    pub fn as_bytes(&self) -> Option<&Bytes> {
        match self {
            Self::Bytes(v) => Some(v),
            _ => None,
        }
    }

    /// Returns self as `Cow<str>`, only if self is `Value::Bytes`
    pub fn as_str(&self) -> Option<Cow<'_, str>> {
        self.as_bytes()
            .map(|bytes| String::from_utf8_lossy(bytes.as_ref()))
    }

    /// Converts the Value into a byte representation regardless of its original type.
    /// Object and Array are currently not supported, although technically there's no reason why it
    /// couldn't in future should the need arise.
    ///
    /// # Errors
    /// If the type is Object or Array, and string error description will be returned
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
            Self::Object(_o) => Err("cannot convert object to bytes.".to_string()),
            Self::Array(_a) => Err("cannot convert array to bytes.".to_string()),
            Self::Timestamp(t) => Ok(Bytes::copy_from_slice(&t.timestamp().to_le_bytes())),
            Self::Regex(r) => Ok(r.to_string().into()),
            Self::Null => Ok(Bytes::copy_from_slice(&[0_u8])),
        }
    }

    /// Returns true if self is `Value::Boolean`.
    pub fn is_boolean(&self) -> bool {
        matches!(self, Self::Boolean(_))
    }

    /// Returns self as `bool`, only if self is `Value::Boolean`.
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Boolean(v) => Some(*v),
            _ => None,
        }
    }

    /// Returns true if self is `Value::Regex`.
    pub fn is_regex(&self) -> bool {
        matches!(self, Self::Regex(_))
    }

    /// Returns self as `&ValueRegex`, only if self is `Value::Regex`.
    pub fn as_regex(&self) -> Option<&Regex> {
        match self {
            Self::Regex(v) => Some(v),
            _ => None,
        }
    }

    /// Returns true if self is `Value::Null`.
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Returns self as `())`, only if self is `Value::Null`.
    pub fn as_null(&self) -> Option<()> {
        match self {
            Self::Null => Some(()),
            _ => None,
        }
    }

    /// Returns true if self is `Value::Array`.
    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    /// Returns self as `&[Value]`, only if self is `Value::Array`.
    pub fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(v) => Some(v),
            _ => None,
        }
    }

    /// Returns self as `&mut Vec<Value>`, only if self is `Value::Array`.
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Self>> {
        match self {
            Self::Array(v) => Some(v),
            _ => None,
        }
    }

    /// Returns true if self is `Value::Object`.
    pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }

    /// Returns self as `&BTreeMap<String, Value>`, only if self is `Value::Object`.
    pub fn as_object(&self) -> Option<&BTreeMap<String, Self>> {
        match self {
            Self::Object(v) => Some(v),
            _ => None,
        }
    }

    /// Returns self as `&mut BTreeMap<String, Value>`, only if self is `Value::Object`.
    pub fn as_object_mut(&mut self) -> Option<&mut BTreeMap<String, Self>> {
        match self {
            Self::Object(v) => Some(v),
            _ => None,
        }
    }

    /// Returns true if self is `Value::Timestamp`.
    pub fn is_timestamp(&self) -> bool {
        matches!(self, Self::Timestamp(_))
    }

    /// Returns the `Kind` of this `Value`
    pub fn kind(&self) -> Kind {
        self.into()
    }
}

impl From<Bytes> for Value {
    fn from(bytes: Bytes) -> Self {
        Self::Bytes(bytes)
    }
}

impl<const N: usize> From<[u8; N]> for Value {
    fn from(data: [u8; N]) -> Self {
        Self::from(Bytes::copy_from_slice(&data[..]))
    }
}

impl<const N: usize> From<&[u8; N]> for Value {
    fn from(data: &[u8; N]) -> Self {
        Self::from(Bytes::copy_from_slice(data))
    }
}

impl From<&[u8]> for Value {
    fn from(data: &[u8]) -> Self {
        Self::from(Bytes::copy_from_slice(data))
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
    fn from(v: &str) -> Self {
        Self::Bytes(Bytes::copy_from_slice(v.as_bytes()))
    }
}

impl From<DateTime<Utc>> for Value {
    fn from(timestamp: DateTime<Utc>) -> Self {
        Self::Timestamp(timestamp)
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Self::Null
    }
}

impl From<Cow<'_, str>> for Value {
    fn from(v: Cow<'_, str>) -> Self {
        v.as_ref().into()
    }
}

impl<T: Into<Self>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Null, Into::into)
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
        Self::Object(value)
    }
}

impl FromIterator<Self> for Value {
    fn from_iter<I: IntoIterator<Item = Self>>(iter: I) -> Self {
        Self::Array(iter.into_iter().collect::<Vec<Self>>())
    }
}

impl FromIterator<(String, Self)> for Value {
    fn from_iter<I: IntoIterator<Item = (String, Self)>>(iter: I) -> Self {
        Self::Object(iter.into_iter().collect::<BTreeMap<String, Self>>())
    }
}

impl From<Arc<Regex>> for Value {
    fn from(r: Arc<Regex>) -> Self {
        Self::Regex(ValueRegex::new(r))
    }
}

impl From<Regex> for Value {
    fn from(r: Regex) -> Self {
        Self::Regex(ValueRegex::new(Arc::new(r)))
    }
}

impl From<ValueRegex> for Value {
    fn from(r: ValueRegex) -> Self {
        Self::Regex(r)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<i16> for Value {
    fn from(value: i16) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<i8> for Value {
    fn from(value: i8) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<u16> for Value {
    fn from(value: u16) -> Self {
        Self::Integer(value as i64)
    }
}
impl From<u8> for Value {
    fn from(value: u8) -> Self {
        Self::Integer(value as i64)
    }
}
impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Self::Integer(value as i64)
    }
}
impl From<isize> for Value {
    fn from(value: isize) -> Self {
        Self::Integer(value as i64)
    }
}
impl From<usize> for Value {
    fn from(value: usize) -> Self {
        Self::Integer(value as i64)
    }
}
impl From<u64> for Value {
    fn from(value: u64) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}
