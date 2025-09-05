//! A collection of formats that can be used to chunk events into multiple byte frames.

use std::vec;

use bytes::BufMut;
use tracing::trace;

const GELF_MAX_TOTAL_CHUNKS: usize = 128;
const GELF_CHUNK_HEADERS_LENGTH: usize = 12;
const GELF_MAGIC_BYTES: [u8; 2] = [0x1e, 0x0f];

/// For chunking.
pub trait Chunking {
    /// Chunks the input into frames.
    fn chunk(&self, bytes: bytes::Bytes) -> Result<Vec<bytes::Bytes>, vector_common::Error>;
}

/// Chunks with GELF native chunking format.
/// Supports up to 128 chunks, each up to 8192 bytes (minus 12 bytes, for headers).
#[derive(Clone, Debug)]
pub struct GelfChunker {
    /// Max chunk size.
    pub max_chunk_size: usize,
}

impl Chunking for GelfChunker {
    fn chunk(&self, bytes: bytes::Bytes) -> Result<Vec<bytes::Bytes>, vector_common::Error> {
        if bytes.len() > self.max_chunk_size {
            let chunk_size = self.max_chunk_size - GELF_CHUNK_HEADERS_LENGTH;
            let message_id: u64 = rand::random();
            let chunk_count = (bytes.len() + chunk_size - 1) / chunk_size;

            trace!(
                message_id = message_id,
                chunk_count = chunk_count,
                chunk_size = chunk_size,
                "Generating chunks for GELF."
            );

            if chunk_count > GELF_MAX_TOTAL_CHUNKS {
                return Err(vector_common::Error::from(format!(
                    "Too many chunks to generate for GELF: {}, max: {}",
                    chunk_count, GELF_MAX_TOTAL_CHUNKS
                )));
            }

            // Split into chunks and add headers to each slice.
            // Map with index to determine sequence number.
            let chunks = bytes
                .chunks(chunk_size)
                .enumerate()
                .map(|(i, chunk)| {
                    let framed = bytes::Bytes::copy_from_slice(chunk);
                    let sequence_number = i as u8;
                    let sequence_count = chunk_count as u8;

                    let mut headers = bytes::BytesMut::with_capacity(GELF_CHUNK_HEADERS_LENGTH);
                    headers.put_slice(&GELF_MAGIC_BYTES);
                    headers.put_u64(message_id);
                    headers.put_u8(sequence_number);
                    headers.put_u8(sequence_count);

                    [headers.freeze(), framed].concat().into()
                })
                .collect();
            Ok(chunks)
        } else {
            Ok(vec![bytes])
        }
    }
}

/// Chunking implementations.
#[derive(Clone, Debug)]
pub enum Chunker {
    /// Chunking in GELF format.
    Gelf(GelfChunker),
}

impl Chunking for Chunker {
    fn chunk(&self, bytes: bytes::Bytes) -> Result<Vec<bytes::Bytes>, vector_common::Error> {
        match self {
            Chunker::Gelf(chunker) => chunker.chunk(bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::encoding::{
        Chunking, GelfChunker,
        chunking::{GELF_CHUNK_HEADERS_LENGTH, GELF_MAGIC_BYTES},
    };

    #[test]
    fn test_gelf_chunker_noop() {
        let chunker = GelfChunker {
            max_chunk_size: 8192,
        };
        let input = bytes::Bytes::from("1234123412341234123");
        let chunks = chunker.chunk(input.clone()).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], input);
    }

    #[test]
    fn test_gelf_chunker_chunk() {
        let chunker = GelfChunker {
            max_chunk_size: GELF_CHUNK_HEADERS_LENGTH + 4,
        };
        let input = bytes::Bytes::from("1234123412341234123");
        let chunks = chunker.chunk(input).unwrap();
        assert_eq!(chunks.len(), 5);

        for i in 0..chunks.len() {
            if i < 4 {
                assert_eq!(chunks[i].len(), GELF_CHUNK_HEADERS_LENGTH + 4);
            } else {
                assert_eq!(chunks[i].len(), GELF_CHUNK_HEADERS_LENGTH + 3);
            }
            // Bytes 0 and 1: Magic bytes
            assert_eq!(chunks[i][0..2], GELF_MAGIC_BYTES);
            // Bytes 2 to 9: Random ID (not checked)
            // Byte 10: Sequence number
            assert_eq!(chunks[i][10], i as u8);
            // Byte 11: Sequence count
            assert_eq!(chunks[i][11], chunks.len() as u8);
            // Payload bytes
            if i < 4 {
                assert_eq!(&chunks[i][GELF_CHUNK_HEADERS_LENGTH..], b"1234");
            } else {
                assert_eq!(&chunks[i][GELF_CHUNK_HEADERS_LENGTH..], b"123");
            }
        }
    }
}
