use bytes::{Buf, Bytes, BytesMut};
use snafu::Snafu;
use tokio_util::codec::Decoder;
use vector_config::configurable_component;

use super::{BoxedFramingError, FramingError, StreamDecodingError};

/// Errors that can occur during varint length delimited framing.
#[derive(Debug, Snafu)]
pub enum VarintFramingError {
    #[snafu(display("Varint too large"))]
    VarintOverflow,

    #[snafu(display("Frame too large: {length} bytes (max: {max})"))]
    FrameTooLarge { length: usize, max: usize },

    #[snafu(display("Trailing data at EOF"))]
    TrailingData,
}

impl StreamDecodingError for VarintFramingError {
    fn can_continue(&self) -> bool {
        match self {
            // Varint overflow and frame too large are not recoverable
            Self::VarintOverflow | Self::FrameTooLarge { .. } => false,
            // Trailing data at EOF is not recoverable
            Self::TrailingData => false,
        }
    }
}

impl FramingError for VarintFramingError {
    fn as_any(&self) -> &dyn std::any::Any {
        self as &dyn std::any::Any
    }
}

/// Config used to build a `VarintLengthDelimitedDecoder`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
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

    /// Decode a varint from the start of the buffer without consuming it.
    fn decode_varint(&self, buf: &BytesMut) -> Result<Option<(u64, usize)>, BoxedFramingError> {
        let mut value: u64 = 0;
        let mut shift: u8 = 0;

        for (index, byte) in buf.iter().enumerate() {
            let byte_value = (*byte & 0x7F) as u64;
            value |= byte_value << shift;

            if *byte & 0x80 == 0 {
                // Last byte of varint
                return Ok(Some((value, index + 1)));
            }

            shift += 7;
            if shift >= 64 {
                return Err(VarintFramingError::VarintOverflow.into());
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
        // First, peek at the varint length prefix.
        let (length, prefix_length) = match self.decode_varint(src)? {
            Some((length, prefix_length)) => {
                (usize::try_from(length).unwrap_or(usize::MAX), prefix_length)
            }
            None => return Ok(None), // Incomplete varint
        };

        // Check if the length is reasonable
        if length > self.max_frame_length {
            return Err(VarintFramingError::FrameTooLarge {
                length,
                max: self.max_frame_length,
            }
            .into());
        }

        // Check if we have enough data for the complete frame
        if src.len() - prefix_length < length {
            return Ok(None); // Incomplete frame
        }

        // Consume the length prefix and extract the frame
        src.advance(prefix_length);
        let frame = src.split_to(length).freeze();
        Ok(Some(frame))
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            Ok(None)
        } else {
            // Try to decode what we have, even if incomplete
            match self.decode(src)? {
                Some(frame) => Ok(Some(frame)),
                None => {
                    // If we have data but couldn't decode it, it's trailing data
                    if !src.is_empty() {
                        Err(VarintFramingError::TrailingData.into())
                    } else {
                        Ok(None)
                    }
                }
            }
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

        assert_eq!(
            decoder.decode(&mut input).unwrap().unwrap(),
            Bytes::from("foo")
        );
        assert_eq!(decoder.decode(&mut input).unwrap(), None);
    }

    #[test]
    fn decode_multi_byte_varint() {
        // 300 in varint encoding: 0xAC 0x02
        let mut input = BytesMut::from(&[0xAC, 0x02][..]);
        // Add 300 bytes of data
        input.extend_from_slice(&vec![b'x'; 300]);
        let mut decoder = VarintLengthDelimitedDecoder::default();

        let result = decoder.decode(&mut input).unwrap().unwrap();
        assert_eq!(result.len(), 300);
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
    fn decode_frames_split_across_read_buffer() {
        const FRAME_LENGTH: usize = 87;
        const FRAME_COUNT: usize = 200;
        const READ_BUFFER_SIZE: usize = 8 * 1024;

        let mut encoded = Vec::new();

        for index in 0..FRAME_COUNT {
            let byte = b'A' + (index % 26) as u8;

            // FRAME_LENGTH fits in a single-byte varint.
            encoded.push(FRAME_LENGTH as u8);
            encoded.extend_from_slice(&[byte; FRAME_LENGTH]);
        }

        let mut decoder = VarintLengthDelimitedDecoder::default();
        let mut input = BytesMut::new();
        let mut decoded = Vec::new();
        let mut chunks = encoded.chunks(READ_BUFFER_SIZE);

        input.extend_from_slice(chunks.next().unwrap());

        while let Some(frame) = decoder.decode(&mut input).unwrap() {
            decoded.push(frame);
        }

        // 93 complete 88-byte frames fit in the first read, followed by the
        // length prefix and seven payload bytes of the next frame.
        assert_eq!(decoded.len(), 93);
        assert_eq!(input.len(), 8);
        assert_eq!(input[0], FRAME_LENGTH as u8);

        for chunk in chunks {
            input.extend_from_slice(chunk);

            while let Some(frame) = decoder.decode(&mut input).unwrap() {
                decoded.push(frame);
            }
        }

        assert_eq!(decoder.decode_eof(&mut input).unwrap(), None);
        assert!(input.is_empty());
        assert_eq!(decoded.len(), FRAME_COUNT);

        for (index, frame) in decoded.iter().enumerate() {
            let expected = b'A' + (index % 26) as u8;

            assert_eq!(frame.len(), FRAME_LENGTH);
            assert!(frame.iter().all(|byte| *byte == expected));
        }
    }

    #[test]
    fn decode_incomplete_multi_byte_varint_leaves_buffer_intact() {
        let mut input = BytesMut::from(&[0xAC, 0x02][..]);
        input.extend_from_slice(&[b'x'; 100]);
        let mut decoder = VarintLengthDelimitedDecoder::default();

        assert_eq!(decoder.decode(&mut input).unwrap(), None);
        assert_eq!(input.len(), 102);
        assert_eq!(&input[..2], &[0xAC, 0x02][..]);

        input.extend_from_slice(&[b'x'; 200]);
        assert_eq!(decoder.decode(&mut input).unwrap().unwrap().len(), 300);
        assert!(input.is_empty());
    }

    #[test]
    fn decode_frame_too_large() {
        let mut input =
            BytesMut::from(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01][..]);
        let mut decoder = VarintLengthDelimitedDecoder::new(1000);

        assert!(decoder.decode(&mut input).is_err());
    }

    #[test]
    fn decode_trailing_data_at_eof() {
        let mut input = BytesMut::from(&[0x03, b'f', b'o', b'o', b'e', b'x', b't', b'r', b'a'][..]);
        let mut decoder = VarintLengthDelimitedDecoder::default();

        // First decode should succeed
        assert_eq!(
            decoder.decode(&mut input).unwrap().unwrap(),
            Bytes::from("foo")
        );

        // Second decode should fail with trailing data
        assert!(decoder.decode_eof(&mut input).is_err());
    }
}
