use std::{borrow::Cow, collections::BTreeMap, convert::TryFrom, iter::FromIterator};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use ordered_float::NotNan;

use super::{Error, Kind, Regex, Value};
use crate::{
    expression::{container, Container, Expr, Literal},
    Expression,
};

impl Value {
    /// Convert a given [`Value`] into a [`Expression`] trait object.
    pub fn into_expression(self) -> Box<dyn Expression> {
        Box::new(self.into_expr())
    }

    /// Convert a given [`Value`] into an [`Expr`] enum variant.
    ///
    /// This is a non-public function because we want to avoid exposing internal
    /// details about the expression variants.
    pub(crate) fn into_expr(self) -> Expr {
        use Value::*;

        match self {
            Bytes(v) => Literal::from(v).into(),
            Integer(v) => Literal::from(v).into(),
            Float(v) => Literal::from(v).into(),
            Boolean(v) => Literal::from(v).into(),
            Object(v) => {
                let object = crate::expression::Object::from(
                    v.into_iter()
                        .map(|(k, v)| (k, v.into_expr()))
                        .collect::<BTreeMap<_, _>>(),
                );

                Container::new(container::Variant::from(object)).into()
            }
            Array(v) => {
                let array = crate::expression::Array::from(
                    v.into_iter().map(|v| v.into_expr()).collect::<Vec<_>>(),
                );

                Container::new(container::Variant::from(array)).into()
            }
            Timestamp(v) => Literal::from(v).into(),
            Regex(v) => Literal::from(v).into(),
            Null => Literal::from(()).into(),
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

    pub fn try_integer(self) -> Result<i64, Error> {
        match self {
            Value::Integer(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::Integer,
            }),
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

impl TryFrom<&Value> for i64 {
    type Error = Error;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v {
            Value::Integer(v) => Ok(*v),
            Value::Float(v) => Ok(v.into_inner() as i64),
            _ => Err(Error::Coerce(v.kind(), Kind::Integer)),
        }
    }
}

// Value::Float ----------------------------------------------------------------

impl Value {
    pub fn is_float(&self) -> bool {
        matches!(self, Value::Float(_))
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(v) => Some(v.into_inner()),
            _ => None,
        }
    }

    pub fn try_float(self) -> Result<f64, Error> {
        match self {
            Value::Float(v) => Ok(v.into_inner()),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::Float,
            }),
        }
    }
}

impl From<NotNan<f64>> for Value {
    fn from(v: NotNan<f64>) -> Self {
        Value::Float(v)
    }
}

impl TryFrom<&Value> for f64 {
    type Error = Error;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v {
            Value::Integer(v) => Ok(*v as f64),
            Value::Float(v) => Ok(v.into_inner()),
            _ => Err(Error::Coerce(v.kind(), Kind::Float)),
        }
    }
}

// TODO: this exists to satisfy the `vector_common::Convert` utility.
//
// We'll have to fix that so that we can remove this impl.
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        let v = if v.is_nan() { 0.0 } else { v };

        Value::Float(NotNan::new(v).unwrap())
    }
}

// impl TryFrom<f64> for Value {
//     type Error = Error;

//     fn try_from(v: f64) -> Result<Self, Self::Error> {
//         Ok(Value::Float(NotNan::new(v).map_err(|_| Error::NanFloat)?))
//     }
// }

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

    pub fn try_bytes(self) -> Result<Bytes, Error> {
        match self {
            Value::Bytes(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::Bytes,
            }),
        }
    }

    pub fn try_bytes_utf8_lossy(&self) -> Result<Cow<'_, str>, Error> {
        match self.as_bytes() {
            Some(bytes) => Ok(String::from_utf8_lossy(bytes)),
            None => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::Bytes,
            }),
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

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        v.as_slice().into()
    }
}

impl From<&[u8]> for Value {
    fn from(v: &[u8]) -> Self {
        Value::Bytes(Bytes::copy_from_slice(v))
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

    pub fn try_boolean(self) -> Result<bool, Error> {
        match self {
            Value::Boolean(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::Boolean,
            }),
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

    pub fn try_regex(self) -> Result<Regex, Error> {
        match self {
            Value::Regex(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::Regex,
            }),
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

    pub fn try_null(self) -> Result<(), Error> {
        match self {
            Value::Null => Ok(()),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::Null,
            }),
        }
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Value::Null
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

    pub fn try_array(self) -> Result<Vec<Value>, Error> {
        match self {
            Value::Array(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::Array,
            }),
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

    pub fn try_object(self) -> Result<BTreeMap<String, Value>, Error> {
        match self {
            Value::Object(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::Object,
            }),
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

    pub fn try_timestamp(self) -> Result<DateTime<Utc>, Error> {
        match self {
            Value::Timestamp(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::Timestamp,
            }),
        }
    }
}

impl From<DateTime<Utc>> for Value {
    fn from(v: DateTime<Utc>) -> Self {
        Value::Timestamp(v)
    }
}
