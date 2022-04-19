//! A collection of codecs that can be used to transform between bytes streams /
//! byte messages, byte frames and structured events.

#![deny(missing_docs)]

mod decoder;
mod encoder;
mod ready_frames;

pub use decoder::{Decoder, DecodingConfig};
pub use encoder::Encoder;
pub use ready_frames::ReadyFrames;
