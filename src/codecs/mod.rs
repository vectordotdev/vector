//! A collection of codecs that can be used to transform between bytes streams /
//! byte messages, byte frames and structured events.

#![deny(missing_docs)]

pub mod decoding;
pub mod encoding;
mod ready_frames;

pub use decoding::{
    BytesDecoder, BytesDecoderConfig, BytesDeserializer, BytesDeserializerConfig,
    CharacterDelimitedDecoder, CharacterDelimitedDecoderConfig, Decoder, JsonDeserializer,
    JsonDeserializerConfig, LengthDelimitedDecoder, LengthDelimitedDecoderConfig,
    NewlineDelimitedDecoder, NewlineDelimitedDecoderConfig, OctetCountingDecoder,
    OctetCountingDecoderConfig,
};
#[cfg(feature = "sources-syslog")]
pub use decoding::{SyslogDeserializer, SyslogDeserializerConfig};
pub use encoding::{
    CharacterDelimitedEncoder, CharacterDelimitedEncoderConfig, JsonSerializer,
    JsonSerializerConfig, NewlineDelimitedEncoder, NewlineDelimitedEncoderConfig,
    RawMessageSerializer, RawMessageSerializerConfig,
};
pub use ready_frames::ReadyFrames;
