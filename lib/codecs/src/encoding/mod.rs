//! A collection of support structures that are used in the process of encoding
//! events into bytes.

pub mod format;
pub mod framing;

pub use format::{
    BoxedSerializer, JsonSerializer, JsonSerializerConfig, NativeJsonSerializer,
    NativeJsonSerializerConfig, NativeSerializer, NativeSerializerConfig, RawMessageSerializer,
    RawMessageSerializerConfig,
};
pub use framing::{
    BoxedFramer, BoxedFramingError, BytesEncoder, BytesEncoderConfig, CharacterDelimitedEncoder,
    CharacterDelimitedEncoderConfig, CharacterDelimitedEncoderOptions, NewlineDelimitedEncoder,
    NewlineDelimitedEncoderConfig,
};

use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use vector_core::{event::Event, schema};

/// An error that occurred while encoding structured events into byte frames.
#[derive(Debug)]
pub enum Error {
    /// The error occurred while encoding the byte frame boundaries.
    FramingError(BoxedFramingError),
    /// The error occurred while serializing a structured event into bytes.
    SerializingError(vector_core::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FramingError(error) => write!(formatter, "FramingError({})", error),
            Self::SerializingError(error) => write!(formatter, "SerializingError({})", error),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::FramingError(Box::new(error))
    }
}

/// Configuration for building a `Framer`.
// Unfortunately, copying options of the nested enum variants is necessary
// since `serde` doesn't allow `flatten`ing these:
// https://github.com/serde-rs/serde/issues/1402.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum FramingConfig {
    /// Configures the `BytesEncoder`.
    Bytes,
    /// Configures the `CharacterDelimitedEncoder`.
    CharacterDelimited {
        /// Options for the character delimited encoder.
        character_delimited: CharacterDelimitedEncoderOptions,
    },
    /// Configures the `NewlineDelimitedEncoder`.
    NewlineDelimited,
}

impl From<CharacterDelimitedEncoderConfig> for FramingConfig {
    fn from(config: CharacterDelimitedEncoderConfig) -> Self {
        Self::CharacterDelimited {
            character_delimited: config.character_delimited,
        }
    }
}

impl From<NewlineDelimitedEncoderConfig> for FramingConfig {
    fn from(_: NewlineDelimitedEncoderConfig) -> Self {
        Self::NewlineDelimited
    }
}

impl FramingConfig {
    /// Build the `Framer` from this configuration.
    pub const fn build(self) -> Framer {
        match self {
            FramingConfig::Bytes => Framer::Bytes(BytesEncoderConfig.build()),
            FramingConfig::CharacterDelimited {
                character_delimited,
            } => Framer::CharacterDelimited(
                CharacterDelimitedEncoderConfig {
                    character_delimited,
                }
                .build(),
            ),
            FramingConfig::NewlineDelimited => {
                Framer::NewlineDelimited(NewlineDelimitedEncoderConfig.build())
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
    /// Uses a `NewlineDelimitedEncoder` for framing.
    NewlineDelimited(NewlineDelimitedEncoder),
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

impl From<NewlineDelimitedEncoder> for Framer {
    fn from(encoder: NewlineDelimitedEncoder) -> Self {
        Self::NewlineDelimited(encoder)
    }
}

impl From<BoxedFramer> for Framer {
    fn from(encoder: BoxedFramer) -> Self {
        Self::Boxed(encoder)
    }
}

impl tokio_util::codec::Encoder<()> for Framer {
    type Error = BoxedFramingError;

    fn encode(&mut self, _: (), dst: &mut BytesMut) -> Result<(), Self::Error> {
        match self {
            Framer::Bytes(framer) => framer.encode((), dst),
            Framer::CharacterDelimited(framer) => framer.encode((), dst),
            Framer::NewlineDelimited(framer) => framer.encode((), dst),
            Framer::Boxed(framer) => framer.encode((), dst),
        }
    }
}

/// Configuration for building a `Serializer`.
// Unfortunately, copying options of the nested enum variants is necessary
// since `serde` doesn't allow `flatten`ing these:
// https://github.com/serde-rs/serde/issues/1402.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "codec", rename_all = "snake_case")]
pub enum SerializerConfig {
    /// Configures the `JsonSerializer`.
    Json,
    /// Configures the `NativeSerializer`.
    Native,
    /// Configures the `NativeJsonSerializer`.
    NativeJson,
    /// Configures the `RawMessageSerializer`.
    RawMessage,
}

impl From<JsonSerializerConfig> for SerializerConfig {
    fn from(_: JsonSerializerConfig) -> Self {
        Self::Json
    }
}

impl From<RawMessageSerializerConfig> for SerializerConfig {
    fn from(_: RawMessageSerializerConfig) -> Self {
        Self::RawMessage
    }
}

impl SerializerConfig {
    /// Build the `Serializer` from this configuration.
    pub const fn build(&self) -> Serializer {
        match self {
            SerializerConfig::Json => Serializer::Json(JsonSerializerConfig.build()),
            SerializerConfig::Native => Serializer::Native(NativeSerializerConfig.build()),
            SerializerConfig::NativeJson => {
                Serializer::NativeJson(NativeJsonSerializerConfig.build())
            }
            SerializerConfig::RawMessage => {
                Serializer::RawMessage(RawMessageSerializerConfig.build())
            }
        }
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        match self {
            SerializerConfig::Json => JsonSerializerConfig.schema_requirement(),
            SerializerConfig::Native => NativeSerializerConfig.schema_requirement(),
            SerializerConfig::NativeJson => NativeJsonSerializerConfig.schema_requirement(),
            SerializerConfig::RawMessage => RawMessageSerializerConfig.schema_requirement(),
        }
    }
}

/// Serialize structured events as bytes.
#[derive(Debug, Clone)]
pub enum Serializer {
    /// Uses a `JsonSerializer` for serialization.
    Json(JsonSerializer),
    /// Uses a `NativeSerializer` for serialization.
    Native(NativeSerializer),
    /// Uses a `NativeJsonSerializer` for serialization.
    NativeJson(NativeJsonSerializer),
    /// Uses a `RawMessageSerializer` for serialization.
    RawMessage(RawMessageSerializer),
}

impl tokio_util::codec::Encoder<Event> for Serializer {
    type Error = vector_core::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        match self {
            Serializer::Json(serializer) => serializer.encode(event, buffer),
            Serializer::Native(serializer) => serializer.encode(event, buffer),
            Serializer::NativeJson(serializer) => serializer.encode(event, buffer),
            Serializer::RawMessage(serializer) => serializer.encode(event, buffer),
        }
    }
}
