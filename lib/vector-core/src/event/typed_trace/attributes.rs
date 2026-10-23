//! Attribute maps mirroring OTLP `AnyValue`.

use std::{collections::BTreeMap, fmt::Write as _};

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

    /// Inserts `key` as a string attribute when `value` is present and non-empty.
    pub fn insert_nonempty(&mut self, key: impl Into<KeyString>, value: Option<String>) {
        if let Some(text) = value
            && !text.is_empty()
        {
            self.insert(key, AttrValue::String(text));
        }
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

    /// Unquoted spelling of a non-finite float: `NaN`, `Infinity`, or `-Infinity`.
    ///
    /// Finite values return `None`. [`Self::write_json`] writes this spelling as a JSON string.
    #[must_use]
    pub const fn non_finite_spelling(value: f64) -> Option<&'static str> {
        if value.is_nan() {
            Some("NaN")
        } else if value.is_infinite() {
            if value.is_sign_negative() {
                Some("-Infinity")
            } else {
                Some("Infinity")
            }
        } else {
            None
        }
    }

    /// Appends compact JSON for this value to `out`.
    ///
    /// Map keys are written in order. Non-finite floats are the JSON strings `NaN`,
    /// `Infinity`, and `-Infinity`. Bytes are lossy UTF-8, then a JSON string.
    pub fn write_json(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(true) => out.push_str("true"),
            Self::Bool(false) => out.push_str("false"),
            Self::Integer(value) => write!(out, "{value}").expect("integer formats"),
            Self::Float(value) => write_json_float(out, *value),
            Self::String(value) => push_json_string(out, value),
            Self::Bytes(value) => push_json_string(out, &String::from_utf8_lossy(value)),
            Self::Array(values) => {
                out.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    value.write_json(out);
                }
                out.push(']');
            }
            Self::Map(map) => {
                out.push('{');
                for (index, (key, value)) in map.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    push_json_string(out, key.as_ref());
                    out.push(':');
                    value.write_json(out);
                }
                out.push('}');
            }
        }
    }
}

fn write_json_float(out: &mut String, value: f64) {
    if let Some(spelling) = AttrValue::non_finite_spelling(value) {
        push_json_string(out, spelling);
    } else {
        serde_json::to_writer(JsonBuf(out), &value).expect("finite f64 serializes to JSON");
    }
}

fn push_json_string(out: &mut String, value: &str) {
    serde_json::to_writer(JsonBuf(out), value).expect("string serializes to JSON");
}

/// Appends `serde_json` output to a [`String`]. Each write is valid UTF-8.
struct JsonBuf<'a>(&'a mut String);

impl std::io::Write for JsonBuf<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .push_str(simdutf8::basic::from_utf8(buf).expect("JSON is UTF-8"));
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
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

    use super::{AttrMap, AttrValue, Attributes};

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

    #[test]
    fn write_json_is_compact_and_quotes_non_finite_floats() {
        let mut out = String::new();
        let mut map = AttrMap::new();
        map.insert("z".into(), AttrValue::Integer(1));
        map.insert("a".into(), AttrValue::Float(f64::NEG_INFINITY));
        AttrValue::Map(map).write_json(&mut out);
        assert_eq!(out, r#"{"a":"-Infinity","z":1}"#);

        out.clear();
        AttrValue::Array(vec![
            AttrValue::String("a\"b".into()),
            AttrValue::Float(-0.0),
            AttrValue::Bytes(bytes::Bytes::from_static(b"ok")),
            AttrValue::Null,
        ])
        .write_json(&mut out);
        assert_eq!(out, r#"["a\"b",-0.0,"ok",null]"#);
        assert_eq!(AttrValue::non_finite_spelling(1.5), None);
        assert_eq!(AttrValue::non_finite_spelling(f64::NAN), Some("NaN"));
    }

    #[test]
    fn insert_nonempty_skips_absent_and_empty() {
        let mut attributes = Attributes::new();
        attributes.insert_nonempty("a", None);
        attributes.insert_nonempty("b", Some(String::new()));
        attributes.insert_nonempty("c", Some("ok".into()));
        assert!(attributes.get("a").is_none());
        assert!(attributes.get("b").is_none());
        assert_eq!(attributes.get("c"), Some(&AttrValue::String("ok".into())));
    }
}
