use std::{fmt, marker::PhantomData};

use serde::{de, Deserialize, Deserializer};

/// Answers "Is this value in it's default state?" which can be used to skip serializing the value.
#[inline]
pub fn is_default<E: Default + PartialEq>(e: &E) -> bool {
    e == &E::default()
}

/// Enables deserializing from a value that could be a bool or a struct.
///
/// Example:
/// healthcheck: bool
/// healthcheck.enabled: bool
/// Both are accepted.
///
/// # Errors
///
/// Returns the error from deserializing the underlying struct.
pub fn bool_or_struct<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + From<bool>,
    D: Deserializer<'de>,
{
    struct BoolOrStruct<T>(PhantomData<fn() -> T>);

    impl<'de, T> de::Visitor<'de> for BoolOrStruct<T>
    where
        T: Deserialize<'de> + From<bool>,
    {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("bool or map")
        }

        fn visit_bool<E>(self, value: bool) -> Result<T, E>
        where
            E: de::Error,
        {
            Ok(value.into())
        }

        fn visit_map<M>(self, map: M) -> Result<T, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(BoolOrStruct(PhantomData))
}

/// Handling of ASCII characters in `u8` fields via `serde`s `with` attribute.
///
/// ```rust
/// # use serde::{Deserialize, Serialize};
/// use vector_core::serde::ascii_char;
///
/// #[derive(Deserialize, Serialize)]
/// struct Foo {
///    #[serde(with = "ascii_char")]
///    character: u8,
/// }
/// ```
pub mod ascii_char {
    use serde::{de, Deserialize, Deserializer, Serializer};

    /// Deserialize an ASCII character as `u8`.
    ///
    /// # Errors
    ///
    /// If the item fails to be deserialized as a character, of the character to
    /// be deserialized is not part of the ASCII range, an error is returned.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<u8, D::Error>
    where
        D: Deserializer<'de>,
    {
        let character = char::deserialize(deserializer)?;
        if character.is_ascii() {
            Ok(character as u8)
        } else {
            Err(de::Error::custom(format!(
                "invalid character: {character}, expected character in ASCII range"
            )))
        }
    }

    /// Serialize an `u8` as ASCII character.
    ///
    /// # Errors
    ///
    /// Does not error.
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
            assert!(serde_json::from_str::<Foo>(r#"{ "character": "ß" }"#).is_err());
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
