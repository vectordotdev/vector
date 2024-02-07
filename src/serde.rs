#![allow(missing_docs)]
use indexmap::map::IndexMap;
use serde::{Deserialize, Serialize};
use vector_lib::codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    BytesDecoderConfig, BytesDeserializerConfig,
};
use vector_lib::configurable::configurable_component;
pub use vector_lib::serde::{bool_or_struct, is_default};

pub const fn default_true() -> bool {
    true
}

pub const fn default_false() -> bool {
    false
}

/// The default max length of the input buffer.
///
/// Any input exceeding this limit will be discarded.
pub fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
}

pub fn default_framing_message_based() -> FramingConfig {
    BytesDecoderConfig::new().into()
}

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
                            .map(move |(nested_k, v)| (format!("{}.{}", k, nested_k), v)),
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
