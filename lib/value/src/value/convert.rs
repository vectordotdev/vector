use crate::value::regex::ValueRegex;
use crate::{Kind, Value};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use regex::Regex;
use std::borrow::Cow;
use std::collections::BTreeMap;

impl Value {
    /// Returns self as `&BTreeMap<String, Value>`, only if self is `Value::Map`.
    pub fn as_map(&self) -> Option<&BTreeMap<String, Self>> {
        match &self {
            Value::Object(map) => Some(map),
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
            Value::Object(map) => Some(map),
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
            Value::Object(ref mut m) => m,
            _ => panic!("Tried to call `Value::as_map` on a non-map value."),
        }
    }

    /// Returns self as a `Vec<Value>`.
    ///
    /// # Panics
    ///
    /// This function will panic if self is anything other than `Value::Array`.
    pub fn as_array_unwrap(&self) -> &Vec<Self> {
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
    pub fn as_array_mut_unwrap(&mut self) -> &mut Vec<Self> {
        match self {
            Value::Array(ref mut a) => a,
            _ => panic!("Tried to call `Value::as_array` on a non-array value."),
        }
    }

    /// Returns true if self is `Value::Integer`.
    pub fn is_integer(&self) -> bool {
        matches!(self, Value::Integer(_))
    }

    /// Returns self as `f64`, only if self is `Value::Integer`.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Value::Integer(v) => Some(*v),
            _ => None,
        }
    }

    /// Returns true if self is `Value::Float`.
    pub fn is_float(&self) -> bool {
        matches!(self, Value::Float(_))
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
        matches!(self, Value::Bytes(_))
    }

    /// Returns self as `&Bytes`, only if self is `Value::Bytes`.
    pub fn as_bytes(&self) -> Option<&Bytes> {
        match self {
            Value::Bytes(v) => Some(v),
            _ => None,
        }
    }

    /// Converts the Value into a byte representation regardless of its original type.
    /// Object and Array are currently not supported, although technically there's no reason why it
    /// couldn't in future should the need arise.
    ///
    /// # Errors
    /// If the type is Object or Array, and string error description will be returned
    pub fn encode_as_bytes(&self) -> Result<Bytes, String> {
        match self {
            Value::Bytes(bytes) => Ok(bytes.clone()),
            Value::Integer(i) => Ok(Bytes::copy_from_slice(&i.to_le_bytes())),
            Value::Float(f) => Ok(Bytes::copy_from_slice(&f.into_inner().to_le_bytes())),
            Value::Boolean(b) => Ok(if *b {
                Bytes::copy_from_slice(&[1_u8])
            } else {
                Bytes::copy_from_slice(&[0_u8])
            }),
            Value::Object(_o) => Err("cannot convert object to bytes.".to_string()),
            Value::Array(_a) => Err("cannot convert array to bytes.".to_string()),
            Value::Timestamp(t) => Ok(Bytes::copy_from_slice(&t.timestamp().to_le_bytes())),
            Value::Regex(r) => Ok(r.to_string().into()),
            Value::Null => Ok(Bytes::copy_from_slice(&[0_u8])),
        }
    }

    /// Returns true if self is `Value::Boolean`.
    pub fn is_boolean(&self) -> bool {
        matches!(self, Value::Boolean(_))
    }

    /// Returns self as `bool`, only if self is `Value::Boolean`.
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            Value::Boolean(v) => Some(*v),
            _ => None,
        }
    }

    /// Returns true if self is `Value::Regex`.
    pub fn is_regex(&self) -> bool {
        matches!(self, Value::Regex(_))
    }

    /// Returns self as `&ValueRegex`, only if self is `Value::Regex`.
    pub fn as_regex(&self) -> Option<&Regex> {
        match self {
            Value::Regex(v) => Some(v),
            _ => None,
        }
    }

    /// Returns true if self is `Value::Null`.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Returns self as `())`, only if self is `Value::Null`.
    pub fn as_null(&self) -> Option<()> {
        match self {
            Value::Null => Some(()),
            _ => None,
        }
    }

    /// Returns true if self is `Value::Array`.
    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    /// Returns self as `&[Value]`, only if self is `Value::Array`.
    pub fn as_array(&self) -> Option<&[Self]> {
        match self {
            Value::Array(v) => Some(v),
            _ => None,
        }
    }

    /// Returns self as `&mut Vec<Value>`, only if self is `Value::Array`.
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Self>> {
        match self {
            Value::Array(v) => Some(v),
            _ => None,
        }
    }

    /// Returns true if self is `Value::Object`.
    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    /// Returns self as `&BTreeMap<String, Value>`, only if self is `Value::Object`.
    pub fn as_object(&self) -> Option<&BTreeMap<String, Self>> {
        match self {
            Value::Object(v) => Some(v),
            _ => None,
        }
    }

    /// Returns self as `&mut BTreeMap<String, Value>`, only if self is `Value::Object`.
    pub fn as_object_mut(&mut self) -> Option<&mut BTreeMap<String, Self>> {
        match self {
            Value::Object(v) => Some(v),
            _ => None,
        }
    }

    /// Returns true if self is `Value::Timestamp`.
    pub fn is_timestamp(&self) -> bool {
        matches!(self, Value::Timestamp(_))
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

impl From<Regex> for Value {
    fn from(r: Regex) -> Self {
        Self::Regex(ValueRegex::new(r))
    }
}

impl From<ValueRegex> for Value {
    fn from(r: ValueRegex) -> Self {
        Self::Regex(r)
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
impl_valuekind_from_integer!(u64);

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}
