mod kind;
mod object;

use crate::{Field, Path, Segment, Segment::*};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::iter::FromIterator;
use std::str::FromStr;

pub use kind::Kind;

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // TODO: investigate
pub enum Value {
    Bytes(Bytes),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Map(BTreeMap<String, Value>),
    Array(Vec<Value>),
    Timestamp(DateTime<Utc>),
    Regex(regex::Regex),
    Null,
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use Value::*;

        match &self {
            Bytes(v) => serializer.serialize_str(&String::from_utf8_lossy(&v)),
            Integer(v) => serializer.serialize_i64(*v),
            Float(v) => serializer.serialize_f64(*v),
            Boolean(v) => serializer.serialize_bool(*v),
            Map(v) => serializer.collect_map(v),
            Array(v) => serializer.collect_seq(v),
            Timestamp(v) => serializer.serialize_str(&v.to_string()),
            Regex(v) => serializer.serialize_str(&v.to_string()),
            Null => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ValueVisitor;

        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = Value;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("any valid JSON value")
            }

            #[inline]
            fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
                Ok(value.into())
            }

            #[inline]
            fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
                Ok(value.into())
            }

            #[inline]
            fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
                Ok((value as i64).into())
            }

            #[inline]
            fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
                Ok(value.into())
            }

            #[inline]
            fn visit_str<E>(self, value: &str) -> Result<Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Value::Bytes(Bytes::copy_from_slice(value.as_bytes())))
            }

            #[inline]
            fn visit_string<E>(self, value: String) -> Result<Value, E> {
                Ok(Value::Bytes(value.into()))
            }

            #[inline]
            fn visit_none<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }

            #[inline]
            fn visit_some<D>(self, deserializer: D) -> Result<Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Deserialize::deserialize(deserializer)
            }

            #[inline]
            fn visit_unit<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }

            #[inline]
            fn visit_seq<V>(self, mut visitor: V) -> Result<Value, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let mut vec = Vec::new();
                while let Some(value) = visitor.next_element()? {
                    vec.push(value);
                }

                Ok(Value::Array(vec))
            }

            fn visit_map<V>(self, mut visitor: V) -> Result<Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut map = BTreeMap::new();
                while let Some((key, value)) = visitor.next_entry()? {
                    map.insert(key, value);
                }

                Ok(Value::Map(map))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        use Value::*;

        match self {
            Bytes(v1) => other.as_bytes().map(|v2| v1 == v2).unwrap_or_default(),
            Integer(v1) => other.as_integer().map(|v2| v1 == v2).unwrap_or_default(),
            Float(v1) => other.as_float().map(|v2| v1 == v2).unwrap_or_default(),
            Boolean(v1) => other.as_boolean().map(|v2| v1 == v2).unwrap_or_default(),
            Map(v1) => other.as_map().map(|v2| v1 == v2).unwrap_or_default(),
            Array(v1) => other.as_array().map(|v2| v1 == v2).unwrap_or_default(),
            Timestamp(v1) => other.as_timestamp().map(|v2| v1 == v2).unwrap_or_default(),
            Null => other.is_null(),
            Regex(v1) => match other {
                Regex(v2) => v1.as_str() == v2.as_str(),
                _ => false,
            },
        }
    }
}

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error(
        r#"expected {}, got "{1}""#,
        if .0.is_some() {
            format!(r#"{}"#, .0)
        } else {
            format!(r#""{}""#, .0)
        }
    )]
    Expected(Kind, Kind),

    #[error(r#"unable to coerce "{0}" into "{1}""#)]
    Coerce(Kind, Kind),

    #[error("unable to calculate remainder of values type {0} and {1}")]
    Rem(Kind, Kind),

    #[error("unable to multiply value type {0} by {1}")]
    Mul(Kind, Kind),

    #[error("unable to divide value type {0} by {1}")]
    Div(Kind, Kind),

    #[error("unable to integer divide value type {0} by {1}")]
    IntDiv(Kind, Kind),

    #[error("unable to divide by zero")]
    DivideByZero,

    #[error("unable to add value type {1} to {0}")]
    Add(Kind, Kind),

    #[error("unable to subtract value type {1} from {0}")]
    Sub(Kind, Kind),

    #[error("unable to OR value type {0} with {1}")]
    Or(Kind, Kind),

    #[error("unable to AND value type {0} with {1}")]
    And(Kind, Kind),

    #[error("unable to compare {0} > {1}")]
    Gt(Kind, Kind),

    #[error("unable to compare {0} >= {1}")]
    Ge(Kind, Kind),

    #[error("unable to compare {0} < {1}")]
    Lt(Kind, Kind),

    #[error("unable to compare {0} <= {1}")]
    Le(Kind, Kind),

    #[error("unable to query into {0}")]
    Query(Kind),

    #[error("invalid field format: {0}")]
    Field(String),
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

impl From<crate::Error> for Value {
    fn from(v: crate::Error) -> Self {
        Value::Bytes(v.to_string().into())
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Boolean(v)
    }
}

impl From<regex::Regex> for Value {
    fn from(v: regex::Regex) -> Self {
        Value::Regex(v)
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

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::Array(v.into_iter().map(Into::into).collect::<Vec<_>>())
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Bytes(Bytes::copy_from_slice(v.as_bytes()))
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Value::Null
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(value: BTreeMap<String, Value>) -> Self {
        Value::Map(value)
    }
}

impl FromIterator<(String, Value)> for Value {
    fn from_iter<I: IntoIterator<Item = (String, Value)>>(iter: I) -> Self {
        Value::Map(iter.into_iter().collect::<BTreeMap<String, Value>>())
    }
}

impl FromIterator<Value> for Value {
    fn from_iter<I: IntoIterator<Item = Value>>(iter: I) -> Self {
        Value::Array(iter.into_iter().collect::<Vec<Value>>())
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
            _ => Err(Error::Coerce(value.kind(), Kind::Float)),
        }
    }
}

impl TryFrom<&Value> for i64 {
    type Error = Error;

    fn try_from(value: &Value) -> std::result::Result<Self, Self::Error> {
        match value {
            Value::Integer(v) => Ok(*v),
            Value::Float(v) => Ok(*v as i64),
            _ => Err(Error::Coerce(value.kind(), Kind::Integer)),
        }
    }
}

impl TryFrom<&Value> for String {
    type Error = Error;

    fn try_from(value: &Value) -> std::result::Result<Self, Self::Error> {
        use Value::*;

        match value {
            Bytes(v) => Ok(String::from_utf8_lossy(&v).into_owned()),
            Integer(v) => Ok(format!("{}", v)),
            Float(v) => Ok(format!("{}", v)),
            Boolean(v) => Ok(format!("{}", v)),
            Null => Ok("".to_owned()),
            _ => Err(Error::Coerce(value.kind(), Kind::Bytes)),
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

macro_rules! value_impl {
    ($(($func:expr, $variant:expr, $ret:ty)),+ $(,)*) => {
        impl Value {
            $(paste::paste! {
            pub fn [<is_ $func>](&self) -> bool {
                matches!(self, Value::$variant(_))
            }

            pub fn [<as_ $func>](&self) -> Option<&$ret> {
                match self {
                    Value::$variant(v) => Some(v),
                    _ => None,
                }
            }

            pub fn [<as_ $func _mut>](&mut self) -> Option<&mut $ret> {
                match self {
                    Value::$variant(v) => Some(v),
                    _ => None,
                }
            }

            pub fn [<try_ $func>](self) -> Result<$ret, Error> {
                match self {
                    Value::$variant(v) => Ok(v),
                    v => Err(Error::Expected(Kind::$variant, v.kind())),
                }
            }

            pub fn [<unwrap_ $func>](self) -> $ret {
                self.[<try_ $func>]().expect(stringify!($func))
            }
            })+

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
                    v => Err(Error::Expected(Kind::Null, v.kind())),
                }
            }

            pub fn unwrap_null(self) -> () {
                self.try_null().expect("null")
            }

            pub fn try_bytes_utf8_lossy<'a>(&'a self) -> Result<std::borrow::Cow<'a, str>, Error> {
                match self.as_bytes() {
                    Some(bytes) => Ok(String::from_utf8_lossy(&bytes)),
                    None => Err(Error::Expected(Kind::Bytes, self.kind())),
                }
            }
        }
    };
}

value_impl! {
    (bytes, Bytes, Bytes),
    (integer, Integer, i64),
    (float, Float, f64),
    (boolean, Boolean, bool),
    (map, Map, BTreeMap<String, Value>),
    (array, Array, Vec<Value>),
    (timestamp, Timestamp, DateTime<Utc>),
    (regex, Regex, regex::Regex),
    // manually implemented due to no variant value
    // (null, Null, ()),
}

impl Value {
    pub fn kind(&self) -> Kind {
        self.into()
    }

    /// Similar to [`std::ops::Mul`], but fallible (e.g. `TryMul`).
    pub fn try_mul(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Mul(self.kind(), rhs.kind());

        let value = match &self {
            Value::Bytes(lhv) => lhv
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

        let rhs = f64::try_from(&rhs).map_err(|_| err())?;

        if rhs == 0.0 {
            return Err(Error::DivideByZero);
        }

        let value = match self {
            Value::Integer(lhv) => (lhv as f64 / rhs).into(),
            Value::Float(lhv) => (lhv / rhs).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    pub fn try_int_div(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::IntDiv(self.kind(), rhs.kind());

        let rhs = i64::try_from(&rhs).map_err(|_| err())?;

        if rhs == 0 {
            return Err(Error::DivideByZero);
        }

        let value = match &self {
            Value::Integer(lhv) => (lhv / rhs).into(),
            Value::Float(lhv) => (*lhv as i64 / rhs).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Add`], but fallible (e.g. `TryAdd`).
    pub fn try_add(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Add(self.kind(), rhs.kind());

        let value = match &self {
            Value::Bytes(lhv) => format!(
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

    /// "OR" (`||`) two values types.
    ///
    /// A lhs value of `Null` or a `false` delegates to the rhs value,
    /// everything else delegates to `lhs`.
    pub fn or(self, rhs: Self) -> Self {
        match self {
            Value::Null => rhs,
            Value::Boolean(lhv) if !lhv => rhs,
            value => value,
        }
    }

    /// Try to "AND" (`&&`) two values types.
    ///
    /// A lhs or rhs value of `Null` returns `false`.
    ///
    /// TODO: this should maybe work similar to `OR`, in that it supports any
    /// rhs value kind, to support: `true && "foo"` to resolve to "foo".
    pub fn try_and(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Or(self.kind(), rhs.kind());

        let value = match self {
            Value::Null => false.into(),
            Value::Boolean(lhv) => match rhs {
                Value::Null => false.into(),
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

    /// Return a list of [`Path`]s present in this [`Value`].
    ///
    /// This method will always return at least _one_ path (the root path,
    /// pointing to the value itself).
    ///
    /// If the value represents a [`Value::Map`] or [`Value::Array`], and the
    /// relevant collection is not empty, it will recursively traverse into
    /// those values to return the final list of paths.
    ///
    /// # Errors
    ///
    /// This function can return the [`Error::Field`] error, if one of the
    /// [`Value::Map`] keys does not conform to the `ident` or `string` rule as
    /// defined by the Remap language grammar.
    ///
    /// # Examples
    ///
    /// ```
    /// # use remap_lang::{Path, Value};
    /// # use std::str::FromStr;
    /// # use std::collections::BTreeMap;
    /// # use std::iter::FromIterator;
    ///
    /// let fields = vec![("foo".to_owned(), Value::Array(vec![0.into(), 1.into()]))];
    /// let map = BTreeMap::from_iter(fields.into_iter());
    ///
    /// let paths = Value::Map(map).paths().unwrap();
    ///
    /// assert_eq!(
    ///     paths.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
    ///     vec![
    ///         ".".to_owned(),
    ///         ".foo".to_owned(),
    ///         ".foo[0]".to_owned(),
    ///         ".foo[1]".to_owned(),
    ///     ],
    /// )
    /// ```
    pub fn paths(&self) -> Result<Vec<Path>, Error> {
        let mut paths = vec![Path::root()];
        paths.append(&mut self.paths_from_segments(&mut vec![])?);

        Ok(paths)
    }

    /// Get a reference to a value from a given path.
    ///
    /// # Examples
    ///
    /// Given an existing value, there are three routes this function can take:
    ///
    /// 1. If the path points to the root (`.`), it will return the current
    ///    value:
    ///
    ///    ```rust
    ///    # use remap_lang::{Path, Value};
    ///    # use std::str::FromStr;
    ///
    ///    let value = Value::Boolean(true);
    ///    let path = Path::from_str(".").unwrap();
    ///
    ///    assert_eq!(value.get_by_path(&path), Some(&Value::Boolean(true)))
    ///    ```
    ///
    /// 2. If the path points to an index, if the value is an `Array`, it will
    ///    return the value at the given index, if one exists, or it will return
    ///    `None`:
    ///
    ///    ```rust
    ///    # use remap_lang::{Path, Value};
    ///    # use std::str::FromStr;
    ///
    ///    let value = Value::Array(vec![false.into(), true.into()]);
    ///    let path = Path::from_str(".[1]").unwrap();
    ///
    ///    assert_eq!(value.get_by_path(&path), Some(&Value::Boolean(true)))
    ///    ```
    ///
    /// 3. If the path points to a nested path, if the value is a `Map`, it will
    ///    traverse into the map, and return the appropriate value, if one
    ///    exists:
    ///
    ///    ```rust
    ///    # use remap_lang::{Path, Value};
    ///    # use std::str::FromStr;
    ///    # use std::collections::BTreeMap;
    ///    # use std::iter::FromIterator;
    ///
    ///    let map = BTreeMap::from_iter(vec![("foo".to_owned(), true.into())].into_iter());
    ///    let value = Value::Map(map);
    ///    let path = Path::from_str(".foo").unwrap();
    ///
    ///    assert_eq!(value.get_by_path(&path), Some(&Value::Boolean(true)))
    ///    ```
    ///
    pub fn get_by_path(&self, path: &Path) -> Option<&Value> {
        self.get_by_segments(path.segments())
    }

    /// Similar to [`Value::get_by_path`], but returns a mutable reference to
    /// the value.
    pub fn get_by_path_mut(&mut self, path: &Path) -> Option<&mut Value> {
        self.get_by_segments_mut(path.segments())
    }

    /// Insert a value, given the provided path.
    ///
    /// # Examples
    ///
    /// ## Insert At Field
    ///
    /// ```
    /// # use remap_lang::{Path, Value};
    /// # use std::str::FromStr;
    /// # use std::collections::BTreeMap;
    /// # use std::iter::FromIterator;
    ///
    /// let fields = vec![("foo".to_owned(), Value::from("bar"))];
    /// let map = BTreeMap::from_iter(fields.into_iter());
    ///
    /// let mut value = Value::Map(map);
    /// let path = Path::from_str(".foo").unwrap();
    ///
    /// value.insert_by_path(&path, true.into());
    ///
    /// assert_eq!(
    ///     value.get_by_path(&path),
    ///     Some(&true.into()),
    /// )
    /// ```
    ///
    /// ## Insert Into Array
    ///
    /// ```
    /// # use remap_lang::{value, Path, Value, map};
    /// # use std::str::FromStr;
    /// # use std::collections::BTreeMap;
    /// # use std::iter::FromIterator;
    ///
    /// let mut value = value!([false, true]);
    /// let path = Path::from_str(".[1].foo").unwrap();
    ///
    /// value.insert_by_path(&path, "bar".into());
    ///
    /// assert_eq!(
    ///     value.get_by_path(&Path::from_str(".").unwrap()),
    ///     Some(&value!([false, {foo: "bar"}])),
    /// )
    /// ```
    ///
    pub fn insert_by_path(&mut self, path: &Path, new: Value) {
        self.insert_by_segments(path.segments(), new)
    }

    /// Remove a value, given the provided path.
    ///
    /// This works similar to [`Value::get_by_path`], except that it removes the
    /// value at the provided path, instead of returning it.
    ///
    /// The one difference is if a root path (`.`) is provided. In this case,
    /// the [`Value`] object (i.e. "self") is set to `Value::Null`.
    ///
    /// If the `compact` argument is set to `true`, then any `Array` or `Map`
    /// that had one of its elements removed and is now empty, is removed as
    /// well.
    pub fn remove_by_path(&mut self, path: &Path, compact: bool) {
        self.remove_by_segments(path.segments(), compact)
    }

    fn get_by_segments(&self, segments: &[Segment]) -> Option<&Value> {
        let (segment, next) = match segments.split_first() {
            Some(segments) => segments,
            None => return Some(self),
        };

        self.get_by_segment(segment)
            .and_then(|value| value.get_by_segments(next))
    }

    fn get_by_segment(&self, segment: &Segment) -> Option<&Value> {
        match segment {
            Field(field) => self.as_map().and_then(|map| map.get(field.as_str())),
            Coalesce(fields) => self
                .as_map()
                .and_then(|map| fields.iter().find_map(|field| map.get(field.as_str()))),
            Index(index) => self.as_array().and_then(|array| array.get(*index)),
        }
    }

    fn get_by_segments_mut(&mut self, segments: &[Segment]) -> Option<&mut Value> {
        let (segment, next) = match segments.split_first() {
            Some(segments) => segments,
            None => return Some(self),
        };

        self.get_by_segment_mut(segment)
            .and_then(|value| value.get_by_segments_mut(next))
    }

    fn get_by_segment_mut(&mut self, segment: &Segment) -> Option<&mut Value> {
        match segment {
            Field(field) => self
                .as_map_mut()
                .and_then(|map| map.get_mut(field.as_str())),
            Coalesce(fields) => self.as_map_mut().and_then(|map| {
                fields
                    .iter()
                    .find(|field| map.contains_key(field.as_str()))
                    .and_then(move |field| map.get_mut(field.as_str()))
            }),
            Index(index) => self.as_array_mut().and_then(|array| array.get_mut(*index)),
        }
    }

    fn remove_by_segments(&mut self, segments: &[Segment], compact: bool) {
        let (segment, next) = match segments.split_first() {
            Some(segments) => segments,
            None => {
                return match self {
                    Value::Map(v) => v.clear(),
                    Value::Array(v) => v.clear(),
                    _ => *self = Value::Null,
                }
            }
        };

        if next.is_empty() {
            return self.remove_by_segment(segment);
        }

        if let Some(value) = self.get_by_segment_mut(segment) {
            value.remove_by_segments(next, compact);

            match value {
                Value::Map(v) if compact & v.is_empty() => self.remove_by_segment(segment),
                Value::Array(v) if compact & v.is_empty() => self.remove_by_segment(segment),
                _ => {}
            }
        }
    }

    fn remove_by_segment(&mut self, segment: &Segment) {
        match segment {
            Field(field) => self.as_map_mut().and_then(|map| map.remove(field.as_str())),

            Coalesce(fields) => fields
                .iter()
                .find(|field| {
                    self.as_map()
                        .map(|map| map.contains_key(field.as_str()))
                        .unwrap_or_default()
                })
                .and_then(|field| self.as_map_mut().and_then(|map| map.remove(field.as_str()))),

            Index(index) => self.as_array_mut().map(|array| array.remove(*index)),
        };
    }

    /// Create a list of [`Path`]s from a list of [`Segment`]s.
    ///
    /// # Errors
    ///
    /// This function can return the [`Error::Field`] error, if one of the
    /// [`Value::Map`] keys does not conform to the `ident` or `string` rule as
    /// defined by the Remap language grammar.
    fn paths_from_segments(&self, segments: &mut Vec<Segment>) -> Result<Vec<Path>, Error> {
        let mut paths = vec![];

        let mut handle_value = |value: &Value, segments: &mut Vec<Segment>| {
            paths.push(Path::new_unchecked(segments.clone()));

            if let Value::Map(_) | Value::Array(_) = value {
                paths.append(&mut value.paths_from_segments(segments)?)
            }

            Ok(())
        };

        match self {
            Value::Map(map) => map.iter().try_for_each(|(key, value)| {
                let field = Field::from_str(key).map_err(|err| Error::Field(err.to_string()))?;
                segments.push(Field(field));

                handle_value(value, segments)?;
                segments.clear();

                Ok(())
            })?,

            Value::Array(array) => array.iter().enumerate().try_for_each(|(index, value)| {
                let mut segs = segments.clone();
                segs.push(Index(index));

                handle_value(value, &mut segs)
            })?,
            _ => {}
        }

        segments.clear();

        Ok(paths)
    }

    fn insert_by_segments(&mut self, segments: &[Segment], new: Value) {
        let (segment, rest) = match segments.split_first() {
            Some(segments) => segments,
            None => return *self = new,
        };

        // As long as the provided segments match the shape of the value, we'll
        // traverse down the tree. Once we encounter a value kind that does not
        // match the requested segment, we'll update the value to match and
        // continue on, until we're able to assign the final `new` value.
        match self.get_by_segment_mut(segment) {
            Some(value) => value.insert_by_segments(rest, new),
            None => self.update_by_segments(segments, new),
        };
    }

    fn update_by_segments(&mut self, segments: &[Segment], new: Value) {
        let (segment, rest) = match segments.split_first() {
            Some(segments) => segments,
            None => return,
        };

        let mut handle_field = |field: &Field, new| {
            let key = field.as_str().to_owned();

            // `handle_field` is used to update map values, if the current value
            // isn't a map, we need to make it one.
            if !matches!(self, Value::Map(_)) {
                *self = BTreeMap::default().into()
            }

            let map = match self {
                Value::Map(map) => map,
                _ => unreachable!(),
            };

            match rest.first() {
                // If there are no other segments to traverse, we'll add the new
                // value to the current map.
                None => {
                    map.insert(key, new);
                    return;
                }
                // If there are more segments to traverse, insert an empty map
                // or array depending on what the next segment is, and continue
                // to add the next segment.
                Some(next) => match next {
                    Index(_) => map.insert(key, Value::Array(vec![])),
                    _ => map.insert(key, BTreeMap::default().into()),
                },
            };

            map.get_mut(field.as_str())
                .unwrap()
                .insert_by_segments(rest, new);
        };

        match segment {
            Field(field) => handle_field(field, new),

            Coalesce(fields) => {
                // At this point, we know that the coalesced field query did not
                // result in an actual value, so none of the fields match an
                // existing field. We'll pick the last field in the list to
                // insert the new value into.
                let field = match fields.last() {
                    Some(field) => field,
                    None => return,
                };

                handle_field(field, new)
            }

            Index(index) => match self {
                // If the current value is an array, we need to either swap out
                // an existing value, or append the new value to the array.
                Value::Array(array) => {
                    // If the array has less items than needed, we'll fill it in
                    // with `Null` values.
                    if array.len() < *index {
                        array.resize(*index, Value::Null);
                    }

                    match rest.first() {
                        None => {
                            array.push(new);
                            return;
                        }
                        Some(next) => match next {
                            Index(_) => array.push(Value::Array(vec![])),
                            _ => array.push(BTreeMap::default().into()),
                        },
                    };

                    array
                        .last_mut()
                        .expect("exists")
                        .insert_by_segments(rest, new);
                }

                // Any non-array value is swapped out with an array.
                _ => {
                    let mut array = Vec::with_capacity(index + 1);
                    array.resize(*index, Value::Null);

                    match rest.first() {
                        None => {
                            array.push(new);
                            return *self = array.into();
                        }
                        Some(next) => match next {
                            Index(_) => array.push(Value::Array(vec![])),
                            _ => array.push(BTreeMap::default().into()),
                        },
                    };

                    array
                        .last_mut()
                        .expect("exists")
                        .insert_by_segments(rest, new);

                    *self = array.into();
                }
            },
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bytes(val) => write!(f, r#""{}""#, String::from_utf8_lossy(val)),
            Value::Integer(val) => write!(f, "{}", val),
            Value::Float(val) => write!(f, "{}", val),
            Value::Boolean(val) => write!(f, "{}", val),
            Value::Map(map) => {
                let joined = map
                    .iter()
                    .map(|(key, val)| format!(r#""{}": {}"#, key, val))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{{ {} }}", joined)
            }
            Value::Array(array) => {
                let joined = array
                    .iter()
                    .map(|val| format!("{}", val))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "[{}]", joined)
            }
            Value::Timestamp(val) => write!(f, "{}", val.to_string()),
            Value::Regex(regex) => write!(f, "/{}/", regex.to_string()),
            Value::Null => write!(f, "null"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::prelude::*;

    #[test]
    fn test_display() {
        let string = format!("{}", Value::from("Sausage 🌭"));
        assert_eq!(r#""Sausage 🌭""#, string);

        let int = format!("{}", Value::from(42));
        assert_eq!("42", int);

        let float = format!("{}", Value::from(42.5));
        assert_eq!("42.5", float);

        let boolean = format!("{}", Value::from(true));
        assert_eq!("true", boolean);

        let mut map = BTreeMap::new();
        map.insert("field".to_string(), Value::from(1));
        let map = format!("{}", Value::Map(map));
        assert_eq!(r#"{ "field": 1 }"#, map);

        let array = format!("{}", Value::from(vec![1, 2, 3]));
        assert_eq!("[1, 2, 3]", array);

        let timestamp = format!("{}", Value::from(Utc.ymd(2020, 10, 21).and_hms(16, 20, 13)));
        assert_eq!("2020-10-21 16:20:13 UTC", timestamp);

        let regex = format!("{}", Value::from(regex::Regex::new("foo ba+r").unwrap()));
        assert_eq!("/foo ba+r/", regex);

        let null = format!("{}", Value::Null);
        assert_eq!("null", null);
    }
}
