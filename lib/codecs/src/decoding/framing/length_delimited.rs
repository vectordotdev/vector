use bytes::{Bytes, BytesMut};
use derivative::Derivative;
use tokio_util::codec::Decoder;
use vector_config::configurable_component;

use crate::common::length_delimited::LengthDelimitedCoderOptions;

use super::BoxedFramingError;

/// Config used to build a `LengthDelimitedDecoder`.
#[configurable_component]
#[derive(Debug, Clone, Derivative)]
#[derivative(Default)]
pub struct LengthDelimitedDecoderConfig {
    /// Options for the length delimited decoder.
    #[serde(skip_serializing_if = "vector_core::serde::is_default")]
    pub length_delimited: LengthDelimitedCoderOptions,
}

impl LengthDelimitedDecoderConfig {
    /// Build the `LengthDelimitedDecoder` from this configuration.
    pub fn build(&self) -> LengthDelimitedDecoder {
        LengthDelimitedDecoder::new(&self.length_delimited)
    }
}

/// A codec for handling bytes sequences whose length is encoded in a frame head.
#[derive(Debug, Clone)]
pub struct LengthDelimitedDecoder(tokio_util::codec::LengthDelimitedCodec);

impl LengthDelimitedDecoder {
    /// Creates a new `LengthDelimitedDecoder`.
    pub fn new(config: &LengthDelimitedCoderOptions) -> Self {
        Self(config.build_codec())
    }
}

impl Default for LengthDelimitedDecoder {
    fn default() -> Self {
        Self(tokio_util::codec::LengthDelimitedCodec::new())
    }
}

impl Decoder for LengthDelimitedDecoder {
    type Item = Bytes;
    type Error = BoxedFramingError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.0
            .decode(src)
            .map(|bytes| bytes.map(BytesMut::freeze))
            .map_err(Into::into)
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.0
            .decode_eof(src)
            .map(|bytes| bytes.map(BytesMut::freeze))
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_frame() {
        let mut input = BytesMut::from("\x00\x00\x00\x03foo");
        let mut decoder = LengthDelimitedDecoder::default();

        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_frame_2byte_length() {
        let mut input = BytesMut::from("\x00\x03foo");
        let mut decoder = LengthDelimitedDecoder::new(&LengthDelimitedCoderOptions {
            length_field_length: 2,
            ..Default::default()
        });

        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_frame_little_endian() {
        let mut input = BytesMut::from("\x03\x00\x00\x00foo");
        let mut decoder = LengthDelimitedDecoder::new(&LengthDelimitedCoderOptions {
            length_field_is_big_endian: false,
            ..Default::default()
        });

        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_frame_2byte_length_with_offset() {
        let mut input = BytesMut::from("\x00\x00\x00\x03foo");
        let mut decoder = LengthDelimitedDecoder::new(&LengthDelimitedCoderOptions {
            length_field_length: 2,
            length_field_offset: 2,
            ..Default::default()
        });

        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_frame_ignore_unexpected_eof() {
        let mut input = BytesMut::from("\x00\x00\x00\x03fo");
        let mut decoder = LengthDelimitedDecoder::default();

        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_frame_ignore_exceeding_bytes_without_header() {
        let mut input = BytesMut::from("\x00\x00\x00\x03fooo");
        let mut decoder = LengthDelimitedDecoder::default();

        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_frame_ignore_missing_header() {
        let mut input = BytesMut::from("foo");
        let mut decoder = LengthDelimitedDecoder::default();

        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_frames() {
        let mut input = BytesMut::from("\x00\x00\x00\x03foo\x00\x00\x00\x03bar");
        let mut decoder = LengthDelimitedDecoder::default();

        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "bar");
        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_eof_frame() {
        let mut input = BytesMut::from("\x00\x00\x00\x03foo");
        let mut decoder = LengthDelimitedDecoder::default();

        assert_eq!(decoder.decode_eof(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode_eof(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_eof_frame_unexpected_eof() {
        let mut input = BytesMut::from("\x00\x00\x00\x03fo");
        let mut decoder = LengthDelimitedDecoder::default();

        assert!(decoder.decode_eof(&mut input).is_err());
    }

    #[test]
    fn decode_eof_frame_exceeding_bytes_without_header() {
        let mut input = BytesMut::from("\x00\x00\x00\x03fooo");
        let mut decoder = LengthDelimitedDecoder::default();

        assert_eq!(decoder.decode_eof(&mut input).unwrap().unwrap(), "foo");
        assert!(decoder.decode_eof(&mut input).is_err());
    }

    #[test]
    fn decode_eof_frame_missing_header() {
        let mut input = BytesMut::from("foo");
        let mut decoder = LengthDelimitedDecoder::default();

        assert!(decoder.decode_eof(&mut input).is_err());
    }

    #[test]
    fn decode_eof_frames() {
        let mut input = BytesMut::from("\x00\x00\x00\x03foo\x00\x00\x00\x03bar");
        let mut decoder = LengthDelimitedDecoder::default();

        assert_eq!(decoder.decode_eof(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode_eof(&mut input).unwrap().unwrap(), "bar");
        assert_eq!(decoder.decode_eof(&mut input).unwrap(), None);
    }
}
