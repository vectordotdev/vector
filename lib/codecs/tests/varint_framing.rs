#![allow(clippy::unwrap_used)]

use bytes::BytesMut;
use codecs::{
    VarintLengthDelimitedDecoder, VarintLengthDelimitedDecoderConfig,
    encoding::{VarintLengthDelimitedEncoder, VarintLengthDelimitedEncoderConfig},
};
use tokio_util::codec::{Decoder, Encoder};

#[test]
fn test_varint_framing_roundtrip() {
    let test_data = vec![b"hello".to_vec(), b"world".to_vec(), b"protobuf".to_vec()];

    let mut encoder = VarintLengthDelimitedEncoder::default();
    let mut decoder = VarintLengthDelimitedDecoder::default();
    let mut encoded_buffer = BytesMut::new();

    // Encode all test data
    for data in &test_data {
        let mut frame_buffer = BytesMut::from(data.as_slice());
        encoder.encode((), &mut frame_buffer).unwrap();
        encoded_buffer.extend_from_slice(&frame_buffer);
    }

    // Decode all test data
    let mut decoded_data = Vec::new();
    let mut decode_buffer = encoded_buffer;

    while !decode_buffer.is_empty() {
        match decoder.decode(&mut decode_buffer) {
            Ok(Some(frame)) => {
                decoded_data.push(frame.to_vec());
            }
            Ok(None) => {
                // Need more data, but we've provided all data
                break;
            }
            Err(e) => {
                panic!("Decoding error: {e:?}");
            }
        }
    }

    // Verify roundtrip
    assert_eq!(decoded_data.len(), test_data.len());
    for (original, decoded) in test_data.iter().zip(decoded_data.iter()) {
        assert_eq!(original, decoded);
    }
}

#[test]
fn test_varint_framing_large_frame() {
    let large_data = vec![b'x'; 300]; // 300 bytes
    let mut encoder = VarintLengthDelimitedEncoder::default();
    let mut decoder = VarintLengthDelimitedDecoder::default();

    // Encode
    let mut frame_buffer = BytesMut::from(&large_data[..]);
    encoder.encode((), &mut frame_buffer).unwrap();

    // Verify varint encoding (300 = 0xAC 0x02)
    assert_eq!(frame_buffer[0], 0xAC);
    assert_eq!(frame_buffer[1], 0x02);
    assert_eq!(frame_buffer.len(), 302); // 2 bytes varint + 300 bytes data

    // Decode
    let decoded = decoder.decode(&mut frame_buffer).unwrap().unwrap();
    assert_eq!(decoded.to_vec(), large_data);
}

#[test]
fn test_varint_framing_incomplete_frame() {
    let mut decoder = VarintLengthDelimitedDecoder::default();
    let mut buffer = BytesMut::from(&[0x05, b'f', b'o'][..]); // Length 5, but only 2 bytes

    // Should return None for incomplete frame
    assert_eq!(decoder.decode(&mut buffer).unwrap(), None);
}

#[test]
fn test_varint_framing_incomplete_varint() {
    let mut decoder = VarintLengthDelimitedDecoder::default();
    let mut buffer = BytesMut::from(&[0x80][..]); // Incomplete varint

    // Should return None for incomplete varint
    assert_eq!(decoder.decode(&mut buffer).unwrap(), None);
}

#[test]
fn test_varint_framing_frame_too_large() {
    let mut encoder = VarintLengthDelimitedEncoder::new(1000);
    let large_data = vec![b'x'; 1001]; // Exceeds max frame length

    let mut frame_buffer = BytesMut::from(&large_data[..]);
    assert!(encoder.encode((), &mut frame_buffer).is_err());
}

#[test]
fn test_varint_framing_empty_frame() {
    let mut encoder = VarintLengthDelimitedEncoder::default();
    let mut decoder = VarintLengthDelimitedDecoder::default();
    let mut buffer = BytesMut::new();

    // Encode empty frame
    encoder.encode((), &mut buffer).unwrap();
    assert_eq!(buffer.len(), 0); // Empty frames are not encoded

    // Try to decode empty buffer
    assert_eq!(decoder.decode(&mut buffer).unwrap(), None);
}

#[test]
fn test_varint_framing_config() {
    let config = VarintLengthDelimitedEncoderConfig {
        max_frame_length: 1000,
    };
    let _encoder = config.build();

    let decoder_config = VarintLengthDelimitedDecoderConfig {
        max_frame_length: 1000,
    };
    let _decoder = decoder_config.build();
}
