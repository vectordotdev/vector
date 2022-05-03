use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Decoder;

use super::BoxedFramingError;

/// Config used to build a `LengthDelimitedDecoder`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LengthDelimitedDecoderConfig;

impl LengthDelimitedDecoderConfig {
    /// Build the `LengthDelimitedDecoder` from this configuration.
    pub fn build(&self) -> LengthDelimitedDecoder {
        LengthDelimitedDecoder::new()
    }
}

/// A codec for handling bytes sequences whose length is encoded in a frame head.
///
/// Currently, this expects a length header in 32-bit MSB by default; options to
/// control the format of the header can be added in the future.
#[derive(Debug)]
pub struct LengthDelimitedDecoder(tokio_util::codec::LengthDelimitedCodec);

impl LengthDelimitedDecoder {
    /// Creates a new `LengthDelimitedDecoder`.
    pub fn new() -> Self {
        Self(tokio_util::codec::LengthDelimitedCodec::new())
    }
}

impl Default for LengthDelimitedDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for LengthDelimitedDecoder {
    fn clone(&self) -> Self {
        // This has been fixed with https://github.com/tokio-rs/tokio/pull/4089,
        // however we are blocked on upgrading to a new release of `tokio-util`
        // that includes the `Clone` implementation:
        // https://github.com/vectordotdev/vector/issues/11257.
        //
        // This is an awful implementation for `Clone` since it resets the
        // internal state. However, it works for our use case because we
        // generally only clone a codec that has not been mutated yet.
        //
        // Ideally, `tokio_util::codec::LengthDelimitedCodec` should implement
        // `Clone` and it doesn't look like it was a deliberate decision to
        // leave out the implementation. All of its internal fields implement
        // `Clone`, so adding an implementation for `Clone` could be contributed
        // to the upstream repo easily by adding it to the `derive` macro.
        Self::new()
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
        let mut decoder = LengthDelimitedDecoder::new();

        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_frame_ignore_unexpected_eof() {
        let mut input = BytesMut::from("\x00\x00\x00\x03fo");
        let mut decoder = LengthDelimitedDecoder::new();

        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_frame_ignore_exceeding_bytes_without_header() {
        let mut input = BytesMut::from("\x00\x00\x00\x03fooo");
        let mut decoder = LengthDelimitedDecoder::new();

        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_frame_ignore_missing_header() {
        let mut input = BytesMut::from("foo");
        let mut decoder = LengthDelimitedDecoder::new();

        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_frames() {
        let mut input = BytesMut::from("\x00\x00\x00\x03foo\x00\x00\x00\x03bar");
        let mut decoder = LengthDelimitedDecoder::new();

        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "bar");
        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_eof_frame() {
        let mut input = BytesMut::from("\x00\x00\x00\x03foo");
        let mut decoder = LengthDelimitedDecoder::new();

        assert_eq!(decoder.decode_eof(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode_eof(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_eof_frame_unexpected_eof() {
        let mut input = BytesMut::from("\x00\x00\x00\x03fo");
        let mut decoder = LengthDelimitedDecoder::new();

        assert!(decoder.decode_eof(&mut input).is_err());
    }

    #[test]
    fn decode_eof_frame_exceeding_bytes_without_header() {
        let mut input = BytesMut::from("\x00\x00\x00\x03fooo");
        let mut decoder = LengthDelimitedDecoder::new();

        assert_eq!(decoder.decode_eof(&mut input).unwrap().unwrap(), "foo");
        assert!(decoder.decode_eof(&mut input).is_err());
    }

    #[test]
    fn decode_eof_frame_missing_header() {
        let mut input = BytesMut::from("foo");
        let mut decoder = LengthDelimitedDecoder::new();

        assert!(decoder.decode_eof(&mut input).is_err());
    }

    #[test]
    fn decode_eof_frames() {
        let mut input = BytesMut::from("\x00\x00\x00\x03foo\x00\x00\x00\x03bar");
        let mut decoder = LengthDelimitedDecoder::new();

        assert_eq!(decoder.decode_eof(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode_eof(&mut input).unwrap().unwrap(), "bar");
        assert_eq!(decoder.decode_eof(&mut input).unwrap(), None);
    }
}
