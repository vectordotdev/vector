//! A collection of formats that can be used to chunk events into multiple byte frames.

use bytes::BufMut;

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

/// Chunks with GELF native chunking format. Supports up to 128 chunks, each up to 8192 bytes (minus 12 for headers).
pub struct GelfChunker;

impl Default for GelfChunker {
    fn default() -> Self {
        Self {}
    }
}

impl Chunker for GelfChunker {
    fn chunk(&self, bytes: bytes::Bytes) -> Result<Vec<bytes::Bytes>, vector_common::Error> {
        // GELF chunking implementation
        let chunk_size = 8192 - 12;
        if bytes.len() > chunk_size {
            let message_id: u64 = rand::random();

            // Split into chunks of (8192-12) and add chunking headers to each slice.
            // Map with index to determine sequence number.
            let chunk_count = (bytes.len() + chunk_size - 1) / chunk_size;

            if chunk_count > 128 {
                panic!("too many chunks!"); // todo: don't panic.
            }

            let chunks = bytes
                .chunks(chunk_size)
                .enumerate()
                .map(|(i, chunk)| {
                    let framed = bytes::Bytes::copy_from_slice(chunk);
                    let magic_bytes = [0x1e, 0x0f];
                    let sequence_number = i as u8;
                    let sequence_count = chunk_count as u8;

                    let mut headers = bytes::BytesMut::with_capacity(12);
                    headers.put_slice(&magic_bytes);
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
