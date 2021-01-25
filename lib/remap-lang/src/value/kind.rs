#![allow(non_upper_case_globals)]

use super::Value;
use std::fmt;
use std::ops::Deref;

bitflags::bitflags! {
    pub struct Kind: u16 {
        const Bytes = 1 << 1;
        const Integer = 1 << 2;
        const Float = 1 << 3;
        const Boolean = 1 << 4;
        const Map = 1 << 5;
        const Array = 1 << 6;
        const Timestamp = 1 << 7;
        const Regex = 1 << 8;
        const Null = 1 << 9;
    }
}

impl Kind {
    /// Returns `true` if self is more than one, but not all
    /// [`value::Kind`]s.
    pub fn is_some(self) -> bool {
        !self.is_exact() && !self.is_all() && !self.is_empty()
    }

    /// Return the existing kinds, without non-scalar kinds (maps and arrays).
    pub fn scalar(self) -> Self {
        self & !(Kind::Array | Kind::Map)
    }

    /// Returns `true` if the [`value::Kind`] is a scalar and `false` if it's
    /// map or array.
    pub fn is_scalar(self) -> bool {
        self == self.scalar()
    }
}

macro_rules! impl_kind {
    ($(($kind:tt, $name:tt)),+ $(,)*) => {
        impl fmt::Display for Kind {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                if !self.is_some() {
                    return f.write_str(self)
                }

                let mut kinds = vec![];
                $(paste::paste! {
                if self.[<contains_ $name>]() {
                    kinds.push(Kind::$kind.as_str())
                }
                })+

                let last = kinds.pop();
                let mut string = kinds.join(", ");

                if let Some(last) = last {
                    if !string.is_empty() {
                        string.push_str(" or ")
                    }

                    string.push_str(last);
                }

                f.write_str(&string)
            }
        }

        impl Kind {
            pub fn as_str(self) -> &'static str {
                #[allow(unreachable_patterns)]
                match self {
                    Kind::Bytes => "string", // special-cased
                    $(Kind::$kind => stringify!($name)),+,
                    _ if self.is_all() => "any",
                    _ if self.is_empty() => "none",
                    _ => "some",
                }
            }

            pub fn is_exact(self) -> bool {
                match self {
                    $(Kind::$kind => true,)+
                    _ => false,
                }
            }

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
    (Map, map),
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
            Value::Map(_) => Kind::Map,
            Value::Array(_) => Kind::Array,
            Value::Timestamp(_) => Kind::Timestamp,
            Value::Regex(_) => Kind::Regex,
            Value::Null => Kind::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Kind;

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
            Kind::Map,
            Kind::Array | Kind::Integer,
            Kind::Map | Kind::Bytes,
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
