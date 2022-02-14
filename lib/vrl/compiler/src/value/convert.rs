use std::{borrow::Cow, collections::BTreeMap, iter::FromIterator};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use value::kind::Collection;
use value::Value as VectorValue;

use super::{Error, Kind, Regex, Value};
use crate::value::VrlValueKind;
use crate::{expression::Expr, Expression};

pub trait VrlValueConvert: Sized {
    /// Convert a given [`Value`] into a [`Expression`] trait object.
    fn into_expression(self) -> Box<dyn Expression>;

    fn try_integer(self) -> Result<i64, Error>;
    fn try_float(self) -> Result<f64, Error>;
    fn try_bytes(self) -> Result<Bytes, Error>;
    fn try_boolean(self) -> Result<bool, Error>;
    fn try_regex(self) -> Result<Regex, Error>;
    fn try_null(self) -> Result<(), Error>;
    fn try_array(self) -> Result<Vec<Value>, Error>;
    fn try_object(self) -> Result<BTreeMap<String, Value>, Error>;
    fn try_timestamp(self) -> Result<DateTime<Utc>, Error>;

    fn try_into_i64(&self) -> Result<i64, Error>;
    fn try_into_f64(&self) -> Result<f64, Error>;

    fn try_bytes_utf8_lossy(&self) -> Result<Cow<'_, str>, Error>;
}

impl VrlValueConvert for Value {
    /// Convert a given [`Value`] into a [`Expression`] trait object.
    fn into_expression(self) -> Box<dyn Expression> {
        Box::new(Expr::from(self))
    }

    fn try_integer(self) -> Result<i64, Error> {
        match self {
            Value::Integer(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::integer(),
            }),
        }
    }

    fn try_into_i64(self: &Value) -> Result<i64, Error> {
        match self {
            Value::Integer(v) => Ok(*v),
            Value::Float(v) => Ok(v.into_inner() as i64),
            _ => Err(Error::Coerce(self.kind(), Kind::integer())),
        }
    }

    fn try_float(self) -> Result<f64, Error> {
        match self {
            Value::Float(v) => Ok(v.into_inner()),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::float(),
            }),
        }
    }

    fn try_into_f64(&self) -> Result<f64, Error> {
        match self {
            Value::Integer(v) => Ok(*v as f64),
            Value::Float(v) => Ok(v.into_inner()),
            _ => Err(Error::Coerce(self.kind(), Kind::float())),
        }
    }

    fn try_bytes(self) -> Result<Bytes, Error> {
        match self {
            Value::Bytes(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::bytes(),
            }),
        }
    }

    fn try_bytes_utf8_lossy(&self) -> Result<Cow<'_, str>, Error> {
        match self.as_bytes() {
            Some(bytes) => Ok(String::from_utf8_lossy(bytes)),
            None => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::bytes(),
            }),
        }
    }

    fn try_boolean(self) -> Result<bool, Error> {
        match self {
            Value::Boolean(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::boolean(),
            }),
        }
    }

    fn try_regex(self) -> Result<Regex, Error> {
        match self {
            Value::Regex(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::regex(),
            }),
        }
    }

    fn try_null(self) -> Result<(), Error> {
        match self {
            Value::Null => Ok(()),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::null(),
            }),
        }
    }

    fn try_array(self) -> Result<Vec<Value>, Error> {
        match self {
            Value::Array(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::array(Collection::any()),
            }),
        }
    }

    fn try_object(self) -> Result<BTreeMap<String, Value>, Error> {
        match self {
            Value::Object(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::object(Collection::any()),
            }),
        }
    }

    fn try_timestamp(self) -> Result<DateTime<Utc>, Error> {
        match self {
            Value::Timestamp(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::timestamp(),
            }),
        }
    }
}

// Value::Integer --------------------------------------------------------------

impl Value {
    pub fn is_integer(&self) -> bool {
        matches!(self, Value::Integer(_))
    }

    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Value::Integer(v) => Some(*v),
            _ => None,
        }
    }
}

impl From<i8> for Value {
    fn from(v: i8) -> Self {
        Value::Integer(v as i64)
    }
}

impl From<i16> for Value {
    fn from(v: i16) -> Self {
        Value::Integer(v as i64)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Integer(v as i64)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Integer(v)
    }
}

impl From<u16> for Value {
    fn from(v: u16) -> Self {
        Value::Integer(v as i64)
    }
}

impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::Integer(v as i64)
    }
}

impl From<u64> for Value {
    fn from(v: u64) -> Self {
        Value::Integer(v as i64)
    }
}

impl From<usize> for Value {
    fn from(v: usize) -> Self {
        Value::Integer(v as i64)
    }
}

// Value::Float ----------------------------------------------------------------

impl Value {
    pub fn is_float(&self) -> bool {
        matches!(self, Value::Float(_))
    }

    // This replaces the more implicit "From<f64>", but keeps the same behavior.
    // Ideally https://github.com/vectordotdev/vector/issues/11177 will remove this entirely
    pub fn from_f64_or_zero(value: f64) -> Value {
        NotNan::new(value)
            .map(Value::Float)
            .unwrap_or_else(|_| Value::Float(NotNan::new(0.0).unwrap()))
    }
}

#[cfg(any(test, feature = "test"))]
impl From<f64> for Value {
    fn from(f: f64) -> Self {
        NotNan::new(f).unwrap().into()
    }
}

impl From<NotNan<f64>> for Value {
    fn from(v: NotNan<f64>) -> Self {
        Value::Float(v)
    }
}

// Value::Bytes ----------------------------------------------------------------

impl Value {
    pub fn is_bytes(&self) -> bool {
        matches!(self, Value::Bytes(_))
    }

    pub fn as_bytes(&self) -> Option<&Bytes> {
        match self {
            Value::Bytes(v) => Some(v),
            _ => None,
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
            Value::Object(_o) => Err("cannot convert object to bytes.".to_string()),
            Value::Array(_a) => Err("cannot convert array to bytes.".to_string()),
            Value::Timestamp(t) => Ok(Bytes::copy_from_slice(&t.timestamp().to_le_bytes())),
            Value::Regex(r) => Ok(r.to_string().into()),
            Value::Null => Ok(Bytes::copy_from_slice(&[0_u8])),
        }
    }
}

impl From<Bytes> for Value {
    fn from(v: Bytes) -> Self {
        Value::Bytes(v)
    }
}

impl From<Cow<'_, str>> for Value {
    fn from(v: Cow<'_, str>) -> Self {
        v.as_ref().into()
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Self::Null
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Bytes(v.into())
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Bytes(Bytes::copy_from_slice(v.as_bytes()))
    }
}

// Value::Boolean --------------------------------------------------------------

impl Value {
    pub fn is_boolean(&self) -> bool {
        matches!(self, Value::Boolean(_))
    }

    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            Value::Boolean(v) => Some(*v),
            _ => None,
        }
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Boolean(v)
    }
}

// Value::Regex ----------------------------------------------------------------

impl Value {
    pub fn is_regex(&self) -> bool {
        matches!(self, Value::Regex(_))
    }

    pub fn as_regex(&self) -> Option<&Regex> {
        match self {
            Value::Regex(v) => Some(v),
            _ => None,
        }
    }
}

impl From<Regex> for Value {
    fn from(v: Regex) -> Self {
        Value::Regex(v)
    }
}

impl From<regex::Regex> for Value {
    fn from(regex: regex::Regex) -> Self {
        Value::Regex(regex.into())
    }
}

// Value::Null -----------------------------------------------------------------

impl Value {
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_null(&self) -> Option<()> {
        match self {
            Value::Null => Some(()),
            _ => None,
        }
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(v) => v.into(),
            None => Value::Null,
        }
    }
}

// Value::Array ----------------------------------------------------------------

impl Value {
    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(v) => Some(v),
            _ => None,
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

// Value::Object ---------------------------------------------------------------

impl Value {
    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Object(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut BTreeMap<String, Value>> {
        match self {
            Value::Object(v) => Some(v),
            _ => None,
        }
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(value: BTreeMap<String, Value>) -> Self {
        Value::Object(value)
    }
}

impl FromIterator<(String, Value)> for Value {
    fn from_iter<I: IntoIterator<Item = (String, Value)>>(iter: I) -> Self {
        Value::Object(iter.into_iter().collect::<BTreeMap<_, _>>())
    }
}

// Value::Timestamp ------------------------------------------------------------

impl Value {
    pub fn is_timestamp(&self) -> bool {
        matches!(self, Value::Timestamp(_))
    }

    pub fn as_timestamp(&self) -> Option<&DateTime<Utc>> {
        match self {
            Value::Timestamp(v) => Some(v),
            _ => None,
        }
    }
}

impl From<DateTime<Utc>> for Value {
    fn from(v: DateTime<Utc>) -> Self {
        Value::Timestamp(v)
    }
}

impl From<Value> for VectorValue {
    fn from(v: Value) -> Self {
        match v {
            Value::Bytes(v) => VectorValue::Bytes(v),
            Value::Integer(v) => VectorValue::Integer(v),
            Value::Float(v) => VectorValue::Float(v),
            Value::Boolean(v) => VectorValue::Boolean(v),
            Value::Object(v) => {
                VectorValue::Object(v.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
            Value::Array(v) => VectorValue::Array(v.into_iter().map(Into::into).collect()),
            Value::Timestamp(v) => VectorValue::Timestamp(v),
            Value::Regex(v) => {
                VectorValue::Bytes(bytes::Bytes::copy_from_slice(v.to_string().as_bytes()))
            }
            Value::Null => VectorValue::Null,
        }
    }
}

impl From<VectorValue> for Value {
    fn from(v: VectorValue) -> Self {
        match v {
            VectorValue::Bytes(v) => v.into(),
            VectorValue::Regex(regex) => regex.into_inner().into(),
            VectorValue::Integer(v) => v.into(),
            VectorValue::Float(v) => v.into(),
            VectorValue::Boolean(v) => v.into(),
            VectorValue::Object(v) => {
                Value::Object(v.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
            VectorValue::Array(v) => Value::Array(v.into_iter().map(Into::into).collect()),
            VectorValue::Timestamp(v) => v.into(),
            VectorValue::Null => Value::Null,
        }
    }
}
