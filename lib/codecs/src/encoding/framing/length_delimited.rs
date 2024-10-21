use bytes::BytesMut;
use derivative::Derivative;
use tokio_util::codec::{Encoder, LengthDelimitedCodec};
use vector_config::configurable_component;

use crate::common::length_delimited::LengthDelimitedCoderOptions;

use super::BoxedFramingError;

/// Config used to build a `LengthDelimitedEncoder`.
#[configurable_component]
#[derive(Debug, Clone, Derivative, Eq, PartialEq)]
#[derivative(Default)]
pub struct LengthDelimitedEncoderConfig {
    /// Options for the length delimited decoder.
    #[serde(skip_serializing_if = "vector_core::serde::is_default")]
    pub length_delimited: LengthDelimitedCoderOptions,
}

impl LengthDelimitedEncoderConfig {
    /// Build the `LengthDelimitedEncoder` from this configuration.
    pub fn build(&self) -> LengthDelimitedEncoder {
        LengthDelimitedEncoder::new(&self.length_delimited)
    }
}

/// An encoder for handling bytes that are delimited by a length header.
#[derive(Debug, Clone)]
pub struct LengthDelimitedEncoder(LengthDelimitedCodec);

impl LengthDelimitedEncoder {
    /// Creates a new `LengthDelimitedEncoder`.
    pub fn new(config: &LengthDelimitedCoderOptions) -> Self {
        Self(config.build_codec())
    }
}

impl Default for LengthDelimitedEncoder {
    fn default() -> Self {
        Self(LengthDelimitedCodec::new())
    }
}

impl Encoder<()> for LengthDelimitedEncoder {
    type Error = BoxedFramingError;

    fn encode(&mut self, _: (), buffer: &mut BytesMut) -> Result<(), BoxedFramingError> {
        let bytes = buffer.split().freeze();
        self.0.encode(bytes, buffer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode() {
        let mut codec = LengthDelimitedEncoder::default();

        let mut buffer = BytesMut::from("abc");
        codec.encode((), &mut buffer).unwrap();

        assert_eq!(&buffer[..], b"\0\0\0\x03abc");
    }

    #[test]
    fn encode_2byte_length() {
        let mut codec = LengthDelimitedEncoder::new(&LengthDelimitedCoderOptions {
            length_field_length: 2,
            ..Default::default()
        });

        let mut buffer = BytesMut::from("abc");
        codec.encode((), &mut buffer).unwrap();

        assert_eq!(&buffer[..], b"\0\x03abc");
    }
}
