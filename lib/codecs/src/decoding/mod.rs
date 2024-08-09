//! A collection of support structures that are used in the process of decoding
//! bytes into events.

mod error;
pub mod format;
pub mod framing;

use crate::decoding::format::{VrlDeserializer, VrlDeserializerConfig};
use bytes::{Bytes, BytesMut};
pub use error::StreamDecodingError;
pub use format::{
    BoxedDeserializer, BytesDeserializer, BytesDeserializerConfig, GelfDeserializer,
    GelfDeserializerConfig, GelfDeserializerOptions, InfluxdbDeserializer,
    InfluxdbDeserializerConfig, JsonDeserializer, JsonDeserializerConfig, JsonDeserializerOptions,
    NativeDeserializer, NativeDeserializerConfig, NativeJsonDeserializer,
    NativeJsonDeserializerConfig, NativeJsonDeserializerOptions, ProtobufDeserializer,
    ProtobufDeserializerConfig, ProtobufDeserializerOptions,
};
#[cfg(feature = "syslog")]
pub use format::{SyslogDeserializer, SyslogDeserializerConfig, SyslogDeserializerOptions};
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

use self::format::{AvroDeserializer, AvroDeserializerConfig, AvroDeserializerOptions};

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
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "method", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The framing method."))]
pub enum FramingConfig {
    /// Byte frames are passed through as-is according to the underlying I/O boundaries (for example, split between messages or stream segments).
    Bytes,

    /// Byte frames which are delimited by a chosen character.
    CharacterDelimited(CharacterDelimitedDecoderConfig),

    /// Byte frames which are prefixed by an unsigned big-endian 32-bit integer indicating the length.
    LengthDelimited(LengthDelimitedDecoderConfig),

    /// Byte frames which are delimited by a newline character.
    NewlineDelimited(NewlineDelimitedDecoderConfig),

    /// Byte frames according to the [octet counting][octet_counting] format.
    ///
    /// [octet_counting]: https://tools.ietf.org/html/rfc6587#section-3.4.1
    OctetCounting(OctetCountingDecoderConfig),
}

impl From<BytesDecoderConfig> for FramingConfig {
    fn from(_: BytesDecoderConfig) -> Self {
        Self::Bytes
    }
}

impl From<CharacterDelimitedDecoderConfig> for FramingConfig {
    fn from(config: CharacterDelimitedDecoderConfig) -> Self {
        Self::CharacterDelimited(config)
    }
}

impl From<LengthDelimitedDecoderConfig> for FramingConfig {
    fn from(config: LengthDelimitedDecoderConfig) -> Self {
        Self::LengthDelimited(config)
    }
}

impl From<NewlineDelimitedDecoderConfig> for FramingConfig {
    fn from(config: NewlineDelimitedDecoderConfig) -> Self {
        Self::NewlineDelimited(config)
    }
}

impl From<OctetCountingDecoderConfig> for FramingConfig {
    fn from(config: OctetCountingDecoderConfig) -> Self {
        Self::OctetCounting(config)
    }
}

impl FramingConfig {
    /// Build the `Framer` from this configuration.
    pub fn build(&self) -> Framer {
        match self {
            FramingConfig::Bytes => Framer::Bytes(BytesDecoderConfig.build()),
            FramingConfig::CharacterDelimited(config) => Framer::CharacterDelimited(config.build()),
            FramingConfig::LengthDelimited(config) => Framer::LengthDelimited(config.build()),
            FramingConfig::NewlineDelimited(config) => Framer::NewlineDelimited(config.build()),
            FramingConfig::OctetCounting(config) => Framer::OctetCounting(config.build()),
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
    Json(JsonDeserializerConfig),

    /// Decodes the raw bytes as [protobuf][protobuf].
    ///
    /// [protobuf]: https://protobuf.dev/
    Protobuf(ProtobufDeserializerConfig),

    #[cfg(feature = "syslog")]
    /// Decodes the raw bytes as a Syslog message.
    ///
    /// Decodes either as the [RFC 3164][rfc3164]-style format ("old" style) or the
    /// [RFC 5424][rfc5424]-style format ("new" style, includes structured data).
    ///
    /// [rfc3164]: https://www.ietf.org/rfc/rfc3164.txt
    /// [rfc5424]: https://www.ietf.org/rfc/rfc5424.txt
    Syslog(SyslogDeserializerConfig),

    /// Decodes the raw bytes as [native Protocol Buffers format][vector_native_protobuf].
    ///
    /// This codec is **[experimental][experimental]**.
    ///
    /// [vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
    /// [experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
    Native,

    /// Decodes the raw bytes as [native JSON format][vector_native_json].
    ///
    /// This codec is **[experimental][experimental]**.
    ///
    /// [vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
    /// [experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
    NativeJson(NativeJsonDeserializerConfig),

    /// Decodes the raw bytes as a [GELF][gelf] message.
    ///
    /// This codec is experimental for the following reason:
    ///
    /// The GELF specification is more strict than the actual Graylog receiver.
    /// Vector's decoder currently adheres more strictly to the GELF spec, with
    /// the exception that some characters such as `@`  are allowed in field names.
    ///
    /// Other GELF codecs such as Loki's, use a [Go SDK][implementation] that is maintained
    /// by Graylog, and is much more relaxed than the GELF spec.
    ///
    /// Going forward, Vector will use that [Go SDK][implementation] as the reference implementation, which means
    /// the codec may continue to relax the enforcement of specification.

    ///
    /// [gelf]: https://docs.graylog.org/docs/gelf
    /// [implementation]: https://github.com/Graylog2/go-gelf/blob/v2/gelf/reader.go
    Gelf(GelfDeserializerConfig),

    /// Decodes the raw bytes as an [Influxdb Line Protocol][influxdb] message.
    ///
    /// [influxdb]: https://docs.influxdata.com/influxdb/cloud/reference/syntax/line-protocol
    Influxdb(InfluxdbDeserializerConfig),

    /// Decodes the raw bytes as as an [Apache Avro][apache_avro] message.
    ///
    /// [apache_avro]: https://avro.apache.org/
    Avro {
        /// Apache Avro-specific encoder options.
        avro: AvroDeserializerOptions,
    },

    /// Decodes the raw bytes as a string and passes them as input to a [VRL][vrl] program.
    ///
    /// [vrl]: https://vector.dev/docs/reference/vrl
    Vrl(VrlDeserializerConfig),
}

impl From<BytesDeserializerConfig> for DeserializerConfig {
    fn from(_: BytesDeserializerConfig) -> Self {
        Self::Bytes
    }
}

impl From<JsonDeserializerConfig> for DeserializerConfig {
    fn from(config: JsonDeserializerConfig) -> Self {
        Self::Json(config)
    }
}

#[cfg(feature = "syslog")]
impl From<SyslogDeserializerConfig> for DeserializerConfig {
    fn from(config: SyslogDeserializerConfig) -> Self {
        Self::Syslog(config)
    }
}

impl From<GelfDeserializerConfig> for DeserializerConfig {
    fn from(config: GelfDeserializerConfig) -> Self {
        Self::Gelf(config)
    }
}

impl From<NativeDeserializerConfig> for DeserializerConfig {
    fn from(_: NativeDeserializerConfig) -> Self {
        Self::Native
    }
}

impl From<NativeJsonDeserializerConfig> for DeserializerConfig {
    fn from(config: NativeJsonDeserializerConfig) -> Self {
        Self::NativeJson(config)
    }
}

impl From<InfluxdbDeserializerConfig> for DeserializerConfig {
    fn from(config: InfluxdbDeserializerConfig) -> Self {
        Self::Influxdb(config)
    }
}

impl DeserializerConfig {
    /// Build the `Deserializer` from this configuration.
    pub fn build(&self) -> vector_common::Result<Deserializer> {
        match self {
            DeserializerConfig::Avro { avro } => Ok(Deserializer::Avro(
                AvroDeserializerConfig {
                    avro_options: avro.clone(),
                }
                .build(),
            )),
            DeserializerConfig::Bytes => Ok(Deserializer::Bytes(BytesDeserializerConfig.build())),
            DeserializerConfig::Json(config) => Ok(Deserializer::Json(config.build())),
            DeserializerConfig::Protobuf(config) => Ok(Deserializer::Protobuf(config.build()?)),
            #[cfg(feature = "syslog")]
            DeserializerConfig::Syslog(config) => Ok(Deserializer::Syslog(config.build())),
            DeserializerConfig::Native => {
                Ok(Deserializer::Native(NativeDeserializerConfig.build()))
            }
            DeserializerConfig::NativeJson(config) => Ok(Deserializer::NativeJson(config.build())),
            DeserializerConfig::Gelf(config) => Ok(Deserializer::Gelf(config.build())),
            DeserializerConfig::Influxdb(config) => Ok(Deserializer::Influxdb(config.build())),
            DeserializerConfig::Vrl(config) => Ok(Deserializer::Vrl(config.build()?)),
        }
    }

    /// Return an appropriate default framer for the given deserializer
    pub fn default_stream_framing(&self) -> FramingConfig {
        match self {
            DeserializerConfig::Avro { .. } => FramingConfig::Bytes,
            DeserializerConfig::Native => FramingConfig::LengthDelimited(Default::default()),
            DeserializerConfig::Bytes
            | DeserializerConfig::Json(_)
            | DeserializerConfig::Influxdb(_)
            | DeserializerConfig::NativeJson(_) => {
                FramingConfig::NewlineDelimited(Default::default())
            }
            DeserializerConfig::Protobuf(_) => FramingConfig::Bytes,
            #[cfg(feature = "syslog")]
            DeserializerConfig::Syslog(_) => FramingConfig::NewlineDelimited(Default::default()),
            DeserializerConfig::Vrl(_) => FramingConfig::Bytes,
            DeserializerConfig::Gelf(_) => {
                FramingConfig::CharacterDelimited(CharacterDelimitedDecoderConfig::new(0))
            }
        }
    }

    /// Return the type of event build by this deserializer.
    pub fn output_type(&self) -> DataType {
        match self {
            DeserializerConfig::Avro { avro } => AvroDeserializerConfig {
                avro_options: avro.clone(),
            }
            .output_type(),
            DeserializerConfig::Bytes => BytesDeserializerConfig.output_type(),
            DeserializerConfig::Json(config) => config.output_type(),
            DeserializerConfig::Protobuf(config) => config.output_type(),
            #[cfg(feature = "syslog")]
            DeserializerConfig::Syslog(config) => config.output_type(),
            DeserializerConfig::Native => NativeDeserializerConfig.output_type(),
            DeserializerConfig::NativeJson(config) => config.output_type(),
            DeserializerConfig::Gelf(config) => config.output_type(),
            DeserializerConfig::Vrl(config) => config.output_type(),
            DeserializerConfig::Influxdb(config) => config.output_type(),
        }
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match self {
            DeserializerConfig::Avro { avro } => AvroDeserializerConfig {
                avro_options: avro.clone(),
            }
            .schema_definition(log_namespace),
            DeserializerConfig::Bytes => BytesDeserializerConfig.schema_definition(log_namespace),
            DeserializerConfig::Json(config) => config.schema_definition(log_namespace),
            DeserializerConfig::Protobuf(config) => config.schema_definition(log_namespace),
            #[cfg(feature = "syslog")]
            DeserializerConfig::Syslog(config) => config.schema_definition(log_namespace),
            DeserializerConfig::Native => NativeDeserializerConfig.schema_definition(log_namespace),
            DeserializerConfig::NativeJson(config) => config.schema_definition(log_namespace),
            DeserializerConfig::Gelf(config) => config.schema_definition(log_namespace),
            DeserializerConfig::Influxdb(config) => config.schema_definition(log_namespace),
            DeserializerConfig::Vrl(config) => config.schema_definition(log_namespace),
        }
    }

    /// Get the HTTP content type.
    pub const fn content_type(&self, framer: &FramingConfig) -> &'static str {
        match (&self, framer) {
            (
                DeserializerConfig::Json(_) | DeserializerConfig::NativeJson(_),
                FramingConfig::NewlineDelimited(_),
            ) => "application/x-ndjson",
            (
                DeserializerConfig::Gelf(_)
                | DeserializerConfig::Json(_)
                | DeserializerConfig::NativeJson(_),
                FramingConfig::CharacterDelimited(CharacterDelimitedDecoderConfig {
                    character_delimited:
                        CharacterDelimitedDecoderOptions {
                            delimiter: b',',
                            max_length: Some(usize::MAX),
                        },
                }),
            ) => "application/json",
            (DeserializerConfig::Native, _) | (DeserializerConfig::Avro { .. }, _) => {
                "application/octet-stream"
            }
            (DeserializerConfig::Protobuf(_), _) => "application/octet-stream",
            (
                DeserializerConfig::Json(_)
                | DeserializerConfig::NativeJson(_)
                | DeserializerConfig::Bytes
                | DeserializerConfig::Gelf(_)
                | DeserializerConfig::Influxdb(_)
                | DeserializerConfig::Vrl(_),
                _,
            ) => "text/plain",
            #[cfg(feature = "syslog")]
            (DeserializerConfig::Syslog(_), _) => "text/plain",
        }
    }
}

/// Parse structured events from bytes.
#[derive(Clone)]
pub enum Deserializer {
    /// Uses a `AvroDeserializer` for deserialization.
    Avro(AvroDeserializer),
    /// Uses a `BytesDeserializer` for deserialization.
    Bytes(BytesDeserializer),
    /// Uses a `JsonDeserializer` for deserialization.
    Json(JsonDeserializer),
    /// Uses a `ProtobufDeserializer` for deserialization.
    Protobuf(ProtobufDeserializer),
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
    /// Uses a `InfluxdbDeserializer` for deserialization.
    Influxdb(InfluxdbDeserializer),
    /// Uses a `VrlDeserializer` for deserialization.
    Vrl(VrlDeserializer),
}

impl format::Deserializer for Deserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        match self {
            Deserializer::Avro(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::Bytes(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::Json(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::Protobuf(deserializer) => deserializer.parse(bytes, log_namespace),
            #[cfg(feature = "syslog")]
            Deserializer::Syslog(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::Native(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::NativeJson(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::Boxed(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::Gelf(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::Influxdb(deserializer) => deserializer.parse(bytes, log_namespace),
            Deserializer::Vrl(deserializer) => deserializer.parse(bytes, log_namespace),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gelf_stream_default_framing_is_null_delimited() {
        let deserializer_config = DeserializerConfig::from(GelfDeserializerConfig::default());
        let framing_config = deserializer_config.default_stream_framing();
        assert!(matches!(
            framing_config,
            FramingConfig::CharacterDelimited(CharacterDelimitedDecoderConfig {
                character_delimited: CharacterDelimitedDecoderOptions {
                    delimiter: 0,
                    max_length: None,
                }
            })
        ));
    }
}
