use crate::codecs::{BoxedFramer, BoxedFramingError, FramingConfig};
use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Decoder;

/// Config used to build a `BytesDecoderConfig`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BytesDecoderConfig;

#[typetag::serde(name = "bytes")]
impl FramingConfig for BytesDecoderConfig {
    fn build(&self) -> crate::Result<BoxedFramer> {
        Ok(Box::new(BytesCodec::new()))
    }
}

/// A codec for passing through bytes as-is.
///
/// This is basically a no-op and is used to convert from `BytesMut` to `Bytes`.
#[derive(Debug, Clone)]
pub struct BytesCodec;

impl BytesCodec {
    /// Creates a new `BytesCodec`.
    pub const fn new() -> Self {
        Self
    }
}

impl Default for BytesCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for BytesCodec {
    type Item = Bytes;
    type Error = BoxedFramingError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let frame = src.split_to(src.len());
        Ok(Some(frame.freeze()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_frame() {
        let mut input = BytesMut::from("some bytes");
        let mut decoder = BytesCodec::new();

        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "some bytes");
    }

    #[test]
    fn decode_frame_empty() {
        let mut input = BytesMut::from("");
        let mut decoder = BytesCodec::new();

        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "");
    }
}
