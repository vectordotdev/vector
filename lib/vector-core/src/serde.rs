use std::{fmt, marker::PhantomData};

use serde::{Deserialize, Deserializer, Serialize, de};
use vector_config::Configurable;

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

/// One ASCII character, represented as a string in configuration and a byte in codecs.
#[derive(Clone, Copy, Configurable, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
#[configurable(metadata(docs::type_override = "ascii_char"))]
pub struct AsciiChar(#[configurable(validation(pattern = r"^[\x00-\x7F]$"))] char);

impl AsciiChar {
    /// Creates an ASCII character from a known character, including in constants.
    ///
    /// Use [`Self::try_from`] for untrusted input.
    ///
    /// # Panics
    ///
    /// Panics if `character` is not ASCII.
    #[must_use]
    pub const fn new(character: char) -> Self {
        assert!(character.is_ascii(), "expected an ASCII character");
        Self(character)
    }

    /// Returns the single byte representing this character.
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self.0 as u8
    }
}

impl TryFrom<char> for AsciiChar {
    type Error = String;

    fn try_from(character: char) -> Result<Self, Self::Error> {
        if character.is_ascii() {
            Ok(Self::new(character))
        } else {
            Err(format!(
                "invalid character: {character}, expected character in ASCII range"
            ))
        }
    }
}

impl<'de> Deserialize<'de> for AsciiChar {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(char::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use vector_config::{configurable_component, schema::generate_root_schema};

    use super::AsciiChar;

    /// Configuration with a default ASCII character.
    #[configurable_component]
    struct Config {
        /// The delimiter.
        #[serde(default = "default_delimiter")]
        delimiter: AsciiChar,
    }

    const fn default_delimiter() -> AsciiChar {
        AsciiChar::new(',')
    }

    #[test]
    fn ascii_character_config_contract() {
        // Exhaust the ASCII range, including NUL, digits, whitespace, and DEL.
        for byte in 0..=127 {
            let character = char::from(byte);
            let input = json!(character);
            let parsed: AsciiChar = serde_json::from_value(input.clone()).unwrap();
            assert_eq!(parsed.as_byte(), byte);
            assert_eq!(parsed, AsciiChar::try_from(character).unwrap());
            assert_eq!(serde_json::to_value(parsed).unwrap(), input);
        }

        for (name, input) in [
            ("non-ASCII", json!("é")),
            ("multibyte Unicode", json!("🦀")),
            ("empty", json!("")),
            ("multiple characters", json!("ab")),
            ("numeric digit", json!(1)),
            ("numeric byte", json!(44)),
            ("boolean", json!(true)),
            ("null", Value::Null),
            ("array", json!([","])),
            ("object", json!({})),
        ] {
            assert!(
                serde_json::from_value::<AsciiChar>(input).is_err(),
                "{name}"
            );
        }
        assert!(AsciiChar::try_from('é').is_err());
        assert_eq!(AsciiChar::default().as_byte(), 0);

        let config: Config = serde_json::from_value(json!({})).unwrap();
        assert_eq!(config.delimiter.as_byte(), b',');
        let schema = serde_json::to_value(generate_root_schema::<Config>().unwrap()).unwrap();
        let property = &schema["properties"]["delimiter"];
        assert_eq!(property["default"], ",");
        assert_eq!(property["type"], "string", "{schema}");
        assert_eq!(property["minLength"], 1);
        assert_eq!(property["maxLength"], 1);
        assert_eq!(property["pattern"], r"^[\x00-\x7F]$");
    }
}
