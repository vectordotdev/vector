//! A collection of formats that can be used to chunk events into multiple byte frames.

use std::vec;

use bytes::BufMut;
use tracing::trace;

const GELF_MAX_CHUNK_SIZE: usize = 8192;
const GELF_MAX_TOTAL_CHUNKS: usize = 128;
const GELF_CHUNK_HEADERS_LENGTH: usize = 12;
const GELF_MAGIC_BYTES: [u8; 2] = [0x1e, 0x0f];

/// For chunking.
pub trait Chunker {
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

impl Default for GelfChunker {
    fn default() -> Self {
        Self {
            max_chunk_size: GELF_MAX_CHUNK_SIZE,
        }
    }
}

impl Chunker for GelfChunker {
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

/// Chunkers.
#[derive(Clone, Debug, Default)]
pub enum Chunkers {
    /// No chunking (pass-through).
    #[default]
    Noop,
    /// Chunking in GELF format.
    Gelf(GelfChunker),
}

impl Chunker for Chunkers {
    fn chunk(&self, bytes: bytes::Bytes) -> Result<Vec<bytes::Bytes>, vector_common::Error> {
        match self {
            Chunkers::Noop => Ok(vec![bytes]),
            Chunkers::Gelf(chunker) => chunker.chunk(bytes),
        }
    }
}
