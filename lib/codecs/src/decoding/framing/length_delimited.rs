use bytes::{Bytes, BytesMut};
use derivative::Derivative;
use tokio_util::codec::Decoder;
use vector_config_macros::configurable_component;

use super::BoxedFramingError;

/// Config used to build a `LengthDelimitedDecoder`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct LengthDelimitedDecoderConfig {
    /// Options for the length delimited decoder.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub length_delimited: LengthDelimitedDecoderOptions,
}

/// Options for building a `LengthDelimitedDecoder`.
#[configurable_component]
#[derive(Clone, Debug, Derivative, PartialEq, Eq)]
#[derivative(Default)]
pub struct LengthDelimitedDecoderOptions {
    /// The maximum length of the frames.
    ///
    /// This length does *not* include the frame's header.
    ///
    /// By default, the maximum size of the frame is
    /// [8MiB](https://docs.rs/tokio-util/0.7.10/tokio_util/codec/length_delimited/struct.Builder.html#method.max_frame_length).
    #[serde(skip_serializing_if = "vector_core::serde::is_default")]
    pub max_frame_length: Option<usize>,
}

impl LengthDelimitedDecoderOptions {
    /// Creates a `LengthDelimitedDecoderOptions` with a maximum frame length limit.
    pub const fn new_with_max_frame_length(max_length: usize) -> Self {
        Self {
            max_frame_length: Some(max_length),
        }
    }
}

impl LengthDelimitedDecoderConfig {
    /// Creates a new `LengthDelimitedDecoderConfig`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Build the `LengthDelimitedDecoder` from this configuration.
    pub fn build(&self) -> LengthDelimitedDecoder {
        if let Some(max_length) = self.length_delimited.max_frame_length {
            LengthDelimitedDecoder::new_with_max_frame_length(max_length)
        } else {
            LengthDelimitedDecoder::default()
        }
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

    /// Creates a `LengthDelimitedDecoder` with a maximum frame length limit.
    pub fn new_with_max_frame_length(max_length: usize) -> Self {
        let mut codec = tokio_util::codec::LengthDelimitedCodec::new();
        codec.set_max_frame_length(max_length);
        Self(codec)
    }
}

impl Default for LengthDelimitedDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for LengthDelimitedDecoder {
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
    fn clone(&self) -> Self {
        Self::new_with_max_frame_length(self.0.max_frame_length())
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

    #[test]
    fn decode_frame_max_length() {
        let mut input = BytesMut::from("\x00\x00\x00\x03foo");
        let mut decoder = LengthDelimitedDecoder::new_with_max_frame_length(2);

        assert!(decoder.decode(&mut input).is_err());
    }
}
