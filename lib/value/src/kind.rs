//! The `kind` module has all relevant types related to progressive type checking.

mod builder;
mod collection;
mod comparison;
mod conversion;
mod debug;
pub mod find;
pub mod insert;
pub mod merge;
pub mod nest;
pub mod remove;

use std::collections::BTreeMap;

pub use collection::{Collection, Field, Index, Unknown};

use crate::Value;

/// The type (kind) of a given value.
///
/// This struct tracks the known states a type can have. By allowing one type to have multiple
/// states, the type definition can be progressively refined.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd)]
pub struct Kind {
    // NOTE: The internal API uses `Option` over `bool` for primitive types, as it makes internal
    // usage of the API easier to work with. There is no impact on the memory size of the type.
    bytes: Option<()>,
    integer: Option<()>,
    float: Option<()>,
    boolean: Option<()>,
    timestamp: Option<()>,
    regex: Option<()>,
    null: Option<()>,
    array: Option<Collection<Index>>,
    object: Option<Collection<Field>>,
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_any() {
            return f.write_str("any");
        }

        // For collections, we expand to a more descriptive representation only
        // if the type can only be this collection.
        if self.is_exact() {
            if let Some(object) = &self.object {
                return object.fmt(f);
            } else if let Some(array) = &self.array {
                return array.fmt(f);
            }
        }

        let mut kinds = vec![];

        if self.contains_bytes() {
            kinds.push("string");
        }
        if self.contains_integer() {
            kinds.push("integer");
        }
        if self.contains_float() {
            kinds.push("float");
        }
        if self.contains_boolean() {
            kinds.push("boolean");
        }
        if self.contains_timestamp() {
            kinds.push("timestamp");
        }
        if self.contains_regex() {
            kinds.push("regex");
        }
        if self.contains_null() {
            kinds.push("null");
        }
        if self.contains_array() {
            kinds.push("array");
        }
        if self.contains_object() {
            kinds.push("object");
        }
        if kinds.is_empty() {
            return f.write_str("never");
        }

        let len = kinds.len();
        for (i, kind) in kinds.into_iter().enumerate() {
            if i != 0 {
                if i == len - 1 {
                    f.write_str(" or ")?;
                } else {
                    f.write_str(", ")?;
                }
            }
            kind.fmt(f)?;
        }

        Ok(())
    }
}

impl From<&Value> for Kind {
    fn from(value: &Value) -> Self {
        match value {
            Value::Bytes(_) => Self::bytes(),
            Value::Integer(_) => Self::integer(),
            Value::Float(_) => Self::float(),
            Value::Boolean(_) => Self::boolean(),
            Value::Timestamp(_) => Self::timestamp(),
            Value::Regex(_) => Self::regex(),
            Value::Null => Self::null(),

            Value::Object(object) => Self::object(
                object
                    .iter()
                    .map(|(k, v)| (k.clone().into(), v.into()))
                    .collect::<BTreeMap<_, _>>(),
            ),

            Value::Array(array) => Self::array(
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
