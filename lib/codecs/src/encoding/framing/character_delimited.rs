use bytes::{BufMut, BytesMut};
use serde_with::serde_as;
use tokio_util::codec::Encoder;
use vector_config::configurable_component;

use super::BoxedFramingError;

/// Config used to build a `CharacterDelimitedEncoder`.
#[configurable_component]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CharacterDelimitedEncoderConfig {
    /// Options for the character delimited encoder.
    pub character_delimited: CharacterDelimitedEncoderOptions,
}

impl CharacterDelimitedEncoderConfig {
    /// Creates a `CharacterDelimitedEncoderConfig` with the specified delimiter.
    #[must_use]
    pub const fn new(delimiter: u8) -> Self {
        Self {
            character_delimited: CharacterDelimitedEncoderOptions { delimiter },
        }
    }

    /// Build the `CharacterDelimitedEncoder` from this configuration.
    #[must_use]
    pub const fn build(&self) -> CharacterDelimitedEncoder {
        CharacterDelimitedEncoder::new(self.character_delimited.delimiter)
    }
}

/// Configuration for character-delimited framing.
#[serde_as]
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterDelimitedEncoderOptions {
    /// The ASCII (7-bit) character that delimits byte sequences.
    #[configurable(metadata(docs::type_override = "ascii_char"))]
    #[serde_as(as = "vector_core::serde::ascii_char::AsciiChar")]
    pub delimiter: u8,
}

/// An encoder for handling bytes that are delimited by (a) chosen character(s).
#[derive(Debug, Clone)]
pub struct CharacterDelimitedEncoder {
    /// The character that delimits byte sequences.
    pub delimiter: u8,
}

impl CharacterDelimitedEncoder {
    /// Creates a `CharacterDelimitedEncoder` with the specified delimiter.
    #[must_use]
    pub const fn new(delimiter: u8) -> Self {
        Self { delimiter }
    }
}

impl Encoder<()> for CharacterDelimitedEncoder {
    type Error = BoxedFramingError;

    fn encode(&mut self, (): (), buffer: &mut BytesMut) -> Result<(), BoxedFramingError> {
        buffer.put_u8(self.delimiter);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delimiter_schema_matches_its_ascii_string_representation() {
        let schema = serde_json::to_value(
            vector_config::schema::generate_root_schema::<CharacterDelimitedEncoderOptions>()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(schema["properties"]["delimiter"]["type"], "string");

        let input = serde_json::json!({"delimiter": "1"});
        let options: CharacterDelimitedEncoderOptions =
            serde_json::from_value(input.clone()).unwrap();
        assert_eq!(options.delimiter, b'1');
        assert_eq!(serde_json::to_value(options).unwrap(), input);
        assert!(
            serde_json::from_value::<CharacterDelimitedEncoderOptions>(
                serde_json::json!({"delimiter": 1})
            )
            .is_err()
        );
    }

    #[test]
    fn encode() {
        let mut codec = CharacterDelimitedEncoder::new(b'\n');

        let mut buffer = BytesMut::from("abc");
        codec.encode((), &mut buffer).unwrap();

        assert_eq!(b"abc\n", &buffer[..]);
    }
}
