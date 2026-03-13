#![allow(missing_docs)]
use indexmap::map::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, de};
pub use vector_lib::serde::{bool_or_struct, is_default};
use vector_lib::{
    codecs::{
        BytesDecoderConfig, BytesDeserializerConfig,
        decoding::{DeserializerConfig, FramingConfig},
    },
    configurable::configurable_component,
};

use crate::sinks::util::buffer::compression::Compression;

/// Enables deserializing compression from a bool (legacy) or Compression enum (new).
///
/// For backward compatibility:
/// - `true` maps to `Compression::gzip_default()`
/// - `false` maps to `Compression::None`
///
/// New syntax:
/// - `"none"`, `"gzip"`, `"zstd"` as strings
/// - `{ algorithm: "gzip", level: 6 }` as objects
///
/// # Errors
///
/// Returns the error from deserializing the underlying Compression type.
pub fn bool_or_compression<'de, D>(deserializer: D) -> Result<Compression, D::Error>
where
    D: Deserializer<'de>,
{
    struct BoolOrCompression;

    impl<'de> de::Visitor<'de> for BoolOrCompression {
        type Value = Compression;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("boolean (deprecated), string, or compression configuration object")
        }

        fn visit_bool<E>(self, value: bool) -> Result<Compression, E>
        where
            E: de::Error,
        {
            if value {
                Ok(Compression::gzip_default())
            } else {
                Ok(Compression::None)
            }
        }

        fn visit_str<E>(self, value: &str) -> Result<Compression, E>
        where
            E: de::Error,
        {
            Compression::deserialize(de::value::StrDeserializer::new(value))
        }

        fn visit_map<M>(self, map: M) -> Result<Compression, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            Compression::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(BoolOrCompression)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::util::buffer::compression::{Compression, CompressionLevel};
    use serde::Deserialize;

    // Test struct that uses the bool_or_compression deserializer
    #[derive(Deserialize)]
    struct TestConfig {
        #[serde(deserialize_with = "bool_or_compression")]
        compression: Compression,
    }

    #[test]
    fn test_bool_or_compression_legacy_true() {
        let json = r#"{"compression": true}"#;
        let result: TestConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(result.compression, Compression::Gzip(_)));
    }

    #[test]
    fn test_bool_or_compression_legacy_false() {
        let json = r#"{"compression": false}"#;
        let result: TestConfig = serde_json::from_str(json).unwrap();
        assert_eq!(result.compression, Compression::None);
    }

    #[test]
    fn test_bool_or_compression_string_gzip() {
        let json = r#"{"compression": "gzip"}"#;
        let result: TestConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(result.compression, Compression::Gzip(_)));
    }

    #[test]
    fn test_bool_or_compression_string_zstd() {
        let json = r#"{"compression": "zstd"}"#;
        let result: TestConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(result.compression, Compression::Zstd(_)));
    }

    #[test]
    fn test_bool_or_compression_string_none() {
        let json = r#"{"compression": "none"}"#;
        let result: TestConfig = serde_json::from_str(json).unwrap();
        assert_eq!(result.compression, Compression::None);
    }

    #[test]
    fn test_bool_or_compression_object_with_level() {
        let json = r#"{"compression": {"algorithm": "zstd", "level": 3}}"#;
        let result: TestConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(
            result.compression,
            Compression::Zstd(CompressionLevel::Val(3))
        ));
    }
}
