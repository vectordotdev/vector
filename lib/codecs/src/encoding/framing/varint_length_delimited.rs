use bytes::{BufMut, BytesMut};
use derivative::Derivative;
use snafu::Snafu;
use tokio_util::codec::Encoder;
use vector_config::configurable_component;

use super::{BoxedFramingError, FramingError};

/// Errors that can occur during varint length delimited framing.
#[derive(Debug, Snafu)]
pub enum VarintFramingError {
    #[snafu(display("Frame too large: {length} bytes (max: {max})"))]
    FrameTooLarge { length: usize, max: usize },
}

impl FramingError for VarintFramingError {}

/// Config used to build a `VarintLengthDelimitedEncoder`.
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Derivative)]
#[derivative(Default)]
pub struct VarintLengthDelimitedEncoderConfig {
    /// Maximum frame length
    #[serde(default = "default_max_frame_length")]
    pub max_frame_length: usize,
}

const fn default_max_frame_length() -> usize {
    8 * 1_024 * 1_024
}

impl VarintLengthDelimitedEncoderConfig {
    /// Build the `VarintLengthDelimitedEncoder` from this configuration.
    pub fn build(&self) -> VarintLengthDelimitedEncoder {
        VarintLengthDelimitedEncoder::new(self.max_frame_length)
    }
}

/// A codec for handling bytes sequences whose length is encoded as a varint prefix.
/// This is compatible with protobuf's length-delimited encoding.
#[derive(Debug, Clone)]
pub struct VarintLengthDelimitedEncoder {
    max_frame_length: usize,
}

impl VarintLengthDelimitedEncoder {
    /// Creates a new `VarintLengthDelimitedEncoder`.
    pub fn new(max_frame_length: usize) -> Self {
        Self { max_frame_length }
    }

    /// Encode a varint into the buffer
    fn encode_varint(&self, value: usize, buf: &mut BytesMut) -> Result<(), BoxedFramingError> {
        if value > self.max_frame_length {
            return Err(VarintFramingError::FrameTooLarge {
                length: value,
                max: self.max_frame_length,
            }
            .into());
        }

        let mut val = value;
        while val >= 0x80 {
            buf.put_u8((val as u8) | 0x80);
            val >>= 7;
        }
        buf.put_u8(val as u8);
        Ok(())
    }
}

impl Default for VarintLengthDelimitedEncoder {
    fn default() -> Self {
        Self::new(default_max_frame_length())
    }
}

impl Encoder<()> for VarintLengthDelimitedEncoder {
    type Error = BoxedFramingError;

    fn encode(&mut self, _: (), buffer: &mut BytesMut) -> Result<(), Self::Error> {
        // This encoder expects the data to already be in the buffer
        // We just need to prepend the varint length
        let data_length = buffer.len();
        if data_length == 0 {
            return Ok(());
        }

        // Create a temporary buffer to hold the varint
        let mut varint_buffer = BytesMut::new();
        self.encode_varint(data_length, &mut varint_buffer)?;

        // Prepend the varint to the buffer
        let varint_bytes = varint_buffer.freeze();
        let data_bytes = buffer.split_to(buffer.len());
        buffer.extend_from_slice(&varint_bytes);
        buffer.extend_from_slice(&data_bytes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_single_byte_varint() {
        let mut buffer = BytesMut::from(&b"foo"[..]);
        let mut encoder = VarintLengthDelimitedEncoder::default();

        encoder.encode((), &mut buffer).unwrap();
        assert_eq!(buffer, &[0x03, b'f', b'o', b'o'][..]);
    }

    #[test]
    fn encode_multi_byte_varint() {
        let mut buffer = BytesMut::from(&b"foo"[..]);
        let mut encoder = VarintLengthDelimitedEncoder::new(1000);

        // Set a larger frame to trigger multi-byte varint
        buffer.clear();
        buffer.extend_from_slice(&vec![b'x'; 300]);
        encoder.encode((), &mut buffer).unwrap();

        // 300 in varint encoding: 0xAC 0x02
        assert_eq!(buffer[0..2], [0xAC, 0x02]);
        assert_eq!(buffer.len(), 302); // 2 bytes varint + 300 bytes data
    }

    #[test]
    fn encode_frame_too_large() {
        let large_data = vec![b'x'; 1001];
        let mut buffer = BytesMut::from(&large_data[..]);
        let mut encoder = VarintLengthDelimitedEncoder::new(1000);

        assert!(encoder.encode((), &mut buffer).is_err());
    }

    #[test]
    fn encode_empty_buffer() {
        let mut buffer = BytesMut::new();
        let mut encoder = VarintLengthDelimitedEncoder::default();

        encoder.encode((), &mut buffer).unwrap();
        assert_eq!(buffer.len(), 0);
    }
}
