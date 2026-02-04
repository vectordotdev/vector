//! A collection of codecs that can be used to transform between bytes streams /
//! byte messages, byte frames and structured events.

#![deny(missing_docs)]
#![deny(warnings)]

pub mod avro;
mod common;
pub mod decoding;
pub mod encoding;
pub mod gelf;
pub mod internal_events;
mod ready_frames;

pub use decoding::{
    BytesDecoder, BytesDecoderConfig, BytesDeserializer, BytesDeserializerConfig,
    CharacterDelimitedDecoder, CharacterDelimitedDecoderConfig, Decoder, DecodingConfig,
    GelfDeserializer, GelfDeserializerConfig, JsonDeserializer, JsonDeserializerConfig,
    LengthDelimitedDecoder, LengthDelimitedDecoderConfig, NativeDeserializer,
    NativeDeserializerConfig, NativeJsonDeserializer, NativeJsonDeserializerConfig,
    NewlineDelimitedDecoder, NewlineDelimitedDecoderConfig, OctetCountingDecoder,
    OctetCountingDecoderConfig, StreamDecodingError, VarintLengthDelimitedDecoder,
    VarintLengthDelimitedDecoderConfig,
};
#[cfg(feature = "syslog")]
pub use decoding::{SyslogDeserializer, SyslogDeserializerConfig};
pub use encoding::{
    BatchEncoder, BatchSerializer, BytesEncoder, BytesEncoderConfig, CharacterDelimitedEncoder,
    CharacterDelimitedEncoderConfig, CsvSerializer, CsvSerializerConfig, Encoder, EncoderKind,
    EncodingConfig, EncodingConfigWithFraming, GelfSerializer, GelfSerializerConfig,
    JsonSerializer, JsonSerializerConfig, LengthDelimitedEncoder, LengthDelimitedEncoderConfig,
    LogfmtSerializer, LogfmtSerializerConfig, NativeJsonSerializer, NativeJsonSerializerConfig,
    NativeSerializer, NativeSerializerConfig, NewlineDelimitedEncoder,
    NewlineDelimitedEncoderConfig, RawMessageSerializer, RawMessageSerializerConfig, SinkType,
    TextSerializer, TextSerializerConfig, TimestampFormat, Transformer,
};
pub use gelf::{VALID_FIELD_REGEX, gelf_fields};
pub use ready_frames::ReadyFrames;
use vector_config_macros::configurable_component;

/// The user configuration to choose the metric tag strategy.
#[configurable_component]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MetricTagValues {
    /// Tag values are exposed as single strings, the same as they were before this config
    /// option. Tags with multiple values show the last assigned value, and null values
    /// are ignored.
    #[default]
    Single,
    /// All tags are exposed as arrays of either string or null values.
    Full,
}
