use bytes::{Buf, Bytes, BytesMut};
use derivative::Derivative;
use tokio_util::codec::Decoder;
use vector_config::configurable_component;

use super::BoxedFramingError;

/// Config used to build a `VarintLengthDelimitedDecoder`.
#[configurable_component]
#[derive(Debug, Clone, Derivative)]
#[derivative(Default)]
pub struct VarintLengthDelimitedDecoderConfig {
    /// Maximum frame length
    #[serde(default = "default_max_frame_length")]
    pub max_frame_length: usize,
}

const fn default_max_frame_length() -> usize {
    8 * 1_024 * 1_024
}

impl VarintLengthDelimitedDecoderConfig {
    /// Build the `VarintLengthDelimitedDecoder` from this configuration.
    pub fn build(&self) -> VarintLengthDelimitedDecoder {
        VarintLengthDelimitedDecoder::new(self.max_frame_length)
    }
}

/// A codec for handling bytes sequences whose length is encoded as a varint prefix.
/// This is compatible with protobuf's length-delimited encoding.
#[derive(Debug, Clone)]
pub struct VarintLengthDelimitedDecoder {
    max_frame_length: usize,
}

impl VarintLengthDelimitedDecoder {
    /// Creates a new `VarintLengthDelimitedDecoder`.
    pub fn new(max_frame_length: usize) -> Self {
        Self { max_frame_length }
    }

    /// Decode a varint from the buffer
    fn decode_varint(&self, buf: &mut BytesMut) -> Result<Option<usize>, BoxedFramingError> {
        if buf.is_empty() {
            return Ok(None);
        }

        let mut value: usize = 0;
        let mut shift: u32 = 0;
        let mut bytes_read = 0;

        for byte in buf.iter() {
            bytes_read += 1;
            let byte_value = (*byte & 0x7F) as usize;
            value |= byte_value << shift;

            if *byte & 0x80 == 0 {
                // Last byte of varint
                buf.advance(bytes_read);
                return Ok(Some(value));
            }

            shift += 7;
            if shift >= 64 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Varint too large",
                ).into());
            }
        }

        // Incomplete varint
        Ok(None)
    }
}

impl Default for VarintLengthDelimitedDecoder {
    fn default() -> Self {
        Self::new(default_max_frame_length())
    }
}

impl Decoder for VarintLengthDelimitedDecoder {
    type Item = Bytes;
    type Error = BoxedFramingError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // First, try to decode the varint length
        let length = match self.decode_varint(src)? {
            Some(len) => len,
            None => return Ok(None), // Incomplete varint
        };

        // Check if the length is reasonable
        if length > self.max_frame_length {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame too large: {} bytes (max: {})", length, self.max_frame_length),
            ).into());
        }

        // Check if we have enough data for the complete frame
        if src.len() < length {
            return Ok(None); // Incomplete frame
        }

        // Extract the frame
        let frame = src.split_to(length).freeze();
        Ok(Some(frame))
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            Ok(None)
        } else {
            // Try to decode what we have, even if incomplete
            self.decode(src)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_single_byte_varint() {
        let mut input = BytesMut::from(&[0x03, b'f', b'o', b'o'][..]);
        let mut decoder = VarintLengthDelimitedDecoder::default();

        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_multi_byte_varint() {
        // 300 in varint encoding: 0xAC 0x02
        let mut input = BytesMut::from(&[0xAC, 0x02, b'f', b'o', b'o'][..]);
        let mut decoder = VarintLengthDelimitedDecoder::default();

        assert_eq!(decoder.decode(&mut input).unwrap().unwrap(), "foo");
        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_incomplete_varint() {
        let mut input = BytesMut::from(&[0x80][..]); // Incomplete varint
        let mut decoder = VarintLengthDelimitedDecoder::default();

        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_incomplete_frame() {
        let mut input = BytesMut::from(&[0x05, b'f', b'o'][..]); // Length 5, but only 2 bytes
        let mut decoder = VarintLengthDelimitedDecoder::default();

        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_frame_too_large() {
        let mut input = BytesMut::from(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01][..]);
        let mut decoder = VarintLengthDelimitedDecoder::new(1000);

        assert!(decoder.decode(&mut input).is_err());
    }
} 