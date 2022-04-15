use indexmap::map::IndexMap;
use serde::{Deserialize, Serialize};
pub use vector_core::serde::{bool_or_struct, skip_serializing_if_default};

#[cfg(feature = "codecs")]
use codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    BytesDecoderConfig, BytesDeserializerConfig, NewlineDelimitedDecoderConfig,
};

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

#[cfg(feature = "codecs")]
pub fn default_framing_message_based() -> FramingConfig {
    BytesDecoderConfig::new().into()
}

#[cfg(feature = "codecs")]
pub fn default_framing_stream_based() -> FramingConfig {
    NewlineDelimitedDecoderConfig::new().into()
}

#[cfg(feature = "codecs")]
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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum FieldsOrValue<V> {
    Fields(Fields<V>),
    Value(V),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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

/// Structure to handle when a configuration field can be a value
/// or a list of values.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T: ToString> OneOrMany<T> {
    pub fn stringify(&self) -> OneOrMany<String> {
        match self {
            Self::One(value) => value.to_string().into(),
            Self::Many(values) => values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .into(),
        }
    }
}

impl<T> OneOrMany<T> {
    pub fn into_vec(self) -> Vec<T> {
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
