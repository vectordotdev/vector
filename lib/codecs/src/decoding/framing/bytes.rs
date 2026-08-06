use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Decoder;

use super::BoxedFramingError;

/// Config used to build a `BytesDecoderConfig`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BytesDecoderConfig;

impl BytesDecoderConfig {
    /// Creates a new `BytesDecoderConfig`.
    pub const fn new() -> Self {
        Self
    }

    /// Build the `ByteDecoder` from this configuration.
    pub const fn build(&self) -> BytesDecoder {
        BytesDecoder::new()
    }
}

/// A decoder for passing through bytes as-is.
///
/// This is basically a no-op and is used to convert from `BytesMut` to `Bytes`.
#[derive(Debug, Clone)]
pub struct BytesDecoder {
    /// Whether the empty buffer has been flushed. This is important to
    /// propagate empty frames in message based transports.
    flushed: bool,
}

impl BytesDecoder {
    /// Creates a new `BytesDecoder`.
    pub const fn new() -> Self {
        Self { flushed: false }
    }
}

impl Default for BytesDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for BytesDecoder {
    type Item = Bytes;
    type Error = BoxedFramingError;

    fn decode(&mut self, _src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.flushed = false;
        Ok(None)
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.flushed && src.is_empty() {
            Ok(None)
        } else {
            self.flushed = true;
            let frame = src.split();
            Ok(Some(frame.freeze()))
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use tokio_util::codec::FramedRead;

    use super::*;

    #[test]
    fn decode_frame() {
        let mut input = BytesMut::from("some bytes");
        let mut decoder = BytesDecoder::new();

        assert_eq!(decoder.decode(&mut input).unwrap(), None);
        assert_eq!(
            decoder.decode_eof(&mut input).unwrap().unwrap(),
            "some bytes"
        );
        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[tokio::test]
    async fn decode_frame_reader() {
        let input: &[u8] = b"foo";
        let decoder = BytesDecoder::new();

        let mut reader = FramedRead::new(input, decoder);

        assert_eq!(reader.next().await.unwrap().unwrap(), "foo");
        assert!(reader.next().await.is_none());
    }

    #[tokio::test]
    async fn decode_frame_reader_empty() {
        let input: &[u8] = b"";
        let decoder = BytesDecoder::new();

        let mut reader = FramedRead::new(input, decoder);

        assert_eq!(reader.next().await.unwrap().unwrap(), "");
        assert!(reader.next().await.is_none());
    }
}
