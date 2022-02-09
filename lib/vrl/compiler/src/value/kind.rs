use std::collections::BTreeMap;

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

pub use ::value::{
    kind::{find, insert, merge, nest, remove, Collection, Field, Index},
    Kind,
};

impl Value {
    pub fn kind(&self) -> Kind {
        self.into()
    }
}

pub trait DefaultValue {
    /// Returns the default [`Value`] for a given [`Kind`].
    ///
    /// If the kind is unknown (or inexact), `null` is returned as the default
    /// value.
    ///
    /// These are (somewhat) arbitrary values that mostly shouldn't be used, but
    /// are particularly useful for the "infallible assignment" expression,
    /// where the `ok` value is set to the default value kind if the expression
    /// results in an error.
    fn default_value(&self) -> Value;
}

impl DefaultValue for Kind {
    fn default_value(&self) -> Value {
        if self.is_bytes() {
            return value!("");
        }

        if self.is_integer() {
            return value!(0);
        }

        if self.is_float() {
            return value!(0.0);
        }

        if self.is_boolean() {
            return value!(false);
        }

        if self.is_timestamp() {
            return Utc.timestamp(0, 0).into();
        }

        if self.is_regex() {
            #[allow(clippy::trivial_regex)]
            return Regex::new("").unwrap().into();
        }

        if self.is_array() {
            return value!([]);
        }

        if self.is_object() {
            return value!({});
        }

        Value::Null
    }
}

impl From<&Value> for Kind {
    fn from(value: &Value) -> Self {
        match value {
            Value::Bytes(_) => Kind::bytes(),
            Value::Integer(_) => Kind::integer(),
            Value::Float(_) => Kind::float(),
            Value::Boolean(_) => Kind::boolean(),
            Value::Timestamp(_) => Kind::timestamp(),
            Value::Regex(_) => Kind::regex(),
            Value::Null => Kind::null(),

            Value::Object(object) => Kind::object(
                object
                    .iter()
                    .map(|(k, v)| (k.clone().into(), v.into()))
                    .collect::<BTreeMap<_, _>>(),
            ),

            Value::Array(array) => Kind::array(
                array
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (i.into(), v.into()))
                    .collect::<BTreeMap<_, _>>(),
            ),
        }
    }
}

impl From<Value> for Kind {
    fn from(value: Value) -> Self {
        (&value).into()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_from_value() {
        struct TestCase {
            value: Value,
            want: Kind,
        }

        for (title, TestCase { value, want }) in HashMap::from([
            (
                "bytes",
                TestCase {
                    value: value!("foo"),
                    want: Kind::bytes(),
                },
            ),
            (
                "integer",
                TestCase {
                    value: value!(3),
                    want: Kind::integer(),
                },
            ),
            (
                "float",
                TestCase {
                    value: value!(3.3),
                    want: Kind::float(),
                },
            ),
            (
                "boolean",
                TestCase {
                    value: value!(true),
                    want: Kind::boolean(),
                },
            ),
            (
                "timestamp",
                TestCase {
                    value: Utc::now().into(),
                    want: Kind::timestamp(),
                },
            ),
            (
                "regex",
                TestCase {
                    value: Regex::new("").unwrap().into(),
                    want: Kind::regex(),
                },
            ),
            (
                "null",
                TestCase {
                    value: value!(null),
                    want: Kind::null(),
                },
            ),
            (
                "object",
                TestCase {
                    value: value!({ "foo": { "bar": 12 }, "baz": true }),
                    want: Kind::object(BTreeMap::from([
                        (
                            "foo".into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::integer())])),
                        ),
                        ("baz".into(), Kind::boolean()),
                    ])),
                },
            ),
            (
                "array",
                TestCase {
                    value: value!([12, true, "foo", { "bar": null }]),
                    want: Kind::array(BTreeMap::from([
                        (0.into(), Kind::integer()),
                        (1.into(), Kind::boolean()),
                        (2.into(), Kind::bytes()),
                        (
                            3.into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::null())])),
                        ),
                    ])),
                },
            ),
        ]) {
            assert_eq!(Kind::from(value), want, "{}", title);
        }
    }
}
