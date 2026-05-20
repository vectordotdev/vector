//! Configuration types for framing methods.

use bytes::BytesMut;
use vector_config::configurable_component;

use super::{
    BoxedFramer, BoxedFramingError, BytesEncoder, BytesEncoderConfig, CharacterDelimitedEncoder,
    CharacterDelimitedEncoderConfig, LengthDelimitedEncoder, LengthDelimitedEncoderConfig,
    NewlineDelimitedEncoder, NewlineDelimitedEncoderConfig, VarintLengthDelimitedEncoder,
    VarintLengthDelimitedEncoderConfig,
};

/// Framing configuration.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The framing method."))]
pub enum FramingConfig {
    /// Event data is not delimited at all.
    Bytes,

    /// Event data is delimited by a single ASCII (7-bit) character.
    CharacterDelimited(CharacterDelimitedEncoderConfig),

    /// Event data is prefixed with its length in bytes.
    ///
    /// The prefix is a 32-bit unsigned integer, little endian.
    LengthDelimited(LengthDelimitedEncoderConfig),

    /// Event data is delimited by a newline (LF) character.
    NewlineDelimited,

    /// Event data is prefixed with its length in bytes as a varint.
    ///
    /// This is compatible with protobuf's length-delimited encoding.
    VarintLengthDelimited(VarintLengthDelimitedEncoderConfig),
}

impl From<BytesEncoderConfig> for FramingConfig {
    fn from(_: BytesEncoderConfig) -> Self {
        Self::Bytes
    }
}

impl From<CharacterDelimitedEncoderConfig> for FramingConfig {
    fn from(config: CharacterDelimitedEncoderConfig) -> Self {
        Self::CharacterDelimited(config)
    }
}

impl From<LengthDelimitedEncoderConfig> for FramingConfig {
    fn from(config: LengthDelimitedEncoderConfig) -> Self {
        Self::LengthDelimited(config)
    }
}

impl From<NewlineDelimitedEncoderConfig> for FramingConfig {
    fn from(_: NewlineDelimitedEncoderConfig) -> Self {
        Self::NewlineDelimited
    }
}

impl From<VarintLengthDelimitedEncoderConfig> for FramingConfig {
    fn from(config: VarintLengthDelimitedEncoderConfig) -> Self {
        Self::VarintLengthDelimited(config)
    }
}

impl FramingConfig {
    /// Build the `Framer` from this configuration.
    pub fn build(&self) -> Framer {
        match self {
            FramingConfig::Bytes => Framer::Bytes(BytesEncoderConfig.build()),
            FramingConfig::CharacterDelimited(config) => Framer::CharacterDelimited(config.build()),
            FramingConfig::LengthDelimited(config) => Framer::LengthDelimited(config.build()),
            FramingConfig::NewlineDelimited => {
                Framer::NewlineDelimited(NewlineDelimitedEncoderConfig.build())
            }
            FramingConfig::VarintLengthDelimited(config) => {
                Framer::VarintLengthDelimited(config.build())
            }
        }
    }
}

/// Produce a byte stream from byte frames.
#[derive(Debug, Clone)]
pub enum Framer {
    /// Uses a `BytesEncoder` for framing.
    Bytes(BytesEncoder),
    /// Uses a `CharacterDelimitedEncoder` for framing.
    CharacterDelimited(CharacterDelimitedEncoder),
    /// Uses a `LengthDelimitedEncoder` for framing.
    LengthDelimited(LengthDelimitedEncoder),
    /// Uses a `NewlineDelimitedEncoder` for framing.
    NewlineDelimited(NewlineDelimitedEncoder),
    /// Uses a `VarintLengthDelimitedEncoder` for framing.
    VarintLengthDelimited(VarintLengthDelimitedEncoder),
    /// Uses an opaque `Encoder` implementation for framing.
    Boxed(BoxedFramer),
}

impl From<BytesEncoder> for Framer {
    fn from(encoder: BytesEncoder) -> Self {
        Self::Bytes(encoder)
    }
}

impl From<CharacterDelimitedEncoder> for Framer {
    fn from(encoder: CharacterDelimitedEncoder) -> Self {
        Self::CharacterDelimited(encoder)
    }
}

impl From<LengthDelimitedEncoder> for Framer {
    fn from(encoder: LengthDelimitedEncoder) -> Self {
        Self::LengthDelimited(encoder)
    }
}

impl From<NewlineDelimitedEncoder> for Framer {
    fn from(encoder: NewlineDelimitedEncoder) -> Self {
        Self::NewlineDelimited(encoder)
    }
}

impl From<VarintLengthDelimitedEncoder> for Framer {
    fn from(encoder: VarintLengthDelimitedEncoder) -> Self {
        Self::VarintLengthDelimited(encoder)
    }
}

impl From<BoxedFramer> for Framer {
    fn from(encoder: BoxedFramer) -> Self {
        Self::Boxed(encoder)
    }
}

impl tokio_util::codec::Encoder<()> for Framer {
    type Error = BoxedFramingError;

    fn encode(&mut self, _: (), buffer: &mut BytesMut) -> Result<(), Self::Error> {
        match self {
            Framer::Bytes(framer) => framer.encode((), buffer),
            Framer::CharacterDelimited(framer) => framer.encode((), buffer),
            Framer::LengthDelimited(framer) => framer.encode((), buffer),
            Framer::NewlineDelimited(framer) => framer.encode((), buffer),
            Framer::VarintLengthDelimited(framer) => framer.encode((), buffer),
            Framer::Boxed(framer) => framer.encode((), buffer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_framing_config_build() {
        // Test that FramingConfig can be built to create Framers
        let config = FramingConfig::NewlineDelimited;
        let framer = config.build();
        assert!(matches!(framer, Framer::NewlineDelimited(_)));

        let config = FramingConfig::Bytes;
        let framer = config.build();
        assert!(matches!(framer, Framer::Bytes(_)));
    }

    #[test]
    fn test_framing_config_from_encoder_config() {
        // Test that FramingConfig can be created from encoder configs
        let bytes_config = BytesEncoderConfig;
        let framing_config: FramingConfig = bytes_config.into();
        assert!(matches!(framing_config, FramingConfig::Bytes));

        let newline_config = NewlineDelimitedEncoderConfig;
        let framing_config: FramingConfig = newline_config.into();
        assert!(matches!(framing_config, FramingConfig::NewlineDelimited));
    }

    #[test]
    fn test_framer_from_encoder() {
        // Test that Framer can be created from encoders
        let bytes_encoder = BytesEncoderConfig.build();
        let framer: Framer = bytes_encoder.into();
        assert!(matches!(framer, Framer::Bytes(_)));

        let newline_encoder = NewlineDelimitedEncoderConfig.build();
        let framer: Framer = newline_encoder.into();
        assert!(matches!(framer, Framer::NewlineDelimited(_)));
    }

    #[test]
    fn test_framing_config_equality() {
        // Test that FramingConfig can be compared for equality
        let config1 = FramingConfig::NewlineDelimited;
        let config2 = FramingConfig::NewlineDelimited;
        assert_eq!(config1, config2);

        let config3 = FramingConfig::Bytes;
        assert_ne!(config1, config3);
    }

    #[test]
    fn test_framing_config_clone() {
        // Test that FramingConfig can be cloned
        let config = FramingConfig::LengthDelimited(LengthDelimitedEncoderConfig::default());
        let cloned = config.clone();
        assert_eq!(config, cloned);
    }
}
