//! A collection of codecs that can be used to transform between bytes streams /
//! byte messages, byte frames and structured events.

#![deny(missing_docs)]
#![deny(warnings)]

mod common;
mod decoder_framed_read;
pub mod decoding;
pub mod encoding;
pub mod gelf;
pub mod internal_events;
mod ready_frames;

pub use decoder_framed_read::DecoderFramedRead;
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
    /// Tag values are exposed using their underlying shape: single-value tags as strings,
    /// multi-value tags as arrays. Writes follow the same rule -- a string or null produces
    /// a single tag; an array of length >= 2 produces a multi-value tag. A length-1 array
    /// round-trips as a scalar; use `Full` to force array shape.
    Auto,
}

impl From<MetricTagValues> for vector_core::event::MetricTagMode {
    fn from(value: MetricTagValues) -> Self {
        match value {
            MetricTagValues::Single => Self::Single,
            MetricTagValues::Full => Self::Full,
            MetricTagValues::Auto => Self::Auto,
        }
    }
}
