use indexmap::map::IndexMap;
use serde::{Deserialize, Serialize};
pub use vector_core::serde::{bool_or_struct, skip_serializing_if_default};

#[cfg(feature = "codecs")]
use crate::codecs::{
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

/// Handling of ASCII characters in `u8` fields via `serde`s `with` attribute.
pub mod ascii_char {
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u8, D::Error>
    where
        D: Deserializer<'de>,
    {
        let character = char::deserialize(deserializer)?;
        if character.is_ascii() {
            Ok(character as u8)
        } else {
            Err(de::Error::custom(format!(
                "invalid character: {}, expected character in ASCII range",
                character
            )))
        }
    }

    pub fn serialize<S>(character: &u8, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_char(*character as char)
    }

    #[cfg(test)]
    mod tests {
        use serde::{Deserialize, Serialize};

        #[derive(Deserialize, Serialize)]
        struct Foo {
            #[serde(with = "super")]
            character: u8,
        }

        #[test]
        fn test_deserialize_ascii_valid() {
            let foo = serde_json::from_str::<Foo>(r#"{ "character": "\n" }"#).unwrap();
            assert_eq!(foo.character, b'\n');
        }

        #[test]
        fn test_deserialize_ascii_invalid_range() {
            assert!(serde_json::from_str::<Foo>(r#"{ "character": "💩" }"#).is_err());
        }

        #[test]
        fn test_deserialize_ascii_invalid_character() {
            assert!(serde_json::from_str::<Foo>(r#"{ "character": 0 }"#).is_err());
        }

        #[test]
        fn test_serialize_ascii() {
            let foo = Foo { character: b'\n' };
            assert_eq!(
                serde_json::to_string(&foo).unwrap(),
                r#"{"character":"\n"}"#
            );
        }
    }
}
