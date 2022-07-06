use std::io;

use bytes::BytesMut;
use tokio_util::codec::Decoder;

#[derive(Debug)]
pub struct SizedBytesCodec {
    max_length: usize,
}

impl SizedBytesCodec {
    pub const fn new_with_max_length(max_length: usize) -> Self {
        SizedBytesCodec { max_length }
    }
}

// TODO: If needed it might be useful to return a list of BytesMut for
//       a single call but returning a single seems to work fine.
impl Decoder for SizedBytesCodec {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<BytesMut>, io::Error> {
        if !buf.is_empty() {
            let incoming_length = buf.len();
            if incoming_length >= self.max_length {
                // Buffer full
                Ok(Some(buf.split_to(self.max_length)))
            } else {
                // Buffer not full yet
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<BytesMut>, io::Error> {
        Ok(match self.decode(buf)? {
            Some(frame) => Some(frame),
            None => {
                if !buf.is_empty() {
                    // Send what ever is left over
                    Some(buf.split_to(buf.len()))
                } else {
                    // Nothing left to send
                    None
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_buffer_less() {
        let mut codec = SizedBytesCodec::new_with_max_length(10);
        let mut bytes = BytesMut::from("12345678");
        let result = codec.decode(&mut bytes).unwrap();

        assert_eq!(None, result);
    }

    #[test]
    fn test_decode_buffer_exact() {
        let mut codec = SizedBytesCodec::new_with_max_length(10);
        let mut bytes = BytesMut::from("1234567890");

        if let Some(bytes_response) = codec.decode(&mut bytes).unwrap() {
            assert_eq!(BytesMut::from("1234567890"), bytes_response)
        } else {
            panic!("Should have returned some bytes")
        }
    }

    #[test]
    fn test_decode_buffer_over() {
        let mut codec = SizedBytesCodec::new_with_max_length(10);
        let mut bytes = BytesMut::from("1234561234567890");

        if let Some(bytes_response) = codec.decode(&mut bytes).unwrap() {
            assert_eq!(BytesMut::from("1234561234"), bytes_response)
        } else {
            panic!("Should have returned some bytes")
        }
    }

    #[test]
    fn test_decode_eof_remainder() {
        let mut codec = SizedBytesCodec::new_with_max_length(10);
        let mut bytes = BytesMut::from("123456");

        if let Some(bytes_response) = codec.decode_eof(&mut bytes).unwrap() {
            assert_eq!(BytesMut::from("123456"), bytes_response)
        } else {
            panic!("Should have returned some bytes")
        }
    }
}
