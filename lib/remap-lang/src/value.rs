use bytes::Bytes;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::ops::Deref;
use std::string::String as StdString;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(Bytes),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Map(BTreeMap<String, Value>),
    Array(Vec<Value>),
    Timestamp(DateTime<Utc>),
    Null,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy, Ord, PartialOrd)]
pub enum ValueKind {
    String,
    Integer,
    Float,
    Boolean,
    Map,
    Array,
    Timestamp,
    Null,
}

impl fmt::Display for ValueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self)
    }
}

impl ValueKind {
    pub fn all() -> Vec<Self> {
        use ValueKind::*;

        vec![String, Integer, Float, Boolean, Map, Array, Timestamp, Null]
    }

    pub fn as_str(&self) -> &'static str {
        use ValueKind::*;

        match self {
            String => "string",
            Integer => "integer",
            Float => "float",
            Boolean => "boolean",
            Map => "map",
            Array => "array",
            Timestamp => "timestamp",
            Null => "null",
        }
    }
}

impl Deref for ValueKind {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl From<&Value> for ValueKind {
    fn from(value: &Value) -> Self {
        use ValueKind::*;

        match value {
            Value::String(_) => String,
            Value::Integer(_) => Integer,
            Value::Float(_) => Float,
            Value::Boolean(_) => Boolean,
            Value::Map(_) => Map,
            Value::Array(_) => Array,
            Value::Timestamp(_) => Timestamp,
            Value::Null => Null,
        }
    }
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error(r#"expected "{0}", got "{1}""#)]
    Expected(ValueKind, ValueKind),

    #[error(r#"unable to coerce "{0}" into "{1}""#)]
    Coerce(ValueKind, ValueKind),

    #[error("unable to calculate remainder of values type {0} and {1}")]
    Rem(ValueKind, ValueKind),

    #[error("unable to multiply value type {0} by {1}")]
    Mul(ValueKind, ValueKind),

    #[error("unable to divide value type {0} by {1}")]
    Div(ValueKind, ValueKind),

    #[error("unable to add value type {1} to {0}")]
    Add(ValueKind, ValueKind),

    #[error("unable to subtract value type {1} from {0}")]
    Sub(ValueKind, ValueKind),

    #[error("unable to OR value type {0} with {1}")]
    Or(ValueKind, ValueKind),

    #[error("unable to AND value type {0} with {1}")]
    And(ValueKind, ValueKind),

    #[error("unable to compare {0} > {1}")]
    Gt(ValueKind, ValueKind),

    #[error("unable to compare {0} >= {1}")]
    Ge(ValueKind, ValueKind),

    #[error("unable to compare {0} < {1}")]
    Lt(ValueKind, ValueKind),

    #[error("unable to compare {0} <= {1}")]
    Le(ValueKind, ValueKind),
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

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<Bytes> for Value {
    fn from(v: Bytes) -> Self {
        Value::String(v)
    }
}

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        v.as_slice().into()
    }
}

impl From<&[u8]> for Value {
    fn from(v: &[u8]) -> Self {
        Value::String(Bytes::copy_from_slice(v))
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v.into())
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Boolean(v)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::Array(v.into_iter().map(Into::into).collect::<Vec<_>>())
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(Vec::from(v.as_bytes()).into())
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(value: BTreeMap<String, Value>) -> Self {
        Value::Map(value)
    }
}

impl From<DateTime<Utc>> for Value {
    fn from(v: DateTime<Utc>) -> Self {
        Value::Timestamp(v)
    }
}

impl TryFrom<&Value> for f64 {
    type Error = Error;

    fn try_from(value: &Value) -> std::result::Result<Self, Self::Error> {
        match value {
            Value::Integer(v) => Ok(*v as f64),
            Value::Float(v) => Ok(*v),
            _ => Err(Error::Coerce(value.kind(), ValueKind::Float)),
        }
    }
}

impl TryFrom<&Value> for i64 {
    type Error = Error;

    fn try_from(value: &Value) -> std::result::Result<Self, Self::Error> {
        match value {
            Value::Integer(v) => Ok(*v),
            Value::Float(v) => Ok(*v as i64),
            _ => Err(Error::Coerce(value.kind(), ValueKind::Integer)),
        }
    }
}

impl TryFrom<&Value> for String {
    type Error = Error;

    fn try_from(value: &Value) -> std::result::Result<Self, Self::Error> {
        use Value::*;

        match value {
            String(v) => Ok(StdString::from_utf8_lossy(&v).into_owned()),
            Integer(v) => Ok(format!("{}", v)),
            Float(v) => Ok(format!("{}", v)),
            Boolean(v) => Ok(format!("{}", v)),
            Null => Ok("".to_owned()),
            _ => Err(Error::Coerce(value.kind(), ValueKind::String)),
        }
    }
}

impl TryFrom<Value> for String {
    type Error = Error;

    fn try_from(value: Value) -> std::result::Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<Value> for i64 {
    type Error = Error;

    fn try_from(value: Value) -> std::result::Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl Value {
    pub fn kind(&self) -> ValueKind {
        self.into()
    }

    /// Similar to [`std::ops::Mul`], but fallible (e.g. `TryMul`).
    pub fn try_mul(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Mul(self.kind(), rhs.kind());

        let value = match &self {
            Value::String(lhv) => lhv
                .repeat(i64::try_from(&rhs).map_err(|_| err())? as usize)
                .into(),
            Value::Integer(lhv) => (lhv * i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv * f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Div`], but fallible (e.g. `TryDiv`).
    pub fn try_div(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Div(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv / i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv / f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Add`], but fallible (e.g. `TryAdd`).
    pub fn try_add(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Add(self.kind(), rhs.kind());

        let value = match &self {
            Value::String(lhv) => format!(
                "{}{}",
                String::from_utf8_lossy(&lhv),
                String::try_from(&rhs).map_err(|_| err())?
            )
            .into(),
            Value::Integer(lhv) => (lhv + i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv + f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Sub`], but fallible (e.g. `TrySub`).
    pub fn try_sub(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Sub(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv - i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv - f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Try to "OR" (`||`) two values types.
    ///
    /// A lhs value of `Null` delegates to the rhs value.
    pub fn try_or(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Or(self.kind(), rhs.kind());

        let value = match self {
            Value::Null => rhs,
            Value::Boolean(lhv) => match rhs {
                Value::Boolean(rhv) => (lhv || rhv).into(),
                _ => return Err(err()),
            },
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Try to "AND" (`&&`) two values types.
    ///
    /// A lhs value of `Null` returns `false`.
    pub fn try_and(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Or(self.kind(), rhs.kind());

        let value = match self {
            Value::Null => Value::Boolean(false),
            Value::Boolean(lhv) => match rhs {
                Value::Boolean(rhv) => (lhv && rhv).into(),
                _ => return Err(err()),
            },
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Rem`], but fallible (e.g. `TryRem`).
    pub fn try_rem(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Rem(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv % i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv % f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_gt(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Rem(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv > i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv > f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_ge(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Ge(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv >= i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv >= f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_lt(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Ge(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv < i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv < f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_le(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Ge(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv <= i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv <= f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Eq`], but does a lossless comparison for integers
    /// and floats.
    pub fn eq_lossy(&self, rhs: &Self) -> bool {
        use Value::*;

        match self {
            // FIXME: when cmoparing ints to floats, always change the int to
            // float, not the other way around
            //
            // Do the same for multiplication, etc.
            Integer(lhv) => i64::try_from(rhs).map(|rhv| *lhv == rhv).unwrap_or(false),
            Float(lhv) => f64::try_from(rhs).map(|rhv| *lhv == rhv).unwrap_or(false),
            _ => self == rhs,
        }
    }

    /// Returns [`Value::String`], lossy converting any other variant.
    pub fn as_string_lossy(&self) -> Self {
        use Value::*;

        match self {
            s @ String(_) => s.clone(), // cloning a Bytes is cheap
            Integer(v) => Value::from(format!("{}", v)),
            Float(v) => Value::from(format!("{}", v)),
            Boolean(v) => Value::from(format!("{}", v)),
            Map(_) => Value::from(""),
            Array(_) => Value::from(""),
            Timestamp(v) => Value::from(v.to_string()),
            Null => Value::from(""),
        }
    }
}
