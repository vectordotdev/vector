#![allow(non_upper_case_globals)]

use std::{fmt, ops::Deref};

use chrono::{TimeZone, Utc};
use regex::Regex;

use super::Value;
use crate::value;

pub const BYTES: u16 = 1 << 1;
pub const INTEGER: u16 = 1 << 2;
pub const FLOAT: u16 = 1 << 3;
pub const BOOLEAN: u16 = 1 << 4;
pub const OBJECT: u16 = 1 << 5;
pub const ARRAY: u16 = 1 << 6;
pub const TIMESTAMP: u16 = 1 << 7;
pub const REGEX: u16 = 1 << 8;
pub const NULL: u16 = 1 << 9;

pub const ANY: u16 = BYTES | INTEGER | FLOAT | BOOLEAN | OBJECT | ARRAY | TIMESTAMP | REGEX | NULL;
pub const SCALAR: u16 = BYTES | INTEGER | FLOAT | BOOLEAN | TIMESTAMP | REGEX | NULL;
pub const CONTAINER: u16 = OBJECT | ARRAY;

bitflags::bitflags! {
    pub struct Kind: u16 {
        const Bytes = BYTES;
        const Integer = INTEGER;
        const Float = FLOAT;
        const Boolean = BOOLEAN;
        const Object = OBJECT;
        const Array = ARRAY;
        const Timestamp = TIMESTAMP;
        const Regex = REGEX;
        const Null = NULL;
    }
}

impl Value {
    pub fn kind(&self) -> Kind {
        self.into()
    }
}

impl Kind {
    pub const fn new(kind: u16) -> Self {
        Kind::from_bits_truncate(kind)
    }

    /// Returns `true` if self is more than one, but not all
    /// [`value::Kind`]s.
    pub fn is_many(self) -> bool {
        !self.is_exact() && !self.is_all() && !self.is_empty()
    }

    /// Returns `true` if self is any valid [`value::Kind`].
    pub fn is_any(self) -> bool {
        self.is_all()
    }

    /// Return the existing kinds, without non-scalar kinds (objects and arrays).
    pub fn scalar(self) -> Self {
        self & !(Kind::Array | Kind::Object)
    }

    /// Returns `true` if the [`value::Kind`] is a scalar and `false` if it's
    /// map or array.
    pub fn is_scalar(self) -> bool {
        self == self.scalar()
    }

    /// Returns a quoted variant of `as_str`
    ///
    /// This function is a close duplicate of `as_str`, returning the same
    /// underlying str but with quotes. We avoid the obvious `format!("{}",
    /// self.as_str())` here as that incurs an allocation cost and the `Display`
    /// of `Kind` is sometimes in the hot path.
    ///
    /// See https://github.com/timberio/vector/pull/6878 for details.
    pub(crate) fn quoted(self) -> &'static str {
        match self {
            Kind::Bytes => "\"string\"",
            Kind::Integer => "\"integer\"",
            Kind::Float => "\"float\"",
            Kind::Boolean => "\"boolean\"",
            Kind::Object => "\"object\"",
            Kind::Array => "\"array\"",
            Kind::Timestamp => "\"timestamp\"",
            Kind::Regex => "\"regex\"",
            Kind::Null => "\"null\"",
            _ if self.is_all() => "\"unknown type\"",
            _ if self.is_empty() => "\"none\"",
            _ => "\"multiple\"",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Bytes => "string",
            Kind::Integer => "integer",
            Kind::Float => "float",
            Kind::Boolean => "boolean",
            Kind::Object => "object",
            Kind::Array => "array",
            Kind::Timestamp => "timestamp",
            Kind::Regex => "regex",
            Kind::Null => "null",
            _ if self.is_all() => "unknown type",
            _ if self.is_empty() => "none",
            _ => "multiple",
        }
    }

    pub fn is_exact(self) -> bool {
        matches!(
            self,
            Kind::Bytes
                | Kind::Integer
                | Kind::Float
                | Kind::Boolean
                | Kind::Object
                | Kind::Array
                | Kind::Timestamp
                | Kind::Regex
                | Kind::Null
        )
    }

    /// Returns the default [`Value`] for a given [`Kind`].
    ///
    /// If the kind is unknown (or inexact), `null` is returned as the default
    /// value.
    ///
    /// These are (somewhat) arbitrary values that mostly shouldn't be used, but
    /// are particularly useful for the "infallible assignment" expression,
    /// where the `ok` value is set to the default value kind if the expression
    /// results in an error.
    pub fn default_value(self) -> Value {
        match self {
            Kind::Bytes => value!(""),
            Kind::Integer => value!(0),
            Kind::Float => value!(0.0),
            Kind::Boolean => value!(false),
            Kind::Object => value!({}),
            Kind::Array => value!([]),
            Kind::Timestamp => Utc.timestamp(0, 0).into(),
            #[allow(clippy::trivial_regex)]
            Kind::Regex => Regex::new("").unwrap().into(),
            _ => Value::Null,
        }
    }
}

macro_rules! impl_kind {
    ($(($kind:tt, $name:tt)),+ $(,)*) => {
        impl fmt::Display for Kind {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                if !self.is_many() {
                    return write!(f, "{}", self.quoted())
                }

                let mut kinds = Vec::with_capacity(16);
                $(paste::paste! {
                if self.[<contains_ $name>]() {
                    kinds.push(Kind::$kind.quoted())
                }
                })+

                let last = kinds.pop();
                let mut string = kinds.join(", ");

                if let Some(last) = last {
                    if !string.is_empty() {
                        string.push_str(" or ")
                    }

                    string.push_str(&last);
                }

                f.write_str(&string)
            }
        }

        impl Kind {
            $(paste::paste! {
            pub fn [<is_ $name>](self) -> bool {
                matches!(self, Kind::$kind)
            }

            pub fn [<contains_ $name>](self) -> bool {
                self.contains(Kind::$kind)
            }
            })+
        }

        impl IntoIterator for Kind {
            type Item = Self;
            type IntoIter = std::vec::IntoIter<Self::Item>;

            fn into_iter(self) -> Self::IntoIter {
                let mut kinds = vec![];
                $(paste::paste! {
                if self.[<contains_ $name>]() {
                    kinds.push(Kind::$kind)
                }
                })+

                kinds.into_iter()
            }
        }
    };
}

impl_kind![
    (Bytes, bytes),
    (Integer, integer),
    (Float, float),
    (Boolean, boolean),
    (Object, object),
    (Array, array),
    (Timestamp, timestamp),
    (Regex, regex),
    (Null, null),
];

impl Deref for Kind {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl From<&Value> for Kind {
    fn from(value: &Value) -> Self {
        match value {
            Value::Bytes(_) => Kind::Bytes,
            Value::Integer(_) => Kind::Integer,
            Value::Float(_) => Kind::Float,
            Value::Boolean(_) => Kind::Boolean,
            Value::Object(_) => Kind::Object,
            Value::Array(_) => Kind::Array,
            Value::Timestamp(_) => Kind::Timestamp,
            Value::Regex(_) => Kind::Regex,
            Value::Null => Kind::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted() {
        for i in 0..u16::MAX {
            let kind = Kind::new(i);
            assert_eq!(kind.quoted(), format!(r#""{}""#, kind.as_str()));
        }
    }

    #[test]
    fn kind_is_scalar() {
        let scalars = vec![
            Kind::Integer,
            Kind::Bytes,
            Kind::Null | Kind::Regex,
            Kind::Timestamp | Kind::Float | Kind::Null,
        ];

        let non_scalars = vec![
            Kind::Array,
            Kind::Object,
            Kind::Array | Kind::Integer,
            Kind::Object | Kind::Array,
            Kind::Object | Kind::Bytes,
            Kind::Boolean | Kind::Null | Kind::Array,
        ];

        for kind in scalars {
            assert!(kind.is_scalar());
        }

        for kind in non_scalars {
            assert!(!kind.is_scalar());
        }
    }
}
