//! A collection of codecs that can be used to transform between bytes streams /
//! byte messages, byte frames and structured events.

#![deny(missing_docs)]

pub mod decoding;
pub mod encoding;
mod format;
mod framing;
mod ready_frames;

pub use decoding::Decoder;
pub use format::{
    BytesDeserializer, BytesDeserializerConfig, JsonDeserializer, JsonDeserializerConfig,
    JsonSerializer, JsonSerializerConfig, RawMessageSerializer, RawMessageSerializerConfig,
};
#[cfg(feature = "sources-syslog")]
pub use format::{SyslogDeserializer, SyslogDeserializerConfig};
pub use framing::{
    BytesDecoder, BytesDecoderConfig, CharacterDelimitedDecoder, CharacterDelimitedDecoderConfig,
    CharacterDelimitedEncoder, CharacterDelimitedEncoderConfig, LengthDelimitedDecoder,
    LengthDelimitedDecoderConfig, NewlineDelimitedDecoder, NewlineDelimitedDecoderConfig,
    NewlineDelimitedEncoder, NewlineDelimitedEncoderConfig, OctetCountingDecoder,
    OctetCountingDecoderConfig,
};
pub use ready_frames::ReadyFrames;
