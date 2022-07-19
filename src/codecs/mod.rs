//! A collection of codecs that can be used to transform between bytes streams /
//! byte messages, byte frames and structured events.

#![deny(missing_docs)]

mod decoding;
mod encoding;
mod ready_frames;

pub use decoding::{Decoder, DecodingConfig};
pub use encoding::{
    Encoder, EncodingConfig, EncodingConfigWithFraming, SinkType, TimestampFormat, Transformer,
};
pub use ready_frames::ReadyFrames;
