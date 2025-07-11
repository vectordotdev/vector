use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;

use super::BoxedFramingError;

/// Config used to build a `BytesEncoder`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BytesEncoderConfig;

impl BytesEncoderConfig {
    /// Creates a `BytesEncoderConfig`.
    pub const fn new() -> Self {
        Self
    }

    /// Build the `BytesEncoder` from this configuration.
    pub fn build(&self) -> BytesEncoder {
        BytesEncoder
    }
}

/// An encoder for handling of plain bytes.
///
/// This encoder does nothing, really. It mainly exists as a symmetric
/// counterpart to `BytesDeserializer`. `BytesEncoder` can be used to explicitly
/// disable framing for formats that encode intrinsic length information - since
/// a sink might set a framing configuration by default depending on the
/// streaming or message based nature of the sink.
#[derive(Debug, Clone)]
pub struct BytesEncoder;

impl Default for BytesEncoderConfig {
    /// Creates a `BytesEncoder`.
    fn default() -> Self {
        Self
    }
}

impl Encoder<()> for BytesEncoder {
    type Error = BoxedFramingError;

    fn encode(&mut self, _: (), _: &mut BytesMut) -> Result<(), BoxedFramingError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode() {
        let mut codec = BytesEncoder;

        let mut buffer = BytesMut::from("abc");
        codec.encode((), &mut buffer).unwrap();

        assert_eq!(b"abc", &buffer[..]);
    }
}
