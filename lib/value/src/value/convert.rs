// Value::Array ----------------------------------------------------------------

use crate::value::timestamp_to_string;
use crate::Value;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use regex::Regex;
use std::borrow::Cow;
use std::collections::BTreeMap;

use toml::Value as TomlValue;

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

impl Value {
    /// Returns the value as Vec<Value> only if the type is Value::Array, otherwise returns None
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(v) => Some(v),
            _ => None,
        }
    }

    /// Returns the value as Vec<Value> only if the type is Value::Array, otherwise returns None
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(v) => Some(v),
            _ => None,
        }
    }

    /// Returns the value as DateTime<Utc> only if the type is Value::Timestamp, otherwise returns None
    pub fn as_timestamp(&self) -> Option<&DateTime<Utc>> {
        match &self {
            Value::Timestamp(ts) => Some(ts),
            _ => None,
        }
    }

    /// Returns the value as Bytes only if the type is Value::Bytes, otherwise returns None
    pub fn as_bytes(&self) -> Option<&Bytes> {
        match self {
            Value::Bytes(bytes) => Some(bytes), // cloning a Bytes is cheap
            _ => None,
        }
    }

    /// Returns the value as BTreeMap<String, Value> only if the type is Value::Map, otherwise returns None
    pub fn as_map(&self) -> Option<&BTreeMap<String, Value>> {
        match &self {
            Value::Map(map) => Some(map),
            _ => None,
        }
    }

    /// Returns the value as NotNan<f64> only if the type is Value::Float, otherwise returns None
    pub fn as_float(&self) -> Option<NotNan<f64>> {
        match self {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Returns the value as Bytes only if the type is Value::Bytes, otherwise returns None
    pub fn into_bytes(self) -> Bytes {
        self.convert_to_bytes()
    }

    /// Returns the value as BTreeMap<String, Value> only if the type is Value::Map, otherwise returns None
    pub fn into_map(self) -> Option<BTreeMap<String, Value>> {
        match self {
            Value::Map(map) => Some(map),
            _ => None,
        }
    }

    /// Checks if the Value is a Value::Integer
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns self as a mutable `BTreeMap<String, Value>`
    pub fn as_map_mut(&mut self) -> Option<&mut BTreeMap<String, Value>> {
        match self {
            Value::Map(ref mut m) => Some(m),
            _ => None,
        }
    }

    /// Returns self as a mutable `BTreeMap<String, Value>`
    ///
    /// # Panics
    ///
    /// This function will panic if self is anything other than `Value::Map`.
    pub fn unwrap_map_mut(&mut self) -> &mut BTreeMap<String, Value> {
        self.as_map_mut()
            .expect("Tried to call `Value::unwrap_map_mut` on a non-map value.")
    }

    /// Returns self as a mutable `BTreeMap<String, Value>`
    ///
    /// # Panics
    ///
    /// This function will panic if self is anything other than `Value::Map`.
    pub fn unwrap_map(&self) -> &BTreeMap<String, Value> {
        self.as_map()
            .expect("Tried to call `Value::unwrap_map` on a non-map value.")
    }

    /// Converts the Value into Bytes, using it's String representation
    pub fn convert_to_bytes(&self) -> Bytes {
        match self {
            Value::Bytes(bytes) => bytes.clone(), // cloning a Bytes is cheap
            Value::Regex(regex) => regex_to_bytes(regex),
            Value::Timestamp(timestamp) => Bytes::from(timestamp_to_string(timestamp)),
            Value::Integer(num) => Bytes::from(format!("{}", num)),
            Value::Float(num) => Bytes::from(format!("{}", num)),
            Value::Boolean(b) => Bytes::from(format!("{}", b)),
            Value::Map(map) => Bytes::from(serde_json::to_vec(map).expect("Cannot serialize map")),
            Value::Array(arr) => {
                Bytes::from(serde_json::to_vec(arr).expect("Cannot serialize array"))
            }
            Value::Null => Bytes::from("<null>"),
        }
    }

    /// Converts the Value into a byte representation regardless of its original type.
    /// Object and Array are currently not supported, although technically there's no reason why it
    /// couldn't in future should the need arise.
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
            Value::Map(_o) => Err("cannot convert object to bytes.".to_string()),
            Value::Array(_a) => Err("cannot convert array to bytes.".to_string()),
            Value::Timestamp(t) => Ok(Bytes::copy_from_slice(&t.timestamp().to_le_bytes())),
            Value::Regex(r) => Ok(r.to_string().into()),
            Value::Null => Ok(Bytes::copy_from_slice(&[0_u8])),
        }
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::Array(v.into_iter().map(Into::into).collect::<Vec<_>>())
    }
}

impl FromIterator<Value> for Value {
    fn from_iter<I: IntoIterator<Item = Value>>(iter: I) -> Self {
        Value::Array(iter.into_iter().collect::<Vec<_>>())
    }
}

impl From<NotNan<f32>> for Value {
    fn from(value: NotNan<f32>) -> Self {
        Value::Float(NotNan::<f64>::from(value))
    }
}

impl From<NotNan<f64>> for Value {
    fn from(value: NotNan<f64>) -> Self {
        Value::Float(value)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Bytes(Vec::from(s.as_bytes()).into())
    }
}

impl From<DateTime<Utc>> for Value {
    fn from(timestamp: DateTime<Utc>) -> Self {
        Value::Timestamp(timestamp)
    }
}

impl From<Bytes> for Value {
    fn from(bytes: Bytes) -> Self {
        Value::Bytes(bytes)
    }
}

impl From<String> for Value {
    fn from(string: String) -> Self {
        Value::Bytes(string.into())
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(value: BTreeMap<String, Value>) -> Self {
        Value::Map(value)
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Value::Null
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        match value {
            None => Value::Null,
            Some(v) => v.into(),
        }
    }
}

// TODO: this was copied from the VRL "Value".
// TODO: this exists to satisfy the `vector_common::Convert` utility.
//
// We'll have to fix that so that we can remove this impl.
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        let v = if v.is_nan() { 0.0 } else { v };

        Value::Float(NotNan::new(v).unwrap())
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
        Value::Boolean(value)
    }
}

impl FromIterator<(String, Value)> for Value {
    fn from_iter<I: IntoIterator<Item = (String, Value)>>(iter: I) -> Self {
        Value::Map(iter.into_iter().collect::<BTreeMap<String, Value>>())
    }
}

impl From<&[u8]> for Value {
    fn from(v: &[u8]) -> Self {
        Value::Bytes(Bytes::copy_from_slice(v))
    }
}

impl From<Cow<'_, str>> for Value {
    fn from(v: Cow<'_, str>) -> Self {
        v.as_ref().into()
    }
}

//TODO: use toml feature?
impl TryFrom<TomlValue> for Value {
    type Error = Error;

    fn try_from(toml: TomlValue) -> Result<Self, Error> {
        Ok(match toml {
            TomlValue::String(s) => Self::from(s),
            TomlValue::Integer(i) => Self::from(i),
            TomlValue::Array(a) => Self::from(
                a.into_iter()
                    .map(Value::try_from)
                    .collect::<Result<Vec<_>, Error>>()?,
            ),
            TomlValue::Table(t) => Self::from(
                t.into_iter()
                    .map(|(k, v)| Value::try_from(v).map(|v| (k, v)))
                    .collect::<Result<BTreeMap<_, _>, Error>>()?,
            ),
            TomlValue::Datetime(dt) => Self::from(dt.to_string().parse::<DateTime<Utc>>()?),
            TomlValue::Boolean(b) => Self::from(b),
            TomlValue::Float(f) => Self::from(f),
        })
    }
}

impl From<serde_json::Value> for Value {
    fn from(json_value: serde_json::Value) -> Self {
        match json_value {
            serde_json::Value::Bool(b) => Value::Boolean(b),
            serde_json::Value::Number(n) => {
                let float_or_byte = || {
                    n.as_f64()
                        // JSON doesn't support "NaN"
                        .map(|f| Value::Float(NotNan::new(f).unwrap()))
                        .unwrap_or_else(|| Value::Bytes(n.to_string().into()))
                };
                n.as_i64().map_or_else(float_or_byte, Value::Integer)
            }
            serde_json::Value::String(s) => Value::Bytes(Bytes::from(s)),
            serde_json::Value::Object(obj) => Value::Map(
                obj.into_iter()
                    .map(|(key, value)| (key, Value::from(value)))
                    .collect(),
            ),
            serde_json::Value::Array(arr) => {
                Value::Array(arr.into_iter().map(Value::from).collect())
            }
            serde_json::Value::Null => Value::Null,
        }
    }
}

// impl TryFrom<f64> for Value {
//     type Error = ();
//
//     fn try_from(value: f64) -> Result<Self, Self::Error> {
//         Ok(Value::Float(NotNan::new(value).map_err(|_| ())?))
//     }
// }

impl TryFrom<Value> for serde_json::Value {
    type Error = Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Boolean(v) => Ok(serde_json::Value::from(v)),
            Value::Integer(v) => Ok(serde_json::Value::from(v)),
            Value::Float(v) => Ok(serde_json::Value::from(v.into_inner())),
            Value::Bytes(v) => Ok(serde_json::Value::from(String::from_utf8(v.to_vec())?)),
            Value::Regex(regex) => Ok(serde_json::Value::from(regex.to_string())),
            Value::Map(v) => Ok(serde_json::to_value(v)?),
            Value::Array(v) => Ok(serde_json::to_value(v)?),
            Value::Null => Ok(serde_json::Value::Null),
            Value::Timestamp(v) => Ok(serde_json::Value::from(timestamp_to_string(&v))),
        }
    }
}

pub fn regex_to_bytes(regex: &Regex) -> Bytes {
    Bytes::copy_from_slice(regex.to_string().as_bytes())
}
