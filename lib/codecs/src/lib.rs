//! A collection of codecs that can be used to transform between bytes streams /
//! byte messages, byte frames and structured events.

#![deny(missing_docs)]
#![deny(warnings)]

mod common;
pub mod decoding;
pub mod encoding;
pub mod gelf;

pub use decoding::{
    BytesDecoder, BytesDecoderConfig, BytesDeserializer, BytesDeserializerConfig,
    CharacterDelimitedDecoder, CharacterDelimitedDecoderConfig, GelfDeserializer,
    GelfDeserializerConfig, JsonDeserializer, JsonDeserializerConfig, LengthDelimitedDecoder,
    LengthDelimitedDecoderConfig, NativeDeserializer, NativeDeserializerConfig,
    NativeJsonDeserializer, NativeJsonDeserializerConfig, NewlineDelimitedDecoder,
    NewlineDelimitedDecoderConfig, OctetCountingDecoder, OctetCountingDecoderConfig,
    StreamDecodingError,
};
#[cfg(feature = "syslog")]
pub use decoding::{SyslogDeserializer, SyslogDeserializerConfig};
pub use encoding::{
    BytesEncoder, BytesEncoderConfig, CharacterDelimitedEncoder, CharacterDelimitedEncoderConfig,
    CsvSerializer, CsvSerializerConfig, GelfSerializer, GelfSerializerConfig, JsonSerializer,
    JsonSerializerConfig, LengthDelimitedEncoder, LengthDelimitedEncoderConfig, LogfmtSerializer,
    LogfmtSerializerConfig, NativeJsonSerializer, NativeJsonSerializerConfig, NativeSerializer,
    NativeSerializerConfig, NewlineDelimitedEncoder, NewlineDelimitedEncoderConfig,
    RawMessageSerializer, RawMessageSerializerConfig, TextSerializer, TextSerializerConfig,
};
pub use gelf::{gelf_fields, VALID_FIELD_REGEX};
use vector_config::configurable_component;

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
