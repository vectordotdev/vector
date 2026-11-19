//! Attribute maps mirroring OTLP `AnyValue`.

use std::collections::BTreeMap;

use bytes::Bytes;
use vector_common::byte_size_of::ByteSizeOf;
use vrl::value::KeyString;

/// Ordered map of attribute keys to [`AttrValue`]s.
pub type AttrMap = BTreeMap<KeyString, AttrValue>;

/// Ordered attribute map used throughout the typed trace model.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Attributes(AttrMap);

impl Attributes {
    /// Creates an empty map.
    #[must_use]
    pub fn new() -> Self {
        Self(AttrMap::new())
    }

    /// Returns the number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` when the map has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns a value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&AttrValue> {
        self.0.get(key)
    }

    /// Inserts `key` / `value`, returning the previous value if present.
    pub fn insert(&mut self, key: impl Into<KeyString>, value: AttrValue) -> Option<AttrValue> {
        self.0.insert(key.into(), value)
    }

    /// Removes `key` if present.
    pub fn remove(&mut self, key: &str) -> Option<AttrValue> {
        self.0.remove(key)
    }

    /// Iterates entries in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&KeyString, &AttrValue)> {
        self.0.iter()
    }

    /// Returns a mutable view of the inner map.
    pub fn as_map_mut(&mut self) -> &mut AttrMap {
        &mut self.0
    }

    /// Consumes the newtype, returning the inner map.
    #[must_use]
    pub fn into_inner(self) -> AttrMap {
        self.0
    }
}

impl From<AttrMap> for Attributes {
    fn from(map: AttrMap) -> Self {
        Self(map)
    }
}

impl FromIterator<(KeyString, AttrValue)> for Attributes {
    fn from_iter<T: IntoIterator<Item = (KeyString, AttrValue)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl ByteSizeOf for Attributes {
    fn allocated_bytes(&self) -> usize {
        self.0.allocated_bytes()
    }
}

/// Leaf attribute value. Mirrors OTLP `AnyValue`.
#[derive(Clone, Debug)]
pub enum AttrValue {
    /// UTF-8 string.
    String(String),
    /// Arbitrary bytes.
    Bytes(Bytes),
    /// Boolean.
    Bool(bool),
    /// Signed 64-bit integer.
    Integer(i64),
    /// IEEE-754 floating-point value, including non-finite values.
    Float(f64),
    /// Homogeneous or mixed array.
    Array(Vec<AttrValue>),
    /// Nested map.
    Map(AttrMap),
    /// Explicit null.
    Null,
}

impl AttrValue {
    /// Returns the string, or `None` for any other variant.
    #[must_use]
    pub fn into_string(self) -> Option<String> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the boolean, or `None` for any other variant.
    #[must_use]
    pub fn into_bool(self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(v),
            _ => None,
        }
    }

    /// Returns the integer, or `None` for any other variant.
    #[must_use]
    pub fn into_integer(self) -> Option<i64> {
        match self {
            Self::Integer(v) => Some(v),
            _ => None,
        }
    }

    /// Returns the float, or `None` for any other variant.
    #[must_use]
    pub fn into_float(self) -> Option<f64> {
        match self {
            Self::Float(v) => Some(v),
            _ => None,
        }
    }

    /// Returns the map, or `None` for any other variant.
    #[must_use]
    pub fn into_map(self) -> Option<AttrMap> {
        match self {
            Self::Map(map) => Some(map),
            _ => None,
        }
    }
}

impl PartialEq for AttrValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Bytes(a), Self::Bytes(b)) => a == b,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::Array(a), Self::Array(b)) => a == b,
            (Self::Map(a), Self::Map(b)) => a == b,
            (Self::Null, Self::Null) => true,
            _ => false,
        }
    }
}

impl Eq for AttrValue {}

impl ByteSizeOf for AttrValue {
    fn allocated_bytes(&self) -> usize {
        match self {
            Self::String(s) => s.len(),
            Self::Bytes(b) => b.len(),
            Self::Bool(_) | Self::Integer(_) | Self::Float(_) | Self::Null => 0,
            Self::Array(values) => values.allocated_bytes(),
            Self::Map(map) => map.allocated_bytes(),
        }
    }
}

impl From<String> for AttrValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for AttrValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<Bytes> for AttrValue {
    fn from(value: Bytes) -> Self {
        Self::Bytes(value)
    }
}

impl From<bool> for AttrValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for AttrValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<f64> for AttrValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

#[cfg(test)]
mod tests {
    use similar_asserts::assert_eq;

    use super::{AttrMap, AttrValue};

    #[test]
    fn equality_uses_float_bits() {
        assert_eq!(AttrValue::Float(f64::NAN), AttrValue::Float(f64::NAN));
        assert_ne!(AttrValue::Float(0.0), AttrValue::Float(-0.0));
    }

    #[test]
    fn variant_accessors_return_only_their_variant() {
        assert_eq!(AttrValue::from("s").into_string(), Some("s".to_owned()));
        assert_eq!(AttrValue::from(true).into_bool(), Some(true));
        assert_eq!(AttrValue::from(3_i64).into_integer(), Some(3));
        assert_eq!(AttrValue::from(0.5).into_float(), Some(0.5));
        assert_eq!(
            AttrValue::Map(AttrMap::new()).into_map(),
            Some(AttrMap::new())
        );

        assert_eq!(AttrValue::from(1_i64).into_string(), None);
        assert_eq!(AttrValue::from("true").into_bool(), None);
        assert_eq!(AttrValue::from(1.0).into_integer(), None);
        assert_eq!(AttrValue::from(1_i64).into_float(), None);
        assert_eq!(AttrValue::Null.into_map(), None);
    }
}
