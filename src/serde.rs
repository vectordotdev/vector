#![allow(missing_docs)]
use indexmap::map::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Number;
pub use vector_lib::serde::{bool_or_struct, is_default};
use vector_lib::{
    codecs::{
        BytesDecoderConfig, BytesDeserializerConfig,
        decoding::{DeserializerConfig, FramingConfig},
    },
    configurable::configurable_component,
};

pub(crate) fn remove_digit_separators(input: &str, radix: u32) -> Option<String> {
    // TOML permits underscores only between digits, including exponent digits.
    let is_digit = |byte: u8| char::from(byte).is_digit(radix);
    if input.starts_with('_')
        || input.ends_with('_')
        || input
            .as_bytes()
            .windows(3)
            .any(|bytes| bytes[1] == b'_' && (!is_digit(bytes[0]) || !is_digit(bytes[2])))
    {
        return None;
    }
    Some(input.replace('_', ""))
}

pub(crate) fn parse_integer(input: &str) -> Option<Number> {
    let input = input.trim();
    let (sign, digits) = if let Some(digits) = input.strip_prefix('-') {
        ("-", digits)
    } else {
        ("", input.strip_prefix('+').unwrap_or(input))
    };
    let (digits, radix) = [("0x", 16), ("0o", 8), ("0b", 2)]
        .into_iter()
        .find_map(|(prefix, radix)| digits.strip_prefix(prefix).map(|digits| (digits, radix)))
        .unwrap_or((digits, 10));
    if digits.starts_with(['+', '-']) {
        return None;
    }
    // Keep the sign for checked parsing, including i64::MIN, without accepting
    // arrays, maps, tags, or other configuration syntax from substituted values.
    let digits = remove_digit_separators(digits, radix)?;
    let number = format!("{sign}{digits}");
    i64::from_str_radix(&number, radix)
        .map(Number::from)
        .or_else(|_| u64::from_str_radix(&number, radix).map(Number::from))
        .ok()
}

pub(crate) fn parse_unsigned_integer_literal(input: &str) -> Option<u64> {
    let input = input.trim();

    // Preserve rejection of negative spellings, including "-0".
    if input.starts_with('-') {
        return None;
    }

    parse_integer(input)?.as_u64()
}

#[must_use]
pub const fn default_true() -> bool {
    true
}

#[must_use]
pub const fn default_false() -> bool {
    false
}

/// The default max length of the input buffer.
///
/// Any input exceeding this limit will be discarded.
#[must_use]
pub const fn default_max_length() -> usize {
    100 * 1024
}

#[must_use]
pub fn default_framing_message_based() -> FramingConfig {
    BytesDecoderConfig::new().into()
}

#[must_use]
pub fn default_decoding() -> DeserializerConfig {
    BytesDeserializerConfig::new().into()
}

/// Utilities for the `serde_json` crate.
pub mod json {
    use bytes::{BufMut, BytesMut};
    use serde::Serialize;

    /// Serialize the given data structure as JSON to `String`.
    ///
    /// # Panics
    ///
    /// Serialization can panic if `T`'s implementation of `Serialize` decides
    /// to fail, or if `T` contains a map with non-string keys.
    pub fn to_string(value: impl Serialize) -> String {
        let value = serde_json::to_value(value).unwrap();
        value.as_str().unwrap().into()
    }

    /// Serialize the given data structure as JSON to `BytesMut`.
    ///
    /// # Errors
    ///
    /// Serialization can fail if `T`'s implementation of `Serialize` decides to
    /// fail, or if `T` contains a map with non-string keys.
    pub fn to_bytes<T>(value: &T) -> serde_json::Result<BytesMut>
    where
        T: ?Sized + Serialize,
    {
        // Allocate same capacity as `serde_json::to_vec`:
        // https://github.com/serde-rs/json/blob/5fe9bdd3562bf29d02d1ab798bbcff069173306b/src/ser.rs#L2195.
        let mut bytes = BytesMut::with_capacity(128);
        serde_json::to_writer((&mut bytes).writer(), value)?;
        Ok(bytes)
    }
}

/// A field reference or value.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum FieldsOrValue<V> {
    /// A set of fields mapped by to either another field or a value.
    Fields(Fields<V>),

    /// A value.
    Value(V),
}

/// Mapping of field names to either a value or another field.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Fields<V>(IndexMap<String, FieldsOrValue<V>>);

impl<V: 'static> Fields<V> {
    pub fn all_fields(self) -> impl Iterator<Item = (String, V)> {
        self.0
            .into_iter()
            .flat_map(|(k, v)| -> Box<dyn Iterator<Item = (String, V)>> {
                match v {
                    // boxing is used as a way to avoid incompatible types of the match arms
                    FieldsOrValue::Value(v) => Box::new(std::iter::once((k, v))),
                    FieldsOrValue::Fields(f) => Box::new(
                        f.all_fields()
                            .map(move |(nested_k, v)| (format!("{k}.{nested_k}"), v)),
                    ),
                }
            })
    }
}

/// A value which can be (de)serialized from one or many instances of `T`.
#[configurable_component]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[serde(untagged)]
pub enum OneOrMany<T: 'static> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    pub fn to_vec(self) -> Vec<T> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(list) => list,
        }
    }
}

impl<T> From<T> for OneOrMany<T> {
    fn from(value: T) -> Self {
        Self::One(value)
    }
}

impl<T> From<Vec<T>> for OneOrMany<T> {
    fn from(value: Vec<T>) -> Self {
        Self::Many(value)
    }
}
