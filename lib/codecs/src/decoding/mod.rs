//! A collection of support structures that are used in the process of decoding
//! bytes into events.

mod error;
pub mod format;
pub mod framing;

use bytes::{Bytes, BytesMut};
pub use error::StreamDecodingError;
pub use format::{
    BoxedDeserializer, BytesDeserializer, BytesDeserializerConfig, GelfDeserializer,
    GelfDeserializerConfig, JsonDeserializer, JsonDeserializerConfig, NativeDeserializer,
    NativeDeserializerConfig, NativeJsonDeserializer, NativeJsonDeserializerConfig,
};
#[cfg(feature = "syslog")]
pub use format::{SyslogDeserializer, SyslogDeserializerConfig};
pub use framing::{
    BoxedFramer, BoxedFramingError, BytesDecoder, BytesDecoderConfig, CharacterDelimitedDecoder,
    CharacterDelimitedDecoderConfig, CharacterDelimitedDecoderOptions, FramingError,
    LengthDelimitedDecoder, LengthDelimitedDecoderConfig, NewlineDelimitedDecoder,
    NewlineDelimitedDecoderConfig, NewlineDelimitedDecoderOptions, OctetCountingDecoder,
    OctetCountingDecoderConfig, OctetCountingDecoderOptions,
};
use smallvec::SmallVec;
use std::fmt::Debug;
use vector_config::configurable_component;
use vector_core::{
    config::{DataType, LogNamespace},
    event::Event,
    schema,
};

/// An error that occurred while decoding structured events from a byte stream /
/// byte messages.
#[derive(Debug)]
pub enum Error {
    /// The error occurred while producing byte frames from the byte stream /
    /// byte messages.
    FramingError(BoxedFramingError),
    /// The error occurred while parsing structured events from a byte frame.
    ParsingError(vector_common::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FramingError(error) => write!(formatter, "FramingError({})", error),
            Self::ParsingError(error) => write!(formatter, "ParsingError({})", error),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::FramingError(Box::new(error))
    }
}

impl StreamDecodingError for Error {
    fn can_continue(&self) -> bool {
        match self {
            Self::FramingError(error) => error.can_continue(),
            Self::ParsingError(_) => true,
        }
    }
}

/// Framing configuration.
///
/// Framing handles how events are separated when encoded in a raw byte form, where each event is
/// a frame that must be prefixed, or delimited, in a way that marks where an event begins and
/// ends within the byte stream.
// Unfortunately, copying options of the nested enum variants is necessary
// since `serde` doesn't allow `flatten`ing these:
// https://github.com/serde-rs/serde/issues/1402.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "method", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The framing method."))]
pub enum FramingConfig {
    /// Byte frames are passed through as-is according to the underlying I/O boundaries (for example, split between messages or stream segments).
    Bytes,

    /// Byte frames which are delimited by a chosen character.
    CharacterDelimited {
        /// Options for the character delimited decoder.
        character_delimited: CharacterDelimitedDecoderOptions,
    },

    /// Byte frames which are prefixed by an unsigned big-endian 32-bit integer indicating the length.
    LengthDelimited,

    /// Byte frames which are delimited by a newline character.
    NewlineDelimited {
        #[serde(
            default,
            skip_serializing_if = "vector_core::serde::skip_serializing_if_default"
        )]
        /// Options for the newline delimited decoder.
        newline_delimited: NewlineDelimitedDecoderOptions,
    },

    /// Byte frames according to the [octet counting][octet_counting] format.
    ///
    /// [octet_counting]: https://tools.ietf.org/html/rfc6587#section-3.4.1
    OctetCounting {
        #[serde(
            default,
            skip_serializing_if = "vector_core::serde::skip_serializing_if_default"
        )]
        /// Options for the octet counting decoder.
        octet_counting: OctetCountingDecoderOptions,
    },
}

impl From<BytesDecoderConfig> for FramingConfig {
    fn from(_: BytesDecoderConfig) -> Self {
        Self::Bytes
    }
}

impl From<CharacterDelimitedDecoderConfig> for FramingConfig {
    fn from(config: CharacterDelimitedDecoderConfig) -> Self {
        Self::CharacterDelimited {
            character_delimited: config.character_delimited,
        }
    }
}

impl From<LengthDelimitedDecoderConfig> for FramingConfig {
    fn from(_: LengthDelimitedDecoderConfig) -> Self {
        Self::LengthDelimited
    }
}

impl From<NewlineDelimitedDecoderConfig> for FramingConfig {
    fn from(config: NewlineDelimitedDecoderConfig) -> Self {
        Self::NewlineDelimited {
            newline_delimited: config.newline_delimited,
        }
    }
}

impl From<OctetCountingDecoderConfig> for FramingConfig {
    fn from(config: OctetCountingDecoderConfig) -> Self {
        Self::OctetCounting {
            octet_counting: config.octet_counting,
        }
    }
}

impl FramingConfig {
    /// Build the `Framer` from this configuration.
    pub fn build(&self) -> Framer {
        match self {
            FramingConfig::Bytes => Framer::Bytes(BytesDecoderConfig.build()),
            FramingConfig::CharacterDelimited {
                character_delimited,
            } => Framer::CharacterDelimited(
                CharacterDelimitedDecoderConfig {
                    character_delimited: character_delimited.clone(),
                }
                .build(),
            ),
            FramingConfig::LengthDelimited => {
                Framer::LengthDelimited(LengthDelimitedDecoderConfig.build())
            }
            FramingConfig::NewlineDelimited { newline_delimited } => Framer::NewlineDelimited(
                NewlineDelimitedDecoderConfig {
                    newline_delimited: newline_delimited.clone(),
                }
                .build(),
            ),
            FramingConfig::OctetCounting { octet_counting } => Framer::OctetCounting(
                OctetCountingDecoderConfig {
                    octet_counting: octet_counting.clone(),
                }
                .build(),
            ),
        }
    }
}

/// Produce byte frames from a byte stream / byte message.
#[derive(Debug, Clone)]
pub enum Framer {
    /// Uses a `BytesDecoder` for framing.
    Bytes(BytesDecoder),
    /// Uses a `CharacterDelimitedDecoder` for framing.
    CharacterDelimited(CharacterDelimitedDecoder),
    /// Uses a `LengthDelimitedDecoder` for framing.
    LengthDelimited(LengthDelimitedDecoder),
    /// Uses a `NewlineDelimitedDecoder` for framing.
    NewlineDelimited(NewlineDelimitedDecoder),
    /// Uses a `OctetCountingDecoder` for framing.
    OctetCounting(OctetCountingDecoder),
    /// Uses an opaque `Framer` implementation for framing.
    Boxed(BoxedFramer),
}

impl tokio_util::codec::Decoder for Framer {
    type Item = Bytes;
    type Error = BoxedFramingError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self {
            Framer::Bytes(framer) => framer.decode(src),
            Framer::CharacterDelimited(framer) => framer.decode(src),
            Framer::LengthDelimited(framer) => framer.decode(src),
            Framer::NewlineDelimited(framer) => framer.decode(src),
            Framer::OctetCounting(framer) => framer.decode(src),
            Framer::Boxed(framer) => framer.decode(src),
        }
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self {
            Framer::Bytes(framer) => framer.decode_eof(src),
            Framer::CharacterDelimited(framer) => framer.decode_eof(src),
            Framer::LengthDelimited(framer) => framer.decode_eof(src),
            Framer::NewlineDelimited(framer) => framer.decode_eof(src),
            Framer::OctetCounting(framer) => framer.decode_eof(src),
            Framer::Boxed(framer) => framer.decode_eof(src),
        }
    }
}

/// Deserializer configuration.
// Unfortunately, copying options of the nested enum variants is necessary
// since `serde` doesn't allow `flatten`ing these:
// https://github.com/serde-rs/serde/issues/1402.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "codec", rename_all = "snake_case")]
#[configurable(description = "Configures how events are decoded from raw bytes.")]
#[configurable(metadata(docs::enum_tag_description = "The codec to use for decoding events."))]
pub enum DeserializerConfig {
    /// Uses the raw bytes as-is.
    Bytes,

    /// Decodes the raw bytes as [JSON][json].
    ///
    /// [json]: https://www.json.org/
    Json,

    #[cfg(feature = "syslog")]
    /// Decodes the raw bytes as a Syslog message.
    ///
    /// Decodes either as the [RFC 3164][rfc3164]-style format ("old" style) or the
    /// [RFC 5424][rfc5424]-style format ("new" style, includes structured data).
    ///
    /// [rfc3164]: https://www.ietf.org/rfc/rfc3164.txt
    /// [rfc5424]: https://www.ietf.org/rfc/rfc5424.txt
    Syslog,

    /// Decodes the raw bytes as Vector’s [native Protocol Buffers format][vector_native_protobuf].
    ///
    /// This codec is **[experimental][experimental]**.
    ///
    /// [vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
    /// [experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
    Native,

    /// Decodes the raw bytes as Vector’s [native JSON format][vector_native_json].
    ///
    /// This codec is **[experimental][experimental]**.
    ///
    /// [vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
    /// [experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
    NativeJson,

    /// Decodes the raw bytes as a [GELF][gelf] message.
    ///
    /// [gelf]: https://docs.graylog.org/docs/gelf
    Gelf,
}

impl From<BytesDeserializerConfig> for DeserializerConfig {
    fn from(_: BytesDeserializerConfig) -> Self {
        Self::Bytes
    }
}

impl From<JsonDeserializerConfig> for DeserializerConfig {
    fn from(_: JsonDeserializerConfig) -> Self {
        Self::Json
    }
}

#[cfg(feature = "syslog")]
impl From<SyslogDeserializerConfig> for DeserializerConfig {
    fn from(_: SyslogDeserializerConfig) -> Self {
        Self::Syslog
    }
}

impl From<GelfDeserializerConfig> for DeserializerConfig {
    fn from(_: GelfDeserializerConfig) -> Self {
        Self::Gelf
    }
}

impl DeserializerConfig {
    /// Build the `Deserializer` from this configuration.
    pub fn build(&self) -> Deserializer {
        match self {
            DeserializerConfig::Bytes => Deserializer::Bytes(BytesDeserializerConfig.build()),
            DeserializerConfig::Json => Deserializer::Json(JsonDeserializerConfig.build()),
            #[cfg(feature = "syslog")]
            DeserializerConfig::Syslog => {
                Deserializer::Syslog(SyslogDeserializerConfig::default().build())
            }
            DeserializerConfig::Native => Deserializer::Native(NativeDeserializerConfig.build()),
            DeserializerConfig::NativeJson => {
                Deserializer::NativeJson(NativeJsonDeserializerConfig.build())
            }
            DeserializerConfig::Gelf => Deserializer::Gelf(GelfDeserializerConfig.build()),
        }
    }

    /// Return an appropriate default framer for the given deserializer
    pub fn default_stream_framing(&self) -> FramingConfig {
        match self {
            DeserializerConfig::Native => FramingConfig::LengthDelimited,
            DeserializerConfig::Bytes
            | DeserializerConfig::Json
            | DeserializerConfig::Gelf
            | DeserializerConfig::NativeJson => FramingConfig::NewlineDelimited {
                newline_delimited: Default::default(),
            },
            #[cfg(feature = "syslog")]
            DeserializerConfig::Syslog => FramingConfig::NewlineDelimited {
                newline_delimited: Default::default(),
            },
        }
    }

    /// Return the type of event build by this deserializer.
    pub fn output_type(&self) -> DataType {
        match self {
            DeserializerConfig::Bytes => BytesDeserializerConfig.output_type(),
            DeserializerConfig::Json => JsonDeserializerConfig.output_type(),
            #[cfg(feature = "syslog")]
            DeserializerConfig::Syslog => SyslogDeserializerConfig::default().output_type(),
            DeserializerConfig::Native => NativeDeserializerConfig.output_type(),
            DeserializerConfig::NativeJson => NativeJsonDeserializerConfig.output_type(),
            DeserializerConfig::Gelf => GelfDeserializerConfig.output_type(),
        }
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match self {
            DeserializerConfig::Bytes => BytesDeserializerConfig.schema_definition(log_namespace),
            DeserializerConfig::Json => JsonDeserializerConfig.schema_definition(log_namespace),
            #[cfg(feature = "syslog")]
            DeserializerConfig::Syslog => {
                SyslogDeserializerConfig::default().schema_definition(log_namespace)
            }
            DeserializerConfig::Native => NativeDeserializerConfig.schema_definition(log_namespace),
            DeserializerConfig::NativeJson => {
                NativeJsonDeserializerConfig.schema_definition(log_namespace)
            }
            DeserializerConfig::Gelf => GelfDeserializerConfig.schema_definition(log_namespace),
        }
    }

    /// Get the HTTP content type.
    pub const fn content_type(&self, framer: &FramingConfig) -> &'static str {
        match (&self, framer) {
            (
                DeserializerConfig::Json | DeserializerConfig::NativeJson,
                FramingConfig::NewlineDelimited { .. },
            ) => "application/x-ndjson",
            (
                DeserializerConfig::Gelf
                | DeserializerConfig::Json
                | DeserializerConfig::NativeJson,
                FramingConfig::CharacterDelimited {
                    character_delimited:
                        CharacterDelimitedDecoderOptions {
                            delimiter: b',',
                            max_length: Some(usize::MAX),
                        },
                },
            ) => "application/json",
            (DeserializerConfig::Native, _) => "application/octet-stream",
            (
                DeserializerConfig::Json
                | DeserializerConfig::NativeJson
                | DeserializerConfig::Bytes
                | DeserializerConfig::Gelf,
                _,
            ) => "text/plain",
            #[cfg(feature = "syslog")]
            (DeserializerConfig::Syslog, _) => "text/plain",
        }
    }
}

/// Parse structured events from bytes.
#[derive(Clone)]
pub enum Deserializer {
    /// Uses a `BytesDeserializer` for deserialization.
    Bytes(BytesDeserializer),
    /// Uses a `JsonDeserializer` for deserialization.
    Json(JsonDeserializer),
    #[cfg(feature = "syslog")]
    /// Uses a `SyslogDeserializer` for deserialization.
    Syslog(SyslogDeserializer),
    /// Uses a `NativeDeserializer` for deserialization.
    Native(NativeDeserializer),
    /// Uses a `NativeDeserializer` for deserialization.
    NativeJson(NativeJsonDeserializer),
    /// Uses an opaque `Deserializer` implementation for deserialization.
    Boxed(BoxedDeserializer),
    /// Uses a `GelfDeserializer` for deserialization.
    Gelf(GelfDeserializer),
}

impl format::Deserializer for Deserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        match self {
            Deserializer::Bytes(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::Json(deserializer) => deserializer.parse(bytes, log_namespace),
            #[cfg(feature = "syslog")]
            Deserializer::Syslog(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::Native(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::NativeJson(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::Boxed(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::Gelf(deserializer) => deserializer.parse(bytes, log_namespace),
        }
    }
}
