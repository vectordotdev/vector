//! A collection of formats that can be used to chunk events into multiple byte frames.

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

/// Does not chunk.
#[derive(Default)]
pub struct NoopChunker;

impl Chunker for NoopChunker {
    fn chunk(&self, bytes: bytes::Bytes) -> Result<Vec<bytes::Bytes>, vector_common::Error> {
        // No-op chunking implementation
        Ok(vec![bytes])
    }
}

/// Chunks with GELF native chunking format.
/// Supports up to 128 chunks, each up to 8192 bytes (minus 12 bytes, for headers).
pub struct GelfChunker {
    mtu: usize,
}

impl Default for GelfChunker {
    fn default() -> Self {
        Self {
            mtu: GELF_MAX_CHUNK_SIZE,
        }
    }
}

impl Chunker for GelfChunker {
    fn chunk(&self, bytes: bytes::Bytes) -> Result<Vec<bytes::Bytes>, vector_common::Error> {
        if bytes.len() > self.mtu {
            let chunk_size = self.mtu - GELF_CHUNK_HEADERS_LENGTH;
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
